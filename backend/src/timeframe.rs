use axum::{
    extract::{Query, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use thiserror::Error;

use crate::state::AppState;

const DEFAULT_LIMIT: i64 = 1_000;
const MAX_LIMIT: i64 = 5_000;
const DEFAULT_INTERVAL: &str = "1m";
const MAX_SYMBOL_LENGTH: usize = 64;

pub fn router() -> Router<AppState> {
    Router::new().route("/series/timeframe", get(timeframe_series))
}

#[derive(Debug, Deserialize)]
struct TimeframeParams {
    instrument_id: Option<i64>,
    symbol: Option<String>,
    provider: Option<String>,
    interval: Option<String>,
    timeframe: Option<String>,
    from: Option<String>,
    start: Option<String>,
    to: Option<String>,
    end: Option<String>,
    limit: Option<i64>,
}

impl TimeframeParams {
    fn validate(&self) -> Result<ValidatedTimeframeQuery, TimeframeError> {
        let instrument_id = validate_instrument_id(self.instrument_id)?;
        let symbol = normalize_symbol(self.symbol.as_deref())?;

        if instrument_id.is_none() && symbol.is_none() {
            return Err(TimeframeError::MissingInstrumentSelector);
        }

        let interval = normalize_interval(
            self.interval
                .as_deref()
                .or(self.timeframe.as_deref())
                .unwrap_or(DEFAULT_INTERVAL),
        )?;
        let from = parse_timestamp("from", self.from.as_deref().or(self.start.as_deref()))?;
        let to = parse_timestamp("to", self.to.as_deref().or(self.end.as_deref()))?;

        if let (Some(from), Some(to)) = (from, to) {
            if from > to {
                return Err(TimeframeError::InvalidTimeRange);
            }
        }

        Ok(ValidatedTimeframeQuery {
            instrument_id,
            symbol,
            provider: normalize_provider(self.provider.as_deref()),
            interval,
            from,
            to,
            limit: self.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT),
        })
    }
}

#[derive(Debug, PartialEq, Eq)]
struct ValidatedTimeframeQuery {
    instrument_id: Option<i64>,
    symbol: Option<String>,
    provider: Option<String>,
    interval: String,
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
    limit: i64,
}

#[derive(Debug, Serialize)]
struct TimeframeResponse {
    query: AppliedTimeframeQuery,
    count: usize,
    series: PriceSeriesSummary,
    points: Vec<TimeframePoint>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
struct AppliedTimeframeQuery {
    instrument_id: Option<i64>,
    symbol: Option<String>,
    provider: Option<String>,
    interval: String,
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
    limit: i64,
}

impl From<&ValidatedTimeframeQuery> for AppliedTimeframeQuery {
    fn from(query: &ValidatedTimeframeQuery) -> Self {
        Self {
            instrument_id: query.instrument_id,
            symbol: query.symbol.clone(),
            provider: query.provider.clone(),
            interval: query.interval.clone(),
            from: query.from,
            to: query.to,
            limit: query.limit,
        }
    }
}

#[derive(Debug, Serialize, PartialEq, Eq)]
struct PriceSeriesSummary {
    id: i64,
    provider: String,
    provider_instrument_id: String,
    symbol: String,
    asset_class: String,
    interval: String,
    currency: Option<String>,
    first_observed_at: Option<DateTime<Utc>>,
    last_observed_at: Option<DateTime<Utc>>,
    last_refreshed_at: Option<DateTime<Utc>>,
    source_updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, PartialEq)]
struct TimeframePoint {
    observed_at: DateTime<Utc>,
    open_price: Option<Decimal>,
    high_price: Option<Decimal>,
    low_price: Option<Decimal>,
    close_price: Decimal,
    volume: Option<Decimal>,
    trade_count: Option<i64>,
    vwap: Option<Decimal>,
    is_final: bool,
    provider_updated_at: Option<DateTime<Utc>>,
    ingested_at: DateTime<Utc>,
}

#[derive(Debug, FromRow)]
struct PriceSeriesSummaryRow {
    id: i64,
    provider: String,
    provider_instrument_id: String,
    symbol: String,
    asset_class: String,
    interval: String,
    currency: Option<String>,
    first_observed_at: Option<DateTime<Utc>>,
    last_observed_at: Option<DateTime<Utc>>,
    last_refreshed_at: Option<DateTime<Utc>>,
    source_updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, FromRow)]
struct TimeframePointRow {
    observed_at: DateTime<Utc>,
    open_price: Option<Decimal>,
    high_price: Option<Decimal>,
    low_price: Option<Decimal>,
    close_price: Decimal,
    volume: Option<Decimal>,
    trade_count: Option<i64>,
    vwap: Option<Decimal>,
    is_final: bool,
    provider_updated_at: Option<DateTime<Utc>>,
    ingested_at: DateTime<Utc>,
}

async fn timeframe_series(
    State(state): State<AppState>,
    Query(params): Query<TimeframeParams>,
) -> Result<Json<TimeframeResponse>, (StatusCode, Json<TimeframeErrorResponse>)> {
    let query = params.validate().map_err(timeframe_error_response)?;
    let series = fetch_series(&state, &query)
        .await
        .map_err(TimeframeError::Database)
        .map_err(timeframe_error_response)?
        .ok_or(TimeframeError::SeriesNotFound)
        .map_err(timeframe_error_response)?;
    let points = fetch_points(&state, series.id, &query)
        .await
        .map_err(TimeframeError::Database)
        .map_err(timeframe_error_response)?
        .into_iter()
        .map(TimeframePoint::from)
        .collect::<Vec<_>>();

    Ok(Json(TimeframeResponse {
        query: AppliedTimeframeQuery::from(&query),
        count: points.len(),
        series: PriceSeriesSummary::from(series),
        points,
    }))
}

async fn fetch_series(
    state: &AppState,
    query: &ValidatedTimeframeQuery,
) -> Result<Option<PriceSeriesSummaryRow>, sqlx::Error> {
    sqlx::query_as::<_, PriceSeriesSummaryRow>(
        r#"
        SELECT
            s.id,
            s.provider,
            s.provider_instrument_id,
            s.symbol,
            s.asset_class,
            s.interval,
            s.currency,
            s.first_observed_at,
            s.last_observed_at,
            s.last_refreshed_at,
            s.source_updated_at
        FROM price_series_cache s
        LEFT JOIN instruments i
            ON lower(i.canonical_symbol) = lower(s.symbol)
            AND i.asset_class = s.asset_class
        WHERE s.interval = $1
            AND ($2::BIGINT IS NULL OR i.id = $2)
            AND ($3::TEXT IS NULL OR lower(s.symbol) = lower($3))
            AND ($4::TEXT IS NULL OR s.provider = $4)
        ORDER BY
            s.last_observed_at DESC NULLS LAST,
            s.last_refreshed_at DESC NULLS LAST,
            s.updated_at DESC
        LIMIT 1
        "#,
    )
    .bind(query.interval.as_str())
    .bind(query.instrument_id)
    .bind(query.symbol.as_deref())
    .bind(query.provider.as_deref())
    .fetch_optional(state.database().pool())
    .await
}

async fn fetch_points(
    state: &AppState,
    series_id: i64,
    query: &ValidatedTimeframeQuery,
) -> Result<Vec<TimeframePointRow>, sqlx::Error> {
    sqlx::query_as::<_, TimeframePointRow>(
        r#"
        SELECT
            observed_at,
            open_price,
            high_price,
            low_price,
            close_price,
            volume,
            trade_count,
            vwap,
            is_final,
            provider_updated_at,
            ingested_at
        FROM price_series_points
        WHERE series_id = $1
            AND ($2::TIMESTAMPTZ IS NULL OR observed_at >= $2)
            AND ($3::TIMESTAMPTZ IS NULL OR observed_at <= $3)
        ORDER BY observed_at ASC
        LIMIT $4
        "#,
    )
    .bind(series_id)
    .bind(query.from)
    .bind(query.to)
    .bind(query.limit)
    .fetch_all(state.database().pool())
    .await
}

impl From<PriceSeriesSummaryRow> for PriceSeriesSummary {
    fn from(row: PriceSeriesSummaryRow) -> Self {
        Self {
            id: row.id,
            provider: row.provider,
            provider_instrument_id: row.provider_instrument_id,
            symbol: row.symbol,
            asset_class: row.asset_class,
            interval: row.interval,
            currency: row.currency,
            first_observed_at: row.first_observed_at,
            last_observed_at: row.last_observed_at,
            last_refreshed_at: row.last_refreshed_at,
            source_updated_at: row.source_updated_at,
        }
    }
}

impl From<TimeframePointRow> for TimeframePoint {
    fn from(row: TimeframePointRow) -> Self {
        Self {
            observed_at: row.observed_at,
            open_price: row.open_price,
            high_price: row.high_price,
            low_price: row.low_price,
            close_price: row.close_price,
            volume: row.volume,
            trade_count: row.trade_count,
            vwap: row.vwap,
            is_final: row.is_final,
            provider_updated_at: row.provider_updated_at,
            ingested_at: row.ingested_at,
        }
    }
}

fn validate_instrument_id(value: Option<i64>) -> Result<Option<i64>, TimeframeError> {
    match value {
        Some(value) if value <= 0 => Err(TimeframeError::InvalidInstrumentId),
        value => Ok(value),
    }
}

fn normalize_symbol(value: Option<&str>) -> Result<Option<String>, TimeframeError> {
    let Some(symbol) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    if symbol.len() > MAX_SYMBOL_LENGTH {
        return Err(TimeframeError::SymbolTooLong);
    }
    Ok(Some(symbol.to_ascii_uppercase()))
}

fn normalize_provider(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn normalize_interval(value: &str) -> Result<String, TimeframeError> {
    let normalized = value
        .trim()
        .to_ascii_lowercase()
        .chars()
        .map(|character| match character {
            '_' | ' ' => '-',
            character => character,
        })
        .collect::<String>();
    match normalized.as_str() {
        "minute" | "1-minute" | "1-min" | "1m" => Ok("1m".to_owned()),
        "5-minute" | "5-minutes" | "5-min" | "five-minute" | "five-minutes" | "5m" => {
            Ok("5m".to_owned())
        }
        "hour" | "hourly" | "1-hour" | "1h" => Ok("1h".to_owned()),
        _ => Err(TimeframeError::InvalidInterval),
    }
}

fn parse_timestamp(
    field: &'static str,
    value: Option<&str>,
) -> Result<Option<DateTime<Utc>>, TimeframeError> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };

    DateTime::parse_from_rfc3339(value)
        .map(|timestamp| Some(timestamp.with_timezone(&Utc)))
        .map_err(|_| TimeframeError::InvalidTimestamp { field })
}

#[derive(Debug, Error)]
enum TimeframeError {
    #[error("instrument_id or symbol is required")]
    MissingInstrumentSelector,
    #[error("instrument_id must be a positive integer")]
    InvalidInstrumentId,
    #[error("symbol cannot exceed 64 characters")]
    SymbolTooLong,
    #[error("interval must be minute, 5-minute, hourly, 1m, 5m, or 1h")]
    InvalidInterval,
    #[error("{field} must be an RFC3339 timestamp")]
    InvalidTimestamp { field: &'static str },
    #[error("from cannot be later than to")]
    InvalidTimeRange,
    #[error("cached series not found for the requested timeframe")]
    SeriesNotFound,
    #[error("timeframe query failed: {0}")]
    Database(#[source] sqlx::Error),
}

impl TimeframeError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::MissingInstrumentSelector
            | Self::InvalidInstrumentId
            | Self::SymbolTooLong
            | Self::InvalidInterval
            | Self::InvalidTimestamp { .. }
            | Self::InvalidTimeRange => StatusCode::BAD_REQUEST,
            Self::SeriesNotFound => StatusCode::NOT_FOUND,
            Self::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn code(&self) -> &'static str {
        match self {
            Self::MissingInstrumentSelector => "missing_instrument_selector",
            Self::InvalidInstrumentId => "invalid_instrument_id",
            Self::SymbolTooLong => "symbol_too_long",
            Self::InvalidInterval => "invalid_interval",
            Self::InvalidTimestamp { .. } => "invalid_timestamp",
            Self::InvalidTimeRange => "invalid_time_range",
            Self::SeriesNotFound => "series_not_found",
            Self::Database(_) => "timeframe_query_failed",
        }
    }

    fn public_message(&self) -> String {
        match self {
            Self::Database(_) => "timeframe data is temporarily unavailable".to_owned(),
            _ => self.to_string(),
        }
    }
}

#[derive(Debug, Serialize)]
struct TimeframeErrorResponse {
    error: &'static str,
    message: String,
}

fn timeframe_error_response(
    error: TimeframeError,
) -> (StatusCode, Json<TimeframeErrorResponse>) {
    if matches!(error, TimeframeError::Database(_)) {
        tracing::error!(%error, "timeframe query failed");
    }

    (
        error.status_code(),
        Json(TimeframeErrorResponse {
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
        normalize_interval, PriceSeriesSummary, PriceSeriesSummaryRow, TimeframeParams,
        TimeframePoint, TimeframePointRow, ValidatedTimeframeQuery,
    };

    #[test]
    fn validates_symbol_timeframe_aliases_and_custom_range() {
        let params = TimeframeParams {
            instrument_id: None,
            symbol: Some(" spy ".to_owned()),
            provider: Some(" provider-a ".to_owned()),
            interval: Some("5-minute".to_owned()),
            timeframe: None,
            from: Some("2026-07-21T09:30:00Z".to_owned()),
            start: None,
            to: Some("2026-07-21T16:00:00Z".to_owned()),
            end: None,
            limit: Some(9_999),
        };

        let expected_from = Utc
            .with_ymd_and_hms(2026, 7, 21, 9, 30, 0)
            .single()
            .expect("valid timestamp");
        let expected_to = Utc
            .with_ymd_and_hms(2026, 7, 21, 16, 0, 0)
            .single()
            .expect("valid timestamp");

        assert_eq!(
            params.validate().expect("params should be valid"),
            ValidatedTimeframeQuery {
                instrument_id: None,
                symbol: Some("SPY".to_owned()),
                provider: Some("provider-a".to_owned()),
                interval: "5m".to_owned(),
                from: Some(expected_from),
                to: Some(expected_to),
                limit: 5_000,
            }
        );
    }

    #[test]
    fn validates_instrument_id_and_hourly_alias() {
        let params = TimeframeParams {
            instrument_id: Some(42),
            symbol: None,
            provider: None,
            interval: None,
            timeframe: Some("hourly".to_owned()),
            from: None,
            start: None,
            to: None,
            end: None,
            limit: None,
        };

        let query = params.validate().expect("params should be valid");
        assert_eq!(query.instrument_id, Some(42));
        assert_eq!(query.interval, "1h");
        assert_eq!(query.limit, 1_000);
    }

    #[test]
    fn rejects_missing_selector_and_invalid_ranges() {
        let missing_selector = TimeframeParams {
            instrument_id: None,
            symbol: None,
            provider: None,
            interval: None,
            timeframe: None,
            from: None,
            start: None,
            to: None,
            end: None,
            limit: None,
        };
        assert!(missing_selector.validate().is_err());

        let inverted_range = TimeframeParams {
            instrument_id: None,
            symbol: Some("SPY".to_owned()),
            provider: None,
            interval: Some("1m".to_owned()),
            timeframe: None,
            from: Some("2026-07-21T16:00:00Z".to_owned()),
            start: None,
            to: Some("2026-07-21T09:30:00Z".to_owned()),
            end: None,
            limit: None,
        };
        assert!(inverted_range.validate().is_err());
    }

    #[test]
    fn normalizes_supported_intervals() {
        assert_eq!(normalize_interval("minute").expect("minute"), "1m");
        assert_eq!(normalize_interval("5m").expect("5m"), "5m");
        assert_eq!(normalize_interval("1 hour").expect("hour"), "1h");
        assert!(normalize_interval("daily").is_err());
    }

    #[test]
    fn maps_series_and_point_rows_to_responses() {
        let observed_at = Utc
            .with_ymd_and_hms(2026, 7, 21, 14, 30, 0)
            .single()
            .expect("valid timestamp");
        let ingested_at = Utc
            .with_ymd_and_hms(2026, 7, 21, 14, 31, 0)
            .single()
            .expect("valid timestamp");
        let series = PriceSeriesSummary::from(PriceSeriesSummaryRow {
            id: 7,
            provider: "provider-a".to_owned(),
            provider_instrument_id: "provider-spy".to_owned(),
            symbol: "SPY".to_owned(),
            asset_class: "equity".to_owned(),
            interval: "1m".to_owned(),
            currency: Some("USD".to_owned()),
            first_observed_at: Some(observed_at),
            last_observed_at: Some(observed_at),
            last_refreshed_at: None,
            source_updated_at: None,
        });
        let point = TimeframePoint::from(TimeframePointRow {
            observed_at,
            open_price: Some(Decimal::new(50000, 2)),
            high_price: Some(Decimal::new(50500, 2)),
            low_price: Some(Decimal::new(49900, 2)),
            close_price: Decimal::new(50225, 2),
            volume: Some(Decimal::new(1500, 0)),
            trade_count: Some(15),
            vwap: Some(Decimal::new(50150, 2)),
            is_final: true,
            provider_updated_at: None,
            ingested_at,
        });

        assert_eq!(series.id, 7);
        assert_eq!(series.interval, "1m");
        assert_eq!(point.close_price, Decimal::new(50225, 2));
        assert_eq!(point.trade_count, Some(15));
    }
}
