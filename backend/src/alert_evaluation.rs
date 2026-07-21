#![allow(dead_code)]

use std::time::Duration;

use chrono::{DateTime, Utc};
use futures_util::StreamExt;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{FromRow, Postgres, Transaction};
use thiserror::Error;

use crate::{
    config::AppConfig,
    db::Database,
    email::{send_email, EmailDelivery},
    redis::{channels, RedisClient, RedisError},
};

const RECONNECT_INITIAL_DELAY: Duration = Duration::from_secs(1);
const RECONNECT_MAX_DELAY: Duration = Duration::from_secs(30);

pub fn spawn_worker(config: AppConfig, database: Database, redis: RedisClient) {
    tokio::spawn(async move {
        AlertEvaluationWorker::new(config, database, redis)
            .run_forever()
            .await;
    });
}

#[derive(Debug, Clone)]
pub struct AlertEvaluationWorker {
    config: AppConfig,
    database: Database,
    redis: RedisClient,
}

impl AlertEvaluationWorker {
    pub fn new(config: AppConfig, database: Database, redis: RedisClient) -> Self {
        Self {
            config,
            database,
            redis,
        }
    }

    pub async fn run_forever(&self) {
        let mut reconnect_delay = RECONNECT_INITIAL_DELAY;

        loop {
            match self.run_until_stream_ends().await {
                Ok(()) => {
                    tracing::warn!(
                        "alert evaluation Redis stream ended; reconnecting after delay"
                    );
                    reconnect_delay = RECONNECT_INITIAL_DELAY;
                }
                Err(error) => {
                    tracing::error!(%error, "alert evaluation worker failed; reconnecting after delay");
                }
            }

            tokio::time::sleep(reconnect_delay).await;
            reconnect_delay = (reconnect_delay * 2).min(RECONNECT_MAX_DELAY);
        }
    }

    async fn run_until_stream_ends(&self) -> Result<(), AlertEvaluationError> {
        let mut pubsub = self.redis.pubsub().await?;
        pubsub
            .psubscribe(channels::MARKET_TICKS_PATTERN)
            .await
            .map_err(RedisError::Command)?;
        let mut messages = pubsub.into_on_message();

        while let Some(message) = messages.next().await {
            let redis_channel = message.get_channel_name().to_owned();
            let payload = match message.get_payload::<String>() {
                Ok(payload) => payload,
                Err(error) => {
                    tracing::warn!(%error, %redis_channel, "failed to decode market tick payload for alert evaluation");
                    continue;
                }
            };

            let tick = match serde_json::from_str::<EvaluableMarketTick>(&payload) {
                Ok(tick) => tick,
                Err(error) => {
                    tracing::warn!(%error, %redis_channel, "failed to parse market tick payload for alert evaluation");
                    continue;
                }
            };

            if let Err(error) = self.evaluate_tick(&tick).await {
                tracing::error!(
                    %error,
                    %redis_channel,
                    symbol = %tick.symbol,
                    "failed to evaluate market tick alert rules"
                );
            }
        }

        Ok(())
    }

    pub async fn evaluate_tick(
        &self,
        tick: &EvaluableMarketTick,
    ) -> Result<AlertEvaluationReport, AlertEvaluationError> {
        let rules = fetch_matching_rules(&self.database, tick).await?;
        let evaluated_rules = rules.len();
        let mut triggered_rules = 0_usize;
        let mut published_events = 0_u64;
        let mut emailed_alerts = 0_usize;

        for rule in rules {
            let Some(observed_value) = observed_value(&rule, tick) else {
                continue;
            };

            if !threshold_crossed(observed_value, rule.comparator.as_str(), rule.threshold) {
                continue;
            }

            if !cooldown_elapsed(rule.last_triggered_at, rule.cooldown_seconds, Utc::now()) {
                continue;
            }

            let Some(event) = mark_triggered(&self.database, &rule, tick, observed_value).await?
            else {
                continue;
            };

            triggered_rules += 1;
            let delivery = self.deliver_triggered_alert(&rule, &event).await?;
            published_events += delivery.redis_subscribers;
            if delivery.email_sent {
                emailed_alerts += 1;
            }
        }

        Ok(AlertEvaluationReport {
            evaluated_rules,
            triggered_rules,
            published_events,
            emailed_alerts,
        })
    }

    async fn deliver_triggered_alert(
        &self,
        rule: &ActiveAlertRule,
        event: &AlertTriggeredEvent,
    ) -> Result<AlertDeliveryOutcome, AlertEvaluationError> {
        let payload = serde_json::to_string(event)?;
        let redis_subscribers = self
            .redis
            .publish_user_alert_event(rule.user_sub.as_str(), payload.as_str())
            .await?;
        mark_in_app_delivered(&self.database, event.trigger_id).await?;

        let email_sent = match send_trigger_email(&self.config, rule, event).await {
            Ok(EmailDelivery::Sent { message_id }) => {
                mark_email_delivery(
                    &self.database,
                    event.trigger_id,
                    "sent",
                    message_id.as_deref(),
                    None,
                )
                .await?;
                true
            }
            Ok(EmailDelivery::SkippedNotConfigured) => {
                mark_email_delivery(
                    &self.database,
                    event.trigger_id,
                    "skipped_not_configured",
                    None,
                    None,
                )
                .await?;
                false
            }
            Err(error) => {
                let error_message = error.to_string();
                let error_message = truncated_error(error_message.as_str()).to_owned();
                tracing::warn!(
                    %error,
                    alert_id = event.alert_id,
                    trigger_id = event.trigger_id,
                    user_sub = %rule.user_sub,
                    "failed to deliver alert email"
                );
                mark_email_delivery(
                    &self.database,
                    event.trigger_id,
                    "failed",
                    None,
                    Some(error_message.as_str()),
                )
                .await?;
                false
            }
        };

        Ok(AlertDeliveryOutcome {
            redis_subscribers,
            email_sent,
        })
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct EvaluableMarketTick {
    #[serde(rename = "type")]
    pub event_type: Option<String>,
    pub provider: String,
    pub provider_instrument_id: String,
    pub series_id: Option<i64>,
    pub symbol: String,
    pub asset_class: String,
    pub price: Decimal,
    pub currency: Option<String>,
    pub as_of: DateTime<Utc>,
    pub bid: Option<Decimal>,
    pub ask: Option<Decimal>,
    pub volume: Option<Decimal>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AlertEvaluationReport {
    pub evaluated_rules: usize,
    pub triggered_rules: usize,
    pub published_events: u64,
    pub emailed_alerts: usize,
}

#[derive(Debug, Clone, FromRow, PartialEq, Eq)]
struct ActiveAlertRule {
    alert_id: i64,
    user_sub: String,
    user_email: String,
    user_name: Option<String>,
    instrument_id: i64,
    canonical_symbol: String,
    display_name: String,
    metric: String,
    comparator: String,
    threshold: Decimal,
    label: Option<String>,
    cooldown_seconds: i64,
    last_triggered_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
struct AlertTriggeredEvent {
    #[serde(rename = "type")]
    event_type: &'static str,
    alert_id: i64,
    trigger_id: i64,
    instrument_id: i64,
    symbol: String,
    display_name: String,
    metric: String,
    comparator: String,
    threshold: Decimal,
    observed_value: Decimal,
    label: Option<String>,
    tick_observed_at: DateTime<Utc>,
    triggered_at: DateTime<Utc>,
}

#[derive(Debug, PartialEq, Eq)]
struct AlertDeliveryOutcome {
    redis_subscribers: u64,
    email_sent: bool,
}

async fn fetch_matching_rules(
    database: &Database,
    tick: &EvaluableMarketTick,
) -> Result<Vec<ActiveAlertRule>, AlertEvaluationError> {
    let rows = sqlx::query_as::<_, ActiveAlertRule>(
        r#"
        SELECT
            alert.id AS alert_id,
            alert.user_sub,
            app_user.email AS user_email,
            app_user.name AS user_name,
            alert.instrument_id,
            instrument.canonical_symbol,
            instrument.display_name,
            alert.metric,
            alert.comparator,
            alert.threshold,
            alert.label,
            alert.cooldown_seconds,
            alert.last_triggered_at
        FROM user_alert_rules alert
        INNER JOIN instruments instrument ON instrument.id = alert.instrument_id
        INNER JOIN users app_user ON app_user.sub = alert.user_sub
        WHERE alert.status = 'active'
            AND instrument.status = 'active'
            AND instrument.asset_class = $4
            AND (
                lower(instrument.canonical_symbol) = lower($1)
                OR EXISTS (
                    SELECT 1
                    FROM instrument_identifiers identifier
                    WHERE identifier.instrument_id = instrument.id
                        AND identifier.identifier_type IN ('symbol', 'ticker')
                        AND lower(identifier.identifier_value) = lower($1)
                )
                OR EXISTS (
                    SELECT 1
                    FROM instrument_identifiers identifier
                    WHERE identifier.instrument_id = instrument.id
                        AND identifier.identifier_type = 'provider_id'
                        AND lower(identifier.identifier_value) = lower($2)
                        AND (
                            $3::TEXT IS NULL
                            OR identifier.provider IS NULL
                            OR lower(identifier.provider) = lower($3)
                        )
                )
            )
        ORDER BY alert.id ASC
        "#,
    )
    .bind(tick.symbol.as_str())
    .bind(tick.provider_instrument_id.as_str())
    .bind(non_empty(tick.provider.as_str()))
    .bind(tick.asset_class.as_str())
    .fetch_all(database.pool())
    .await?;

    Ok(rows)
}

async fn mark_in_app_delivered(
    database: &Database,
    trigger_id: i64,
) -> Result<(), AlertEvaluationError> {
    sqlx::query(
        r#"
        UPDATE user_alert_rule_triggers
        SET in_app_delivered_at = COALESCE(in_app_delivered_at, NOW())
        WHERE id = $1
        "#,
    )
    .bind(trigger_id)
    .execute(database.pool())
    .await?;

    Ok(())
}

async fn mark_email_delivery(
    database: &Database,
    trigger_id: i64,
    status: &str,
    message_id: Option<&str>,
    error: Option<&str>,
) -> Result<(), AlertEvaluationError> {
    sqlx::query(
        r#"
        UPDATE user_alert_rule_triggers
        SET
            email_delivery_status = $2,
            email_message_id = $3,
            email_delivered_at = CASE WHEN $2 = 'sent' THEN NOW() ELSE email_delivered_at END,
            email_error = $4
        WHERE id = $1
        "#,
    )
    .bind(trigger_id)
    .bind(status)
    .bind(message_id)
    .bind(error)
    .execute(database.pool())
    .await?;

    Ok(())
}

async fn send_trigger_email(
    config: &AppConfig,
    rule: &ActiveAlertRule,
    event: &AlertTriggeredEvent,
) -> Result<EmailDelivery, crate::email::EmailError> {
    let subject = format!(
        "Alert triggered: {} {} {}",
        event.symbol.as_str(),
        event.metric.as_str(),
        event.comparator.as_str()
    );
    let label = rule.label.as_deref().unwrap_or("Alert rule");
    let greeting = rule
        .user_name
        .as_deref()
        .filter(|name| !name.trim().is_empty())
        .unwrap_or("there");
    let threshold_line = format!(
        "{} {} {}",
        event.metric.as_str(),
        event.comparator.as_str(),
        event.threshold
    );
    let observed_line = format!(
        "{} observed at {} on {}",
        event.observed_value,
        event.tick_observed_at,
        event.symbol.as_str()
    );
    let html = format!(
        concat!(
            "<p>Hello {greeting},</p>",
            "<p>Your alert <strong>{label}</strong> triggered for <strong>{symbol}</strong>.</p>",
            "<p>Rule: {threshold_line}<br>Observed: {observed_line}</p>"
        ),
        greeting = escape_html(greeting),
        label = escape_html(label),
        symbol = escape_html(event.symbol.as_str()),
        threshold_line = escape_html(threshold_line.as_str()),
        observed_line = escape_html(observed_line.as_str()),
    );
    let text = format!(
        "Hello {greeting},\n\nYour alert {label} triggered for {symbol}.\nRule: {threshold_line}\nObserved: {observed_line}",
        greeting = greeting,
        label = label,
        symbol = event.symbol.as_str(),
        threshold_line = threshold_line,
        observed_line = observed_line,
    );

    send_email(config, rule.user_email.as_str(), subject.as_str(), html.as_str(), text.as_str())
        .await
}

async fn mark_triggered(
    database: &Database,
    rule: &ActiveAlertRule,
    tick: &EvaluableMarketTick,
    observed_value: Decimal,
) -> Result<Option<AlertTriggeredEvent>, AlertEvaluationError> {
    let triggered_at = Utc::now();
    let mut transaction = database.pool().begin().await?;
    let updated = update_rule_trigger_time(&mut transaction, rule, triggered_at).await?;
    if !updated {
        transaction.rollback().await?;
        return Ok(None);
    }

    let payload = json!({
        "provider": &tick.provider,
        "provider_instrument_id": &tick.provider_instrument_id,
        "series_id": tick.series_id,
        "currency": &tick.currency,
        "price": tick.price,
        "bid": tick.bid,
        "ask": tick.ask,
        "volume": tick.volume,
    });
    let trigger_id = insert_trigger_row(
        &mut transaction,
        rule,
        tick,
        observed_value,
        triggered_at,
        payload,
    )
    .await?;

    transaction.commit().await?;

    Ok(Some(AlertTriggeredEvent {
        event_type: "alert.triggered",
        alert_id: rule.alert_id,
        trigger_id,
        instrument_id: rule.instrument_id,
        symbol: rule.canonical_symbol.clone(),
        display_name: rule.display_name.clone(),
        metric: rule.metric.clone(),
        comparator: rule.comparator.clone(),
        threshold: rule.threshold,
        observed_value,
        label: rule.label.clone(),
        tick_observed_at: tick.as_of,
        triggered_at,
    }))
}

async fn update_rule_trigger_time(
    transaction: &mut Transaction<'_, Postgres>,
    rule: &ActiveAlertRule,
    triggered_at: DateTime<Utc>,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        r#"
        UPDATE user_alert_rules
        SET last_triggered_at = $3
        WHERE id = $1
            AND user_sub = $2
            AND status = 'active'
            AND (
                last_triggered_at IS NULL
                OR last_triggered_at <= $3 - ($4::BIGINT * INTERVAL '1 second')
            )
        "#,
    )
    .bind(rule.alert_id)
    .bind(rule.user_sub.as_str())
    .bind(triggered_at)
    .bind(rule.cooldown_seconds)
    .execute(&mut **transaction)
    .await?;

    Ok(result.rows_affected() > 0)
}

async fn insert_trigger_row(
    transaction: &mut Transaction<'_, Postgres>,
    rule: &ActiveAlertRule,
    tick: &EvaluableMarketTick,
    observed_value: Decimal,
    triggered_at: DateTime<Utc>,
    payload: serde_json::Value,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar::<_, i64>(
        r#"
        INSERT INTO user_alert_rule_triggers (
            alert_rule_id,
            user_sub,
            instrument_id,
            metric,
            comparator,
            threshold,
            observed_value,
            tick_observed_at,
            triggered_at,
            payload
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
        ON CONFLICT (
            alert_rule_id,
            tick_observed_at,
            metric,
            comparator,
            threshold,
            observed_value
        ) DO UPDATE
        SET triggered_at = EXCLUDED.triggered_at
        RETURNING id
        "#,
    )
    .bind(rule.alert_id)
    .bind(rule.user_sub.as_str())
    .bind(rule.instrument_id)
    .bind(rule.metric.as_str())
    .bind(rule.comparator.as_str())
    .bind(rule.threshold)
    .bind(observed_value)
    .bind(tick.as_of)
    .bind(triggered_at)
    .bind(payload)
    .fetch_one(&mut **transaction)
    .await
}

fn observed_value(rule: &ActiveAlertRule, tick: &EvaluableMarketTick) -> Option<Decimal> {
    match rule.metric.as_str() {
        "price" => Some(tick.price),
        "volume" => tick.volume,
        _ => None,
    }
}

fn threshold_crossed(observed_value: Decimal, comparator: &str, threshold: Decimal) -> bool {
    match comparator {
        "above" => observed_value >= threshold,
        "below" => observed_value <= threshold,
        _ => false,
    }
}

fn cooldown_elapsed(
    last_triggered_at: Option<DateTime<Utc>>,
    cooldown_seconds: i64,
    now: DateTime<Utc>,
) -> bool {
    let Some(last_triggered_at) = last_triggered_at else {
        return true;
    };
    if cooldown_seconds <= 0 {
        return true;
    }

    now.signed_duration_since(last_triggered_at)
        .num_seconds()
        >= cooldown_seconds
}

fn non_empty(value: &str) -> Option<&str> {
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

fn escape_html(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(character),
        }
    }
    escaped
}

fn truncated_error(error: &str) -> &str {
    const MAX_ERROR_LENGTH: usize = 500;
    if error.len() <= MAX_ERROR_LENGTH {
        return error;
    }

    let mut end = MAX_ERROR_LENGTH;
    while !error.is_char_boundary(end) {
        end -= 1;
    }
    &error[..end]
}

#[derive(Debug, Error)]
pub enum AlertEvaluationError {
    #[error("database alert evaluation failed: {0}")]
    Database(#[from] sqlx::Error),
    #[error("redis alert evaluation failed: {0}")]
    Redis(#[from] RedisError),
    #[error("failed to serialize alert event: {0}")]
    Serialize(#[from] serde_json::Error),
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, TimeZone, Utc};
    use rust_decimal::Decimal;

    use super::{
        cooldown_elapsed, escape_html, observed_value, threshold_crossed, truncated_error,
        ActiveAlertRule, EvaluableMarketTick,
    };

    fn timestamp() -> chrono::DateTime<Utc> {
        let Some(timestamp) = Utc.with_ymd_and_hms(2026, 7, 21, 1, 30, 0).single() else {
            panic!("valid timestamp");
        };
        timestamp
    }

    fn tick() -> EvaluableMarketTick {
        EvaluableMarketTick {
            event_type: Some("market.tick".to_owned()),
            provider: "test-provider".to_owned(),
            provider_instrument_id: "provider-spy".to_owned(),
            series_id: Some(42),
            symbol: "SPY".to_owned(),
            asset_class: "equity".to_owned(),
            price: Decimal::new(55125, 2),
            currency: Some("USD".to_owned()),
            as_of: timestamp(),
            bid: None,
            ask: None,
            volume: Some(Decimal::new(1_200_000, 0)),
        }
    }

    fn rule(metric: &str, comparator: &str, threshold: Decimal) -> ActiveAlertRule {
        ActiveAlertRule {
            alert_id: 7,
            user_sub: "user-1".to_owned(),
            user_email: "trader@example.com".to_owned(),
            user_name: Some("Trader".to_owned()),
            instrument_id: 42,
            canonical_symbol: "SPY".to_owned(),
            display_name: "SPDR S&P 500 ETF".to_owned(),
            metric: metric.to_owned(),
            comparator: comparator.to_owned(),
            threshold,
            label: Some("watch".to_owned()),
            cooldown_seconds: 900,
            last_triggered_at: None,
        }
    }

    #[test]
    fn evaluates_price_thresholds() {
        assert!(threshold_crossed(
            Decimal::new(55125, 2),
            "above",
            Decimal::new(55000, 2)
        ));
        assert!(threshold_crossed(
            Decimal::new(55125, 2),
            "below",
            Decimal::new(55200, 2)
        ));
        assert!(!threshold_crossed(
            Decimal::new(55125, 2),
            "above",
            Decimal::new(55200, 2)
        ));
    }

    #[test]
    fn selects_observed_metric_value() {
        assert_eq!(
            observed_value(&rule("price", "above", Decimal::new(1, 0)), &tick()),
            Some(Decimal::new(55125, 2))
        );
        assert_eq!(
            observed_value(&rule("volume", "above", Decimal::new(1, 0)), &tick()),
            Some(Decimal::new(1_200_000, 0))
        );
        assert_eq!(
            observed_value(&rule("volume", "above", Decimal::new(1, 0)), &EvaluableMarketTick {
                volume: None,
                ..tick()
            }),
            None
        );
    }

    #[test]
    fn enforces_cooldown_window() {
        let now = timestamp();
        assert!(cooldown_elapsed(None, 900, now));
        assert!(cooldown_elapsed(Some(now - Duration::seconds(900)), 900, now));
        assert!(!cooldown_elapsed(
            Some(now - Duration::seconds(899)),
            900,
            now
        ));
        assert!(cooldown_elapsed(Some(now), 0, now));
    }

    #[test]
    fn escapes_alert_email_html() {
        assert_eq!(
            escape_html(r#"<A&B "quoted">"#),
            "&lt;A&amp;B &quot;quoted&quot;&gt;"
        );
    }

    #[test]
    fn truncates_error_without_breaking_utf8() {
        let error = format!("{}é", "x".repeat(500));
        assert_eq!(truncated_error(error.as_str()).len(), 500);
    }
}
