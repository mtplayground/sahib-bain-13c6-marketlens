use std::time::Duration;

use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::config::AppConfig;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EmailDelivery {
    Sent { message_id: Option<String> },
    SkippedNotConfigured,
}

#[derive(Debug, Error)]
pub enum EmailError {
    #[error("email service is rate limited")]
    RateLimited,
    #[error("timed out sending email")]
    Timeout,
    #[error("email service request failed: {0}")]
    Request(#[source] reqwest::Error),
    #[error("email service returned {status}: {body}")]
    Service { status: StatusCode, body: String },
}

#[derive(Debug, Serialize)]
struct EmailRequest<'a> {
    to: &'a str,
    subject: &'a str,
    html: &'a str,
    text: &'a str,
}

#[derive(Debug, Deserialize)]
struct EmailResponse {
    id: Option<String>,
}

pub async fn send_email(
    config: &AppConfig,
    to: &str,
    subject: &str,
    html: &str,
    text: &str,
) -> Result<EmailDelivery, EmailError> {
    let Some(email_url) = config.mctai_email_url.as_deref() else {
        tracing::warn!("MCTAI_EMAIL_URL is not configured; skipping email send");
        return Ok(EmailDelivery::SkippedNotConfigured);
    };
    let Some(app_token) = config.mctai_email_app_token.as_deref() else {
        tracing::warn!("MCTAI_EMAIL_APP_TOKEN is not configured; skipping email send");
        return Ok(EmailDelivery::SkippedNotConfigured);
    };

    let request = EmailRequest {
        to,
        subject,
        html,
        text,
    };
    let client = reqwest::Client::new();
    let response = tokio::time::timeout(
        Duration::from_secs(10),
        client
            .post(email_url)
            .bearer_auth(app_token)
            .json(&request)
            .send(),
    )
    .await
    .map_err(|_| EmailError::Timeout)?
    .map_err(EmailError::Request)?;

    if response.status() == StatusCode::TOO_MANY_REQUESTS {
        return Err(EmailError::RateLimited);
    }

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_else(|error| {
            format!("failed to read email service error response: {error}")
        });
        return Err(EmailError::Service { status, body });
    }

    let message_id = response
        .json::<EmailResponse>()
        .await
        .map_err(EmailError::Request)?
        .id;

    Ok(EmailDelivery::Sent { message_id })
}

#[cfg(test)]
mod tests {
    use crate::config::AppConfig;

    use super::{send_email, EmailDelivery};

    fn config_without_email() -> AppConfig {
        AppConfig {
            host: "0.0.0.0".to_owned(),
            port: 8080,
            database_url: "postgres://example".to_owned(),
            database_max_connections: 5,
            database_connect_timeout_seconds: 5,
            database_ssl_mode: None,
            redis_url: "redis://localhost:6379".to_owned(),
            jwt_secret: "unused".to_owned(),
            market_data_provider_key: "unused".to_owned(),
            market_data_provider_name: "http-json".to_owned(),
            market_data_provider_base_url: None,
            market_data_request_timeout_seconds: 10,
            news_provider_key: "unused".to_owned(),
            news_provider_name: "http-json-news".to_owned(),
            news_provider_base_url: None,
            news_provider_request_timeout_seconds: 10,
            mctai_auth_url: "https://auth.mctai.app".to_owned(),
            mctai_auth_app_token: "app_test".to_owned(),
            mctai_auth_jwks_url: "https://auth.mctai.app/.well-known/jwks.json".to_owned(),
            mctai_email_url: None,
            mctai_email_app_token: None,
            self_url: Some("https://marketlens.mctai.app".to_owned()),
            allowed_cors_origin: None,
        }
    }

    #[tokio::test]
    async fn skips_email_when_service_is_not_configured() {
        let result = send_email(
            &config_without_email(),
            "trader@example.com",
            "Verify your email",
            "<p>Verify</p>",
            "Verify",
        )
        .await;

        assert!(matches!(result, Ok(EmailDelivery::SkippedNotConfigured)));
    }
}
