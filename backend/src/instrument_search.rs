use std::collections::BTreeMap;

use axum::{
    extract::{Query, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use thiserror::Error;

use crate::state::AppState;

const DEFAULT_LIMIT: i64 = 20;
const MAX_LIMIT: i64 = 50;
const MIN_QUERY_LENGTH: usize = 1;
const MAX_QUERY_LENGTH: usize = 128;

pub fn router() -> Router<AppState> {
    Router::new().route("/instruments/search", get(search_instruments))
}

#[derive(Debug, Deserialize)]
struct InstrumentSearchParams {
    q: Option<String>,
    query: Option<String>,
    asset_class: Option<String>,
    region: Option<String>,
    issuer: Option<String>,
    limit: Option<i64>,
}

impl InstrumentSearchParams {
    fn validate(&self) -> Result<ValidatedInstrumentSearch, InstrumentSearchError> {
        let raw_query = self
            .q
            .as_deref()
            .or(self.query.as_deref())
            .unwrap_or_default()
            .trim();

        if raw_query.len() < MIN_QUERY_LENGTH {
            return Err(InstrumentSearchError::EmptyQuery);
        }
        if raw_query.len() > MAX_QUERY_LENGTH {
            return Err(InstrumentSearchError::QueryTooLong);
        }

        Ok(ValidatedInstrumentSearch {
            query: raw_query.to_owned(),
            pattern: like_contains(raw_query),
            prefix_pattern: like_prefix(raw_query),
            asset_class: normalize_asset_class(self.asset_class.as_deref())?,
            region: normalize_upper_optional(self.region.as_deref()),
            issuer_pattern: normalize_search_optional(self.issuer.as_deref()).map(like_contains),
            limit: self.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT),
        })
    }
}

#[derive(Debug, PartialEq, Eq)]
struct ValidatedInstrumentSearch {
    query: String,
    pattern: String,
    prefix_pattern: String,
    asset_class: Option<String>,
    region: Option<String>,
    issuer_pattern: Option<String>,
    limit: i64,
}

#[derive(Debug, Serialize)]
struct InstrumentSearchResponse {
    query: String,
    count: usize,
    results: Vec<InstrumentSearchResult>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
struct InstrumentSearchResult {
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
    identifiers: Vec<InstrumentIdentifierSummary>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct InstrumentIdentifierSummary {
    identifier_type: String,
    identifier_value: String,
    provider: Option<String>,
    is_primary: bool,
}

#[derive(Debug, FromRow)]
struct InstrumentSearchRow {
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

#[derive(Debug, FromRow)]
struct InstrumentIdentifierRow {
    instrument_id: i64,
    identifier_type: String,
    identifier_value: String,
    provider: Option<String>,
    is_primary: bool,
}

async fn search_instruments(
    State(state): State<AppState>,
    Query(params): Query<InstrumentSearchParams>,
) -> Result<Json<InstrumentSearchResponse>, (StatusCode, Json<SearchErrorResponse>)> {
    let search = params.validate().map_err(search_error_response)?;
    let rows = fetch_matching_instruments(&state, &search)
        .await
        .map_err(InstrumentSearchError::Database)
        .map_err(search_error_response)?;
    let identifiers = fetch_identifiers(&state, &rows)
        .await
        .map_err(InstrumentSearchError::Database)
        .map_err(search_error_response)?;
    let results = rows
        .into_iter()
        .map(|row| {
            let identifiers = identifiers.get(&row.id).cloned().unwrap_or_default();
            InstrumentSearchResult::from_row(row, identifiers)
        })
        .collect::<Vec<_>>();

    Ok(Json(InstrumentSearchResponse {
        query: search.query,
        count: results.len(),
        results,
    }))
}

async fn fetch_matching_instruments(
    state: &AppState,
    search: &ValidatedInstrumentSearch,
) -> Result<Vec<InstrumentSearchRow>, sqlx::Error> {
    sqlx::query_as::<_, InstrumentSearchRow>(
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
            i.updated_at
        FROM instruments i
        WHERE (
            i.canonical_symbol ILIKE $2 ESCAPE '\'
            OR i.display_name ILIKE $2 ESCAPE '\'
            OR COALESCE(i.issuer_name, '') ILIKE $2 ESCAPE '\'
            OR EXISTS (
                SELECT 1
                FROM instrument_identifiers ident
                WHERE ident.instrument_id = i.id
                    AND ident.identifier_value ILIKE $2 ESCAPE '\'
            )
        )
        AND ($4::TEXT IS NULL OR i.asset_class = $4)
        AND ($5::TEXT IS NULL OR i.region = $5)
        AND ($6::TEXT IS NULL OR COALESCE(i.issuer_name, '') ILIKE $6 ESCAPE '\')
        ORDER BY
            CASE
                WHEN upper(i.canonical_symbol) = upper($1) THEN 0
                WHEN upper(i.canonical_symbol) LIKE upper($3) ESCAPE '\' THEN 1
                WHEN i.display_name ILIKE $2 ESCAPE '\' THEN 2
                ELSE 3
            END,
            i.canonical_symbol ASC,
            i.display_name ASC
        LIMIT $7
        "#,
    )
    .bind(search.query.as_str())
    .bind(search.pattern.as_str())
    .bind(search.prefix_pattern.as_str())
    .bind(search.asset_class.as_deref())
    .bind(search.region.as_deref())
    .bind(search.issuer_pattern.as_deref())
    .bind(search.limit)
    .fetch_all(state.database().pool())
    .await
}

async fn fetch_identifiers(
    state: &AppState,
    rows: &[InstrumentSearchRow],
) -> Result<BTreeMap<i64, Vec<InstrumentIdentifierSummary>>, sqlx::Error> {
    let ids = rows.iter().map(|row| row.id).collect::<Vec<_>>();
    if ids.is_empty() {
        return Ok(BTreeMap::new());
    }

    let identifier_rows = sqlx::query_as::<_, InstrumentIdentifierRow>(
        r#"
        SELECT
            instrument_id,
            identifier_type,
            identifier_value,
            provider,
            is_primary
        FROM instrument_identifiers
        WHERE instrument_id = ANY($1)
        ORDER BY instrument_id ASC, is_primary DESC, identifier_type ASC, provider ASC NULLS LAST
        "#,
    )
    .bind(ids)
    .fetch_all(state.database().pool())
    .await?;

    Ok(group_identifiers(identifier_rows))
}

fn group_identifiers(
    identifier_rows: Vec<InstrumentIdentifierRow>,
) -> BTreeMap<i64, Vec<InstrumentIdentifierSummary>> {
    let mut grouped = BTreeMap::<i64, Vec<InstrumentIdentifierSummary>>::new();
    for row in identifier_rows {
        grouped
            .entry(row.instrument_id)
            .or_default()
            .push(InstrumentIdentifierSummary {
                identifier_type: row.identifier_type,
                identifier_value: row.identifier_value,
                provider: row.provider,
                is_primary: row.is_primary,
            });
    }
    grouped
}

impl InstrumentSearchResult {
    fn from_row(
        row: InstrumentSearchRow,
        identifiers: Vec<InstrumentIdentifierSummary>,
    ) -> Self {
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
            identifiers,
        }
    }
}

fn normalize_asset_class(value: Option<&str>) -> Result<Option<String>, InstrumentSearchError> {
    match normalize_optional(value).as_deref() {
        Some("equity") => Ok(Some("equity".to_owned())),
        Some("corporate_bond") => Ok(Some("corporate_bond".to_owned())),
        Some("government_bond") => Ok(Some("government_bond".to_owned())),
        Some(_) => Err(InstrumentSearchError::InvalidAssetClass),
        None => Ok(None),
    }
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

fn normalize_search_optional(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn like_contains(value: &str) -> String {
    format!("%{}%", escape_like(value))
}

fn like_prefix(value: &str) -> String {
    format!("{}%", escape_like(value))
}

fn escape_like(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        if matches!(character, '\\' | '%' | '_') {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    escaped
}

#[derive(Debug, Error)]
enum InstrumentSearchError {
    #[error("search query cannot be empty")]
    EmptyQuery,
    #[error("search query cannot exceed 128 characters")]
    QueryTooLong,
    #[error("asset_class must be equity, corporate_bond, or government_bond")]
    InvalidAssetClass,
    #[error("instrument search query failed: {0}")]
    Database(#[source] sqlx::Error),
}

impl InstrumentSearchError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::EmptyQuery | Self::QueryTooLong | Self::InvalidAssetClass => {
                StatusCode::BAD_REQUEST
            }
            Self::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn code(&self) -> &'static str {
        match self {
            Self::EmptyQuery => "empty_query",
            Self::QueryTooLong => "query_too_long",
            Self::InvalidAssetClass => "invalid_asset_class",
            Self::Database(_) => "instrument_search_failed",
        }
    }

    fn public_message(&self) -> String {
        match self {
            Self::Database(_) => "instrument search is temporarily unavailable".to_owned(),
            _ => self.to_string(),
        }
    }
}

#[derive(Debug, Serialize)]
struct SearchErrorResponse {
    error: &'static str,
    message: String,
}

fn search_error_response(error: InstrumentSearchError) -> (StatusCode, Json<SearchErrorResponse>) {
    if matches!(error, InstrumentSearchError::Database(_)) {
        tracing::error!(%error, "instrument search failed");
    }

    (
        error.status_code(),
        Json(SearchErrorResponse {
            error: error.code(),
            message: error.public_message(),
        }),
    )
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::{
        escape_like, group_identifiers, InstrumentIdentifierRow, InstrumentSearchParams,
        InstrumentSearchRow, InstrumentSearchResult, ValidatedInstrumentSearch,
    };

    #[test]
    fn validates_and_clamps_search_params() {
        let params = InstrumentSearchParams {
            q: Some(" spy ".to_owned()),
            query: None,
            asset_class: Some(" EQUITY ".to_owned()),
            region: Some(" us ".to_owned()),
            issuer: Some(" state ".to_owned()),
            limit: Some(500),
        };

        assert_eq!(
            params.validate().expect("params should be valid"),
            ValidatedInstrumentSearch {
                query: "spy".to_owned(),
                pattern: "%spy%".to_owned(),
                prefix_pattern: "spy%".to_owned(),
                asset_class: Some("equity".to_owned()),
                region: Some("US".to_owned()),
                issuer_pattern: Some("%state%".to_owned()),
                limit: 50,
            }
        );
    }

    #[test]
    fn escapes_like_wildcards() {
        assert_eq!(escape_like(r"SPY_%\"), r"SPY\_\%\\");
    }

    #[test]
    fn groups_identifier_rows_by_instrument() {
        let grouped = group_identifiers(vec![
            InstrumentIdentifierRow {
                instrument_id: 2,
                identifier_type: "isin".to_owned(),
                identifier_value: "US0000000001".to_owned(),
                provider: None,
                is_primary: false,
            },
            InstrumentIdentifierRow {
                instrument_id: 2,
                identifier_type: "provider_id".to_owned(),
                identifier_value: "provider-spy".to_owned(),
                provider: Some("http-json".to_owned()),
                is_primary: true,
            },
        ]);

        let identifiers = grouped.get(&2).expect("instrument identifiers");
        assert_eq!(identifiers.len(), 2);
        assert!(identifiers.iter().any(|identifier| identifier.is_primary));
    }

    #[test]
    fn maps_row_and_identifiers_to_result() {
        let Some(updated_at) = Utc.with_ymd_and_hms(2026, 7, 20, 12, 0, 0).single() else {
            panic!("valid timestamp");
        };
        let row = InstrumentSearchRow {
            id: 7,
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
        };

        let result = InstrumentSearchResult::from_row(row, Vec::new());

        assert_eq!(result.id, 7);
        assert_eq!(result.canonical_symbol, "SPY");
        assert!(result.identifiers.is_empty());
    }
}
