use std::{
    str::FromStr,
    time::{Duration, Instant},
};

use serde::Serialize;
use sqlx::{
    postgres::{PgConnectOptions, PgPoolOptions, PgSslMode},
    PgPool,
};
use thiserror::Error;

use crate::config::AppConfig;

#[derive(Clone, Debug)]
pub struct Database {
    pool: PgPool,
}

#[derive(Debug, Error)]
pub enum DbError {
    #[error("failed to connect to PostgreSQL: {0}")]
    Connect(#[source] sqlx::Error),
    #[error("failed to run PostgreSQL migrations: {0}")]
    Migrate(#[from] sqlx::migrate::MigrateError),
}

#[derive(Debug, Serialize)]
pub struct DatabaseHealth {
    pub status: &'static str,
    pub latency_ms: u128,
    pub pool_size: u32,
    pub idle_connections: usize,
    pub error: Option<String>,
}

impl Database {
    pub async fn connect(config: &AppConfig) -> Result<Self, DbError> {
        let connect_options =
            PgConnectOptions::from_str(&config.database_url).map_err(DbError::Connect)?;
        let connect_options = apply_ssl_mode(connect_options, config.database_ssl_mode.as_deref());
        let pool_options = || {
            PgPoolOptions::new()
                .max_connections(config.database_max_connections)
                .acquire_timeout(Duration::from_secs(
                    config.database_connect_timeout_seconds,
                ))
        };

        let pool = match pool_options().connect_with(connect_options.clone()).await {
            Ok(pool) => pool,
            Err(error) if should_retry_without_tls(&config.database_url, &error) => {
                tracing::warn!(
                    %error,
                    "PostgreSQL TLS negotiation failed; retrying with sslmode=disable because DATABASE_URL does not set sslmode"
                );

                pool_options()
                    .connect_with(connect_options.ssl_mode(PgSslMode::Disable))
                    .await
                    .map_err(DbError::Connect)?
            }
            Err(error) => return Err(DbError::Connect(error)),
        };

        Ok(Self { pool })
    }

    pub async fn run_migrations(&self) -> Result<(), DbError> {
        sqlx::migrate!("./migrations").run(&self.pool).await?;
        Ok(())
    }

    pub async fn health_check(&self) -> DatabaseHealth {
        let started_at = Instant::now();
        let result = sqlx::query_scalar::<_, i32>("SELECT 1")
            .fetch_one(&self.pool)
            .await;
        let latency_ms = started_at.elapsed().as_millis();

        match result {
            Ok(1) => DatabaseHealth {
                status: "ok",
                latency_ms,
                pool_size: self.pool.size(),
                idle_connections: self.pool.num_idle(),
                error: None,
            },
            Ok(value) => DatabaseHealth {
                status: "degraded",
                latency_ms,
                pool_size: self.pool.size(),
                idle_connections: self.pool.num_idle(),
                error: Some(format!("unexpected database health value: {value}")),
            },
            Err(error) => DatabaseHealth {
                status: "down",
                latency_ms,
                pool_size: self.pool.size(),
                idle_connections: self.pool.num_idle(),
                error: Some(error.to_string()),
            },
        }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
}

fn should_retry_without_tls(database_url: &str, error: &sqlx::Error) -> bool {
    !database_url.to_ascii_lowercase().contains("sslmode=")
        && error
            .to_string()
            .contains("unexpected response from SSLRequest")
}

fn apply_ssl_mode(options: PgConnectOptions, ssl_mode: Option<&str>) -> PgConnectOptions {
    match ssl_mode {
        Some("disable") => options.ssl_mode(PgSslMode::Disable),
        Some("prefer") => options.ssl_mode(PgSslMode::Prefer),
        Some("require") => options.ssl_mode(PgSslMode::Require),
        Some("verify-ca") => options.ssl_mode(PgSslMode::VerifyCa),
        Some("verify-full") => options.ssl_mode(PgSslMode::VerifyFull),
        Some(_) | None => options,
    }
}
