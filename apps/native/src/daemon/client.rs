//! The daemon's loopback HTTP and WebSocket client. This is the same contract the
//! Tauri shell uses (`apps/desktop/src-tauri/src/lib.rs:471` `rpc_call`, `:894`
//! `event_loop`), minus Tauri: the window talks to the daemon itself, in-process.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message;

use super::supervisor::Connection;
use super::worker::Update;

/// A daemon error worth showing a person. Provider error text never appears here.
#[derive(Debug, Clone)]
pub enum DaemonError {
    Transport(String),
    Contract(String),
    Remote { code: String, message: String },
}

impl std::fmt::Display for DaemonError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DaemonError::Transport(message) => write!(formatter, "Daemon request failed: {message}"),
            DaemonError::Contract(message) => write!(formatter, "Daemon returned invalid JSON: {message}"),
            DaemonError::Remote { code, message } if message.is_empty() => write!(formatter, "{code}"),
            DaemonError::Remote { message, .. } => write!(formatter, "{message}"),
        }
    }
}

impl DaemonError {
    pub fn code(&self) -> Option<&str> {
        match self {
            DaemonError::Remote { code, .. } => Some(code),
            _ => None,
        }
    }
}

#[derive(Deserialize)]
struct RpcResponse {
    v: u32,
    id: String,
    ok: bool,
    #[serde(default)]
    result: Option<Value>,
    #[serde(default)]
    error: Option<RpcErrorBody>,
}

#[derive(Deserialize)]
struct RpcErrorBody {
    code: String,
    #[serde(default)]
    message: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct DaemonEvent {
    #[serde(default)]
    pub v: u32,
    #[serde(default)]
    pub sequence: u64,
    #[serde(rename = "type")]
    pub event_type: String,
    #[serde(default)]
    pub payload: Value,
}

pub struct Client {
    connection: Connection,
    http: reqwest::Client,
}

impl Client {
    pub fn new(connection: Connection) -> Result<Self, String> {
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(3))
            .timeout(Duration::from_secs(60))
            .build()
            .map_err(|error| format!("Could not build the daemon HTTP client: {error}"))?;
        Ok(Self { connection, http })
    }


    /// `POST /v1/rpc`. The request id is checked against the answer so a late or
    /// crossed reply can never be taken for another call's result.
    pub async fn rpc(&self, method: &str, params: Value) -> Result<Value, DaemonError> {
        let request_id = format!("native_{}", uuid::Uuid::new_v4());
        let body = json!({"v": 1, "id": request_id, "method": method, "params": params});
        let response = self
            .http
            .post(self.connection.rpc_url())
            .header(reqwest::header::AUTHORIZATION, format!("Bearer {}", self.connection.capability()))
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|error| DaemonError::Transport(error.to_string()))?;
        let status = response.status();
        let parsed = response
            .json::<RpcResponse>()
            .await
            .map_err(|error| DaemonError::Contract(error.to_string()))?;
        if parsed.v != 1 || parsed.id != request_id {
            return Err(DaemonError::Contract("Daemon response did not match the request.".into()));
        }
        if !status.is_success() || !parsed.ok {
            let error = parsed.error.unwrap_or(RpcErrorBody { code: "internal".into(), message: "Daemon request failed.".into() });
            return Err(DaemonError::Remote { code: error.code, message: error.message });
        }
        Ok(parsed.result.unwrap_or(Value::Null))
    }
}

/// Follow the daemon's event stream until the worker stops us. Reconnects with
/// backoff, asks for everything after the snapshot's cursor, and drops events we
/// have already seen (the replay and the live stream can overlap).
pub async fn follow_events(connection: Connection, cursor: Arc<AtomicU64>, updates: async_channel::Sender<Update>) {
    let mut backoff = Duration::from_millis(500);
    loop {
        if connect_once(&connection, cursor.clone(), &updates).await.is_ok() {
            backoff = Duration::from_millis(500);
        }
        if updates.is_closed() {
            return;
        }
        let _ = updates.send(Update::Connected(false)).await;
        tokio::time::sleep(backoff).await;
        backoff = (backoff * 2).min(Duration::from_secs(8));
    }
}

async fn connect_once(
    connection: &Connection,
    cursor: Arc<AtomicU64>,
    updates: &async_channel::Sender<Update>,
) -> Result<(), ()> {
    let mut request = connection.events_url().into_client_request().map_err(|_| ())?;
    if let Ok(value) = format!("Bearer {}", connection.capability()).parse() {
        request.headers_mut().insert("authorization", value);
    }
    let (mut socket, _) = tokio_tungstenite::connect_async(request).await.map_err(|_| ())?;
    let sequence = cursor.load(Ordering::Acquire);
    if socket
        .send(Message::Text(json!({"v": 1, "afterSequence": sequence}).to_string().into()))
        .await
        .is_err()
    {
        return Err(());
    }
    let mut last = sequence;
    let mut announced = false;
    while let Some(frame) = socket.next().await {
        let Ok(frame) = frame else { break };
        let text = match frame {
            Message::Text(text) => text.to_string(),
            Message::Binary(bytes) => String::from_utf8_lossy(&bytes).to_string(),
            Message::Ping(bytes) => {
                let _ = socket.send(Message::Pong(bytes)).await;
                continue;
            }
            Message::Close(_) => break,
            _ => continue,
        };
        let Ok(event) = serde_json::from_str::<DaemonEvent>(&text) else { continue };
        if event.v != 1 {
            continue;
        }
        if event.event_type == "events.replay.complete" {
            let through = event.payload.get("throughSequence").and_then(Value::as_u64).unwrap_or(last);
            last = last.max(through);
            cursor.fetch_max(last, Ordering::Release);
            if !announced {
                announced = true;
                let _ = updates.send(Update::Connected(true)).await;
            }
            continue;
        }
        if event.event_type == "replay.gap" {
            // The window recovers by taking a fresh snapshot, because the events
            // it missed are gone.
            let _ = updates.send(Update::Event(Box::new(event))).await;
            break;
        }
        if event.sequence <= last {
            continue;
        }
        last = event.sequence;
        cursor.store(last, Ordering::Release);
        if updates.send(Update::Event(Box::new(event))).await.is_err() {
            return Err(());
        }
    }
    Err(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_event_ignores_unknown_fields_and_keeps_its_type() {
        let event: DaemonEvent = serde_json::from_str(
            r#"{"v":1,"eventId":"evt_1","sequence":7,"timestamp":"2026-11-08T10:00:00Z","type":"session.changed","payload":{"sessionId":"s"},"somethingNew":true}"#,
        )
        .unwrap();
        assert_eq!(event.sequence, 7);
        assert_eq!(event.event_type, "session.changed");
    }

    #[test]
    fn a_remote_error_shows_its_message_and_keeps_its_code() {
        let error = DaemonError::Remote { code: "not_found".into(), message: "That project no longer exists.".into() };
        assert_eq!(error.to_string(), "That project no longer exists.");
        assert_eq!(error.code(), Some("not_found"));
    }
}
