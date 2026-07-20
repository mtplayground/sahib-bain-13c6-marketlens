#![allow(dead_code)]

use std::{fmt, str::FromStr};

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use thiserror::Error;

use crate::market_data::AssetClass;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SeriesInterval {
    Tick,
    OneMinute,
    FiveMinutes,
    FifteenMinutes,
    ThirtyMinutes,
    OneHour,
    FourHours,
    OneDay,
    OneWeek,
    OneMonth,
}

impl SeriesInterval {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Tick => "tick",
            Self::OneMinute => "1m",
            Self::FiveMinutes => "5m",
            Self::FifteenMinutes => "15m",
            Self::ThirtyMinutes => "30m",
            Self::OneHour => "1h",
            Self::FourHours => "4h",
            Self::OneDay => "1d",
            Self::OneWeek => "1w",
            Self::OneMonth => "1mo",
        }
    }
}

impl fmt::Display for SeriesInterval {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for SeriesInterval {
    type Err = SeriesModelError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "tick" => Ok(Self::Tick),
            "1m" => Ok(Self::OneMinute),
            "5m" => Ok(Self::FiveMinutes),
            "15m" => Ok(Self::FifteenMinutes),
            "30m" => Ok(Self::ThirtyMinutes),
            "1h" => Ok(Self::OneHour),
            "4h" => Ok(Self::FourHours),
            "1d" => Ok(Self::OneDay),
            "1w" => Ok(Self::OneWeek),
            "1mo" => Ok(Self::OneMonth),
            _ => Err(SeriesModelError::InvalidInterval),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RefreshStatus {
    Idle,
    Refreshing,
    Backoff,
    Disabled,
}

impl RefreshStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Refreshing => "refreshing",
            Self::Backoff => "backoff",
            Self::Disabled => "disabled",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SeriesKey {
    pub provider: String,
    pub provider_instrument_id: String,
    pub interval: SeriesInterval,
}

impl SeriesKey {
    pub fn new(
        provider: impl Into<String>,
        provider_instrument_id: impl Into<String>,
        interval: SeriesInterval,
    ) -> Result<Self, SeriesModelError> {
        Ok(Self {
            provider: normalized_required(provider.into(), SeriesModelError::EmptyProvider)?,
            provider_instrument_id: normalized_required(
                provider_instrument_id.into(),
                SeriesModelError::EmptyProviderInstrumentId,
            )?,
            interval,
        })
    }
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, PartialEq)]
pub struct PriceSeries {
    pub id: i64,
    pub provider: String,
    pub provider_instrument_id: String,
    pub symbol: String,
    pub asset_class: String,
    pub interval: String,
    pub currency: Option<String>,
    pub first_observed_at: Option<DateTime<Utc>>,
    pub last_observed_at: Option<DateTime<Utc>>,
    pub last_refreshed_at: Option<DateTime<Utc>>,
    pub source_updated_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NewPriceSeries {
    pub provider: String,
    pub provider_instrument_id: String,
    pub symbol: String,
    pub asset_class: AssetClass,
    pub interval: SeriesInterval,
    pub currency: Option<String>,
}

impl NewPriceSeries {
    pub fn new(
        provider: impl Into<String>,
        provider_instrument_id: impl Into<String>,
        symbol: impl Into<String>,
        asset_class: AssetClass,
        interval: SeriesInterval,
        currency: Option<String>,
    ) -> Result<Self, SeriesModelError> {
        Ok(Self {
            provider: normalized_required(provider.into(), SeriesModelError::EmptyProvider)?,
            provider_instrument_id: normalized_required(
                provider_instrument_id.into(),
                SeriesModelError::EmptyProviderInstrumentId,
            )?,
            symbol: normalized_required(symbol.into(), SeriesModelError::EmptySymbol)?,
            asset_class,
            interval,
            currency: normalize_optional(currency),
        })
    }
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, PartialEq)]
pub struct PriceSeriesPoint {
    pub series_id: i64,
    pub observed_at: DateTime<Utc>,
    pub open_price: Option<Decimal>,
    pub high_price: Option<Decimal>,
    pub low_price: Option<Decimal>,
    pub close_price: Decimal,
    pub volume: Option<Decimal>,
    pub trade_count: Option<i64>,
    pub vwap: Option<Decimal>,
    pub is_final: bool,
    pub provider_updated_at: Option<DateTime<Utc>>,
    pub ingested_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NewPriceSeriesPoint {
    pub observed_at: DateTime<Utc>,
    pub open_price: Option<Decimal>,
    pub high_price: Option<Decimal>,
    pub low_price: Option<Decimal>,
    pub close_price: Decimal,
    pub volume: Option<Decimal>,
    pub trade_count: Option<i64>,
    pub vwap: Option<Decimal>,
    pub is_final: bool,
    pub provider_updated_at: Option<DateTime<Utc>>,
}

impl NewPriceSeriesPoint {
    pub fn ohlc(
        observed_at: DateTime<Utc>,
        open_price: Decimal,
        high_price: Decimal,
        low_price: Decimal,
        close_price: Decimal,
    ) -> Result<Self, SeriesModelError> {
        let point = Self {
            observed_at,
            open_price: Some(open_price),
            high_price: Some(high_price),
            low_price: Some(low_price),
            close_price,
            volume: None,
            trade_count: None,
            vwap: None,
            is_final: true,
            provider_updated_at: None,
        };
        point.validate()?;
        Ok(point)
    }

    pub fn close_only(
        observed_at: DateTime<Utc>,
        close_price: Decimal,
    ) -> Result<Self, SeriesModelError> {
        let point = Self {
            observed_at,
            open_price: None,
            high_price: None,
            low_price: None,
            close_price,
            volume: None,
            trade_count: None,
            vwap: None,
            is_final: true,
            provider_updated_at: None,
        };
        point.validate()?;
        Ok(point)
    }

    pub fn with_volume(mut self, volume: Option<Decimal>) -> Result<Self, SeriesModelError> {
        self.volume = volume;
        self.validate()?;
        Ok(self)
    }

    pub fn validate(&self) -> Result<(), SeriesModelError> {
        validate_positive(self.close_price, SeriesModelError::InvalidClose)?;
        validate_optional_positive(self.open_price, SeriesModelError::InvalidOpen)?;
        validate_optional_positive(self.high_price, SeriesModelError::InvalidHigh)?;
        validate_optional_positive(self.low_price, SeriesModelError::InvalidLow)?;
        validate_optional_positive(self.vwap, SeriesModelError::InvalidVwap)?;
        validate_optional_non_negative(self.volume, SeriesModelError::InvalidVolume)?;

        if matches!(self.trade_count, Some(value) if value < 0) {
            return Err(SeriesModelError::InvalidTradeCount);
        }

        if let (Some(high), Some(low)) = (self.high_price, self.low_price) {
            if high < low {
                return Err(SeriesModelError::HighBelowLow);
            }
            if self.close_price > high || self.close_price < low {
                return Err(SeriesModelError::CloseOutsideRange);
            }
            if let Some(open) = self.open_price {
                if open > high || open < low {
                    return Err(SeriesModelError::OpenOutsideRange);
                }
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, PartialEq)]
pub struct PriceSeriesRefreshState {
    pub series_id: i64,
    pub status: String,
    pub provider_cursor: Option<String>,
    pub next_refresh_after: Option<DateTime<Utc>>,
    pub last_success_at: Option<DateTime<Utc>>,
    pub last_error_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SeriesModelError {
    #[error("provider cannot be empty")]
    EmptyProvider,
    #[error("provider instrument id cannot be empty")]
    EmptyProviderInstrumentId,
    #[error("symbol cannot be empty")]
    EmptySymbol,
    #[error("invalid series interval")]
    InvalidInterval,
    #[error("close price must be positive and finite")]
    InvalidClose,
    #[error("open price must be positive and finite")]
    InvalidOpen,
    #[error("high price must be positive and finite")]
    InvalidHigh,
    #[error("low price must be positive and finite")]
    InvalidLow,
    #[error("vwap must be positive and finite")]
    InvalidVwap,
    #[error("volume must be non-negative and finite")]
    InvalidVolume,
    #[error("trade count cannot be negative")]
    InvalidTradeCount,
    #[error("high price cannot be below low price")]
    HighBelowLow,
    #[error("close price must be inside high/low range")]
    CloseOutsideRange,
    #[error("open price must be inside high/low range")]
    OpenOutsideRange,
}

fn normalized_required(value: String, error: SeriesModelError) -> Result<String, SeriesModelError> {
    let normalized = value.trim().to_owned();
    if normalized.is_empty() {
        Err(error)
    } else {
        Ok(normalized)
    }
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn validate_positive(value: Decimal, error: SeriesModelError) -> Result<(), SeriesModelError> {
    if value > Decimal::ZERO {
        Ok(())
    } else {
        Err(error)
    }
}

fn validate_optional_positive(
    value: Option<Decimal>,
    error: SeriesModelError,
) -> Result<(), SeriesModelError> {
    match value {
        Some(value) => validate_positive(value, error),
        None => Ok(()),
    }
}

fn validate_optional_non_negative(
    value: Option<Decimal>,
    error: SeriesModelError,
) -> Result<(), SeriesModelError> {
    match value {
        Some(value) if value >= Decimal::ZERO => Ok(()),
        Some(_) => Err(error),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use chrono::TimeZone;
    use rust_decimal::Decimal;

    use crate::market_data::AssetClass;

    use super::{
        NewPriceSeries, NewPriceSeriesPoint, SeriesInterval, SeriesKey, SeriesModelError,
    };

    fn observed_at() -> chrono::DateTime<chrono::Utc> {
        let Some(timestamp) = chrono::Utc
            .with_ymd_and_hms(2026, 7, 20, 14, 30, 0)
            .single()
        else {
            panic!("test timestamp should be valid");
        };
        timestamp
    }

    fn dec(unscaled: i64, scale: u32) -> Decimal {
        Decimal::new(unscaled, scale)
    }

    #[test]
    fn serializes_intervals_to_database_values() {
        assert_eq!(SeriesInterval::OneMinute.as_str(), "1m");
        assert_eq!(SeriesInterval::OneMonth.as_str(), "1mo");
        assert_eq!(
            SeriesInterval::from_str("4h"),
            Ok(SeriesInterval::FourHours)
        );
        assert_eq!(
            SeriesInterval::from_str("2h"),
            Err(SeriesModelError::InvalidInterval)
        );
    }

    #[test]
    fn normalizes_series_identity() {
        let series = NewPriceSeries::new(
            " provider ",
            " instrument ",
            " spy ",
            AssetClass::Equity,
            SeriesInterval::OneDay,
            Some(" USD ".to_owned()),
        )
        .expect("series identity should be valid");

        assert_eq!(series.provider, "provider");
        assert_eq!(series.provider_instrument_id, "instrument");
        assert_eq!(series.symbol, "spy");
        assert_eq!(series.currency, Some("USD".to_owned()));
    }

    #[test]
    fn rejects_empty_series_keys() {
        assert_eq!(
            SeriesKey::new(" ", "abc", SeriesInterval::OneDay),
            Err(SeriesModelError::EmptyProvider)
        );
        assert_eq!(
            SeriesKey::new("provider", " ", SeriesInterval::OneDay),
            Err(SeriesModelError::EmptyProviderInstrumentId)
        );
    }

    #[test]
    fn validates_ohlc_ranges() {
        assert!(NewPriceSeriesPoint::ohlc(
            observed_at(),
            dec(1000, 2),
            dec(1200, 2),
            dec(950, 2),
            dec(1100, 2)
        )
        .is_ok());
        assert_eq!(
            NewPriceSeriesPoint::ohlc(
                observed_at(),
                dec(1000, 2),
                dec(900, 2),
                dec(950, 2),
                dec(975, 2)
            ),
            Err(SeriesModelError::HighBelowLow)
        );
        assert_eq!(
            NewPriceSeriesPoint::ohlc(
                observed_at(),
                dec(1000, 2),
                dec(1200, 2),
                dec(950, 2),
                dec(1250, 2)
            ),
            Err(SeriesModelError::CloseOutsideRange)
        );
    }

    #[test]
    fn validates_close_only_and_volume() {
        let point = NewPriceSeriesPoint::close_only(observed_at(), dec(10125, 2))
            .expect("close-only point should be valid")
            .with_volume(Some(Decimal::ZERO));
        assert!(point.is_ok());

        let point = NewPriceSeriesPoint::close_only(observed_at(), dec(10125, 2))
            .expect("close-only point should be valid")
            .with_volume(Some(dec(-100, 2)));
        assert_eq!(point, Err(SeriesModelError::InvalidVolume));
    }
}
