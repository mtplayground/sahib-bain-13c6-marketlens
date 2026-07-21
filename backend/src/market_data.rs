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

    pub fn finnhub() -> Self {
        Self {
            asset_classes: vec![AssetClass::Equity, AssetClass::Crypto],
            supports_global_equities: true,
            supports_corporate_bonds: false,
            supports_government_bonds: false,
            supports_realtime_quotes: true,
            supports_delayed_quotes: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AssetClass {
    Equity,
    Crypto,
    CorporateBond,
    GovernmentBond,
}

impl AssetClass {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Equity => "equity",
            Self::Crypto => "crypto",
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

#[derive(Debug, Clone)]
pub enum ConfiguredMarketDataProvider {
    Finnhub(FinnhubMarketDataProvider),
    Http(HttpMarketDataProvider),
}

impl ConfiguredMarketDataProvider {
    pub fn from_config(config: &AppConfig) -> Result<Self, MarketDataError> {
        match config
            .market_data_provider_name
            .trim()
            .to_ascii_lowercase()
            .as_str()
        {
            "finnhub" => FinnhubMarketDataProvider::from_config(config).map(Self::Finnhub),
            _ => HttpMarketDataProvider::from_config(config).map(Self::Http),
        }
    }
}

#[async_trait]
impl MarketDataProvider for ConfiguredMarketDataProvider {
    fn name(&self) -> &str {
        match self {
            Self::Finnhub(provider) => provider.name(),
            Self::Http(provider) => provider.name(),
        }
    }

    fn capabilities(&self) -> ProviderCapabilities {
        match self {
            Self::Finnhub(provider) => provider.capabilities(),
            Self::Http(provider) => provider.capabilities(),
        }
    }

    async fn search_instruments(
        &self,
        request: InstrumentSearchRequest,
    ) -> Result<Vec<MarketInstrument>, MarketDataError> {
        match self {
            Self::Finnhub(provider) => provider.search_instruments(request).await,
            Self::Http(provider) => provider.search_instruments(request).await,
        }
    }

    async fn latest_quotes(
        &self,
        request: LatestQuoteRequest,
    ) -> Result<Vec<MarketQuote>, MarketDataError> {
        match self {
            Self::Finnhub(provider) => provider.latest_quotes(request).await,
            Self::Http(provider) => provider.latest_quotes(request).await,
        }
    }

    async fn fundamentals(
        &self,
        request: FundamentalsRequest,
    ) -> Result<ProviderFundamentalsSnapshot, MarketDataError> {
        match self {
            Self::Finnhub(provider) => provider.fundamentals(request).await,
            Self::Http(provider) => provider.fundamentals(request).await,
        }
    }
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
            client: reqwest::Client::builder()
                .timeout(timeout)
                .build()
                .map_err(MarketDataError::Client)?,
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

const FINNHUB_DEFAULT_BASE_URL: &str = "https://finnhub.io";

#[derive(Debug, Clone)]
pub struct FinnhubMarketDataProvider {
    name: String,
    base_url: Url,
    api_key: String,
    timeout: Duration,
    client: reqwest::Client,
}

#[derive(Debug, Deserialize)]
struct FinnhubQuoteResponse {
    #[serde(default)]
    c: Option<f64>,
    #[serde(default)]
    h: Option<f64>,
    #[serde(default)]
    l: Option<f64>,
    #[serde(default)]
    o: Option<f64>,
    #[serde(default)]
    pc: Option<f64>,
    #[serde(default)]
    t: Option<i64>,
    #[serde(default)]
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FinnhubSearchResponse {
    #[serde(default)]
    result: Vec<FinnhubSearchResult>,
}

#[derive(Debug, Deserialize)]
struct FinnhubSearchResult {
    #[serde(default)]
    description: String,
    #[serde(default, rename = "displaySymbol")]
    display_symbol: String,
    #[serde(default)]
    symbol: String,
    #[serde(default, rename = "type")]
    instrument_type: String,
}

impl FinnhubMarketDataProvider {
    pub fn from_config(config: &AppConfig) -> Result<Self, MarketDataError> {
        Self::new(
            config.market_data_provider_name.clone(),
            config
                .market_data_provider_base_url
                .as_deref()
                .unwrap_or(FINNHUB_DEFAULT_BASE_URL),
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
            client: reqwest::Client::builder()
                .timeout(timeout)
                .build()
                .map_err(MarketDataError::Client)?,
        })
    }

    async fn get_json<Response>(
        &self,
        path: &str,
        query: Vec<(&str, String)>,
    ) -> Result<Response, MarketDataError>
    where
        Response: for<'de> Deserialize<'de>,
    {
        let url = self.base_url.join(path).map_err(MarketDataError::InvalidBaseUrl)?;
        let response = tokio::time::timeout(
            self.timeout,
            self.client
                .get(url)
                .query(&query)
                .query(&[("token", self.api_key.as_str())])
                .send(),
        )
        .await
        .map_err(|_| MarketDataError::Timeout)?
        .map_err(MarketDataError::Request)?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_else(|error| {
                format!("failed to read Finnhub error response: {error}")
            });
            return Err(classify_finnhub_http_error(status, body));
        }

        response
            .json::<Response>()
            .await
            .map_err(MarketDataError::Request)
    }

    async fn fetch_quote(
        &self,
        instrument: ProviderInstrumentRef,
    ) -> Result<MarketQuote, MarketDataError> {
        let finnhub_symbol = to_finnhub_symbol(&instrument)?;
        let response: FinnhubQuoteResponse = self
            .get_json("/api/v1/quote", vec![("symbol", finnhub_symbol)])
            .await?;

        if let Some(error) = response.error.as_deref().filter(|value| !value.trim().is_empty()) {
            return Err(classify_finnhub_message(error.to_owned()));
        }

        let price = response.c.ok_or_else(|| MarketDataError::MalformedProviderResponse {
            provider: "finnhub",
            message: "quote response is missing current price `c`".to_owned(),
        })?;
        if price <= 0.0 {
            return Err(MarketDataError::NoQuote {
                provider: "finnhub",
                instrument: instrument.provider_id.clone(),
            });
        }

        let timestamp = response.t.unwrap_or_default();
        let as_of = if timestamp > 0 {
            DateTime::<Utc>::from_timestamp(timestamp, 0).ok_or_else(|| {
                MarketDataError::MalformedProviderResponse {
                    provider: "finnhub",
                    message: format!("quote response has invalid unix timestamp {timestamp}"),
                }
            })?
        } else {
            Utc::now()
        };

        Ok(MarketQuote {
            currency: quote_currency(&instrument),
            instrument,
            price,
            as_of,
            bid: response.l,
            ask: response.h,
            yield_to_maturity: None,
            duration: None,
        })
    }
}

#[async_trait]
impl MarketDataProvider for FinnhubMarketDataProvider {
    fn name(&self) -> &str {
        self.name.as_str()
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::finnhub()
    }

    async fn search_instruments(
        &self,
        request: InstrumentSearchRequest,
    ) -> Result<Vec<MarketInstrument>, MarketDataError> {
        let response: FinnhubSearchResponse = self
            .get_json("/api/v1/search", vec![("q", request.query.clone())])
            .await?;
        let requested_classes = request.asset_classes;

        Ok(response
            .result
            .into_iter()
            .filter_map(|result| finnhub_search_result_to_instrument(result).ok())
            .filter(|instrument| {
                requested_classes.is_empty()
                    || requested_classes.contains(&instrument.asset_class)
            })
            .take(request.limit as usize)
            .collect())
    }

    async fn latest_quotes(
        &self,
        request: LatestQuoteRequest,
    ) -> Result<Vec<MarketQuote>, MarketDataError> {
        let mut quotes = Vec::with_capacity(request.instruments.len());
        for instrument in request.instruments {
            quotes.push(self.fetch_quote(instrument).await?);
        }
        Ok(quotes)
    }

    async fn fundamentals(
        &self,
        request: FundamentalsRequest,
    ) -> Result<ProviderFundamentalsSnapshot, MarketDataError> {
        Err(MarketDataError::UnsupportedOperation {
            provider: self.name.clone(),
            operation: format!(
                "fundamentals for {}",
                request.instrument.symbol.as_deref().unwrap_or(
                    request.instrument.provider_id.as_str()
                )
            ),
        })
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
    #[error("failed to configure market data HTTP client: {0}")]
    Client(#[source] reqwest::Error),
    #[error("market data request failed: {0}")]
    Request(#[source] reqwest::Error),
    #[error("market data provider returned {status}: {body}")]
    Provider { status: StatusCode, body: String },
    #[error("market data provider authentication failed: {body}")]
    Authentication { body: String },
    #[error("market data provider rate limit exceeded: {body}")]
    RateLimited { body: String },
    #[error("market data provider is temporarily unavailable ({status}): {body}")]
    ProviderUnavailable { status: StatusCode, body: String },
    #[error("{provider} response could not be understood: {message}")]
    MalformedProviderResponse {
        provider: &'static str,
        message: String,
    },
    #[error("{provider} returned no quote for {instrument}")]
    NoQuote {
        provider: &'static str,
        instrument: String,
    },
    #[error("{provider} does not support {operation}")]
    UnsupportedOperation { provider: String, operation: String },
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

fn to_finnhub_symbol(instrument: &ProviderInstrumentRef) -> Result<String, MarketDataError> {
    let raw = instrument
        .symbol
        .as_deref()
        .unwrap_or(instrument.provider_id.as_str())
        .trim();
    if raw.is_empty() {
        return Err(MarketDataError::EmptyProviderInstrumentId);
    }

    if raw.contains(':') {
        return Ok(raw.to_ascii_uppercase());
    }

    let normalized = raw.to_ascii_uppercase();
    if let Some((base, quote)) = normalized.split_once('/') {
        let base = base.trim();
        let quote = match quote.trim() {
            "USD" => "USDT",
            other => other,
        };
        if base.is_empty() || quote.is_empty() {
            return Err(MarketDataError::MalformedProviderResponse {
                provider: "finnhub",
                message: format!("invalid crypto pair symbol `{raw}`"),
            });
        }
        return Ok(format!("BINANCE:{base}{quote}"));
    }

    Ok(normalized)
}

fn quote_currency(instrument: &ProviderInstrumentRef) -> String {
    instrument
        .symbol
        .as_deref()
        .and_then(|symbol| symbol.split_once('/').map(|(_, quote)| quote.trim()))
        .filter(|quote| !quote.is_empty())
        .unwrap_or("USD")
        .to_ascii_uppercase()
}

fn classify_finnhub_http_error(status: StatusCode, body: String) -> MarketDataError {
    match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => MarketDataError::Authentication { body },
        StatusCode::TOO_MANY_REQUESTS => MarketDataError::RateLimited { body },
        status if status.is_server_error() || status == StatusCode::REQUEST_TIMEOUT => {
            MarketDataError::ProviderUnavailable { status, body }
        }
        _ => MarketDataError::Provider { status, body },
    }
}

fn classify_finnhub_message(message: String) -> MarketDataError {
    let normalized = message.to_ascii_lowercase();
    if normalized.contains("token")
        || normalized.contains("api key")
        || normalized.contains("permission")
        || normalized.contains("unauthorized")
    {
        MarketDataError::Authentication { body: message }
    } else if normalized.contains("limit") || normalized.contains("too many") {
        MarketDataError::RateLimited { body: message }
    } else {
        MarketDataError::Provider {
            status: StatusCode::OK,
            body: message,
        }
    }
}

fn finnhub_search_result_to_instrument(
    result: FinnhubSearchResult,
) -> Result<MarketInstrument, MarketDataError> {
    let provider_id = normalize_required(result.symbol, "finnhub_symbol")?;
    let symbol = normalize_required(
        if result.display_symbol.trim().is_empty() {
            provider_id.clone()
        } else {
            result.display_symbol
        },
        "finnhub_display_symbol",
    )?;
    let name = normalize_required(result.description, "finnhub_description")
        .unwrap_or_else(|_| symbol.clone());
    let asset_class = if result.instrument_type.to_ascii_lowercase().contains("crypto")
        || symbol.contains('/')
        || provider_id.to_ascii_uppercase().starts_with("BINANCE:")
    {
        AssetClass::Crypto
    } else {
        AssetClass::Equity
    };
    let currency = if asset_class == AssetClass::Crypto {
        Some(quote_currency(&ProviderInstrumentRef {
            provider_id: provider_id.clone(),
            symbol: Some(symbol.clone()),
            asset_class: asset_class.clone(),
        }))
    } else {
        Some("USD".to_owned())
    };

    Ok(MarketInstrument {
        provider_id,
        symbol,
        name,
        asset_class,
        currency,
        country: None,
        exchange: None,
        isin: None,
        cusip: None,
        issuer: None,
        maturity_date: None,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        classify_finnhub_http_error, quote_currency, to_finnhub_symbol, AssetClass,
        ConfiguredMarketDataProvider, FinnhubMarketDataProvider, FundamentalsRequest,
        HttpMarketDataProvider, InstrumentSearchRequest, LatestQuoteRequest, MarketDataError,
        ProviderCapabilities, ProviderInstrumentRef,
    };
    use crate::config::AppConfig;
    use reqwest::StatusCode;

    #[test]
    fn exposes_global_equity_and_bond_capabilities() {
        let capabilities = ProviderCapabilities::equities_and_bonds(false);

        assert!(capabilities.supports_global_equities);
        assert!(capabilities.supports_corporate_bonds);
        assert!(capabilities.supports_government_bonds);
        assert!(capabilities.asset_classes.contains(&AssetClass::Equity));
        assert!(ProviderCapabilities::finnhub()
            .asset_classes
            .contains(&AssetClass::Crypto));
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

    #[test]
    fn finnhub_adapter_translates_equity_and_crypto_symbols() {
        let equity = ProviderInstrumentRef {
            provider_id: "SPY".to_owned(),
            symbol: Some("spy".to_owned()),
            asset_class: AssetClass::Equity,
        };
        assert_eq!(to_finnhub_symbol(&equity).unwrap(), "SPY");
        assert_eq!(quote_currency(&equity), "USD");

        let crypto = ProviderInstrumentRef {
            provider_id: "BTC/USD".to_owned(),
            symbol: Some("BTC/USD".to_owned()),
            asset_class: AssetClass::Crypto,
        };
        assert_eq!(to_finnhub_symbol(&crypto).unwrap(), "BINANCE:BTCUSDT");
        assert_eq!(quote_currency(&crypto), "USD");

        let provider_specific = ProviderInstrumentRef {
            provider_id: "BINANCE:ETHUSDT".to_owned(),
            symbol: None,
            asset_class: AssetClass::Crypto,
        };
        assert_eq!(
            to_finnhub_symbol(&provider_specific).unwrap(),
            "BINANCE:ETHUSDT"
        );
    }

    #[test]
    fn finnhub_adapter_classifies_auth_rate_limit_and_transient_errors() {
        assert!(matches!(
            classify_finnhub_http_error(StatusCode::UNAUTHORIZED, "bad token".to_owned()),
            MarketDataError::Authentication { .. }
        ));
        assert!(matches!(
            classify_finnhub_http_error(StatusCode::TOO_MANY_REQUESTS, "slow down".to_owned()),
            MarketDataError::RateLimited { .. }
        ));
        assert!(matches!(
            classify_finnhub_http_error(StatusCode::BAD_GATEWAY, "upstream".to_owned()),
            MarketDataError::ProviderUnavailable { .. }
        ));
    }

    #[test]
    fn configured_provider_selects_finnhub_by_name() {
        let config = AppConfig {
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
            market_data_request_timeout_seconds: 5,
            live_market_ingestion_enabled: false,
            live_market_symbols: vec!["SPY".to_owned(), "BTC/USD".to_owned()],
            live_market_poll_interval_seconds: 5,
            live_market_provider_name: "finnhub".to_owned(),
            live_market_provider_base_url: Some("https://finnhub.io".to_owned()),
            news_provider_key: "news-key".to_owned(),
            news_provider_name: "http-json-news".to_owned(),
            news_provider_base_url: None,
            news_provider_request_timeout_seconds: 5,
            mctai_auth_url: "https://auth.mctai.app".to_owned(),
            mctai_auth_app_token: "app_test".to_owned(),
            mctai_auth_jwks_url: "https://auth.mctai.app/.well-known/jwks.json".to_owned(),
            mctai_email_url: None,
            mctai_email_app_token: None,
            self_url: None,
            allowed_cors_origin: None,
        };

        let provider = ConfiguredMarketDataProvider::from_config(&config).unwrap();
        assert!(matches!(
            provider,
            ConfiguredMarketDataProvider::Finnhub(FinnhubMarketDataProvider { .. })
        ));
    }
}
