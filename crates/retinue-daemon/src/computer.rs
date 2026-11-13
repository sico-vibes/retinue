//! The owner's linked VPS computer (Harbor), and the tools employees get for it.
//!
//! Harbor manages one persistent Linux desktop in a container and answers an HTTP
//! API under `<address>/harbor/api/computer`, authorised by a bearer token the
//! owner makes in Harbor. This module links one Harbor per install, shows its
//! state, wakes and sleeps it, and serves `vps-computer` MCP tools to new
//! conversations.
//!
//! Where the token lives: Windows Credential Manager, as a generic credential
//! `Retinue/computer/<id>` (see `secrets::store`). The database keeps the address,
//! the switch and that name only (`computer_link`, storage). The token is never in
//! an RPC reply, an event, a log line, a backup, a diagnostics bundle, the coding
//! agent's config on disk, its argv or its environment: the stdio bridge reads a
//! user-only endpoint file that names this daemon, and every Harbor call is made
//! here, with the credential read for that request and dropped after it.
//!
//! Everything here is desktop only: every method is `never` in `tiers.json` and
//! `computer.changed` is `drop`, so nothing in this module is reachable from the
//! remote gateway or from the Retinue MCP.

use crate::{derr, AppState, DispatchError};
use axum::{
    body::Body,
    extract::{
        ws::{Message as WsMessage, WebSocket, WebSocketUpgrade},
        FromRequestParts, Path as AxumPath, State,
    },
    http::{header, HeaderMap, HeaderName, HeaderValue, Request, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use chrono::{SecondsFormat, Utc};
use futures_util::{SinkExt, StreamExt};
use retinue_agent_core::McpServerSpec;
use retinue_storage::ComputerLink;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Mutex as StdMutex,
    time::Duration,
};
use tokio_tungstenite::tungstenite::{client::IntoClientRequest, Message as TgMessage};
use uuid::Uuid;
use zeroize::Zeroizing;

/// The MCP server's name, as the provider sees it.
pub const SERVER_NAME: &str = "vps-computer";
/// Where the Harbor computer API lives under the linked address.
const API: &str = "/harbor/api/computer";
/// How long a status read may take. A slow or unreachable server never blocks a
/// status call for longer than this.
const STATUS_TIMEOUT: Duration = Duration::from_secs(8);
/// How long an action, a routine or a log read may take.
const ACTION_TIMEOUT: Duration = Duration::from_secs(20);
/// Extra time beyond `exec`'s own limit, so Harbor answers first.
const EXEC_MARGIN: Duration = Duration::from_secs(15);
/// Harbor's own ceiling for one `exec`.
const EXEC_MAX_MS: u64 = 300_000;
/// The most one tool result may carry back to the agent.
const OUTPUT_MAX: usize = 64 * 1024;
/// A routine's last output is cut to this many characters in `computer.routines`.
const ROUTINE_OUTPUT_MAX: usize = 2_000;
const LOGS_MIN: u64 = 20;
const LOGS_MAX: u64 = 1_000;
const LOGS_DEFAULT: u64 = 200;
/// The endpoint folders and files the bridge reads, inside the data folder.
const ENDPOINT_FOLDER: &str = "computer";
/// Every credential this module names starts with this, so it never matches a
/// secret the owner can see or edit in Settings (those names carry no `/`).
const TOKEN_PREFIX: &str = "computer/";
const ADDRESS_MAX: usize = 2_048;
const TOKEN_MAX: usize = 2_048;
const ROUTINE_ID_MAX: usize = 200;
/// Harbor's owner-only live view, under the linked address.
pub const VIEW_PATH: &str = "/harbor/computer/view";
/// The prefix this daemon serves the framed screen on.
pub const VIEW_ROUTE: &str = "/computer/view";
/// A view ticket is 64 hex characters: 256 random bits.
const VIEW_TICKET_LEN: usize = 64;
/// How long a view ticket lives after its last use.
const VIEW_TTL: Duration = Duration::from_secs(600);
/// The most view tickets kept at once; the oldest goes first.
const VIEW_TICKETS_MAX: usize = 16;
/// How long one proxied request or the token probe may take.
const VIEW_TIMEOUT: Duration = Duration::from_secs(30);
/// The app's own origins, in the packaged window and in the Vite dev server.
/// Only these may frame the proxy's answers.
const APP_ORIGINS: &str = "http://tauri.localhost https://tauri.localhost tauri://localhost http://127.0.0.1:1420";
/// Headers an answer from Harbor must never carry through the proxy: they would
/// either break the frame, set a cookie the window must not hold, or contradict
/// the buffered body's own length.
const STRIPPED_VIEW_HEADERS: [&str; 8] = ["set-cookie", "set-cookie2", "content-security-policy", "content-security-policy-report-only", "x-frame-options", "transfer-encoding", "connection", "content-length"];

const NO_VIEW: &str = "Your server does not offer its screen to Retinue yet. Update Harbor, or open it in your browser.";

const NOT_LINKED: &str = "No computer is linked in Retinue, so there is no computer to use. Ask the owner to link one in Settings.";
const SWITCHED_OFF: &str = "The computer is switched off in Retinue, so this conversation has no computer tools. Ask the owner to switch it on.";
const UNATTENDED: &str = "This conversation runs unattended, so it cannot run commands, create routines or start routines on the computer.";

/// The tools that change the computer, refused while nobody is watching.
const COMMAND_TOOLS: [&str; 3] = ["computer_exec", "computer_create_routine", "computer_run_routine"];

/// What the daemon remembers between reads: the last status it announced, so an
/// unchanged read does not produce an event, and the live tickets for the
/// computer's screen.
#[derive(Default)]
pub struct Computer {
    last: StdMutex<Option<Value>>,
    /// A frame cannot send the daemon's capability, so the screen is served
    /// under single-purpose tickets instead: random, short-lived, never logged.
    view: StdMutex<Vec<ViewTicket>>,
}

/// One live view ticket, and when it was last used.
struct ViewTicket {
    value: String,
    last_used: std::time::Instant,
}

impl Computer {
    /// Drops tickets nobody has used within the lifetime.
    fn prune_view(tickets: &mut Vec<ViewTicket>) {
        tickets.retain(|ticket| ticket.last_used.elapsed() < VIEW_TTL);
    }

    /// A new screen ticket: 256 random bits, hex.
    fn mint_view_ticket(&self) -> String {
        let mut tickets = self.view.lock().unwrap_or_else(|poison| poison.into_inner());
        Self::prune_view(&mut tickets);
        while tickets.len() >= VIEW_TICKETS_MAX {
            // The least recently used goes first: a frame that is being watched
            // keeps its ticket for as long as its own requests arrive.
            let Some(oldest) = tickets.iter().enumerate().min_by_key(|(_, ticket)| ticket.last_used).map(|(index, _)| index) else { break };
            tickets.remove(oldest);
        }
        let value = retinue_security::new_capability();
        tickets.push(ViewTicket { value: value.clone(), last_used: std::time::Instant::now() });
        value
    }

    /// Whether `supplied` is one of the live tickets. Every live ticket is
    /// compared, in constant time, so the answer does not depend on which one
    /// matched, and a match keeps its ticket alive for another lifetime.
    fn accepts_view_ticket(&self, supplied: &str) -> bool {
        if supplied.len() != VIEW_TICKET_LEN || !supplied.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return false;
        }
        let mut tickets = self.view.lock().unwrap_or_else(|poison| poison.into_inner());
        Self::prune_view(&mut tickets);
        let mut accepted = false;
        for ticket in tickets.iter_mut() {
            let matched: bool = retinue_security::capability_matches(&ticket.value, supplied);
            accepted |= matched;
            if matched {
                ticket.last_used = std::time::Instant::now();
            }
        }
        accepted
    }

    fn clear_view_tickets(&self) {
        if let Ok(mut tickets) = self.view.lock() {
            tickets.clear();
        }
    }
}

fn now_iso() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn invalid(message: &str) -> DispatchError {
    derr("invalid_argument", message, StatusCode::BAD_REQUEST)
}

fn internal(message: &str) -> DispatchError {
    derr("internal", message, StatusCode::INTERNAL_SERVER_ERROR)
}

fn unavailable(message: &str, status: StatusCode) -> DispatchError {
    derr("unavailable", message, status)
}

fn conflict(message: &str) -> DispatchError {
    derr("conflict", message, StatusCode::CONFLICT)
}

fn data_dir(st: &AppState) -> Option<PathBuf> {
    st.team.data_dir.lock().ok().and_then(|dir| dir.clone())
}

// ---------------------------------------------------------------------------
// Addressing a Harbor

/// Why one Harbor call failed. Every variant is worded for the owner rather
/// than carrying a response body, so no error text can hold the token.
#[derive(Debug)]
enum HarborError {
    /// 401: the token is wrong or revoked.
    Unauthorized,
    /// No answer, a timeout, or a connection that never completed.
    Unreachable,
    /// Anything else, already a plain sentence.
    Other(String),
}

/// One link's address and token, for the length of one request.
struct Harbor {
    address: String,
    token: Zeroizing<String>,
}

/// A sentence built from Harbor's own message: one line, trimmed, capped.
fn sentence(raw: &str) -> String {
    let line = raw.lines().map(str::trim).find(|line| !line.is_empty()).unwrap_or("");
    let mut text = line.chars().filter(|ch| !ch.is_control()).take(200).collect::<String>();
    if text.is_empty() {
        return "Harbor refused the request.".into();
    }
    if !text.ends_with(['.', '!', '?']) {
        text.push('.');
    }
    text
}

impl Harbor {
    /// The one HTTP client every call uses: the owner's own server, contacted
    /// directly. No redirect is followed (it would carry the bearer token to
    /// another origin), no proxy is consulted, and the User-Agent is Retinue's.
    fn client() -> Result<reqwest::Client, HarborError> {
        reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .connect_timeout(STATUS_TIMEOUT)
            .user_agent(concat!("Retinue/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|_| HarborError::Other("The request to Harbor could not be prepared.".into()))
    }

    async fn call(&self, method: reqwest::Method, path: &str, body: Option<Value>, limit: Duration) -> Result<Value, HarborError> {
        let client = Self::client()?;
        let mut request = client
            .request(method, format!("{}{API}{path}", self.address))
            .header(reqwest::header::AUTHORIZATION, format!("Bearer {}", self.token.as_str()))
            .header("X-Harbor-Client", "mcp")
            .header(reqwest::header::ACCEPT, "application/json")
            .timeout(limit);
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request.send().await.map_err(|_| HarborError::Unreachable)?;
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(HarborError::Unauthorized);
        }
        if !status.is_success() {
            let message = serde_json::from_str::<Value>(&text).ok().and_then(|value| value["error"]["message"].as_str().map(sentence));
            return Err(HarborError::Other(message.unwrap_or_else(|| format!("Harbor answered with HTTP {}.", status.as_u16()))));
        }
        // Harbor answers JSON; anything else is kept as text rather than thrown away.
        Ok(serde_json::from_str(&text).unwrap_or(Value::String(text)))
    }
}

/// Reads a link's token. The error never names or shows the value.
fn token_of(link: &ComputerLink) -> Result<Zeroizing<String>, DispatchError> {
    match crate::secrets::store::read(&link.token_ref) {
        Ok(Some(token)) => Ok(token),
        Ok(None) => Err(conflict("The computer's token is missing from the credential store, so the computer can't be reached. Link it again in Settings.")),
        Err(_) => Err(unavailable("The credential store is unavailable, so the computer can't be reached.", StatusCode::SERVICE_UNAVAILABLE)),
    }
}

fn linked(st: &AppState) -> Result<ComputerLink, DispatchError> {
    st.db.computer_link().map_err(|_| internal("The computer's link could not be read."))?.ok_or_else(|| conflict("No computer is linked. Link one first."))
}

fn harbor_for(link: &ComputerLink) -> Result<Harbor, DispatchError> {
    Ok(Harbor { address: link.address.clone(), token: token_of(link)? })
}

/// Whether employees get the computer's tools: linked and switched on.
fn enabled_link(st: &AppState) -> Option<ComputerLink> {
    st.db.computer_link().ok().flatten().filter(|link| link.enabled)
}

// ---------------------------------------------------------------------------
// The address

fn ipv4_in_tailnet(octets: [u8; 4]) -> bool {
    // 100.64.0.0/10, the range Tailscale hands out.
    octets[0] == 100 && (64..=127).contains(&octets[1])
}

fn host_is_loopback_or_tailnet(host: &str) -> bool {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    if host == "localhost" || host.ends_with(".ts.net") {
        return true;
    }
    match host.parse::<std::net::IpAddr>() {
        // The owner's own machine, and the 100.64.0.0/10 range Tailscale hands out.
        Ok(std::net::IpAddr::V4(v4)) => v4.is_loopback() || ipv4_in_tailnet(v4.octets()),
        Ok(std::net::IpAddr::V6(v6)) => v6.is_loopback(),
        Err(_) => false,
    }
}

/// Validates a Harbor address and returns it as `scheme://host[:port]`.
///
/// HTTPS always; plain HTTP only for loopback and tailnet addresses, where the
/// owner's own machine or their own tailnet is the only place the token can go.
/// No path, query, fragment or credentials: the API's own path is appended.
pub(crate) fn normalize_address(raw: &str) -> Result<String, DispatchError> {
    let text = raw.trim();
    if text.is_empty() {
        return Err(invalid("Enter the address of your Harbor server."));
    }
    if text.len() > ADDRESS_MAX || text.contains('\0') || text.chars().any(|ch| ch.is_control() || ch.is_whitespace()) {
        return Err(invalid("That address is not a web address Retinue can use."));
    }
    let url = reqwest::Url::parse(text).map_err(|_| invalid("That address is not a web address Retinue can use, with http:// or https:// in front."))?;
    let host = url.host_str().ok_or_else(|| invalid("That address names no server."))?;
    match url.scheme() {
        "https" => {}
        "http" if host_is_loopback_or_tailnet(host) => {}
        "http" => return Err(invalid("Use https:// for that address. Plain http:// is only allowed for this machine and for a Tailscale address.")),
        _ => return Err(invalid("Harbor is reached over https://, so that address has to start with https://.")),
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(invalid("Leave the user name and password out of the address; the token authorises Retinue."));
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err(invalid("That address carries a query or a fragment. Enter only the server, like https://harbor.example.com."));
    }
    if !matches!(url.path(), "" | "/") {
        return Err(invalid("That address carries a path. Enter only the server, like https://harbor.example.com."));
    }
    Ok(url.origin().ascii_serialization())
}

/// Whether a desktop URL sits on the linked address's own origin. Anything else
/// is dropped: it would send the owner to a host Harbor does not control.
fn same_origin(address: &str, url: &str) -> bool {
    match (reqwest::Url::parse(address), reqwest::Url::parse(url)) {
        (Ok(address), Ok(url)) => address.scheme() == url.scheme() && address.host_str() == url.host_str() && address.port_or_known_default() == url.port_or_known_default(),
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// The status object

fn percent(value: &Value) -> Option<f64> {
    if let Some(number) = value.as_f64() {
        return Some(number);
    }
    value.as_str()?.trim().trim_end_matches('%').trim().parse::<f64>().ok()
}

fn unlinked_status() -> Value {
    json!({
        "linked": false,
        "enabled": false,
        "address": null,
        "name": null,
        "reachable": false,
        "state": "unknown",
        "since": null,
        "restarts": 0,
        "cpuPercent": null,
        "memoryPercent": null,
        "memoryText": null,
        "pids": null,
        "diskBytes": null,
        "limits": null,
        "desktopUrl": null,
        "checkedAt": null,
        "problem": null,
    })
}

/// Harbor's own answer, as the status object. Docker's `status` decides the
/// state: running is awake; exited, created and paused are asleep; missing is
/// missing; anything else is unknown.
pub(crate) fn map_status(address: &str, enabled: bool, body: &Value) -> Value {
    let computer = &body["computer"];
    let stats = &computer["stats"];
    let state = match computer["status"].as_str().unwrap_or("") {
        "running" => "awake",
        "exited" | "created" | "paused" => "asleep",
        "missing" => "missing",
        _ => "unknown",
    };
    let limits = body["limits"].as_object().map(|limits| {
        json!({
            "cpu": limits.get("cpu").and_then(Value::as_str),
            "memory": limits.get("memory").and_then(Value::as_str),
        })
    });
    json!({
        "linked": true,
        "enabled": enabled,
        "address": address,
        "name": computer["name"].as_str(),
        "reachable": true,
        "state": state,
        "since": computer["since"].as_str(),
        "restarts": computer["restarts"].as_u64().unwrap_or(0),
        "cpuPercent": percent(&stats["cpu"]),
        "memoryPercent": percent(&stats["memoryPercent"]),
        "memoryText": stats["memory"].as_str(),
        "pids": stats["pids"].as_u64().or_else(|| stats["pids"].as_str().and_then(|text| text.trim().parse().ok())),
        "diskBytes": computer["disk_bytes"].as_u64(),
        "limits": limits,
        "desktopUrl": body["desktop_url"].as_str().filter(|url| same_origin(address, url)),
        "checkedAt": now_iso(),
        "problem": Value::Null,
    })
}

/// A status object for a server that did not answer. The last name it gave is
/// kept, so the owner still sees which computer is unreachable.
fn offline_status(link: &ComputerLink, name: Option<&str>, problem: String) -> Value {
    json!({
        "linked": true,
        "enabled": link.enabled,
        "address": link.address,
        "name": name,
        "reachable": false,
        "state": "unknown",
        "since": null,
        "restarts": 0,
        "cpuPercent": null,
        "memoryPercent": null,
        "memoryText": null,
        "pids": null,
        "diskBytes": null,
        "limits": null,
        "desktopUrl": null,
        "checkedAt": now_iso(),
        "problem": problem,
    })
}

/// The plain sentence an unreachable or refused read reports.
fn problem_of(error: HarborError) -> String {
    match error {
        HarborError::Unauthorized => "Harbor refused the token. The owner needs to link the computer again.".into(),
        HarborError::Unreachable => "The computer's server did not answer. It may be off, or the address may have changed.".into(),
        HarborError::Other(message) => message,
    }
}

fn last_name(st: &AppState) -> Option<String> {
    st.computer.last.lock().ok().and_then(|last| last.as_ref().and_then(|last| last["name"].as_str().map(str::to_owned)))
}

/// One status read. Never fails: an unreachable server is `reachable: false`
/// with a plain `problem`.
async fn read_status(st: &AppState) -> Value {
    let Some(link) = st.db.computer_link().ok().flatten() else { return unlinked_status() };
    let name = last_name(st);
    let harbor = match token_of(&link) {
        Ok(token) => Harbor { address: link.address.clone(), token },
        // A missing or unreadable credential is a plain problem, not a failure.
        Err(error) => return offline_status(&link, name.as_deref(), error.1.message),
    };
    let address = link.address.clone();
    let enabled = link.enabled;
    match harbor.call(reqwest::Method::GET, "", None, STATUS_TIMEOUT).await {
        Ok(body) if body["computer"].is_object() => map_status(&address, enabled, &body),
        Ok(_) => offline_status(&link, name.as_deref(), "That address answered, but not like Harbor.".into()),
        Err(error) => offline_status(&link, name.as_deref(), problem_of(error)),
    }
}

/// Two status objects that differ only in `checkedAt` describe the same state.
fn same_state(a: &Value, b: &Value) -> bool {
    fn without_checked_at(value: &Value) -> Value {
        let mut value = value.clone();
        if let Some(object) = value.as_object_mut() {
            object.remove("checkedAt");
        }
        value
    }
    without_checked_at(a) == without_checked_at(b)
}

/// Announces a status: after every write, and on a read only when it differs
/// from the last one announced. `checkedAt` moves on every read, so it is not a
/// change by itself.
async fn publish(st: &AppState, status: Value, force: bool) {
    let announced = {
        let mut last = st.computer.last.lock().unwrap_or_else(|poison| poison.into_inner());
        let changed = force || !last.as_ref().is_some_and(|previous| same_state(previous, &status));
        if changed {
            *last = Some(status.clone());
        }
        changed
    };
    if announced {
        st.emit("computer.changed", status).await;
    }
}

// ---------------------------------------------------------------------------
// RPC

pub async fn rpc(st: &AppState, method: &str, p: &Value) -> Result<Value, DispatchError> {
    match method {
        "computer.status" => {
            let status = read_status(st).await;
            publish(st, status.clone(), false).await;
            Ok(status)
        }
        "computer.link" => link(st, p).await,
        "computer.unlink" => unlink(st).await,
        "computer.setEnabled" => set_enabled(st, p).await,
        "computer.action" => action(st, p).await,
        "computer.routines" => Ok(json!({"routines": routines(st).await?})),
        "computer.routine.run" => run_routine(st, p).await,
        "computer.logs" => logs(st, p).await,
        "computer.view" => view(st).await,
        _ => Err(derr("not_found", "unknown computer method", StatusCode::NOT_FOUND)),
    }
}

/// `computer.link {address, token, confirm}`. Nothing is stored unless Harbor
/// answers 200: a wrong token or a wrong address leaves any existing link as it
/// was.
async fn link(st: &AppState, p: &Value) -> Result<Value, DispatchError> {
    if p["confirm"] != true {
        return Err(invalid("confirm: true is required"));
    }
    let address = normalize_address(p["address"].as_str().unwrap_or(""))?;
    let token = p["token"]
        .as_str()
        .filter(|token| (1..=TOKEN_MAX).contains(&token.len()) && token.bytes().all(|byte| (0x21..=0x7e).contains(&byte)))
        .ok_or_else(|| invalid("The token must be 1 to 2,048 printable characters."))?;
    let harbor = Harbor { address: address.clone(), token: Zeroizing::new(token.to_owned()) };
    let body = harbor.call(reqwest::Method::GET, "", None, STATUS_TIMEOUT).await.map_err(|error| match error {
        HarborError::Unauthorized => invalid("Harbor refused that token."),
        HarborError::Unreachable => unavailable("That address did not answer. Check the address and that Harbor is running.", StatusCode::BAD_GATEWAY),
        HarborError::Other(message) => unavailable(&message, StatusCode::BAD_GATEWAY),
    })?;
    if !body["computer"].is_object() {
        return Err(invalid("That address answered, but not like Harbor."));
    }
    let previous = st.db.computer_link().ok().flatten();
    let token_ref = format!("{TOKEN_PREFIX}{}", Uuid::new_v4());
    crate::secrets::store::write(&token_ref, token).map_err(|_| unavailable("The credential store could not save the token, so the computer was not linked.", StatusCode::SERVICE_UNAVAILABLE))?;
    if st.db.computer_link_set(&address, &token_ref).is_err() {
        let _ = crate::secrets::store::delete(&token_ref);
        return Err(internal("The computer's link could not be saved."));
    }
    if let Some(previous) = previous {
        let _ = crate::secrets::store::delete(&previous.token_ref);
    }
    let status = map_status(&address, false, &body);
    // A new link is a new token: the old link's tickets die with it.
    st.computer.clear_view_tickets();
    publish(st, status.clone(), true).await;
    Ok(status)
}

/// `computer.unlink {}`. The address, the switch and the token all go.
async fn unlink(st: &AppState) -> Result<Value, DispatchError> {
    let previous = st.db.computer_link().ok().flatten();
    st.db.computer_link_clear().map_err(|_| internal("The computer's link could not be removed."))?;
    st.computer.clear_view_tickets();
    if let Some(previous) = previous {
        let _ = crate::secrets::store::delete(&previous.token_ref);
    }
    if let Some(dir) = data_dir(st) {
        sweep_endpoints(&dir);
    }
    let status = unlinked_status();
    publish(st, status.clone(), true).await;
    Ok(status)
}

/// `computer.setEnabled {enabled, confirm}`. Off after linking; turning it on
/// gives every new conversation the tools, so it takes `confirm: true`.
async fn set_enabled(st: &AppState, p: &Value) -> Result<Value, DispatchError> {
    let enabled = p["enabled"].as_bool().ok_or_else(|| invalid("enabled must be true or false"))?;
    linked(st)?;
    if enabled && p["confirm"] != true {
        return Err(invalid("confirm: true is required"));
    }
    st.db.computer_link_set_enabled(enabled).map_err(|_| internal("The computer's switch could not be saved."))?;
    let status = read_status(st).await;
    publish(st, status.clone(), true).await;
    Ok(status)
}

/// `computer.action {action}`. `sleep` is Harbor's `stop`: the container stops
/// and its `/config` volume stays.
async fn action(st: &AppState, p: &Value) -> Result<Value, DispatchError> {
    let path = match p["action"].as_str().unwrap_or("") {
        "wake" => "/action/wake",
        "sleep" => "/action/stop",
        "restart" => "/action/restart",
        _ => return Err(invalid("action must be wake, sleep or restart")),
    };
    let link = linked(st)?;
    harbor_for(&link)?
        .call(reqwest::Method::POST, path, Some(json!({})), ACTION_TIMEOUT)
        .await
        .map_err(|error| unavailable(&problem_of(error), StatusCode::BAD_GATEWAY))?;
    // Harbor's own action reply carries no limits or desktop URL, so the status
    // comes from a fresh read. A wake can still be settling when this runs.
    let status = read_status(st).await;
    publish(st, status.clone(), true).await;
    Ok(status)
}

fn text_of(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Number(number) => number.to_string(),
        _ => String::new(),
    }
}

fn cut_chars(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let mut cut = text.chars().take(max.saturating_sub(1)).collect::<String>();
    cut.push('…');
    cut
}

/// Harbor's `lastResult` in whatever shape it arrives: an object with `ok` and
/// an output field, a bare string, or nothing.
fn routine_result(last: &Value) -> (Option<bool>, Option<String>) {
    if let Some(object) = last.as_object() {
        let ok = object.get("ok").and_then(Value::as_bool).or_else(|| object.get("success").and_then(Value::as_bool));
        let output = ["output", "log", "text", "message"].iter().find_map(|key| object.get(*key).and_then(Value::as_str)).map(str::to_owned);
        return (ok, output);
    }
    if let Some(text) = last.as_str().filter(|text| !text.trim().is_empty()) {
        return (None, Some(text.to_owned()));
    }
    (None, None)
}

/// Accepts both `{routines: [...]}` and a bare array.
pub(crate) fn parse_routines(body: &Value) -> Vec<Value> {
    let rows = body["routines"].as_array().or_else(|| body.as_array()).cloned().unwrap_or_default();
    rows.iter()
        .map(|row| {
            let (ok, output) = routine_result(&row["lastResult"]);
            json!({
                "id": text_of(&row["id"]),
                "name": row["name"].as_str().unwrap_or(""),
                "cron": row["cron"].as_str().unwrap_or(""),
                "command": row["command"].as_str().unwrap_or(""),
                "enabled": row["enabled"].as_bool().unwrap_or(false),
                "lastRunAt": row["lastRunAt"].as_str(),
                "lastOk": ok,
                "lastOutput": output.map(|text| cut_chars(&text, ROUTINE_OUTPUT_MAX)),
            })
        })
        .collect()
}

/// A routine's id as one safe path segment.
fn path_segment(id: &str) -> String {
    id.bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (byte as char).to_string(),
            _ => format!("%{byte:02X}"),
        })
        .collect()
}

async fn routines(st: &AppState) -> Result<Vec<Value>, DispatchError> {
    let link = linked(st)?;
    let body = harbor_for(&link)?
        .call(reqwest::Method::GET, "/routines", None, ACTION_TIMEOUT)
        .await
        .map_err(|error| unavailable(&problem_of(error), StatusCode::BAD_GATEWAY))?;
    Ok(parse_routines(&body))
}

/// `computer.routine.run {id}`: start one routine now. The computer has to be
/// awake for it to run; Harbor answers either way.
async fn run_routine(st: &AppState, p: &Value) -> Result<Value, DispatchError> {
    let id = p["id"].as_str().filter(|id| (1..=ROUTINE_ID_MAX).contains(&id.len())).ok_or_else(|| invalid("id is required"))?;
    let link = linked(st)?;
    let body = harbor_for(&link)?
        .call(reqwest::Method::POST, &format!("/routines/{}/run", path_segment(id)), Some(json!({})), ACTION_TIMEOUT)
        .await
        .map_err(|error| unavailable(&problem_of(error), StatusCode::BAD_GATEWAY))?;
    let ok = body["ok"]
        .as_bool()
        .or_else(|| body["success"].as_bool())
        .or_else(|| body["code"].as_i64().map(|code| code == 0))
        .unwrap_or(true);
    let output = ["output", "log", "text"].iter().find_map(|key| body.get(*key).and_then(Value::as_str)).map(str::to_owned);
    Ok(json!({"ok": ok, "output": output}))
}

/// `computer.logs {tail}`: the container's log tail. `tail` is clamped to 20 to
/// 1,000 lines, 200 by default, exactly as the bridge does.
async fn logs(st: &AppState, p: &Value) -> Result<Value, DispatchError> {
    let tail = match p.get("tail") {
        None | Some(Value::Null) => LOGS_DEFAULT,
        Some(value) => value.as_u64().ok_or_else(|| invalid("tail must be a number of lines"))?.clamp(LOGS_MIN, LOGS_MAX),
    };
    let link = linked(st)?;
    let body = harbor_for(&link)?
        .call(reqwest::Method::GET, &format!("/logs?tail={tail}"), None, ACTION_TIMEOUT)
        .await
        .map_err(|error| unavailable(&problem_of(error), StatusCode::BAD_GATEWAY))?;
    let text = match &body {
        Value::String(text) => text.clone(),
        other => other["logs"].as_str().unwrap_or("").to_owned(),
    };
    Ok(json!({"logs": text}))
}

/// `computer.view {}`: a loopback address the window may frame, carrying a
/// ticket the proxy accepts, or `url: null` and a plain reason.
///
/// The token is checked once against the server's view path first: today Harbor
/// only offers its screen to the owner's browser sign-in, so an update can add
/// this later without the window ever holding the token.
async fn view(st: &AppState) -> Result<Value, DispatchError> {
    let no = |reason: String| json!({"url": Value::Null, "reason": reason});
    let Some(link) = st.db.computer_link().ok().flatten() else {
        return Ok(no("No computer is linked in Retinue. Link one in Settings.".into()));
    };
    let status = read_status(st).await;
    if status["state"] != "awake" {
        let reason = match status["state"].as_str().unwrap_or("") {
            "asleep" => "The computer is asleep. Wake it first.".to_owned(),
            "missing" => "The computer is missing on its server.".to_owned(),
            _ => status["problem"].as_str().map(str::to_owned).unwrap_or_else(|| "The computer is not awake.".into()),
        };
        return Ok(no(reason));
    }
    let token = match token_of(&link) {
        Ok(token) => token,
        Err(error) => return Ok(no(error.1.message)),
    };
    match probe_view(&link, &token).await {
        Ok(code) if code.is_success() => {}
        // A sign-in page or a plain refusal: Harbor does not offer the screen to
        // Retinue yet, and the window falls back to the owner's browser.
        Ok(code) if code.is_redirection() || code == StatusCode::UNAUTHORIZED || code == StatusCode::FORBIDDEN => return Ok(no(NO_VIEW.into())),
        Ok(code) => return Ok(no(format!("Harbor answered HTTP {} on its view path.", code.as_u16()))),
        Err(error) => return Ok(no(problem_of(error))),
    }
    let Some(address) = st.team.address.lock().ok().and_then(|address| address.clone()) else {
        return Ok(no("Retinue's own address is not ready yet. Try again in a moment.".into()));
    };
    let ticket = st.computer.mint_view_ticket();
    Ok(json!({"url": format!("http://{address}{VIEW_ROUTE}/{ticket}/"), "reason": Value::Null}))
}

/// One check that the server accepts the token on its view path: the status
/// code only, never the body. Redirects are not followed.
async fn probe_view(link: &ComputerLink, token: &str) -> Result<StatusCode, HarborError> {
    let client = Harbor::client()?;
    let response = client
        .get(format!("{}{VIEW_PATH}/", link.address))
        .header(reqwest::header::AUTHORIZATION, format!("Bearer {token}"))
        .header("X-Harbor-Client", "mcp")
        .header(reqwest::header::ACCEPT, "text/html,application/xhtml+xml")
        .timeout(STATUS_TIMEOUT)
        .send()
        .await
        .map_err(|_| HarborError::Unreachable)?;
    Ok(StatusCode::from_u16(response.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY))
}

// ---------------------------------------------------------------------------
// The bridge: MCP tools over stdio, forwarded here

/// The ten tools, their names, descriptions and schemas, exactly as the VPS
/// computer proposal's Node bridge (`tools/vps-computer-mcp/`, commit `2182a94`)
/// defines them. That folder is not in this branch; this module is its Retinue
/// side.
pub fn tool_schemas() -> Vec<Value> {
    vec![
        json!({"name":"computer_status","description":"Read the bot computer state, CPU/memory usage, port and persistent disk usage.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}}),
        json!({"name":"computer_wake","description":"Wake (start) the 24/7 bot computer. Use before work that needs the desktop, browser or a routine.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}}),
        json!({"name":"computer_sleep","description":"Stop the bot computer while keeping its persistent /config volume.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}}),
        json!({"name":"computer_restart","description":"Restart the bot computer without deleting its persistent /config volume.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}}),
        json!({"name":"computer_logs","description":"Read the tail of the computer container log.","inputSchema":{"type":"object","properties":{"tail":{"type":"number","description":"Lines from the end, default 200, max 1000."}},"additionalProperties":false}}),
        json!({"name":"computer_exec","description":"Run a shell command inside the bot computer as the desktop user, in /config. Use this to install CLIs, run scripts, or drive automation. The command runs inside the container, never on the VPS host.","inputSchema":{"type":"object","properties":{"command":{"type":"string","description":"Shell command to run inside the computer."},"timeout_ms":{"type":"number","description":"Timeout in milliseconds, max 300000."}},"required":["command"],"additionalProperties":false}}),
        json!({"name":"computer_list_routines","description":"List schedules stored on the computer.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}}),
        json!({"name":"computer_create_routine","description":"Create a cron routine that runs a shell command inside the computer. Routines run only while the computer is awake.","inputSchema":{"type":"object","properties":{"name":{"type":"string"},"cron":{"type":"string","description":"Five-field cron in Europe/London, e.g. 0 8 * * *."},"command":{"type":"string"}},"required":["name","cron","command"],"additionalProperties":false}}),
        json!({"name":"computer_run_routine","description":"Run an existing routine immediately.","inputSchema":{"type":"object","properties":{"id":{"type":"string"}},"required":["id"],"additionalProperties":false}}),
        json!({"name":"computer_desktop_url","description":"Return the owner-only live desktop URL. Opening it requires a Harbor owner login.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}}),
    ]
}

/// Nothing but the last question answer can carry more than this back.
pub(crate) fn cap_output(text: String) -> String {
    if text.len() <= OUTPUT_MAX {
        return text;
    }
    let mut cut = String::with_capacity(OUTPUT_MAX + 40);
    for ch in text.chars() {
        if cut.len() + ch.len_utf8() > OUTPUT_MAX {
            break;
        }
        cut.push(ch);
    }
    cut.push_str("\n… (output truncated at 64 KB)");
    cut
}

fn as_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => serde_json::to_string_pretty(other).unwrap_or_default(),
    }
}

/// Whether nobody is watching this conversation: an automation, an unattended
/// task, or a delegated child of one. An unreadable session counts as nobody.
fn nobody_is_watching(st: &AppState, session_id: &str) -> bool {
    match st.db.session_detail(session_id) {
        Ok(session) => crate::questions::may_ask(st, &session).is_err(),
        Err(_) => true,
    }
}

/// One tool call, with the gate every call passes: the computer has to be
/// linked and switched on when the call arrives, not when the conversation
/// started.
async fn call_tool(st: &AppState, session_id: &str, name: &str, args: &Value) -> Result<String, String> {
    let Some(link) = st.db.computer_link().ok().flatten() else { return Err(NOT_LINKED.into()) };
    if !link.enabled {
        return Err(SWITCHED_OFF.into());
    }
    if !tool_schemas().iter().any(|tool| tool["name"].as_str() == Some(name)) {
        return Err(format!("Unknown tool {name}."));
    }
    if COMMAND_TOOLS.contains(&name) && nobody_is_watching(st, session_id) {
        return Err(UNATTENDED.into());
    }
    let harbor = Harbor { address: link.address.clone(), token: token_of(&link).map_err(|error| error.1.message)? };
    let failure = |error: HarborError| problem_of(error);
    match name {
        "computer_status" => harbor.call(reqwest::Method::GET, "", None, STATUS_TIMEOUT).await.map(|body| as_text(&body)).map_err(failure),
        "computer_wake" => harbor.call(reqwest::Method::POST, "/action/wake", Some(json!({})), ACTION_TIMEOUT).await.map(|body| as_text(&body)).map_err(failure),
        "computer_sleep" => harbor.call(reqwest::Method::POST, "/action/stop", Some(json!({})), ACTION_TIMEOUT).await.map(|body| as_text(&body)).map_err(failure),
        "computer_restart" => harbor.call(reqwest::Method::POST, "/action/restart", Some(json!({})), ACTION_TIMEOUT).await.map(|body| as_text(&body)).map_err(failure),
        "computer_logs" => {
            let tail = args["tail"].as_u64().unwrap_or(LOGS_DEFAULT).clamp(LOGS_MIN, LOGS_MAX);
            harbor.call(reqwest::Method::GET, &format!("/logs?tail={tail}"), None, ACTION_TIMEOUT).await.map(|body| as_text(&body)).map_err(failure)
        }
        "computer_exec" => {
            let command = args["command"].as_str().map(str::trim).filter(|command| !command.is_empty()).ok_or("command is required.")?;
            if command.chars().count() > 20_000 {
                return Err("command must be at most 20,000 characters.".into());
            }
            let timeout_ms = args["timeout_ms"].as_u64().map(|ms| ms.clamp(1, EXEC_MAX_MS));
            let limit = Duration::from_millis(timeout_ms.unwrap_or(EXEC_MAX_MS)) + EXEC_MARGIN;
            let mut body = json!({"command": command});
            if let Some(ms) = timeout_ms {
                body["timeout_ms"] = json!(ms);
            }
            harbor.call(reqwest::Method::POST, "/exec", Some(body), limit).await.map(|body| as_text(&body)).map_err(failure)
        }
        "computer_list_routines" => harbor.call(reqwest::Method::GET, "/routines", None, ACTION_TIMEOUT).await.map(|body| as_text(&body)).map_err(failure),
        "computer_create_routine" => {
            let routine_name = args["name"].as_str().map(str::trim).filter(|name| !name.is_empty() && name.chars().count() <= 80).ok_or("name must be 1 to 80 characters.")?;
            let cron = args["cron"].as_str().map(str::trim).filter(|cron| !cron.is_empty() && cron.chars().count() <= 120).ok_or("cron must be a five-field schedule.")?;
            let command = args["command"].as_str().map(str::trim).filter(|command| !command.is_empty() && command.chars().count() <= 20_000).ok_or("command must be 1 to 20,000 characters.")?;
            harbor
                .call(reqwest::Method::POST, "/routines", Some(json!({"name": routine_name, "cron": cron, "command": command})), ACTION_TIMEOUT)
                .await
                .map(|body| as_text(&body))
                .map_err(failure)
        }
        "computer_run_routine" => {
            let id = args["id"].as_str().filter(|id| (1..=ROUTINE_ID_MAX).contains(&id.len())).ok_or("id is required.")?;
            harbor.call(reqwest::Method::POST, &format!("/routines/{}/run", path_segment(id)), Some(json!({})), ACTION_TIMEOUT).await.map(|body| as_text(&body)).map_err(failure)
        }
        "computer_desktop_url" => {
            let body = harbor.call(reqwest::Method::GET, "", None, STATUS_TIMEOUT).await.map_err(failure)?;
            let url = body["desktop_url"].as_str().filter(|url| same_origin(&link.address, url));
            Ok(serde_json::to_string_pretty(&json!({
                "desktop_url": url,
                "note": "Owner-only live view. The Harbor owner must sign in.",
            }))
            .unwrap_or_default())
        }
        other => Err(format!("Unknown tool {other}.")),
    }
}

/// MCP calls from `retinued computer-server`, authorised by the conversation's
/// own capability (the same one the team bridge uses).
pub async fn mcp_rpc(st: &AppState, session_id: &str, method: &str, params: &Value) -> Result<Value, DispatchError> {
    match method {
        "mcp.computer.list" => Ok(json!({"tools": tool_schemas()})),
        "mcp.computer.call" => {
            let name = params["name"].as_str().unwrap_or("");
            if name.is_empty() || name.len() > 40 || !name.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'_') {
                return Ok(json!({"content":[{"type":"text","text":"Unknown tool."}],"isError":true}));
            }
            // One content-free line per call: the tool's own name, never its
            // arguments, its output or the address.
            tracing::debug!("vps-computer tool call: {name}");
            let result = call_tool(st, session_id, name, &params["arguments"]).await;
            Ok(match result {
                Ok(text) => json!({"content":[{"type":"text","text":cap_output(text)}]}),
                Err(text) => json!({"content":[{"type":"text","text":cap_output(text)}],"isError":true}),
            })
        }
        _ => Err(derr("not_found", "unknown computer method", StatusCode::NOT_FOUND)),
    }
}

// ---------------------------------------------------------------------------
// The MCP server a new conversation gets

fn endpoint_path(dir: &Path, session_id: &str) -> PathBuf {
    let safe = session_id.chars().filter(|ch| ch.is_ascii_alphanumeric() || *ch == '-' || *ch == '_').take(80).collect::<String>();
    let safe = if safe.is_empty() { Uuid::new_v4().to_string() } else { safe };
    dir.join(ENDPOINT_FOLDER).join(format!("mcp-{safe}.json"))
}

/// Creates the endpoint file empty, restricts it to the current user, and only
/// then writes the address and token, so the token is never in a file others can
/// read.
fn write_endpoint(path: &Path, address: &str, token: &str) -> Result<(), String> {
    use std::io::Write;
    let body = json!({"address": address, "token": token}).to_string();
    // The options are rebuilt on several paths; an unchanged file is left alone,
    // so a running bridge keeps reading the file it already has.
    if std::fs::read_to_string(path).is_ok_and(|existing| existing == body) {
        return Ok(());
    }
    let dir = path.parent().ok_or_else(|| "the endpoint file has no folder".to_owned())?;
    std::fs::create_dir_all(dir).map_err(|_| "the endpoint folder could not be created".to_owned())?;
    let _ = std::fs::remove_file(path);
    let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(path).map_err(|_| "the endpoint file could not be created".to_owned())?;
    crate::remote::native::apply_key_permissions(path);
    file.write_all(body.as_bytes()).and_then(|_| file.flush()).map_err(|_| "the endpoint file could not be written".to_owned())
}

/// Deletes every endpoint file in the folder. Called at startup and when the
/// computer is unlinked or replaced: a stale file names a token that a restart
/// ended, and the bridge re-reads a fresh one only when a conversation starts.
pub(crate) fn sweep_endpoints(data_dir: &Path) {
    let dir = data_dir.join(ENDPOINT_FOLDER);
    let Ok(entries) = std::fs::read_dir(&dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// The `vps-computer` server for one conversation, or `None` when the computer
/// is not linked, is switched off, or the endpoint file could not be written.
/// The spec carries no token: the bridge reads the endpoint file, and every
/// Harbor call is made here.
pub fn mcp_server_for(st: &AppState, session_id: &str) -> Option<McpServerSpec> {
    enabled_link(st)?;
    let address = st.team.address.lock().ok().and_then(|address| address.clone())?;
    let dir = data_dir(st)?;
    let executable = std::env::current_exe().ok()?;
    let endpoint = endpoint_path(&dir, session_id);
    if write_endpoint(&endpoint, &address, &st.team.token_for(session_id)).is_err() {
        tracing::warn!("the computer bridge's endpoint file could not be written");
        return None;
    }
    Some(McpServerSpec {
        name: SERVER_NAME.into(),
        command: executable,
        args: vec!["computer-server".into(), "--endpoint".into(), endpoint.to_string_lossy().into_owned()],
        env: BTreeMap::new(),
    })
}

/// A backup import leaves the computer unlinked: the address and the token are
/// this machine's own, and a restored install must link its computer again.
pub async fn forget_link(st: &AppState) {
    let Some(_previous) = st.db.computer_link().ok().flatten() else { return };
    let _ = unlink(st).await;
}

// ---------------------------------------------------------------------------
// The screen: HTTP and WebSocket proxying under a ticket

/// The two routes the framed screen uses. A frame cannot send the daemon's
/// capability, so the ticket in the path is the only proof.
pub(crate) fn view_routes() -> Router<AppState> {
    Router::new()
        .route(&format!("{VIEW_ROUTE}/{{ticket}}"), get(view_root))
        // The framed URL always ends with `/`, and axum treats that as a
        // different path from the bare ticket.
        .route(&format!("{VIEW_ROUTE}/{{ticket}}/"), get(view_root))
        .route(&format!("{VIEW_ROUTE}/{{ticket}}/{{*rest}}"), get(view_under))
}

pub(crate) async fn view_root(State(st): State<AppState>, AxumPath(ticket): AxumPath<String>, request: Request<Body>) -> Response {
    view_request(st, ticket, String::new(), request).await
}

pub(crate) async fn view_under(State(st): State<AppState>, AxumPath((ticket, rest)): AxumPath<(String, String)>, request: Request<Body>) -> Response {
    view_request(st, ticket, rest, request).await
}

async fn view_request(st: AppState, ticket: String, rest: String, request: Request<Body>) -> Response {
    if !st.computer.accepts_view_ticket(&ticket) {
        return StatusCode::NOT_FOUND.into_response();
    }
    // Unlinking clears the tickets; reading the link again keeps a stale state
    // from reaching a server the owner has already unlinked.
    let Ok(Some(link)) = st.db.computer_link() else { return StatusCode::NOT_FOUND.into_response() };
    if !view_rest_is_safe(&rest) {
        return StatusCode::NOT_FOUND.into_response();
    }
    if upgrade_requested(request.headers()) {
        return websocket(st, link, rest, request).await;
    }
    forward_http(link, &ticket, &rest, request).await
}

/// Whether a captured path stays inside the view prefix: no segment climbs out,
/// no backslash, and no query or fragment smuggled through the path.
fn view_rest_is_safe(rest: &str) -> bool {
    if rest.chars().any(|ch| ch.is_control() || ch == '\\' || ch == '?' || ch == '#') {
        return false;
    }
    rest.split('/').all(|segment| segment != "." && segment != "..")
}

fn upgrade_requested(headers: &HeaderMap) -> bool {
    let connection = headers.get(header::CONNECTION).and_then(|value| value.to_str().ok()).unwrap_or("");
    let upgrade = headers.get(header::UPGRADE).and_then(|value| value.to_str().ok()).unwrap_or("");
    connection.split(',').any(|part| part.trim().eq_ignore_ascii_case("upgrade")) && upgrade.eq_ignore_ascii_case("websocket")
}

/// The upstream URL for one path and query. Always the linked address and the
/// fixed view prefix: no part of the request can choose a host.
fn upstream_view(address: &str, rest: &str, query: Option<&str>) -> String {
    let query = query.map(|query| format!("?{query}")).unwrap_or_default();
    if rest.is_empty() {
        return format!("{address}{VIEW_PATH}/{query}");
    }
    format!("{address}{VIEW_PATH}/{rest}{query}")
}

/// A redirect back to the server's own view path, re-pointed at the framed
/// copy; anything else is left alone.
fn rewrite_location(address: &str, ticket: &str, location: &str) -> String {
    for prefix in [format!("{address}{VIEW_PATH}"), VIEW_PATH.to_owned()] {
        if let Some(rest) = location.strip_prefix(&prefix) {
            if rest.is_empty() || rest.starts_with('/') {
                return format!("{VIEW_ROUTE}/{ticket}{rest}");
            }
        }
    }
    location.to_owned()
}

async fn forward_http(link: ComputerLink, ticket: &str, rest: &str, request: Request<Body>) -> Response {
    let Ok(token) = token_of(&link) else { return StatusCode::NOT_FOUND.into_response() };
    let query = request.uri().query().map(str::to_owned);
    let target = upstream_view(&link.address, rest, query.as_deref());
    let Ok(client) = Harbor::client() else { return StatusCode::BAD_GATEWAY.into_response() };
    let mut upstream = client
        .get(&target)
        .header(reqwest::header::AUTHORIZATION, format!("Bearer {}", token.as_str()))
        .header("X-Harbor-Client", "mcp")
        .timeout(VIEW_TIMEOUT);
    // Only what a page and its assets need: no cookie, no authorization of the
    // window's, and no accept-encoding, so the answer can be buffered as it is.
    for name in ["accept", "accept-language", "range", "if-none-match", "if-modified-since"] {
        if let Some(value) = request.headers().get(name).and_then(|value| value.to_str().ok()) {
            upstream = upstream.header(name, value);
        }
    }
    let Ok(answer) = upstream.send().await else { return StatusCode::BAD_GATEWAY.into_response() };
    let status = StatusCode::from_u16(answer.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let mut headers = HeaderMap::new();
    for (name, value) in answer.headers() {
        let text = name.as_str();
        if STRIPPED_VIEW_HEADERS.contains(&text) {
            continue;
        }
        if text.eq_ignore_ascii_case("location") {
            let location = rewrite_location(&link.address, ticket, value.to_str().unwrap_or(""));
            if let Ok(value) = HeaderValue::from_str(&location) {
                headers.insert(header::LOCATION, value);
            }
            continue;
        }
        if let (Ok(name), Ok(value)) = (HeaderName::from_bytes(text.as_bytes()), HeaderValue::from_bytes(value.as_bytes())) {
            headers.append(name, value);
        }
    }
    // The screen may be framed by the app and by nothing else.
    if let Ok(value) = HeaderValue::from_str(&format!("frame-ancestors {APP_ORIGINS}")) {
        headers.insert(header::CONTENT_SECURITY_POLICY, value);
    }
    let Ok(body) = answer.bytes().await else { return StatusCode::BAD_GATEWAY.into_response() };
    (status, headers, Body::from(body)).into_response()
}

/// A WebSocket upgrade to the server's view path, carrying the token in the
/// handshake, then both directions copied between the frame and the server.
async fn websocket(st: AppState, link: ComputerLink, rest: String, request: Request<Body>) -> Response {
    let (mut parts, _body) = request.into_parts();
    let protocols = parts
        .headers
        .get(header::SEC_WEBSOCKET_PROTOCOL)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.split(',').map(|protocol| protocol.trim().to_owned()).filter(|protocol| !protocol.is_empty()).collect::<Vec<_>>())
        .unwrap_or_default();
    let query = parts.uri.query().map(str::to_owned);
    let Ok(upgrade) = WebSocketUpgrade::from_request_parts(&mut parts, &st).await else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Ok(token) = token_of(&link) else { return StatusCode::NOT_FOUND.into_response() };
    let target = ws_target(&link.address, &rest, query.as_deref());
    let upgrade = if protocols.is_empty() { upgrade } else { upgrade.protocols(protocols) };
    upgrade.on_upgrade(move |socket| pump(socket, target, token))
}

fn ws_target(address: &str, rest: &str, query: Option<&str>) -> String {
    let (scheme, host) = match address.strip_prefix("https://") {
        Some(host) => ("wss", host),
        None => ("ws", address.strip_prefix("http://").unwrap_or(address)),
    };
    let query = query.map(|query| format!("?{query}")).unwrap_or_default();
    if rest.is_empty() {
        return format!("{scheme}://{host}{VIEW_PATH}/{query}");
    }
    format!("{scheme}://{host}{VIEW_PATH}/{rest}{query}")
}

/// One WebSocket, both directions until either side closes. The token is read
/// for this connection and dropped with it.
async fn pump(socket: WebSocket, target: String, token: Zeroizing<String>) {
    let Ok(mut request) = target.as_str().into_client_request() else { return };
    let Ok(authorization) = HeaderValue::from_str(&format!("Bearer {}", token.as_str())) else { return };
    request.headers_mut().insert(header::AUTHORIZATION, authorization);
    request.headers_mut().insert("x-harbor-client", HeaderValue::from_static("mcp"));
    let Ok((upstream, _answer)) = tokio_tungstenite::connect_async(request).await else { return };
    let (mut to_client, mut from_client) = socket.split();
    let (mut to_server, mut from_server) = upstream.split();
    let client_to_server = async {
        while let Some(Ok(message)) = from_client.next().await {
            if to_server.send(to_tungstenite(message)).await.is_err() {
                break;
            }
        }
        let _ = to_server.close().await;
    };
    let server_to_client = async {
        while let Some(Ok(message)) = from_server.next().await {
            if let Some(message) = from_tungstenite(message) {
                if to_client.send(message).await.is_err() {
                    break;
                }
            }
        }
        let _ = to_client.close().await;
    };
    tokio::select! {
        _ = client_to_server => {}
        _ = server_to_client => {}
    }
}

fn to_tungstenite(message: WsMessage) -> TgMessage {
    match message {
        WsMessage::Text(text) => TgMessage::Text(text.as_str().to_owned().into()),
        WsMessage::Binary(bytes) => TgMessage::Binary(bytes),
        WsMessage::Ping(bytes) => TgMessage::Ping(bytes),
        WsMessage::Pong(bytes) => TgMessage::Pong(bytes),
        WsMessage::Close(_) => TgMessage::Close(None),
    }
}

fn from_tungstenite(message: TgMessage) -> Option<WsMessage> {
    match message {
        TgMessage::Text(text) => Some(WsMessage::Text(text.as_str().into())),
        TgMessage::Binary(bytes) => Some(WsMessage::Binary(bytes)),
        TgMessage::Ping(bytes) => Some(WsMessage::Ping(bytes)),
        TgMessage::Pong(bytes) => Some(WsMessage::Pong(bytes)),
        TgMessage::Close(_) => Some(WsMessage::Close(None)),
        TgMessage::Frame(_) => None,
    }
}

#[cfg(test)]
impl Computer {
    pub(crate) fn mint_view_ticket_for_tests(&self) -> String {
        self.mint_view_ticket()
    }

    pub(crate) fn expire_view_tickets_for_tests(&self) {
        let mut tickets = self.view.lock().unwrap_or_else(|poison| poison.into_inner());
        for ticket in tickets.iter_mut() {
            ticket.last_used = std::time::Instant::now() - VIEW_TTL - Duration::from_secs(1);
        }
    }
}
