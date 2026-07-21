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

const DEFAULT_LIMIT: i64 = 10;
const MAX_LIMIT: i64 = 50;

pub fn router() -> Router<AppState> {
    Router::new().route("/instruments/popular", get(popular_instruments))
}

#[derive(Debug, Deserialize)]
struct PopularityParams {
    limit: Option<i64>,
}

impl PopularityParams {
    fn validate(&self) -> ValidatedPopularityQuery {
        ValidatedPopularityQuery {
            limit: self.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
struct ValidatedPopularityQuery {
    limit: i64,
}

#[derive(Debug, Serialize)]
struct PopularInstrumentsResponse {
    count: usize,
    refreshed_at: Option<DateTime<Utc>>,
    results: Vec<PopularInstrumentEntry>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
struct PopularInstrumentEntry {
    instrument_id: i64,
    platform_rank: i64,
    popularity_score: i64,
    total_views: i64,
    unique_viewers: i64,
    recent_views: i64,
    last_viewed_at: Option<DateTime<Utc>>,
    refreshed_at: DateTime<Utc>,
    instrument: PopularInstrument,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
struct PopularInstrument {
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
struct PopularInstrumentRow {
    instrument_id: i64,
    platform_rank: i64,
    popularity_score: i64,
    total_views: i64,
    unique_viewers: i64,
    recent_views: i64,
    last_viewed_at: Option<DateTime<Utc>>,
    refreshed_at: DateTime<Utc>,
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

async fn popular_instruments(
    State(state): State<AppState>,
    Query(params): Query<PopularityParams>,
) -> Result<Json<PopularInstrumentsResponse>, (StatusCode, Json<PopularityErrorResponse>)> {
    let query = params.validate();
    refresh_popularity(&state)
        .await
        .map_err(PopularityError::Database)
        .map_err(popularity_error_response)?;
    let results = fetch_popular_instruments(&state, query.limit)
        .await
        .map_err(PopularityError::Database)
        .map_err(popularity_error_response)?;
    let refreshed_at = results.first().map(|entry| entry.refreshed_at);

    Ok(Json(PopularInstrumentsResponse {
        count: results.len(),
        refreshed_at,
        results,
    }))
}

async fn refresh_popularity(state: &AppState) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        WITH aggregated AS (
            SELECT
                history.instrument_id,
                SUM(history.view_count)::BIGINT AS total_views,
                COUNT(*)::BIGINT AS unique_viewers,
                COALESCE(
                    SUM(history.view_count) FILTER (
                        WHERE history.last_viewed_at >= NOW() - INTERVAL '7 days'
                    ),
                    0
                )::BIGINT AS recent_views,
                MAX(history.last_viewed_at) AS last_viewed_at
            FROM user_instrument_view_history history
            GROUP BY history.instrument_id
        ),
        scored AS (
            SELECT
                instrument_id,
                total_views,
                unique_viewers,
                recent_views,
                last_viewed_at,
                (
                    total_views
                    + (unique_viewers * 3)
                    + (recent_views * 2)
                )::BIGINT AS popularity_score
            FROM aggregated
        ),
        ranked AS (
            SELECT
                instrument_id,
                total_views,
                unique_viewers,
                recent_views,
                popularity_score,
                RANK() OVER (
                    ORDER BY
                        popularity_score DESC,
                        total_views DESC,
                        unique_viewers DESC,
                        last_viewed_at DESC,
                        instrument_id ASC
                )::BIGINT AS platform_rank,
                last_viewed_at
            FROM scored
        )
        INSERT INTO instrument_popularity (
            instrument_id,
            total_views,
            unique_viewers,
            recent_views,
            popularity_score,
            platform_rank,
            last_viewed_at,
            refreshed_at
        )
        SELECT
            instrument_id,
            total_views,
            unique_viewers,
            recent_views,
            popularity_score,
            platform_rank,
            last_viewed_at,
            NOW()
        FROM ranked
        ON CONFLICT (instrument_id) DO UPDATE
        SET
            total_views = EXCLUDED.total_views,
            unique_viewers = EXCLUDED.unique_viewers,
            recent_views = EXCLUDED.recent_views,
            popularity_score = EXCLUDED.popularity_score,
            platform_rank = EXCLUDED.platform_rank,
            last_viewed_at = EXCLUDED.last_viewed_at,
            refreshed_at = NOW()
        "#,
    )
    .execute(state.database().pool())
    .await?;

    sqlx::query(
        r#"
        DELETE FROM instrument_popularity popularity
        WHERE NOT EXISTS (
            SELECT 1
            FROM user_instrument_view_history history
            WHERE history.instrument_id = popularity.instrument_id
        )
        "#,
    )
    .execute(state.database().pool())
    .await?;

    Ok(())
}

async fn fetch_popular_instruments(
    state: &AppState,
    limit: i64,
) -> Result<Vec<PopularInstrumentEntry>, sqlx::Error> {
    let rows = sqlx::query_as::<_, PopularInstrumentRow>(
        r#"
        SELECT
            popularity.instrument_id,
            popularity.platform_rank,
            popularity.popularity_score,
            popularity.total_views,
            popularity.unique_viewers,
            popularity.recent_views,
            popularity.last_viewed_at,
            popularity.refreshed_at,
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
        FROM instrument_popularity popularity
        INNER JOIN instruments i ON i.id = popularity.instrument_id
        ORDER BY
            popularity.platform_rank ASC,
            popularity.popularity_score DESC,
            i.canonical_symbol ASC
        LIMIT $1
        "#,
    )
    .bind(limit)
    .fetch_all(state.database().pool())
    .await?;

    Ok(rows.into_iter().map(PopularInstrumentEntry::from).collect())
}

impl From<PopularInstrumentRow> for PopularInstrumentEntry {
    fn from(row: PopularInstrumentRow) -> Self {
        Self {
            instrument_id: row.instrument_id,
            platform_rank: row.platform_rank,
            popularity_score: row.popularity_score,
            total_views: row.total_views,
            unique_viewers: row.unique_viewers,
            recent_views: row.recent_views,
            last_viewed_at: row.last_viewed_at,
            refreshed_at: row.refreshed_at,
            instrument: PopularInstrument {
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
enum PopularityError {
    #[error("popularity query failed: {0}")]
    Database(#[source] sqlx::Error),
}

impl PopularityError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn code(&self) -> &'static str {
        match self {
            Self::Database(_) => "popularity_failed",
        }
    }

    fn public_message(&self) -> String {
        match self {
            Self::Database(_) => "popular instruments are temporarily unavailable".to_owned(),
        }
    }
}

#[derive(Debug, Serialize)]
struct PopularityErrorResponse {
    error: &'static str,
    message: String,
}

fn popularity_error_response(
    error: PopularityError,
) -> (StatusCode, Json<PopularityErrorResponse>) {
    tracing::error!(%error, "popularity handler failed");

    (
        error.status_code(),
        Json(PopularityErrorResponse {
            error: error.code(),
            message: error.public_message(),
        }),
    )
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::{
        PopularInstrumentEntry, PopularInstrumentRow, PopularityParams,
        ValidatedPopularityQuery,
    };

    #[test]
    fn clamps_popularity_limit() {
        assert_eq!(
            (PopularityParams { limit: Some(500) }).validate(),
            ValidatedPopularityQuery { limit: 50 }
        );
        assert_eq!(
            (PopularityParams { limit: Some(-4) }).validate(),
            ValidatedPopularityQuery { limit: 1 }
        );
    }

    #[test]
    fn maps_popularity_row_to_entry() {
        let Some(refreshed_at) = Utc.with_ymd_and_hms(2026, 7, 21, 0, 20, 0).single() else {
            panic!("valid timestamp");
        };
        let Some(last_viewed_at) = Utc.with_ymd_and_hms(2026, 7, 21, 0, 19, 0).single() else {
            panic!("valid timestamp");
        };
        let row = PopularInstrumentRow {
            instrument_id: 7,
            platform_rank: 1,
            popularity_score: 41,
            total_views: 20,
            unique_viewers: 3,
            recent_views: 6,
            last_viewed_at: Some(last_viewed_at),
            refreshed_at,
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
            updated_at: refreshed_at,
        };

        let entry = PopularInstrumentEntry::from(row);

        assert_eq!(entry.platform_rank, 1);
        assert_eq!(entry.popularity_score, 41);
        assert_eq!(entry.instrument.canonical_symbol, "SPY");
    }
}
