use std::sync::Arc;

use crate::config::AppConfig;
use crate::db::Database;
use crate::redis::RedisClient;

#[derive(Clone, Debug)]
pub struct AppState {
    inner: Arc<AppStateInner>,
}

#[derive(Debug)]
struct AppStateInner {
    config: AppConfig,
    database: Database,
    redis: RedisClient,
}

impl AppState {
    pub fn new(config: AppConfig, database: Database, redis: RedisClient) -> Self {
        Self {
            inner: Arc::new(AppStateInner {
                config,
                database,
                redis,
            }),
        }
    }

    pub fn config(&self) -> &AppConfig {
        &self.inner.config
    }

    pub fn database(&self) -> &Database {
        &self.inner.database
    }

    pub fn redis(&self) -> &RedisClient {
        &self.inner.redis
    }
}
