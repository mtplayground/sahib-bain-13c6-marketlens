#![allow(dead_code)]

use std::{fmt, str::FromStr};

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use thiserror::Error;

use crate::market_data::{AssetClass, MarketInstrument};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SeedInstrumentCatalogEntry {
    pub instrument: NewInstrument,
    pub identifiers: Vec<NewInstrumentIdentifier>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InstrumentStatus {
    Active,
    Inactive,
    Delisted,
    Matured,
}

impl InstrumentStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Inactive => "inactive",
            Self::Delisted => "delisted",
            Self::Matured => "matured",
        }
    }
}

impl fmt::Display for InstrumentStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for InstrumentStatus {
    type Err = InstrumentModelError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "active" => Ok(Self::Active),
            "inactive" => Ok(Self::Inactive),
            "delisted" => Ok(Self::Delisted),
            "matured" => Ok(Self::Matured),
            _ => Err(InstrumentModelError::InvalidStatus),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InstrumentIdentifierType {
    ProviderId,
    Symbol,
    Ticker,
    Isin,
    Cusip,
    Figi,
    Sedol,
    Lei,
}

impl InstrumentIdentifierType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ProviderId => "provider_id",
            Self::Symbol => "symbol",
            Self::Ticker => "ticker",
            Self::Isin => "isin",
            Self::Cusip => "cusip",
            Self::Figi => "figi",
            Self::Sedol => "sedol",
            Self::Lei => "lei",
        }
    }
}

impl fmt::Display for InstrumentIdentifierType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, PartialEq, Eq)]
pub struct Instrument {
    pub id: i64,
    pub canonical_symbol: String,
    pub display_name: String,
    pub asset_class: String,
    pub region: String,
    pub country: Option<String>,
    pub currency: Option<String>,
    pub exchange: Option<String>,
    pub issuer_name: Option<String>,
    pub issuer_region: Option<String>,
    pub maturity_date: Option<NaiveDate>,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NewInstrument {
    pub canonical_symbol: String,
    pub display_name: String,
    pub asset_class: AssetClass,
    pub region: String,
    pub country: Option<String>,
    pub currency: Option<String>,
    pub exchange: Option<String>,
    pub issuer_name: Option<String>,
    pub issuer_region: Option<String>,
    pub maturity_date: Option<NaiveDate>,
    pub status: InstrumentStatus,
}

impl NewInstrument {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        canonical_symbol: impl Into<String>,
        display_name: impl Into<String>,
        asset_class: AssetClass,
        region: impl Into<String>,
        country: Option<String>,
        currency: Option<String>,
        exchange: Option<String>,
        issuer_name: Option<String>,
        issuer_region: Option<String>,
        maturity_date: Option<NaiveDate>,
    ) -> Result<Self, InstrumentModelError> {
        Ok(Self {
            canonical_symbol: normalized_upper_required(
                canonical_symbol.into(),
                InstrumentModelError::EmptySymbol,
            )?,
            display_name: normalized_required(
                display_name.into(),
                InstrumentModelError::EmptyDisplayName,
            )?,
            asset_class,
            region: normalized_upper_required(region.into(), InstrumentModelError::EmptyRegion)?,
            country: normalize_upper_optional(country),
            currency: normalize_upper_optional(currency),
            exchange: normalize_optional(exchange),
            issuer_name: normalize_optional(issuer_name),
            issuer_region: normalize_upper_optional(issuer_region),
            maturity_date,
            status: InstrumentStatus::Active,
        })
    }

    pub fn from_market_instrument(
        provider: impl Into<String>,
        instrument: MarketInstrument,
    ) -> Result<(Self, Vec<NewInstrumentIdentifier>), InstrumentModelError> {
        let provider = normalized_required(provider.into(), InstrumentModelError::EmptyProvider)?;
        let region = instrument
            .country
            .clone()
            .or_else(|| instrument.exchange.clone())
            .unwrap_or_else(|| "GLOBAL".to_owned());
        let maturity_date = parse_optional_date(instrument.maturity_date.as_deref())?;
        let catalog = Self::new(
            instrument.symbol.clone(),
            instrument.name,
            instrument.asset_class,
            region,
            instrument.country.clone(),
            instrument.currency,
            instrument.exchange,
            instrument.issuer.clone(),
            instrument.country,
            maturity_date,
        )?;
        let mut identifiers = vec![
            NewInstrumentIdentifier::new(
                InstrumentIdentifierType::ProviderId,
                instrument.provider_id,
                Some(provider.clone()),
                true,
            )?,
            NewInstrumentIdentifier::new(
                InstrumentIdentifierType::Symbol,
                instrument.symbol,
                Some(provider),
                true,
            )?,
        ];

        if let Some(isin) = instrument.isin {
            identifiers.push(NewInstrumentIdentifier::new(
                InstrumentIdentifierType::Isin,
                isin,
                None,
                false,
            )?);
        }
        if let Some(cusip) = instrument.cusip {
            identifiers.push(NewInstrumentIdentifier::new(
                InstrumentIdentifierType::Cusip,
                cusip,
                None,
                false,
            )?);
        }

        Ok((catalog, identifiers))
    }
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, PartialEq, Eq)]
pub struct InstrumentIdentifier {
    pub id: i64,
    pub instrument_id: i64,
    pub identifier_type: String,
    pub identifier_value: String,
    pub provider: Option<String>,
    pub is_primary: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NewInstrumentIdentifier {
    pub identifier_type: InstrumentIdentifierType,
    pub identifier_value: String,
    pub provider: Option<String>,
    pub is_primary: bool,
}

impl NewInstrumentIdentifier {
    pub fn new(
        identifier_type: InstrumentIdentifierType,
        identifier_value: impl Into<String>,
        provider: Option<String>,
        is_primary: bool,
    ) -> Result<Self, InstrumentModelError> {
        Ok(Self {
            identifier_type,
            identifier_value: normalized_required(
                identifier_value.into(),
                InstrumentModelError::EmptyIdentifierValue,
            )?,
            provider: normalize_optional(provider),
            is_primary,
        })
    }
}

pub fn live_seed_catalog_entries(
    provider: impl Into<String>,
    symbols: &[String],
) -> Result<Vec<SeedInstrumentCatalogEntry>, InstrumentModelError> {
    let provider = normalized_required(provider.into(), InstrumentModelError::EmptyProvider)?;
    symbols
        .iter()
        .map(|symbol| live_seed_catalog_entry(provider.as_str(), symbol.as_str()))
        .collect()
}

fn live_seed_catalog_entry(
    provider: &str,
    symbol: &str,
) -> Result<SeedInstrumentCatalogEntry, InstrumentModelError> {
    let symbol = normalized_upper_required(symbol.to_owned(), InstrumentModelError::EmptySymbol)?;
    let metadata = live_symbol_metadata(symbol.as_str());
    let instrument = NewInstrument::new(
        symbol.clone(),
        metadata.display_name,
        metadata.asset_class.clone(),
        metadata.region,
        metadata.country.clone(),
        metadata.currency.clone(),
        metadata.exchange,
        metadata.issuer_name,
        metadata.issuer_region,
        None,
    )?;
    let provider_id = provider_identifier(provider, symbol.as_str(), &metadata.asset_class);
    let mut identifiers = vec![
        NewInstrumentIdentifier::new(
            InstrumentIdentifierType::ProviderId,
            provider_id,
            Some(provider.to_owned()),
            true,
        )?,
        NewInstrumentIdentifier::new(
            InstrumentIdentifierType::Symbol,
            symbol.clone(),
            Some(provider.to_owned()),
            true,
        )?,
    ];

    if metadata.asset_class == AssetClass::Equity {
        identifiers.push(NewInstrumentIdentifier::new(
            InstrumentIdentifierType::Ticker,
            symbol,
            Some(provider.to_owned()),
            false,
        )?);
    }

    Ok(SeedInstrumentCatalogEntry {
        instrument,
        identifiers,
    })
}

#[derive(Debug, Clone)]
struct LiveSymbolMetadata {
    display_name: String,
    asset_class: AssetClass,
    region: String,
    country: Option<String>,
    currency: Option<String>,
    exchange: Option<String>,
    issuer_name: Option<String>,
    issuer_region: Option<String>,
}

fn live_symbol_metadata(symbol: &str) -> LiveSymbolMetadata {
    match symbol {
        "SPY" => equity_metadata("SPDR S&P 500 ETF Trust", "NYSE Arca", Some("State Street")),
        "NVDA" => equity_metadata("NVIDIA Corporation", "NASDAQ", Some("NVIDIA Corporation")),
        "VIX" => equity_metadata("Cboe Volatility Index", "Cboe", Some("Cboe Global Markets")),
        value if value.contains('/') => crypto_metadata(value),
        value => equity_metadata(value, "US", None),
    }
}

fn equity_metadata(
    display_name: &str,
    exchange: &str,
    issuer_name: Option<&str>,
) -> LiveSymbolMetadata {
    LiveSymbolMetadata {
        display_name: display_name.to_owned(),
        asset_class: AssetClass::Equity,
        region: "US".to_owned(),
        country: Some("US".to_owned()),
        currency: Some("USD".to_owned()),
        exchange: Some(exchange.to_owned()),
        issuer_name: issuer_name.map(str::to_owned),
        issuer_region: Some("US".to_owned()),
    }
}

fn crypto_metadata(symbol: &str) -> LiveSymbolMetadata {
    let currency = symbol
        .split_once('/')
        .map(|(_, quote)| quote.to_ascii_uppercase())
        .filter(|quote| !quote.is_empty())
        .unwrap_or_else(|| "USD".to_owned());

    LiveSymbolMetadata {
        display_name: format!("{symbol} Crypto Pair"),
        asset_class: AssetClass::Crypto,
        region: "GLOBAL".to_owned(),
        country: None,
        currency: Some(currency),
        exchange: Some("Crypto".to_owned()),
        issuer_name: None,
        issuer_region: None,
    }
}

fn provider_identifier(provider: &str, symbol: &str, asset_class: &AssetClass) -> String {
    if provider.eq_ignore_ascii_case("finnhub") && *asset_class == AssetClass::Crypto {
        if let Some((base, quote)) = symbol.split_once('/') {
            let quote = if quote == "USD" { "USDT" } else { quote };
            return format!("BINANCE:{base}{quote}");
        }
    }

    symbol.to_owned()
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum InstrumentModelError {
    #[error("provider cannot be empty")]
    EmptyProvider,
    #[error("symbol cannot be empty")]
    EmptySymbol,
    #[error("display name cannot be empty")]
    EmptyDisplayName,
    #[error("region cannot be empty")]
    EmptyRegion,
    #[error("identifier value cannot be empty")]
    EmptyIdentifierValue,
    #[error("invalid instrument status")]
    InvalidStatus,
    #[error("invalid maturity date")]
    InvalidMaturityDate,
}

fn normalized_required(
    value: String,
    error: InstrumentModelError,
) -> Result<String, InstrumentModelError> {
    let normalized = value.trim().to_owned();
    if normalized.is_empty() {
        Err(error)
    } else {
        Ok(normalized)
    }
}

fn normalized_upper_required(
    value: String,
    error: InstrumentModelError,
) -> Result<String, InstrumentModelError> {
    normalized_required(value, error).map(|value| value.to_ascii_uppercase())
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn normalize_upper_optional(value: Option<String>) -> Option<String> {
    normalize_optional(value).map(|value| value.to_ascii_uppercase())
}

fn parse_optional_date(value: Option<&str>) -> Result<Option<NaiveDate>, InstrumentModelError> {
    match value.map(str::trim).filter(|value| !value.is_empty()) {
        Some(value) => NaiveDate::parse_from_str(value, "%Y-%m-%d")
            .map(Some)
            .map_err(|_| InstrumentModelError::InvalidMaturityDate),
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        live_seed_catalog_entries, InstrumentIdentifierType, InstrumentModelError,
        InstrumentStatus, NewInstrument, NewInstrumentIdentifier,
    };
    use crate::market_data::{AssetClass, MarketInstrument};

    #[test]
    fn normalizes_instrument_catalog_metadata() {
        let instrument = NewInstrument::new(
            " spy ",
            " SPDR S&P 500 ETF ",
            AssetClass::Equity,
            " us ",
            Some(" us ".to_owned()),
            Some(" usd ".to_owned()),
            Some(" arcx ".to_owned()),
            None,
            None,
            None,
        )
        .expect("instrument metadata should be valid");

        assert_eq!(instrument.canonical_symbol, "SPY");
        assert_eq!(instrument.display_name, "SPDR S&P 500 ETF");
        assert_eq!(instrument.region, "US");
        assert_eq!(instrument.country, Some("US".to_owned()));
        assert_eq!(instrument.currency, Some("USD".to_owned()));
        assert_eq!(instrument.exchange, Some("arcx".to_owned()));
        assert_eq!(instrument.status, InstrumentStatus::Active);
    }

    #[test]
    fn rejects_empty_required_instrument_fields() {
        let error = NewInstrument::new(
            " ",
            "Name",
            AssetClass::Equity,
            "US",
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .expect_err("empty symbol should be rejected");

        assert_eq!(error, InstrumentModelError::EmptySymbol);
    }

    #[test]
    fn normalizes_identifier_metadata() {
        let identifier = NewInstrumentIdentifier::new(
            InstrumentIdentifierType::Isin,
            " US78462F1030 ",
            Some(" provider ".to_owned()),
            false,
        )
        .expect("identifier should be valid");

        assert_eq!(identifier.identifier_type.as_str(), "isin");
        assert_eq!(identifier.identifier_value, "US78462F1030");
        assert_eq!(identifier.provider, Some("provider".to_owned()));
    }

    #[test]
    fn converts_provider_instrument_to_catalog_and_identifiers() {
        let provider_instrument = MarketInstrument {
            provider_id: "provider-spy".to_owned(),
            symbol: "spy".to_owned(),
            name: "SPDR S&P 500 ETF".to_owned(),
            asset_class: AssetClass::Equity,
            currency: Some("usd".to_owned()),
            country: Some("us".to_owned()),
            exchange: Some("ARCX".to_owned()),
            isin: Some("US78462F1030".to_owned()),
            cusip: None,
            issuer: Some("State Street".to_owned()),
            maturity_date: None,
        };

        let (catalog, identifiers) =
            NewInstrument::from_market_instrument("http-json", provider_instrument)
                .expect("provider metadata should convert");

        assert_eq!(catalog.canonical_symbol, "SPY");
        assert_eq!(catalog.asset_class, AssetClass::Equity);
        assert_eq!(catalog.issuer_name, Some("State Street".to_owned()));
        assert_eq!(identifiers.len(), 3);
        assert!(identifiers
            .iter()
            .any(|identifier| identifier.identifier_type == InstrumentIdentifierType::ProviderId));
    }

    #[test]
    fn rejects_invalid_provider_maturity_date() {
        let provider_instrument = MarketInstrument {
            provider_id: "bond-1".to_owned(),
            symbol: "BOND1".to_owned(),
            name: "Issuer 2030".to_owned(),
            asset_class: AssetClass::CorporateBond,
            currency: Some("USD".to_owned()),
            country: Some("US".to_owned()),
            exchange: None,
            isin: None,
            cusip: None,
            issuer: Some("Issuer".to_owned()),
            maturity_date: Some("not-a-date".to_owned()),
        };

        let error = NewInstrument::from_market_instrument("provider", provider_instrument)
            .expect_err("invalid maturity date should be rejected");

        assert_eq!(error, InstrumentModelError::InvalidMaturityDate);
    }

    #[test]
    fn builds_seed_catalog_entries_for_default_live_symbols() {
        let symbols = vec![
            "SPY".to_owned(),
            "BTC/USD".to_owned(),
            "NVDA".to_owned(),
            "ETH/USD".to_owned(),
            "VIX".to_owned(),
        ];

        let entries =
            live_seed_catalog_entries("finnhub", &symbols).expect("seed metadata should build");

        assert_eq!(entries.len(), 5);
        let spy = entries
            .iter()
            .find(|entry| entry.instrument.canonical_symbol == "SPY")
            .expect("SPY seed should exist");
        assert_eq!(spy.instrument.asset_class, AssetClass::Equity);
        assert_eq!(spy.instrument.region, "US");
        assert!(spy.identifiers.iter().any(|identifier| {
            identifier.identifier_type == InstrumentIdentifierType::ProviderId
                && identifier.identifier_value == "SPY"
        }));

        let btc = entries
            .iter()
            .find(|entry| entry.instrument.canonical_symbol == "BTC/USD")
            .expect("BTC/USD seed should exist");
        assert_eq!(btc.instrument.asset_class, AssetClass::Crypto);
        assert_eq!(btc.instrument.region, "GLOBAL");
        assert_eq!(btc.instrument.currency, Some("USD".to_owned()));
        assert!(btc.identifiers.iter().any(|identifier| {
            identifier.identifier_type == InstrumentIdentifierType::ProviderId
                && identifier.identifier_value == "BINANCE:BTCUSDT"
        }));
    }
}
