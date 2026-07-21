use std::{env, net::SocketAddr};

use thiserror::Error;
use url::Url;

#[derive(Clone, Debug)]
pub struct AppConfig {
    pub host: String,
    pub port: u16,
    pub database_url: String,
    pub database_max_connections: u32,
    pub database_connect_timeout_seconds: u64,
    pub database_ssl_mode: Option<String>,
    pub redis_url: String,
    pub jwt_secret: String,
    pub market_data_provider_key: String,
    pub market_data_provider_name: String,
    pub market_data_provider_base_url: Option<String>,
    pub market_data_request_timeout_seconds: u64,
    pub news_provider_key: String,
    pub news_provider_name: String,
    pub news_provider_base_url: Option<String>,
    pub news_provider_request_timeout_seconds: u64,
    pub mctai_auth_url: String,
    pub mctai_auth_app_token: String,
    pub mctai_auth_jwks_url: String,
    pub mctai_email_url: Option<String>,
    pub mctai_email_app_token: Option<String>,
    pub self_url: Option<String>,
    pub allowed_cors_origin: Option<String>,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("missing required environment variable {0}")]
    Missing(&'static str),
    #[error("PORT must be a valid u16: {0}")]
    InvalidPort(std::num::ParseIntError),
    #[error("{name} must be a valid positive integer: {source}")]
    InvalidInteger {
        name: &'static str,
        source: std::num::ParseIntError,
    },
    #[error("{0} must be greater than zero")]
    NonPositive(&'static str),
    #[error("DATABASE_SSL_MODE must be one of disable, prefer, require, verify-ca, verify-full")]
    InvalidDatabaseSslMode,
    #[error("{name} must be a valid URL: {source}")]
    InvalidUrl {
        name: &'static str,
        source: url::ParseError,
    },
    #[error("DATABASE_URL must use postgres:// or postgresql://")]
    InvalidDatabaseUrlScheme,
    #[error("REDIS_URL must use redis:// or rediss://")]
    InvalidRedisUrlScheme,
    #[error("{0} must use http:// or https://")]
    InvalidHttpUrlScheme(&'static str),
    #[error("MCTAI_EMAIL_URL and MCTAI_EMAIL_APP_TOKEN must be configured together")]
    IncompleteEmailConfig,
    #[error("HOST and PORT must form a valid socket address: {0}")]
    InvalidSocketAddress(#[from] std::net::AddrParseError),
}

impl AppConfig {
    pub fn from_env() -> Result<Self, ConfigError> {
        dotenvy::dotenv().ok();

        let config = Self {
            host: env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_owned()),
            port: optional_env("PORT")?
                .map(|value| value.parse::<u16>())
                .transpose()
                .map_err(ConfigError::InvalidPort)?
                .unwrap_or(8080),
            database_url: required_env("DATABASE_URL")?,
            database_max_connections: parse_optional_u32("DATABASE_MAX_CONNECTIONS", 5)?,
            database_connect_timeout_seconds: parse_optional_u64(
                "DATABASE_CONNECT_TIMEOUT_SECONDS",
                5,
            )?,
            database_ssl_mode: parse_database_ssl_mode()?,
            redis_url: required_env("REDIS_URL")?,
            jwt_secret: required_env("JWT_SECRET")?,
            market_data_provider_key: required_env("MARKET_DATA_PROVIDER_KEY")?,
            market_data_provider_name: optional_env("MARKET_DATA_PROVIDER_NAME")?
                .unwrap_or_else(|| "http-json".to_owned()),
            market_data_provider_base_url: optional_env("MARKET_DATA_PROVIDER_BASE_URL")?,
            market_data_request_timeout_seconds: parse_optional_u64(
                "MARKET_DATA_REQUEST_TIMEOUT_SECONDS",
                10,
            )?,
            news_provider_key: required_env("NEWS_PROVIDER_KEY")?,
            news_provider_name: optional_env("NEWS_PROVIDER_NAME")?
                .unwrap_or_else(|| "http-json-news".to_owned()),
            news_provider_base_url: optional_env("NEWS_PROVIDER_BASE_URL")?,
            news_provider_request_timeout_seconds: parse_optional_u64(
                "NEWS_PROVIDER_REQUEST_TIMEOUT_SECONDS",
                10,
            )?,
            mctai_auth_url: required_env("MCTAI_AUTH_URL")?,
            mctai_auth_app_token: required_env("MCTAI_AUTH_APP_TOKEN")?,
            mctai_auth_jwks_url: required_env("MCTAI_AUTH_JWKS_URL")?,
            mctai_email_url: optional_env("MCTAI_EMAIL_URL")?,
            mctai_email_app_token: optional_env("MCTAI_EMAIL_APP_TOKEN")?,
            self_url: optional_env("SELF_URL")?,
            allowed_cors_origin: optional_env("ALLOWED_CORS_ORIGIN")?,
        };

        config.validate()?;
        Ok(config)
    }

    pub fn socket_addr(&self) -> Result<SocketAddr, ConfigError> {
        format!("{}:{}", self.host, self.port)
            .parse()
            .map_err(ConfigError::from)
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        validate_database_url(&self.database_url)?;
        validate_redis_url(&self.redis_url)?;
        validate_http_url("MCTAI_AUTH_URL", self.mctai_auth_url.as_str())?;
        validate_http_url("MCTAI_AUTH_JWKS_URL", self.mctai_auth_jwks_url.as_str())?;

        if let Some(value) = self.market_data_provider_base_url.as_deref() {
            validate_http_url("MARKET_DATA_PROVIDER_BASE_URL", value)?;
        }
        if let Some(value) = self.news_provider_base_url.as_deref() {
            validate_http_url("NEWS_PROVIDER_BASE_URL", value)?;
        }
        if let Some(value) = self.self_url.as_deref() {
            validate_http_url("SELF_URL", value)?;
        }
        if let Some(value) = self.allowed_cors_origin.as_deref() {
            validate_http_url("ALLOWED_CORS_ORIGIN", value)?;
        }

        match (
            self.mctai_email_url.as_deref(),
            self.mctai_email_app_token.as_deref(),
        ) {
            (Some(value), Some(_)) => validate_http_url("MCTAI_EMAIL_URL", value)?,
            (None, None) => {}
            (Some(_), None) | (None, Some(_)) => return Err(ConfigError::IncompleteEmailConfig),
        }

        Ok(())
    }
}

fn required_env(name: &'static str) -> Result<String, ConfigError> {
    env::var(name)
        .map(|value| value.trim().to_owned())
        .ok()
        .filter(|value| !value.is_empty())
        .ok_or(ConfigError::Missing(name))
}

fn optional_env(name: &'static str) -> Result<Option<String>, ConfigError> {
    match env::var(name) {
        Ok(value) => {
            let trimmed = value.trim().to_owned();
            Ok((!trimmed.is_empty()).then_some(trimmed))
        }
        Err(env::VarError::NotPresent) => Ok(None),
        Err(env::VarError::NotUnicode(_)) => Err(ConfigError::Missing(name)),
    }
}

fn parse_optional_u32(name: &'static str, default: u32) -> Result<u32, ConfigError> {
    let value = optional_env(name)?
        .map(|value| value.parse::<u32>())
        .transpose()
        .map_err(|source| ConfigError::InvalidInteger { name, source })
        .map(|value| value.unwrap_or(default))?;

    if value == 0 {
        return Err(ConfigError::NonPositive(name));
    }

    Ok(value)
}

fn parse_optional_u64(name: &'static str, default: u64) -> Result<u64, ConfigError> {
    let value = optional_env(name)?
        .map(|value| value.parse::<u64>())
        .transpose()
        .map_err(|source| ConfigError::InvalidInteger { name, source })
        .map(|value| value.unwrap_or(default))?;

    if value == 0 {
        return Err(ConfigError::NonPositive(name));
    }

    Ok(value)
}

fn parse_database_ssl_mode() -> Result<Option<String>, ConfigError> {
    let mode = optional_env("DATABASE_SSL_MODE")?;

    match mode.as_deref() {
        None => Ok(None),
        Some("disable" | "prefer" | "require" | "verify-ca" | "verify-full") => Ok(mode),
        Some(_) => Err(ConfigError::InvalidDatabaseSslMode),
    }
}

fn parse_url(name: &'static str, value: &str) -> Result<Url, ConfigError> {
    Url::parse(value).map_err(|source| ConfigError::InvalidUrl { name, source })
}

fn validate_database_url(value: &str) -> Result<(), ConfigError> {
    let url = parse_url("DATABASE_URL", value)?;
    match url.scheme() {
        "postgres" | "postgresql" => Ok(()),
        _ => Err(ConfigError::InvalidDatabaseUrlScheme),
    }
}

fn validate_redis_url(value: &str) -> Result<(), ConfigError> {
    let url = parse_url("REDIS_URL", value)?;
    match url.scheme() {
        "redis" | "rediss" => Ok(()),
        _ => Err(ConfigError::InvalidRedisUrlScheme),
    }
}

fn validate_http_url(name: &'static str, value: &str) -> Result<(), ConfigError> {
    let url = parse_url(name, value)?;
    match url.scheme() {
        "http" | "https" => Ok(()),
        _ => Err(ConfigError::InvalidHttpUrlScheme(name)),
    }
}
