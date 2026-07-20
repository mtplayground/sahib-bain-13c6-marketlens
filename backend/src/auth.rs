use std::time::Duration;

use axum::{
    extract::{Query, State},
    http::{header::COOKIE, HeaderMap, StatusCode},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
    Json, Router,
};
use chrono::{Duration as ChronoDuration, Utc};
use jsonwebtoken::{
    decode, decode_header,
    jwk::JwkSet,
    Algorithm, DecodingKey, Validation,
};
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::FromRow;
use thiserror::Error;
use url::Url;

use crate::{
    config::AppConfig,
    email::{send_email, EmailDelivery, EmailError},
    state::AppState,
    users::{UpsertUser, User, UserModelError, UserProfile},
};

const SESSION_COOKIE_NAME: &str = "mctai_session";

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/auth/login", get(login))
        .route("/auth/register", get(register))
        .route("/auth/session", get(session))
        .route("/auth/email-verification", post(send_verification))
        .route("/auth/email-verification/confirm", get(confirm_verification))
}

async fn login(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Redirect, AuthHandlerError> {
    Ok(Redirect::temporary(
        auth_redirect_url(state.config(), &headers)?.as_str(),
    ))
}

async fn register(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Redirect, AuthHandlerError> {
    Ok(Redirect::temporary(
        auth_redirect_url(state.config(), &headers)?.as_str(),
    ))
}

async fn session(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<SessionResponse>, AuthHandlerError> {
    let claims = verify_session(state.config(), &headers).await?;
    let profile = UserProfile::new(
        claims.sub,
        claims.email.unwrap_or_default(),
        claims.email_verified.unwrap_or(false),
        claims.name,
        claims.picture,
    )?;
    let upsert = profile.into_upsert(Utc::now());
    let user = upsert_user(&state, upsert).await?;

    Ok(Json(SessionResponse {
        authenticated: true,
        registration: if user.inserted { "new" } else { "returning" },
        message: if user.inserted {
            "Registration complete!"
        } else {
            "Welcome back."
        },
        user: user.user,
    }))
}

async fn send_verification(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<SendVerificationResponse>, AuthHandlerError> {
    let claims = verify_session(state.config(), &headers).await?;
    let profile = UserProfile::new(
        claims.sub,
        claims.email.unwrap_or_default(),
        claims.email_verified.unwrap_or(false),
        claims.name,
        claims.picture,
    )?;
    let user = upsert_user(&state, profile.into_upsert(Utc::now())).await?.user;

    if user.email_verified {
        return Ok(Json(SendVerificationResponse {
            status: "already_verified",
            email: user.email,
            delivery: "not_needed",
            message_id: None,
            expires_at: None,
        }));
    }

    let token = generate_verification_token();
    let token_hash = hash_verification_token(token.as_str());
    let expires_at = Utc::now() + ChronoDuration::hours(24);
    store_verification_token(&state, user.sub.as_str(), user.email.as_str(), &token_hash, expires_at)
        .await?;

    let link = verification_link(state.config(), &headers, token.as_str())?;
    let subject = "Verify your email address";
    let html = verification_email_html(link.as_str());
    let text = verification_email_text(link.as_str());
    let delivery = send_email(state.config(), user.email.as_str(), subject, &html, &text).await?;
    let (delivery_status, message_id) = match delivery {
        EmailDelivery::Sent { message_id } => ("sent", message_id),
        EmailDelivery::SkippedNotConfigured => ("skipped_not_configured", None),
    };

    Ok(Json(SendVerificationResponse {
        status: "verification_pending",
        email: user.email,
        delivery: delivery_status,
        message_id,
        expires_at: Some(expires_at),
    }))
}

async fn confirm_verification(
    State(state): State<AppState>,
    Query(query): Query<ConfirmVerificationQuery>,
) -> Result<Json<ConfirmVerificationResponse>, AuthHandlerError> {
    let token = query
        .token
        .as_deref()
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .ok_or(AuthHandlerError::MissingVerificationToken)?;
    let token_hash = hash_verification_token(token);
    let user = consume_verification_token(&state, token_hash.as_str())
        .await?
        .ok_or(AuthHandlerError::InvalidVerificationToken)?;

    Ok(Json(ConfirmVerificationResponse {
        status: "verified",
        user,
    }))
}

pub async fn verify_session(
    config: &AppConfig,
    headers: &HeaderMap,
) -> Result<SessionClaims, AuthError> {
    let token = session_cookie(headers).ok_or(AuthError::MissingSession)?;
    let header = decode_header(token).map_err(|_| AuthError::InvalidSession)?;
    let kid = header.kid.ok_or(AuthError::InvalidSession)?;
    let jwks = fetch_jwks(config.mctai_auth_jwks_url.as_str()).await?;
    let jwk = jwks.find(&kid).ok_or(AuthError::InvalidSession)?;
    let key = DecodingKey::from_jwk(jwk).map_err(|_| AuthError::InvalidSession)?;
    let mut validation = validation_for_algorithm(header.alg)?;
    validation.set_audience(&[config.mctai_auth_app_token.as_str()]);
    validation.set_issuer(&[config.mctai_auth_url.as_str()]);

    let decoded =
        decode::<SessionClaims>(token, &key, &validation).map_err(|_| AuthError::InvalidSession)?;

    Ok(decoded.claims)
}

async fn fetch_jwks(jwks_url: &str) -> Result<JwkSet, AuthError> {
    let response = tokio::time::timeout(Duration::from_secs(5), reqwest::get(jwks_url))
        .await
        .map_err(|_| AuthError::JwksTimeout)?
        .map_err(|source| AuthError::JwksFetch { source })?;
    let response = response
        .error_for_status()
        .map_err(|source| AuthError::JwksFetch { source })?;

    response
        .json::<JwkSet>()
        .await
        .map_err(|source| AuthError::JwksFetch { source })
}

pub fn session_cookie(headers: &HeaderMap) -> Option<&str> {
    let cookie_header = headers.get(COOKIE)?.to_str().ok()?;

    cookie_header.split(';').find_map(|cookie| {
        let (name, value) = cookie.trim().split_once('=')?;
        (name == SESSION_COOKIE_NAME && !value.is_empty()).then_some(value)
    })
}

fn validation_for_algorithm(algorithm: Algorithm) -> Result<Validation, AuthError> {
    match algorithm {
        Algorithm::RS256
        | Algorithm::RS384
        | Algorithm::RS512
        | Algorithm::PS256
        | Algorithm::PS384
        | Algorithm::PS512
        | Algorithm::ES256
        | Algorithm::ES384 => Ok(Validation::new(algorithm)),
        Algorithm::HS256 | Algorithm::HS384 | Algorithm::HS512 => Err(AuthError::InvalidSession),
        _ => Err(AuthError::InvalidSession),
    }
}

async fn upsert_user(state: &AppState, user: UpsertUser) -> Result<UpsertedUser, sqlx::Error> {
    let row = sqlx::query_as::<_, UpsertedUserRow>(
        r#"
        INSERT INTO users (
            sub,
            email,
            email_verified,
            email_verified_at,
            name,
            picture_url,
            last_seen_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, NOW())
        ON CONFLICT (sub) DO UPDATE
        SET
            email = EXCLUDED.email,
            email_verified = EXCLUDED.email_verified,
            email_verified_at = EXCLUDED.email_verified_at,
            name = EXCLUDED.name,
            picture_url = EXCLUDED.picture_url,
            last_seen_at = NOW()
        RETURNING
            (xmax = 0) AS inserted,
            sub,
            email,
            email_verified,
            email_verified_at,
            name,
            picture_url,
            created_at,
            updated_at,
            last_seen_at
        "#,
    )
    .bind(user.sub)
    .bind(user.email)
    .bind(user.email_verified)
    .bind(user.email_verified_at)
    .bind(user.name)
    .bind(user.picture_url)
    .fetch_one(state.database().pool())
    .await?;

    Ok(UpsertedUser {
        inserted: row.inserted,
        user: row.into_user(),
    })
}

async fn store_verification_token(
    state: &AppState,
    user_sub: &str,
    email: &str,
    token_hash: &str,
    expires_at: chrono::DateTime<Utc>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        UPDATE email_verification_tokens
        SET consumed_at = NOW()
        WHERE user_sub = $1
            AND lower(email) = lower($2)
            AND consumed_at IS NULL
        "#,
    )
    .bind(user_sub)
    .bind(email)
    .execute(state.database().pool())
    .await?;

    sqlx::query(
        r#"
        INSERT INTO email_verification_tokens (user_sub, email, token_hash, expires_at)
        VALUES ($1, $2, $3, $4)
        "#,
    )
    .bind(user_sub)
    .bind(email)
    .bind(token_hash)
    .bind(expires_at)
    .execute(state.database().pool())
    .await?;

    Ok(())
}

async fn consume_verification_token(
    state: &AppState,
    token_hash: &str,
) -> Result<Option<User>, sqlx::Error> {
    sqlx::query_as::<_, User>(
        r#"
        WITH consumed AS (
            UPDATE email_verification_tokens
            SET consumed_at = NOW()
            WHERE token_hash = $1
                AND consumed_at IS NULL
                AND expires_at > NOW()
            RETURNING user_sub, email
        )
        UPDATE users
        SET
            email_verified = TRUE,
            email_verified_at = COALESCE(email_verified_at, NOW()),
            updated_at = NOW()
        FROM consumed
        WHERE users.sub = consumed.user_sub
            AND lower(users.email) = lower(consumed.email)
        RETURNING
            users.sub,
            users.email,
            users.email_verified,
            users.email_verified_at,
            users.name,
            users.picture_url,
            users.created_at,
            users.updated_at,
            users.last_seen_at
        "#,
    )
    .bind(token_hash)
    .fetch_optional(state.database().pool())
    .await
}

fn auth_redirect_url(config: &AppConfig, headers: &HeaderMap) -> Result<String, url::ParseError> {
    let mut url = Url::parse(config.mctai_auth_url.as_str())?.join("login")?;
    let return_to = frontend_return_to(config, headers);
    url.query_pairs_mut()
        .append_pair("app_token", config.mctai_auth_app_token.as_str())
        .append_pair("return_to", return_to.as_str());
    Ok(url.to_string())
}

fn frontend_return_to(config: &AppConfig, headers: &HeaderMap) -> String {
    if let Some(self_url) = config.self_url.as_deref() {
        return format!("{}/", self_url.trim_end_matches('/'));
    }

    let proto = header_value(headers, "x-forwarded-proto").unwrap_or("https");
    if let Some(host) = header_value(headers, "x-forwarded-host") {
        return format!("{proto}://{host}/");
    }

    "/".to_owned()
}

fn verification_link(
    config: &AppConfig,
    headers: &HeaderMap,
    token: &str,
) -> Result<String, url::ParseError> {
    let mut url = Url::parse(public_origin(config, headers).as_str())?
        .join("/api/v1/auth/email-verification/confirm")?;
    url.query_pairs_mut().append_pair("token", token);
    Ok(url.to_string())
}

fn public_origin(config: &AppConfig, headers: &HeaderMap) -> String {
    if let Some(self_url) = config.self_url.as_deref() {
        return format!("{}/", self_url.trim_end_matches('/'));
    }

    let proto = header_value(headers, "x-forwarded-proto").unwrap_or("http");
    if let Some(host) = header_value(headers, "x-forwarded-host") {
        return format!("{proto}://{host}/");
    }

    format!("http://{}:{}/", config.host, config.port)
}

fn generate_verification_token() -> String {
    let mut bytes = [0_u8; 32];
    OsRng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}

fn hash_verification_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hex::encode(hasher.finalize())
}

fn verification_email_html(link: &str) -> String {
    format!(
        "<p>Confirm this email address by opening this secure verification link:</p><p><a href=\"{link}\">Verify email address</a></p><p>This link expires in 24 hours.</p>"
    )
}

fn verification_email_text(link: &str) -> String {
    format!("Confirm this email address by opening this verification link: {link}\n\nThis link expires in 24 hours.")
}

fn header_value<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name)?.to_str().ok()
}

#[derive(Debug, Clone, Deserialize)]
pub struct SessionClaims {
    pub sub: String,
    pub email: Option<String>,
    pub email_verified: Option<bool>,
    pub name: Option<String>,
    pub picture: Option<String>,
}

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("missing mctai_session cookie")]
    MissingSession,
    #[error("invalid mctai_session cookie")]
    InvalidSession,
    #[error("timed out fetching auth JWKS")]
    JwksTimeout,
    #[error("failed to fetch auth JWKS: {source}")]
    JwksFetch { source: reqwest::Error },
}

impl AuthError {
    pub fn status_code(&self) -> StatusCode {
        match self {
            Self::MissingSession | Self::InvalidSession => StatusCode::UNAUTHORIZED,
            Self::JwksTimeout | Self::JwksFetch { .. } => StatusCode::SERVICE_UNAVAILABLE,
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            Self::MissingSession => "missing_session",
            Self::InvalidSession => "invalid_session",
            Self::JwksTimeout | Self::JwksFetch { .. } => "auth_unavailable",
        }
    }

    pub fn public_message(&self) -> &'static str {
        match self {
            Self::MissingSession => "Authentication requires mctai_session cookie",
            Self::InvalidSession => "Authentication failed",
            Self::JwksTimeout | Self::JwksFetch { .. } => "authentication service is unavailable",
        }
    }
}

#[derive(Debug, Error)]
enum AuthHandlerError {
    #[error("{0}")]
    Auth(#[from] AuthError),
    #[error("{0}")]
    UserModel(#[from] UserModelError),
    #[error("{0}")]
    Email(#[from] EmailError),
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("auth redirect URL error: {0}")]
    RedirectUrl(#[from] url::ParseError),
    #[error("missing verification token")]
    MissingVerificationToken,
    #[error("invalid or expired verification token")]
    InvalidVerificationToken,
}

impl IntoResponse for AuthHandlerError {
    fn into_response(self) -> Response {
        let (status, code, message) = match &self {
            Self::Auth(error) => (
                error.status_code(),
                error.code(),
                error.public_message().to_owned(),
            ),
            Self::UserModel(_) => (
                StatusCode::BAD_REQUEST,
                "invalid_user_profile",
                "session profile is missing required user fields".to_owned(),
            ),
            Self::Email(EmailError::RateLimited) => (
                StatusCode::TOO_MANY_REQUESTS,
                "email_rate_limited",
                "email service is rate limited; try again shortly".to_owned(),
            ),
            Self::Email(_) => (
                StatusCode::BAD_GATEWAY,
                "email_send_failed",
                "failed to send verification email".to_owned(),
            ),
            Self::Database(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "user_persistence_failed",
                "failed to persist authenticated user".to_owned(),
            ),
            Self::RedirectUrl(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "auth_redirect_failed",
                "failed to build authentication redirect".to_owned(),
            ),
            Self::MissingVerificationToken => (
                StatusCode::BAD_REQUEST,
                "missing_verification_token",
                "verification token is required".to_owned(),
            ),
            Self::InvalidVerificationToken => (
                StatusCode::BAD_REQUEST,
                "invalid_verification_token",
                "verification token is invalid or expired".to_owned(),
            ),
        };

        if status.is_server_error() || matches!(self, Self::Email(_)) {
            tracing::error!(%self, "auth handler failed");
        }

        (
            status,
            Json(ErrorResponse {
                error: code,
                message,
            }),
        )
            .into_response()
    }
}

#[derive(Debug, FromRow)]
struct UpsertedUserRow {
    inserted: bool,
    sub: String,
    email: String,
    email_verified: bool,
    email_verified_at: Option<chrono::DateTime<Utc>>,
    name: Option<String>,
    picture_url: Option<String>,
    created_at: chrono::DateTime<Utc>,
    updated_at: chrono::DateTime<Utc>,
    last_seen_at: chrono::DateTime<Utc>,
}

impl UpsertedUserRow {
    fn into_user(self) -> User {
        User {
            sub: self.sub,
            email: self.email,
            email_verified: self.email_verified,
            email_verified_at: self.email_verified_at,
            name: self.name,
            picture_url: self.picture_url,
            created_at: self.created_at,
            updated_at: self.updated_at,
            last_seen_at: self.last_seen_at,
        }
    }
}

struct UpsertedUser {
    inserted: bool,
    user: User,
}

#[derive(Serialize)]
struct SessionResponse {
    authenticated: bool,
    registration: &'static str,
    message: &'static str,
    user: User,
}

#[derive(Debug, Deserialize)]
struct ConfirmVerificationQuery {
    token: Option<String>,
}

#[derive(Serialize)]
struct SendVerificationResponse {
    status: &'static str,
    email: String,
    delivery: &'static str,
    message_id: Option<String>,
    expires_at: Option<chrono::DateTime<Utc>>,
}

#[derive(Serialize)]
struct ConfirmVerificationResponse {
    status: &'static str,
    user: User,
}

#[derive(Serialize)]
struct ErrorResponse {
    error: &'static str,
    message: String,
}

#[cfg(test)]
mod tests {
    use axum::http::{HeaderMap, HeaderValue};

    use crate::config::AppConfig;

    use super::{
        auth_redirect_url, frontend_return_to, hash_verification_token, session_cookie,
        verification_link, COOKIE,
    };

    fn config() -> AppConfig {
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
            news_provider_key: "unused".to_owned(),
            mctai_auth_url: "https://auth.mctai.app".to_owned(),
            mctai_auth_app_token: "app_test".to_owned(),
            mctai_auth_jwks_url: "https://auth.mctai.app/.well-known/jwks.json".to_owned(),
            mctai_email_url: None,
            mctai_email_app_token: None,
            self_url: Some("https://marketlens.mctai.app".to_owned()),
            allowed_cors_origin: None,
        }
    }

    #[test]
    fn extracts_mctai_session_cookie() {
        let mut headers = HeaderMap::new();
        headers.insert(
            COOKIE,
            HeaderValue::from_static("other=value; mctai_session=session-token; theme=dark"),
        );

        assert_eq!(session_cookie(&headers), Some("session-token"));
    }

    #[test]
    fn builds_auth_redirect_to_frontend_root() {
        let headers = HeaderMap::new();
        let url = match auth_redirect_url(&config(), &headers) {
            Ok(url) => url,
            Err(error) => panic!("redirect URL should be valid: {error}"),
        };

        assert!(url.starts_with("https://auth.mctai.app/login?"));
        assert!(url.contains("app_token=app_test"));
        assert!(url.contains("return_to=https%3A%2F%2Fmarketlens.mctai.app%2F"));
    }

    #[test]
    fn derives_return_to_from_forwarded_host_without_self_url() {
        let mut config = config();
        config.self_url = None;
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-proto", HeaderValue::from_static("https"));
        headers.insert("x-forwarded-host", HeaderValue::from_static("public.example"));

        assert_eq!(
            frontend_return_to(&config, &headers),
            "https://public.example/"
        );
    }

    #[test]
    fn builds_verification_link_to_api_confirm_endpoint() {
        let headers = HeaderMap::new();
        let link = match verification_link(&config(), &headers, "abc123") {
            Ok(link) => link,
            Err(error) => panic!("verification link should be valid: {error}"),
        };

        assert_eq!(
            link,
            "https://marketlens.mctai.app/api/v1/auth/email-verification/confirm?token=abc123"
        );
    }

    #[test]
    fn hashes_verification_tokens_without_storing_raw_token() {
        let hash = hash_verification_token("abc123");

        assert_ne!(hash, "abc123");
        assert_eq!(hash.len(), 64);
        assert_eq!(hash, hash_verification_token("abc123"));
    }
}
