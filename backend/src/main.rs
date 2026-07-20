mod config;
mod db;
mod redis;
mod routes;
mod state;
mod ws;

use axum::Router;
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
    let redis = RedisClient::connect(&config)?;
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
    Router::new()
        .nest("/api/v1", routes::api_router())
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
