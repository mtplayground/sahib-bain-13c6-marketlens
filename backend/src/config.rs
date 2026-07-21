use std::{collections::BTreeSet, env, net::SocketAddr};

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
    pub live_market_ingestion_enabled: bool,
    pub live_market_symbols: Vec<String>,
    pub live_market_poll_interval_seconds: u64,
    pub live_market_provider_name: String,
    pub live_market_provider_base_url: Option<String>,
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
    #[error("{name} must be true or false")]
    InvalidBoolean { name: &'static str },
    #[error("{0} must be greater than zero")]
    NonPositive(&'static str),
    #[error("{0} must include at least one symbol")]
    EmptySymbolList(&'static str),
    #[error("{name} contains invalid live market symbol `{symbol}`")]
    InvalidLiveMarketSymbol { name: &'static str, symbol: String },
    #[error("MARKET_DATA_PROVIDER_KEY is required when live market ingestion is enabled for provider {provider}")]
    MissingLiveMarketProviderKey { provider: String },
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
        let market_data_provider_name =
            optional_env("MARKET_DATA_PROVIDER_NAME")?.unwrap_or_else(|| "finnhub".to_owned());
        let market_data_provider_base_url = optional_env("MARKET_DATA_PROVIDER_BASE_URL")?;
        let live_market_provider_name = optional_env("LIVE_MARKET_PROVIDER_NAME")?
            .unwrap_or_else(|| market_data_provider_name.clone());
        let live_market_provider_base_url =
            optional_env("LIVE_MARKET_PROVIDER_BASE_URL")?.or_else(|| {
                market_data_provider_base_url.clone()
            });

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
            market_data_provider_key: optional_env("MARKET_DATA_PROVIDER_KEY")?.unwrap_or_default(),
            market_data_provider_name,
            market_data_provider_base_url,
            market_data_request_timeout_seconds: parse_optional_u64(
                "MARKET_DATA_REQUEST_TIMEOUT_SECONDS",
                10,
            )?,
            live_market_ingestion_enabled: parse_optional_bool(
                "LIVE_MARKET_INGESTION_ENABLED",
                false,
            )?,
            live_market_symbols: parse_symbol_list(
                "LIVE_MARKET_SYMBOLS",
                "SPY,BTC/USD,NVDA,ETH/USD,VIX",
            )?,
            live_market_poll_interval_seconds: parse_optional_u64(
                "LIVE_MARKET_POLL_INTERVAL_SECONDS",
                5,
            )?,
            live_market_provider_name,
            live_market_provider_base_url,
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
        if let Some(value) = self.live_market_provider_base_url.as_deref() {
            validate_http_url("LIVE_MARKET_PROVIDER_BASE_URL", value)?;
        }
        validate_provider_name(
            "MARKET_DATA_PROVIDER_NAME",
            self.market_data_provider_name.as_str(),
        )?;
        validate_provider_name(
            "LIVE_MARKET_PROVIDER_NAME",
            self.live_market_provider_name.as_str(),
        )?;
        if self.live_market_ingestion_enabled && self.market_data_provider_key.trim().is_empty() {
            return Err(ConfigError::MissingLiveMarketProviderKey {
                provider: self.live_market_provider_name.clone(),
            });
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

fn parse_optional_bool(name: &'static str, default: bool) -> Result<bool, ConfigError> {
    match optional_env(name)?.as_deref().map(str::to_ascii_lowercase) {
        None => Ok(default),
        Some(value) if matches!(value.as_str(), "true" | "1" | "yes" | "on") => Ok(true),
        Some(value) if matches!(value.as_str(), "false" | "0" | "no" | "off") => Ok(false),
        Some(_) => Err(ConfigError::InvalidBoolean { name }),
    }
}

fn parse_symbol_list(name: &'static str, default: &str) -> Result<Vec<String>, ConfigError> {
    let raw = optional_env(name)?.unwrap_or_else(|| default.to_owned());
    parse_symbol_list_value(name, raw.as_str())
}

fn parse_symbol_list_value(name: &'static str, raw: &str) -> Result<Vec<String>, ConfigError> {
    let mut seen = BTreeSet::new();
    let mut symbols = Vec::new();

    for item in raw.split(',') {
        let symbol = item.trim().to_ascii_uppercase();
        if symbol.is_empty() {
            continue;
        }
        validate_live_market_symbol(name, symbol.as_str())?;
        if seen.insert(symbol.clone()) {
            symbols.push(symbol);
        }
    }

    if symbols.is_empty() {
        return Err(ConfigError::EmptySymbolList(name));
    }

    Ok(symbols)
}

fn validate_live_market_symbol(name: &'static str, symbol: &str) -> Result<(), ConfigError> {
    let valid = symbol
        .chars()
        .all(|ch| {
            ch.is_ascii_uppercase()
                || ch.is_ascii_digit()
                || matches!(ch, '/' | '.' | '-' | ':')
        });

    if valid {
        Ok(())
    } else {
        Err(ConfigError::InvalidLiveMarketSymbol {
            name,
            symbol: symbol.to_owned(),
        })
    }
}

fn validate_provider_name(name: &'static str, value: &str) -> Result<(), ConfigError> {
    if value.trim().is_empty() {
        Err(ConfigError::Missing(name))
    } else {
        Ok(())
    }
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

#[cfg(test)]
mod tests {
    use super::{parse_symbol_list_value, AppConfig, ConfigError};

    #[test]
    fn live_market_symbols_default_to_frontend_realtime_set() {
        let symbols =
            parse_symbol_list_value("LIVE_MARKET_SYMBOLS", "SPY,BTC/USD,NVDA,ETH/USD,VIX")
                .expect("default symbols should parse");

        assert_eq!(
            symbols,
            vec![
                "SPY".to_owned(),
                "BTC/USD".to_owned(),
                "NVDA".to_owned(),
                "ETH/USD".to_owned(),
                "VIX".to_owned()
            ]
        );
    }

    #[test]
    fn live_market_symbols_are_normalized_and_deduplicated() {
        let symbols = parse_symbol_list_value("LIVE_MARKET_SYMBOLS", " spy, BTC/usd,SPY ");

        assert_eq!(
            symbols.expect("symbols should parse"),
            vec!["SPY".to_owned(), "BTC/USD".to_owned()]
        );
    }

    #[test]
    fn live_market_symbols_reject_invalid_characters() {
        let result = parse_symbol_list_value("LIVE_MARKET_SYMBOLS", "SPY,not valid");

        assert!(matches!(
            result,
            Err(ConfigError::InvalidLiveMarketSymbol { .. })
        ));
    }

    #[test]
    fn live_market_ingestion_requires_provider_key_only_when_enabled() {
        let mut config = minimal_config();
        config.market_data_provider_key.clear();
        config.live_market_ingestion_enabled = false;
        assert!(config.validate().is_ok());

        config.live_market_ingestion_enabled = true;
        assert!(matches!(
            config.validate(),
            Err(ConfigError::MissingLiveMarketProviderKey { .. })
        ));
    }

    fn minimal_config() -> AppConfig {
        AppConfig {
            host: "0.0.0.0".to_owned(),
            port: 8080,
            database_url: "postgres://postgres:postgres@localhost:5432/marketlens".to_owned(),
            database_max_connections: 5,
            database_connect_timeout_seconds: 5,
            database_ssl_mode: None,
            redis_url: "redis://localhost:6379".to_owned(),
            jwt_secret: "legacy".to_owned(),
            market_data_provider_key: "finnhub-key".to_owned(),
            market_data_provider_name: "finnhub".to_owned(),
            market_data_provider_base_url: Some("https://finnhub.io".to_owned()),
            market_data_request_timeout_seconds: 10,
            live_market_ingestion_enabled: false,
            live_market_symbols: vec!["SPY".to_owned(), "BTC/USD".to_owned()],
            live_market_poll_interval_seconds: 5,
            live_market_provider_name: "finnhub".to_owned(),
            live_market_provider_base_url: Some("https://finnhub.io".to_owned()),
            news_provider_key: "news-key".to_owned(),
            news_provider_name: "http-json-news".to_owned(),
            news_provider_base_url: Some("https://news-provider.example.com".to_owned()),
            news_provider_request_timeout_seconds: 10,
            mctai_auth_url: "https://auth.mctai.app".to_owned(),
            mctai_auth_app_token: "app_test".to_owned(),
            mctai_auth_jwks_url: "https://auth.mctai.app/.well-known/jwks.json".to_owned(),
            mctai_email_url: None,
            mctai_email_app_token: None,
            self_url: None,
            allowed_cors_origin: None,
        }
    }
}
