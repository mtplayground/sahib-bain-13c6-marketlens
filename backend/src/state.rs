use std::sync::Arc;

use crate::config::AppConfig;
use crate::db::Database;

#[derive(Clone, Debug)]
pub struct AppState {
    inner: Arc<AppStateInner>,
}

#[derive(Debug)]
struct AppStateInner {
    config: AppConfig,
    database: Database,
}

impl AppState {
    pub fn new(config: AppConfig, database: Database) -> Self {
        Self {
            inner: Arc::new(AppStateInner { config, database }),
        }
    }

    pub fn config(&self) -> &AppConfig {
        &self.inner.config
    }

    pub fn database(&self) -> &Database {
        &self.inner.database
    }
}
