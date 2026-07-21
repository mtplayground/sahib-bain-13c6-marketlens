use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    routing::{delete, get, patch, post},
    Json, Router,
};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use thiserror::Error;

use crate::{auth::AuthenticatedSession, state::AppState};

const MAX_WATCHLIST_NAME_LENGTH: usize = 80;
const MAX_ITEM_NOTES_LENGTH: usize = 500;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/watchlists", get(list_watchlists).post(create_watchlist))
        .route(
            "/watchlists/:watchlist_id",
            get(get_watchlist).patch(update_watchlist).delete(delete_watchlist),
        )
        .route("/watchlists/:watchlist_id/items", post(add_watchlist_item))
        .route(
            "/watchlists/:watchlist_id/items/:instrument_id",
            delete(remove_watchlist_item),
        )
}

#[derive(Debug, Deserialize)]
struct WatchlistPath {
    watchlist_id: i64,
}

impl WatchlistPath {
    fn validate(&self) -> Result<ValidatedWatchlistPath, WatchlistError> {
        if self.watchlist_id <= 0 {
            return Err(WatchlistError::InvalidWatchlistId);
        }

        Ok(ValidatedWatchlistPath {
            watchlist_id: self.watchlist_id,
        })
    }
}

#[derive(Debug, Deserialize)]
struct WatchlistItemPath {
    watchlist_id: i64,
    instrument_id: i64,
}

impl WatchlistItemPath {
    fn validate(&self) -> Result<ValidatedWatchlistItemPath, WatchlistError> {
        if self.watchlist_id <= 0 {
            return Err(WatchlistError::InvalidWatchlistId);
        }
        if self.instrument_id <= 0 {
            return Err(WatchlistError::InvalidInstrumentId);
        }

        Ok(ValidatedWatchlistItemPath {
            watchlist_id: self.watchlist_id,
            instrument_id: self.instrument_id,
        })
    }
}

#[derive(Debug, PartialEq, Eq)]
struct ValidatedWatchlistPath {
    watchlist_id: i64,
}

#[derive(Debug, PartialEq, Eq)]
struct ValidatedWatchlistItemPath {
    watchlist_id: i64,
    instrument_id: i64,
}

#[derive(Debug, Deserialize)]
struct CreateWatchlistRequest {
    name: String,
}

impl CreateWatchlistRequest {
    fn validate(&self) -> Result<ValidatedWatchlistName, WatchlistError> {
        ValidatedWatchlistName::new(self.name.as_str())
    }
}

#[derive(Debug, Deserialize)]
struct UpdateWatchlistRequest {
    name: String,
}

impl UpdateWatchlistRequest {
    fn validate(&self) -> Result<ValidatedWatchlistName, WatchlistError> {
        ValidatedWatchlistName::new(self.name.as_str())
    }
}

#[derive(Debug, Deserialize)]
struct AddWatchlistItemRequest {
    instrument_id: i64,
    notes: Option<String>,
}

impl AddWatchlistItemRequest {
    fn validate(&self) -> Result<ValidatedAddWatchlistItem, WatchlistError> {
        if self.instrument_id <= 0 {
            return Err(WatchlistError::InvalidInstrumentId);
        }

        Ok(ValidatedAddWatchlistItem {
            instrument_id: self.instrument_id,
            notes: normalize_notes(self.notes.as_deref())?,
        })
    }
}

#[derive(Debug, PartialEq, Eq)]
struct ValidatedWatchlistName {
    name: String,
}

impl ValidatedWatchlistName {
    fn new(value: &str) -> Result<Self, WatchlistError> {
        let name = value.trim();
        if name.is_empty() {
            return Err(WatchlistError::InvalidWatchlistName);
        }
        if name.chars().count() > MAX_WATCHLIST_NAME_LENGTH {
            return Err(WatchlistError::InvalidWatchlistName);
        }

        Ok(Self {
            name: name.to_owned(),
        })
    }
}

#[derive(Debug, PartialEq, Eq)]
struct ValidatedAddWatchlistItem {
    instrument_id: i64,
    notes: Option<String>,
}

#[derive(Debug, Serialize)]
struct WatchlistsResponse {
    count: usize,
    results: Vec<Watchlist>,
}

#[derive(Debug, Serialize)]
struct WatchlistResponse {
    watchlist: Watchlist,
}

#[derive(Debug, Serialize)]
struct DeleteWatchlistResponse {
    status: &'static str,
    watchlist_id: i64,
}

#[derive(Debug, Serialize)]
struct DeleteWatchlistItemResponse {
    status: &'static str,
    watchlist_id: i64,
    instrument_id: i64,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
struct Watchlist {
    id: i64,
    name: String,
    item_count: i64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    items: Vec<WatchlistItem>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
struct WatchlistItem {
    instrument_id: i64,
    position: i64,
    notes: Option<String>,
    added_at: DateTime<Utc>,
    instrument: WatchlistInstrument,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct WatchlistInstrument {
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
struct WatchlistRow {
    id: i64,
    name: String,
    item_count: i64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow)]
struct WatchlistItemRow {
    watchlist_id: i64,
    instrument_id: i64,
    position: i64,
    notes: Option<String>,
    added_at: DateTime<Utc>,
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

async fn list_watchlists(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthenticatedSession>,
) -> Result<Json<WatchlistsResponse>, (StatusCode, Json<WatchlistErrorResponse>)> {
    let results = fetch_watchlists(&state, auth.user.sub.as_str())
        .await
        .map_err(watchlist_error_response)?;

    Ok(Json(WatchlistsResponse {
        count: results.len(),
        results,
    }))
}

async fn get_watchlist(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthenticatedSession>,
    Path(path): Path<WatchlistPath>,
) -> Result<Json<WatchlistResponse>, (StatusCode, Json<WatchlistErrorResponse>)> {
    let path = path.validate().map_err(watchlist_error_response)?;
    let watchlist = fetch_watchlist(&state, auth.user.sub.as_str(), path.watchlist_id)
        .await
        .map_err(watchlist_error_response)?;

    Ok(Json(WatchlistResponse { watchlist }))
}

async fn create_watchlist(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthenticatedSession>,
    Json(request): Json<CreateWatchlistRequest>,
) -> Result<(StatusCode, Json<WatchlistResponse>), (StatusCode, Json<WatchlistErrorResponse>)> {
    let validated = request.validate().map_err(watchlist_error_response)?;
    let row = sqlx::query_as::<_, WatchlistRow>(
        r#"
        INSERT INTO user_watchlists (user_sub, name)
        VALUES ($1, $2)
        RETURNING
            id,
            name,
            0::BIGINT AS item_count,
            created_at,
            updated_at
        "#,
    )
    .bind(auth.user.sub.as_str())
    .bind(validated.name.as_str())
    .fetch_one(state.database().pool())
    .await
    .map_err(map_database_error)
    .map_err(watchlist_error_response)?;

    Ok((
        StatusCode::CREATED,
        Json(WatchlistResponse {
            watchlist: Watchlist::from_row(row, Vec::new()),
        }),
    ))
}

async fn update_watchlist(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthenticatedSession>,
    Path(path): Path<WatchlistPath>,
    Json(request): Json<UpdateWatchlistRequest>,
) -> Result<Json<WatchlistResponse>, (StatusCode, Json<WatchlistErrorResponse>)> {
    let path = path.validate().map_err(watchlist_error_response)?;
    let validated = request.validate().map_err(watchlist_error_response)?;
    let row = sqlx::query_as::<_, WatchlistRow>(
        r#"
        UPDATE user_watchlists watchlist
        SET name = $3
        WHERE watchlist.user_sub = $1
            AND watchlist.id = $2
        RETURNING
            watchlist.id,
            watchlist.name,
            (
                SELECT COUNT(*)::BIGINT
                FROM user_watchlist_items item
                WHERE item.watchlist_id = watchlist.id
            ) AS item_count,
            watchlist.created_at,
            watchlist.updated_at
        "#,
    )
    .bind(auth.user.sub.as_str())
    .bind(path.watchlist_id)
    .bind(validated.name.as_str())
    .fetch_optional(state.database().pool())
    .await
    .map_err(map_database_error)
    .map_err(watchlist_error_response)?
    .ok_or(WatchlistError::WatchlistNotFound)
    .map_err(watchlist_error_response)?;
    let items = fetch_watchlist_items(&state, path.watchlist_id)
        .await
        .map_err(watchlist_error_response)?;

    Ok(Json(WatchlistResponse {
        watchlist: Watchlist::from_row(row, items),
    }))
}

async fn delete_watchlist(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthenticatedSession>,
    Path(path): Path<WatchlistPath>,
) -> Result<Json<DeleteWatchlistResponse>, (StatusCode, Json<WatchlistErrorResponse>)> {
    let path = path.validate().map_err(watchlist_error_response)?;
    let result = sqlx::query(
        r#"
        DELETE FROM user_watchlists
        WHERE user_sub = $1
            AND id = $2
        "#,
    )
    .bind(auth.user.sub.as_str())
    .bind(path.watchlist_id)
    .execute(state.database().pool())
    .await
    .map_err(WatchlistError::Database)
    .map_err(watchlist_error_response)?;

    if result.rows_affected() == 0 {
        return Err(watchlist_error_response(WatchlistError::WatchlistNotFound));
    }

    Ok(Json(DeleteWatchlistResponse {
        status: "deleted",
        watchlist_id: path.watchlist_id,
    }))
}

async fn add_watchlist_item(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthenticatedSession>,
    Path(path): Path<WatchlistPath>,
    Json(request): Json<AddWatchlistItemRequest>,
) -> Result<(StatusCode, Json<WatchlistResponse>), (StatusCode, Json<WatchlistErrorResponse>)> {
    let path = path.validate().map_err(watchlist_error_response)?;
    let item = request.validate().map_err(watchlist_error_response)?;
    ensure_watchlist_owner(&state, auth.user.sub.as_str(), path.watchlist_id).await?;
    ensure_instrument_exists(&state, item.instrument_id).await?;

    let inserted = sqlx::query(
        r#"
        INSERT INTO user_watchlist_items (
            watchlist_id,
            instrument_id,
            position,
            notes
        )
        VALUES (
            $1,
            $2,
            COALESCE(
                (
                    SELECT MAX(position) + 1
                    FROM user_watchlist_items
                    WHERE watchlist_id = $1
                ),
                0
            ),
            $3
        )
        ON CONFLICT (watchlist_id, instrument_id) DO NOTHING
        "#,
    )
    .bind(path.watchlist_id)
    .bind(item.instrument_id)
    .bind(item.notes.as_deref())
    .execute(state.database().pool())
    .await
    .map_err(WatchlistError::Database)
    .map_err(watchlist_error_response)?;

    if inserted.rows_affected() == 0 {
        return Err(watchlist_error_response(WatchlistError::DuplicateWatchlistItem));
    }

    touch_watchlist(&state, path.watchlist_id).await?;
    let watchlist = fetch_watchlist(&state, auth.user.sub.as_str(), path.watchlist_id)
        .await
        .map_err(watchlist_error_response)?;

    Ok((StatusCode::CREATED, Json(WatchlistResponse { watchlist })))
}

async fn remove_watchlist_item(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthenticatedSession>,
    Path(path): Path<WatchlistItemPath>,
) -> Result<Json<DeleteWatchlistItemResponse>, (StatusCode, Json<WatchlistErrorResponse>)> {
    let path = path.validate().map_err(watchlist_error_response)?;
    ensure_watchlist_owner(&state, auth.user.sub.as_str(), path.watchlist_id).await?;

    let result = sqlx::query(
        r#"
        DELETE FROM user_watchlist_items
        WHERE watchlist_id = $1
            AND instrument_id = $2
        "#,
    )
    .bind(path.watchlist_id)
    .bind(path.instrument_id)
    .execute(state.database().pool())
    .await
    .map_err(WatchlistError::Database)
    .map_err(watchlist_error_response)?;

    if result.rows_affected() == 0 {
        return Err(watchlist_error_response(WatchlistError::WatchlistItemNotFound));
    }

    touch_watchlist(&state, path.watchlist_id).await?;

    Ok(Json(DeleteWatchlistItemResponse {
        status: "deleted",
        watchlist_id: path.watchlist_id,
        instrument_id: path.instrument_id,
    }))
}

async fn fetch_watchlists(
    state: &AppState,
    user_sub: &str,
) -> Result<Vec<Watchlist>, WatchlistError> {
    let rows = sqlx::query_as::<_, WatchlistRow>(
        r#"
        SELECT
            watchlist.id,
            watchlist.name,
            COUNT(item.instrument_id)::BIGINT AS item_count,
            watchlist.created_at,
            watchlist.updated_at
        FROM user_watchlists watchlist
        LEFT JOIN user_watchlist_items item ON item.watchlist_id = watchlist.id
        WHERE watchlist.user_sub = $1
        GROUP BY watchlist.id
        ORDER BY watchlist.updated_at DESC, watchlist.id DESC
        "#,
    )
    .bind(user_sub)
    .fetch_all(state.database().pool())
    .await
    .map_err(WatchlistError::Database)?;
    let items = fetch_user_watchlist_items(state, user_sub).await?;

    Ok(rows
        .into_iter()
        .map(|row| {
            let row_items = items
                .iter()
                .filter(|item| item.watchlist_id == row.id)
                .cloned()
                .collect::<Vec<_>>();
            Watchlist::from_row(row, row_items)
        })
        .collect())
}

async fn fetch_watchlist(
    state: &AppState,
    user_sub: &str,
    watchlist_id: i64,
) -> Result<Watchlist, WatchlistError> {
    let row = sqlx::query_as::<_, WatchlistRow>(
        r#"
        SELECT
            watchlist.id,
            watchlist.name,
            COUNT(item.instrument_id)::BIGINT AS item_count,
            watchlist.created_at,
            watchlist.updated_at
        FROM user_watchlists watchlist
        LEFT JOIN user_watchlist_items item ON item.watchlist_id = watchlist.id
        WHERE watchlist.user_sub = $1
            AND watchlist.id = $2
        GROUP BY watchlist.id
        "#,
    )
    .bind(user_sub)
    .bind(watchlist_id)
    .fetch_optional(state.database().pool())
    .await
    .map_err(WatchlistError::Database)?
    .ok_or(WatchlistError::WatchlistNotFound)?;
    let items = fetch_watchlist_items(state, watchlist_id).await?;

    Ok(Watchlist::from_row(row, items))
}

async fn fetch_user_watchlist_items(
    state: &AppState,
    user_sub: &str,
) -> Result<Vec<WatchlistItemRow>, WatchlistError> {
    let rows = sqlx::query_as::<_, WatchlistItemRow>(
        r#"
        SELECT
            item.watchlist_id,
            item.instrument_id,
            item.position,
            item.notes,
            item.added_at,
            instrument.id,
            instrument.canonical_symbol,
            instrument.display_name,
            instrument.asset_class,
            instrument.region,
            instrument.country,
            instrument.currency,
            instrument.exchange,
            instrument.issuer_name,
            instrument.issuer_region,
            instrument.maturity_date,
            instrument.status,
            instrument.updated_at
        FROM user_watchlist_items item
        INNER JOIN user_watchlists watchlist ON watchlist.id = item.watchlist_id
        INNER JOIN instruments instrument ON instrument.id = item.instrument_id
        WHERE watchlist.user_sub = $1
        ORDER BY item.watchlist_id ASC, item.position ASC, item.added_at ASC, item.instrument_id ASC
        "#,
    )
    .bind(user_sub)
    .fetch_all(state.database().pool())
    .await
    .map_err(WatchlistError::Database)?;

    Ok(rows)
}

async fn fetch_watchlist_items(
    state: &AppState,
    watchlist_id: i64,
) -> Result<Vec<WatchlistItemRow>, WatchlistError> {
    let rows = sqlx::query_as::<_, WatchlistItemRow>(
        r#"
        SELECT
            item.watchlist_id,
            item.instrument_id,
            item.position,
            item.notes,
            item.added_at,
            instrument.id,
            instrument.canonical_symbol,
            instrument.display_name,
            instrument.asset_class,
            instrument.region,
            instrument.country,
            instrument.currency,
            instrument.exchange,
            instrument.issuer_name,
            instrument.issuer_region,
            instrument.maturity_date,
            instrument.status,
            instrument.updated_at
        FROM user_watchlist_items item
        INNER JOIN instruments instrument ON instrument.id = item.instrument_id
        WHERE item.watchlist_id = $1
        ORDER BY item.position ASC, item.added_at ASC, item.instrument_id ASC
        "#,
    )
    .bind(watchlist_id)
    .fetch_all(state.database().pool())
    .await
    .map_err(WatchlistError::Database)?;

    Ok(rows)
}

async fn ensure_watchlist_owner(
    state: &AppState,
    user_sub: &str,
    watchlist_id: i64,
) -> Result<(), (StatusCode, Json<WatchlistErrorResponse>)> {
    let exists = sqlx::query_scalar::<_, bool>(
        r#"
        SELECT EXISTS (
            SELECT 1
            FROM user_watchlists
            WHERE user_sub = $1
                AND id = $2
        )
        "#,
    )
    .bind(user_sub)
    .bind(watchlist_id)
    .fetch_one(state.database().pool())
    .await
    .map_err(WatchlistError::Database)
    .map_err(watchlist_error_response)?;

    if exists {
        Ok(())
    } else {
        Err(watchlist_error_response(WatchlistError::WatchlistNotFound))
    }
}

async fn ensure_instrument_exists(
    state: &AppState,
    instrument_id: i64,
) -> Result<(), (StatusCode, Json<WatchlistErrorResponse>)> {
    let exists = sqlx::query_scalar::<_, bool>(
        r#"
        SELECT EXISTS (
            SELECT 1
            FROM instruments
            WHERE id = $1
        )
        "#,
    )
    .bind(instrument_id)
    .fetch_one(state.database().pool())
    .await
    .map_err(WatchlistError::Database)
    .map_err(watchlist_error_response)?;

    if exists {
        Ok(())
    } else {
        Err(watchlist_error_response(WatchlistError::InstrumentNotFound))
    }
}

async fn touch_watchlist(
    state: &AppState,
    watchlist_id: i64,
) -> Result<(), (StatusCode, Json<WatchlistErrorResponse>)> {
    sqlx::query(
        r#"
        UPDATE user_watchlists
        SET updated_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(watchlist_id)
    .execute(state.database().pool())
    .await
    .map_err(WatchlistError::Database)
    .map_err(watchlist_error_response)?;

    Ok(())
}

impl Watchlist {
    fn from_row(row: WatchlistRow, items: Vec<WatchlistItemRow>) -> Self {
        Self {
            id: row.id,
            name: row.name,
            item_count: row.item_count,
            created_at: row.created_at,
            updated_at: row.updated_at,
            items: items.into_iter().map(WatchlistItem::from).collect(),
        }
    }
}

impl From<WatchlistItemRow> for WatchlistItem {
    fn from(row: WatchlistItemRow) -> Self {
        Self {
            instrument_id: row.instrument_id,
            position: row.position,
            notes: row.notes,
            added_at: row.added_at,
            instrument: WatchlistInstrument {
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

fn normalize_notes(value: Option<&str>) -> Result<Option<String>, WatchlistError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let notes = value.trim();
    if notes.is_empty() {
        return Ok(None);
    }
    if notes.chars().count() > MAX_ITEM_NOTES_LENGTH {
        return Err(WatchlistError::InvalidWatchlistItemNotes);
    }

    Ok(Some(notes.to_owned()))
}

fn map_database_error(error: sqlx::Error) -> WatchlistError {
    if let sqlx::Error::Database(database_error) = &error {
        if database_error
            .constraint()
            .is_some_and(|constraint| constraint == "user_watchlists_user_name_unique_idx")
        {
            return WatchlistError::DuplicateWatchlistName;
        }
    }

    WatchlistError::Database(error)
}

#[derive(Debug, Error)]
enum WatchlistError {
    #[error("watchlist_id must be a positive integer")]
    InvalidWatchlistId,
    #[error("instrument_id must be a positive integer")]
    InvalidInstrumentId,
    #[error("watchlist name must be 1 to 80 characters")]
    InvalidWatchlistName,
    #[error("watchlist item notes must be 500 characters or fewer")]
    InvalidWatchlistItemNotes,
    #[error("watchlist was not found")]
    WatchlistNotFound,
    #[error("instrument was not found")]
    InstrumentNotFound,
    #[error("watchlist item was not found")]
    WatchlistItemNotFound,
    #[error("watchlist name already exists")]
    DuplicateWatchlistName,
    #[error("instrument is already in this watchlist")]
    DuplicateWatchlistItem,
    #[error("watchlist query failed: {0}")]
    Database(#[source] sqlx::Error),
}

impl WatchlistError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::InvalidWatchlistId
            | Self::InvalidInstrumentId
            | Self::InvalidWatchlistName
            | Self::InvalidWatchlistItemNotes => StatusCode::BAD_REQUEST,
            Self::WatchlistNotFound | Self::InstrumentNotFound | Self::WatchlistItemNotFound => {
                StatusCode::NOT_FOUND
            }
            Self::DuplicateWatchlistName | Self::DuplicateWatchlistItem => StatusCode::CONFLICT,
            Self::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn code(&self) -> &'static str {
        match self {
            Self::InvalidWatchlistId => "invalid_watchlist_id",
            Self::InvalidInstrumentId => "invalid_instrument_id",
            Self::InvalidWatchlistName => "invalid_watchlist_name",
            Self::InvalidWatchlistItemNotes => "invalid_watchlist_item_notes",
            Self::WatchlistNotFound => "watchlist_not_found",
            Self::InstrumentNotFound => "instrument_not_found",
            Self::WatchlistItemNotFound => "watchlist_item_not_found",
            Self::DuplicateWatchlistName => "duplicate_watchlist_name",
            Self::DuplicateWatchlistItem => "duplicate_watchlist_item",
            Self::Database(_) => "watchlist_failed",
        }
    }

    fn public_message(&self) -> String {
        match self {
            Self::Database(_) => "watchlists are temporarily unavailable".to_owned(),
            _ => self.to_string(),
        }
    }
}

#[derive(Debug, Serialize)]
struct WatchlistErrorResponse {
    error: &'static str,
    message: String,
}

fn watchlist_error_response(error: WatchlistError) -> (StatusCode, Json<WatchlistErrorResponse>) {
    if matches!(error, WatchlistError::Database(_)) {
        tracing::error!(%error, "watchlist handler failed");
    }

    (
        error.status_code(),
        Json(WatchlistErrorResponse {
            error: error.code(),
            message: error.public_message(),
        }),
    )
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::{
        AddWatchlistItemRequest, CreateWatchlistRequest, UpdateWatchlistRequest,
        ValidatedAddWatchlistItem, ValidatedWatchlistName, Watchlist, WatchlistItem,
        WatchlistItemPath, WatchlistItemRow, WatchlistPath, WatchlistRow,
    };

    #[test]
    fn validates_watchlist_name() {
        let request = CreateWatchlistRequest {
            name: "  Macro book  ".to_owned(),
        };

        assert_eq!(
            request.validate().expect("name should be valid"),
            ValidatedWatchlistName {
                name: "Macro book".to_owned()
            }
        );
    }

    #[test]
    fn rejects_invalid_watchlist_name() {
        assert!(CreateWatchlistRequest {
            name: " ".to_owned()
        }
        .validate()
        .is_err());
        assert!(UpdateWatchlistRequest {
            name: "x".repeat(81)
        }
        .validate()
        .is_err());
    }

    #[test]
    fn validates_watchlist_paths() {
        assert!(WatchlistPath { watchlist_id: 1 }.validate().is_ok());
        assert!(WatchlistPath { watchlist_id: 0 }.validate().is_err());
        assert!(WatchlistItemPath {
            watchlist_id: 1,
            instrument_id: 2
        }
        .validate()
        .is_ok());
        assert!(WatchlistItemPath {
            watchlist_id: 1,
            instrument_id: -2
        }
        .validate()
        .is_err());
    }

    #[test]
    fn validates_add_item_request() {
        let request = AddWatchlistItemRequest {
            instrument_id: 42,
            notes: Some("  rate hedge  ".to_owned()),
        };

        assert_eq!(
            request.validate().expect("item should be valid"),
            ValidatedAddWatchlistItem {
                instrument_id: 42,
                notes: Some("rate hedge".to_owned())
            }
        );
        assert!(AddWatchlistItemRequest {
            instrument_id: 0,
            notes: None,
        }
        .validate()
        .is_err());
        assert!(AddWatchlistItemRequest {
            instrument_id: 1,
            notes: Some("x".repeat(501)),
        }
        .validate()
        .is_err());
    }

    #[test]
    fn maps_rows_to_watchlist() {
        let Some(timestamp) = Utc.with_ymd_and_hms(2026, 7, 21, 0, 33, 0).single() else {
            panic!("valid timestamp");
        };
        let row = WatchlistRow {
            id: 7,
            name: "Rates".to_owned(),
            item_count: 1,
            created_at: timestamp,
            updated_at: timestamp,
        };
        let item = WatchlistItemRow {
            watchlist_id: 7,
            instrument_id: 9,
            position: 0,
            notes: Some("benchmark".to_owned()),
            added_at: timestamp,
            id: 9,
            canonical_symbol: "US10Y".to_owned(),
            display_name: "US 10Y Treasury".to_owned(),
            asset_class: "government_bond".to_owned(),
            region: "US".to_owned(),
            country: Some("US".to_owned()),
            currency: Some("USD".to_owned()),
            exchange: None,
            issuer_name: Some("US Treasury".to_owned()),
            issuer_region: Some("US".to_owned()),
            maturity_date: None,
            status: "active".to_owned(),
            updated_at: timestamp,
        };

        let watchlist = Watchlist::from_row(row, vec![item]);

        assert_eq!(watchlist.id, 7);
        assert_eq!(watchlist.item_count, 1);
        assert_eq!(watchlist.items.len(), 1);
        assert_eq!(
            watchlist.items,
            vec![WatchlistItem {
                instrument_id: 9,
                position: 0,
                notes: Some("benchmark".to_owned()),
                added_at: timestamp,
                instrument: watchlist.items[0].instrument.clone()
            }]
        );
    }
}
