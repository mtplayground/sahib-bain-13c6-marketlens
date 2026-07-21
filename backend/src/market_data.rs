#![allow(dead_code)]

use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
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

    async fn fundamentals(
        &self,
        request: FundamentalsRequest,
    ) -> Result<ProviderFundamentalsSnapshot, MarketDataError>;
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

impl AssetClass {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Equity => "equity",
            Self::CorporateBond => "corporate_bond",
            Self::GovernmentBond => "government_bond",
        }
    }
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FundamentalsRequest {
    pub instrument: ProviderInstrumentRef,
}

impl FundamentalsRequest {
    pub fn new(instrument: ProviderInstrumentRef) -> Result<Self, MarketDataError> {
        if instrument.provider_id.trim().is_empty() {
            return Err(MarketDataError::EmptyProviderInstrumentId);
        }

        Ok(Self { instrument })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProviderFundamentalsSnapshot {
    pub instrument: ProviderInstrumentRef,
    pub company_financials: Vec<ProviderCompanyFinancial>,
    pub bond_yield_curve_points: Vec<ProviderBondYieldCurvePoint>,
    pub credit_ratings: Vec<ProviderCreditRating>,
    pub key_ratios: Vec<ProviderKeyRatios>,
    pub source_updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProviderCompanyFinancial {
    pub fiscal_period_end: NaiveDate,
    pub fiscal_period_type: String,
    pub currency: Option<String>,
    pub revenue: Option<Decimal>,
    pub gross_profit: Option<Decimal>,
    pub operating_income: Option<Decimal>,
    pub net_income: Option<Decimal>,
    pub ebitda: Option<Decimal>,
    pub eps_diluted: Option<Decimal>,
    pub total_assets: Option<Decimal>,
    pub total_liabilities: Option<Decimal>,
    pub shareholder_equity: Option<Decimal>,
    pub operating_cash_flow: Option<Decimal>,
    pub free_cash_flow: Option<Decimal>,
    pub source_updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProviderBondYieldCurvePoint {
    pub curve_name: String,
    pub region: Option<String>,
    pub currency: Option<String>,
    pub tenor_months: i32,
    pub yield_percent: Decimal,
    pub observed_at: DateTime<Utc>,
    pub source_updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderCreditRating {
    pub agency: String,
    pub rating_type: String,
    pub rating: String,
    pub outlook: Option<String>,
    pub watch_status: Option<String>,
    pub effective_at: Option<DateTime<Utc>>,
    pub source_updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProviderKeyRatios {
    pub as_of_date: NaiveDate,
    pub pe_ratio: Option<Decimal>,
    pub pb_ratio: Option<Decimal>,
    pub ps_ratio: Option<Decimal>,
    pub dividend_yield: Option<Decimal>,
    pub return_on_equity: Option<Decimal>,
    pub return_on_assets: Option<Decimal>,
    pub debt_to_equity: Option<Decimal>,
    pub current_ratio: Option<Decimal>,
    pub quick_ratio: Option<Decimal>,
    pub gross_margin: Option<Decimal>,
    pub operating_margin: Option<Decimal>,
    pub net_margin: Option<Decimal>,
    pub source_updated_at: Option<DateTime<Utc>>,
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

    async fn fundamentals(
        &self,
        request: FundamentalsRequest,
    ) -> Result<ProviderFundamentalsSnapshot, MarketDataError> {
        self.post_json("/v1/fundamentals/snapshot", &request).await
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
    #[error("provider instrument id cannot be empty")]
    EmptyProviderInstrumentId,
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
        AssetClass, FundamentalsRequest, HttpMarketDataProvider, InstrumentSearchRequest,
        LatestQuoteRequest, MarketDataError, ProviderCapabilities, ProviderInstrumentRef,
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
    fn rejects_empty_fundamentals_instrument_id() {
        let result = FundamentalsRequest::new(ProviderInstrumentRef {
            provider_id: " ".to_owned(),
            symbol: Some("SPY".to_owned()),
            asset_class: AssetClass::Equity,
        });

        assert!(matches!(
            result,
            Err(MarketDataError::EmptyProviderInstrumentId)
        ));
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
