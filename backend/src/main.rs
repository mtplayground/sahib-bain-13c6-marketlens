mod analytics;
mod alert_evaluation;
mod alerts;
mod auth;
mod config;
mod db;
mod email;
mod estimator;
mod fundamentals;
mod ingestion;
mod instrument_filter;
mod instrument_search;
mod instruments;
mod market_data;
mod news;
mod popularity;
mod redis;
mod routes;
mod series;
mod state;
mod timeframe;
mod users;
mod view_history;
mod watchlists;
mod ws;

use axum::{middleware, Router};
use config::AppConfig;
use db::Database;
use redis::RedisClient;
use state::AppState;
use tokio::net::TcpListener;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_tracing();

    let config = AppConfig::from_env()?;
    let addr = config.socket_addr()?;
    let database = Database::connect(&config).await?;
    database.run_migrations().await?;
    let seed_report = ingestion::seed_live_instrument_catalog(&database, &config).await?;
    tracing::info!(
        requested = seed_report.requested,
        upserted_instruments = seed_report.upserted_instruments,
        upserted_identifiers = seed_report.upserted_identifiers,
        provider = %config.live_market_provider_name,
        "seeded configured live instrument catalog"
    );
    let redis = RedisClient::connect(&config)?;
    alert_evaluation::spawn_worker(config.clone(), database.clone(), redis.clone());
    ingestion::spawn_live_market_worker(config.clone(), database.clone(), redis.clone())?;
    let state = AppState::new(config, database, redis);
    let app = app(state);
    let listener = TcpListener::bind(addr).await?;

    tracing::info!("MarketLens backend listening on http://{addr}");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

fn app(state: AppState) -> Router {
    let protected_auth = auth::protected_router()
        .merge(alerts::router())
        .merge(view_history::router())
        .merge(watchlists::router())
        .route_layer(middleware::from_fn_with_state(state.clone(), auth::require_auth));

    Router::new()
        .nest(
            "/api/v1",
            routes::api_router()
                .merge(estimator::router())
                .merge(fundamentals::router())
                .merge(instrument_filter::router())
                .merge(instrument_search::router())
                .merge(news::router())
                .merge(popularity::router())
                .merge(timeframe::router())
                .merge(auth::public_router())
                .merge(protected_auth),
        )
        .merge(ws::router())
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,marketlens_backend=debug,tower_http=info"));

    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer())
        .init();
}

async fn shutdown_signal() {
    if let Err(error) = tokio::signal::ctrl_c().await {
        tracing::warn!(%error, "failed to listen for shutdown signal");
    }
}
