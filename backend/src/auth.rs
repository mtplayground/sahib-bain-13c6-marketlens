use std::time::Duration;

use axum::{
    extract::State,
    http::{header::COOKIE, HeaderMap, StatusCode},
    response::{IntoResponse, Redirect, Response},
    routing::get,
    Json, Router,
};
use chrono::Utc;
use jsonwebtoken::{
    decode, decode_header,
    jwk::JwkSet,
    Algorithm, DecodingKey, Validation,
};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use thiserror::Error;
use url::Url;

use crate::{
    config::AppConfig,
    state::AppState,
    users::{UpsertUser, User, UserModelError, UserProfile},
};

const SESSION_COOKIE_NAME: &str = "mctai_session";

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/auth/login", get(login))
        .route("/auth/register", get(register))
        .route("/auth/session", get(session))
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
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("auth redirect URL error: {0}")]
    RedirectUrl(#[from] url::ParseError),
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
        };

        if status.is_server_error() {
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

#[derive(Serialize)]
struct ErrorResponse {
    error: &'static str,
    message: String,
}

#[cfg(test)]
mod tests {
    use axum::http::{HeaderMap, HeaderValue};

    use crate::config::AppConfig;

    use super::{auth_redirect_url, frontend_return_to, session_cookie, COOKIE};

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
}
