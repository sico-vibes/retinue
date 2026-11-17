//! Chat channels to the CEO (16 November 2026): Telegram by long polling, and
//! the outbound half of the WhatsApp Cloud API.
//!
//! The owner's decisions are in `AGENTS.md` (16 November 2026) and the contract
//! in `docs/plans/vault-channels-pi.md`, section B:
//!
//! * One channel row per kind (`crates/retinue-storage/src/channels.rs`).
//!   Tokens are in the Windows credential store under
//!   `Retinue/channel/<kind>/<name>`, never in SQLite, an event, a log or a
//!   backup.
//! * Telegram only ever polls: no listener, no public address, no webhook.
//! * Exactly one sender is paired, approved on the desktop after they use a
//!   one-time code. Groups, other senders and channel posts are dropped, and
//!   every check runs again before the message is used.
//! * An inbound message becomes a user message in the CEO's channel
//!   conversation; only the CEO's final reply goes back, sanitised and split.
//!   The inbound ledger is keyed by Telegram's own message id, so a delivery
//!   that arrives twice starts one turn.
//! * The CEO's approval cards also arrive in the chat, as Allow and Deny
//!   buttons carrying an opaque one-use id. Vault uses, `owner_*` and
//!   `computer_*` grants, questions, proposals and Full access never do.
//!
//! WhatsApp Web is not built in this build (see the report): the crate has no
//! client that is ready to ship, so `channel.whatsappWeb.start` says so rather
//! than pretending. The Cloud API is built for sending only; its inbound path
//! needs a public address and is written up in the lane's report.

use crate::{derr, secrets, team, AppState, DispatchError};
use axum::http::StatusCode;
use chrono::{Duration as ChronoDuration, SecondsFormat, Utc};
use regex::Regex;
use reqwest::redirect::Policy;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::{Mutex as StdMutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};
use subtle::ConstantTimeEq;
use zeroize::Zeroizing;

/// The Telegram Bot API. Polling only: this is an outbound client, never a
/// listener.
const TELEGRAM_API: &str = "https://api.telegram.org";
/// The Graph API version pinned for the WhatsApp Cloud API.
const CLOUD_API: &str = "https://graph.facebook.com/v25.0";
/// A pairing code lives ten minutes, like the contract says, and burns after
/// five wrong tries.
const PAIRING_MINUTES: i64 = 10;
const PAIRING_ATTEMPTS: u8 = 5;
/// Telegram's long poll holds the request this long; the client waits a little
/// longer than that before calling it a failure.
const POLL_SECS: u64 = 20;
const POLL_TIMEOUT_SECS: u64 = 30;
/// Telegram's own limit is 4,096 characters; a little margin keeps a chunk that
/// gains an escape from being refused.
const CHAT_CHUNK: usize = 3_800;
const DETAIL_MAX: usize = 300;
const ACCOUNT_MAX: usize = 64;
const TOKEN_MAX: usize = 300;

pub(crate) const KINDS: [&str; 3] = ["telegram", "whatsapp-web", "whatsapp-cloud"];

// ---------------------------------------------------------------------------
// Runtime state (memory only)

/// What a running channel holds in memory: which task is allowed to run, the
/// open pairing window, the approval buttons still answerable, and the inbound
/// messages whose reply is still owed.
#[derive(Default)]
pub(crate) struct Channels {
    runtime: StdMutex<Runtime>,
}

#[derive(Default)]
struct Runtime {
    /// The generation each kind's tasks belong to. A connect bumps it, so an
    /// older poller or watcher sees it is stale and stops.
    generation: HashMap<String, u64>,
    pairing: Option<Pairing>,
    buttons: HashMap<String, Button>,
    owed: HashMap<String, Vec<Owed>>,
}

struct Pairing {
    kind: String,
    code: String,
    hash: String,
    link: String,
    expires: Instant,
    expires_at: String,
    attempts: u8,
}

struct Button {
    permission_id: String,
    expires: Instant,
}

/// One inbound message a reply is owed for. The turn id is known unless the
/// message was queued behind a running turn.
struct Owed {
    turn_id: Option<String>,
}

impl Channels {
    fn lock(&self) -> MutexGuard<'_, Runtime> {
        self.runtime.lock().unwrap_or_else(|error| error.into_inner())
    }

    /// Starts a new generation for `kind`: every older task stops at its next
    /// check, and nothing older is owed any more. The pairing window is the
    /// owner's and survives a restart of the poller; `stop` closes it.
    fn begin(&self, kind: &str) -> u64 {
        let mut rt = self.lock();
        let next = rt.generation.get(kind).copied().unwrap_or(0) + 1;
        rt.generation.insert(kind.to_owned(), next);
        rt.owed.entry(kind.to_owned()).or_default().clear();
        rt.buttons.clear();
        next
    }

    /// Ends the channel: its tasks stop and the pairing window closes.
    fn stop(&self, kind: &str) {
        self.begin(kind);
        let mut rt = self.lock();
        if rt.pairing.as_ref().is_some_and(|pairing| pairing.kind == kind) { rt.pairing = None; }
    }

    fn alive(&self, kind: &str, generation: u64) -> bool {
        self.lock().generation.get(kind).copied() == Some(generation)
    }

    fn owe(&self, kind: &str) {
        let mut rt = self.lock();
        rt.owed.entry(kind.to_owned()).or_default().push(Owed { turn_id: None });
    }

    /// Names the turn the reservation belongs to, once the prompt knows it. The
    /// reservation is made before the prompt, so a turn that finishes within the
    /// send cannot outrun it.
    fn owe_turn(&self, kind: &str, turn: Option<String>) {
        let Some(turn) = turn else { return };
        let mut rt = self.lock();
        if let Some(entry) = rt.owed.get_mut(kind).and_then(|owed| owed.last_mut()) {
            if entry.turn_id.is_none() { entry.turn_id = Some(turn); }
        }
    }

    /// Drops the reservation a message that never reached the CEO took.
    fn unowe(&self, kind: &str) {
        if let Some(owed) = self.lock().owed.get_mut(kind) { owed.pop(); }
    }

    /// Takes the inbound entry a finished turn answers. A known turn id must
    /// match; an unknown one (a queued message, or a provider that reports no
    /// turn) answers the oldest entry that has none.
    fn settle(&self, kind: &str, turn: Option<&str>) -> bool {
        let mut rt = self.lock();
        let Some(owed) = rt.owed.get_mut(kind) else { return false };
        let found = match turn {
            Some(turn) => owed.iter().position(|entry| entry.turn_id.as_deref() == Some(turn))
                .or_else(|| owed.iter().position(|entry| entry.turn_id.is_none())),
            None => owed.iter().position(|entry| entry.turn_id.is_none()).or(if owed.is_empty() { None } else { Some(0) }),
        };
        match found {
            Some(index) => { owed.remove(index); true }
            None => false,
        }
    }

    /// An opaque one-use id for one card, and the mark on the two buttons.
    fn button_for(&self, permission_id: &str) -> String {
        let token = uuid::Uuid::new_v4().simple().to_string();
        let mut rt = self.lock();
        let now = Instant::now();
        // Buttons whose card expired long ago are forgotten here, so the map
        // cannot grow without bound.
        rt.buttons.retain(|_, button| button.expires > now);
        rt.buttons.insert(token.clone(), Button { permission_id: permission_id.to_owned(), expires: now + Duration::from_secs(3600) });
        token
    }

    /// Takes an approval button: one use, and only while it is still fresh.
    fn take_button(&self, token: &str) -> Option<String> {
        let mut rt = self.lock();
        let button = rt.buttons.remove(token)?;
        (button.expires > Instant::now()).then_some(button.permission_id)
    }

    fn forget_card(&self, permission_id: &str) {
        self.lock().buttons.retain(|_, button| button.permission_id != permission_id);
    }
}

// ---------------------------------------------------------------------------
// Small helpers

fn now() -> String { Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true) }

fn invalid(message: &str) -> DispatchError { derr("invalid_argument", message, StatusCode::BAD_REQUEST) }

fn conflict(message: &str) -> DispatchError { derr("conflict", message, StatusCode::CONFLICT) }

fn unavailable(message: &str) -> DispatchError { derr("unavailable", message, StatusCode::SERVICE_UNAVAILABLE) }

fn unsupported(message: &str) -> DispatchError { derr("unsupported", message, StatusCode::BAD_REQUEST) }

fn storage_error(_: retinue_storage::StorageError) -> DispatchError {
    derr("internal", "The channel store could not be read or written.", StatusCode::INTERNAL_SERVER_ERROR)
}

fn kind_of(p: &Value) -> Result<&str, DispatchError> {
    p["kind"].as_str().filter(|kind| KINDS.contains(kind)).ok_or_else(|| invalid("kind must be telegram, whatsapp-web or whatsapp-cloud."))
}

/// The credential store's name for one of a channel's fields: `Retinue/` is
/// added by the store itself.
fn secret_name(kind: &str, field: &str) -> String { format!("channel/{kind}/{field}") }

pub(crate) fn read_secret(kind: &str, field: &str) -> Option<Zeroizing<String>> {
    secrets::store::read(&secret_name(kind, field)).ok().flatten()
}

fn write_secret(kind: &str, field: &str, value: &str) -> Result<(), DispatchError> {
    secrets::store::write(&secret_name(kind, field), value).map_err(|_| unavailable("The credential store could not save that, so the channel was not started."))
}

fn delete_secret(kind: &str, field: &str) { let _ = secrets::store::delete(&secret_name(kind, field)); }

/// A token or key typed into Settings: printable, no spaces, bounded.
fn valid_secret(value: &str, max: usize) -> bool {
    !value.is_empty() && value.len() <= max && !value.chars().any(|ch| ch.is_control() || ch.is_whitespace())
}

/// The CEO, or the sentence the contract names.
fn ceo(st: &AppState) -> Result<Value, DispatchError> {
    st.db.manager_agent().map_err(storage_error)?.ok_or_else(|| conflict("Hire your CEO first."))
}

fn row_out(st: &AppState, kind: &str) -> Result<Value, DispatchError> {
    let stored = st.db.channel_get(kind).map_err(storage_error)?.unwrap_or_else(|| {
        json!({"kind": kind, "status": "off", "detail": null, "account": null, "peer": null, "approvals": true, "sessionId": null, "candidate": null, "lastMessageAt": null})
    });
    let status = stored["status"].as_str().unwrap_or("off").to_owned();
    let pairing = {
        let rt = st.channels.lock();
        rt.pairing.as_ref().filter(|pairing| pairing.kind == kind && pairing.expires > Instant::now()).map(|pairing| {
            json!({"link": pairing.link, "code": pairing.code, "expiresAt": pairing.expires_at})
        })
    };
    // The public peer is a name and a handle; its numeric id stays in the store.
    let peer = stored["peer"].as_object().map(|peer| json!({"name": peer.get("name").cloned().unwrap_or(Value::Null), "handle": peer.get("handle").cloned().unwrap_or(Value::Null)}));
    let candidate = stored["candidate"].as_object().map(|candidate| json!({
        "name": candidate.get("name").cloned().unwrap_or(Value::Null),
        "handle": candidate.get("handle").cloned().unwrap_or(Value::Null),
        "at": candidate.get("at").cloned().unwrap_or(Value::Null)
    }));
    let detail = stored["detail"].as_str().map(|detail| detail.chars().take(DETAIL_MAX).collect::<String>());
    Ok(json!({
        "kind": kind,
        "status": status,
        "detail": detail,
        "account": stored["account"],
        "peer": peer,
        "approvals": stored["approvals"].as_bool().unwrap_or(true),
        "sessionId": stored["sessionId"],
        "qrDataUrl": Value::Null,
        "pairing": pairing,
        "candidate": candidate,
        "lastMessageAt": stored["lastMessageAt"],
    }))
}

async fn announce(st: &AppState, kind: &str) {
    if let Ok(channel) = row_out(st, kind) { st.emit("channel.updated", json!({"channel": channel})).await; }
}

/// Reads the stored row, applies `change`, writes it back and announces it.
/// Every status change goes through here, so the UI and `channel.list` never
/// disagree.
async fn update_row<F>(st: &AppState, kind: &str, change: F) -> Result<Value, DispatchError>
where
    F: FnOnce(&mut Value),
{
    let mut row = st.db.channel_get(kind).map_err(storage_error)?.unwrap_or_else(|| {
        json!({"kind": kind, "status": "off", "detail": null, "account": null, "peer": null, "approvals": true, "sessionId": null, "candidate": null, "lastMessageAt": null})
    });
    change(&mut row);
    row["kind"] = json!(kind);
    row["updatedAt"] = json!(now());
    st.db.channel_put(kind, &row).map_err(storage_error)?;
    announce(st, kind).await;
    row_out(st, kind)
}

// ---------------------------------------------------------------------------
// The Telegram client

/// The one client that talks to Telegram. `base` is the real API in production
/// and a local server in tests; the token is only ever in the URL path of this
/// client and never in an error, a log or a message.
pub(crate) struct Telegram {
    base: String,
    token: Zeroizing<String>,
    http: reqwest::Client,
}

impl Telegram {
    pub(crate) fn new(base: &str, token: &str) -> Result<Self, String> {
        let http = reqwest::Client::builder()
            .redirect(Policy::none())
            .user_agent("Retinue")
            .timeout(Duration::from_secs(POLL_TIMEOUT_SECS))
            .build()
            .map_err(|_| "The Telegram client could not be created.".to_owned())?;
        Ok(Self { base: base.trim_end_matches('/').to_owned(), token: Zeroizing::new(token.to_owned()), http })
    }

    /// One Bot API call. `timeout` must exceed the long poll's own timeout.
    async fn call_at(&self, method: &str, body: &Value, timeout: Duration) -> Result<Value, String> {
        let url = format!("{}/bot{}/{}", self.base, self.token.as_str(), method);
        let response = self.http.post(&url).timeout(timeout).json(body).send().await.map_err(|error| {
            if error.is_timeout() { "Telegram took too long to answer.".to_owned() } else { "Telegram could not be reached.".to_owned() }
        })?;
        let status = response.status();
        let text = response.text().await.map_err(|_| "Telegram's answer could not be read.".to_owned())?;
        if text.len() > 1_000_000 { return Err("Telegram's answer was too large to read.".to_owned()); }
        let value: Value = serde_json::from_str(&text).map_err(|_| format!("Telegram sent an answer Retinue could not read (HTTP {}).", status.as_u16()))?;
        if value["ok"] == true { return Ok(value["result"].clone()); }
        let description = value["description"].as_str().unwrap_or("no reason was given");
        Err(format!("Telegram refused that: {}", self.safe(description)))
    }

    async fn call(&self, method: &str, body: &Value) -> Result<Value, String> {
        self.call_at(method, body, Duration::from_secs(POLL_TIMEOUT_SECS + 10)).await
    }

    /// Telegram's own words, bounded and with anything that looks like the token
    /// or a credential replaced.
    fn safe(&self, text: &str) -> String {
        let cleaned: String = text.chars().filter(|ch| !ch.is_control()).collect();
        let cleaned = cleaned.replace(self.token.as_str(), "[hidden]");
        let cleaned = crate::redact_secret_words(&cleaned);
        cleaned.chars().take(200).collect()
    }

    async fn updates(&self, offset: i64) -> Result<Vec<Value>, String> {
        let body = json!({"offset": offset, "timeout": POLL_SECS, "limit": 50, "allowed_updates": ["message", "callback_query"]});
        let value = self.call_at("getUpdates", &body, Duration::from_secs(POLL_TIMEOUT_SECS)).await?;
        Ok(value.as_array().cloned().unwrap_or_default())
    }

    async fn send(&self, chat_id: &str, text: &str, markup: Option<Value>) -> Result<(), String> {
        let mut body = json!({"chat_id": chat_id, "text": text, "disable_web_page_preview": true});
        if let Some(markup) = markup { body["reply_markup"] = markup; }
        self.call("sendMessage", &body).await.map(|_| ())
    }

    async fn answer_callback(&self, callback_id: &str, text: &str) {
        let _ = self.call("answerCallbackQuery", &json!({"callback_query_id": callback_id, "text": text})).await;
    }
}

fn telegram_for(kind: &str) -> Result<Telegram, String> {
    let Some(token) = read_secret(kind, "token") else { return Err("The channel's token is missing. Connect it again in Settings.".to_owned()) };
    Telegram::new(&api_base(), token.as_str())
}

/// Where the Telegram client points. Only a test can move it, and it is the same
/// address the connect check and every later call use, so a token can never be
/// sent anywhere but Telegram.
fn api_base() -> String {
    #[cfg(test)]
    { if let Some(base) = TEST_API_BASE.lock().ok().and_then(|base| base.clone()) { return base; } }
    TELEGRAM_API.to_owned()
}

/// The Graph API address, moved only by a test.
fn cloud_api_base() -> String {
    #[cfg(test)]
    { if let Some(base) = TEST_CLOUD_API_BASE.lock().ok().and_then(|base| base.clone()) { return base; } }
    CLOUD_API.to_owned()
}

#[cfg(test)]
pub(crate) static TEST_API_BASE: StdMutex<Option<String>> = StdMutex::new(None);
#[cfg(test)]
pub(crate) static TEST_CLOUD_API_BASE: StdMutex<Option<String>> = StdMutex::new(None);

// ---------------------------------------------------------------------------
// RPC

pub async fn rpc(st: &AppState, method: &str, p: &Value) -> Result<Value, DispatchError> {
    match method {
        "channel.list" => {
            let mut channels = Vec::with_capacity(KINDS.len());
            for kind in KINDS { channels.push(row_out(st, kind)?); }
            Ok(json!({"channels": channels}))
        }
        "channel.telegram.connect" => connect_telegram(st, p).await,
        "channel.whatsappWeb.start" => Err(unsupported(
            "WhatsApp Web isn't available in this build: it needs a WhatsApp client Retinue doesn't ship yet. Use the WhatsApp Cloud API, or ask for WhatsApp Web to be added.",
        )),
        "channel.whatsappCloud.connect" => connect_cloud(st, p).await,
        "channel.pairing.answer" => answer_pairing(st, p).await,
        "channel.update" => {
            let kind = kind_of(p)?;
            st.db.channel_get(kind).map_err(storage_error)?.ok_or_else(|| conflict("That channel isn't set up."))?;
            let approvals = match p.get("approvals") {
                None | Some(Value::Null) => None,
                Some(value) => Some(value.as_bool().ok_or_else(|| invalid("approvals must be true or false."))?),
            };
            let channel = update_row(st, kind, |row| {
                if let Some(approvals) = approvals { row["approvals"] = json!(approvals); }
            }).await?;
            Ok(json!({"channel": channel}))
        }
        "channel.disconnect" => disconnect(st, p).await,
        "channel.test" => test_channel(st, p).await,
        _ => Err(derr("not_found", "Unknown channel method.", StatusCode::NOT_FOUND)),
    }
}

async fn connect_telegram(st: &AppState, p: &Value) -> Result<Value, DispatchError> {
    ceo(st)?;
    let token = p["botToken"].as_str().map(str::trim).filter(|token| valid_secret(token, TOKEN_MAX))
        .ok_or_else(|| invalid("botToken is required: paste the token BotFather gave you."))?;
    let api = Telegram::new(&api_base(), token).map_err(|error| unavailable(&error))?;
    // The row says `connecting` before the first call, so the page shows what is
    // happening even if Telegram is slow.
    update_row(st, "telegram", |row| {
        row["status"] = json!("connecting");
        row["detail"] = json!(null);
    }).await?;
    let me = match api.call("getMe", &json!({})).await {
        Ok(me) => me,
        Err(error) => {
            update_row(st, "telegram", |row| {
                row["status"] = json!("error");
                row["detail"] = json!(error.chars().take(DETAIL_MAX).collect::<String>());
            }).await?;
            return Err(conflict("Telegram refused that token."));
        }
    };
    if me["is_bot"] != true {
        // The row must not be left saying `connecting` for a refusal that will
        // never finish.
        update_row(st, "telegram", |row| {
            row["status"] = json!("error");
            row["detail"] = json!("That token isn't a bot's: paste the token BotFather gave you.");
        }).await?;
        return Err(invalid("That token isn't a bot's: paste the token BotFather gave you."));
    }
    let username = me["username"].as_str().map(|name| format!("@{name}")).ok_or_else(|| invalid("That bot has no username, so a chat cannot reach it."))?;
    if username.len() > ACCOUNT_MAX { return Err(invalid("That bot's username is longer than a Telegram username can be.")); }
    let token = Zeroizing::new(token.to_owned());
    write_secret("telegram", "token", token.as_str())?;

    // A different bot means the old paired sender belongs to someone else's chat.
    let previous = st.db.channel_get("telegram").map_err(storage_error)?;
    let same_bot = previous.as_ref().and_then(|row| row["account"].as_str()) == Some(username.as_str());
    let paired = if same_bot { previous.as_ref().map(|row| row["peer"].clone()) } else { None };
    let paired = paired.filter(|peer| peer.is_object());
    let session = if same_bot { previous.as_ref().map(|row| row["sessionId"].clone()) } else { None };
    let session = session.filter(|id| id.is_string());
    let channel = update_row(st, "telegram", |row| {
        row["account"] = json!(username);
        row["peer"] = paired.clone().unwrap_or(Value::Null);
        row["sessionId"] = session.unwrap_or(Value::Null);
        row["candidate"] = json!(null);
        row["detail"] = json!(null);
        row["status"] = json!(if paired.is_some() { "connected" } else { "pairing" });
    }).await?;
    // One code at a time, with only its hash kept; the code itself lives in
    // memory for ten minutes so the page can show it again.
    let pairing = if channel["peer"].is_null() { Some(issue_pairing(st, "telegram", channel["account"].as_str().unwrap_or(""))?) } else { None };
    start_telegram(st);
    Ok(json!({"channel": row_out(st, "telegram")?, "pairing": pairing}))
}

/// Issues the deep link and the code for a Telegram pairing window.
fn issue_pairing(st: &AppState, kind: &str, account: &str) -> Result<Value, DispatchError> {
    let mut code = String::with_capacity(8);
    {
        use rand::Rng;
        let mut rng = rand::rng();
        for _ in 0..8 { code.push(b"23456789ABCDEFGHJKLMNPQRSTUVWXYZ"[rng.random_range(0..32)] as char); }
    }
    let expires_at = (Utc::now() + ChronoDuration::minutes(PAIRING_MINUTES)).to_rfc3339_opts(SecondsFormat::Millis, true);
    let bot = account.trim_start_matches('@');
    let link = format!("https://t.me/{bot}?start={code}");
    let hash = hash_code(&code);
    {
        let mut rt = st.channels.lock();
        rt.pairing = Some(Pairing {
            kind: kind.to_owned(),
            code: code.clone(),
            hash,
            link: link.clone(),
            expires: Instant::now() + Duration::from_secs((PAIRING_MINUTES * 60) as u64),
            expires_at: expires_at.clone(),
            attempts: 0,
        });
    }
    Ok(json!({"link": link, "code": code, "expiresAt": expires_at}))
}

/// The only form of a code the daemon compares against: its SHA-256, as hex.
fn hash_code(code: &str) -> String {
    let mut hex = String::with_capacity(64);
    for byte in Sha256::digest(code.as_bytes()) { hex.push_str(&format!("{byte:02x}")); }
    hex
}

enum Attempt { Matched, Wrong, NoCode }

/// One `/start <code>`: constant-time against the stored hash, five tries, then
/// the window closes.
fn pairing_attempt(st: &AppState, kind: &str, code: &str) -> Attempt {
    let mut rt = st.channels.lock();
    let (expires, attempts, stored) = match rt.pairing.as_ref().filter(|pairing| pairing.kind == kind) {
        Some(pairing) => (pairing.expires, pairing.attempts, pairing.hash.clone()),
        None => return Attempt::NoCode,
    };
    if expires <= Instant::now() || attempts >= PAIRING_ATTEMPTS {
        rt.pairing = None;
        return Attempt::NoCode;
    }
    let candidate = hash_code(code.trim().to_ascii_uppercase().as_str());
    if stored.len() == candidate.len() && candidate.as_bytes().ct_eq(stored.as_bytes()).into() { return Attempt::Matched }
    let attempts = attempts + 1;
    if attempts >= PAIRING_ATTEMPTS { rt.pairing = None; return Attempt::NoCode }
    if let Some(pairing) = rt.pairing.as_mut() { pairing.attempts = attempts; }
    Attempt::Wrong
}

async fn answer_pairing(st: &AppState, p: &Value) -> Result<Value, DispatchError> {
    let kind = kind_of(p)?;
    let approve = match p.get("approve") {
        Some(Value::Bool(value)) => *value,
        _ => return Err(invalid("approve must be true or false.")),
    };
    let stored = st.db.channel_get(kind).map_err(storage_error)?.ok_or_else(|| conflict("That channel isn't set up."))?;
    if stored["candidate"].is_null() { return Err(conflict("Nobody has used the code yet, so there is nothing to approve.")); }
    if approve {
        let candidate = stored["candidate"].clone();
        let channel = update_row(st, kind, |row| {
            row["peer"] = json!({
                "id": candidate["id"],
                "name": candidate["name"],
                "handle": candidate["handle"],
            });
            row["candidate"] = json!(null);
            row["status"] = json!("connected");
            row["detail"] = json!(null);
        }).await?;
        // The pairing window is closed: the first approved sender is the one.
        { st.channels.lock().pairing = None; }
        if kind == "telegram" { let _ = send_to_peer(st, kind, "Retinue is connected. Send a message and it reaches your CEO.").await; }
        return Ok(json!({"channel": channel}));
    }
    let channel = update_row(st, kind, |row| {
        row["candidate"] = json!(null);
        row["status"] = json!("pairing");
        row["detail"] = json!("That sender was refused. Start the code again to try another.");
    }).await?;
    Ok(json!({"channel": channel}))
}

async fn disconnect(st: &AppState, p: &Value) -> Result<Value, DispatchError> {
    let kind = kind_of(p)?;
    // Stop the tasks first, so nothing writes the row after it is gone.
    st.channels.stop(kind);
    for field in ["token", "accessToken", "appSecret", "verifyToken", "phoneNumberId"] { delete_secret(kind, field); }
    st.db.channel_delete(kind).map_err(storage_error)?;
    announce(st, kind).await;
    Ok(json!({"channel": row_out(st, kind)?}))
}

async fn test_channel(st: &AppState, p: &Value) -> Result<Value, DispatchError> {
    let kind = kind_of(p)?;
    let stored = st.db.channel_get(kind).map_err(storage_error)?.ok_or_else(|| conflict("That channel isn't set up."))?;
    if stored["peer"].is_null() { return Err(conflict("No sender is paired yet, so there is nowhere to send.")); }
    send_to_peer(st, kind, "Retinue is connected.").await.map_err(|error| conflict(&error))?;
    Ok(json!({"sent": true}))
}

// ---------------------------------------------------------------------------
// WhatsApp Cloud API: connect and the outbound half

async fn connect_cloud(st: &AppState, p: &Value) -> Result<Value, DispatchError> {
    ceo(st)?;
    let phone_number_id = p["phoneNumberId"].as_str().map(str::trim).filter(|id| !id.is_empty() && id.len() <= 32 && id.bytes().all(|byte| byte.is_ascii_digit()))
        .ok_or_else(|| invalid("phoneNumberId must be the numeric id of your WhatsApp phone number."))?;
    let access_token = p["accessToken"].as_str().map(str::trim).filter(|token| valid_secret(token, TOKEN_MAX))
        .ok_or_else(|| invalid("accessToken is required: it is the Meta token for that phone number."))?;
    let app_secret = p["appSecret"].as_str().map(str::trim).filter(|secret| (16..=128).contains(&secret.len()) && secret.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .ok_or_else(|| invalid("appSecret must be Meta's app secret: 16 to 128 hexadecimal characters."))?;
    update_row(st, "whatsapp-cloud", |row| { row["status"] = json!("connecting"); row["detail"] = json!(null); }).await?;
    let http = reqwest::Client::builder().redirect(Policy::none()).user_agent("Retinue").timeout(Duration::from_secs(20)).build()
        .map_err(|_| unavailable("The WhatsApp client could not be created."))?;
    let reply = http.get(format!("{}/{phone_number_id}?fields=display_phone_number,verified_name", cloud_api_base()))
        .bearer_auth(access_token)
        .send().await;
    let display = match reply {
        Ok(response) if response.status().is_success() => {
            let body: Value = response.json().await.unwrap_or(Value::Null);
            body["display_phone_number"].as_str().map(mask_phone)
        }
        Ok(response) => {
            let status = response.status().as_u16();
            update_row(st, "whatsapp-cloud", |row| {
                row["status"] = json!("error");
                row["detail"] = json!(format!("Meta refused that token (HTTP {status}). Check the phone number id and the token, then connect again."));
            }).await?;
            return Err(conflict("Meta refused that token."));
        }
        Err(_) => {
            update_row(st, "whatsapp-cloud", |row| {
                row["status"] = json!("error");
                row["detail"] = json!("Meta could not be reached. Check this computer's connection, then connect again.");
            }).await?;
            return Err(unavailable("Meta could not be reached."));
        }
    };
    write_secret("whatsapp-cloud", "accessToken", access_token)?;
    write_secret("whatsapp-cloud", "appSecret", app_secret)?;
    write_secret("whatsapp-cloud", "phoneNumberId", phone_number_id)?;
    if read_secret("whatsapp-cloud", "verifyToken").is_none() {
        let verify: String = { use rand::Rng; let mut rng = rand::rng(); (0..32).map(|_| format!("{:x}", rng.random_range(0..16))).collect() };
        write_secret("whatsapp-cloud", "verifyToken", &verify)?;
    }
    let account = display.clone().unwrap_or_else(|| mask_phone(phone_number_id));
    let channel = update_row(st, "whatsapp-cloud", |row| {
        row["account"] = json!(account);
        row["status"] = json!("pairing");
        row["detail"] = json!("Sending works. Reading messages needs Meta's webhook, which this build does not have yet, so no sender can be paired here.");
    }).await?;
    // The verify token is Meta's, not a secret value, but it is only shown once
    // to the owner: the URL is null because no public address exists yet.
    let verify_token = read_secret("whatsapp-cloud", "verifyToken").map(|token| token.to_string());
    Ok(json!({"channel": channel, "webhook": {"url": Value::Null, "verifyToken": verify_token}}))
}

fn mask_phone(number: &str) -> String {
    let digits: String = number.chars().filter(|ch| ch.is_ascii_digit()).collect();
    if digits.len() < 8 { return number.to_owned() }
    // The country code and the last four digits, which is what the owner needs
    // to recognise the number; nothing between them is shown.
    let country = digits.chars().take(2).collect::<String>();
    let tail = digits.chars().skip(digits.len() - 4).collect::<String>();
    format!("+{country} \u{2022}\u{2022}\u{2022}\u{2022} {tail}")
}

/// The outbound half of the Cloud API: one text message to one number. A reply
/// outside Meta's 24-hour window is refused by Meta, and its refusal is passed
/// on as a plain sentence rather than retried.
pub(crate) async fn cloud_send_text(base: &str, phone_number_id: &str, token: &str, to: &str, text: &str) -> Result<String, String> {
    let http = reqwest::Client::builder().redirect(Policy::none()).user_agent("Retinue").timeout(Duration::from_secs(20)).build()
        .map_err(|_| "The WhatsApp client could not be created.".to_owned())?;
    let body = json!({"messaging_product": "whatsapp", "recipient_type": "individual", "to": to, "type": "text", "text": {"preview_url": false, "body": text}});
    let response = http.post(format!("{}/{}/messages", base.trim_end_matches('/'), phone_number_id))
        .bearer_auth(token).json(&body).send().await
        .map_err(|_| "WhatsApp could not be reached.".to_owned())?;
    let status = response.status();
    let value: Value = response.json().await.unwrap_or(Value::Null);
    if status.is_success() {
        let id = value["messages"][0]["id"].as_str().unwrap_or("").to_owned();
        return Ok(id);
    }
    let reason = value["error"]["message"].as_str().map(|text| text.chars().take(200).collect::<String>()).unwrap_or_else(|| format!("HTTP {}", status.as_u16()));
    Err(format!("WhatsApp refused that: {}", crate::redact_secret_words(&reason)))
}

async fn cloud_send(to: &str, text: &str) -> Result<(), String> {
    let token = read_secret("whatsapp-cloud", "accessToken").ok_or("The WhatsApp token is missing. Connect the channel again in Settings.")?;
    let phone = read_secret("whatsapp-cloud", "phoneNumberId").ok_or("The WhatsApp phone number id is missing. Connect the channel again in Settings.")?;
    cloud_send_text(&cloud_api_base(), phone.as_str(), token.as_str(), to, text).await.map(|_| ())
}

// ---------------------------------------------------------------------------
// Sending to the paired sender

async fn send_to_peer(st: &AppState, kind: &str, text: &str) -> Result<(), String> {
    let stored = st.db.channel_get(kind).map_err(|_| "The channel could not be read.".to_owned())?;
    let Some(stored) = stored else { return Err("That channel isn't set up.".to_owned()) };
    let Some(peer) = stored["peer"].as_object() else { return Err("No sender is paired yet.".to_owned()) };
    let text = chat_text(text);
    if text.trim().is_empty() { return Ok(()) }
    match kind {
        "telegram" => {
            let api = telegram_for(kind)?;
            let chat_id = peer.get("id").and_then(|id| id.as_str().map(str::to_owned).or_else(|| id.as_i64().map(|id| id.to_string())))
                .ok_or("The paired sender has no chat to answer in.")?;
            for chunk in chunks(&text, CHAT_CHUNK) {
                api.send(&chat_id, &chunk, None).await?;
            }
            touch_last_message(st, kind).await;
            Ok(())
        }
        "whatsapp-cloud" => {
            let to = peer.get("id").and_then(Value::as_str).ok_or("The paired sender has no number to answer.")?.to_owned();
            for chunk in chunks(&text, CHAT_CHUNK) { cloud_send(&to, &chunk).await?; }
            touch_last_message(st, kind).await;
            Ok(())
        }
        _ => Err("That channel has no way to send in this build.".to_owned()),
    }
}

async fn touch_last_message(st: &AppState, kind: &str) {
    let _ = update_row(st, kind, |row| { row["lastMessageAt"] = json!(now()); }).await;
}

// ---------------------------------------------------------------------------
// Outbound text hygiene

fn path_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r#"[A-Za-z]:\\[^\s"'<>|`]+|\\\\[^\s"'<>|`]+"#).expect("path pattern"))
}

fn local_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r#"https?://(?:127\.0\.0\.1|localhost|\[::1\])[^\s)>\]]*|file://[^\s)>\]]*"#).expect("local address pattern"))
}

/// A reply as a chat may see it: no local paths, no loopback or file links, no
/// word that looks like a credential. The chat lives outside this computer, so
/// what only makes sense inside it is replaced.
pub(crate) fn chat_text(text: &str) -> String {
    let replaced = path_pattern().replace_all(text, "[path]");
    let replaced = local_pattern().replace_all(&replaced, "[local address]");
    crate::redact_secret_words(&replaced)
}

/// Splits a reply at Telegram's and WhatsApp's message size, at a line break
/// where one is near, and never in the middle of a character.
pub(crate) fn chunks(text: &str, limit: usize) -> Vec<String> {
    let mut parts = Vec::new();
    let mut rest = text.trim();
    while rest.chars().count() > limit {
        // The byte offset just past the character at `limit`.
        let mut cut = rest.char_indices().take(limit).last().map(|(index, ch)| index + ch.len_utf8()).unwrap_or(0);
        if let Some(position) = rest[..cut].rfind('\n').filter(|position| *position > cut / 2) { cut = position; }
        let part = rest[..cut].trim_end();
        if !part.is_empty() { parts.push(part.to_owned()); }
        rest = rest[cut..].trim_start_matches('\n').trim_start();
    }
    if !rest.is_empty() { parts.push(rest.to_owned()); }
    parts
}

// ---------------------------------------------------------------------------
// The running channel

/// Starts the poller and the event watcher for a channel that has a credential.
/// Called when the owner connects and at boot.
fn start_telegram(st: &AppState) {
    let generation = st.channels.begin("telegram");
    let poller = st.clone();
    tokio::spawn(async move { telegram_loop(poller, generation).await });
    let watcher = st.clone();
    tokio::spawn(async move { event_loop(watcher, "telegram".to_owned(), generation).await });
}

/// At boot: a channel that still has a credential picks up where it left off, so
/// a restart does not silently stop the owner's chat.
pub async fn start_at_boot(st: &AppState) {
    let Some(row) = st.db.channel_get("telegram").map_err(|_| ()).ok().flatten() else { return };
    if read_secret("telegram", "token").is_none() {
        // The credential is gone (cleared by hand, or by another profile): say so
        // rather than showing a connected channel that cannot work.
        if row["status"] != "off" {
            let _ = update_row(st, "telegram", |row| {
                row["status"] = json!("error");
                row["detail"] = json!("The channel's token is missing. Connect it again in Settings.");
                row["peer"] = json!(null);
                row["sessionId"] = json!(null);
            }).await;
        }
        return;
    }
    start_telegram(st);
    // A channel still waiting for its first sender gets a fresh window, so the
    // owner can pair after a restart without connecting again.
    if row["peer"].is_null() {
        if let Some(account) = row["account"].as_str().filter(|account| !account.is_empty()) {
            let account = account.to_owned();
            let _ = issue_pairing(st, "telegram", &account);
        }
        let _ = update_row(st, "telegram", |row| { row["status"] = json!("pairing"); }).await;
    }
}

/// The Telegram long poll, and nothing else: no listener, no webhook.
async fn telegram_loop(st: AppState, generation: u64) {
    let Some(token) = read_secret("telegram", "token") else { return };
    let api = match Telegram::new(&api_base(), token.as_str()) {        Ok(api) => api,
        Err(error) => { let _ = set_detail(&st, "telegram", "error", &error).await; return }
    };
    let mut offset: i64 = 0;
    let mut failures: u32 = 0;
    loop {
        if !st.channels.alive("telegram", generation) { return }
        match api.updates(offset).await {
            Ok(updates) => {
                if failures > 0 { failures = 0; let _ = mark_reachable(&st, "telegram").await; }
                for update in updates {
                    if let Some(id) = update["update_id"].as_i64() { offset = offset.max(id + 1); }
                    if !st.channels.alive("telegram", generation) { return }
                    handle_update(&st, &api, &update).await;
                }
            }
            Err(error) => {
                failures = failures.saturating_add(1);
                let _ = set_detail(&st, "telegram", "error", &format!("{error} Retinue keeps trying.")).await;
                // Wait, but notice a disconnect within half a second.
                let wait = Duration::from_secs(match failures { 1 => 2, 2 => 5, _ => 15 });
                let started = Instant::now();
                while started.elapsed() < wait {
                    if !st.channels.alive("telegram", generation) { return }
                    tokio::time::sleep(Duration::from_millis(250)).await;
                }
            }
        }
    }
}

async fn mark_reachable(st: &AppState, kind: &str) -> Result<Value, DispatchError> {
    let Some(row) = st.db.channel_get(kind).map_err(storage_error)? else { return Ok(Value::Null) };
    if row["status"] != "error" { return Ok(Value::Null) }
    let status = if row["peer"].is_null() { "pairing" } else { "connected" };
    update_row(st, kind, |row| { row["status"] = json!(status); row["detail"] = json!(null); }).await
}

/// Writes a status and detail, but only when it changed: a channel that cannot
/// reach its provider would otherwise announce the same thing every few seconds.
async fn set_detail(st: &AppState, kind: &str, status: &str, detail: &str) -> Result<Value, DispatchError> {
    let existing = st.db.channel_get(kind).map_err(storage_error)?;
    let detail: String = detail.chars().take(DETAIL_MAX).collect();
    if existing.as_ref().is_some_and(|row| row["status"] == status && row["detail"] == detail) { return Ok(Value::Null) }
    update_row(st, kind, |row| { row["status"] = json!(status); row["detail"] = json!(detail); }).await
}

/// One update from Telegram. Every refusal is structural: the chat must be a
/// private one, the sender must be its owner, and while nothing is paired only a
/// code is looked at.
pub(crate) async fn handle_update(st: &AppState, api: &Telegram, update: &Value) {
    if let Some(callback) = update.get("callback_query").filter(|value| value.is_object()) {
        answer_callback(st, api, callback).await;
        return;
    }
    let Some(message) = update.get("message").filter(|value| value.is_object()) else { return };
    // Groups, channels and supergroups never reach the CEO.
    if message["chat"]["type"].as_str() != Some("private") { return }
    let from = message["from"]["id"].as_i64();
    let chat = message["chat"]["id"].as_i64();
    if from.is_none() || from != chat { return }
    if message["from"]["is_bot"] == true { return }
    let Some(message_id) = message["message_id"].as_i64() else { return };
    let text = message["text"].as_str().unwrap_or("").trim().to_owned();
    let sender = json!({
        "id": from.map(|id| id.to_string()).unwrap_or_default(),
        "name": sender_name(&message["from"]),
        "handle": message["from"]["username"].as_str().map(|name| format!("@{name}")),
    });
    let stored = match st.db.channel_get("telegram") { Ok(Some(row)) => row, _ => return };

    // Nothing at all is accepted until the owner approves one sender.
    if stored["peer"].is_null() {
        if let Some(code) = start_code(&text) {
            match pairing_attempt(st, "telegram", &code) {
                Attempt::Matched => {
                    let at = now();
                    let mut candidate = sender.clone();
                    candidate["at"] = json!(at);
                    let _ = update_row(st, "telegram", |row| {
                        row["candidate"] = candidate.clone();
                        row["status"] = json!("pairing");
                        row["detail"] = json!(format!("A sender used the code. Approve {} on the desktop.", candidate["name"].as_str().unwrap_or("them")));
                    }).await;
                    st.emit("channel.pairing.candidate", json!({"kind": "telegram", "name": candidate["name"], "handle": candidate["handle"], "at": candidate["at"]})).await;
                }
                // A wrong code, an expired window and a stranger all look the
                // same from outside, and none of them answers.
                Attempt::Wrong | Attempt::NoCode => {}
            }
        }
        return;
    }
    // The one paired sender, checked again here even though nothing else got in.
    let paired_id = stored["peer"]["id"].as_str().map(str::to_owned).or_else(|| stored["peer"]["id"].as_i64().map(|id| id.to_string()));
    if paired_id.as_deref() != from.map(|id| id.to_string()).as_deref() { return }

    if text.eq_ignore_ascii_case("/start") || text.to_ascii_lowercase().starts_with("/start@") {
        let _ = send_to_peer(st, "telegram", "You are already connected.").await;
        return;
    }
    if text.is_empty() { return }
    let account = stored["account"].as_str().unwrap_or("bot").trim_start_matches('@').to_owned();
    let source_id = format!("channel:telegram:{account}:{message_id}");
    match st.db.channel_inbox_claim(&source_id, "telegram", &message_id.to_string(), &now()) {
        Ok(true) => {}
        // The same delivery again (a retry, or a restart mid-handling).
        Ok(false) | Err(_) => return,
    }
    // The reply slot is reserved before the prompt, so a turn that finishes
    // within the send still has somewhere to answer.
    st.channels.owe("telegram");
    match deliver(st, "telegram", &text, &source_id).await {
        Ok((session, turn)) => {
            st.channels.owe_turn("telegram", turn.clone());
            let _ = st.db.channel_inbox_finish(&source_id, "submitted", Some(&session), turn.as_deref());
        }
        Err(error) => {
            st.channels.unowe("telegram");
            let _ = st.db.channel_inbox_finish(&source_id, "blocked", None, None);
            let _ = send_to_peer(st, "telegram", &error).await;
        }
    }
}

fn sender_name(from: &Value) -> String {
    let first = from["first_name"].as_str().unwrap_or("");
    let last = from["last_name"].as_str().unwrap_or("");
    let name = format!("{first} {last}").trim().to_owned();
    if name.is_empty() { from["username"].as_str().map(|name| format!("@{name}")).unwrap_or_else(|| "Someone".to_owned()) } else { name.chars().take(120).collect() }
}

/// `/start <code>` and `/start@bot <code>`.
fn start_code(text: &str) -> Option<String> {
    let mut words = text.split_whitespace();
    let command = words.next()?.to_ascii_lowercase();
    if command != "/start" && !command.starts_with("/start@") { return None }
    let code = words.next()?.trim().to_owned();
    (code.len() == 8 && code.chars().all(|ch| ch.is_ascii_alphanumeric())).then_some(code)
}

/// One button press: the token names one card and one answer, once.
async fn answer_callback(st: &AppState, api: &Telegram, callback: &Value) {
    let Some(callback_id) = callback["id"].as_str() else { return };
    let Some(message) = callback.get("message").filter(|value| value.is_object()) else { return };
    if message["chat"]["type"].as_str() != Some("private") { return }
    let from = callback["from"]["id"].as_i64();
    if from.is_none() || from != message["chat"]["id"].as_i64() { return }
    let stored = match st.db.channel_get("telegram") { Ok(Some(row)) => row, _ => return };
    let paired_id = stored["peer"]["id"].as_str().map(str::to_owned).or_else(|| stored["peer"]["id"].as_i64().map(|id| id.to_string()));
    if paired_id.as_deref() != from.map(|id| id.to_string()).as_deref() { return }
    let Some(data) = callback["data"].as_str() else { return };
    let (token, choice) = match data.split_once(':') { Some((token, choice)) => (token, choice), None => return };
    if !matches!(choice, "allow" | "deny") { return }
    let Some(permission_id) = st.channels.take_button(token) else {
        api.answer_callback(callback_id, "That button has expired. Answer it in Retinue.").await;
        return;
    };
    // The same route the desktop uses, so a rule, an audit line and the
    // conversation's state all follow from one place.
    let reply = crate::dispatch(st, "permission.reply", json!({"permissionId": permission_id, "choice": choice})).await;
    match reply {
        Ok(_) => api.answer_callback(callback_id, if choice == "allow" { "Allowed." } else { "Denied." }).await,
        Err((_, error)) => api.answer_callback(callback_id, &format!("Retinue could not use that answer. {}", error.message)).await,
    }
}

// ---------------------------------------------------------------------------
// Inbound to the CEO's conversation

/// Puts one inbound message into the CEO's channel conversation, creating it the
/// first time, and returns the conversation and the turn it started.
async fn deliver(st: &AppState, kind: &str, text: &str, source_id: &str) -> Result<(String, Option<String>), String> {
    let chief = ceo(st).map_err(|(_, error)| error.message)?;
    let ceo_id = chief["id"].as_str().ok_or("The CEO has no id, so the message has nowhere to go.")?.to_owned();
    let stored = st.db.channel_get(kind).map_err(|_| "The channel could not be read.".to_owned())?.ok_or("That channel isn't set up.")?;
    let usable = stored["sessionId"].as_str().map(str::to_owned).filter(|id| {
        st.db.session_exists(id).unwrap_or(false) && st.db.session_brief(id).map(|row| row["archived"] != true).unwrap_or(false)
    });
    let session = match usable {
        Some(id) => id,
        None => {
            let title = match kind { "telegram" => "Telegram", "whatsapp-cloud" | "whatsapp-web" => "WhatsApp", _ => "Channel" };
            let created = team::agent_conversation_create(st, &json!({"agentId": ceo_id, "title": title})).await
                .map_err(|(_, error)| error.message)?;
            let id = created["session"]["id"].as_str().ok_or("The CEO's conversation could not be started.")?.to_owned();
            update_row(st, kind, |row| { row["sessionId"] = json!(id.clone()); }).await.map_err(|(_, error)| error.message)?;
            id
        }
    };
    let client_id = format!("chan{}", hex(&Sha256::digest(source_id.as_bytes()))[..32].to_owned());
    let prompt = crate::session_prompt_for_channel(st, json!({
        "sessionId": session,
        "text": text,
        "clientMessageId": client_id,
        "meta": {"kind": "channel", "channel": kind},
    })).await.map_err(|(_, error)| error.message)?;
    let turn = prompt["turnId"].as_str().map(str::to_owned);
    Ok((session, turn))
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes { out.push_str(&format!("{byte:02x}")); }
    out
}

// ---------------------------------------------------------------------------
// Events: the CEO's reply, and the approval cards

async fn event_loop(st: AppState, kind: String, generation: u64) {
    let mut events = st.events.subscribe();
    loop {
        if !st.channels.alive(&kind, generation) { return }
        let event = match events.recv().await {
            Ok(event) => event,
            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
            Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
        };
        if !st.channels.alive(&kind, generation) { return }
        match event.event_type.as_str() {
            "permission.requested" => card_for_chat(&st, &kind, &event.payload).await,
            "permission.resolved" => {
                if let Some(id) = event.payload["permissionId"].as_str() { st.channels.forget_card(id); }
            }
            "turn.completed" | "turn.error" | "turn.cancelled" => on_turn_end(&st, &kind, &event.event_type, &event.payload).await,
            _ => {}
        }
    }
}

async fn card_for_chat(st: &AppState, kind: &str, card: &Value) {
    let Some(stored) = st.db.channel_get(kind).ok().flatten() else { return };
    if !stored["approvals"].as_bool().unwrap_or(true) { return }
    if stored["peer"].is_null() { return }
    let Some(session) = stored["sessionId"].as_str() else { return };
    if card["sessionId"].as_str() != Some(session) { return }
    let Some(permission_id) = card["id"].as_str() else { return };
    let card_kind = st.db.permission_kind(permission_id).ok().flatten();
    if !chat_may_answer(card_kind.as_deref()) { return }
    let choices = card["choices"].as_array().cloned().unwrap_or_default();
    let has = |name: &str| choices.iter().any(|choice| choice.as_str() == Some(name));
    if !(has("allow") && has("deny")) { return }
    let token = st.channels.button_for(permission_id);
    let title = card["title"].as_str().unwrap_or("The CEO is asking for something");
    let detail = card["detail"].as_str().map(chat_text).unwrap_or_default();
    let detail: String = detail.chars().take(1_500).collect();
    let mut text = if detail.trim().is_empty() { format!("{title}\n\nAllow it?") } else { format!("{title}\n\n{detail}\n\nAllow it?") };
    text.push_str("\n\nAllow or deny here, or open Retinue to read the whole card.");
    let markup = json!({"inline_keyboard": [[
        {"text": "Allow", "callback_data": format!("{token}:allow")},
        {"text": "Deny", "callback_data": format!("{token}:deny")}
    ]]});
    if let Err(error) = telegram_send_markup(st, kind, &text, markup).await {
        // A card that cannot be sent is not an approval: the desktop card stays.
        st.channels.forget_card(permission_id);
        let _ = set_detail(st, kind, "error", &format!("An approval card could not be sent: {error}")).await;
    }
}

/// The exclusions the owner set on 16 November 2026. A proposal is decided with
/// `proposal.decide`, a question asks for words, and the consent cards for the
/// owner's computer, the linked computer and the vault keep asking in the app.
/// Full access and settings never raise a card here at all.
fn chat_may_answer(kind: Option<&str>) -> bool {
    !matches!(kind, Some("proposal") | Some("question") | Some("owner_computer") | Some("computer") | Some("vault") | Some("vault_use"))
}

async fn telegram_send_markup(st: &AppState, kind: &str, text: &str, markup: Value) -> Result<(), String> {
    let stored = st.db.channel_get(kind).map_err(|_| "The channel could not be read.".to_owned())?.ok_or("That channel isn't set up.")?;
    let Some(peer) = stored["peer"].as_object() else { return Err("No sender is paired yet.".to_owned()) };
    let chat_id = peer.get("id").and_then(|id| id.as_str().map(str::to_owned).or_else(|| id.as_i64().map(|id| id.to_string()))).ok_or("The paired sender has no chat to answer in.")?;
    let api = telegram_for(kind)?;
    api.send(&chat_id, text, Some(markup)).await
}

async fn on_turn_end(st: &AppState, kind: &str, event_type: &str, payload: &Value) {
    let Some(stored) = st.db.channel_get(kind).ok().flatten() else { return };
    let Some(session) = stored["sessionId"].as_str() else { return };
    if payload["sessionId"].as_str() != Some(session) { return }
    let turn = payload["turnId"].as_str();
    // Only a turn this channel started is answered here: a reply to something
    // the owner typed in the app stays in the app.
    if !st.channels.settle(kind, turn) { return }
    let text = match event_type {
        "turn.completed" => {
            let reply = match turn {
                Some(turn) => st.db.turn_reply(session, turn).ok().flatten(),
                None => st.db.latest_turn_reply(session).ok().flatten(),
            };
            reply.unwrap_or_else(|| "The CEO finished without a reply.".to_owned())
        }
        "turn.cancelled" => "That turn was stopped, so there is no reply.".to_owned(),
        _ => {
            let message = payload["message"].as_str().map(chat_text).unwrap_or_default();
            if message.is_empty() { "The CEO's turn failed, so there is no reply.".to_owned() } else { format!("The CEO's turn failed: {message}") }
        }
    };
    let _ = send_to_peer(st, kind, &text).await;
}

// ---------------------------------------------------------------------------
// Unit tests for the pure parts

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reply_leaves_local_detail_behind() {
        let text = "Done. The file is at C:\\Users\\vex\\work\\notes.md and the app is on http://127.0.0.1:5173/settings. Token: sk-abcdefghijklmnopqrstuvwxyz0123456789";
        let out = chat_text(text);
        assert!(!out.contains("notes.md"), "{out}");
        assert!(!out.contains("127.0.0.1"), "{out}");
        assert!(!out.contains("sk-abcdefghijklmnopqrstuvwxyz0123456789"), "{out}");
        assert!(out.contains("Done."), "{out}");
    }

    #[test]
    fn a_long_reply_is_split_without_breaking_a_character() {
        let line = "é".repeat(40);
        let body = std::iter::repeat_n(line.clone(), 40).collect::<Vec<_>>().join("\n");
        let parts = chunks(&body, 100);
        assert!(parts.len() > 1);
        assert!(parts.iter().all(|part| part.chars().count() <= 100), "{}", parts.iter().map(|p| p.chars().count()).max().unwrap());
        assert_eq!(parts.concat().replace(['\n', ' '], ""), body.replace(['\n', ' '], ""));
    }

    #[test]
    fn a_start_command_is_the_code_and_nothing_else() {
        assert_eq!(start_code("/start ABCD2345").as_deref(), Some("ABCD2345"));
        assert_eq!(start_code("/start@retinue_bot ABCD2345").as_deref(), Some("ABCD2345"));
        assert_eq!(start_code("/start"), None);
        assert_eq!(start_code("hello /start ABCD2345"), None);
        assert_eq!(start_code("/start ABC"), None);
    }

    #[test]
    fn the_owner_s_exclusions_keep_their_cards_in_the_app() {
        assert!(chat_may_answer(Some("command")));
        assert!(chat_may_answer(None));
        for kind in ["proposal", "question", "owner_computer", "computer", "vault", "vault_use"] {
            assert!(!chat_may_answer(Some(kind)), "{kind}");
        }
    }

    #[test]
    fn a_phone_number_is_shown_masked() {
        assert_eq!(mask_phone("+44 7700 900123"), "+44 •••• 0123");
        assert_eq!(mask_phone("447700900123"), "+44 •••• 0123");
    }

    #[test]
    fn a_pairing_code_is_not_reused_and_has_eight_symbols() {
        let first = { use rand::Rng; let mut rng = rand::rng(); (0..8).map(|_| b"23456789ABCDEFGHJKLMNPQRSTUVWXYZ"[rng.random_range(0..32)] as char).collect::<String>() };
        let second = { use rand::Rng; let mut rng = rand::rng(); (0..8).map(|_| b"23456789ABCDEFGHJKLMNPQRSTUVWXYZ"[rng.random_range(0..32)] as char).collect::<String>() };
        assert_eq!(first.len(), 8);
        assert!(first.chars().all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit()));
        assert_ne!(first, second);
    }
}
