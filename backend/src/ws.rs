use std::collections::HashMap;

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    http::HeaderMap,
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use futures_util::{
    stream::SplitSink,
    SinkExt, StreamExt,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::{
    auth::{authenticate_request, SessionClaims},
    redis::{channels, RedisClient},
    state::AppState,
};

const HEARTBEAT_INTERVAL_MS: u64 = 30_000;
const RECONNECT_INITIAL_DELAY_MS: u64 = 1_000;
const RECONNECT_MAX_DELAY_MS: u64 = 30_000;
const MARKET_TICKS_SUBSCRIPTION_HELP: &str = concat!(
    "subscribe with channel=market_ticks and instrument_symbol; ",
    "ticks are delivered as subscription.event messages when Redis publishes ",
    "marketlens:market:ticks:*"
);

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
            "subscription.event",
            "error",
        ],
        reconnect: ReconnectContract {
            initial_delay_ms: RECONNECT_INITIAL_DELAY_MS,
            max_delay_ms: RECONNECT_MAX_DELAY_MS,
            backoff: "exponential",
            resume_token: "connection_id",
        },
        subscriptions: SubscriptionContract {
            market_ticks: MARKET_TICKS_SUBSCRIPTION_HELP,
            alert_events:
                "subscribe with channel=alert_events; alert.triggered payloads are scoped to the authenticated user's Redis channel",
        },
    }
}

async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    match authenticate_request(&state, &headers).await {
        Ok(auth) => {
            let redis = state.redis().clone();
            ws.on_upgrade(move |socket| websocket_session(socket, auth.claims, redis))
        }
        Err(error) => {
            tracing::warn!(%error, "WebSocket handshake rejected");
            (
                error.status_code(),
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

type WsSender = SplitSink<WebSocket, Message>;

async fn websocket_session(socket: WebSocket, claims: SessionClaims, redis: RedisClient) {
    let connection_id = Uuid::new_v4();
    let (mut sender, mut receiver) = socket.split();

    let mut pubsub = match redis.pubsub().await {
        Ok(pubsub) => pubsub,
        Err(error) => {
            tracing::error!(%error, %connection_id, "failed to open Redis pub/sub connection");
            let _ = send_error(
                &mut sender,
                None,
                "redis_unavailable",
                "real-time subscriptions are temporarily unavailable",
            )
            .await;
            return;
        }
    };

    if let Err(error) = pubsub.psubscribe(channels::MARKET_TICKS_PATTERN).await {
        tracing::error!(%error, %connection_id, "failed to subscribe to Redis market tick pattern");
        let _ = send_error(
            &mut sender,
            None,
            "redis_subscription_failed",
            "market tick subscriptions are temporarily unavailable",
        )
        .await;
        return;
    }

    let user_alert_channel = channels::user_alert_events(&claims.sub);
    if let Err(error) = pubsub.subscribe(&user_alert_channel).await {
        tracing::error!(%error, %connection_id, "failed to subscribe to Redis user alert channel");
        let _ = send_error(
            &mut sender,
            None,
            "redis_subscription_failed",
            "alert subscriptions are temporarily unavailable",
        )
        .await;
        return;
    }

    let mut redis_messages = pubsub.into_on_message();
    let mut subscriptions = HashMap::new();

    if send_json(
        &mut sender,
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

    loop {
        tokio::select! {
            client_message = receiver.next() => {
                if !handle_client_message(
                    &mut sender,
                    &claims,
                    &mut subscriptions,
                    client_message,
                    connection_id,
                )
                .await {
                    break;
                }
            }
            redis_message = redis_messages.next() => {
                match redis_message {
                    Some(message) => {
                        if fan_out_redis_message(&mut sender, &subscriptions, message).await.is_err() {
                            break;
                        }
                    }
                    None => {
                        tracing::warn!(%connection_id, "Redis pub/sub stream ended");
                        let _ = send_error(
                            &mut sender,
                            None,
                            "redis_stream_closed",
                            "real-time subscription stream closed",
                        )
                        .await;
                        break;
                    }
                }
            }
        }
    }
}

async fn handle_client_message(
    sender: &mut WsSender,
    claims: &SessionClaims,
    subscriptions: &mut HashMap<String, ActiveSubscription>,
    message: Option<Result<Message, axum::Error>>,
    connection_id: Uuid,
) -> bool {
    match message {
        Some(Ok(Message::Text(text))) => {
            handle_client_text(sender, claims, subscriptions, &text)
                .await
                .is_ok()
        }
        Some(Ok(Message::Ping(payload))) => sender.send(Message::Pong(payload)).await.is_ok(),
        Some(Ok(Message::Pong(_))) => true,
        Some(Ok(Message::Close(_))) | None => false,
        Some(Ok(Message::Binary(_))) => send_error(
            sender,
            None,
            "unsupported_message",
            "binary messages are not supported",
        )
        .await
        .is_ok(),
        Some(Err(error)) => {
            tracing::debug!(%error, %connection_id, "WebSocket receive error");
            false
        }
    }
}

async fn handle_client_text(
    sender: &mut WsSender,
    claims: &SessionClaims,
    subscriptions: &mut HashMap<String, ActiveSubscription>,
    text: &str,
) -> Result<(), axum::Error> {
    let message = match serde_json::from_str::<ClientMessage>(text) {
        Ok(message) => message,
        Err(error) => {
            return send_error(
                sender,
                None,
                "invalid_json",
                &format!("message must match the WebSocket contract: {error}"),
            )
            .await;
        }
    };

    match message {
        ClientMessage::Ping { request_id } => {
            send_json(sender, &ServerMessage::Pong { request_id }).await
        }
        ClientMessage::Subscribe {
            request_id,
            subscription_id,
            topic,
        } => {
            let resolved = topic.resolve(&claims.sub);
            subscriptions.insert(
                subscription_id.clone(),
                ActiveSubscription {
                    redis_channel: resolved.redis_channel.clone(),
                },
            );
            send_json(
                sender,
                &ServerMessage::SubscriptionAck {
                    request_id,
                    subscription_id,
                    status: "accepted",
                    redis_channel: resolved.redis_channel,
                    note: "subscription registered; matching Redis events will fan out to this socket",
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
                sender,
                &ServerMessage::SubscriptionRemoved {
                    request_id,
                    subscription_id,
                    removed: removed.is_some(),
                },
            )
            .await
        }
    }
}

async fn fan_out_redis_message(
    sender: &mut WsSender,
    subscriptions: &HashMap<String, ActiveSubscription>,
    message: ::redis::Msg,
) -> Result<(), axum::Error> {
    let redis_channel = message.get_channel_name().to_owned();
    let subscription_ids = matching_subscription_ids(subscriptions, &redis_channel);
    if subscription_ids.is_empty() {
        return Ok(());
    }

    let payload_text = match message.get_payload::<String>() {
        Ok(payload) => payload,
        Err(error) => {
            tracing::warn!(%error, %redis_channel, "failed to decode Redis pub/sub payload");
            return send_error(
                sender,
                None,
                "invalid_redis_payload",
                "received a Redis event payload that could not be decoded",
            )
            .await;
        }
    };
    let payload = match serde_json::from_str::<Value>(&payload_text) {
        Ok(value) => value,
        Err(_) => Value::String(payload_text),
    };

    send_json(
        sender,
        &ServerMessage::SubscriptionEvent {
            subscription_ids,
            redis_channel,
            payload,
        },
    )
    .await
}

fn matching_subscription_ids(
    subscriptions: &HashMap<String, ActiveSubscription>,
    redis_channel: &str,
) -> Vec<String> {
    let mut subscription_ids = subscriptions
        .iter()
        .filter_map(|(subscription_id, subscription)| {
            (subscription.redis_channel == redis_channel).then(|| subscription_id.clone())
        })
        .collect::<Vec<_>>();
    subscription_ids.sort();
    subscription_ids
}

async fn send_error(
    sender: &mut WsSender,
    request_id: Option<String>,
    code: &'static str,
    message: &str,
) -> Result<(), axum::Error> {
    send_json(
        sender,
        &ServerMessage::Error {
            request_id,
            code,
            message: message.to_owned(),
        },
    )
    .await
}

async fn send_json<T: Serialize>(sender: &mut WsSender, value: &T) -> Result<(), axum::Error> {
    let payload = match serde_json::to_string(value) {
        Ok(payload) => payload,
        Err(error) => {
            tracing::error!(%error, "failed to serialize WebSocket message");
            return sender
                .send(Message::Text(
                    r#"{"type":"error","code":"serialization_failed","message":"failed to serialize server message"}"#.to_owned(),
                ))
                .await;
        }
    };

    sender.send(Message::Text(payload)).await
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct ActiveSubscription {
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
    #[serde(rename = "subscription.event")]
    SubscriptionEvent {
        subscription_ids: Vec<String>,
        redis_channel: String,
        payload: Value,
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
    message: String,
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
    use std::collections::HashMap;

    use super::{
        matching_subscription_ids, ActiveSubscription, ClientMessage, SubscriptionTopic,
    };

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

    #[test]
    fn matches_only_subscriptions_for_redis_channel() {
        let mut subscriptions = HashMap::new();
        subscriptions.insert(
            "sub-2".to_owned(),
            ActiveSubscription {
                redis_channel: "marketlens:market:ticks:spy".to_owned(),
            },
        );
        subscriptions.insert(
            "sub-1".to_owned(),
            ActiveSubscription {
                redis_channel: "marketlens:market:ticks:spy".to_owned(),
            },
        );
        subscriptions.insert(
            "other".to_owned(),
            ActiveSubscription {
                redis_channel: "marketlens:market:ticks:qqq".to_owned(),
            },
        );

        assert_eq!(
            matching_subscription_ids(&subscriptions, "marketlens:market:ticks:spy"),
            vec!["sub-1".to_owned(), "sub-2".to_owned()]
        );
        assert!(
            matching_subscription_ids(&subscriptions, "marketlens:market:ticks:iwm").is_empty()
        );
    }
}
