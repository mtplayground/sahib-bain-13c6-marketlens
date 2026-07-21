use std::str::FromStr;

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

use crate::state::AppState;

const DEFAULT_LIMIT: i64 = 50;
const MAX_LIMIT: i64 = 100;

pub fn router() -> Router<AppState> {
    Router::new().route("/instruments/filter", get(filter_instruments))
}

#[derive(Debug, Deserialize)]
struct InstrumentFilterParams {
    asset_type: Option<String>,
    asset_class: Option<String>,
    region: Option<String>,
    min_price: Option<String>,
    max_price: Option<String>,
    limit: Option<i64>,
}

impl InstrumentFilterParams {
    fn validate(&self) -> Result<ValidatedInstrumentFilter, InstrumentFilterError> {
        let asset_type = self
            .asset_type
            .as_deref()
            .or(self.asset_class.as_deref());
        let min_price = parse_price_bound("min_price", self.min_price.as_deref())?;
        let max_price = parse_price_bound("max_price", self.max_price.as_deref())?;

        if let (Some(min_price), Some(max_price)) = (min_price, max_price) {
            if min_price > max_price {
                return Err(InstrumentFilterError::InvalidPriceRange);
            }
        }

        Ok(ValidatedInstrumentFilter {
            asset_type: normalize_asset_type(asset_type)?,
            region: normalize_upper_optional(self.region.as_deref()),
            min_price,
            max_price,
            limit: self.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT),
        })
    }
}

#[derive(Debug, PartialEq, Eq)]
struct ValidatedInstrumentFilter {
    asset_type: Option<String>,
    region: Option<String>,
    min_price: Option<Decimal>,
    max_price: Option<Decimal>,
    limit: i64,
}

#[derive(Debug, Serialize)]
struct InstrumentFilterResponse {
    filters: AppliedInstrumentFilters,
    count: usize,
    results: Vec<InstrumentFilterResult>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
struct AppliedInstrumentFilters {
    asset_type: Option<String>,
    region: Option<String>,
    min_price: Option<Decimal>,
    max_price: Option<Decimal>,
    limit: i64,
}

impl From<&ValidatedInstrumentFilter> for AppliedInstrumentFilters {
    fn from(filter: &ValidatedInstrumentFilter) -> Self {
        Self {
            asset_type: filter.asset_type.clone(),
            region: filter.region.clone(),
            min_price: filter.min_price,
            max_price: filter.max_price,
            limit: filter.limit,
        }
    }
}

#[derive(Debug, Serialize, PartialEq, Eq)]
struct InstrumentFilterResult {
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
    latest_price: Option<LatestPrice>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
struct LatestPrice {
    close_price: Decimal,
    observed_at: DateTime<Utc>,
    currency: Option<String>,
}

#[derive(Debug, FromRow)]
struct InstrumentFilterRow {
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

async fn filter_instruments(
    State(state): State<AppState>,
    Query(params): Query<InstrumentFilterParams>,
) -> Result<Json<InstrumentFilterResponse>, (StatusCode, Json<FilterErrorResponse>)> {
    let filter = params.validate().map_err(filter_error_response)?;
    let rows = fetch_filtered_instruments(&state, &filter)
        .await
        .map_err(InstrumentFilterError::Database)
        .map_err(filter_error_response)?;
    let results = rows
        .into_iter()
        .map(InstrumentFilterResult::from_row)
        .collect::<Vec<_>>();

    Ok(Json(InstrumentFilterResponse {
        filters: AppliedInstrumentFilters::from(&filter),
        count: results.len(),
        results,
    }))
}

async fn fetch_filtered_instruments(
    state: &AppState,
    filter: &ValidatedInstrumentFilter,
) -> Result<Vec<InstrumentFilterRow>, sqlx::Error> {
    sqlx::query_as::<_, InstrumentFilterRow>(
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
        WHERE ($1::TEXT IS NULL OR i.asset_class = $1)
            AND ($2::TEXT IS NULL OR i.region = $2)
            AND ($3::NUMERIC IS NULL OR latest_price.close_price >= $3)
            AND ($4::NUMERIC IS NULL OR latest_price.close_price <= $4)
        ORDER BY i.asset_class ASC, i.region ASC, i.canonical_symbol ASC
        LIMIT $5
        "#,
    )
    .bind(filter.asset_type.as_deref())
    .bind(filter.region.as_deref())
    .bind(filter.min_price)
    .bind(filter.max_price)
    .bind(filter.limit)
    .fetch_all(state.database().pool())
    .await
}

impl InstrumentFilterResult {
    fn from_row(row: InstrumentFilterRow) -> Self {
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
            latest_price: match (row.latest_close_price, row.latest_price_observed_at) {
                (Some(close_price), Some(observed_at)) => Some(LatestPrice {
                    close_price,
                    observed_at,
                    currency: row.latest_price_currency,
                }),
                _ => None,
            },
        }
    }
}

fn normalize_asset_type(value: Option<&str>) -> Result<Option<String>, InstrumentFilterError> {
    match normalize_optional(value).as_deref() {
        Some("equity") => Ok(Some("equity".to_owned())),
        Some("corporate_bond") => Ok(Some("corporate_bond".to_owned())),
        Some("government_bond") => Ok(Some("government_bond".to_owned())),
        Some(_) => Err(InstrumentFilterError::InvalidAssetType),
        None => Ok(None),
    }
}

fn parse_price_bound(
    field: &'static str,
    value: Option<&str>,
) -> Result<Option<Decimal>, InstrumentFilterError> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };

    let price = Decimal::from_str(value)
        .map_err(|_| InstrumentFilterError::InvalidPrice { field })?;
    if price <= Decimal::ZERO {
        return Err(InstrumentFilterError::InvalidPrice { field });
    }

    Ok(Some(price))
}

fn normalize_optional(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_lowercase())
}

fn normalize_upper_optional(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_uppercase())
}

#[derive(Debug, Error)]
enum InstrumentFilterError {
    #[error("asset_type must be equity, corporate_bond, or government_bond")]
    InvalidAssetType,
    #[error("{field} must be a positive decimal value")]
    InvalidPrice { field: &'static str },
    #[error("min_price cannot be greater than max_price")]
    InvalidPriceRange,
    #[error("instrument filter query failed: {0}")]
    Database(#[source] sqlx::Error),
}

impl InstrumentFilterError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::InvalidAssetType | Self::InvalidPrice { .. } | Self::InvalidPriceRange => {
                StatusCode::BAD_REQUEST
            }
            Self::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn code(&self) -> &'static str {
        match self {
            Self::InvalidAssetType => "invalid_asset_type",
            Self::InvalidPrice { .. } => "invalid_price",
            Self::InvalidPriceRange => "invalid_price_range",
            Self::Database(_) => "instrument_filter_failed",
        }
    }

    fn public_message(&self) -> String {
        match self {
            Self::Database(_) => "instrument filtering is temporarily unavailable".to_owned(),
            _ => self.to_string(),
        }
    }
}

#[derive(Debug, Serialize)]
struct FilterErrorResponse {
    error: &'static str,
    message: String,
}

fn filter_error_response(
    error: InstrumentFilterError,
) -> (StatusCode, Json<FilterErrorResponse>) {
    if matches!(error, InstrumentFilterError::Database(_)) {
        tracing::error!(%error, "instrument filter failed");
    }

    (
        error.status_code(),
        Json(FilterErrorResponse {
            error: error.code(),
            message: error.public_message(),
        }),
    )
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use rust_decimal::Decimal;

    use super::{
        InstrumentFilterParams, InstrumentFilterResult, InstrumentFilterRow,
        ValidatedInstrumentFilter,
    };

    #[test]
    fn validates_filters_and_clamps_limit() {
        let params = InstrumentFilterParams {
            asset_type: Some(" EQUITY ".to_owned()),
            asset_class: None,
            region: Some(" us ".to_owned()),
            min_price: Some("10.25".to_owned()),
            max_price: Some("250".to_owned()),
            limit: Some(500),
        };

        assert_eq!(
            params.validate().expect("params should be valid"),
            ValidatedInstrumentFilter {
                asset_type: Some("equity".to_owned()),
                region: Some("US".to_owned()),
                min_price: Some(Decimal::new(1025, 2)),
                max_price: Some(Decimal::new(250, 0)),
                limit: 100,
            }
        );
    }

    #[test]
    fn accepts_asset_class_alias_when_asset_type_is_missing() {
        let params = InstrumentFilterParams {
            asset_type: None,
            asset_class: Some("government_bond".to_owned()),
            region: None,
            min_price: None,
            max_price: None,
            limit: None,
        };

        assert_eq!(
            params.validate().expect("params should be valid").asset_type,
            Some("government_bond".to_owned())
        );
    }

    #[test]
    fn rejects_invalid_asset_type() {
        let params = InstrumentFilterParams {
            asset_type: Some("crypto".to_owned()),
            asset_class: None,
            region: None,
            min_price: None,
            max_price: None,
            limit: None,
        };

        assert!(params.validate().is_err());
    }

    #[test]
    fn rejects_inverted_price_range() {
        let params = InstrumentFilterParams {
            asset_type: None,
            asset_class: None,
            region: None,
            min_price: Some("200".to_owned()),
            max_price: Some("100".to_owned()),
            limit: None,
        };

        assert!(params.validate().is_err());
    }

    #[test]
    fn maps_latest_price_when_present() {
        let Some(updated_at) = Utc.with_ymd_and_hms(2026, 7, 20, 12, 0, 0).single() else {
            panic!("valid timestamp");
        };
        let Some(observed_at) = Utc.with_ymd_and_hms(2026, 7, 20, 12, 1, 0).single() else {
            panic!("valid timestamp");
        };
        let row = InstrumentFilterRow {
            id: 3,
            canonical_symbol: "SPY".to_owned(),
            display_name: "SPDR S&P 500 ETF".to_owned(),
            asset_class: "equity".to_owned(),
            region: "US".to_owned(),
            country: Some("US".to_owned()),
            currency: Some("USD".to_owned()),
            exchange: Some("ARCX".to_owned()),
            issuer_name: Some("State Street".to_owned()),
            issuer_region: Some("US".to_owned()),
            maturity_date: None,
            status: "active".to_owned(),
            updated_at,
            latest_close_price: Some(Decimal::new(51234, 2)),
            latest_price_observed_at: Some(observed_at),
            latest_price_currency: Some("USD".to_owned()),
        };

        let result = InstrumentFilterResult::from_row(row);

        assert_eq!(result.id, 3);
        assert_eq!(
            result.latest_price.expect("latest price").close_price,
            Decimal::new(51234, 2)
        );
    }
}
