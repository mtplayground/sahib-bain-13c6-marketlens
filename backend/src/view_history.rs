use axum::{
    extract::{Extension, Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use thiserror::Error;

use crate::{auth::AuthenticatedSession, state::AppState};

const DEFAULT_LIMIT: i64 = 10;
const MAX_LIMIT: i64 = 25;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/view-history", post(record_view))
        .route("/view-history/most-viewed", get(most_viewed))
}

#[derive(Debug, Deserialize)]
struct RecordViewRequest {
    instrument_id: i64,
}

impl RecordViewRequest {
    fn validate(&self) -> Result<ValidatedRecordView, ViewHistoryError> {
        if self.instrument_id <= 0 {
            return Err(ViewHistoryError::InvalidInstrumentId);
        }

        Ok(ValidatedRecordView {
            instrument_id: self.instrument_id,
        })
    }
}

#[derive(Debug, PartialEq, Eq)]
struct ValidatedRecordView {
    instrument_id: i64,
}

#[derive(Debug, Deserialize)]
struct MostViewedParams {
    limit: Option<i64>,
}

impl MostViewedParams {
    fn validate(&self) -> ValidatedMostViewed {
        ValidatedMostViewed {
            limit: self.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
struct ValidatedMostViewed {
    limit: i64,
}

#[derive(Debug, Serialize)]
struct RecordViewResponse {
    status: &'static str,
    entry: ViewHistoryEntry,
}

#[derive(Debug, Serialize)]
struct MostViewedResponse {
    count: usize,
    results: Vec<ViewHistoryEntry>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
struct ViewHistoryEntry {
    instrument_id: i64,
    view_count: i64,
    first_viewed_at: DateTime<Utc>,
    last_viewed_at: DateTime<Utc>,
    instrument: ViewedInstrument,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
struct ViewedInstrument {
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
struct ViewHistoryRow {
    instrument_id: i64,
    view_count: i64,
    first_viewed_at: DateTime<Utc>,
    last_viewed_at: DateTime<Utc>,
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

async fn record_view(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthenticatedSession>,
    Json(request): Json<RecordViewRequest>,
) -> Result<Json<RecordViewResponse>, (StatusCode, Json<ViewHistoryErrorResponse>)> {
    let view = request.validate().map_err(view_history_error_response)?;
    let entry = record_instrument_view(&state, auth.user.sub.as_str(), view.instrument_id)
        .await
        .map_err(view_history_error_response)?;

    Ok(Json(RecordViewResponse {
        status: "recorded",
        entry,
    }))
}

async fn most_viewed(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthenticatedSession>,
    Query(params): Query<MostViewedParams>,
) -> Result<Json<MostViewedResponse>, (StatusCode, Json<ViewHistoryErrorResponse>)> {
    let query = params.validate();
    let results = fetch_most_viewed(&state, auth.user.sub.as_str(), query.limit)
        .await
        .map_err(ViewHistoryError::Database)
        .map_err(view_history_error_response)?;

    Ok(Json(MostViewedResponse {
        count: results.len(),
        results,
    }))
}

async fn record_instrument_view(
    state: &AppState,
    user_sub: &str,
    instrument_id: i64,
) -> Result<ViewHistoryEntry, ViewHistoryError> {
    let row = sqlx::query_as::<_, ViewHistoryRow>(
        r#"
        WITH recorded AS (
            INSERT INTO user_instrument_view_history (
                user_sub,
                instrument_id,
                view_count,
                first_viewed_at,
                last_viewed_at
            )
            SELECT $1, i.id, 1, NOW(), NOW()
            FROM instruments i
            WHERE i.id = $2
            ON CONFLICT (user_sub, instrument_id) DO UPDATE
            SET
                view_count = user_instrument_view_history.view_count + 1,
                last_viewed_at = NOW()
            RETURNING
                instrument_id,
                view_count,
                first_viewed_at,
                last_viewed_at
        )
        SELECT
            recorded.instrument_id,
            recorded.view_count,
            recorded.first_viewed_at,
            recorded.last_viewed_at,
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
        FROM recorded
        INNER JOIN instruments i ON i.id = recorded.instrument_id
        "#,
    )
    .bind(user_sub)
    .bind(instrument_id)
    .fetch_optional(state.database().pool())
    .await
    .map_err(ViewHistoryError::Database)?
    .ok_or(ViewHistoryError::InstrumentNotFound)?;

    Ok(ViewHistoryEntry::from(row))
}

async fn fetch_most_viewed(
    state: &AppState,
    user_sub: &str,
    limit: i64,
) -> Result<Vec<ViewHistoryEntry>, sqlx::Error> {
    let rows = sqlx::query_as::<_, ViewHistoryRow>(
        r#"
        SELECT
            history.instrument_id,
            history.view_count,
            history.first_viewed_at,
            history.last_viewed_at,
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
        FROM user_instrument_view_history history
        INNER JOIN instruments i ON i.id = history.instrument_id
        WHERE history.user_sub = $1
        ORDER BY
            history.view_count DESC,
            history.last_viewed_at DESC,
            i.canonical_symbol ASC
        LIMIT $2
        "#,
    )
    .bind(user_sub)
    .bind(limit)
    .fetch_all(state.database().pool())
    .await?;

    Ok(rows.into_iter().map(ViewHistoryEntry::from).collect())
}

impl From<ViewHistoryRow> for ViewHistoryEntry {
    fn from(row: ViewHistoryRow) -> Self {
        Self {
            instrument_id: row.instrument_id,
            view_count: row.view_count,
            first_viewed_at: row.first_viewed_at,
            last_viewed_at: row.last_viewed_at,
            instrument: ViewedInstrument {
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
            },
        }
    }
}

#[derive(Debug, Error)]
enum ViewHistoryError {
    #[error("instrument_id must be a positive integer")]
    InvalidInstrumentId,
    #[error("instrument was not found")]
    InstrumentNotFound,
    #[error("view-history query failed: {0}")]
    Database(#[source] sqlx::Error),
}

impl ViewHistoryError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::InvalidInstrumentId => StatusCode::BAD_REQUEST,
            Self::InstrumentNotFound => StatusCode::NOT_FOUND,
            Self::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn code(&self) -> &'static str {
        match self {
            Self::InvalidInstrumentId => "invalid_instrument_id",
            Self::InstrumentNotFound => "instrument_not_found",
            Self::Database(_) => "view_history_failed",
        }
    }

    fn public_message(&self) -> String {
        match self {
            Self::Database(_) => "view history is temporarily unavailable".to_owned(),
            _ => self.to_string(),
        }
    }
}

#[derive(Debug, Serialize)]
struct ViewHistoryErrorResponse {
    error: &'static str,
    message: String,
}

fn view_history_error_response(
    error: ViewHistoryError,
) -> (StatusCode, Json<ViewHistoryErrorResponse>) {
    if matches!(error, ViewHistoryError::Database(_)) {
        tracing::error!(%error, "view-history handler failed");
    }

    (
        error.status_code(),
        Json(ViewHistoryErrorResponse {
            error: error.code(),
            message: error.public_message(),
        }),
    )
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::{
        MostViewedParams, RecordViewRequest, ValidatedMostViewed, ValidatedRecordView,
        ViewHistoryEntry, ViewHistoryRow,
    };

    #[test]
    fn validates_record_view_request() {
        let request = RecordViewRequest { instrument_id: 42 };

        assert_eq!(
            request.validate().expect("request should be valid"),
            ValidatedRecordView { instrument_id: 42 }
        );
    }

    #[test]
    fn rejects_non_positive_instrument_id() {
        let request = RecordViewRequest { instrument_id: 0 };

        assert!(request.validate().is_err());
    }

    #[test]
    fn clamps_most_viewed_limit() {
        assert_eq!(
            (MostViewedParams { limit: Some(500) }).validate(),
            ValidatedMostViewed { limit: 25 }
        );
        assert_eq!(
            (MostViewedParams { limit: Some(-10) }).validate(),
            ValidatedMostViewed { limit: 1 }
        );
    }

    #[test]
    fn maps_history_row_to_entry() {
        let Some(first_viewed_at) = Utc.with_ymd_and_hms(2026, 7, 21, 0, 10, 0).single() else {
            panic!("valid timestamp");
        };
        let Some(last_viewed_at) = Utc.with_ymd_and_hms(2026, 7, 21, 0, 12, 0).single() else {
            panic!("valid timestamp");
        };
        let row = ViewHistoryRow {
            instrument_id: 9,
            view_count: 3,
            first_viewed_at,
            last_viewed_at,
            id: 9,
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
            updated_at: last_viewed_at,
        };

        let entry = ViewHistoryEntry::from(row);

        assert_eq!(entry.instrument_id, 9);
        assert_eq!(entry.view_count, 3);
        assert_eq!(entry.instrument.canonical_symbol, "SPY");
    }
}
