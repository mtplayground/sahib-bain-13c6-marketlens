#![allow(dead_code)]

use axum::{
    extract::{Query, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use thiserror::Error;

use crate::{
    db::Database,
    market_data::{
        FundamentalsRequest, MarketDataError, MarketDataProvider, ProviderBondYieldCurvePoint,
        ProviderCompanyFinancial, ProviderCreditRating, ProviderFundamentalsSnapshot,
        ProviderInstrumentRef, ProviderKeyRatios,
    },
    state::AppState,
};

const DEFAULT_LIMIT: i64 = 4;
const MAX_LIMIT: i64 = 12;
const MAX_SYMBOL_LENGTH: usize = 64;

pub fn router() -> Router<AppState> {
    Router::new().route("/fundamentals", get(fundamentals_panel))
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, PartialEq)]
pub struct CompanyFinancial {
    pub id: i64,
    pub instrument_id: i64,
    pub provider: String,
    pub fiscal_period_end: NaiveDate,
    pub fiscal_period_type: String,
    pub currency: Option<String>,
    pub revenue: Option<Decimal>,
    pub gross_profit: Option<Decimal>,
    pub operating_income: Option<Decimal>,
    pub net_income: Option<Decimal>,
    pub ebitda: Option<Decimal>,
    pub eps_diluted: Option<Decimal>,
    pub total_assets: Option<Decimal>,
    pub total_liabilities: Option<Decimal>,
    pub shareholder_equity: Option<Decimal>,
    pub operating_cash_flow: Option<Decimal>,
    pub free_cash_flow: Option<Decimal>,
    pub source_updated_at: Option<DateTime<Utc>>,
    pub fetched_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, PartialEq)]
pub struct BondYieldCurvePoint {
    pub id: i64,
    pub instrument_id: i64,
    pub provider: String,
    pub curve_name: String,
    pub region: Option<String>,
    pub currency: Option<String>,
    pub tenor_months: i32,
    pub yield_percent: Decimal,
    pub observed_at: DateTime<Utc>,
    pub source_updated_at: Option<DateTime<Utc>>,
    pub fetched_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, PartialEq, Eq)]
pub struct CreditRating {
    pub id: i64,
    pub instrument_id: i64,
    pub provider: String,
    pub agency: String,
    pub rating_type: String,
    pub rating: String,
    pub outlook: Option<String>,
    pub watch_status: Option<String>,
    pub effective_at: Option<DateTime<Utc>>,
    pub source_updated_at: Option<DateTime<Utc>>,
    pub fetched_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, PartialEq)]
pub struct KeyRatios {
    pub id: i64,
    pub instrument_id: i64,
    pub provider: String,
    pub as_of_date: NaiveDate,
    pub pe_ratio: Option<Decimal>,
    pub pb_ratio: Option<Decimal>,
    pub ps_ratio: Option<Decimal>,
    pub dividend_yield: Option<Decimal>,
    pub return_on_equity: Option<Decimal>,
    pub return_on_assets: Option<Decimal>,
    pub debt_to_equity: Option<Decimal>,
    pub current_ratio: Option<Decimal>,
    pub quick_ratio: Option<Decimal>,
    pub gross_margin: Option<Decimal>,
    pub operating_margin: Option<Decimal>,
    pub net_margin: Option<Decimal>,
    pub source_updated_at: Option<DateTime<Utc>>,
    pub fetched_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
struct FundamentalsParams {
    instrument_id: Option<i64>,
    symbol: Option<String>,
    provider: Option<String>,
    limit: Option<i64>,
}

impl FundamentalsParams {
    fn validate(&self) -> Result<ValidatedFundamentalsQuery, FundamentalsApiError> {
        let instrument_id = validate_optional_instrument_id(self.instrument_id)?;
        let symbol = normalize_symbol(self.symbol.as_deref())?;

        if instrument_id.is_none() && symbol.is_none() {
            return Err(FundamentalsApiError::MissingInstrumentSelector);
        }

        Ok(ValidatedFundamentalsQuery {
            instrument_id,
            symbol,
            provider: normalize_provider(self.provider.as_deref()),
            limit: self.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT),
        })
    }
}

#[derive(Debug, PartialEq, Eq)]
struct ValidatedFundamentalsQuery {
    instrument_id: Option<i64>,
    symbol: Option<String>,
    provider: Option<String>,
    limit: i64,
}

#[derive(Debug, Serialize, PartialEq)]
struct FundamentalsResponse {
    query: AppliedFundamentalsQuery,
    instrument: FundamentalsInstrument,
    latest_price: Option<FundamentalsLatestPrice>,
    company_financials: Vec<CompanyFinancial>,
    key_ratios: Vec<KeyRatios>,
    credit_ratings: Vec<CreditRating>,
    bond_yield_curve_points: Vec<BondYieldCurvePoint>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
struct AppliedFundamentalsQuery {
    instrument_id: Option<i64>,
    symbol: Option<String>,
    provider: Option<String>,
    limit: i64,
}

impl From<&ValidatedFundamentalsQuery> for AppliedFundamentalsQuery {
    fn from(query: &ValidatedFundamentalsQuery) -> Self {
        Self {
            instrument_id: query.instrument_id,
            symbol: query.symbol.clone(),
            provider: query.provider.clone(),
            limit: query.limit,
        }
    }
}

#[derive(Debug, Serialize, PartialEq, Eq)]
struct FundamentalsInstrument {
    id: i64,
    canonical_symbol: String,
    display_name: String,
    asset_class: String,
    region: String,
    country: Option<String>,
    currency: Option<String>,
    exchange: Option<String>,
    issuer_name: Option<String>,
    issuer_region: Option<String>,
    maturity_date: Option<NaiveDate>,
    status: String,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, PartialEq)]
struct FundamentalsLatestPrice {
    close_price: Decimal,
    observed_at: DateTime<Utc>,
    currency: Option<String>,
}

#[derive(Debug, FromRow)]
struct FundamentalsInstrumentRow {
    id: i64,
    canonical_symbol: String,
    display_name: String,
    asset_class: String,
    region: String,
    country: Option<String>,
    currency: Option<String>,
    exchange: Option<String>,
    issuer_name: Option<String>,
    issuer_region: Option<String>,
    maturity_date: Option<NaiveDate>,
    status: String,
    updated_at: DateTime<Utc>,
    latest_close_price: Option<Decimal>,
    latest_price_observed_at: Option<DateTime<Utc>>,
    latest_price_currency: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FundamentalsIngestReport {
    pub instrument_id: i64,
    pub provider: String,
    pub company_financials: usize,
    pub bond_yield_curve_points: usize,
    pub credit_ratings: usize,
    pub key_ratios: usize,
}

pub struct FundamentalsFetcher<P> {
    provider: P,
    database: Database,
}

impl<P> FundamentalsFetcher<P>
where
    P: MarketDataProvider,
{
    pub fn new(provider: P, database: Database) -> Self {
        Self { provider, database }
    }

    pub async fn fetch_for_instrument(
        &self,
        instrument_id: i64,
        instrument: ProviderInstrumentRef,
    ) -> Result<FundamentalsIngestReport, FundamentalsError> {
        validate_instrument_id(instrument_id)?;
        let request = FundamentalsRequest::new(instrument)?;
        let snapshot = self.provider.fundamentals(request).await?;
        persist_snapshot(
            &self.database,
            self.provider.name(),
            instrument_id,
            snapshot,
        )
        .await
    }
}

pub async fn persist_snapshot(
    database: &Database,
    provider: &str,
    instrument_id: i64,
    snapshot: ProviderFundamentalsSnapshot,
) -> Result<FundamentalsIngestReport, FundamentalsError> {
    validate_instrument_id(instrument_id)?;
    let provider = normalize_required(provider, FundamentalsError::EmptyProvider)?;

    let mut company_financials = 0_usize;
    let mut bond_yield_curve_points = 0_usize;
    let mut credit_ratings = 0_usize;
    let mut key_ratios = 0_usize;

    for financial in snapshot.company_financials {
        upsert_company_financial(database, provider.as_str(), instrument_id, financial).await?;
        company_financials += 1;
    }

    for point in snapshot.bond_yield_curve_points {
        upsert_bond_yield_curve_point(database, provider.as_str(), instrument_id, point).await?;
        bond_yield_curve_points += 1;
    }

    for rating in snapshot.credit_ratings {
        upsert_credit_rating(database, provider.as_str(), instrument_id, rating).await?;
        credit_ratings += 1;
    }

    for ratios in snapshot.key_ratios {
        upsert_key_ratios(database, provider.as_str(), instrument_id, ratios).await?;
        key_ratios += 1;
    }

    Ok(FundamentalsIngestReport {
        instrument_id,
        provider,
        company_financials,
        bond_yield_curve_points,
        credit_ratings,
        key_ratios,
    })
}

async fn fundamentals_panel(
    State(state): State<AppState>,
    Query(params): Query<FundamentalsParams>,
) -> Result<Json<FundamentalsResponse>, (StatusCode, Json<FundamentalsErrorResponse>)> {
    let query = params.validate().map_err(fundamentals_error_response)?;
    let instrument_row = fetch_instrument(&state, &query)
        .await
        .map_err(FundamentalsApiError::Database)
        .map_err(fundamentals_error_response)?
        .ok_or(FundamentalsApiError::InstrumentNotFound)
        .map_err(fundamentals_error_response)?;
    let instrument_id = instrument_row.id;
    let latest_price = latest_price_from_row(&instrument_row);
    let instrument = FundamentalsInstrument::from(instrument_row);

    let company_financials = fetch_company_financials(&state, instrument_id, &query)
        .await
        .map_err(FundamentalsApiError::Database)
        .map_err(fundamentals_error_response)?;
    let key_ratios = fetch_key_ratios(&state, instrument_id, &query)
        .await
        .map_err(FundamentalsApiError::Database)
        .map_err(fundamentals_error_response)?;
    let credit_ratings = fetch_credit_ratings(&state, instrument_id, &query)
        .await
        .map_err(FundamentalsApiError::Database)
        .map_err(fundamentals_error_response)?;
    let bond_yield_curve_points = fetch_bond_yield_curve_points(&state, instrument_id, &query)
        .await
        .map_err(FundamentalsApiError::Database)
        .map_err(fundamentals_error_response)?;

    Ok(Json(FundamentalsResponse {
        query: AppliedFundamentalsQuery::from(&query),
        instrument,
        latest_price,
        company_financials,
        key_ratios,
        credit_ratings,
        bond_yield_curve_points,
    }))
}

async fn fetch_instrument(
    state: &AppState,
    query: &ValidatedFundamentalsQuery,
) -> Result<Option<FundamentalsInstrumentRow>, sqlx::Error> {
    sqlx::query_as::<_, FundamentalsInstrumentRow>(
        r#"
        SELECT
            i.id,
            i.canonical_symbol,
            i.display_name,
            i.asset_class,
            i.region,
            i.country,
            i.currency,
            i.exchange,
            i.issuer_name,
            i.issuer_region,
            i.maturity_date,
            i.status,
            i.updated_at,
            latest_price.close_price AS latest_close_price,
            latest_price.observed_at AS latest_price_observed_at,
            latest_price.currency AS latest_price_currency
        FROM instruments i
        LEFT JOIN LATERAL (
            SELECT
                p.close_price,
                p.observed_at,
                s.currency
            FROM price_series_cache s
            INNER JOIN price_series_points p ON p.series_id = s.id
            WHERE lower(s.symbol) = lower(i.canonical_symbol)
                AND s.asset_class = i.asset_class
            ORDER BY p.observed_at DESC
            LIMIT 1
        ) latest_price ON TRUE
        WHERE ($1::BIGINT IS NULL OR i.id = $1)
            AND ($2::TEXT IS NULL OR lower(i.canonical_symbol) = lower($2))
        ORDER BY i.updated_at DESC
        LIMIT 1
        "#,
    )
    .bind(query.instrument_id)
    .bind(query.symbol.as_deref())
    .fetch_optional(state.database().pool())
    .await
}

async fn fetch_company_financials(
    state: &AppState,
    instrument_id: i64,
    query: &ValidatedFundamentalsQuery,
) -> Result<Vec<CompanyFinancial>, sqlx::Error> {
    sqlx::query_as::<_, CompanyFinancial>(
        r#"
        SELECT *
        FROM company_financials
        WHERE instrument_id = $1
            AND ($2::TEXT IS NULL OR provider = $2)
        ORDER BY fiscal_period_end DESC, fetched_at DESC
        LIMIT $3
        "#,
    )
    .bind(instrument_id)
    .bind(query.provider.as_deref())
    .bind(query.limit)
    .fetch_all(state.database().pool())
    .await
}

async fn fetch_key_ratios(
    state: &AppState,
    instrument_id: i64,
    query: &ValidatedFundamentalsQuery,
) -> Result<Vec<KeyRatios>, sqlx::Error> {
    sqlx::query_as::<_, KeyRatios>(
        r#"
        SELECT *
        FROM key_ratios
        WHERE instrument_id = $1
            AND ($2::TEXT IS NULL OR provider = $2)
        ORDER BY as_of_date DESC, fetched_at DESC
        LIMIT $3
        "#,
    )
    .bind(instrument_id)
    .bind(query.provider.as_deref())
    .bind(query.limit)
    .fetch_all(state.database().pool())
    .await
}

async fn fetch_credit_ratings(
    state: &AppState,
    instrument_id: i64,
    query: &ValidatedFundamentalsQuery,
) -> Result<Vec<CreditRating>, sqlx::Error> {
    sqlx::query_as::<_, CreditRating>(
        r#"
        SELECT *
        FROM credit_ratings
        WHERE instrument_id = $1
            AND ($2::TEXT IS NULL OR provider = $2)
        ORDER BY agency ASC, rating_type ASC, fetched_at DESC
        LIMIT $3
        "#,
    )
    .bind(instrument_id)
    .bind(query.provider.as_deref())
    .bind(query.limit)
    .fetch_all(state.database().pool())
    .await
}

async fn fetch_bond_yield_curve_points(
    state: &AppState,
    instrument_id: i64,
    query: &ValidatedFundamentalsQuery,
) -> Result<Vec<BondYieldCurvePoint>, sqlx::Error> {
    sqlx::query_as::<_, BondYieldCurvePoint>(
        r#"
        SELECT *
        FROM bond_yield_curve_points
        WHERE instrument_id = $1
            AND ($2::TEXT IS NULL OR provider = $2)
        ORDER BY observed_at DESC, tenor_months ASC
        LIMIT $3
        "#,
    )
    .bind(instrument_id)
    .bind(query.provider.as_deref())
    .bind(query.limit * 6)
    .fetch_all(state.database().pool())
    .await
}

async fn upsert_company_financial(
    database: &Database,
    provider: &str,
    instrument_id: i64,
    financial: ProviderCompanyFinancial,
) -> Result<(), FundamentalsError> {
    let fiscal_period_type = normalize_period_type(financial.fiscal_period_type.as_str())?;
    let currency = normalize_upper_optional(financial.currency);

    sqlx::query(
        r#"
        INSERT INTO company_financials (
            instrument_id,
            provider,
            fiscal_period_end,
            fiscal_period_type,
            currency,
            revenue,
            gross_profit,
            operating_income,
            net_income,
            ebitda,
            eps_diluted,
            total_assets,
            total_liabilities,
            shareholder_equity,
            operating_cash_flow,
            free_cash_flow,
            source_updated_at,
            fetched_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, NOW())
        ON CONFLICT (instrument_id, provider, fiscal_period_end, fiscal_period_type) DO UPDATE
        SET
            currency = EXCLUDED.currency,
            revenue = EXCLUDED.revenue,
            gross_profit = EXCLUDED.gross_profit,
            operating_income = EXCLUDED.operating_income,
            net_income = EXCLUDED.net_income,
            ebitda = EXCLUDED.ebitda,
            eps_diluted = EXCLUDED.eps_diluted,
            total_assets = EXCLUDED.total_assets,
            total_liabilities = EXCLUDED.total_liabilities,
            shareholder_equity = EXCLUDED.shareholder_equity,
            operating_cash_flow = EXCLUDED.operating_cash_flow,
            free_cash_flow = EXCLUDED.free_cash_flow,
            source_updated_at = EXCLUDED.source_updated_at,
            fetched_at = NOW()
        "#,
    )
    .bind(instrument_id)
    .bind(provider)
    .bind(financial.fiscal_period_end)
    .bind(fiscal_period_type.as_str())
    .bind(currency.as_deref())
    .bind(financial.revenue)
    .bind(financial.gross_profit)
    .bind(financial.operating_income)
    .bind(financial.net_income)
    .bind(financial.ebitda)
    .bind(financial.eps_diluted)
    .bind(financial.total_assets)
    .bind(financial.total_liabilities)
    .bind(financial.shareholder_equity)
    .bind(financial.operating_cash_flow)
    .bind(financial.free_cash_flow)
    .bind(financial.source_updated_at)
    .execute(database.pool())
    .await?;

    Ok(())
}

async fn upsert_bond_yield_curve_point(
    database: &Database,
    provider: &str,
    instrument_id: i64,
    point: ProviderBondYieldCurvePoint,
) -> Result<(), FundamentalsError> {
    if point.tenor_months <= 0 {
        return Err(FundamentalsError::InvalidTenor);
    }
    if point.yield_percent < Decimal::new(-100_000, 3) {
        return Err(FundamentalsError::InvalidYield);
    }

    let curve_name = normalize_required(point.curve_name.as_str(), FundamentalsError::EmptyCurve)?;
    let region = normalize_upper_optional(point.region);
    let currency = normalize_upper_optional(point.currency);

    sqlx::query(
        r#"
        INSERT INTO bond_yield_curve_points (
            instrument_id,
            provider,
            curve_name,
            region,
            currency,
            tenor_months,
            yield_percent,
            observed_at,
            source_updated_at,
            fetched_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, NOW())
        ON CONFLICT (instrument_id, provider, curve_name, tenor_months, observed_at) DO UPDATE
        SET
            region = EXCLUDED.region,
            currency = EXCLUDED.currency,
            yield_percent = EXCLUDED.yield_percent,
            source_updated_at = EXCLUDED.source_updated_at,
            fetched_at = NOW()
        "#,
    )
    .bind(instrument_id)
    .bind(provider)
    .bind(curve_name.as_str())
    .bind(region.as_deref())
    .bind(currency.as_deref())
    .bind(point.tenor_months)
    .bind(point.yield_percent)
    .bind(point.observed_at)
    .bind(point.source_updated_at)
    .execute(database.pool())
    .await?;

    Ok(())
}

async fn upsert_credit_rating(
    database: &Database,
    provider: &str,
    instrument_id: i64,
    rating: ProviderCreditRating,
) -> Result<(), FundamentalsError> {
    let agency = normalize_required(rating.agency.as_str(), FundamentalsError::EmptyAgency)?;
    let rating_type = normalize_required(
        rating.rating_type.as_str(),
        FundamentalsError::EmptyRatingType,
    )?;
    let rating_value = normalize_required(rating.rating.as_str(), FundamentalsError::EmptyRating)?;
    let outlook = normalize_optional(rating.outlook);
    let watch_status = normalize_optional(rating.watch_status);

    sqlx::query(
        r#"
        INSERT INTO credit_ratings (
            instrument_id,
            provider,
            agency,
            rating_type,
            rating,
            outlook,
            watch_status,
            effective_at,
            source_updated_at,
            fetched_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, NOW())
        ON CONFLICT (instrument_id, provider, agency, rating_type) DO UPDATE
        SET
            rating = EXCLUDED.rating,
            outlook = EXCLUDED.outlook,
            watch_status = EXCLUDED.watch_status,
            effective_at = EXCLUDED.effective_at,
            source_updated_at = EXCLUDED.source_updated_at,
            fetched_at = NOW()
        "#,
    )
    .bind(instrument_id)
    .bind(provider)
    .bind(agency.as_str())
    .bind(rating_type.as_str())
    .bind(rating_value.as_str())
    .bind(outlook.as_deref())
    .bind(watch_status.as_deref())
    .bind(rating.effective_at)
    .bind(rating.source_updated_at)
    .execute(database.pool())
    .await?;

    Ok(())
}

async fn upsert_key_ratios(
    database: &Database,
    provider: &str,
    instrument_id: i64,
    ratios: ProviderKeyRatios,
) -> Result<(), FundamentalsError> {
    sqlx::query(
        r#"
        INSERT INTO key_ratios (
            instrument_id,
            provider,
            as_of_date,
            pe_ratio,
            pb_ratio,
            ps_ratio,
            dividend_yield,
            return_on_equity,
            return_on_assets,
            debt_to_equity,
            current_ratio,
            quick_ratio,
            gross_margin,
            operating_margin,
            net_margin,
            source_updated_at,
            fetched_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, NOW())
        ON CONFLICT (instrument_id, provider, as_of_date) DO UPDATE
        SET
            pe_ratio = EXCLUDED.pe_ratio,
            pb_ratio = EXCLUDED.pb_ratio,
            ps_ratio = EXCLUDED.ps_ratio,
            dividend_yield = EXCLUDED.dividend_yield,
            return_on_equity = EXCLUDED.return_on_equity,
            return_on_assets = EXCLUDED.return_on_assets,
            debt_to_equity = EXCLUDED.debt_to_equity,
            current_ratio = EXCLUDED.current_ratio,
            quick_ratio = EXCLUDED.quick_ratio,
            gross_margin = EXCLUDED.gross_margin,
            operating_margin = EXCLUDED.operating_margin,
            net_margin = EXCLUDED.net_margin,
            source_updated_at = EXCLUDED.source_updated_at,
            fetched_at = NOW()
        "#,
    )
    .bind(instrument_id)
    .bind(provider)
    .bind(ratios.as_of_date)
    .bind(ratios.pe_ratio)
    .bind(ratios.pb_ratio)
    .bind(ratios.ps_ratio)
    .bind(ratios.dividend_yield)
    .bind(ratios.return_on_equity)
    .bind(ratios.return_on_assets)
    .bind(ratios.debt_to_equity)
    .bind(ratios.current_ratio)
    .bind(ratios.quick_ratio)
    .bind(ratios.gross_margin)
    .bind(ratios.operating_margin)
    .bind(ratios.net_margin)
    .bind(ratios.source_updated_at)
    .execute(database.pool())
    .await?;

    Ok(())
}

fn validate_instrument_id(instrument_id: i64) -> Result<(), FundamentalsError> {
    if instrument_id <= 0 {
        Err(FundamentalsError::InvalidInstrumentId)
    } else {
        Ok(())
    }
}

fn validate_optional_instrument_id(
    instrument_id: Option<i64>,
) -> Result<Option<i64>, FundamentalsApiError> {
    match instrument_id {
        Some(value) if value <= 0 => Err(FundamentalsApiError::InvalidInstrumentId),
        value => Ok(value),
    }
}

fn normalize_symbol(value: Option<&str>) -> Result<Option<String>, FundamentalsApiError> {
    let Some(symbol) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    if symbol.len() > MAX_SYMBOL_LENGTH {
        return Err(FundamentalsApiError::SymbolTooLong);
    }
    Ok(Some(symbol.to_ascii_uppercase()))
}

fn normalize_provider(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn normalize_period_type(value: &str) -> Result<String, FundamentalsError> {
    let normalized = value
        .trim()
        .to_ascii_lowercase()
        .chars()
        .map(|character| match character {
            ' ' | '-' => '_',
            character => character,
        })
        .collect::<String>();
    match normalized.as_str() {
        "annual" | "quarterly" | "ttm" => Ok(normalized),
        _ => Err(FundamentalsError::InvalidFiscalPeriodType),
    }
}

fn normalize_required(value: &str, error: FundamentalsError) -> Result<String, FundamentalsError> {
    let normalized = value.trim().to_owned();
    if normalized.is_empty() {
        Err(error)
    } else {
        Ok(normalized)
    }
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn normalize_upper_optional(value: Option<String>) -> Option<String> {
    normalize_optional(value).map(|value| value.to_ascii_uppercase())
}

fn latest_price_from_row(row: &FundamentalsInstrumentRow) -> Option<FundamentalsLatestPrice> {
    match (row.latest_close_price, row.latest_price_observed_at) {
        (Some(close_price), Some(observed_at)) => Some(FundamentalsLatestPrice {
            close_price,
            observed_at,
            currency: row.latest_price_currency.clone(),
        }),
        _ => None,
    }
}

impl From<FundamentalsInstrumentRow> for FundamentalsInstrument {
    fn from(row: FundamentalsInstrumentRow) -> Self {
        Self {
            id: row.id,
            canonical_symbol: row.canonical_symbol,
            display_name: row.display_name,
            asset_class: row.asset_class,
            region: row.region,
            country: row.country,
            currency: row.currency,
            exchange: row.exchange,
            issuer_name: row.issuer_name,
            issuer_region: row.issuer_region,
            maturity_date: row.maturity_date,
            status: row.status,
            updated_at: row.updated_at,
        }
    }
}

#[derive(Debug, Error)]
pub enum FundamentalsError {
    #[error("instrument_id must be a positive integer")]
    InvalidInstrumentId,
    #[error("provider cannot be empty")]
    EmptyProvider,
    #[error("fiscal period type must be annual, quarterly, or ttm")]
    InvalidFiscalPeriodType,
    #[error("curve name cannot be empty")]
    EmptyCurve,
    #[error("yield curve tenor must be positive")]
    InvalidTenor,
    #[error("yield percent is outside the accepted range")]
    InvalidYield,
    #[error("rating agency cannot be empty")]
    EmptyAgency,
    #[error("rating type cannot be empty")]
    EmptyRatingType,
    #[error("rating cannot be empty")]
    EmptyRating,
    #[error("fundamentals provider failed: {0}")]
    Provider(#[from] MarketDataError),
    #[error("fundamentals database write failed: {0}")]
    Database(#[from] sqlx::Error),
}

#[derive(Debug, Error)]
enum FundamentalsApiError {
    #[error("instrument_id or symbol is required")]
    MissingInstrumentSelector,
    #[error("instrument_id must be a positive integer")]
    InvalidInstrumentId,
    #[error("symbol cannot exceed 64 characters")]
    SymbolTooLong,
    #[error("instrument fundamentals were not found")]
    InstrumentNotFound,
    #[error("fundamentals query failed: {0}")]
    Database(#[source] sqlx::Error),
}

impl FundamentalsApiError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::MissingInstrumentSelector | Self::InvalidInstrumentId | Self::SymbolTooLong => {
                StatusCode::BAD_REQUEST
            }
            Self::InstrumentNotFound => StatusCode::NOT_FOUND,
            Self::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn code(&self) -> &'static str {
        match self {
            Self::MissingInstrumentSelector => "missing_instrument_selector",
            Self::InvalidInstrumentId => "invalid_instrument_id",
            Self::SymbolTooLong => "symbol_too_long",
            Self::InstrumentNotFound => "instrument_not_found",
            Self::Database(_) => "fundamentals_query_failed",
        }
    }

    fn public_message(&self) -> String {
        match self {
            Self::Database(_) => "fundamentals are temporarily unavailable".to_owned(),
            _ => self.to_string(),
        }
    }
}

#[derive(Debug, Serialize)]
struct FundamentalsErrorResponse {
    error: &'static str,
    message: String,
}

fn fundamentals_error_response(
    error: FundamentalsApiError,
) -> (StatusCode, Json<FundamentalsErrorResponse>) {
    if matches!(error, FundamentalsApiError::Database(_)) {
        tracing::error!(%error, "fundamentals query failed");
    }

    (
        error.status_code(),
        Json(FundamentalsErrorResponse {
            error: error.code(),
            message: error.public_message(),
        }),
    )
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;
    use rust_decimal::Decimal;

    use super::{
        normalize_period_type, normalize_symbol, validate_instrument_id,
        validate_optional_instrument_id, FundamentalsApiError, FundamentalsError,
        FundamentalsParams, ValidatedFundamentalsQuery,
    };
    use crate::market_data::ProviderBondYieldCurvePoint;

    #[test]
    fn validates_instrument_id() {
        assert!(validate_instrument_id(1).is_ok());
        assert!(matches!(
            validate_instrument_id(0),
            Err(FundamentalsError::InvalidInstrumentId)
        ));
    }

    #[test]
    fn normalizes_fiscal_period_types() {
        assert_eq!(
            normalize_period_type(" Quarterly ").expect("quarterly should be valid"),
            "quarterly"
        );
        assert_eq!(
            normalize_period_type("TTM").expect("ttm should be valid"),
            "ttm"
        );
        assert!(normalize_period_type("monthly").is_err());
    }

    #[test]
    fn provider_payload_accepts_financial_decimal_values() {
        let point = ProviderBondYieldCurvePoint {
            curve_name: "US Treasury".to_owned(),
            region: Some("us".to_owned()),
            currency: Some("usd".to_owned()),
            tenor_months: 120,
            yield_percent: Decimal::new(425, 2),
            observed_at: NaiveDate::from_ymd_opt(2026, 7, 21)
                .and_then(|date| date.and_hms_opt(0, 0, 0))
                .map(|timestamp| timestamp.and_utc())
                .expect("valid timestamp"),
            source_updated_at: None,
        };

        assert_eq!(point.tenor_months, 120);
        assert_eq!(point.yield_percent, Decimal::new(425, 2));
    }

    #[test]
    fn normalizes_hyphenated_fiscal_period_type() {
        assert_eq!(
            normalize_period_type(" annual ").expect("annual should be valid"),
            "annual"
        );
    }

    #[test]
    fn validates_fundamentals_api_selector() {
        let params = FundamentalsParams {
            instrument_id: Some(42),
            symbol: None,
            provider: Some(" provider-a ".to_owned()),
            limit: Some(99),
        };

        assert_eq!(
            params.validate().expect("selector should be valid"),
            ValidatedFundamentalsQuery {
                instrument_id: Some(42),
                symbol: None,
                provider: Some("provider-a".to_owned()),
                limit: 12,
            }
        );
    }

    #[test]
    fn rejects_missing_and_invalid_fundamentals_selectors() {
        let params = FundamentalsParams {
            instrument_id: None,
            symbol: None,
            provider: None,
            limit: None,
        };
        assert!(matches!(
            params.validate(),
            Err(FundamentalsApiError::MissingInstrumentSelector)
        ));
        assert!(matches!(
            validate_optional_instrument_id(Some(-1)),
            Err(FundamentalsApiError::InvalidInstrumentId)
        ));
        assert!(normalize_symbol(Some("SPY")).is_ok());
    }
}
