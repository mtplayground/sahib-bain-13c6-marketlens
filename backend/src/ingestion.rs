#![allow(dead_code)]

use chrono::{DateTime, Utc};
use rust_decimal::{prelude::FromPrimitive, Decimal};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    db::Database,
    market_data::{
        AssetClass, LatestQuoteRequest, MarketDataError, MarketDataProvider, MarketQuote,
        ProviderInstrumentRef,
    },
    redis::{RedisClient, RedisError},
    series::{NewPriceSeries, NewPriceSeriesPoint, SeriesInterval, SeriesModelError},
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IngestionInstrument {
    pub provider_id: String,
    pub symbol: String,
    pub asset_class: AssetClass,
    pub interval: SeriesInterval,
    pub currency: Option<String>,
}

impl IngestionInstrument {
    pub fn new(
        provider_id: impl Into<String>,
        symbol: impl Into<String>,
        asset_class: AssetClass,
        interval: SeriesInterval,
        currency: Option<String>,
    ) -> Result<Self, IngestionError> {
        Ok(Self {
            provider_id: normalize_required(provider_id.into(), "provider_id")?,
            symbol: normalize_required(symbol.into(), "symbol")?,
            asset_class,
            interval,
            currency: normalize_optional(currency),
        })
    }

    fn provider_ref(&self) -> ProviderInstrumentRef {
        ProviderInstrumentRef {
            provider_id: self.provider_id.clone(),
            symbol: Some(self.symbol.clone()),
            asset_class: self.asset_class.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IngestionBatch {
    pub instruments: Vec<IngestionInstrument>,
}

impl IngestionBatch {
    pub fn new(instruments: Vec<IngestionInstrument>) -> Result<Self, IngestionError> {
        if instruments.is_empty() {
            return Err(IngestionError::EmptyBatch);
        }

        Ok(Self { instruments })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IngestionReport {
    pub requested: usize,
    pub persisted: usize,
    pub published: usize,
    pub redis_subscribers: u64,
}

pub struct IngestionWorker<P> {
    provider: P,
    database: Database,
    redis: RedisClient,
}

impl<P> IngestionWorker<P>
where
    P: MarketDataProvider,
{
    pub fn new(provider: P, database: Database, redis: RedisClient) -> Self {
        Self {
            provider,
            database,
            redis,
        }
    }

    pub async fn run_once(&self, batch: IngestionBatch) -> Result<IngestionReport, IngestionError> {
        let requested = batch.instruments.len();
        let request = LatestQuoteRequest::new(
            batch
                .instruments
                .iter()
                .map(IngestionInstrument::provider_ref)
                .collect(),
        )?;
        let quotes = self.provider.latest_quotes(request).await?;
        let mut persisted = 0_usize;
        let mut published = 0_usize;
        let mut redis_subscribers = 0_u64;

        for quote in quotes {
            let Some(instrument) = batch
                .instruments
                .iter()
                .find(|instrument| instrument.provider_id == quote.instrument.provider_id)
            else {
                tracing::warn!(
                    provider_id = %quote.instrument.provider_id,
                    "provider returned quote for an instrument outside this ingestion batch"
                );
                continue;
            };

            let normalized = normalize_quote(self.provider.name(), instrument, &quote)?;
            let series_id = upsert_series(&self.database, &normalized.series).await?;
            upsert_point(&self.database, series_id, &normalized.point).await?;
            persisted += 1;

            let payload = normalized.tick_payload(series_id);
            let payload_json = serde_json::to_string(&payload)?;
            redis_subscribers += self
                .redis
                .publish_market_tick(instrument.symbol.as_str(), payload_json.as_str())
                .await?;
            published += 1;
        }

        Ok(IngestionReport {
            requested,
            persisted,
            published,
            redis_subscribers,
        })
    }
}

#[derive(Debug, Clone)]
struct NormalizedQuote {
    series: NewPriceSeries,
    point: NewPriceSeriesPoint,
    symbol: String,
    provider: String,
    provider_instrument_id: String,
    asset_class: AssetClass,
    price: Decimal,
    currency: String,
    as_of: DateTime<Utc>,
    bid: Option<Decimal>,
    ask: Option<Decimal>,
}

impl NormalizedQuote {
    fn tick_payload(&self, series_id: i64) -> MarketTickPayload {
        MarketTickPayload {
            r#type: "market.tick",
            provider: self.provider.clone(),
            provider_instrument_id: self.provider_instrument_id.clone(),
            series_id,
            symbol: self.symbol.clone(),
            asset_class: self.asset_class.as_str(),
            price: self.price,
            currency: self.currency.clone(),
            as_of: self.as_of,
            bid: self.bid,
            ask: self.ask,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MarketTickPayload {
    pub r#type: &'static str,
    pub provider: String,
    pub provider_instrument_id: String,
    pub series_id: i64,
    pub symbol: String,
    pub asset_class: &'static str,
    pub price: Decimal,
    pub currency: String,
    pub as_of: DateTime<Utc>,
    pub bid: Option<Decimal>,
    pub ask: Option<Decimal>,
}

fn normalize_quote(
    provider_name: &str,
    instrument: &IngestionInstrument,
    quote: &MarketQuote,
) -> Result<NormalizedQuote, IngestionError> {
    let price = decimal_from_f64("price", quote.price)?;
    let bid = optional_decimal_from_f64("bid", quote.bid)?;
    let ask = optional_decimal_from_f64("ask", quote.ask)?;
    let currency = normalize_required(quote.currency.clone(), "currency")?;
    let series = NewPriceSeries::new(
        provider_name,
        instrument.provider_id.as_str(),
        instrument.symbol.as_str(),
        instrument.asset_class.clone(),
        instrument.interval,
        instrument.currency.clone().or_else(|| Some(currency.clone())),
    )?;
    let point = NewPriceSeriesPoint::close_only(quote.as_of, price)?;

    Ok(NormalizedQuote {
        series,
        point,
        symbol: instrument.symbol.clone(),
        provider: provider_name.to_owned(),
        provider_instrument_id: instrument.provider_id.clone(),
        asset_class: instrument.asset_class.clone(),
        price,
        currency,
        as_of: quote.as_of,
        bid,
        ask,
    })
}

async fn upsert_series(
    database: &Database,
    series: &NewPriceSeries,
) -> Result<i64, IngestionError> {
    let id = sqlx::query_scalar::<_, i64>(
        r#"
        INSERT INTO price_series_cache (
            provider,
            provider_instrument_id,
            symbol,
            asset_class,
            interval,
            currency,
            first_observed_at,
            last_observed_at,
            last_refreshed_at,
            source_updated_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, NULL, NULL, NOW(), NULL)
        ON CONFLICT (provider, provider_instrument_id, interval) DO UPDATE
        SET
            symbol = EXCLUDED.symbol,
            asset_class = EXCLUDED.asset_class,
            currency = EXCLUDED.currency,
            last_refreshed_at = NOW()
        RETURNING id
        "#,
    )
    .bind(series.provider.as_str())
    .bind(series.provider_instrument_id.as_str())
    .bind(series.symbol.as_str())
    .bind(series.asset_class.as_str())
    .bind(series.interval.as_str())
    .bind(series.currency.as_deref())
    .fetch_one(database.pool())
    .await?;

    Ok(id)
}

async fn upsert_point(
    database: &Database,
    series_id: i64,
    point: &NewPriceSeriesPoint,
) -> Result<(), IngestionError> {
    sqlx::query(
        r#"
        INSERT INTO price_series_points (
            series_id,
            observed_at,
            open_price,
            high_price,
            low_price,
            close_price,
            volume,
            trade_count,
            vwap,
            is_final,
            provider_updated_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
        ON CONFLICT (series_id, observed_at) DO UPDATE
        SET
            open_price = EXCLUDED.open_price,
            high_price = EXCLUDED.high_price,
            low_price = EXCLUDED.low_price,
            close_price = EXCLUDED.close_price,
            volume = EXCLUDED.volume,
            trade_count = EXCLUDED.trade_count,
            vwap = EXCLUDED.vwap,
            is_final = EXCLUDED.is_final,
            provider_updated_at = EXCLUDED.provider_updated_at,
            ingested_at = NOW()
        "#,
    )
    .bind(series_id)
    .bind(point.observed_at)
    .bind(point.open_price)
    .bind(point.high_price)
    .bind(point.low_price)
    .bind(point.close_price)
    .bind(point.volume)
    .bind(point.trade_count)
    .bind(point.vwap)
    .bind(point.is_final)
    .bind(point.provider_updated_at)
    .execute(database.pool())
    .await?;

    sqlx::query(
        r#"
        UPDATE price_series_cache
        SET
            first_observed_at = LEAST(COALESCE(first_observed_at, $2), $2),
            last_observed_at = GREATEST(COALESCE(last_observed_at, $2), $2),
            source_updated_at = GREATEST(COALESCE(source_updated_at, $3), $3)
        WHERE id = $1
        "#,
    )
    .bind(series_id)
    .bind(point.observed_at)
    .bind(point.provider_updated_at.unwrap_or(point.observed_at))
    .execute(database.pool())
    .await?;

    Ok(())
}

fn decimal_from_f64(field: &'static str, value: f64) -> Result<Decimal, IngestionError> {
    Decimal::from_f64(value).ok_or(IngestionError::InvalidNumber { field })
}

fn optional_decimal_from_f64(
    field: &'static str,
    value: Option<f64>,
) -> Result<Option<Decimal>, IngestionError> {
    value
        .map(|value| decimal_from_f64(field, value))
        .transpose()
}

fn normalize_required(value: String, field: &'static str) -> Result<String, IngestionError> {
    let normalized = value.trim().to_owned();
    if normalized.is_empty() {
        Err(IngestionError::EmptyField { field })
    } else {
        Ok(normalized)
    }
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

#[derive(Debug, Error)]
pub enum IngestionError {
    #[error("ingestion batch must include at least one instrument")]
    EmptyBatch,
    #[error("{field} cannot be empty")]
    EmptyField { field: &'static str },
    #[error("provider returned invalid numeric field {field}")]
    InvalidNumber { field: &'static str },
    #[error("{0}")]
    Provider(#[from] MarketDataError),
    #[error("{0}")]
    Series(#[from] SeriesModelError),
    #[error("database ingestion failed: {0}")]
    Database(#[from] sqlx::Error),
    #[error("redis publish failed: {0}")]
    Redis(#[from] RedisError),
    #[error("failed to serialize market tick: {0}")]
    Serialize(#[from] serde_json::Error),
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use crate::{
        market_data::{AssetClass, MarketQuote, ProviderInstrumentRef},
        series::SeriesInterval,
    };

    use super::{
        normalize_quote, IngestionBatch, IngestionError, IngestionInstrument,
    };

    fn as_of() -> chrono::DateTime<chrono::Utc> {
        let Some(timestamp) = chrono::Utc
            .with_ymd_and_hms(2026, 7, 20, 15, 30, 0)
            .single()
        else {
            panic!("test timestamp should be valid");
        };
        timestamp
    }

    fn instrument() -> IngestionInstrument {
        IngestionInstrument::new(
            "provider-spy",
            "SPY",
            AssetClass::Equity,
            SeriesInterval::Tick,
            Some("USD".to_owned()),
        )
        .expect("instrument should be valid")
    }

    #[test]
    fn rejects_empty_batches() {
        assert!(matches!(
            IngestionBatch::new(Vec::new()),
            Err(IngestionError::EmptyBatch)
        ));
    }

    #[test]
    fn normalizes_provider_quote_to_series_point_and_tick() {
        let quote = MarketQuote {
            instrument: ProviderInstrumentRef {
                provider_id: "provider-spy".to_owned(),
                symbol: Some("SPY".to_owned()),
                asset_class: AssetClass::Equity,
            },
            price: 551.25,
            currency: "USD".to_owned(),
            as_of: as_of(),
            bid: Some(551.2),
            ask: Some(551.3),
            yield_to_maturity: None,
            duration: None,
        };
        let normalized = normalize_quote("http-json", &instrument(), &quote)
            .expect("quote should normalize");

        assert_eq!(normalized.series.provider, "http-json");
        assert_eq!(normalized.series.provider_instrument_id, "provider-spy");
        assert_eq!(normalized.series.symbol, "SPY");
        assert_eq!(normalized.point.observed_at, as_of());
        assert_eq!(normalized.tick_payload(42).series_id, 42);
        assert_eq!(normalized.tick_payload(42).asset_class, "equity");
    }

    #[test]
    fn rejects_nan_provider_prices() {
        let quote = MarketQuote {
            instrument: ProviderInstrumentRef {
                provider_id: "provider-spy".to_owned(),
                symbol: Some("SPY".to_owned()),
                asset_class: AssetClass::Equity,
            },
            price: f64::NAN,
            currency: "USD".to_owned(),
            as_of: as_of(),
            bid: None,
            ask: None,
            yield_to_maturity: None,
            duration: None,
        };

        assert!(matches!(
            normalize_quote("http-json", &instrument(), &quote),
            Err(IngestionError::InvalidNumber { field: "price" })
        ));
    }
}
