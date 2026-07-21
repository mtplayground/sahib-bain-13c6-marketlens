#![allow(dead_code)]

use axum::{
    extract::{Query, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use chrono::{DateTime, Utc};
use rust_decimal::{prelude::ToPrimitive, Decimal};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use thiserror::Error;

use crate::{
    analytics::{AnalyticsError, CrossAssetAnalyticsService},
    db::Database,
    state::AppState,
};

const DEFAULT_INTERVAL: &str = "1m";
const DEFAULT_POINT_LIMIT: i64 = 180;
const MAX_POINT_LIMIT: i64 = 1_000;
const DEFAULT_NEWS_LIMIT: i64 = 5;
const MAX_COMPARISON_SYMBOLS: usize = 6;
const MIN_PRICE_POINTS: usize = 8;

pub fn router() -> Router<AppState> {
    Router::new().route("/estimator", get(estimator))
}

#[derive(Debug, Deserialize)]
struct EstimatorParams {
    instrument_id: Option<i64>,
    symbol: Option<String>,
    comparison_symbols: Option<String>,
    interval: Option<String>,
    limit: Option<i64>,
}

impl EstimatorParams {
    fn validate(&self) -> Result<ValidatedEstimatorQuery, EstimatorError> {
        if self.instrument_id.is_some_and(|id| id <= 0) {
            return Err(EstimatorError::InvalidInstrumentId);
        }
        let symbol = normalize_optional(self.symbol.clone().unwrap_or_default())
            .map(|symbol| symbol.to_ascii_uppercase());
        if self.instrument_id.is_none() && symbol.is_none() {
            return Err(EstimatorError::MissingInstrumentSelector);
        }

        Ok(ValidatedEstimatorQuery {
            instrument_id: self.instrument_id,
            symbol,
            comparison_symbols: parse_comparison_symbols(self.comparison_symbols.as_deref())?,
            interval: normalize_interval(self.interval.as_deref().unwrap_or(DEFAULT_INTERVAL))?,
            limit: self
                .limit
                .unwrap_or(DEFAULT_POINT_LIMIT)
                .clamp(MIN_PRICE_POINTS as i64, MAX_POINT_LIMIT),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ValidatedEstimatorQuery {
    instrument_id: Option<i64>,
    symbol: Option<String>,
    comparison_symbols: Vec<String>,
    interval: String,
    limit: i64,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
struct AppliedEstimatorQuery {
    instrument_id: Option<i64>,
    symbol: Option<String>,
    comparison_symbols: Vec<String>,
    interval: String,
    limit: i64,
}

impl From<&ValidatedEstimatorQuery> for AppliedEstimatorQuery {
    fn from(query: &ValidatedEstimatorQuery) -> Self {
        Self {
            instrument_id: query.instrument_id,
            symbol: query.symbol.clone(),
            comparison_symbols: query.comparison_symbols.clone(),
            interval: query.interval.clone(),
            limit: query.limit,
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct EstimatorResponse {
    query: AppliedEstimatorQuery,
    model: EstimatorModelInfo,
    instrument: EstimatorInstrument,
    direction: EstimatorDirection,
    certainty_percentage: f64,
    composite_score: f64,
    reasons: Vec<EstimatorReason>,
    evidence: EstimatorEvidenceSet,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct EstimatorModelInfo {
    name: &'static str,
    version: &'static str,
    disclaimer: &'static str,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EstimatorDirection {
    Bullish,
    Bearish,
    Neutral,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct EstimatorInstrument {
    id: i64,
    canonical_symbol: String,
    display_name: String,
    asset_class: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct EstimatorReason {
    rank: usize,
    category: &'static str,
    label: String,
    contribution: f64,
    weight: f64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct EstimatorEvidenceSet {
    market_trends: Vec<MarketTrendEvidence>,
    news_articles: Vec<NewsEvidence>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct MarketTrendEvidence {
    name: &'static str,
    value: f64,
    unit: &'static str,
    observed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct NewsEvidence {
    id: i64,
    title: String,
    source_name: String,
    source_url: String,
    published_at: DateTime<Utc>,
    sentiment_score: f64,
}

#[derive(Debug, FromRow)]
struct EstimatorInstrumentRow {
    id: i64,
    canonical_symbol: String,
    display_name: String,
    asset_class: String,
}

#[derive(Debug, FromRow)]
struct PricePointRow {
    observed_at: DateTime<Utc>,
    close_price: Decimal,
    volume: Option<Decimal>,
}

#[derive(Debug, FromRow)]
struct LatestRatiosRow {
    pe_ratio: Option<Decimal>,
    return_on_equity: Option<Decimal>,
    debt_to_equity: Option<Decimal>,
}

#[derive(Debug, FromRow)]
struct LatestFinancialRow {
    revenue: Option<Decimal>,
    net_income: Option<Decimal>,
    free_cash_flow: Option<Decimal>,
}

#[derive(Debug, FromRow)]
struct NewsArticleRow {
    id: i64,
    title: String,
    summary: Option<String>,
    body_excerpt: Option<String>,
    source_name: String,
    source_url: String,
    published_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
struct PriceSignal {
    total_return: f64,
    short_momentum: f64,
    volatility: f64,
    latest_price: f64,
    latest_volume: Option<f64>,
    latest_observed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
struct FundamentalSignal {
    profitability_score: f64,
    valuation_score: f64,
    leverage_score: f64,
}

async fn estimator(
    State(state): State<AppState>,
    Query(params): Query<EstimatorParams>,
) -> Result<Json<EstimatorResponse>, (StatusCode, Json<EstimatorErrorResponse>)> {
    let query = params.validate().map_err(estimator_error_response)?;
    let response = EstimatorService::new(state.database().clone())
        .estimate(query)
        .await
        .map_err(estimator_error_response)?;

    Ok(Json(response))
}

#[derive(Clone, Debug)]
pub struct EstimatorService {
    database: Database,
}

impl EstimatorService {
    pub fn new(database: Database) -> Self {
        Self { database }
    }

    async fn estimate(
        &self,
        query: ValidatedEstimatorQuery,
    ) -> Result<EstimatorResponse, EstimatorError> {
        let instrument = fetch_instrument(&self.database, &query)
            .await?
            .ok_or(EstimatorError::InstrumentNotFound)?;
        let price_points = fetch_price_points(
            &self.database,
            instrument.canonical_symbol.as_str(),
            query.interval.as_str(),
            query.limit,
        )
        .await?;
        let price_signal = price_signal(&price_points)?;
        let fundamental_signal = fetch_fundamental_signal(&self.database, instrument.id).await?;
        let news_articles =
            fetch_news_evidence(&self.database, instrument.id, DEFAULT_NEWS_LIMIT).await?;
        let cross_asset_signal = self.cross_asset_signal(&instrument, &query).await;
        let composite = composite_signal(
            &price_signal,
            fundamental_signal.as_ref(),
            cross_asset_signal,
            &news_articles,
        );

        Ok(EstimatorResponse {
            query: AppliedEstimatorQuery::from(&query),
            model: EstimatorModelInfo {
                name: "marketlens_in_house_composite",
                version: "v1",
                disclaimer: "Deterministic weighted signal composite; not a licensed proprietary prediction model.",
            },
            instrument: EstimatorInstrument::from(instrument),
            direction: direction_from_score(composite.score),
            certainty_percentage: composite.certainty_percentage,
            composite_score: composite.score,
            reasons: composite.reasons,
            evidence: EstimatorEvidenceSet {
                market_trends: market_trend_evidence(&price_signal, cross_asset_signal),
                news_articles,
            },
        })
    }

    async fn cross_asset_signal(
        &self,
        instrument: &EstimatorInstrumentRow,
        query: &ValidatedEstimatorQuery,
    ) -> Option<f64> {
        let symbols = with_primary_symbol(
            instrument.canonical_symbol.as_str(),
            query.comparison_symbols.clone(),
        );
        if symbols.len() < 2 {
            return None;
        }

        let analytics = CrossAssetAnalyticsService::new(self.database.clone())
            .compute_for_symbols(symbols, Some(query.interval.as_str()), Some(query.limit))
            .await
            .ok()?;
        let primary = analytics
            .relative_performance
            .iter()
            .find(|entry| entry.symbol == instrument.canonical_symbol)?;
        let peer_average = analytics
            .relative_performance
            .iter()
            .filter(|entry| entry.symbol != instrument.canonical_symbol)
            .map(|entry| entry.total_return)
            .collect::<Vec<_>>();
        if peer_average.is_empty() {
            return None;
        }

        Some((primary.total_return - mean(&peer_average)).clamp(-1.0, 1.0))
    }
}

#[derive(Debug)]
struct CompositeResult {
    score: f64,
    certainty_percentage: f64,
    reasons: Vec<EstimatorReason>,
}

fn composite_signal(
    price: &PriceSignal,
    fundamentals: Option<&FundamentalSignal>,
    cross_asset_signal: Option<f64>,
    news: &[NewsEvidence],
) -> CompositeResult {
    let technical_score =
        ((price.total_return * 4.0) + (price.short_momentum * 3.0) - (price.volatility * 0.7))
            .clamp(-1.0, 1.0);
    let fundamental_score = fundamentals
        .map(|signal| {
            ((signal.profitability_score * 0.45)
                + (signal.valuation_score * 0.25)
                + (signal.leverage_score * 0.30))
                .clamp(-1.0, 1.0)
        })
        .unwrap_or(0.0);
    let cross_asset_score = cross_asset_signal.unwrap_or(0.0);
    let news_score = if news.is_empty() {
        0.0
    } else {
        mean(&news.iter().map(|article| article.sentiment_score).collect::<Vec<_>>())
    };

    let weighted = vec![
        ("technical", "Price momentum and volatility", technical_score, 0.40),
        ("fundamental", "Cached fundamental quality", fundamental_score, 0.25),
        ("cross_asset", "Relative performance versus selected peers", cross_asset_score, 0.20),
        ("news", "Recent attributed news tone", news_score, 0.15),
    ];
    let score = weighted
        .iter()
        .map(|(_, _, signal, weight)| signal * weight)
        .sum::<f64>()
        .clamp(-1.0, 1.0);
    let evidence_quality = weighted
        .iter()
        .filter(|(_, _, signal, _)| signal.abs() > f64::EPSILON)
        .map(|(_, _, _, weight)| weight)
        .sum::<f64>()
        .clamp(0.0, 1.0);
    let certainty_percentage = (35.0 + (score.abs() * 45.0) + (evidence_quality * 20.0))
        .clamp(0.0, 100.0);
    let mut reasons = weighted
        .into_iter()
        .filter(|(_, _, signal, _)| signal.abs() > f64::EPSILON)
        .map(|(category, label, signal, weight)| EstimatorReason {
            rank: 0,
            category,
            label: label.to_owned(),
            contribution: round4(signal * weight),
            weight,
        })
        .collect::<Vec<_>>();
    reasons.sort_by(|left, right| {
        right
            .contribution
            .abs()
            .partial_cmp(&left.contribution.abs())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    for (index, reason) in reasons.iter_mut().enumerate() {
        reason.rank = index + 1;
    }

    CompositeResult {
        score: round4(score),
        certainty_percentage: round2(certainty_percentage),
        reasons,
    }
}

async fn fetch_instrument(
    database: &Database,
    query: &ValidatedEstimatorQuery,
) -> Result<Option<EstimatorInstrumentRow>, EstimatorError> {
    sqlx::query_as::<_, EstimatorInstrumentRow>(
        r#"
        SELECT id, canonical_symbol, display_name, asset_class
        FROM instruments
        WHERE ($1::BIGINT IS NULL OR id = $1)
            AND ($2::TEXT IS NULL OR lower(canonical_symbol) = lower($2))
        ORDER BY updated_at DESC
        LIMIT 1
        "#,
    )
    .bind(query.instrument_id)
    .bind(query.symbol.as_deref())
    .fetch_optional(database.pool())
    .await
    .map_err(EstimatorError::Database)
}

async fn fetch_price_points(
    database: &Database,
    symbol: &str,
    interval: &str,
    limit: i64,
) -> Result<Vec<PricePointRow>, EstimatorError> {
    sqlx::query_as::<_, PricePointRow>(
        r#"
        SELECT observed_at, close_price, volume
        FROM (
            SELECT p.observed_at, p.close_price, p.volume
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
    .await
    .map_err(EstimatorError::Database)
}

async fn fetch_fundamental_signal(
    database: &Database,
    instrument_id: i64,
) -> Result<Option<FundamentalSignal>, EstimatorError> {
    let ratios = sqlx::query_as::<_, LatestRatiosRow>(
        r#"
        SELECT pe_ratio, return_on_equity, debt_to_equity
        FROM key_ratios
        WHERE instrument_id = $1
        ORDER BY as_of_date DESC, fetched_at DESC
        LIMIT 1
        "#,
    )
    .bind(instrument_id)
    .fetch_optional(database.pool())
    .await?;
    let financial = sqlx::query_as::<_, LatestFinancialRow>(
        r#"
        SELECT revenue, net_income, free_cash_flow
        FROM company_financials
        WHERE instrument_id = $1
        ORDER BY fiscal_period_end DESC, fetched_at DESC
        LIMIT 1
        "#,
    )
    .bind(instrument_id)
    .fetch_optional(database.pool())
    .await?;

    if ratios.is_none() && financial.is_none() {
        return Ok(None);
    }

    let return_on_equity = ratios
        .as_ref()
        .and_then(|row| decimal_to_f64(row.return_on_equity));
    let debt_to_equity = ratios
        .as_ref()
        .and_then(|row| decimal_to_f64(row.debt_to_equity));
    let pe_ratio = ratios.as_ref().and_then(|row| decimal_to_f64(row.pe_ratio));
    let net_margin = match financial.as_ref() {
        Some(row) => match (decimal_to_f64(row.net_income), decimal_to_f64(row.revenue)) {
            (Some(net_income), Some(revenue)) if revenue.abs() > f64::EPSILON => {
                Some(net_income / revenue)
            }
            _ => None,
        },
        None => None,
    };
    let cash_flow_positive = financial
        .as_ref()
        .and_then(|row| decimal_to_f64(row.free_cash_flow))
        .map(|value| if value >= 0.0 { 0.2 } else { -0.2 })
        .unwrap_or(0.0);

    Ok(Some(FundamentalSignal {
        profitability_score: (return_on_equity.unwrap_or(0.0) * 2.5
            + net_margin.unwrap_or(0.0) * 2.0
            + cash_flow_positive)
            .clamp(-1.0, 1.0),
        valuation_score: valuation_score(pe_ratio),
        leverage_score: debt_to_equity
            .map(|value| (0.6 - value / 3.0).clamp(-1.0, 1.0))
            .unwrap_or(0.0),
    }))
}

async fn fetch_news_evidence(
    database: &Database,
    instrument_id: i64,
    limit: i64,
) -> Result<Vec<NewsEvidence>, EstimatorError> {
    let rows = sqlx::query_as::<_, NewsArticleRow>(
        r#"
        SELECT a.id, a.title, a.summary, a.body_excerpt, a.source_name, a.source_url, a.published_at
        FROM news_articles a
        INNER JOIN news_article_instruments ai ON ai.article_id = a.id
        WHERE ai.instrument_id = $1
        ORDER BY a.published_at DESC, a.id DESC
        LIMIT $2
        "#,
    )
    .bind(instrument_id)
    .bind(limit)
    .fetch_all(database.pool())
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| {
            let text = format!(
                "{} {} {}",
                row.title,
                row.summary.as_deref().unwrap_or_default(),
                row.body_excerpt.as_deref().unwrap_or_default()
            );
            NewsEvidence {
                id: row.id,
                title: row.title,
                source_name: row.source_name,
                source_url: row.source_url,
                published_at: row.published_at,
                sentiment_score: round4(heuristic_news_sentiment(text.as_str())),
            }
        })
        .collect())
}

fn price_signal(points: &[PricePointRow]) -> Result<PriceSignal, EstimatorError> {
    if points.len() < MIN_PRICE_POINTS {
        return Err(EstimatorError::InsufficientPriceData);
    }
    let closes = points
        .iter()
        .map(|point| decimal_to_f64(Some(point.close_price)).ok_or(EstimatorError::InvalidDecimal))
        .collect::<Result<Vec<_>, _>>()?;
    let latest_price = closes
        .last()
        .copied()
        .ok_or(EstimatorError::InsufficientPriceData)?;
    let first_price = closes
        .first()
        .copied()
        .ok_or(EstimatorError::InsufficientPriceData)?;
    if first_price <= 0.0 || latest_price <= 0.0 {
        return Err(EstimatorError::InvalidPrice);
    }

    let total_return = (latest_price / first_price) - 1.0;
    let short_window = closes.len().min(12);
    let long_window = closes.len().min(48);
    let short_average = mean(&closes[closes.len() - short_window..]);
    let long_average = mean(&closes[closes.len() - long_window..]);
    let short_momentum = if long_average.abs() > f64::EPSILON {
        (short_average / long_average) - 1.0
    } else {
        0.0
    };
    let returns = closes
        .windows(2)
        .filter_map(|window| {
            let previous = window.first().copied()?;
            let current = window.get(1).copied()?;
            (previous > 0.0).then_some((current / previous) - 1.0)
        })
        .collect::<Vec<_>>();
    let volatility = sample_std_dev(&returns);
    let latest = points.last();

    Ok(PriceSignal {
        total_return,
        short_momentum,
        volatility,
        latest_price,
        latest_volume: latest.and_then(|point| decimal_to_f64(point.volume)),
        latest_observed_at: latest.map(|point| point.observed_at),
    })
}

fn market_trend_evidence(
    price: &PriceSignal,
    cross_asset_signal: Option<f64>,
) -> Vec<MarketTrendEvidence> {
    let mut evidence = vec![
        MarketTrendEvidence {
            name: "total_return",
            value: round4(price.total_return),
            unit: "ratio",
            observed_at: price.latest_observed_at,
        },
        MarketTrendEvidence {
            name: "short_momentum",
            value: round4(price.short_momentum),
            unit: "ratio",
            observed_at: price.latest_observed_at,
        },
        MarketTrendEvidence {
            name: "volatility",
            value: round4(price.volatility),
            unit: "sample_std_dev",
            observed_at: price.latest_observed_at,
        },
        MarketTrendEvidence {
            name: "latest_price",
            value: round4(price.latest_price),
            unit: "price",
            observed_at: price.latest_observed_at,
        },
    ];
    if let Some(volume) = price.latest_volume {
        evidence.push(MarketTrendEvidence {
            name: "latest_volume",
            value: round4(volume),
            unit: "volume",
            observed_at: price.latest_observed_at,
        });
    }
    if let Some(value) = cross_asset_signal {
        evidence.push(MarketTrendEvidence {
            name: "relative_peer_performance",
            value: round4(value),
            unit: "ratio",
            observed_at: price.latest_observed_at,
        });
    }
    evidence
}

fn heuristic_news_sentiment(text: &str) -> f64 {
    let lower = text.to_ascii_lowercase();
    let positive = [
        "beat", "beats", "upgrade", "upgraded", "growth", "profit", "record", "surge", "rally",
        "strong", "positive", "outperform",
    ];
    let negative = [
        "miss", "misses", "downgrade", "downgraded", "loss", "weak", "lawsuit", "probe",
        "default", "cut", "warning", "negative", "underperform",
    ];
    let positive_hits = positive
        .iter()
        .filter(|word| lower.contains(**word))
        .count() as f64;
    let negative_hits = negative
        .iter()
        .filter(|word| lower.contains(**word))
        .count() as f64;

    ((positive_hits - negative_hits) / 4.0).clamp(-1.0, 1.0)
}

fn valuation_score(pe_ratio: Option<f64>) -> f64 {
    match pe_ratio {
        Some(value) if value > 0.0 && value <= 18.0 => 0.35,
        Some(value) if value > 18.0 && value <= 35.0 => 0.1,
        Some(value) if value > 35.0 => -0.2,
        Some(value) if value < 0.0 => -0.3,
        _ => 0.0,
    }
}

fn direction_from_score(score: f64) -> EstimatorDirection {
    if score >= 0.08 {
        EstimatorDirection::Bullish
    } else if score <= -0.08 {
        EstimatorDirection::Bearish
    } else {
        EstimatorDirection::Neutral
    }
}

fn with_primary_symbol(primary: &str, symbols: Vec<String>) -> Vec<String> {
    let mut combined = vec![primary.to_owned()];
    for symbol in symbols {
        if !combined.iter().any(|current| current == &symbol) {
            combined.push(symbol);
        }
    }
    combined.truncate(MAX_COMPARISON_SYMBOLS);
    combined
}

fn parse_comparison_symbols(value: Option<&str>) -> Result<Vec<String>, EstimatorError> {
    let mut symbols = Vec::new();
    for raw in value.unwrap_or_default().split(',') {
        let Some(symbol) = normalize_optional(raw.to_owned()) else {
            continue;
        };
        let symbol = symbol.to_ascii_uppercase();
        if !symbols.contains(&symbol) {
            symbols.push(symbol);
        }
    }
    symbols.truncate(MAX_COMPARISON_SYMBOLS);
    Ok(symbols)
}

fn normalize_interval(value: &str) -> Result<String, EstimatorError> {
    match value.trim() {
        "1m" | "5m" | "15m" | "30m" | "1h" | "4h" | "1d" | "1w" | "1mo" => {
            Ok(value.trim().to_owned())
        }
        _ => Err(EstimatorError::InvalidInterval),
    }
}

fn normalize_optional(value: String) -> Option<String> {
    let normalized = value.trim().to_owned();
    (!normalized.is_empty()).then_some(normalized)
}

fn decimal_to_f64(value: Option<Decimal>) -> Option<f64> {
    value.and_then(|decimal| decimal.to_f64())
}

fn sample_std_dev(values: &[f64]) -> f64 {
    if values.len() < 2 {
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

fn round2(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

fn round4(value: f64) -> f64 {
    (value * 10_000.0).round() / 10_000.0
}

impl From<EstimatorInstrumentRow> for EstimatorInstrument {
    fn from(row: EstimatorInstrumentRow) -> Self {
        Self {
            id: row.id,
            canonical_symbol: row.canonical_symbol,
            display_name: row.display_name,
            asset_class: row.asset_class,
        }
    }
}

#[derive(Debug, Serialize, PartialEq, Eq)]
struct EstimatorErrorResponse {
    error: &'static str,
    message: String,
}

#[derive(Debug, Error)]
pub enum EstimatorError {
    #[error("instrument id must be positive")]
    InvalidInstrumentId,
    #[error("instrument_id or symbol is required")]
    MissingInstrumentSelector,
    #[error("invalid estimator interval")]
    InvalidInterval,
    #[error("instrument was not found")]
    InstrumentNotFound,
    #[error("not enough cached price data to estimate")]
    InsufficientPriceData,
    #[error("price value cannot be represented as f64")]
    InvalidDecimal,
    #[error("price data must be positive")]
    InvalidPrice,
    #[error("estimator database query failed: {0}")]
    Database(#[from] sqlx::Error),
    #[error("cross-asset analytics failed: {0}")]
    Analytics(#[from] AnalyticsError),
}

fn estimator_error_response(
    error: EstimatorError,
) -> (StatusCode, Json<EstimatorErrorResponse>) {
    let (status, code) = match error {
        EstimatorError::InvalidInstrumentId
        | EstimatorError::MissingInstrumentSelector
        | EstimatorError::InvalidInterval => (StatusCode::BAD_REQUEST, "invalid_estimator_query"),
        EstimatorError::InstrumentNotFound => (StatusCode::NOT_FOUND, "instrument_not_found"),
        EstimatorError::InsufficientPriceData
        | EstimatorError::InvalidDecimal
        | EstimatorError::InvalidPrice => (StatusCode::UNPROCESSABLE_ENTITY, "insufficient_evidence"),
        EstimatorError::Database(_) | EstimatorError::Analytics(_) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "estimator_error")
        }
    };

    (
        status,
        Json(EstimatorErrorResponse {
            error: code,
            message: error.to_string(),
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::{
        composite_signal, direction_from_score, heuristic_news_sentiment, parse_comparison_symbols,
        EstimatorDirection, FundamentalSignal, NewsEvidence, PriceSignal,
    };

    #[test]
    fn composite_ranks_reason_contributions() {
        let price = PriceSignal {
            total_return: 0.12,
            short_momentum: 0.04,
            volatility: 0.01,
            latest_price: 100.0,
            latest_volume: None,
            latest_observed_at: None,
        };
        let fundamentals = FundamentalSignal {
            profitability_score: 0.5,
            valuation_score: 0.1,
            leverage_score: 0.2,
        };
        let news = vec![NewsEvidence {
            id: 1,
            title: "Upgrade after earnings beat".to_owned(),
            source_name: "Wire".to_owned(),
            source_url: "https://news.example.com/a".to_owned(),
            published_at: chrono::Utc::now(),
            sentiment_score: 0.5,
        }];

        let composite = composite_signal(&price, Some(&fundamentals), Some(0.2), &news);
        assert!(composite.certainty_percentage > 50.0);
        assert!(composite.score > 0.0);
        assert_eq!(composite.reasons[0].rank, 1);
    }

    #[test]
    fn news_sentiment_is_bounded() {
        assert!(heuristic_news_sentiment("upgrade growth strong profit") > 0.0);
        assert!(heuristic_news_sentiment("downgrade loss warning weak") < 0.0);
        assert!(heuristic_news_sentiment("beat beat beat beat beat beat") <= 1.0);
    }

    #[test]
    fn parses_comparison_symbols() {
        let symbols = parse_comparison_symbols(Some("spy, qqq, SPY, dia"))
            .expect("symbols should parse");
        assert_eq!(symbols, vec!["SPY", "QQQ", "DIA"]);
    }

    #[test]
    fn maps_direction_thresholds() {
        assert_eq!(direction_from_score(0.1), EstimatorDirection::Bullish);
        assert_eq!(direction_from_score(-0.1), EstimatorDirection::Bearish);
        assert_eq!(direction_from_score(0.01), EstimatorDirection::Neutral);
    }
}
