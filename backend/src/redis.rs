use std::time::Instant;

use ::redis::{AsyncCommands, Client};
use serde::Serialize;
use thiserror::Error;

use crate::config::AppConfig;

#[derive(Clone, Debug)]
pub struct RedisClient {
    client: Client,
}

#[derive(Debug, Error)]
pub enum RedisError {
    #[error("failed to configure Redis client: {0}")]
    Configure(#[source] ::redis::RedisError),
    #[error("failed to communicate with Redis: {0}")]
    Command(#[from] ::redis::RedisError),
}

#[derive(Debug, Serialize)]
pub struct RedisHealth {
    pub status: &'static str,
    pub latency_ms: u128,
    pub error: Option<String>,
}

impl RedisClient {
    pub fn connect(config: &AppConfig) -> Result<Self, RedisError> {
        let client = Client::open(config.redis_url.as_str()).map_err(RedisError::Configure)?;
        Ok(Self { client })
    }

    pub async fn health_check(&self) -> RedisHealth {
        let started_at = Instant::now();
        let result = async {
            let mut connection = self.client.get_multiplexed_async_connection().await?;
            ::redis::cmd("PING")
                .query_async::<String>(&mut connection)
                .await
        }
        .await;
        let latency_ms = started_at.elapsed().as_millis();

        match result {
            Ok(response) if response == "PONG" => RedisHealth {
                status: "ok",
                latency_ms,
                error: None,
            },
            Ok(response) => RedisHealth {
                status: "degraded",
                latency_ms,
                error: Some(format!("unexpected Redis PING response: {response}")),
            },
            Err(error) => RedisHealth {
                status: "down",
                latency_ms,
                error: Some(error.to_string()),
            },
        }
    }

    #[allow(dead_code)]
    pub async fn pubsub(&self) -> Result<::redis::aio::PubSub, RedisError> {
        self.client
            .get_async_pubsub()
            .await
            .map_err(RedisError::Command)
    }

    #[allow(dead_code)]
    pub async fn publish(&self, channel: &str, payload: &str) -> Result<u64, RedisError> {
        let mut connection = self.client.get_multiplexed_async_connection().await?;
        let subscribers = connection.publish(channel, payload).await?;
        Ok(subscribers)
    }

    #[allow(dead_code)]
    pub async fn publish_market_tick(
        &self,
        instrument_symbol: &str,
        payload: &str,
    ) -> Result<u64, RedisError> {
        self.publish(&channels::market_ticks(instrument_symbol), payload)
            .await
    }

    #[allow(dead_code)]
    pub async fn publish_alert_event(&self, payload: &str) -> Result<u64, RedisError> {
        self.publish(channels::ALERT_EVENTS, payload).await
    }

    #[allow(dead_code)]
    pub async fn publish_user_alert_event(
        &self,
        user_sub: &str,
        payload: &str,
    ) -> Result<u64, RedisError> {
        self.publish(&channels::user_alert_events(user_sub), payload)
            .await
    }
}

pub mod channels {
    pub const NAMESPACE: &str = "marketlens";
    pub const MARKET_TICKS_PATTERN: &str = "marketlens:market:ticks:*";
    pub const ALERT_EVENTS: &str = "marketlens:alerts:events";
    pub const USER_ALERT_EVENTS_PATTERN: &str = "marketlens:alerts:users:*";

    pub fn market_ticks(instrument_symbol: &str) -> String {
        format!("{NAMESPACE}:market:ticks:{}", channel_segment(instrument_symbol))
    }

    pub fn user_alert_events(user_sub: &str) -> String {
        format!("{NAMESPACE}:alerts:users:{}", channel_segment(user_sub))
    }

    fn channel_segment(value: &str) -> String {
        let mut normalized = String::with_capacity(value.len());

        for character in value.trim().chars() {
            if character.is_ascii_alphanumeric() {
                normalized.push(character.to_ascii_lowercase());
            } else if matches!(character, '-' | '_' | '.') {
                normalized.push(character);
            } else if !normalized.ends_with('-') {
                normalized.push('-');
            }
        }

        let trimmed = normalized.trim_matches('-');
        if trimmed.is_empty() {
            "unknown".to_owned()
        } else {
            trimmed.to_owned()
        }
    }

    #[cfg(test)]
    mod tests {
        use super::{market_ticks, user_alert_events, ALERT_EVENTS, MARKET_TICKS_PATTERN};

        #[test]
        fn builds_market_tick_channels() {
            assert_eq!(market_ticks("BTC/USD"), "marketlens:market:ticks:btc-usd");
            assert_eq!(market_ticks("  SPY  "), "marketlens:market:ticks:spy");
        }

        #[test]
        fn builds_alert_channels() {
            assert_eq!(ALERT_EVENTS, "marketlens:alerts:events");
            assert_eq!(
                user_alert_events("auth0|User 123"),
                "marketlens:alerts:users:auth0-user-123"
            );
        }

        #[test]
        fn exposes_subscription_patterns() {
            assert_eq!(MARKET_TICKS_PATTERN, "marketlens:market:ticks:*");
        }
    }
}
