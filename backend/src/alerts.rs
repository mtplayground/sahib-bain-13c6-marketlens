use std::str::FromStr;

use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    routing::{delete, get, patch, post},
    Json, Router,
};
use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Deserializer, Serialize};
use sqlx::FromRow;
use thiserror::Error;

use crate::{auth::AuthenticatedSession, state::AppState};

const DEFAULT_ALERT_COOLDOWN_SECONDS: i64 = 900;
const MAX_ALERT_COOLDOWN_SECONDS: i64 = 86_400;
const MAX_ALERT_LABEL_LENGTH: usize = 120;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/alerts", get(list_alert_rules).post(create_alert_rule))
        .route(
            "/alerts/:alert_id",
            get(get_alert_rule).patch(update_alert_rule).delete(delete_alert_rule),
        )
}

#[derive(Debug, Deserialize)]
struct AlertRulePath {
    alert_id: i64,
}

impl AlertRulePath {
    fn validate(&self) -> Result<ValidatedAlertRulePath, AlertRuleError> {
        if self.alert_id <= 0 {
            return Err(AlertRuleError::InvalidAlertRuleId);
        }

        Ok(ValidatedAlertRulePath {
            alert_id: self.alert_id,
        })
    }
}

#[derive(Debug, PartialEq, Eq)]
struct ValidatedAlertRulePath {
    alert_id: i64,
}

#[derive(Debug, Deserialize)]
struct CreateAlertRuleRequest {
    instrument_id: i64,
    metric: String,
    comparator: String,
    threshold: String,
    label: Option<String>,
    cooldown_seconds: Option<i64>,
}

impl CreateAlertRuleRequest {
    fn validate(&self) -> Result<ValidatedCreateAlertRule, AlertRuleError> {
        if self.instrument_id <= 0 {
            return Err(AlertRuleError::InvalidInstrumentId);
        }

        Ok(ValidatedCreateAlertRule {
            instrument_id: self.instrument_id,
            metric: normalize_metric(self.metric.as_str())?,
            comparator: normalize_comparator(self.comparator.as_str())?,
            threshold: parse_positive_decimal(self.threshold.as_str())?,
            label: normalize_label(self.label.as_deref())?,
            cooldown_seconds: validate_cooldown(
                self.cooldown_seconds
                    .unwrap_or(DEFAULT_ALERT_COOLDOWN_SECONDS),
            )?,
        })
    }
}

#[derive(Debug, Deserialize)]
struct UpdateAlertRuleRequest {
    instrument_id: Option<i64>,
    metric: Option<String>,
    comparator: Option<String>,
    threshold: Option<String>,
    status: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_label")]
    label: OptionalLabel,
    cooldown_seconds: Option<i64>,
}

impl UpdateAlertRuleRequest {
    fn validate(&self) -> Result<ValidatedUpdateAlertRule, AlertRuleError> {
        if self.instrument_id.is_none()
            && self.metric.is_none()
            && self.comparator.is_none()
            && self.threshold.is_none()
            && self.status.is_none()
            && matches!(self.label, OptionalLabel::Missing)
            && self.cooldown_seconds.is_none()
        {
            return Err(AlertRuleError::EmptyUpdate);
        }

        if self.instrument_id.is_some_and(|instrument_id| instrument_id <= 0) {
            return Err(AlertRuleError::InvalidInstrumentId);
        }

        Ok(ValidatedUpdateAlertRule {
            instrument_id: self.instrument_id,
            metric: self
                .metric
                .as_deref()
                .map(normalize_metric)
                .transpose()?,
            comparator: self
                .comparator
                .as_deref()
                .map(normalize_comparator)
                .transpose()?,
            threshold: self
                .threshold
                .as_deref()
                .map(parse_positive_decimal)
                .transpose()?,
            status: self.status.as_deref().map(normalize_status).transpose()?,
            label_provided: matches!(self.label, OptionalLabel::Provided(_)),
            label: match &self.label {
                OptionalLabel::Missing => None,
                OptionalLabel::Provided(label) => normalize_label(label.as_deref())?,
            },
            cooldown_seconds: self.cooldown_seconds.map(validate_cooldown).transpose()?,
        })
    }
}

#[derive(Debug, Default)]
enum OptionalLabel {
    #[default]
    Missing,
    Provided(Option<String>),
}

fn deserialize_optional_label<'de, D>(deserializer: D) -> Result<OptionalLabel, D::Error>
where
    D: Deserializer<'de>,
{
    Option::<String>::deserialize(deserializer).map(OptionalLabel::Provided)
}

#[derive(Debug, PartialEq, Eq)]
struct ValidatedCreateAlertRule {
    instrument_id: i64,
    metric: String,
    comparator: String,
    threshold: Decimal,
    label: Option<String>,
    cooldown_seconds: i64,
}

#[derive(Debug, PartialEq, Eq)]
struct ValidatedUpdateAlertRule {
    instrument_id: Option<i64>,
    metric: Option<String>,
    comparator: Option<String>,
    threshold: Option<Decimal>,
    status: Option<String>,
    label_provided: bool,
    label: Option<String>,
    cooldown_seconds: Option<i64>,
}

#[derive(Debug, Serialize)]
struct AlertRulesResponse {
    count: usize,
    results: Vec<AlertRule>,
}

#[derive(Debug, Serialize)]
struct AlertRuleResponse {
    alert_rule: AlertRule,
}

#[derive(Debug, Serialize)]
struct DeleteAlertRuleResponse {
    status: &'static str,
    alert_id: i64,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
struct AlertRule {
    id: i64,
    instrument_id: i64,
    metric: String,
    comparator: String,
    threshold: Decimal,
    status: String,
    label: Option<String>,
    cooldown_seconds: i64,
    last_triggered_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    instrument: AlertRuleInstrument,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
struct AlertRuleInstrument {
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
struct AlertRuleRow {
    alert_id: i64,
    instrument_id: i64,
    metric: String,
    comparator: String,
    threshold: Decimal,
    alert_status: String,
    label: Option<String>,
    cooldown_seconds: i64,
    last_triggered_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
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
    instrument_status: String,
    instrument_updated_at: DateTime<Utc>,
}

async fn list_alert_rules(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthenticatedSession>,
) -> Result<Json<AlertRulesResponse>, (StatusCode, Json<AlertRuleErrorResponse>)> {
    let rows = sqlx::query_as::<_, AlertRuleRow>(ALERT_RULE_SELECT_SQL)
        .bind(auth.user.sub.as_str())
        .fetch_all(state.database().pool())
        .await
        .map_err(AlertRuleError::Database)
        .map_err(alert_rule_error_response)?;
    let results = rows.into_iter().map(AlertRule::from).collect::<Vec<_>>();

    Ok(Json(AlertRulesResponse {
        count: results.len(),
        results,
    }))
}

async fn get_alert_rule(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthenticatedSession>,
    Path(path): Path<AlertRulePath>,
) -> Result<Json<AlertRuleResponse>, (StatusCode, Json<AlertRuleErrorResponse>)> {
    let path = path.validate().map_err(alert_rule_error_response)?;
    let alert_rule = fetch_alert_rule(&state, auth.user.sub.as_str(), path.alert_id)
        .await
        .map_err(alert_rule_error_response)?;

    Ok(Json(AlertRuleResponse { alert_rule }))
}

async fn create_alert_rule(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthenticatedSession>,
    Json(request): Json<CreateAlertRuleRequest>,
) -> Result<(StatusCode, Json<AlertRuleResponse>), (StatusCode, Json<AlertRuleErrorResponse>)> {
    let alert = request.validate().map_err(alert_rule_error_response)?;
    ensure_instrument_exists(&state, alert.instrument_id).await?;

    let row = sqlx::query_as::<_, AlertRuleRow>(
        r#"
        WITH created AS (
            INSERT INTO user_alert_rules (
                user_sub,
                instrument_id,
                metric,
                comparator,
                threshold,
                label,
                cooldown_seconds
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING *
        )
        SELECT
            created.id AS alert_id,
            created.instrument_id,
            created.metric,
            created.comparator,
            created.threshold,
            created.status AS alert_status,
            created.label,
            created.cooldown_seconds,
            created.last_triggered_at,
            created.created_at,
            created.updated_at,
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
            instrument.status AS instrument_status,
            instrument.updated_at AS instrument_updated_at
        FROM created
        INNER JOIN instruments instrument ON instrument.id = created.instrument_id
        "#,
    )
    .bind(auth.user.sub.as_str())
    .bind(alert.instrument_id)
    .bind(alert.metric.as_str())
    .bind(alert.comparator.as_str())
    .bind(alert.threshold)
    .bind(alert.label.as_deref())
    .bind(alert.cooldown_seconds)
    .fetch_one(state.database().pool())
    .await
    .map_err(AlertRuleError::Database)
    .map_err(alert_rule_error_response)?;

    Ok((
        StatusCode::CREATED,
        Json(AlertRuleResponse {
            alert_rule: AlertRule::from(row),
        }),
    ))
}

async fn update_alert_rule(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthenticatedSession>,
    Path(path): Path<AlertRulePath>,
    Json(request): Json<UpdateAlertRuleRequest>,
) -> Result<Json<AlertRuleResponse>, (StatusCode, Json<AlertRuleErrorResponse>)> {
    let path = path.validate().map_err(alert_rule_error_response)?;
    let alert = request.validate().map_err(alert_rule_error_response)?;
    if let Some(instrument_id) = alert.instrument_id {
        ensure_instrument_exists(&state, instrument_id).await?;
    }

    let row = sqlx::query_as::<_, AlertRuleRow>(
        r#"
        WITH updated AS (
            UPDATE user_alert_rules alert
            SET
                instrument_id = COALESCE($3::BIGINT, alert.instrument_id),
                metric = COALESCE($4::TEXT, alert.metric),
                comparator = COALESCE($5::TEXT, alert.comparator),
                threshold = COALESCE($6::NUMERIC, alert.threshold),
                status = COALESCE($7::TEXT, alert.status),
                label = CASE WHEN $8::BOOLEAN THEN $9::TEXT ELSE alert.label END,
                cooldown_seconds = COALESCE($10::BIGINT, alert.cooldown_seconds)
            WHERE alert.user_sub = $1
                AND alert.id = $2
            RETURNING *
        )
        SELECT
            updated.id AS alert_id,
            updated.instrument_id,
            updated.metric,
            updated.comparator,
            updated.threshold,
            updated.status AS alert_status,
            updated.label,
            updated.cooldown_seconds,
            updated.last_triggered_at,
            updated.created_at,
            updated.updated_at,
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
            instrument.status AS instrument_status,
            instrument.updated_at AS instrument_updated_at
        FROM updated
        INNER JOIN instruments instrument ON instrument.id = updated.instrument_id
        "#,
    )
    .bind(auth.user.sub.as_str())
    .bind(path.alert_id)
    .bind(alert.instrument_id)
    .bind(alert.metric.as_deref())
    .bind(alert.comparator.as_deref())
    .bind(alert.threshold)
    .bind(alert.status.as_deref())
    .bind(alert.label_provided)
    .bind(alert.label.as_deref())
    .bind(alert.cooldown_seconds)
    .fetch_optional(state.database().pool())
    .await
    .map_err(AlertRuleError::Database)
    .map_err(alert_rule_error_response)?
    .ok_or(AlertRuleError::AlertRuleNotFound)
    .map_err(alert_rule_error_response)?;

    Ok(Json(AlertRuleResponse {
        alert_rule: AlertRule::from(row),
    }))
}

async fn delete_alert_rule(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthenticatedSession>,
    Path(path): Path<AlertRulePath>,
) -> Result<Json<DeleteAlertRuleResponse>, (StatusCode, Json<AlertRuleErrorResponse>)> {
    let path = path.validate().map_err(alert_rule_error_response)?;
    let result = sqlx::query(
        r#"
        DELETE FROM user_alert_rules
        WHERE user_sub = $1
            AND id = $2
        "#,
    )
    .bind(auth.user.sub.as_str())
    .bind(path.alert_id)
    .execute(state.database().pool())
    .await
    .map_err(AlertRuleError::Database)
    .map_err(alert_rule_error_response)?;

    if result.rows_affected() == 0 {
        return Err(alert_rule_error_response(AlertRuleError::AlertRuleNotFound));
    }

    Ok(Json(DeleteAlertRuleResponse {
        status: "deleted",
        alert_id: path.alert_id,
    }))
}

async fn fetch_alert_rule(
    state: &AppState,
    user_sub: &str,
    alert_id: i64,
) -> Result<AlertRule, AlertRuleError> {
    let row = sqlx::query_as::<_, AlertRuleRow>(ALERT_RULE_BY_ID_SQL)
        .bind(user_sub)
        .bind(alert_id)
        .fetch_optional(state.database().pool())
        .await
        .map_err(AlertRuleError::Database)?
        .ok_or(AlertRuleError::AlertRuleNotFound)?;

    Ok(AlertRule::from(row))
}

async fn ensure_instrument_exists(
    state: &AppState,
    instrument_id: i64,
) -> Result<(), (StatusCode, Json<AlertRuleErrorResponse>)> {
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
    .map_err(AlertRuleError::Database)
    .map_err(alert_rule_error_response)?;

    if exists {
        Ok(())
    } else {
        Err(alert_rule_error_response(AlertRuleError::InstrumentNotFound))
    }
}

impl From<AlertRuleRow> for AlertRule {
    fn from(row: AlertRuleRow) -> Self {
        Self {
            id: row.alert_id,
            instrument_id: row.instrument_id,
            metric: row.metric,
            comparator: row.comparator,
            threshold: row.threshold,
            status: row.alert_status,
            label: row.label,
            cooldown_seconds: row.cooldown_seconds,
            last_triggered_at: row.last_triggered_at,
            created_at: row.created_at,
            updated_at: row.updated_at,
            instrument: AlertRuleInstrument {
                id: row.instrument_id,
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
                status: row.instrument_status,
                updated_at: row.instrument_updated_at,
            },
        }
    }
}

fn normalize_metric(value: &str) -> Result<String, AlertRuleError> {
    let metric = value.trim().to_ascii_lowercase();
    match metric.as_str() {
        "price" | "volume" => Ok(metric),
        _ => Err(AlertRuleError::InvalidMetric),
    }
}

fn normalize_comparator(value: &str) -> Result<String, AlertRuleError> {
    let comparator = value.trim().to_ascii_lowercase();
    match comparator.as_str() {
        "above" | "below" => Ok(comparator),
        _ => Err(AlertRuleError::InvalidComparator),
    }
}

fn normalize_status(value: &str) -> Result<String, AlertRuleError> {
    let status = value.trim().to_ascii_lowercase();
    match status.as_str() {
        "active" | "paused" => Ok(status),
        _ => Err(AlertRuleError::InvalidStatus),
    }
}

fn parse_positive_decimal(value: &str) -> Result<Decimal, AlertRuleError> {
    let threshold = Decimal::from_str(value.trim()).map_err(|_| AlertRuleError::InvalidThreshold)?;
    if threshold <= Decimal::ZERO {
        return Err(AlertRuleError::InvalidThreshold);
    }

    Ok(threshold)
}

fn normalize_label(value: Option<&str>) -> Result<Option<String>, AlertRuleError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let label = value.trim();
    if label.is_empty() {
        return Ok(None);
    }
    if label.chars().count() > MAX_ALERT_LABEL_LENGTH {
        return Err(AlertRuleError::InvalidLabel);
    }

    Ok(Some(label.to_owned()))
}

fn validate_cooldown(value: i64) -> Result<i64, AlertRuleError> {
    if !(0..=MAX_ALERT_COOLDOWN_SECONDS).contains(&value) {
        return Err(AlertRuleError::InvalidCooldown);
    }

    Ok(value)
}

#[derive(Debug, Error)]
enum AlertRuleError {
    #[error("alert_id must be a positive integer")]
    InvalidAlertRuleId,
    #[error("instrument_id must be a positive integer")]
    InvalidInstrumentId,
    #[error("metric must be price or volume")]
    InvalidMetric,
    #[error("comparator must be above or below")]
    InvalidComparator,
    #[error("threshold must be a positive decimal")]
    InvalidThreshold,
    #[error("status must be active or paused")]
    InvalidStatus,
    #[error("alert label must be 120 characters or fewer")]
    InvalidLabel,
    #[error("cooldown_seconds must be between 0 and 86400")]
    InvalidCooldown,
    #[error("alert update must include at least one field")]
    EmptyUpdate,
    #[error("alert rule was not found")]
    AlertRuleNotFound,
    #[error("instrument was not found")]
    InstrumentNotFound,
    #[error("alert rule query failed: {0}")]
    Database(#[source] sqlx::Error),
}

impl AlertRuleError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::InvalidAlertRuleId
            | Self::InvalidInstrumentId
            | Self::InvalidMetric
            | Self::InvalidComparator
            | Self::InvalidThreshold
            | Self::InvalidStatus
            | Self::InvalidLabel
            | Self::InvalidCooldown
            | Self::EmptyUpdate => StatusCode::BAD_REQUEST,
            Self::AlertRuleNotFound | Self::InstrumentNotFound => StatusCode::NOT_FOUND,
            Self::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn code(&self) -> &'static str {
        match self {
            Self::InvalidAlertRuleId => "invalid_alert_id",
            Self::InvalidInstrumentId => "invalid_instrument_id",
            Self::InvalidMetric => "invalid_alert_metric",
            Self::InvalidComparator => "invalid_alert_comparator",
            Self::InvalidThreshold => "invalid_alert_threshold",
            Self::InvalidStatus => "invalid_alert_status",
            Self::InvalidLabel => "invalid_alert_label",
            Self::InvalidCooldown => "invalid_alert_cooldown",
            Self::EmptyUpdate => "empty_alert_update",
            Self::AlertRuleNotFound => "alert_rule_not_found",
            Self::InstrumentNotFound => "instrument_not_found",
            Self::Database(_) => "alert_rules_failed",
        }
    }

    fn public_message(&self) -> String {
        match self {
            Self::Database(_) => "alert rules are temporarily unavailable".to_owned(),
            _ => self.to_string(),
        }
    }
}

#[derive(Debug, Serialize)]
struct AlertRuleErrorResponse {
    error: &'static str,
    message: String,
}

fn alert_rule_error_response(error: AlertRuleError) -> (StatusCode, Json<AlertRuleErrorResponse>) {
    if matches!(error, AlertRuleError::Database(_)) {
        tracing::error!(%error, "alert rule handler failed");
    }

    (
        error.status_code(),
        Json(AlertRuleErrorResponse {
            error: error.code(),
            message: error.public_message(),
        }),
    )
}

const ALERT_RULE_SELECT_SQL: &str = r#"
SELECT
    alert.id AS alert_id,
    alert.instrument_id,
    alert.metric,
    alert.comparator,
    alert.threshold,
    alert.status AS alert_status,
    alert.label,
    alert.cooldown_seconds,
    alert.last_triggered_at,
    alert.created_at,
    alert.updated_at,
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
    instrument.status AS instrument_status,
    instrument.updated_at AS instrument_updated_at
FROM user_alert_rules alert
INNER JOIN instruments instrument ON instrument.id = alert.instrument_id
WHERE alert.user_sub = $1
ORDER BY alert.updated_at DESC, alert.id DESC
"#;

const ALERT_RULE_BY_ID_SQL: &str = r#"
SELECT
    alert.id AS alert_id,
    alert.instrument_id,
    alert.metric,
    alert.comparator,
    alert.threshold,
    alert.status AS alert_status,
    alert.label,
    alert.cooldown_seconds,
    alert.last_triggered_at,
    alert.created_at,
    alert.updated_at,
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
    instrument.status AS instrument_status,
    instrument.updated_at AS instrument_updated_at
FROM user_alert_rules alert
INNER JOIN instruments instrument ON instrument.id = alert.instrument_id
WHERE alert.user_sub = $1
    AND alert.id = $2
"#;

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use rust_decimal::Decimal;

    use super::{
        AlertRule, AlertRulePath, AlertRuleRow, CreateAlertRuleRequest, UpdateAlertRuleRequest,
        ValidatedCreateAlertRule, ValidatedUpdateAlertRule, DEFAULT_ALERT_COOLDOWN_SECONDS,
    };

    #[test]
    fn validates_create_alert_rule() {
        let request = CreateAlertRuleRequest {
            instrument_id: 42,
            metric: " Price ".to_owned(),
            comparator: " ABOVE ".to_owned(),
            threshold: "101.25".to_owned(),
            label: Some("  breakout  ".to_owned()),
            cooldown_seconds: None,
        };

        assert_eq!(
            request.validate().expect("alert rule should be valid"),
            ValidatedCreateAlertRule {
                instrument_id: 42,
                metric: "price".to_owned(),
                comparator: "above".to_owned(),
                threshold: Decimal::new(10125, 2),
                label: Some("breakout".to_owned()),
                cooldown_seconds: DEFAULT_ALERT_COOLDOWN_SECONDS,
            }
        );
    }

    #[test]
    fn rejects_invalid_create_alert_rule() {
        assert!(CreateAlertRuleRequest {
            instrument_id: 0,
            metric: "price".to_owned(),
            comparator: "above".to_owned(),
            threshold: "1".to_owned(),
            label: None,
            cooldown_seconds: None,
        }
        .validate()
        .is_err());
        assert!(CreateAlertRuleRequest {
            instrument_id: 1,
            metric: "spread".to_owned(),
            comparator: "above".to_owned(),
            threshold: "1".to_owned(),
            label: None,
            cooldown_seconds: None,
        }
        .validate()
        .is_err());
        assert!(CreateAlertRuleRequest {
            instrument_id: 1,
            metric: "price".to_owned(),
            comparator: "near".to_owned(),
            threshold: "1".to_owned(),
            label: None,
            cooldown_seconds: None,
        }
        .validate()
        .is_err());
        assert!(CreateAlertRuleRequest {
            instrument_id: 1,
            metric: "price".to_owned(),
            comparator: "above".to_owned(),
            threshold: "0".to_owned(),
            label: None,
            cooldown_seconds: None,
        }
        .validate()
        .is_err());
    }

    #[test]
    fn validates_update_alert_rule() {
        let request = UpdateAlertRuleRequest {
            instrument_id: Some(9),
            metric: Some("volume".to_owned()),
            comparator: Some("below".to_owned()),
            threshold: Some("2500".to_owned()),
            status: Some("paused".to_owned()),
            label: super::OptionalLabel::Provided(None),
            cooldown_seconds: Some(60),
        };

        assert_eq!(
            request.validate().expect("update should be valid"),
            ValidatedUpdateAlertRule {
                instrument_id: Some(9),
                metric: Some("volume".to_owned()),
                comparator: Some("below".to_owned()),
                threshold: Some(Decimal::new(2500, 0)),
                status: Some("paused".to_owned()),
                label_provided: true,
                label: None,
                cooldown_seconds: Some(60),
            }
        );
    }

    #[test]
    fn rejects_invalid_update_alert_rule() {
        assert!(UpdateAlertRuleRequest {
            instrument_id: None,
            metric: None,
            comparator: None,
            threshold: None,
            status: None,
            label: super::OptionalLabel::Missing,
            cooldown_seconds: None,
        }
        .validate()
        .is_err());
        assert!(UpdateAlertRuleRequest {
            instrument_id: None,
            metric: None,
            comparator: None,
            threshold: None,
            status: Some("disabled".to_owned()),
            label: super::OptionalLabel::Missing,
            cooldown_seconds: None,
        }
        .validate()
        .is_err());
    }

    #[test]
    fn validates_alert_rule_path() {
        assert!(AlertRulePath { alert_id: 1 }.validate().is_ok());
        assert!(AlertRulePath { alert_id: 0 }.validate().is_err());
    }

    #[test]
    fn maps_alert_rule_row() {
        let Some(timestamp) = Utc.with_ymd_and_hms(2026, 7, 21, 1, 0, 0).single() else {
            panic!("valid timestamp");
        };
        let row = AlertRuleRow {
            alert_id: 7,
            instrument_id: 42,
            metric: "price".to_owned(),
            comparator: "above".to_owned(),
            threshold: Decimal::new(10025, 2),
            alert_status: "active".to_owned(),
            label: Some("breakout".to_owned()),
            cooldown_seconds: 900,
            last_triggered_at: None,
            created_at: timestamp,
            updated_at: timestamp,
            canonical_symbol: "AAPL".to_owned(),
            display_name: "Apple Inc.".to_owned(),
            asset_class: "equity".to_owned(),
            region: "US".to_owned(),
            country: Some("US".to_owned()),
            currency: Some("USD".to_owned()),
            exchange: Some("NASDAQ".to_owned()),
            issuer_name: Some("Apple Inc.".to_owned()),
            issuer_region: Some("US".to_owned()),
            maturity_date: None,
            instrument_status: "active".to_owned(),
            instrument_updated_at: timestamp,
        };

        let alert_rule = AlertRule::from(row);

        assert_eq!(alert_rule.id, 7);
        assert_eq!(alert_rule.instrument_id, 42);
        assert_eq!(alert_rule.metric, "price");
        assert_eq!(alert_rule.instrument.canonical_symbol, "AAPL");
    }
}
