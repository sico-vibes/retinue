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
use base64::Engine as _;
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
/// The gate the owner's amendment puts on the input tools: Full access removes
/// every approval card, and this is the capability that must keep asking.
const FULL_ACCESS: &str = "This conversation has Full access, so it does not get the computer's mouse, keyboard or screenshots: those keep asking, even here. Change the conversation's permission mode and try again.";
/// The gate for a turn a teammate started (`AGENTS.md`, 14 November 2026): a
/// chain of bots never drives the owner's screen, even in an attended
/// conversation. The owner's own next message brings it back.
const TEAM_REPLY: &str = "This turn was started by a teammate's message, so it cannot drive the computer's screen. Follow up in the conversation, and use the screen in a turn the owner starts.";

/// The screen tools (`AGENTS.md`, 14 November 2026). They act on the
/// container's desktop through the same `exec` path as everything else, and
/// they are commands for the unattended refusal: a scheduled run never drives a
/// mouse. Every one is also refused in a Full-access conversation.
const INPUT_TOOLS: [&str; 6] = ["computer_screenshot", "computer_click", "computer_move", "computer_type", "computer_key", "computer_scroll"];

/// The tools that change the computer, refused while nobody is watching.
const COMMAND_TOOLS: [&str; 9] = ["computer_exec", "computer_create_routine", "computer_run_routine", "computer_screenshot", "computer_click", "computer_move", "computer_type", "computer_key", "computer_scroll"];

/// The most characters one `computer_type` call may send.
const TYPE_MAX: usize = 2_000;
/// The most steps one scroll may take, and its default.
const SCROLL_MAX: u64 = 50;
const SCROLL_DEFAULT: u64 = 3;
/// A screenshot is downscaled to this width before it is kept.
const SCREENSHOT_MAX_WIDTH: u32 = 1_280;
/// The largest screenshot kept as one image, the size an attached image is capped at.
const SCREENSHOT_MAX_BYTES: usize = 8 * 1024 * 1024;
/// The capture tools the container may have; the first one found is used.
const CAPTURE_TOOLS: [&str; 3] = ["scrot", "gnome-screenshot", "import"];
/// How long a probe is believed: `computer.status` asks the container again
/// after this, so a poll does not run a command every few seconds.
const INPUT_PROBE_TTL: Duration = Duration::from_secs(300);

/// What the daemon remembers between reads: the last status it announced, so an
/// unchanged read does not produce an event, and the live tickets for the
/// computer's screen.
#[derive(Default)]
pub struct Computer {
    last: StdMutex<Option<Value>>,
    /// A frame cannot send the daemon's capability, so the screen is served
    /// under single-purpose tickets instead: random, short-lived, never logged.
    view: StdMutex<Vec<ViewTicket>>,
    /// The last probe of the container's input tools and display, kept for
    /// `INPUT_PROBE_TTL` so a status poll does not run a command every time.
    input: StdMutex<Option<InputProbe>>,
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

    /// The last input probe, while it is still believed.
    fn cached_input(&self) -> Option<InputProbe> {
        let probe = self.input.lock().unwrap_or_else(|poison| poison.into_inner()).clone()?;
        (probe.at.elapsed() < INPUT_PROBE_TTL).then_some(probe)
    }

    fn remember_input(&self, probe: InputProbe) {
        if let Ok(mut input) = self.input.lock() {
            *input = Some(probe);
        }
    }

    /// A link, an unlink or a switch changes which container is asked, so the
    /// next status read probes again.
    fn clear_input(&self) {
        if let Ok(mut input) = self.input.lock() {
            *input = None;
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
pub(crate) fn enabled_link(st: &AppState) -> Option<ComputerLink> {
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
        "input": null,
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
        "input": Value::Null,
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
        "input": null,
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
    let status = match harbor.call(reqwest::Method::GET, "", None, STATUS_TIMEOUT).await {
        Ok(body) if body["computer"].is_object() => map_status(&address, enabled, &body),
        Ok(_) => offline_status(&link, name.as_deref(), "That address answered, but not like Harbor.".into()),
        Err(error) => offline_status(&link, name.as_deref(), problem_of(error)),
    };
    with_input(st, &link, status).await
}

// ---------------------------------------------------------------------------
// The container's input tools: what it can drive, and the screen it shows

/// What one probe of the container found. `missing` names what to install, in
/// the wording `computer.status` reports and a tool result repeats.
#[derive(Clone)]
struct InputProbe {
    at: std::time::Instant,
    /// The X display the desktop stream draws on, as `:1`.
    display: Option<String>,
    width: u32,
    height: u32,
    missing: Vec<String>,
}

impl InputProbe {
    /// Everything the six tools need: xdotool, one capture tool and a display
    /// whose size can be read.
    fn ready(&self) -> bool {
        self.missing.is_empty() && self.display.is_some() && self.width > 0 && self.height > 0
    }
}

/// One probe of the container, run as one script: which of the tools exist,
/// which display answers, and how large its screen is. The display is found
/// rather than assumed: the panel's own `DISPLAY`, then the sockets under
/// `/tmp/.X11-unix`, then the two numbers a web desktop usually uses.
///
/// Nothing here is built from a caller's input, so the script is a constant.
const PROBE_SCRIPT: &str = r#"# retinue:probe
for t in xdotool scrot gnome-screenshot import convert; do
  if command -v "$t" >/dev/null 2>&1; then echo "have:$t"; else echo "miss:$t"; fi
done
cands="${DISPLAY:-}"
if [ -d /tmp/.X11-unix ]; then for s in /tmp/.X11-unix/X*; do cands="$cands ${s##*X}"; done; fi
cands="$cands 1 0"
for d in $cands; do
  d="${d#:}"
  g=$(DISPLAY=":$d" xdotool getdisplaygeometry 2>/dev/null) || continue
  echo "display:$d"
  echo "geometry:$g"
  break
done
"#;

/// `1`, `:1` or `:1.0` as `:1`. A display spec may carry a screen number, which
/// X ignores when a tool asks for the whole display. Anything that is not a
/// display number is no display: a remote display's host name is never put back
/// into a command.
fn display_for(raw: &str) -> Option<String> {
    let spec = raw.trim().trim_start_matches(':');
    // `host:0` or `host:0.0` would name a host; only a bare number is ours.
    let digits = spec.split('.').next().unwrap_or("");
    if digits.is_empty() || digits.len() > 3 || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    if !spec[digits.len()..].trim_start_matches('.').bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    Some(format!(":{digits}"))
}

/// `xdotool getdisplaygeometry` answers `WxH`, and some versions answer `W H`.
fn screen_size(raw: &str) -> (u32, u32) {
    let cleaned = raw.trim().replace('x', " ");
    let mut parts = cleaned.split_whitespace().filter_map(|part| part.parse::<u32>().ok());
    match (parts.next(), parts.next()) {
        (Some(width), Some(height)) => (width, height),
        _ => (0, 0),
    }
}

/// The probe's own output, as the probe describes it. Never fails: a script
/// that printed nothing describes a container with no tools and no display.
fn parse_probe(output: &str) -> InputProbe {
    let mut have: Vec<String> = Vec::new();
    let mut display = None;
    let mut size = (0, 0);
    for line in output.lines() {
        let line = line.trim();
        if let Some(name) = line.strip_prefix("have:") {
            have.push(name.trim().to_owned());
        } else if let Some(value) = line.strip_prefix("display:") {
            display = display_for(value);
        } else if let Some(value) = line.strip_prefix("geometry:") {
            size = screen_size(value);
        }
    }
    let mut missing = Vec::new();
    if !have.iter().any(|name| name == "xdotool") {
        missing.push("xdotool".to_owned());
    }
    if !CAPTURE_TOOLS.iter().any(|tool| have.iter().any(|name| name == tool)) {
        // Any one of them will do; scrot is the smallest to install.
        missing.push(CAPTURE_TOOLS[0].to_owned());
    }
    InputProbe { at: std::time::Instant::now(), display, width: size.0, height: size.1, missing }
}

/// What a probe that found no tools tells the model to install, and where.
fn missing_tools_message(missing: &[String]) -> String {
    let packages = missing.join(" ");
    format!("The bot computer is missing {} for clicks, keys and screenshots. Install it in the container with: apt-get install -y {packages}.", missing.join(" and "))
}

/// Harbor's `/exec` answer as (exit code, output). A body that says `ok`
/// without a code counts as success, as does a body with neither.
fn exec_answer(body: &Value) -> (i64, String) {
    let output = body["output"].as_str().or_else(|| body["stdout"].as_str()).unwrap_or("").to_owned();
    let code = body["code"].as_i64().or_else(|| body["exitCode"].as_i64()).unwrap_or(0);
    let ok = body["ok"].as_bool().unwrap_or(code == 0);
    if ok { (0, output) } else if code == 0 { (1, output) } else { (code, output) }
}

/// A failed command's own first line, bounded and free of control characters.
fn first_line(output: &str) -> Option<String> {
    let line = output.lines().find(|line| !line.trim().is_empty())?.trim().chars().filter(|ch| !ch.is_control()).take(200).collect::<String>();
    if line.is_empty() { None } else { Some(line) }
}

/// Runs one of Retinue's own scripts in the container, and answers what it
/// printed or the container's own complaint. The caller builds every script from
/// numbers, key names and base64, never from typed text.
async fn run_script(harbor: &Harbor, script: &str, limit: Duration) -> Result<String, String> {
    let timeout_ms = (limit.as_millis() as u64).clamp(1, EXEC_MAX_MS);
    let body = harbor
        .call(reqwest::Method::POST, "/exec", Some(json!({"command": script, "timeout_ms": timeout_ms})), limit + EXEC_MARGIN)
        .await
        .map_err(problem_of)?;
    let (code, output) = exec_answer(&body);
    if code != 0 {
        return Err(first_line(&output).unwrap_or_else(|| format!("The command inside the computer failed (exit code {code}).")));
    }
    Ok(output)
}

/// One probe for a tool call: tools and display, or the sentence saying what to
/// install. A tool always probes again, because a container can gain or lose a
/// package between calls.
async fn probe_for_tool(harbor: &Harbor) -> Result<InputProbe, String> {
    let probe = parse_probe(&run_script(harbor, PROBE_SCRIPT, ACTION_TIMEOUT).await?);
    if !probe.missing.is_empty() {
        return Err(missing_tools_message(&probe.missing));
    }
    if probe.display.is_none() || probe.width == 0 || probe.height == 0 {
        return Err("The bot computer has no desktop running, so there is no screen to use. Wake it, and start its desktop if it stays off.".into());
    }
    Ok(probe)
}

/// The input probe for a status read, from the cache while it is fresh. One
/// command at most every `INPUT_PROBE_TTL`.
async fn input_probe(st: &AppState, link: &ComputerLink) -> Option<InputProbe> {
    if let Some(probe) = st.computer.cached_input() {
        return Some(probe);
    }
    let harbor = Harbor { address: link.address.clone(), token: token_of(link).ok()? };
    let probe = parse_probe(&run_script(&harbor, PROBE_SCRIPT, STATUS_TIMEOUT).await.ok()?);
    st.computer.remember_input(probe.clone());
    Some(probe)
}

/// Adds `input` to a linked status: whether the computer can be driven right
/// now, and which tools it is missing. `null` when the computer is not switched
/// on, is asleep, or the probe did not answer: there is nothing to set up then.
async fn with_input(st: &AppState, link: &ComputerLink, mut status: Value) -> Value {
    if status["reachable"] != true || status["state"] != "awake" || !link.enabled {
        return status;
    }
    if let Some(probe) = input_probe(st, link).await {
        status["input"] = json!({"ready": probe.ready(), "missing": probe.missing});
    }
    status
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
    st.computer.clear_input();
    publish(st, status.clone(), true).await;
    Ok(status)
}

/// `computer.unlink {}`. The address, the switch and the token all go.
async fn unlink(st: &AppState) -> Result<Value, DispatchError> {
    let previous = st.db.computer_link().ok().flatten();
    st.db.computer_link_clear().map_err(|_| internal("The computer's link could not be removed."))?;
    st.computer.clear_view_tickets();
    st.computer.clear_input();
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
    st.computer.clear_input();
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
    st.computer.clear_input();
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

/// The sixteen tools: the ten the VPS computer proposal's Node bridge
/// (`tools/vps-computer-mcp/`, commit `2182a94`) defines with the same names,
/// descriptions and schemas, then the six screen tools of 14 November 2026.
/// That folder is not in this branch; this module is its Retinue side.
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
        json!({"name":"computer_screenshot","description":"Take one screenshot of the bot computer's desktop and store it in this conversation as an image. Never on a timer and never in a loop; use it when the desktop's text is not enough.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}}),
        json!({"name":"computer_click","description":"Click at a point on the bot computer's screen. Use computer_screenshot first to see where things are.","inputSchema":{"type":"object","properties":{"x":{"type":"number","description":"Pixels from the left edge."},"y":{"type":"number","description":"Pixels from the top edge."},"button":{"type":"string","enum":["left","middle","right"],"description":"Default left."},"double":{"type":"boolean","description":"Click twice, one double click."}},"required":["x","y"],"additionalProperties":false}}),
        json!({"name":"computer_move","description":"Move the pointer on the bot computer's screen without clicking.","inputSchema":{"type":"object","properties":{"x":{"type":"number"},"y":{"type":"number"}},"required":["x","y"],"additionalProperties":false}}),
        json!({"name":"computer_type","description":"Type text at the current focus, as the desktop user. The text is never recorded: only how many characters were sent.","inputSchema":{"type":"object","properties":{"text":{"type":"string","description":"At most 2,000 characters."}},"required":["text"],"additionalProperties":false}}),
        json!({"name":"computer_key","description":"Press a key or a combination, like Return, Tab, Escape, ctrl+l or ctrl+shift+t.","inputSchema":{"type":"object","properties":{"keys":{"type":"string","description":"Key names joined by +, e.g. ctrl+l or Return."}},"required":["keys"],"additionalProperties":false}}),
        json!({"name":"computer_scroll","description":"Scroll at a point on the bot computer's screen.","inputSchema":{"type":"object","properties":{"x":{"type":"number"},"y":{"type":"number"},"direction":{"type":"string","enum":["up","down","left","right"]},"amount":{"type":"number","description":"Steps, 1 to 50, default 3."}},"required":["x","y","direction"],"additionalProperties":false}}),
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
fn nobody_is_watching(st: &AppState, session: Option<&Value>) -> bool {
    session.is_none_or(|session| crate::questions::may_ask(st, session).is_err())
}

/// Whether this conversation's newest prompt came from a teammate rather than
/// the owner: a message, a relayed reply or an update from another bot. Those
/// are not the owner watching, so the screen stays out of reach. Only the newest
/// prompt counts, because it is the one that started the running turn; the
/// owner's own next message brings the screen back.
fn woken_by_a_teammate(session: Option<&Value>) -> bool {
    let Some(messages) = session.and_then(|session| session["messages"].as_array()) else { return false };
    messages
        .iter()
        .rev()
        .find(|message| message["role"] == "user")
        .is_some_and(|prompt| matches!(prompt["meta"]["kind"].as_str(), Some("blob_message" | "blob_reply" | "blob_update")))
}

/// Whether this conversation runs with Full access. Full access removes every
/// approval card, and the computer's input tools are the capability that must
/// keep asking, so a `bypass` conversation never gets them (`AGENTS.md`,
/// 14 November 2026).
fn full_access(st: &AppState, session: Option<&Value>) -> bool {
    session.is_some_and(|session| crate::approval_mode_for_session(st, session) == retinue_agent_core::ApprovalMode::Bypass)
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
    // One read of the conversation, for the two gates that need it. An
    // unreadable conversation is never given a mouse.
    let session = st.db.session_detail(session_id).ok();
    if COMMAND_TOOLS.contains(&name) && nobody_is_watching(st, session.as_ref()) {
        return Err(UNATTENDED.into());
    }
    if INPUT_TOOLS.contains(&name) && woken_by_a_teammate(session.as_ref()) {
        return Err(TEAM_REPLY.into());
    }
    if INPUT_TOOLS.contains(&name) && full_access(st, session.as_ref()) {
        return Err(FULL_ACCESS.into());
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
        "computer_screenshot" | "computer_click" | "computer_move" | "computer_type" | "computer_key" | "computer_scroll" => {
            let (result, activity) = input_tool(st, &harbor, session_id, name, args).await?;
            // One content-free row per action class: the action, never the text
            // that was typed and never a pixel.
            crate::team::note(st, session_id, &activity, json!({"kind":"computer_action","action":action_class(name)})).await;
            Ok(result)
        }
        other => Err(format!("Unknown tool {other}.")),
    }
}

/// The vault's fill on the linked computer (16 November 2026): the value is
/// typed at the focused field, under the same gates as `computer_type`, and the
/// text never appears in a reply, a note or a log. A failure of the typing
/// command itself is `uncertain`, because part of the text may have arrived.
pub(crate) async fn type_at_focus(st: &AppState, session_id: &str, text: &str) -> Result<(), crate::vault::FillRefusal> {
    use crate::vault::FillRefusal;
    let Some(link) = st.db.computer_link().ok().flatten() else { return Err(FillRefusal::Refused(NOT_LINKED.into())) };
    if !link.enabled {
        return Err(FillRefusal::Refused(SWITCHED_OFF.into()));
    }
    let session = st.db.session_detail(session_id).ok();
    if nobody_is_watching(st, session.as_ref()) {
        return Err(FillRefusal::Refused(UNATTENDED.into()));
    }
    if woken_by_a_teammate(session.as_ref()) {
        return Err(FillRefusal::Refused(TEAM_REPLY.into()));
    }
    if full_access(st, session.as_ref()) {
        return Err(FillRefusal::Refused(FULL_ACCESS.into()));
    }
    let count = text.chars().count();
    if count == 0 || text.contains('\0') {
        return Err(FillRefusal::Refused("That saved value cannot be typed on the computer.".into()));
    }
    if count > TYPE_MAX {
        return Err(FillRefusal::Refused(format!("That saved value is longer than the computer types in one go ({TYPE_MAX} characters). Save a shorter value, or use the browser.")));
    }
    let harbor = Harbor { address: link.address.clone(), token: token_of(&link).map_err(|error| FillRefusal::Refused(error.1.message))? };
    let probe = probe_for_tool(&harbor).await.map_err(FillRefusal::Refused)?;
    let display = probe.display.clone().unwrap_or_default();
    let encoded = base64::engine::general_purpose::STANDARD.encode(text.as_bytes());
    let script = format!("# retinue:type\nexport DISPLAY={display}\nprintf '%s' '{encoded}' | base64 -d | xdotool type --clearmodifiers --delay 12 --file -\n");
    match run_script(&harbor, &script, ACTION_TIMEOUT).await {
        Ok(_) => {
            crate::team::note(st, session_id, &format!("Typed {count} characters in the bot computer."), json!({"kind":"computer_action","action":"type"})).await;
            Ok(())
        }
        Err(text) => Err(FillRefusal::Uncertain(text)),
    }
}

/// The action an input tool is recorded as.
fn action_class(name: &str) -> &'static str {    match name {
        "computer_screenshot" => "screenshot",
        "computer_click" => "click",
        "computer_move" => "move",
        "computer_type" => "type",
        "computer_key" => "key",
        _ => "scroll",
    }
}

/// A coordinate pair as the caller sent it: whole numbers, never negative.
fn point(args: &Value) -> Result<(i64, i64), String> {
    let read = |key: &str| args.get(key).and_then(Value::as_i64).ok_or_else(|| format!("{key} must be a whole number of pixels."));
    let x = read("x")?;
    let y = read("y")?;
    if x < 0 || y < 0 {
        return Err("x and y must be zero or more pixels from the top-left corner.".into());
    }
    Ok((x, y))
}

/// The mouse button as a number xdotool knows: 1 left, 2 middle, 3 right.
fn button_of(args: &Value) -> Result<u8, String> {
    let name = match args.get("button") {
        None | Some(Value::Null) => return Ok(1),
        Some(Value::Number(number)) => number.as_u64().map(|number| number.to_string()),
        Some(Value::String(text)) => Some(text.trim().to_ascii_lowercase()),
        _ => None,
    };
    match name.as_deref() {
        Some("left" | "1") => Ok(1),
        Some("middle" | "2") => Ok(2),
        Some("right" | "3") => Ok(3),
        _ => Err("button must be left, middle, right, 1, 2 or 3.".into()),
    }
}

fn button_name(button: u8) -> &'static str {
    match button { 2 => "middle", 3 => "right", _ => "left" }
}

/// A keystroke as xdotool names it: `Return`, `ctrl+l`, `ctrl+shift+t`. Each
/// `+`-separated part is letters, digits and `_` only, so a key name can never
/// become anything but a key name in the container's shell.
fn keys_of(args: &Value) -> Result<String, String> {
    let raw = args["keys"].as_str().map(str::trim).filter(|keys| !keys.is_empty()).ok_or("keys is required.")?;
    if raw.len() > 60 {
        return Err("keys must be at most 60 characters.".into());
    }
    let parts = raw.split('+').collect::<Vec<_>>();
    let valid = !parts.is_empty()
        && parts.len() <= 8
        && parts.iter().all(|part| !part.is_empty() && part.len() <= 20 && part.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '_'));
    if !valid {
        return Err("keys must be key names joined by +, like Return, Tab or ctrl+l.".into());
    }
    Ok(raw.to_owned())
}

/// The PNG from the capture script's own output, or the reason it is not one.
fn screenshot_bytes(output: &str) -> Result<Vec<u8>, String> {
    let encoded = output.lines().find_map(|line| line.trim().strip_prefix("RETINUE_PNG:")).ok_or("The screenshot did not come back from the computer.")?;
    let bytes = base64::engine::general_purpose::STANDARD.decode(encoded.trim().as_bytes()).map_err(|_| "The screenshot came back damaged.".to_owned())?;
    if bytes.is_empty() {
        return Err("The screenshot came back empty.".to_owned());
    }
    if bytes.len() > SCREENSHOT_MAX_BYTES {
        return Err("The screenshot was too large to keep.".to_owned());
    }
    if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err("The screenshot was not a PNG.".to_owned());
    }
    Ok(bytes)
}

/// The six screen tools, in one place: validate what the caller sent, probe the
/// container once, refuse a point that is off its screen, then run one command.
/// `Ok((result for the agent, one content-free activity line))`.
async fn input_tool(st: &AppState, harbor: &Harbor, session_id: &str, name: &str, args: &Value) -> Result<(String, String), String> {
    // Everything the caller sends is checked before the container is asked
    // anything, so a bad argument costs no round trip.
    let placed = matches!(name, "computer_click" | "computer_move" | "computer_scroll");
    let position = if placed { Some(point(args)?) } else { None };
    let button = if name == "computer_click" { Some(button_of(args)?) } else { None };
    let double = args["double"].as_bool().unwrap_or(false);
    let keys = if name == "computer_key" { Some(keys_of(args)?) } else { None };
    let typed = if name == "computer_type" {
        let text = args["text"].as_str().filter(|text| !text.is_empty()).ok_or("text is required.")?;
        let count = text.chars().count();
        if count > TYPE_MAX {
            return Err(format!("text must be at most {TYPE_MAX} characters."));
        }
        if text.contains('\0') {
            return Err("text cannot carry a NUL character.".into());
        }
        Some((text.to_owned(), count))
    } else {
        None
    };
    let scroll = if name == "computer_scroll" {
        let direction = args["direction"].as_str().unwrap_or("").trim().to_ascii_lowercase();
        let button = match direction.as_str() {
            "up" => 4u8,
            "down" => 5,
            "left" => 6,
            "right" => 7,
            _ => return Err("direction must be up, down, left or right.".into()),
        };
        let amount = match args.get("amount") {
            None | Some(Value::Null) => SCROLL_DEFAULT,
            Some(value) => value.as_u64().filter(|amount| (1..=SCROLL_MAX).contains(amount)).ok_or("amount must be a whole number of steps, 1 to 50.")?,
        };
        Some((direction, button, amount))
    } else {
        None
    };
    let probe = probe_for_tool(harbor).await?;
    let display = probe.display.clone().unwrap_or_default();
    if let Some((x, y)) = position {
        if x >= probe.width as i64 || y >= probe.height as i64 {
            return Err(format!("That point is off the screen: the bot computer's screen is {} by {} pixels.", probe.width, probe.height));
        }
    }
    match name {
        "computer_move" => {
            let (x, y) = position.unwrap_or((0, 0));
            let script = format!("# retinue:move\nexport DISPLAY={display}\nxdotool mousemove --sync {x} {y}\n");
            run_script(harbor, &script, ACTION_TIMEOUT).await?;
            Ok((format!("Moved the pointer to {x}, {y}."), "Moved the pointer in the bot computer.".to_owned()))
        }
        "computer_click" => {
            let (x, y) = position.unwrap_or((0, 0));
            let button = button.unwrap_or(1);
            let mut script = format!("# retinue:click\nexport DISPLAY={display}\nxdotool mousemove --sync {x} {y}\n");
            if double {
                script.push_str(&format!("xdotool click --repeat 2 --delay 90 {button}\n"));
            } else {
                script.push_str(&format!("xdotool click {button}\n"));
            }
            run_script(harbor, &script, ACTION_TIMEOUT).await?;
            let with = button_name(button);
            if double {
                Ok((format!("Clicked twice at {x}, {y} with the {with} button."), "Clicked twice in the bot computer.".to_owned()))
            } else {
                Ok((format!("Clicked at {x}, {y} with the {with} button."), "Clicked in the bot computer.".to_owned()))
            }
        }
        "computer_scroll" => {
            let (x, y) = position.unwrap_or((0, 0));
            let (direction, button, amount) = scroll.ok_or("direction is required.")?;
            let script = format!("# retinue:scroll\nexport DISPLAY={display}\nxdotool mousemove --sync {x} {y}\nxdotool click --repeat {amount} --delay 40 {button}\n");
            run_script(harbor, &script, ACTION_TIMEOUT).await?;
            Ok((format!("Scrolled {direction} {amount} steps at {x}, {y}."), format!("Scrolled {direction} in the bot computer.")))
        }
        "computer_type" => {
            let (text, count) = typed.ok_or("text is required.")?;
            // The text travels as base64 and appears nowhere else: not in the
            // command, not in the reply, not in the activity row, not in a log.
            let encoded = base64::engine::general_purpose::STANDARD.encode(text.as_bytes());
            let script = format!("# retinue:type\nexport DISPLAY={display}\nprintf '%s' '{encoded}' | base64 -d | xdotool type --clearmodifiers --delay 12 --file -\n");
            run_script(harbor, &script, ACTION_TIMEOUT).await?;
            Ok((format!("Typed {count} characters."), format!("Typed {count} characters in the bot computer.")))
        }
        "computer_key" => {
            let keys = keys.ok_or("keys is required.")?;
            let script = format!("# retinue:key\nexport DISPLAY={display}\nxdotool key --clearmodifiers {keys}\n");
            run_script(harbor, &script, ACTION_TIMEOUT).await?;
            Ok((format!("Pressed {keys}."), format!("Pressed {keys} in the bot computer.")))
        }
        _ => {
            // One screenshot per call, downscaled before it is carried back, and
            // kept as a media record: never inline bytes in the transcript.
            let file = format!("/tmp/retinue-shot-{}.png", Uuid::new_v4());
            let script = format!(r#"# retinue:screenshot
export DISPLAY={display}
f={file}
if command -v scrot >/dev/null 2>&1; then scrot -o -q 90 "$f"
elif command -v gnome-screenshot >/dev/null 2>&1; then gnome-screenshot -f "$f"
else import -window root "$f"
fi
[ -s "$f" ] || {{ echo "The screenshot came back empty."; exit 1; }}
if command -v convert >/dev/null 2>&1; then convert "$f" -resize '{SCREENSHOT_MAX_WIDTH}x>' "$f" 2>/dev/null || true; fi
echo "RETINUE_BYTES:$(wc -c <"$f" | tr -d ' ')"
echo "RETINUE_PNG:$(base64 "$f" | tr -d '\n')"
rm -f "$f"
"#);
            let output = run_script(harbor, &script, ACTION_TIMEOUT).await?;
            let png = screenshot_bytes(&output)?;
            let media_id = crate::media::store_screenshot(st, session_id, &png).await?;
            let caption = "Screenshot of the bot computer";
            Ok((
                format!("{caption} ({} by {} pixels, kept at most {SCREENSHOT_MAX_WIDTH} pixels wide).\nmediaId: {media_id}\nCaption: {caption}", probe.width, probe.height),
                "Took a screenshot of the bot computer.".to_owned(),
            ))
        }
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
// The computer as an employee's workplace
//
// An employee with `computerWorkplace` on works on the owner's computer rather
// than only reaching for it now and then: its conversations carry a short
// section saying so, and a sleeping computer is woken before the conversation's
// first turn. The switch alone does nothing; the computer must be linked and
// switched on, and the tools keep the same gates as every other conversation
// (`call_tool`): the conversation's approval, and the unattended refusal.

/// How long a conversation's first turn may spend waking the computer. The
/// prompt goes out either way when it runs out.
const WAKE_BUDGET: Duration = Duration::from_secs(20);

/// Whether this employee treats the computer as its workplace: its own switch.
pub(crate) fn workplace_on(agent: &Value) -> bool {
    agent["computerWorkplace"].as_bool() == Some(true)
}

/// The instructions section for a workplace employee, or `None` when the switch
/// is off or no computer is linked and switched on. Names the computer when a
/// status read has already told us its name; never its address or its token.
///
/// Kept to two lines: the detail (folders, logs, routines, waking and sleeping)
/// is the built-in `cloud-computer` skill, listed for this employee only while
/// the section is. Changing this text still changes the instruction hash, so it
/// carries only what must be true before the bot reads the skill.
pub(crate) fn workplace_instructions(st: &AppState, agent: &Value) -> Option<String> {
    if !workplace_on(agent) { return None; }
    enabled_link(st)?;
    let opening = match last_name(st) {
        Some(name) => format!("The owner's computer, {name}, is your workplace, not a tool you may get around to using."),
        None => "The owner's computer is your workplace, not a tool you may get around to using.".to_owned(),
    };
    Some([
        "## Your workplace",
        &opening,
        "Run the work itself there with computer_exec, under /config/work/<project>, not in the files on this machine.",
        "Load the skill \"cloud-computer\" before your first command there: it covers where work goes, logs, routines, waking and sleeping, and how work gets back into the owner's project.",
    ].join("\n"))
}

/// Wakes a sleeping computer before `sid`'s first turn. One attempt per
/// conversation, inside its budget, and never a reason to stop the turn. One
/// content-free line says what happened; nothing here names the conversation,
/// the address or the token.
pub(crate) async fn before_first_turn(st: &AppState, sid: &str) {
    before_first_turn_within(st, sid, WAKE_BUDGET).await
}

/// The same, with the budget named: the tests use a short one to prove an
/// unanswered server cannot hold a turn.
pub(crate) async fn before_first_turn_within(st: &AppState, sid: &str, budget: Duration) {
    let Ok(brief) = st.db.session_brief(sid) else { return };
    let Some(agent) = brief["agentId"].as_str().and_then(|id| st.db.agent_get(id).ok()) else { return };
    if !workplace_on(&agent) { return; }
    // The claim is taken even when there is nothing to wake, so a conversation
    // wakes its computer once and never on a later turn.
    if !st.db.session_take_computer_wake(sid).unwrap_or(false) { return; }
    let Some(link) = enabled_link(st) else { return };
    match tokio::time::timeout(budget, wake_if_asleep(st, &link)).await {
        // Statements, not expressions: the daemon's log scan reads a call site up
        // to its `);`, and prose about the token must never fall inside it.
        Ok(Ok(true)) => { tracing::info!("computer workplace: woke the computer before its first turn"); }
        Ok(Ok(false)) => { tracing::debug!("computer workplace: nothing to wake"); }
        _ => { tracing::debug!("computer workplace: the wake did not finish, so the turn carried on"); }
    }
}

/// `true` when it asked Harbor to wake the computer. An unreachable server, a
/// refused token or a sleeping state that cannot be read are all `Err`, which
/// the caller ignores: the turn carries on.
async fn wake_if_asleep(st: &AppState, link: &ComputerLink) -> Result<bool, ()> {
    if read_status(st).await["state"] != "asleep" { return Ok(false); }
    let harbor = Harbor { address: link.address.clone(), token: token_of(link).map_err(|_| ())? };
    harbor.call(reqwest::Method::POST, "/action/wake", Some(json!({})), ACTION_TIMEOUT).await.map_err(|_| ())?;
    // The same forced announcement `computer.action wake` makes; a wake may still
    // be starting when the status is read.
    let status = read_status(st).await;
    publish(st, status, true).await;
    Ok(true)
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
        // A Windows `Instant` counts from boot, so a runner up for less than the
        // lifetime cannot backdate one; dropping the tickets is what expiry does.
        match std::time::Instant::now().checked_sub(VIEW_TTL + Duration::from_secs(1)) {
            Some(earlier) => for ticket in tickets.iter_mut() { ticket.last_used = earlier; },
            None => tickets.clear(),
        }
    }
}
