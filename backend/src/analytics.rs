#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use rust_decimal::{prelude::ToPrimitive, Decimal};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use thiserror::Error;

use crate::db::Database;

const DEFAULT_INTERVAL: &str = "1m";
const DEFAULT_POINT_LIMIT: i64 = 240;
const MAX_POINT_LIMIT: i64 = 5_000;
const MIN_SERIES_COUNT: usize = 2;
const MIN_RETURN_COUNT: usize = 2;
const TRADING_PERIODS_PER_YEAR: f64 = 252.0;
const VALUE_AT_RISK_Z_95: f64 = 1.644_853_626_951_472_2;

#[derive(Clone, Debug)]
pub struct CrossAssetAnalyticsService {
    database: Database,
}

impl CrossAssetAnalyticsService {
    pub fn new(database: Database) -> Self {
        Self { database }
    }

    pub async fn compute_for_symbols(
        &self,
        symbols: Vec<String>,
        interval: Option<&str>,
        limit: Option<i64>,
    ) -> Result<CrossAssetAnalytics, AnalyticsError> {
        let request = AnalyticsRequest::new(symbols, interval, limit)?;
        let mut series = Vec::with_capacity(request.symbols.len());

        for symbol in &request.symbols {
            series.push(fetch_price_series(
                &self.database,
                symbol.as_str(),
                request.interval.as_str(),
                request.limit,
            )
            .await?);
        }

        compute_cross_asset_analytics(series)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AnalyticsPricePoint {
    pub observed_at: DateTime<Utc>,
    pub close_price: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AnalyticsInputSeries {
    pub symbol: String,
    pub points: Vec<AnalyticsPricePoint>,
}

impl AnalyticsInputSeries {
    pub fn new(
        symbol: impl Into<String>,
        points: Vec<AnalyticsPricePoint>,
    ) -> Result<Self, AnalyticsError> {
        let symbol = normalize_symbol(symbol.into())?;
        Ok(Self { symbol, points })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CrossAssetAnalytics {
    pub symbols: Vec<String>,
    pub observations: usize,
    pub correlation_matrix: Vec<CorrelationRow>,
    pub relative_performance: Vec<RelativePerformance>,
    pub portfolio_risk: PortfolioRiskSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CorrelationRow {
    pub symbol: String,
    pub correlations: Vec<CorrelationCell>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CorrelationCell {
    pub symbol: String,
    pub correlation: Option<f64>,
    pub observations: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RelativePerformance {
    pub symbol: String,
    pub start_price: f64,
    pub end_price: f64,
    pub total_return: f64,
    pub average_return: f64,
    pub volatility: f64,
    pub observations: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PortfolioRiskSummary {
    pub method: &'static str,
    pub average_return: f64,
    pub volatility: f64,
    pub annualized_volatility: f64,
    pub value_at_risk_95: f64,
    pub observations: usize,
}

#[derive(Debug, PartialEq, Eq)]
struct AnalyticsRequest {
    symbols: Vec<String>,
    interval: String,
    limit: i64,
}

impl AnalyticsRequest {
    fn new(
        symbols: Vec<String>,
        interval: Option<&str>,
        limit: Option<i64>,
    ) -> Result<Self, AnalyticsError> {
        let symbols = normalize_symbols(symbols)?;
        if symbols.len() < MIN_SERIES_COUNT {
            return Err(AnalyticsError::NotEnoughSymbols);
        }

        Ok(Self {
            symbols,
            interval: normalize_interval(interval.unwrap_or(DEFAULT_INTERVAL))?,
            limit: limit.unwrap_or(DEFAULT_POINT_LIMIT).clamp(2, MAX_POINT_LIMIT),
        })
    }
}

#[derive(Debug, FromRow)]
struct PricePointRow {
    observed_at: DateTime<Utc>,
    close_price: Decimal,
}

pub fn compute_cross_asset_analytics(
    series: Vec<AnalyticsInputSeries>,
) -> Result<CrossAssetAnalytics, AnalyticsError> {
    if series.len() < MIN_SERIES_COUNT {
        return Err(AnalyticsError::NotEnoughSymbols);
    }

    let mut normalized = Vec::with_capacity(series.len());
    for input in series {
        normalized.push(NormalizedSeries::try_from(input)?);
    }

    let symbols = normalized
        .iter()
        .map(|series| series.symbol.clone())
        .collect::<Vec<_>>();
    let returns_by_symbol = normalized
        .iter()
        .map(|series| (series.symbol.clone(), series.returns_by_time()))
        .collect::<BTreeMap<_, _>>();
    let common_times = common_return_times(&returns_by_symbol);
    if common_times.len() < MIN_RETURN_COUNT {
        return Err(AnalyticsError::InsufficientOverlap);
    }

    let aligned_returns = symbols
        .iter()
        .map(|symbol| {
            let series_returns = returns_by_symbol
                .get(symbol)
                .ok_or(AnalyticsError::MissingAlignedReturns)?;
            common_times
                .iter()
                .map(|observed_at| {
                    series_returns
                        .get(observed_at)
                        .copied()
                        .ok_or(AnalyticsError::MissingAlignedReturns)
                })
                .collect::<Result<Vec<_>, _>>()
                .map(|returns| (symbol.clone(), returns))
        })
        .collect::<Result<BTreeMap<_, _>, _>>()?;

    let correlation_matrix = build_correlation_matrix(&symbols, &aligned_returns);
    let relative_performance = normalized
        .iter()
        .map(|series| {
            let returns = aligned_returns
                .get(series.symbol.as_str())
                .ok_or(AnalyticsError::MissingAlignedReturns)?;
            Ok(relative_performance(series, returns))
        })
        .collect::<Result<Vec<_>, AnalyticsError>>()?;
    let portfolio_risk = portfolio_risk(&symbols, &aligned_returns, common_times.len())?;

    Ok(CrossAssetAnalytics {
        symbols,
        observations: common_times.len(),
        correlation_matrix,
        relative_performance,
        portfolio_risk,
    })
}

async fn fetch_price_series(
    database: &Database,
    symbol: &str,
    interval: &str,
    limit: i64,
) -> Result<AnalyticsInputSeries, AnalyticsError> {
    let rows = sqlx::query_as::<_, PricePointRow>(
        r#"
        SELECT observed_at, close_price
        FROM (
            SELECT p.observed_at, p.close_price
            FROM price_series_cache s
            INNER JOIN price_series_points p ON p.series_id = s.id
            WHERE lower(s.symbol) = lower($1)
                AND s.interval = $2
            ORDER BY p.observed_at DESC
            LIMIT $3
        ) recent_points
        ORDER BY observed_at ASC
        "#,
    )
    .bind(symbol)
    .bind(interval)
    .bind(limit)
    .fetch_all(database.pool())
    .await?;

    let points = rows
        .into_iter()
        .map(|row| {
            let close_price = row
                .close_price
                .to_f64()
                .ok_or(AnalyticsError::InvalidPrice)?;
            Ok(AnalyticsPricePoint {
                observed_at: row.observed_at,
                close_price,
            })
        })
        .collect::<Result<Vec<_>, AnalyticsError>>()?;

    AnalyticsInputSeries::new(symbol, points)
}

#[derive(Debug, Clone)]
struct NormalizedSeries {
    symbol: String,
    points: Vec<AnalyticsPricePoint>,
}

impl TryFrom<AnalyticsInputSeries> for NormalizedSeries {
    type Error = AnalyticsError;

    fn try_from(input: AnalyticsInputSeries) -> Result<Self, Self::Error> {
        let symbol = normalize_symbol(input.symbol)?;
        let mut points = input
            .points
            .into_iter()
            .filter(|point| point.close_price.is_finite() && point.close_price > 0.0)
            .collect::<Vec<_>>();
        points.sort_by_key(|point| point.observed_at);
        points.dedup_by_key(|point| point.observed_at);

        if points.len() <= MIN_RETURN_COUNT {
            return Err(AnalyticsError::InsufficientSeriesData { symbol });
        }

        Ok(Self { symbol, points })
    }
}

impl NormalizedSeries {
    fn returns_by_time(&self) -> BTreeMap<DateTime<Utc>, f64> {
        self.points
            .windows(2)
            .filter_map(|window| {
                let previous = window.first()?;
                let current = window.get(1)?;
                let simple_return = (current.close_price / previous.close_price) - 1.0;
                simple_return
                    .is_finite()
                    .then_some((current.observed_at, simple_return))
            })
            .collect()
    }

    fn start_price(&self) -> f64 {
        self.points
            .first()
            .map(|point| point.close_price)
            .unwrap_or_default()
    }

    fn end_price(&self) -> f64 {
        self.points
            .last()
            .map(|point| point.close_price)
            .unwrap_or_default()
    }
}

fn build_correlation_matrix(
    symbols: &[String],
    aligned_returns: &BTreeMap<String, Vec<f64>>,
) -> Vec<CorrelationRow> {
    symbols
        .iter()
        .map(|row_symbol| CorrelationRow {
            symbol: row_symbol.clone(),
            correlations: symbols
                .iter()
                .map(|column_symbol| {
                    let left = aligned_returns.get(row_symbol).map(Vec::as_slice);
                    let right = aligned_returns.get(column_symbol).map(Vec::as_slice);
                    let correlation = match (left, right) {
                        (Some(left), Some(right)) => pearson_correlation(left, right),
                        _ => None,
                    };
                    CorrelationCell {
                        symbol: column_symbol.clone(),
                        correlation,
                        observations: left.map_or(0, <[f64]>::len),
                    }
                })
                .collect(),
        })
        .collect()
}

fn relative_performance(series: &NormalizedSeries, returns: &[f64]) -> RelativePerformance {
    let start_price = series.start_price();
    let end_price = series.end_price();
    let total_return = if start_price > 0.0 {
        (end_price / start_price) - 1.0
    } else {
        0.0
    };

    RelativePerformance {
        symbol: series.symbol.clone(),
        start_price,
        end_price,
        total_return,
        average_return: mean(returns),
        volatility: sample_std_dev(returns),
        observations: returns.len(),
    }
}

fn portfolio_risk(
    symbols: &[String],
    aligned_returns: &BTreeMap<String, Vec<f64>>,
    observations: usize,
) -> Result<PortfolioRiskSummary, AnalyticsError> {
    if symbols.is_empty() || observations < MIN_RETURN_COUNT {
        return Err(AnalyticsError::InsufficientOverlap);
    }

    let weight = 1.0 / symbols.len() as f64;
    let mut portfolio_returns = Vec::with_capacity(observations);
    for index in 0..observations {
        let mut portfolio_return = 0.0;
        for symbol in symbols {
            let returns = aligned_returns
                .get(symbol)
                .ok_or(AnalyticsError::MissingAlignedReturns)?;
            let value = returns
                .get(index)
                .copied()
                .ok_or(AnalyticsError::MissingAlignedReturns)?;
            portfolio_return += value * weight;
        }
        portfolio_returns.push(portfolio_return);
    }

    let average_return = mean(&portfolio_returns);
    let volatility = sample_std_dev(&portfolio_returns);
    let annualized_volatility = volatility * TRADING_PERIODS_PER_YEAR.sqrt();

    Ok(PortfolioRiskSummary {
        method: "equal_weight",
        average_return,
        volatility,
        annualized_volatility,
        value_at_risk_95: (VALUE_AT_RISK_Z_95 * volatility) - average_return,
        observations,
    })
}

fn common_return_times(
    returns_by_symbol: &BTreeMap<String, BTreeMap<DateTime<Utc>, f64>>,
) -> Vec<DateTime<Utc>> {
    let mut iterator = returns_by_symbol.values();
    let Some(first) = iterator.next() else {
        return Vec::new();
    };
    let mut common = first.keys().copied().collect::<BTreeSet<_>>();

    for returns in iterator {
        let times = returns.keys().copied().collect::<BTreeSet<_>>();
        common = common.intersection(&times).copied().collect();
    }

    common.into_iter().collect()
}

fn pearson_correlation(left: &[f64], right: &[f64]) -> Option<f64> {
    if left.len() != right.len() || left.len() < MIN_RETURN_COUNT {
        return None;
    }

    let left_mean = mean(left);
    let right_mean = mean(right);
    let mut numerator = 0.0;
    let mut left_sum = 0.0;
    let mut right_sum = 0.0;

    for (left_value, right_value) in left.iter().zip(right.iter()) {
        let left_delta = left_value - left_mean;
        let right_delta = right_value - right_mean;
        numerator += left_delta * right_delta;
        left_sum += left_delta.powi(2);
        right_sum += right_delta.powi(2);
    }

    let denominator = left_sum.sqrt() * right_sum.sqrt();
    if denominator <= f64::EPSILON {
        None
    } else {
        Some((numerator / denominator).clamp(-1.0, 1.0))
    }
}

fn sample_std_dev(values: &[f64]) -> f64 {
    if values.len() < MIN_RETURN_COUNT {
        return 0.0;
    }
    let average = mean(values);
    let variance = values
        .iter()
        .map(|value| (value - average).powi(2))
        .sum::<f64>()
        / (values.len() - 1) as f64;
    variance.sqrt()
}

fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        0.0
    } else {
        values.iter().sum::<f64>() / values.len() as f64
    }
}

fn normalize_symbols(symbols: Vec<String>) -> Result<Vec<String>, AnalyticsError> {
    let mut seen = BTreeSet::new();
    let mut normalized = Vec::new();

    for symbol in symbols {
        let symbol = normalize_symbol(symbol)?;
        if seen.insert(symbol.clone()) {
            normalized.push(symbol);
        }
    }

    Ok(normalized)
}

fn normalize_symbol(symbol: impl Into<String>) -> Result<String, AnalyticsError> {
    let symbol = symbol.into().trim().to_ascii_uppercase();
    if symbol.is_empty() {
        Err(AnalyticsError::EmptySymbol)
    } else {
        Ok(symbol)
    }
}

fn normalize_interval(interval: &str) -> Result<String, AnalyticsError> {
    match interval.trim() {
        "1m" | "5m" | "15m" | "30m" | "1h" | "4h" | "1d" | "1w" | "1mo" => {
            Ok(interval.trim().to_owned())
        }
        _ => Err(AnalyticsError::InvalidInterval),
    }
}

#[derive(Debug, Error)]
pub enum AnalyticsError {
    #[error("symbol cannot be empty")]
    EmptySymbol,
    #[error("at least two symbols are required")]
    NotEnoughSymbols,
    #[error("invalid analytics interval")]
    InvalidInterval,
    #[error("series {symbol} does not contain enough price data")]
    InsufficientSeriesData { symbol: String },
    #[error("selected instruments do not have enough overlapping returns")]
    InsufficientOverlap,
    #[error("price value cannot be represented as f64")]
    InvalidPrice,
    #[error("aligned returns are incomplete")]
    MissingAlignedReturns,
    #[error("analytics query failed: {0}")]
    Database(#[from] sqlx::Error),
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::{
        compute_cross_asset_analytics, normalize_interval, normalize_symbols,
        AnalyticsInputSeries, AnalyticsPricePoint,
    };

    #[test]
    fn computes_correlation_and_relative_performance() {
        let analytics = compute_cross_asset_analytics(vec![
            series("AAA", &[100.0, 110.0, 121.0, 140.0]),
            series("BBB", &[50.0, 55.0, 60.5, 70.0]),
            series("CCC", &[200.0, 180.0, 162.0, 136.562]),
        ])
        .expect("analytics should compute");

        assert_eq!(analytics.symbols, vec!["AAA", "BBB", "CCC"]);
        assert_eq!(analytics.observations, 3);
        assert_eq!(analytics.relative_performance.len(), 3);
        assert!(analytics.relative_performance[0].total_return > 0.33);
        assert!(analytics.correlation_matrix[0].correlations[1]
            .correlation
            .expect("same-direction correlation")
            > 0.99);
        assert!(analytics.correlation_matrix[0].correlations[2]
            .correlation
            .expect("opposing correlation")
            < -0.99);
    }

    #[test]
    fn computes_equal_weight_portfolio_risk() {
        let analytics = compute_cross_asset_analytics(vec![
            series("AAA", &[100.0, 101.0, 100.0, 104.0]),
            series("BBB", &[100.0, 99.0, 101.0, 102.0]),
        ])
        .expect("analytics should compute");

        assert_eq!(analytics.portfolio_risk.method, "equal_weight");
        assert_eq!(analytics.portfolio_risk.observations, 3);
        assert!(analytics.portfolio_risk.volatility >= 0.0);
    }

    #[test]
    fn normalizes_request_inputs() {
        assert_eq!(
            normalize_symbols(vec![" spy ".to_owned(), "SPY".to_owned(), "qqq".to_owned()])
                .expect("symbols should normalize"),
            vec!["SPY", "QQQ"]
        );
        assert!(normalize_interval("1m").is_ok());
        assert!(normalize_interval("tick").is_err());
    }

    fn series(symbol: &str, prices: &[f64]) -> AnalyticsInputSeries {
        let points = prices
            .iter()
            .enumerate()
            .map(|(index, close_price)| AnalyticsPricePoint {
                observed_at: Utc
                    .with_ymd_and_hms(2026, 7, 21, 14, index as u32, 0)
                    .single()
                    .expect("valid timestamp"),
                close_price: *close_price,
            })
            .collect();

        AnalyticsInputSeries::new(symbol, points).expect("series should be valid")
    }
}
