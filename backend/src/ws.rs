use std::{collections::HashSet, time::Duration};

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    http::{header::COOKIE, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use jsonwebtoken::{
    decode, decode_header,
    jwk::JwkSet,
    Algorithm, DecodingKey, Validation,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::{
    redis::channels,
    state::AppState,
};

const SESSION_COOKIE_NAME: &str = "mctai_session";
const HEARTBEAT_INTERVAL_MS: u64 = 30_000;
const RECONNECT_INITIAL_DELAY_MS: u64 = 1_000;
const RECONNECT_MAX_DELAY_MS: u64 = 30_000;

pub fn router() -> Router<AppState> {
    Router::new().route("/ws", get(websocket_handler))
}

pub fn contract() -> WebSocketContract {
    WebSocketContract {
        endpoint: "/ws",
        authentication: "mctai_session cookie verified against MCTAI_AUTH_JWKS_URL",
        heartbeat_interval_ms: HEARTBEAT_INTERVAL_MS,
        client_messages: &["ping", "subscribe", "unsubscribe"],
        server_messages: &[
            "connection.ready",
            "pong",
            "subscription.ack",
            "subscription.removed",
            "error",
        ],
        reconnect: ReconnectContract {
            initial_delay_ms: RECONNECT_INITIAL_DELAY_MS,
            max_delay_ms: RECONNECT_MAX_DELAY_MS,
            backoff: "exponential",
            resume_token: "connection_id",
        },
        subscriptions: SubscriptionContract {
            market_ticks: concat!(
                "subscribe with channel=market_ticks and instrument_symbol; ",
                "acknowledges only until market data is wired"
            ),
            alert_events: "subscribe with channel=alert_events; scoped to the authenticated user",
        },
    }
}

async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    match authenticate(&state, &headers).await {
        Ok(claims) => ws.on_upgrade(move |socket| websocket_session(socket, claims)),
        Err(error) => {
            let status = error.status_code();
            tracing::warn!(%error, "WebSocket handshake rejected");
            (
                status,
                Json(ErrorMessage {
                    r#type: "error",
                    code: error.code(),
                    message: error.public_message(),
                }),
            )
                .into_response()
        }
    }
}

async fn websocket_session(mut socket: WebSocket, claims: SessionClaims) {
    let connection_id = Uuid::new_v4();
    let mut subscriptions = HashSet::new();

    if send_json(
        &mut socket,
        &ServerMessage::ConnectionReady {
            connection_id,
            user: AuthenticatedUser {
                sub: claims.sub.clone(),
                email: claims.email.clone(),
                name: claims.name.clone(),
                picture: claims.picture.clone(),
            },
            contract: contract(),
        },
    )
    .await
    .is_err()
    {
        return;
    }

    while let Some(message) = socket.recv().await {
        match message {
            Ok(Message::Text(text)) => {
                if handle_client_text(&mut socket, &claims, &mut subscriptions, &text)
                    .await
                    .is_err()
                {
                    break;
                }
            }
            Ok(Message::Ping(payload)) => {
                if socket.send(Message::Pong(payload)).await.is_err() {
                    break;
                }
            }
            Ok(Message::Pong(_)) => {}
            Ok(Message::Close(_)) => break,
            Ok(Message::Binary(_)) => {
                if send_error(
                    &mut socket,
                    None,
                    "unsupported_message",
                    "binary messages are not supported",
                )
                .await
                .is_err()
                {
                    break;
                }
            }
            Err(error) => {
                tracing::debug!(%error, %connection_id, "WebSocket receive error");
                break;
            }
        }
    }
}

async fn handle_client_text(
    socket: &mut WebSocket,
    claims: &SessionClaims,
    subscriptions: &mut HashSet<String>,
    text: &str,
) -> Result<(), axum::Error> {
    let message = match serde_json::from_str::<ClientMessage>(text) {
        Ok(message) => message,
        Err(error) => {
            return send_error(
                socket,
                None,
                "invalid_json",
                &format!("message must match the WebSocket contract: {error}"),
            )
            .await;
        }
    };

    match message {
        ClientMessage::Ping { request_id } => {
            send_json(socket, &ServerMessage::Pong { request_id }).await
        }
        ClientMessage::Subscribe {
            request_id,
            subscription_id,
            topic,
        } => {
            let resolved = topic.resolve(&claims.sub);
            subscriptions.insert(subscription_id.clone());
            send_json(
                socket,
                &ServerMessage::SubscriptionAck {
                    request_id,
                    subscription_id,
                    status: "accepted",
                    redis_channel: resolved.redis_channel,
                    note: "subscription registered; market data fan-out is not wired yet",
                },
            )
            .await
        }
        ClientMessage::Unsubscribe {
            request_id,
            subscription_id,
        } => {
            let removed = subscriptions.remove(&subscription_id);
            send_json(
                socket,
                &ServerMessage::SubscriptionRemoved {
                    request_id,
                    subscription_id,
                    removed,
                },
            )
            .await
        }
    }
}

async fn send_error(
    socket: &mut WebSocket,
    request_id: Option<String>,
    code: &'static str,
    message: &str,
) -> Result<(), axum::Error> {
    send_json(
        socket,
        &ServerMessage::Error {
            request_id,
            code,
            message: message.to_owned(),
        },
    )
    .await
}

async fn send_json<T: Serialize>(socket: &mut WebSocket, value: &T) -> Result<(), axum::Error> {
    let payload = match serde_json::to_string(value) {
        Ok(payload) => payload,
        Err(error) => {
            tracing::error!(%error, "failed to serialize WebSocket message");
            return socket
                .send(Message::Text(
                    r#"{"type":"error","code":"serialization_failed","message":"failed to serialize server message"}"#.to_owned(),
                ))
                .await;
        }
    };

    socket.send(Message::Text(payload)).await
}

async fn authenticate(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<SessionClaims, HandshakeError> {
    let token = session_cookie(headers).ok_or(HandshakeError::MissingSession)?;
    let header = decode_header(token).map_err(|_| HandshakeError::InvalidSession)?;
    let kid = header.kid.ok_or(HandshakeError::InvalidSession)?;
    let jwks = fetch_jwks(state.config().mctai_auth_jwks_url.as_str()).await?;
    let jwk = jwks.find(&kid).ok_or(HandshakeError::InvalidSession)?;
    let key = DecodingKey::from_jwk(jwk).map_err(|_| HandshakeError::InvalidSession)?;
    let mut validation = validation_for_algorithm(header.alg)?;
    validation.set_audience(&[state.config().mctai_auth_app_token.as_str()]);
    validation.set_issuer(&[state.config().mctai_auth_url.as_str()]);

    let decoded = decode::<SessionClaims>(token, &key, &validation)
        .map_err(|_| HandshakeError::InvalidSession)?;

    Ok(decoded.claims)
}

async fn fetch_jwks(jwks_url: &str) -> Result<JwkSet, HandshakeError> {
    let response = tokio::time::timeout(Duration::from_secs(5), reqwest::get(jwks_url))
        .await
        .map_err(|_| HandshakeError::JwksTimeout)?
        .map_err(|source| HandshakeError::JwksFetch { source })?;
    let response = response
        .error_for_status()
        .map_err(|source| HandshakeError::JwksFetch { source })?;

    response
        .json::<JwkSet>()
        .await
        .map_err(|source| HandshakeError::JwksFetch { source })
}

fn session_cookie(headers: &HeaderMap) -> Option<&str> {
    let cookie_header = headers.get(COOKIE)?.to_str().ok()?;

    cookie_header.split(';').find_map(|cookie| {
        let (name, value) = cookie.trim().split_once('=')?;
        (name == SESSION_COOKIE_NAME && !value.is_empty()).then_some(value)
    })
}

fn validation_for_algorithm(algorithm: Algorithm) -> Result<Validation, HandshakeError> {
    match algorithm {
        Algorithm::RS256
        | Algorithm::RS384
        | Algorithm::RS512
        | Algorithm::PS256
        | Algorithm::PS384
        | Algorithm::PS512
        | Algorithm::ES256
        | Algorithm::ES384 => Ok(Validation::new(algorithm)),
        Algorithm::HS256 | Algorithm::HS384 | Algorithm::HS512 => {
            Err(HandshakeError::InvalidSession)
        }
        _ => Err(HandshakeError::InvalidSession),
    }
}

#[derive(Debug, Error)]
enum HandshakeError {
    #[error("missing mctai_session cookie")]
    MissingSession,
    #[error("invalid mctai_session cookie")]
    InvalidSession,
    #[error("timed out fetching auth JWKS")]
    JwksTimeout,
    #[error("failed to fetch auth JWKS: {source}")]
    JwksFetch { source: reqwest::Error },
}

impl HandshakeError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::MissingSession | Self::InvalidSession => StatusCode::UNAUTHORIZED,
            Self::JwksTimeout | Self::JwksFetch { .. } => StatusCode::SERVICE_UNAVAILABLE,
        }
    }

    fn code(&self) -> &'static str {
        match self {
            Self::MissingSession => "missing_session",
            Self::InvalidSession => "invalid_session",
            Self::JwksTimeout | Self::JwksFetch { .. } => "auth_unavailable",
        }
    }

    fn public_message(&self) -> &'static str {
        match self {
            Self::MissingSession => "WebSocket authentication requires mctai_session cookie",
            Self::InvalidSession => "WebSocket authentication failed",
            Self::JwksTimeout | Self::JwksFetch { .. } => "authentication service is unavailable",
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
struct SessionClaims {
    sub: String,
    email: Option<String>,
    name: Option<String>,
    picture: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ClientMessage {
    Ping {
        request_id: Option<String>,
    },
    Subscribe {
        request_id: Option<String>,
        subscription_id: String,
        #[serde(flatten)]
        topic: SubscriptionTopic,
    },
    Unsubscribe {
        request_id: Option<String>,
        subscription_id: String,
    },
}

#[derive(Debug, Deserialize)]
#[serde(tag = "channel", rename_all = "snake_case")]
enum SubscriptionTopic {
    MarketTicks { instrument_symbol: String },
    AlertEvents,
}

impl SubscriptionTopic {
    fn resolve(&self, user_sub: &str) -> ResolvedSubscription {
        match self {
            Self::MarketTicks { instrument_symbol } => ResolvedSubscription {
                redis_channel: channels::market_ticks(instrument_symbol),
            },
            Self::AlertEvents => ResolvedSubscription {
                redis_channel: channels::user_alert_events(user_sub),
            },
        }
    }
}

struct ResolvedSubscription {
    redis_channel: String,
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ServerMessage {
    #[serde(rename = "connection.ready")]
    ConnectionReady {
        connection_id: Uuid,
        user: AuthenticatedUser,
        contract: WebSocketContract,
    },
    Pong {
        request_id: Option<String>,
    },
    #[serde(rename = "subscription.ack")]
    SubscriptionAck {
        request_id: Option<String>,
        subscription_id: String,
        status: &'static str,
        redis_channel: String,
        note: &'static str,
    },
    #[serde(rename = "subscription.removed")]
    SubscriptionRemoved {
        request_id: Option<String>,
        subscription_id: String,
        removed: bool,
    },
    Error {
        request_id: Option<String>,
        code: &'static str,
        message: String,
    },
}

#[derive(Serialize)]
struct ErrorMessage {
    r#type: &'static str,
    code: &'static str,
    message: &'static str,
}

#[derive(Serialize)]
struct AuthenticatedUser {
    sub: String,
    email: Option<String>,
    name: Option<String>,
    picture: Option<String>,
}

#[derive(Serialize)]
pub struct WebSocketContract {
    endpoint: &'static str,
    authentication: &'static str,
    heartbeat_interval_ms: u64,
    client_messages: &'static [&'static str],
    server_messages: &'static [&'static str],
    reconnect: ReconnectContract,
    subscriptions: SubscriptionContract,
}

#[derive(Serialize)]
struct ReconnectContract {
    initial_delay_ms: u64,
    max_delay_ms: u64,
    backoff: &'static str,
    resume_token: &'static str,
}

#[derive(Serialize)]
struct SubscriptionContract {
    market_ticks: &'static str,
    alert_events: &'static str,
}

#[cfg(test)]
mod tests {
    use axum::http::{HeaderMap, HeaderValue};

    use super::{session_cookie, ClientMessage, SubscriptionTopic, COOKIE};

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
    fn parses_market_tick_subscription_message() {
        let message = serde_json::from_str::<ClientMessage>(
            r#"{"type":"subscribe","request_id":"r1","subscription_id":"s1","channel":"market_ticks","instrument_symbol":"BTC/USD"}"#,
        )
        .expect("valid subscription message");

        match message {
            ClientMessage::Subscribe { topic, .. } => match topic {
                SubscriptionTopic::MarketTicks { instrument_symbol } => {
                    assert_eq!(instrument_symbol, "BTC/USD");
                }
                SubscriptionTopic::AlertEvents => panic!("expected market tick topic"),
            },
            _ => panic!("expected subscribe message"),
        }
    }
}
