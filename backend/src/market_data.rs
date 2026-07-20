#![allow(dead_code)]

use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;

use crate::config::AppConfig;

#[async_trait]
pub trait MarketDataProvider: Send + Sync {
    fn name(&self) -> &str;

    fn capabilities(&self) -> ProviderCapabilities;

    async fn search_instruments(
        &self,
        request: InstrumentSearchRequest,
    ) -> Result<Vec<MarketInstrument>, MarketDataError>;

    async fn latest_quotes(
        &self,
        request: LatestQuoteRequest,
    ) -> Result<Vec<MarketQuote>, MarketDataError>;
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderCapabilities {
    pub asset_classes: Vec<AssetClass>,
    pub supports_global_equities: bool,
    pub supports_corporate_bonds: bool,
    pub supports_government_bonds: bool,
    pub supports_realtime_quotes: bool,
    pub supports_delayed_quotes: bool,
}

impl ProviderCapabilities {
    pub fn equities_and_bonds(delayed_only: bool) -> Self {
        Self {
            asset_classes: vec![
                AssetClass::Equity,
                AssetClass::CorporateBond,
                AssetClass::GovernmentBond,
            ],
            supports_global_equities: true,
            supports_corporate_bonds: true,
            supports_government_bonds: true,
            supports_realtime_quotes: !delayed_only,
            supports_delayed_quotes: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AssetClass {
    Equity,
    CorporateBond,
    GovernmentBond,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InstrumentSearchRequest {
    pub query: String,
    pub asset_classes: Vec<AssetClass>,
    pub country: Option<String>,
    pub exchange: Option<String>,
    pub limit: u16,
}

impl InstrumentSearchRequest {
    pub fn equities_and_bonds(query: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            asset_classes: ProviderCapabilities::equities_and_bonds(false).asset_classes,
            country: None,
            exchange: None,
            limit: 25,
        }
    }

    pub fn with_limit(mut self, limit: u16) -> Self {
        self.limit = limit.clamp(1, 100);
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LatestQuoteRequest {
    pub instruments: Vec<ProviderInstrumentRef>,
}

impl LatestQuoteRequest {
    pub fn new(instruments: Vec<ProviderInstrumentRef>) -> Result<Self, MarketDataError> {
        if instruments.is_empty() {
            return Err(MarketDataError::EmptyInstrumentList);
        }

        Ok(Self { instruments })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderInstrumentRef {
    pub provider_id: String,
    pub symbol: Option<String>,
    pub asset_class: AssetClass,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MarketInstrument {
    pub provider_id: String,
    pub symbol: String,
    pub name: String,
    pub asset_class: AssetClass,
    pub currency: Option<String>,
    pub country: Option<String>,
    pub exchange: Option<String>,
    pub isin: Option<String>,
    pub cusip: Option<String>,
    pub issuer: Option<String>,
    pub maturity_date: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MarketQuote {
    pub instrument: ProviderInstrumentRef,
    pub price: f64,
    pub currency: String,
    pub as_of: DateTime<Utc>,
    pub bid: Option<f64>,
    pub ask: Option<f64>,
    pub yield_to_maturity: Option<f64>,
    pub duration: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct HttpMarketDataProvider {
    name: String,
    base_url: Url,
    api_key: String,
    timeout: Duration,
    client: reqwest::Client,
}

impl HttpMarketDataProvider {
    pub fn from_config(config: &AppConfig) -> Result<Self, MarketDataError> {
        let base_url = config
            .market_data_provider_base_url
            .as_deref()
            .ok_or(MarketDataError::NotConfigured {
                setting: "MARKET_DATA_PROVIDER_BASE_URL",
            })?;

        Self::new(
            config.market_data_provider_name.clone(),
            base_url,
            config.market_data_provider_key.clone(),
            Duration::from_secs(config.market_data_request_timeout_seconds),
        )
    }

    pub fn new(
        name: impl Into<String>,
        base_url: &str,
        api_key: impl Into<String>,
        timeout: Duration,
    ) -> Result<Self, MarketDataError> {
        let api_key = normalize_required(api_key.into(), "MARKET_DATA_PROVIDER_KEY")?;
        let base_url = Url::parse(base_url).map_err(MarketDataError::InvalidBaseUrl)?;

        Ok(Self {
            name: normalize_required(name.into(), "MARKET_DATA_PROVIDER_NAME")?,
            base_url,
            api_key,
            timeout,
            client: reqwest::Client::new(),
        })
    }

    async fn post_json<Request, Response>(
        &self,
        path: &str,
        request: &Request,
    ) -> Result<Response, MarketDataError>
    where
        Request: Serialize + Sync,
        Response: for<'de> Deserialize<'de>,
    {
        let url = self.base_url.join(path).map_err(MarketDataError::InvalidBaseUrl)?;
        let response = tokio::time::timeout(
            self.timeout,
            self.client
                .post(url)
                .bearer_auth(self.api_key.as_str())
                .json(request)
                .send(),
        )
        .await
        .map_err(|_| MarketDataError::Timeout)?
        .map_err(MarketDataError::Request)?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_else(|error| {
                format!("failed to read provider error response: {error}")
            });
            return Err(MarketDataError::Provider { status, body });
        }

        response
            .json::<Response>()
            .await
            .map_err(MarketDataError::Request)
    }
}

#[async_trait]
impl MarketDataProvider for HttpMarketDataProvider {
    fn name(&self) -> &str {
        self.name.as_str()
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::equities_and_bonds(false)
    }

    async fn search_instruments(
        &self,
        request: InstrumentSearchRequest,
    ) -> Result<Vec<MarketInstrument>, MarketDataError> {
        self.post_json("/v1/instruments/search", &request).await
    }

    async fn latest_quotes(
        &self,
        request: LatestQuoteRequest,
    ) -> Result<Vec<MarketQuote>, MarketDataError> {
        self.post_json("/v1/quotes/latest", &request).await
    }
}

#[derive(Debug, Error)]
pub enum MarketDataError {
    #[error("missing required market data setting {setting}")]
    NotConfigured { setting: &'static str },
    #[error("market data setting {setting} cannot be empty")]
    EmptySetting { setting: &'static str },
    #[error("market data provider base URL is invalid: {0}")]
    InvalidBaseUrl(#[source] url::ParseError),
    #[error("market data request timed out")]
    Timeout,
    #[error("market data request failed: {0}")]
    Request(#[source] reqwest::Error),
    #[error("market data provider returned {status}: {body}")]
    Provider { status: StatusCode, body: String },
    #[error("latest quote request must include at least one instrument")]
    EmptyInstrumentList,
}

fn normalize_required(
    value: String,
    setting: &'static str,
) -> Result<String, MarketDataError> {
    let normalized = value.trim().to_owned();
    if normalized.is_empty() {
        Err(MarketDataError::EmptySetting { setting })
    } else {
        Ok(normalized)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AssetClass, HttpMarketDataProvider, InstrumentSearchRequest, LatestQuoteRequest,
        MarketDataError, ProviderCapabilities, ProviderInstrumentRef,
    };

    #[test]
    fn exposes_global_equity_and_bond_capabilities() {
        let capabilities = ProviderCapabilities::equities_and_bonds(false);

        assert!(capabilities.supports_global_equities);
        assert!(capabilities.supports_corporate_bonds);
        assert!(capabilities.supports_government_bonds);
        assert!(capabilities.asset_classes.contains(&AssetClass::Equity));
        assert!(capabilities.asset_classes.contains(&AssetClass::CorporateBond));
        assert!(capabilities.asset_classes.contains(&AssetClass::GovernmentBond));
    }

    #[test]
    fn clamps_search_limit_to_provider_boundary() {
        let request = InstrumentSearchRequest::equities_and_bonds("treasury").with_limit(0);
        assert_eq!(request.limit, 1);

        let request = InstrumentSearchRequest::equities_and_bonds("treasury").with_limit(500);
        assert_eq!(request.limit, 100);
    }

    #[test]
    fn rejects_empty_latest_quote_requests() {
        let result = LatestQuoteRequest::new(Vec::new());
        assert!(matches!(result, Err(MarketDataError::EmptyInstrumentList)));
    }

    #[test]
    fn accepts_equity_and_bond_quote_refs() {
        let request = LatestQuoteRequest::new(vec![
            ProviderInstrumentRef {
                provider_id: "eq-1".to_owned(),
                symbol: Some("7203.T".to_owned()),
                asset_class: AssetClass::Equity,
            },
            ProviderInstrumentRef {
                provider_id: "bond-1".to_owned(),
                symbol: None,
                asset_class: AssetClass::GovernmentBond,
            },
        ]);

        assert!(request.is_ok());
    }

    #[test]
    fn http_adapter_requires_non_empty_settings() {
        let provider = HttpMarketDataProvider::new(
            " ",
            "https://provider.example.com",
            "key",
            std::time::Duration::from_secs(5),
        );
        assert!(matches!(
            provider,
            Err(MarketDataError::EmptySetting {
                setting: "MARKET_DATA_PROVIDER_NAME"
            })
        ));

        let provider = HttpMarketDataProvider::new(
            "http-json",
            "https://provider.example.com",
            " ",
            std::time::Duration::from_secs(5),
        );
        assert!(matches!(
            provider,
            Err(MarketDataError::EmptySetting {
                setting: "MARKET_DATA_PROVIDER_KEY"
            })
        ));
    }
}
