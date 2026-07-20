use axum::{extract::State, routing::get, Json, Router};
use serde::Serialize;

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
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        service: "marketlens-backend",
    })
}

#[derive(Serialize)]
struct ConfigStatusResponse {
    database_url_configured: bool,
    redis_url_configured: bool,
    jwt_secret_configured: bool,
    market_data_provider_key_configured: bool,
    news_provider_key_configured: bool,
    auth_configured: bool,
    email_configured: bool,
    self_url_configured: bool,
    allowed_cors_origin_configured: bool,
}

async fn config_status(State(state): State<AppState>) -> Json<ConfigStatusResponse> {
    let config = state.config();

    Json(ConfigStatusResponse {
        database_url_configured: !config.database_url.is_empty(),
        redis_url_configured: !config.redis_url.is_empty(),
        jwt_secret_configured: !config.jwt_secret.is_empty(),
        market_data_provider_key_configured: !config.market_data_provider_key.is_empty(),
        news_provider_key_configured: !config.news_provider_key.is_empty(),
        auth_configured: !config.mctai_auth_url.is_empty()
            && !config.mctai_auth_app_token.is_empty()
            && !config.mctai_auth_jwks_url.is_empty(),
        email_configured: config.mctai_email_url.is_some()
            && config.mctai_email_app_token.is_some(),
        self_url_configured: config.self_url.is_some(),
        allowed_cors_origin_configured: config.allowed_cors_origin.is_some(),
    })
}
