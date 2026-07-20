#![allow(dead_code)]

use std::{fmt, str::FromStr};

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use thiserror::Error;

use crate::market_data::{AssetClass, MarketInstrument};

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
        InstrumentIdentifierType, InstrumentModelError, InstrumentStatus, NewInstrument,
        NewInstrumentIdentifier,
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
}
