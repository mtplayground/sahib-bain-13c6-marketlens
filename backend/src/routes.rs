use axum::{extract::State, http::StatusCode, routing::get, Json, Router};
use serde::Serialize;

use crate::db::DatabaseHealth;
use crate::redis::{channels, RedisHealth};
use crate::state::AppState;

pub fn api_router() -> Router<AppState> {
    Router::new()
        .route("/health", get(health))
        .route("/config/status", get(config_status))
}

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
    service: &'static str,
    database: DatabaseHealth,
    redis: RedisHealth,
}

async fn health(State(state): State<AppState>) -> (StatusCode, Json<HealthResponse>) {
    let database = state.database().health_check().await;
    let redis = state.redis().health_check().await;
    let is_healthy = database.status == "ok" && redis.status == "ok";
    let status = if is_healthy { "ok" } else { "degraded" };
    let status_code = if is_healthy {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (
        status_code,
        Json(HealthResponse {
            status,
            service: "marketlens-backend",
            database,
            redis,
        }),
    )
}

#[derive(Serialize)]
struct ConfigStatusResponse {
    database_url_configured: bool,
    database_max_connections: u32,
    database_connect_timeout_seconds: u64,
    database_ssl_mode: Option<String>,
    redis_url_configured: bool,
    jwt_secret_configured: bool,
    market_data_provider_key_configured: bool,
    market_data_provider_name: String,
    market_data_provider_base_url_configured: bool,
    market_data_request_timeout_seconds: u64,
    news_provider_key_configured: bool,
    auth_configured: bool,
    email_configured: bool,
    self_url_configured: bool,
    allowed_cors_origin_configured: bool,
    redis_channels: RedisChannelsResponse,
}

#[derive(Serialize)]
struct RedisChannelsResponse {
    namespace: &'static str,
    market_ticks_pattern: &'static str,
    market_ticks_example: String,
    alert_events: &'static str,
    user_alert_events_pattern: &'static str,
    user_alert_events_example: String,
}

async fn config_status(State(state): State<AppState>) -> Json<ConfigStatusResponse> {
    let config = state.config();

    Json(ConfigStatusResponse {
        database_url_configured: !config.database_url.is_empty(),
        database_max_connections: config.database_max_connections,
        database_connect_timeout_seconds: config.database_connect_timeout_seconds,
        database_ssl_mode: config.database_ssl_mode.clone(),
        redis_url_configured: !config.redis_url.is_empty(),
        jwt_secret_configured: !config.jwt_secret.is_empty(),
        market_data_provider_key_configured: !config.market_data_provider_key.is_empty(),
        market_data_provider_name: config.market_data_provider_name.clone(),
        market_data_provider_base_url_configured: config.market_data_provider_base_url.is_some(),
        market_data_request_timeout_seconds: config.market_data_request_timeout_seconds,
        news_provider_key_configured: !config.news_provider_key.is_empty(),
        auth_configured: !config.mctai_auth_url.is_empty()
            && !config.mctai_auth_app_token.is_empty()
            && !config.mctai_auth_jwks_url.is_empty(),
        email_configured: config.mctai_email_url.is_some()
            && config.mctai_email_app_token.is_some(),
        self_url_configured: config.self_url.is_some(),
        allowed_cors_origin_configured: config.allowed_cors_origin.is_some(),
        redis_channels: RedisChannelsResponse {
            namespace: channels::NAMESPACE,
            market_ticks_pattern: channels::MARKET_TICKS_PATTERN,
            market_ticks_example: channels::market_ticks("BTC/USD"),
            alert_events: channels::ALERT_EVENTS,
            user_alert_events_pattern: channels::USER_ALERT_EVENTS_PATTERN,
            user_alert_events_example: channels::user_alert_events("user-sub"),
        },
    })
}
