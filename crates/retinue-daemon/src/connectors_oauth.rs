//! MCP OAuth for connectors (report 03, idea 2; plan `nod-adoption-plan.md`, wave 3).
//!
//! An OAuth connector signs in once: the daemon discovers the service (RFC 9728
//! protected-resource metadata, then RFC 8414 authorization-server metadata),
//! registers itself when the service offers dynamic registration (RFC 7591),
//! makes a PKCE (S256) request with a random `state`, opens the owner's own
//! browser at the consent page, and waits for the callback on
//! `http://127.0.0.1:<ephemeral>/callback` — loopback only, one use, five
//! minutes, the `state` compared, then the listener is closed.
//!
//! The tokens live in Windows Credential Manager under `Retinue/connector/<id>`
//! as one JSON bundle, never in SQLite, an event, a reply or a log. A bundle
//! over the 2,048-byte credential cap is split into `connector/<id>.1`,
//! `connector/<id>.2` … and joined on read. The access token is refreshed when
//! it is within 60 seconds of expiry, and a removal revokes it (RFC 7009).
//!
//! Every `connector.oauth.*` method is `never` in `tiers.json`, and
//! `connector.oauth.changed` is `drop`: the consent page is a credential
//! grant, so only the desktop can start one.

use crate::{connectors, derr, AppState, DispatchError};
use axum::http::StatusCode;
use base64::Engine as _;
use chrono::{Duration as ChronoDuration, SecondsFormat, Utc};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::{Mutex as StdMutex, OnceLock};
use std::time::Duration;

const CALLBACK_PATH: &str = "/callback";
/// How long the listener waits for the browser.
const FLOW_LIFETIME: Duration = Duration::from_secs(300);
/// A token is refreshed this many seconds before it expires, so a call never
/// starts with a token that dies mid-flight.
const REFRESH_MARGIN_SECS: i64 = 60;
/// The credential store's cap for one generic credential.
const PART_MAX: usize = 2_048;
/// At most eight parts: 16 KiB of tokens, client information and endpoints.
const PARTS_MAX: usize = 8;
const DISCOVERY_MAX: usize = 256 * 1024;
const TOKEN_MAX: usize = 64 * 1024;
const REQUEST_LINE_MAX: usize = 8 * 1024;

fn invalid(message: &str) -> DispatchError {
    derr("invalid_argument", message, StatusCode::BAD_REQUEST)
}

fn unavailable(message: &str) -> DispatchError {
    derr("unavailable", message, StatusCode::SERVICE_UNAVAILABLE)
}

// ---------------------------------------------------------------------------
// The bundle and the chunked credential store

/// Everything one signed-in connector needs after the flow, except the tokens
/// themselves never leave this struct's JSON form on disk.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
struct Bundle {
    #[serde(default)]
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    token_type: Option<String>,
    #[serde(default)]
    scope: Option<String>,
    /// Unix seconds. Absent when the service did not say.
    #[serde(default)]
    expires_at: Option<i64>,
    #[serde(default)]
    client_id: String,
    #[serde(default)]
    client_secret: Option<String>,
    #[serde(default)]
    token_endpoint_auth_method: Option<String>,
    #[serde(default)]
    token_endpoint: String,
    #[serde(default)]
    revocation_endpoint: Option<String>,
    #[serde(default)]
    issuer: Option<String>,
    #[serde(default)]
    resource: String,
}

fn part_name(id: &str, part: usize) -> String {
    if part == 0 {
        format!("connector/{id}")
    } else {
        format!("connector/{id}.{part}")
    }
}

/// Splits on character boundaries, so a multi-byte character is never cut.
fn parts_of(value: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    for character in value.chars() {
        if current.len() + character.len_utf8() > PART_MAX {
            parts.push(std::mem::take(&mut current));
        }
        current.push(character);
    }
    if !current.is_empty() {
        parts.push(current);
    }
    parts
}

/// The value at `Retinue/connector/<id>`, joined from its numbered parts. A
/// single part is the common case; a bundle over the 2,048-byte cap continues
/// at `.1`, `.2` until a part is missing.
pub(crate) fn read_value(id: &str) -> Result<Option<String>, String> {
    let Some(first) = crate::secrets::store::read(&part_name(id, 0))? else {
        return Ok(None);
    };
    let mut text = first.as_str().to_owned();
    for part in 1..PARTS_MAX {
        match crate::secrets::store::read(&part_name(id, part))? {
            Some(chunk) => text.push_str(chunk.as_str()),
            None => break,
        }
    }
    Ok(Some(text))
}

pub(crate) fn write_value(id: &str, value: &str) -> Result<(), String> {
    let parts = parts_of(value);
    if parts.is_empty() {
        return Err("The sign-in is empty.".to_owned());
    }
    if parts.len() > PARTS_MAX {
        return Err("The sign-in is larger than Retinue keeps for one connector.".to_owned());
    }
    delete_value(id);
    for (index, part) in parts.iter().enumerate() {
        crate::secrets::store::write(&part_name(id, index), part)?;
    }
    Ok(())
}

/// Removes every part, whether the value was split or not.
pub(crate) fn delete_value(id: &str) {
    for part in 0..PARTS_MAX {
        let _ = crate::secrets::store::delete(&part_name(id, part));
    }
}

/// The saved bundle, or `None` when this connector has no sign-in. A value that
/// is not a bundle (a pasted token, say) counts as no sign-in and is replaced by
/// the next flow.
fn bundle(id: &str) -> Result<Option<Bundle>, String> {
    let Some(text) = read_value(id)? else {
        return Ok(None);
    };
    Ok(serde_json::from_str::<Bundle>(&text).ok().filter(|bundle| !bundle.access_token.is_empty()))
}

fn write_bundle(id: &str, bundle: &Bundle) -> Result<(), String> {
    let text = serde_json::to_string(bundle).map_err(|_| "The sign-in could not be saved.".to_owned())?;
    write_value(id, &text)
}

pub(crate) fn is_signed_in(id: &str) -> bool {
    matches!(bundle(id), Ok(Some(_)))
}

// ---------------------------------------------------------------------------
// The flow's live state

struct Flow {
    /// A new id per attempt, so a task from an abandoned attempt can never
    /// overwrite the state of the one that replaced it.
    flow_id: String,
    status: &'static str,
    detail: Option<String>,
    expires_at: Option<String>,
    /// The waiting task. Aborting it drops the listener, which closes it.
    task: Option<tokio::task::JoinHandle<()>>,
}

/// The state of an attempt. Memory only: a restart ends a pending flow, and the
/// listener is closed with it.
fn flows() -> &'static StdMutex<HashMap<String, Flow>> {
    static FLOWS: OnceLock<StdMutex<HashMap<String, Flow>>> = OnceLock::new();
    FLOWS.get_or_init(Default::default)
}

/// One credential write at a time across every connector: a refresh never
/// races another refresh for the same refresh token.
fn refresh_gate() -> &'static tokio::sync::Mutex<()> {
    static GATE: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    GATE.get_or_init(|| tokio::sync::Mutex::new(()))
}

fn status_event(id: &str, auth: &str, status: &str, detail: Option<&str>, expires_at: Option<&str>) -> Value {
    json!({"id": id, "auth": auth, "status": status, "detail": detail, "expiresAt": expires_at})
}

async fn announce(st: &AppState, id: &str, status: &str, detail: Option<&str>, expires_at: Option<&str>) {
    let auth = st.db.connector_get(id).ok().flatten().and_then(|row| row["auth"].as_str().map(str::to_owned)).unwrap_or_else(|| "oauth".to_owned());
    st.emit("connector.oauth.changed", status_event(id, &auth, status, detail, expires_at)).await;
}

/// Records the outcome of the attempt named by `flow_id`. A late task from an
/// abandoned attempt changes nothing.
async fn settle(st: &AppState, id: &str, flow_id: &str, status: &'static str, detail: Option<String>) {
    let current = {
        let mut map = flows().lock().unwrap_or_else(|poison| poison.into_inner());
        match map.get_mut(id) {
            Some(flow) if flow.flow_id == flow_id => {
                flow.status = status;
                flow.detail = detail.clone();
                if status != "pending" {
                    flow.task = None;
                }
                flow.expires_at.clone()
            }
            _ => return,
        }
    };
    let expires = if status == "pending" { current.as_deref() } else { None };
    announce(st, id, status, detail.as_deref(), expires).await;
}

/// The status the screen shows: a live attempt first, then the saved sign-in.
fn current_status(_st: &AppState, id: &str, auth: &str) -> Value {
    let flow = flows().lock().unwrap_or_else(|poison| poison.into_inner()).get(id).map(|flow| (flow.status, flow.detail.clone(), flow.expires_at.clone()));
    if let Some((status, detail, expires_at)) = flow {
        if status == "pending" || status == "error" {
            let expires = if status == "pending" { expires_at.as_deref() } else { None };
            return status_event(id, auth, status, detail.as_deref(), expires);
        }
    }
    match bundle(id) {
        Ok(Some(_)) => status_event(id, auth, "authorized", None, None),
        _ => status_event(id, auth, "idle", None, None),
    }
}

// ---------------------------------------------------------------------------
// HTTP, with the connector module's outbound rules (HTTPS or loopback, DNS
// pinned to the address that was checked, no cross-origin redirect, a cap)

struct Answer {
    status: reqwest::StatusCode,
    body: Vec<u8>,
    www_authenticate: Option<String>,
}

enum Body<'a> {
    None,
    Json(&'a Value),
}

async fn request(method: reqwest::Method, raw_url: &str, body: Body<'_>, bearer: Option<&str>) -> Result<Answer, String> {
    let endpoint = connectors::parse_url(raw_url)?;
    let address = connectors::resolve(&endpoint).await?;
    let http = connectors::build_client(&endpoint, address)?;
    let mut builder = http
        .request(method, endpoint.url.clone())
        .header("Accept", "application/json")
        .header("User-Agent", format!("Retinue/{}", env!("CARGO_PKG_VERSION")));
    builder = match body {
        Body::None => builder,
        Body::Json(value) => builder.header("Content-Type", "application/json").body(value.to_string()),
    };
    if let Some(token) = bearer {
        builder = builder.bearer_auth(token);
    }
    let response = builder.send().await.map_err(|error| {
        if error.is_timeout() {
            "The sign-in service did not answer in time.".to_owned()
        } else if error.is_connect() {
            "Retinue could not reach the sign-in service's address.".to_owned()
        } else {
            "The sign-in service's connection failed.".to_owned()
        }
    })?;
    let status = response.status();
    let www_authenticate = header(&response, "www-authenticate");
    let body = read_capped(response, DISCOVERY_MAX.max(TOKEN_MAX)).await?;
    Ok(Answer { status, body, www_authenticate })
}

fn header(response: &reqwest::Response, name: &str) -> Option<String> {
    response.headers().get(name).and_then(|value| value.to_str().ok()).map(str::to_owned)
}

async fn read_capped(mut response: reqwest::Response, max: usize) -> Result<Vec<u8>, String> {
    if let Some(length) = response.content_length() {
        if length > max as u64 {
            return Err("The sign-in service's answer was larger than Retinue reads.".to_owned());
        }
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| "The sign-in service's answer stopped partway through.".to_owned())? {
        if body.len() + chunk.len() > max {
            return Err("The sign-in service's answer was larger than Retinue reads.".to_owned());
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

async fn get_json(url: &str) -> Result<Value, String> {
    let answer = request(reqwest::Method::GET, url, Body::None, None).await?;
    if !answer.status.is_success() {
        return Err(format!("The sign-in service answered HTTP {}.", answer.status.as_u16()));
    }
    serde_json::from_slice(&answer.body).map_err(|_| "The sign-in service's answer was not JSON.".to_owned())
}

/// The short reason a service gives, and never its raw body: a token endpoint
/// can echo more than the owner should see.
fn service_reason(answer: &Answer) -> String {
    let parsed = serde_json::from_slice::<Value>(&answer.body).ok();
    let code = parsed.as_ref().and_then(|value| value["error"].as_str()).filter(|text| !text.is_empty() && text.len() <= 64 && text.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')).map(str::to_owned);
    match code {
        Some(code) => format!("The sign-in service refused the request ({code})."),
        None => format!("The sign-in service refused the request (HTTP {}).", answer.status.as_u16()),
    }
}

// ---------------------------------------------------------------------------
// Discovery (RFC 9728 then RFC 8414) and dynamic registration (RFC 7591)

struct Service {
    authorization_endpoint: String,
    token_endpoint: String,
    registration_endpoint: Option<String>,
    revocation_endpoint: Option<String>,
    scopes: Option<String>,
    issuer: String,
}

fn origin_of(url: &reqwest::Url) -> String {
    url.origin().ascii_serialization()
}

/// The `resource_metadata` parameter of a `WWW-Authenticate: Bearer` challenge.
fn resource_metadata(header: &str) -> Option<String> {
    let at = header.to_ascii_lowercase().find("resource_metadata=")?;
    let rest = &header[at + "resource_metadata=".len()..];
    if let Some(quoted) = rest.strip_prefix('"') {
        return quoted.split('"').next().map(str::to_owned).filter(|value| !value.is_empty());
    }
    rest.split([',', ' ']).next().map(str::trim).filter(|value| !value.is_empty()).map(str::to_owned)
}

/// An unauthenticated `initialize`: a `401` may name the protected-resource
/// metadata document in its challenge. A transport failure is the caller's
/// answer ("Retinue could not reach the connector's address").
async fn probe(resource: &str) -> Result<Option<String>, String> {
    let body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": {"name": "retinue", "version": env!("CARGO_PKG_VERSION")}
        }
    });
    let answer = request(reqwest::Method::POST, resource, Body::Json(&body), None).await?;
    if answer.status == StatusCode::UNAUTHORIZED {
        return Ok(answer.www_authenticate.as_deref().and_then(resource_metadata));
    }
    Ok(None)
}

fn checked_address(endpoint: &str) -> Result<String, String> {
    connectors::parse_url(endpoint).map(|parsed| parsed.url.to_string()).map_err(|_| "The sign-in service named an address Retinue will not use.".to_owned())
}

async fn discover(resource: &str) -> Result<Service, String> {
    let parsed = connectors::parse_url(resource)?;
    let origin = origin_of(&parsed.url);
    let path = parsed.url.path().trim_end_matches('/').to_owned();

    let mut metadata = match probe(resource).await {
        Ok(Some(named)) => await_json(named).await,
        Ok(None) => None,
        Err(message) => return Err(message),
    };
    if metadata.is_none() {
        let mut candidates = vec![format!("{origin}/.well-known/oauth-protected-resource")];
        if !path.is_empty() {
            candidates.push(format!("{origin}/.well-known/oauth-protected-resource{path}"));
        }
        for candidate in candidates {
            if let Some(value) = await_json(candidate).await {
                metadata = Some(value);
                break;
            }
        }
    }
    let metadata = metadata.ok_or_else(|| "This server does not say how to sign in (no OAuth metadata was found).".to_owned())?;

    let issuer = metadata["authorization_servers"]
        .as_array()
        .and_then(|list| list.first())
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| origin.clone());

    let mut authorization = None;
    let issuer_trimmed = issuer.trim_end_matches('/');
    let issuer_url = reqwest::Url::parse(&issuer).ok();
    let issuer_path = issuer_url.as_ref().map(|url| url.path().trim_end_matches('/').to_owned()).unwrap_or_default();
    let issuer_origin = issuer_url.as_ref().map(origin_of).filter(|origin| !origin.is_empty()).unwrap_or_else(|| issuer_trimmed.to_owned());
    // RFC 8414 inserts the well-known segment before the issuer's own path.
    let mut candidates = vec![format!("{issuer_origin}/.well-known/oauth-authorization-server")];
    if !issuer_path.is_empty() {
        candidates.push(format!("{issuer_origin}/.well-known/oauth-authorization-server{issuer_path}"));
    }
    candidates.push(format!("{issuer_trimmed}/.well-known/openid-configuration"));
    for candidate in candidates {
        if let Some(value) = await_json(candidate).await {
            if value["authorization_endpoint"].as_str().is_some() && value["token_endpoint"].as_str().is_some() {
                authorization = Some(value);
                break;
            }
        }
    }
    let authorization = authorization.ok_or_else(|| "This server's sign-in service did not give its addresses.".to_owned())?;

    if let Some(methods) = authorization["code_challenge_methods_supported"].as_array() {
        let supports_s256 = methods.iter().any(|method| method.as_str() == Some("S256"));
        if !supports_s256 {
            return Err("This sign-in service does not support the protected flow Retinue needs (S256).".to_owned());
        }
    }

    let authorization_endpoint = checked_address(authorization["authorization_endpoint"].as_str().unwrap_or(""))?;
    let token_endpoint = checked_address(authorization["token_endpoint"].as_str().unwrap_or(""))?;
    let registration_endpoint = authorization["registration_endpoint"].as_str().map(checked_address).transpose()?;
    let revocation_endpoint = authorization["revocation_endpoint"].as_str().map(checked_address).transpose()?;

    let scopes = metadata["scopes_supported"]
        .as_array()
        .or_else(|| authorization["scopes_supported"].as_array())
        .map(|list| list.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(" "))
        .filter(|text| !text.is_empty() && text.len() <= 512);

    Ok(Service { authorization_endpoint, token_endpoint, registration_endpoint, revocation_endpoint, scopes, issuer })
}

/// One discovery document, or `None` when the address does not answer with one.
/// The fall-backs are ordinary (a server need not publish every document).
async fn await_json(url: String) -> Option<Value> {
    get_json(&url).await.ok()
}

// ---------------------------------------------------------------------------
// PKCE and the random state

fn random_token() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn challenge_for(verifier: &str) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

/// Appends `name=value` pairs to a URL that may already carry a query.
fn with_query(base: &str, pairs: &[(&str, &str)]) -> String {
    let mut out = base.to_owned();
    let mut separator = if out.contains('?') { '&' } else { '?' };
    for (name, value) in pairs {
        out.push(separator);
        out.push_str(&connectors::percent_encode(name, false));
        out.push('=');
        out.push_str(&connectors::percent_encode(value, false));
        separator = '&';
    }
    out
}

fn form_body(pairs: &[(&str, String)]) -> String {
    pairs.iter().map(|(name, value)| format!("{}={}", connectors::percent_encode(name, false), connectors::percent_encode(value, false))).collect::<Vec<_>>().join("&")
}

// ---------------------------------------------------------------------------
// The owner's browser

#[cfg(test)]
fn opened_urls() -> &'static StdMutex<Vec<String>> {
    static OPENED: OnceLock<StdMutex<Vec<String>>> = OnceLock::new();
    OPENED.get_or_init(Default::default)
}

/// Test builds record the address instead of opening a browser on this PC.
#[cfg(test)]
pub(crate) fn take_opened_urls() -> Vec<String> {
    std::mem::take(&mut opened_urls().lock().unwrap_or_else(|poison| poison.into_inner()))
}

#[cfg(test)]
fn open_in_browser(url: &str) -> Result<(), String> {
    opened_urls().lock().unwrap_or_else(|poison| poison.into_inner()).push(url.to_owned());
    Ok(())
}

#[cfg(all(not(test), windows))]
fn open_in_browser(url: &str) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    let operation: Vec<u16> = "open\0".encode_utf16().collect();
    let file: Vec<u16> = std::ffi::OsStr::new(url).encode_wide().chain(Some(0)).collect();
    // SAFETY: both pointers are null-terminated wide strings that outlive the call.
    let result = unsafe { ShellExecuteW(std::ptr::null_mut(), operation.as_ptr(), file.as_ptr(), std::ptr::null(), std::ptr::null(), 1) };
    if (result as isize) > 32 {
        Ok(())
    } else {
        Err("The browser could not be opened.".to_owned())
    }
}

#[cfg(all(not(test), not(windows), target_os = "linux"))]
fn open_in_browser(url: &str) -> Result<(), String> {
    // The address comes from a connector's own discovery document, so only an
    // HTTPS address (or the loopback callback) is ever handed to the desktop
    // opener: `file://` and `mailto:` are not something a connector may open.
    if !(url.starts_with("https://") || url.starts_with("http://127.0.0.1:") || url.starts_with("http://localhost:")) {
        return Err("The browser could not be opened.".to_owned());
    }
    std::process::Command::new("xdg-open")
        .arg(url)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|_| "The browser could not be opened. Install xdg-utils so `xdg-open` is on PATH.".to_owned())
}

#[cfg(all(not(test), not(windows), not(target_os = "linux")))]
fn open_in_browser(_: &str) -> Result<(), String> {
    Err("Opening the owner's browser is not available on this computer.".to_owned())
}

// ---------------------------------------------------------------------------
// The loopback callback

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'+' => {
                out.push(b' ');
                index += 1;
            }
            b'%' if index + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).ok().and_then(|hex| u8::from_str_radix(hex, 16).ok());
                match hex {
                    Some(byte) => {
                        out.push(byte);
                        index += 3;
                    }
                    None => {
                        out.push(bytes[index]);
                        index += 1;
                    }
                }
            }
            byte => {
                out.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn query_param(query: &str, name: &str) -> Option<String> {
    for pair in query.split('&') {
        let (key, value) = match pair.split_once('=') {
            Some((key, value)) => (key, value),
            None => (pair, ""),
        };
        if percent_decode(key) == name {
            return Some(percent_decode(value));
        }
    }
    None
}

async fn read_request_line(stream: &mut tokio::net::TcpStream) -> Option<String> {
    use tokio::io::AsyncReadExt;
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 1024];
    loop {
        let read = match tokio::time::timeout(Duration::from_secs(10), stream.read(&mut chunk)).await {
            Ok(Ok(read)) if read > 0 => read,
            _ => return None,
        };
        buffer.extend_from_slice(&chunk[..read]);
        if buffer.windows(4).any(|window| window == b"\r\n\r\n") || buffer.len() >= REQUEST_LINE_MAX {
            break;
        }
    }
    let text = String::from_utf8_lossy(&buffer);
    text.lines().next().map(str::to_owned)
}

async fn respond(stream: &mut tokio::net::TcpStream, status: &str, body: &str) {
    use tokio::io::AsyncWriteExt;
    let answer = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(answer.as_bytes()).await;
    let _ = stream.flush().await;
    let _ = stream.shutdown().await;
}

const PAGE_SIGNED_IN: &str = "<!doctype html><meta charset=\"utf-8\"><title>Retinue</title><p>You are signed in. You can close this tab and return to Retinue.";
const PAGE_REFUSED: &str = "<!doctype html><meta charset=\"utf-8\"><title>Retinue</title><p>This sign-in could not be completed. Return to Retinue to try again.";

/// Waits for one callback on the bound listener: one use, the `state` compared,
/// then the listener is closed when this task ends.
#[allow(clippy::too_many_arguments)]
async fn wait_for_callback(st: AppState, id: String, flow_id: String, listener: tokio::net::TcpListener, expected_state: String, verifier: String, redirect_uri: String, client: Client, service: Service, resource: String, scope: Option<String>) {
    let deadline = tokio::time::Instant::now() + FLOW_LIFETIME;
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            settle(&st, &id, &flow_id, "error", Some("The sign-in was not completed in time. Try again.".to_owned())).await;
            return;
        }
        let accepted = tokio::time::timeout(remaining, listener.accept()).await;
        let Ok(Ok((mut stream, _))) = accepted else {
            settle(&st, &id, &flow_id, "error", Some("The sign-in was not completed in time. Try again.".to_owned())).await;
            return;
        };
        // Only one request per connection, and only from this machine: the
        // listener is bound to 127.0.0.1.
        let Some(line) = read_request_line(&mut stream).await else { continue };
        let mut pieces = line.split_whitespace();
        let method = pieces.next().unwrap_or("");
        let target = pieces.next().unwrap_or("");
        let (path, query) = match target.split_once('?') {
            Some((path, query)) => (path, query),
            None => (target, ""),
        };
        if method != "GET" || path != CALLBACK_PATH {
            respond(&mut stream, "404 Not Found", PAGE_REFUSED).await;
            continue;
        }
        if query_param(query, "state").as_deref() != Some(expected_state.as_str()) {
            // A wrong or missing state grants nothing, and the real answer may
            // still arrive: answer no and keep waiting.
            respond(&mut stream, "400 Bad Request", PAGE_REFUSED).await;
            continue;
        }
        if query_param(query, "error").is_some() {
            respond(&mut stream, "200 OK", PAGE_REFUSED).await;
            settle(&st, &id, &flow_id, "error", Some("The sign-in was cancelled at the service.".to_owned())).await;
            return;
        }
        let Some(code) = query_param(query, "code").filter(|code| !code.is_empty() && code.len() <= 4_096) else {
            respond(&mut stream, "400 Bad Request", PAGE_REFUSED).await;
            continue;
        };
        // Answer the browser before the exchange, so the tab never waits on it.
        respond(&mut stream, "200 OK", PAGE_SIGNED_IN).await;
        drop(stream);
    match exchange(&service, &client, &redirect_uri, &verifier, &code, &resource, scope.as_deref()).await {
            Ok(bundle) => {
                if write_bundle(&id, &bundle).is_err() {
                    settle(&st, &id, &flow_id, "error", Some("The sign-in could not be saved.".to_owned())).await;
                } else {
                    if let Ok(Some(row)) = st.db.connector_get(&id) {
                        if row["auth"].as_str() != Some("oauth") {
                            let _ = st.db.connector_update(&id, None, Some("oauth"), None, None);
                            if let Ok(Some(row)) = st.db.connector_get(&id) {
                                st.emit("connector.changed", row).await;
                            }
                        }
                    }
                    settle(&st, &id, &flow_id, "authorized", None).await;
                }
            }
            Err(message) => settle(&st, &id, &flow_id, "error", Some(message)).await,
        }
        return;
    }
}

// ---------------------------------------------------------------------------
// Registration, exchange, refresh and revoke

struct Client {
    id: String,
    secret: Option<String>,
    /// `none`, `client_secret_post` or `client_secret_basic`.
    method: String,
}

async fn register(service: &Service, redirect_uri: &str, scope: Option<&str>) -> Result<Client, String> {
    let endpoint = service.registration_endpoint.as_deref().ok_or("This server does not offer automatic registration, so Retinue cannot sign in to it.")?;
    let mut body = json!({
        "client_name": "Retinue",
        "redirect_uris": [redirect_uri],
        "grant_types": ["authorization_code", "refresh_token"],
        "response_types": ["code"],
        "token_endpoint_auth_method": "none"
    });
    if let Some(scope) = scope {
        body["scope"] = json!(scope);
    }
    let answer = request(reqwest::Method::POST, endpoint, Body::Json(&body), None).await?;
    if !answer.status.is_success() {
        return Err(service_reason(&answer));
    }
    let value: Value = serde_json::from_slice(&answer.body).map_err(|_| "The sign-in service's registration answer was not JSON.".to_owned())?;
    let id = value["client_id"].as_str().filter(|text| !text.is_empty() && text.len() <= 512).ok_or("The sign-in service did not register Retinue.".to_owned())?.to_owned();
    let secret = value["client_secret"].as_str().filter(|text| !text.is_empty() && text.len() <= 1_024).map(str::to_owned);
    let method = value["token_endpoint_auth_method"].as_str().filter(|text| matches!(*text, "none" | "client_secret_post" | "client_secret_basic")).unwrap_or("none").to_owned();
    Ok(Client { id, secret, method })
}

/// The token request's authentication: none, a body pair, or Basic.
fn token_auth(client: &Client, pairs: &mut Vec<(&'static str, String)>, basic: &mut Option<String>) {
    match client.method.as_str() {
        "client_secret_post" => {
            pairs.push(("client_id", client.id.clone()));
            if let Some(secret) = client.secret.as_deref() {
                pairs.push(("client_secret", secret.to_owned()));
            }
        }
        "client_secret_basic" => {
            if let Some(secret) = client.secret.as_deref() {
                *basic = Some(base64::engine::general_purpose::STANDARD.encode(format!("{}:{secret}", client.id)));
            }
        }
        _ => pairs.push(("client_id", client.id.clone())),
    }
}

async fn token_call(endpoint: &str, pairs: Vec<(&str, String)>, basic: Option<String>) -> Result<Value, String> {
    let form = form_body(&pairs);
    let answer = request_with_authorization(reqwest::Method::POST, endpoint, form, basic).await?;
    if !answer.status.is_success() {
        return Err(service_reason(&answer));
    }
    serde_json::from_slice(&answer.body).map_err(|_| "The sign-in service's token answer was not JSON.".to_owned())
}

/// A form POST whose `Authorization` header is Basic rather than Bearer.
async fn request_with_authorization(method: reqwest::Method, raw_url: &str, form: String, basic: Option<String>) -> Result<Answer, String> {
    let endpoint = connectors::parse_url(raw_url)?;
    let address = connectors::resolve(&endpoint).await?;
    let http = connectors::build_client(&endpoint, address)?;
    let mut builder = http
        .request(method, endpoint.url.clone())
        .header("Accept", "application/json")
        .header("Content-Type", "application/x-www-form-urlencoded")
        .header("User-Agent", format!("Retinue/{}", env!("CARGO_PKG_VERSION")))
        .body(form);
    if let Some(basic) = basic {
        builder = builder.header("Authorization", format!("Basic {basic}"));
    }
    let response = builder.send().await.map_err(|error| {
        if error.is_timeout() {
            "The sign-in service did not answer in time.".to_owned()
        } else if error.is_connect() {
            "Retinue could not reach the sign-in service's address.".to_owned()
        } else {
            "The sign-in service's connection failed.".to_owned()
        }
    })?;
    let status = response.status();
    let www_authenticate = header(&response, "www-authenticate");
    let body = read_capped(response, TOKEN_MAX).await?;
    Ok(Answer { status, body, www_authenticate })
}

fn expiry_from(value: &Value) -> Option<i64> {
    let seconds = value["expires_in"].as_i64().or_else(|| value["expires_in"].as_str().and_then(|text| text.parse::<i64>().ok()))?;
    if seconds < 0 {
        return None;
    }
    Some(Utc::now().timestamp() + seconds)
}

#[allow(clippy::too_many_arguments)]
async fn exchange(service: &Service, client: &Client, redirect_uri: &str, verifier: &str, code: &str, resource: &str, scope: Option<&str>) -> Result<Bundle, String> {
    let mut pairs: Vec<(&str, String)> = vec![
        ("grant_type", "authorization_code".to_owned()),
        ("code", code.to_owned()),
        ("redirect_uri", redirect_uri.to_owned()),
        ("code_verifier", verifier.to_owned()),
        ("resource", resource.to_owned()),
    ];
    let mut basic = None;
    token_auth(client, &mut pairs, &mut basic);
    if let Some(scope) = scope {
        pairs.push(("scope", scope.to_owned()));
    }
    let value = token_call(&service.token_endpoint, pairs, basic).await?;
    let access_token = value["access_token"].as_str().filter(|text| !text.is_empty() && text.len() <= 16_384).ok_or("The sign-in service did not return an access token.".to_owned())?.to_owned();
    let refresh_token = value["refresh_token"].as_str().filter(|text| !text.is_empty() && text.len() <= 16_384).map(str::to_owned);
    Ok(Bundle {
        access_token,
        refresh_token,
        token_type: value["token_type"].as_str().map(str::to_owned),
        scope: value["scope"].as_str().map(str::to_owned).or_else(|| scope.map(str::to_owned)),
        expires_at: expiry_from(&value),
        client_id: client.id.clone(),
        client_secret: client.secret.clone(),
        token_endpoint_auth_method: Some(client.method.clone()),
        token_endpoint: service.token_endpoint.clone(),
        revocation_endpoint: service.revocation_endpoint.clone(),
        issuer: Some(service.issuer.clone()),
        resource: resource.to_owned(),
    })
}

/// The access token for a connector, refreshed first when it is within a minute
/// of expiry. `None` means this connector has no sign-in at all.
pub(crate) async fn access_token(st: &AppState, id: &str) -> Result<Option<String>, String> {
    let Some(saved) = bundle(id)? else {
        return Ok(None);
    };
    let stale = saved.expires_at.is_some_and(|expires_at| Utc::now().timestamp() + REFRESH_MARGIN_SECS >= expires_at);
    if !stale {
        return Ok(Some(saved.access_token));
    }
    let Some(refresh_token) = saved.refresh_token.clone() else {
        return Ok(Some(saved.access_token));
    };
    // One refresh at a time, then re-read: another call may have done it.
    let _gate = refresh_gate().lock().await;
    let Some(current) = bundle(id)? else {
        return Ok(None);
    };
    let still_stale = current.expires_at.is_some_and(|expires_at| Utc::now().timestamp() + REFRESH_MARGIN_SECS >= expires_at);
    if !still_stale {
        return Ok(Some(current.access_token));
    }
    match refresh(id, &current, &refresh_token).await {
        Ok(token) => Ok(token),
        Err(message) => {
            // The saved sign-in cannot be used: say so wherever the owner looks,
            // until the next attempt replaces this state.
            let detail = connectors::redact_text(&message, None);
            flows().lock().unwrap_or_else(|poison| poison.into_inner()).insert(
                id.to_owned(),
                Flow { flow_id: uuid::Uuid::new_v4().to_string(), status: "error", detail: Some(detail.clone()), expires_at: None, task: None },
            );
            announce(st, id, "error", Some(&detail), None).await;
            Err(detail)
        }
    }
}

async fn refresh(id: &str, bundle: &Bundle, refresh_token: &str) -> Result<Option<String>, String> {
    let client = Client {
        id: bundle.client_id.clone(),
        secret: bundle.client_secret.clone(),
        method: bundle.token_endpoint_auth_method.clone().unwrap_or_else(|| "none".to_owned()),
    };
    let mut pairs: Vec<(&str, String)> = vec![("grant_type", "refresh_token".to_owned()), ("refresh_token", refresh_token.to_owned())];
    let mut basic = None;
    token_auth(&client, &mut pairs, &mut basic);
    if !bundle.resource.is_empty() {
        pairs.push(("resource", bundle.resource.clone()));
    }
    let value = match token_call(&bundle.token_endpoint, pairs, basic).await {
        Ok(value) => value,
        Err(message) => return Err(format!("The sign-in could not be renewed. Connect the connector again in Settings. {message}")),
    };
    let access_token = value["access_token"].as_str().filter(|text| !text.is_empty()).ok_or("The sign-in service did not return a renewed access token.".to_owned())?.to_owned();
    let mut updated = bundle.clone();
    updated.access_token = access_token.clone();
    if let Some(refresh_token) = value["refresh_token"].as_str().filter(|text| !text.is_empty()) {
        updated.refresh_token = Some(refresh_token.to_owned());
    }
    updated.expires_at = expiry_from(&value);
    if let Some(scope) = value["scope"].as_str() {
        updated.scope = Some(scope.to_owned());
    }
    write_bundle(id, &updated)?;
    Ok(Some(access_token))
}

/// The best-effort RFC 7009 revoke a removal sends. Failures are ignored: the
/// credentials are deleted either way.
pub(crate) async fn revoke(id: &str) {
    let Ok(Some(bundle)) = bundle(id) else { return };
    let Some(endpoint) = bundle.revocation_endpoint.as_deref().filter(|endpoint| !endpoint.is_empty()) else { return };
    let token = bundle.refresh_token.clone().unwrap_or_else(|| bundle.access_token.clone());
    if token.is_empty() {
        return;
    }
    let mut pairs: Vec<(&str, String)> = vec![("token", token), ("token_type_hint", "refresh_token".to_owned())];
    if !bundle.client_id.is_empty() {
        pairs.push(("client_id", bundle.client_id.clone()));
    }
    if let Some(secret) = bundle.client_secret.as_deref() {
        pairs.push(("client_secret", secret.to_owned()));
    }
    let form = form_body(&pairs);
    let _ = request_with_authorization(reqwest::Method::POST, endpoint, form, None).await;
}

// ---------------------------------------------------------------------------
// RPC

fn id_of(p: &Value) -> Result<String, DispatchError> {
    p["id"].as_str().filter(|id| !id.is_empty() && id.len() <= 64).map(str::to_owned).ok_or_else(|| invalid("id is required"))
}

fn row_for(st: &AppState, id: &str) -> Result<Value, DispatchError> {
    st.db.connector_get(id).map_err(|_| derr("internal", "the connector store failed", StatusCode::INTERNAL_SERVER_ERROR))?.ok_or_else(|| derr("not_found", "That connector no longer exists.", StatusCode::NOT_FOUND))
}

pub async fn rpc(st: &AppState, method: &str, p: &Value) -> Result<Value, DispatchError> {
    match method {
        "connector.oauth.start" => start(st, p).await,
        "connector.oauth.status" => {
            let id = id_of(p)?;
            let row = row_for(st, &id)?;
            let auth = row["auth"].as_str().unwrap_or("none");
            Ok(current_status(st, &id, auth))
        }
        "connector.oauth.cancel" => {
            let id = id_of(p)?;
            let _ = row_for(st, &id)?;
            let flow = flows().lock().unwrap_or_else(|poison| poison.into_inner()).remove(&id);
            match flow {
                Some(flow) if flow.status == "pending" => {
                    if let Some(task) = flow.task {
                        task.abort();
                        // Wait for the task to be dropped: the callback listener
                        // closes with it, before this reply says it has.
                        let _ = task.await;
                    }
                    announce(st, &id, "idle", None, None).await;
                    Ok(json!({"cancelled": true}))
                }
                _ => Ok(json!({"cancelled": false})),
            }
        }
        _ => Err(derr("not_found", "unknown connector sign-in method", StatusCode::NOT_FOUND)),
    }
}

async fn start(st: &AppState, p: &Value) -> Result<Value, DispatchError> {
    let id = id_of(p)?;
    let row = row_for(st, &id)?;
    let auth = row["auth"].as_str().unwrap_or("none").to_owned();
    if auth == "token" {
        return Err(derr("conflict", "This connector uses a saved token. Clear the token first.", StatusCode::CONFLICT));
    }
    {
        let map = flows().lock().unwrap_or_else(|poison| poison.into_inner());
        if map.get(&id).is_some_and(|flow| flow.status == "pending") {
            return Err(derr("conflict", "A sign-in is already waiting for this connector.", StatusCode::CONFLICT));
        }
    }

    let resource = row["url"].as_str().unwrap_or("").to_owned();
    // Bind the callback before discovery, so the port named in the request is
    // the port the daemon holds.
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .map_err(|_| unavailable("Retinue could not open its local sign-in listener."))?;
    let port = listener.local_addr().map_err(|_| unavailable("Retinue could not read its local sign-in listener's port."))?.port();
    let redirect_uri = format!("http://127.0.0.1:{port}{CALLBACK_PATH}");

    let service = match discover(&resource).await {
        Ok(service) => service,
        Err(message) => {
            let detail = connectors::redact_text(&message, None);
            flows().lock().unwrap_or_else(|poison| poison.into_inner()).insert(id.clone(), Flow { flow_id: String::new(), status: "error", detail: Some(detail.clone()), expires_at: None, task: None });
            announce(st, &id, "error", Some(&detail), None).await;
            return Err(unavailable(&detail));
        }
    };
    let scope = service.scopes.clone();
    let client = match register(&service, &redirect_uri, scope.as_deref()).await {
        Ok(client) => client,
        Err(message) => {
            let detail = connectors::redact_text(&message, None);
            flows().lock().unwrap_or_else(|poison| poison.into_inner()).insert(id.clone(), Flow { flow_id: String::new(), status: "error", detail: Some(detail.clone()), expires_at: None, task: None });
            announce(st, &id, "error", Some(&detail), None).await;
            let code = if detail.contains("automatic registration") { "invalid_argument" } else { "unavailable" };
            return Err(derr(code, &detail, if code == "invalid_argument" { StatusCode::BAD_REQUEST } else { StatusCode::SERVICE_UNAVAILABLE }));
        }
    };

    let verifier = random_token();
    let challenge = challenge_for(&verifier);
    let state = random_token();
    let mut pairs: Vec<(&str, &str)> = vec![
        ("response_type", "code"),
        ("client_id", client.id.as_str()),
        ("redirect_uri", redirect_uri.as_str()),
        ("state", state.as_str()),
        ("code_challenge", challenge.as_str()),
        ("code_challenge_method", "S256"),
        ("resource", resource.as_str()),
    ];
    if let Some(scope) = scope.as_deref() {
        pairs.push(("scope", scope));
    }
    let authorization_url = with_query(&service.authorization_endpoint, &pairs);
    if let Err(message) = open_in_browser(&authorization_url) {
        return Err(unavailable(&message));
    }

    let expires_at = (Utc::now() + ChronoDuration::from_std(FLOW_LIFETIME).unwrap_or_else(|_| ChronoDuration::seconds(300))).to_rfc3339_opts(SecondsFormat::Millis, true);
    let flow_id = uuid::Uuid::new_v4().to_string();
    {
        let mut map = flows().lock().unwrap_or_else(|poison| poison.into_inner());
        map.insert(id.clone(), Flow { flow_id: flow_id.clone(), status: "pending", detail: None, expires_at: Some(expires_at.clone()), task: None });
    }
    let task = tokio::spawn(wait_for_callback(st.clone(), id.clone(), flow_id.clone(), listener, state, verifier, redirect_uri, client, service, resource, scope));
    {
        let mut map = flows().lock().unwrap_or_else(|poison| poison.into_inner());
        if let Some(flow) = map.get_mut(&id) {
            if flow.flow_id == flow_id {
                flow.task = Some(task);
            }
        }
    }
    if auth != "oauth" {
        let _ = st.db.connector_update(&id, None, Some("oauth"), None, None);
        if let Ok(Some(row)) = st.db.connector_get(&id) {
            st.emit("connector.changed", row).await;
        }
    }
    announce(st, &id, "pending", None, Some(&expires_at)).await;
    Ok(json!({"status": "pending", "authorizationUrl": authorization_url, "expiresAt": expires_at}))
}

/// The plain sentence a check records when a connector has no sign-in yet.
pub(crate) const SIGN_IN_NEEDED: &str = "Sign in to use this connector.";

/// The plain sentence a check records when the saved sign-in cannot be used.
pub(crate) const SIGN_IN_EXPIRED: &str = "The sign-in for this connector is missing or has expired. Connect it again in Settings.";
