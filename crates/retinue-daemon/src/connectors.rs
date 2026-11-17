//! Section D of `docs/plans/nod-adoption-plan.md`: connectors.
//!
//! A connector is an MCP server the owner adds (Notion, Linear, GitHub and the
//! like). The daemon is the MCP client, over Streamable HTTP, so a bot never
//! holds the token: it lives in Credential Manager under
//! `Retinue/connector/<id>`, and every wire decision (which host, which tool,
//! which arguments) stays here beside `tiers.json` and the approval card.
//!
//! Outbound rules (report 03, ideas 6 and 7): HTTPS only, with plain HTTP
//! allowed solely for a server on this PC; DNS pinned to the address that was
//! validated; a cross-origin redirect is refused; the answer is capped; and the
//! connector's own token is masked in its plain, URL-encoded and base64 forms
//! wherever a result or an error is shown.
//!
//! `connector.*` is `never` in `tiers.json` and `connector.changed` is `drop`.
//! The bot's two tools are `connector_search` and `connector_call`; a call asks
//! the owner through the usual approval card unless the tool's standing rule is
//! `allow`, and `deny` refuses. An unattended run may call only a tool the
//! server marks read-only, and only while the connector's `unattended` switch is
//! `readOnly`.

use crate::{derr, AppState, DispatchError};
use axum::http::StatusCode;
use base64::Engine as _;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Mutex as StdMutex, OnceLock};
use std::time::Duration;
use tokio::sync::oneshot;
use uuid::Uuid;
use zeroize::Zeroizing;

const NAME_MAX: usize = 80;
const URL_MAX: usize = 2_048;
const TOOL_MAX: usize = 200;
const QUERY_MAX: usize = 200;
const SCHEMA_MAX: usize = 8_192;
const CALL_TEXT_MAX: usize = 24_000;
const DETAIL_MAX: usize = 500;
const RESPONSE_MAX: usize = 4 * 1024 * 1024;
const SEARCH_LIMIT: usize = 20;
const TOOLS_MAX: usize = 500;
const PAGES_MAX: usize = 20;
const PROTOCOL_VERSION: &str = "2025-06-18";
/// The team bridge gives up on a call after 60 seconds, so the card's wait stays under it.
const WAIT_SECS: u64 = 50;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const UNATTENDED_NOTE: &str = "This run is unattended, so it can only call tools the connector marks read-only, and only while the connector's unattended switch is Read-only.";

fn invalid(message: &str) -> DispatchError {
    derr("invalid_argument", message, StatusCode::BAD_REQUEST)
}

fn storage_error(_: retinue_storage::StorageError) -> DispatchError {
    derr("internal", "the connector store failed", StatusCode::INTERNAL_SERVER_ERROR)
}

// ---------------------------------------------------------------------------
// The short catalogue (report 03, idea 3)
//
// Each address was checked on 16 November 2026: every host answered an MCP
// `initialize` (401 where a token is required). `auth` says how the official
// server takes a credential; OAuth itself arrives in wave 3.

fn catalogue() -> Value {
    json!([
        {"id":"github","name":"GitHub","url":"https://api.githubcopilot.com/mcp/","docsUrl":"https://docs.github.com/copilot/customizing-copilot/extending-copilot-chat-with-mcp","auth":"token","tokenHint":"Paste a fine-grained personal access token with read-only scopes."},
        {"id":"linear","name":"Linear","url":"https://mcp.linear.app/mcp","docsUrl":"https://linear.app/docs/mcp","auth":"token","tokenHint":"Paste a Linear personal API key."},
        {"id":"notion","name":"Notion","url":"https://mcp.notion.com/mcp","docsUrl":"https://developers.notion.com/docs/mcp","auth":"oauth","tokenHint":"Notion uses OAuth, which Retinue does not support yet."},
        {"id":"sentry","name":"Sentry","url":"https://mcp.sentry.dev/mcp","docsUrl":"https://docs.sentry.io/product/sentry-mcp/","auth":"oauth","tokenHint":"Sentry uses OAuth, which Retinue does not support yet."},
        {"id":"stripe","name":"Stripe","url":"https://mcp.stripe.com","docsUrl":"https://docs.stripe.com/mcp","auth":"oauth","tokenHint":"Stripe uses OAuth, which Retinue does not support yet."},
        {"id":"supabase","name":"Supabase","url":"https://mcp.supabase.com/mcp","docsUrl":"https://supabase.com/docs/guides/getting-started/mcp","auth":"token","tokenHint":"Paste a Supabase personal access token, and add ?project_ref=<ref> to the address."}
    ])
}

// ---------------------------------------------------------------------------
// Outbound safety kit

/// A parsed and checked connector address.
#[derive(Debug)]
pub(crate) struct Endpoint {
    url: reqwest::Url,
    host: String,
    port: u16,
}

fn loopback_host(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost") || host == "::1" || host.starts_with("127.")
}

/// HTTPS only; plain HTTP is allowed solely for a server on this PC, which is
/// what "localhost allowed" means in the plan. A sign-in in the address is
/// refused, and so is a fragment.
pub(crate) fn parse_url(raw: &str) -> Result<Endpoint, String> {
    let raw = raw.trim();
    if raw.is_empty() || raw.len() > URL_MAX {
        return Err("The address must be 1 to 2,048 characters.".to_owned());
    }
    let url = reqwest::Url::parse(raw).map_err(|_| "That is not a valid address.".to_owned())?;
    let host = url.host_str().filter(|host| !host.is_empty()).ok_or("The address has no host.")?.to_owned();
    let secure = url.scheme() == "https";
    let local = url.scheme() == "http" && loopback_host(&host);
    if !secure && !local {
        return Err("Use an https:// address. Plain http:// is allowed only for a server on this PC.".to_owned());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("Remove the sign-in from the address; the token is saved separately.".to_owned());
    }
    if url.fragment().is_some() {
        return Err("Remove the # fragment from the address.".to_owned());
    }
    let port = url.port_or_known_default().ok_or("The address has no port.")?;
    Ok(Endpoint { url, host, port })
}

/// The address that was validated is the address the connection uses: the
/// client resolves the host to this one IP and never looks it up again.
async fn resolve(endpoint: &Endpoint) -> Result<std::net::SocketAddr, String> {
    if let Ok(ip) = endpoint.host.parse::<std::net::IpAddr>() {
        return Ok(std::net::SocketAddr::new(ip, endpoint.port));
    }
    let mut addresses = tokio::net::lookup_host((endpoint.host.as_str(), endpoint.port))
        .await
        .map_err(|_| format!("The address {} could not be resolved.", endpoint.host))?;
    addresses.next().ok_or_else(|| format!("The address {} resolved to nothing.", endpoint.host))
}

fn build_client(endpoint: &Endpoint, address: std::net::SocketAddr) -> Result<reqwest::Client, String> {
    let redirects = reqwest::redirect::Policy::custom(|attempt| {
        if attempt.previous().len() >= 5 {
            return attempt.error("The connector redirected too many times.");
        }
        match attempt.previous().last() {
            Some(previous) if previous.origin() != attempt.url().origin() => {
                // Never follow a redirect that would carry the token to another origin.
                attempt.stop()
            }
            _ => attempt.follow(),
        }
    });
    reqwest::Client::builder()
        .redirect(redirects)
        .resolve(&endpoint.host, address)
        // The pinned address is the address contacted: no proxy sees the token.
        .no_proxy()
        .use_rustls_tls()
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .build()
        .map_err(|_| "The connector connection could not be prepared.".to_owned())
}

/// One MCP session: initialize, then any number of calls. It is opened for a
/// check or a call and dropped afterwards, so nothing is pooled and no state
/// outlives the call that needed it.
struct Session {
    http: reqwest::Client,
    url: reqwest::Url,
    token: Option<Zeroizing<String>>,
    session_id: Option<String>,
    /// The version the server agreed to; ours until it answers.
    protocol: String,
    next_id: u64,
}

struct Reply {
    status: reqwest::StatusCode,
    session: Option<String>,
    content_type: String,
    body: Vec<u8>,
}

impl Session {
    async fn open(endpoint: &str, token: Option<&str>) -> Result<Self, String> {
        let parsed = parse_url(endpoint)?;
        let address = resolve(&parsed).await?;
        let http = build_client(&parsed, address)?;
        let mut session = Self { http, url: parsed.url, token: token.map(|value| Zeroizing::new(value.to_owned())), session_id: None, protocol: PROTOCOL_VERSION.to_owned(), next_id: 1 };
        session.initialize().await?;
        Ok(session)
    }

    fn token_text(&self) -> Option<&str> {
        self.token.as_deref().map(String::as_str)
    }

    async fn initialize(&mut self) -> Result<(), String> {
        let params = json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": {},
            "clientInfo": {"name": "retinue", "version": env!("CARGO_PKG_VERSION")}
        });
        let result = self.rpc("initialize", params).await?;
        if let Some(version) = result["protocolVersion"].as_str() {
            // Speak the version the server agreed to; a server that answers an
            // older one is otherwise within its rights to refuse the next call.
            if matches!(version, "2024-11-05" | "2025-03-26" | "2025-06-18") {
                self.protocol = version.to_owned();
            }
        }
        let _ = self.notify("notifications/initialized").await;
        Ok(())
    }

    async fn post(&self, body: &Value) -> Result<Reply, String> {
        let mut request = self
            .http
            .post(self.url.clone())
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream")
            .header("User-Agent", format!("Retinue/{}", env!("CARGO_PKG_VERSION")))
            .body(body.to_string());
        if let Some(token) = self.token_text() {
            request = request.header("Authorization", format!("Bearer {token}"));
        }
        if let Some(session) = self.session_id.as_deref() {
            request = request.header("MCP-Protocol-Version", self.protocol.as_str()).header("Mcp-Session-Id", session);
        }
        let response = request.send().await.map_err(|error| {
            if error.is_timeout() {
                "The connector did not answer in time.".to_owned()
            } else if error.is_connect() {
                "Retinue could not reach the connector's address.".to_owned()
            } else {
                "The connector connection failed.".to_owned()
            }
        })?;
        let status = response.status();
        let session = response.headers().get("mcp-session-id").and_then(|value| value.to_str().ok()).map(str::to_owned);
        let content_type = response.headers().get("content-type").and_then(|value| value.to_str().ok()).unwrap_or("").to_ascii_lowercase();
        let body = read_body(response).await?;
        Ok(Reply { status, session, content_type, body })
    }

    async fn notify(&mut self, method: &str) -> Result<(), String> {
        let reply = self.post(&json!({"jsonrpc": "2.0", "method": method})).await?;
        if let Some(session) = reply.session {
            self.session_id = Some(session);
        }
        Ok(())
    }

    async fn rpc(&mut self, method: &str, params: Value) -> Result<Value, String> {
        let id = self.next_id;
        self.next_id += 1;
        let reply = self.post(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})).await?;
        if let Some(session) = reply.session {
            self.session_id = Some(session);
        }
        if reply.status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(if self.token.is_some() {
                "The connector refused the token (HTTP 401). Check it in Settings, or remove it if the server needs none.".to_owned()
            } else {
                "The connector needs a token before it can be used (HTTP 401). Add one in Settings.".to_owned()
            });
        }
        if !reply.status.is_success() {
            return Err(self.http_error(reply.status, &reply.body));
        }
        let text = String::from_utf8_lossy(&reply.body);
        let message = self.message(&text, &reply.content_type, id)?;
        if let Some(error) = message.get("error") {
            let text = error_message(error);
            return Err(self.redact(&text));
        }
        message.get("result").cloned().ok_or_else(|| "The connector's answer had no result.".to_owned())
    }

    fn message(&self, text: &str, content_type: &str, id: u64) -> Result<Value, String> {
        if content_type.starts_with("text/event-stream") {
            let mut last = None;
            for message in sse_messages(text) {
                if message["id"].as_u64() == Some(id) {
                    return Ok(message);
                }
                if message.get("id").is_some() && (message.get("result").is_some() || message.get("error").is_some()) {
                    last = Some(message);
                }
            }
            return last.ok_or_else(|| "The connector's event stream ended without an answer.".to_owned());
        }
        let value: Value = serde_json::from_str(text.trim()).map_err(|_| "The connector's answer was not JSON.".to_owned())?;
        Ok(value)
    }

    fn http_error(&self, status: reqwest::StatusCode, body: &[u8]) -> String {
        let text = String::from_utf8_lossy(body);
        let text = self.redact(text.trim());
        let text: String = text.chars().take(400).collect();
        if text.is_empty() {
            format!("The connector answered HTTP {status}.")
        } else {
            format!("The connector answered HTTP {status}: {text}")
        }
    }

    fn redact(&self, text: &str) -> String {
        redact_text(text, self.token_text())
    }

    fn bounded(&self, text: &str) -> String {
        let text = self.redact(text);
        if text.chars().count() <= CALL_TEXT_MAX {
            return text;
        }
        let mut out: String = text.chars().take(CALL_TEXT_MAX).collect();
        out.push_str(" [truncated]");
        out
    }

    async fn list_tools(&mut self) -> Result<Vec<Value>, String> {
        let mut tools = Vec::new();
        let mut cursor: Option<String> = None;
        let mut seen: Vec<String> = Vec::new();
        for _ in 0..PAGES_MAX {
            let params = match cursor.as_deref() {
                Some(cursor) => json!({"cursor": cursor}),
                None => json!({}),
            };
            let result = self.rpc("tools/list", params).await?;
            if let Some(list) = result["tools"].as_array() {
                for tool in list {
                    if let Some(entry) = tool_entry(tool) {
                        tools.push(entry);
                    }
                    if tools.len() >= TOOLS_MAX {
                        return Ok(tools);
                    }
                }
            }
            match result["nextCursor"].as_str().filter(|next| !next.is_empty()) {
                Some(next) if !seen.iter().any(|seen| seen == next) => {
                    seen.push(next.to_owned());
                    cursor = Some(next.to_owned());
                }
                _ => break,
            }
        }
        Ok(tools)
    }

    async fn call_tool(&mut self, name: &str, arguments: &Value) -> Result<String, String> {
        let result = self.rpc("tools/call", json!({"name": name, "arguments": arguments})).await?;
        let text = self.bounded(&result_text(&result));
        if result["isError"] == true {
            return Err(text);
        }
        Ok(text)
    }
}

async fn read_body(mut response: reqwest::Response) -> Result<Vec<u8>, String> {
    if let Some(length) = response.content_length() {
        if length > RESPONSE_MAX as u64 {
            return Err("The connector's answer was larger than 4 MB, so Retinue stopped before reading it.".to_owned());
        }
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| "The connector's answer stopped partway through.".to_owned())? {
        if body.len() + chunk.len() > RESPONSE_MAX {
            return Err("The connector's answer was larger than 4 MB, so Retinue stopped reading it.".to_owned());
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

/// The MCP server-sent-events body: each `data:` block is one JSON-RPC message.
fn sse_messages(text: &str) -> Vec<Value> {
    let mut messages = Vec::new();
    let mut data = String::new();
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("data:") {
            if !data.is_empty() {
                data.push('\n');
            }
            data.push_str(rest.strip_prefix(' ').unwrap_or(rest));
        } else if line.trim().is_empty() && !data.is_empty() {
            if let Ok(value) = serde_json::from_str(&data) {
                messages.push(value);
            }
            data.clear();
        }
    }
    if !data.is_empty() {
        if let Ok(value) = serde_json::from_str(&data) {
            messages.push(value);
        }
    }
    messages
}

fn error_message(error: &Value) -> String {
    let message = error["message"].as_str().unwrap_or("The connector refused the call.");
    match error["code"].as_i64() {
        Some(code) => format!("{message} (code {code})"),
        None => message.to_owned(),
    }
}

fn trim_to(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_owned();
    }
    let mut out: String = text.chars().take(limit).collect();
    out.push('…');
    out
}

/// One tool as the settings page and search see it. The schema is stripped of
/// regex-bearing keys before it is stored or shown (report 03: a hostile schema
/// must not be able to run a regex on this machine).
fn tool_entry(tool: &Value) -> Option<Value> {
    let name = tool["name"].as_str()?.trim();
    if name.is_empty() || name.len() > TOOL_MAX {
        return None;
    }
    let description = tool["description"].as_str().map(|text| trim_to(text.trim(), 2_000)).unwrap_or_default();
    let title = tool["annotations"]["title"].as_str().or_else(|| tool["title"].as_str()).map(|text| trim_to(text.trim(), 200)).filter(|text| !text.is_empty());
    let read_only = tool["annotations"]["readOnlyHint"].as_bool();
    let schema = sanitize_schema(&tool["inputSchema"]);
    Some(json!({
        "name": name,
        "title": title,
        "description": description,
        "readOnly": read_only,
        "inputSchema": schema
    }))
}

fn sanitize_schema(schema: &Value) -> Value {
    let mut schema = schema.clone();
    strip_schema_patterns(&mut schema, 0);
    let encoded = schema.to_string();
    if encoded.len() <= SCHEMA_MAX {
        return schema;
    }
    json!({"type": "object", "description": "This tool's schema was too large to show. Call it with the fields its description names."})
}

fn strip_schema_patterns(value: &mut Value, depth: usize) {
    if depth > 12 {
        return;
    }
    match value {
        Value::Object(map) => {
            map.remove("pattern");
            map.remove("patternProperties");
            for child in map.values_mut() {
                strip_schema_patterns(child, depth + 1);
            }
        }
        Value::Array(items) => {
            for child in items.iter_mut() {
                strip_schema_patterns(child, depth + 1);
            }
        }
        _ => {}
    }
}

/// A tool call's text: its text blocks joined, or a note naming the blocks it
/// returned, or the whole result as JSON when there is no `content`.
fn result_text(result: &Value) -> String {
    let blocks = result["content"].as_array();
    let mut parts = Vec::new();
    let structured = result.get("structuredContent").cloned();
    if let Some(blocks) = blocks {
        for block in blocks {
            match block["type"].as_str() {
                Some("text") => parts.push(block["text"].as_str().unwrap_or("").to_owned()),
                Some(kind) => parts.push(format!("[{kind} content omitted]")),
                None => parts.push(trim_to(&block.to_string(), 2_000)),
            }
        }
    }
    let text = if !parts.is_empty() {
        parts.join("\n")
    } else if let Some(structured) = structured {
        structured.to_string()
    } else if result.is_null() {
        "The connector returned nothing.".to_owned()
    } else {
        trim_to(&result.to_string(), 8_000)
    };
    if text.trim().is_empty() {
        "The connector returned an empty answer.".to_owned()
    } else {
        text
    }
}

// ---------------------------------------------------------------------------
// Redaction

fn secret_patterns() -> &'static (regex::Regex, regex::Regex, regex::Regex, regex::Regex) {
    static PATTERNS: OnceLock<(regex::Regex, regex::Regex, regex::Regex, regex::Regex)> = OnceLock::new();
    PATTERNS.get_or_init(|| {
        let key = regex::Regex::new(r"(?s)-----BEGIN [^-]*PRIVATE KEY-----.*?-----END [^-]*PRIVATE KEY-----").expect("private key pattern");
        let header = regex::Regex::new(r"(?i)\b(authorization|proxy-authorization|set-cookie|cookie|x-api-key|x-auth-token)\s*:\s*[^\r\n]+").expect("header pattern");
        let scheme = regex::Regex::new(r"(?i)\b(bearer|basic)\s+[A-Za-z0-9._~+/=-]{8,}").expect("scheme pattern");
        let shapes = regex::Regex::new(r"\b(?:sk-[A-Za-z0-9_-]{12,}|gh[pousr]_[A-Za-z0-9]{16,}|github_pat_[A-Za-z0-9_]{20,}|xox[baprs]-[A-Za-z0-9-]{12,}|(?:AKIA|ASIA)[0-9A-Z]{16}|AIza[0-9A-Za-z_-]{30,}|glpat-[A-Za-z0-9_-]{20,}|eyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+)\b").expect("token shape pattern");
        (key, header, scheme, shapes)
    })
}

fn percent_encode(value: &str, form: bool) -> String {
    let mut out = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(byte as char);
        } else if form && byte == b' ' {
            out.push('+');
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// The connector's token in every form a server might echo it back in. Each form
/// is masked longest first, so a base64 form does not leave the plain form behind.
fn token_variants(token: &str) -> Vec<String> {
    let bytes = token.as_bytes();
    let mut variants = vec![token.to_owned(), percent_encode(token, false), percent_encode(token, true)];
    for engine in [
        &base64::engine::general_purpose::STANDARD,
        &base64::engine::general_purpose::STANDARD_NO_PAD,
        &base64::engine::general_purpose::URL_SAFE,
        &base64::engine::general_purpose::URL_SAFE_NO_PAD,
    ] {
        variants.push(engine.encode(bytes));
    }
    if let Ok(quoted) = serde_json::to_string(token) {
        let inner = quoted.trim_matches('"');
        if inner != token {
            variants.push(inner.to_owned());
        }
    }
    variants.sort_by_key(|variant| std::cmp::Reverse(variant.len()));
    variants.dedup();
    variants.into_iter().filter(|variant| variant.len() >= 6).collect()
}

/// Masks a returned secret in its plain, URL-encoded and base64 forms, plus the
/// generic credential shapes a server can echo from its own errors.
pub(crate) fn redact_text(text: &str, token: Option<&str>) -> String {
    let (key, header, scheme, shapes) = secret_patterns();
    let mut out = key.replace_all(text, "[hidden]").into_owned();
    out = header.replace_all(&out, "[hidden]").into_owned();
    out = scheme.replace_all(&out, "[hidden]").into_owned();
    out = shapes.replace_all(&out, "[hidden]").into_owned();
    if let Some(token) = token.filter(|token| token.len() >= 6) {
        for variant in token_variants(token) {
            if !variant.is_empty() && out.contains(&variant) {
                out = out.replace(&variant, "[hidden]");
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Checks

struct Check {
    ok: bool,
    detail: Option<String>,
    tool_count: Option<i64>,
}

async fn announce(st: &AppState, id: &str) {
    if let Ok(Some(row)) = st.db.connector_get(id) {
        st.emit("connector.changed", row).await;
    }
}

/// Reads the token, connects, lists the tools, and records what it found. A
/// failure is a status on the row, not an error to the caller.
async fn run_check(st: &AppState, id: &str) -> Result<Check, DispatchError> {
    let row = st.db.connector_get(id).map_err(storage_error)?.ok_or_else(|| derr("not_found", "That connector no longer exists.", StatusCode::NOT_FOUND))?;
    let url = row["url"].as_str().unwrap_or("").to_owned();
    let token = match credential(id) {
        Ok(token) => token,
        Err(message) => {
            st.db.connector_set_status(id, "error", Some(&message), None).map_err(storage_error)?;
            announce(st, id).await;
            return Ok(Check { ok: false, detail: Some(message), tool_count: None });
        }
    };
    match Session::open(&url, token.as_deref()).await {
        Ok(mut session) => match session.list_tools().await {
            Ok(tools) => {
                let count = tools.len() as i64;
                st.db.connector_set_check(id, "ok", None, Some(count), &Value::Array(tools)).map_err(storage_error)?;
                announce(st, id).await;
                Ok(Check { ok: true, detail: None, tool_count: Some(count) })
            }
            Err(message) => {
                let message = trim_to(&redact_text(&message, token.as_deref()), DETAIL_MAX);
                st.db.connector_set_status(id, "error", Some(&message), None).map_err(storage_error)?;
                announce(st, id).await;
                Ok(Check { ok: false, detail: Some(message), tool_count: None })
            }
        },
        Err(message) => {
            let message = trim_to(&redact_text(&message, token.as_deref()), DETAIL_MAX);
            st.db.connector_set_status(id, "error", Some(&message), None).map_err(storage_error)?;
            announce(st, id).await;
            Ok(Check { ok: false, detail: Some(message), tool_count: None })
        }
    }
}

// ---------------------------------------------------------------------------
// Credential Manager

fn credential_name(id: &str) -> String {
    format!("connector/{id}")
}

pub(crate) fn credential(id: &str) -> Result<Option<String>, String> {
    match crate::secrets::store::read(&credential_name(id)) {
        Ok(Some(value)) => Ok(Some(value.as_str().to_owned())),
        Ok(None) => Ok(None),
        Err(_) => Err("The credential store is unavailable, so the token could not be read.".to_owned()),
    }
}

// ---------------------------------------------------------------------------
// RPC

fn confirmed(p: &Value) -> Result<(), DispatchError> {
    if p["confirm"] == true {
        Ok(())
    } else {
        Err(invalid("confirm: true is required"))
    }
}

fn id_of(p: &Value) -> Result<String, DispatchError> {
    p["id"].as_str().filter(|id| !id.is_empty() && id.len() <= 64).map(str::to_owned).ok_or_else(|| invalid("id is required"))
}

fn name_of(p: &Value) -> Result<String, DispatchError> {
    let name = p["name"].as_str().map(str::trim).unwrap_or("");
    if name.is_empty() || name.chars().count() > NAME_MAX {
        return Err(invalid("name must be 1 to 80 characters"));
    }
    Ok(name.to_owned())
}

fn unattended_of(value: Option<&Value>) -> Result<Option<&'static str>, DispatchError> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) if text == "off" => Ok(Some("off")),
        Some(Value::String(text)) if text == "readOnly" => Ok(Some("readOnly")),
        Some(_) => Err(invalid("unattended must be \"off\" or \"readOnly\"")),
    }
}

fn url_of(p: &Value) -> Result<String, DispatchError> {
    let raw = p["url"].as_str().unwrap_or("");
    let parsed = parse_url(raw).map_err(|message| invalid(&message))?;
    Ok(parsed.url.to_string())
}

fn token_of(p: &Value) -> Result<Option<String>, DispatchError> {
    match p.get("token") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => {
            let text = text.trim();
            if text.is_empty() {
                // An empty value clears the token (`connector.update`); on add it means none.
                return Ok(None);
            }
            if text.len() > crate::secrets::VALUE_MAX || text.contains('\0') {
                return Err(invalid("token must be 1 to 2,048 bytes of text"));
            }
            if text.chars().any(|ch| ch.is_control()) {
                return Err(invalid("The token has a control character in it."));
            }
            Ok(Some(text.to_owned()))
        }
        Some(_) => Err(invalid("token must be text")),
    }
}

pub async fn rpc(st: &AppState, method: &str, p: &Value) -> Result<Value, DispatchError> {
    match method {
        "connector.list" => {
            Ok(json!({"connectors": st.db.connector_list().map_err(storage_error)?, "catalogue": catalogue()}))
        }
        "connector.add" => {
            let name = name_of(p)?;
            let url = url_of(p)?;
            let token = token_of(p)?;
            let id = Uuid::new_v4().to_string();
            let auth = if token.is_some() { "token" } else { "none" };
            if let Some(token) = token.as_deref() {
                crate::secrets::store::write(&credential_name(&id), token)
                    .map_err(|_| derr("unavailable", "The token could not be saved in the credential store.", StatusCode::SERVICE_UNAVAILABLE))?;
            }
            if let Err(error) = st.db.connector_insert(&id, &name, &url, auth, "off") {
                let _ = crate::secrets::store::delete(&credential_name(&id));
                return Err(storage_error(error));
            }
            run_check(st, &id).await?;
            let connector = st.db.connector_get(&id).map_err(storage_error)?.ok_or_else(|| derr("internal", "the connector was not saved", StatusCode::INTERNAL_SERVER_ERROR))?;
            Ok(json!({"connector": connector}))
        }
        "connector.update" => {
            let id = id_of(p)?;
            st.db.connector_get(&id).map_err(storage_error)?.ok_or_else(|| derr("not_found", "That connector no longer exists.", StatusCode::NOT_FOUND))?;
            let name = match p.get("name") {
                None | Some(Value::Null) => None,
                Some(_) => Some(name_of(p)?),
            };
            let enabled = match p.get("enabled") {
                None | Some(Value::Null) => None,
                Some(value) => Some(value.as_bool().ok_or_else(|| invalid("enabled must be true or false"))?),
            };
            let unattended = unattended_of(p.get("unattended"))?;
            let mut auth: Option<&str> = None;
            if p.get("token").is_some() {
                match token_of(p)? {
                    Some(token) => {
                        crate::secrets::store::write(&credential_name(&id), &token)
                            .map_err(|_| derr("unavailable", "The token could not be saved in the credential store.", StatusCode::SERVICE_UNAVAILABLE))?;
                        auth = Some("token");
                    }
                    None => {
                        let _ = crate::secrets::store::delete(&credential_name(&id));
                        auth = Some("none");
                    }
                }
            }
            st.db.connector_update(&id, name.as_deref(), auth, enabled, unattended).map_err(storage_error)?
                .ok_or_else(|| derr("not_found", "That connector no longer exists.", StatusCode::NOT_FOUND))?;
            announce(st, &id).await;
            let connector = st.db.connector_get(&id).map_err(storage_error)?.ok_or_else(|| derr("internal", "the connector was not saved", StatusCode::INTERNAL_SERVER_ERROR))?;
            Ok(json!({"connector": connector}))
        }
        "connector.remove" => {
            confirmed(p)?;
            let id = id_of(p)?;
            let removed = st.db.connector_delete(&id).map_err(storage_error)?;
            let _ = crate::secrets::store::delete(&credential_name(&id));
            if removed {
                st.emit("connector.changed", json!({"id": id, "removed": true})).await;
            }
            Ok(json!({"removed": removed}))
        }
        "connector.test" => {
            let id = id_of(p)?;
            let check = run_check(st, &id).await?;
            Ok(json!({"ok": check.ok, "detail": check.detail, "toolCount": check.tool_count}))
        }
        "connector.tools" => {
            let id = id_of(p)?;
            let connector = st.db.connector_get(&id).map_err(storage_error)?.ok_or_else(|| derr("not_found", "That connector no longer exists.", StatusCode::NOT_FOUND))?;
            let mut tools = st.db.connector_tools(&id).map_err(storage_error)?;
            if tools.is_empty() && connector["enabled"] == true {
                run_check(st, &id).await?;
                tools = st.db.connector_tools(&id).map_err(storage_error)?;
            }
            let rules = st.db.connector_rules(&id).map_err(storage_error)?;
            let tools = tools.into_iter().map(|mut tool| {
                let rule = tool["name"].as_str().and_then(|name| rules.get(name).cloned()).unwrap_or_else(|| "ask".to_owned());
                tool["rule"] = json!(rule);
                tool
            }).collect::<Vec<_>>();
            Ok(json!({"tools": tools}))
        }
        "connector.tool.rule" => {
            let id = id_of(p)?;
            let tool = p["tool"].as_str().map(str::trim).unwrap_or("");
            if tool.is_empty() || tool.chars().count() > TOOL_MAX {
                return Err(invalid("tool must be 1 to 200 characters"));
            }
            let rule = match p["rule"].as_str() {
                Some(rule @ ("ask" | "allow" | "deny")) => rule,
                _ => return Err(invalid("rule must be \"ask\", \"allow\" or \"deny\"")),
            };
            st.db.connector_get(&id).map_err(storage_error)?.ok_or_else(|| derr("not_found", "That connector no longer exists.", StatusCode::NOT_FOUND))?;
            st.db.connector_rule_set(&id, tool, rule).map_err(storage_error)?;
            announce(st, &id).await;
            Ok(json!({"ok": true, "tool": tool, "rule": rule}))
        }
        _ => Err(derr("not_found", "unknown connector method", StatusCode::NOT_FOUND)),
    }
}

// ---------------------------------------------------------------------------
// Approvals

struct Pending {
    session_id: String,
    sender: Option<oneshot::Sender<bool>>,
}

/// Cards waiting for the owner. Memory only: a restart ends them.
fn pending() -> &'static StdMutex<HashMap<String, Pending>> {
    static PENDING: OnceLock<StdMutex<HashMap<String, Pending>>> = OnceLock::new();
    PENDING.get_or_init(Default::default)
}

fn first_sentence(text: &str, limit: usize) -> String {
    let text = text.trim();
    let end = text.find(['.', '!', '?', '\n']).map(|at| at + 1).unwrap_or(text.len());
    trim_to(&text[..end], limit)
}

/// The usual approval card, unless a standing rule already answered it. This
/// runs only when the rule is `ask`; `allow` skips it and `deny` never gets here.
async fn ensure_allowed(st: &AppState, session_id: &str, session: &Value, connector: &Value, tool: &str, tool_info: Option<&Value>, arguments: &Value) -> Result<(), String> {
    if crate::questions::may_ask(st, session).is_err() {
        return Err(UNATTENDED_NOTE.to_owned());
    }
    let connector_id = connector["id"].as_str().unwrap_or("").to_owned();
    let connector_name = connector["name"].as_str().unwrap_or("the connector").to_owned();
    let agent_id = session["agentId"].as_str().unwrap_or("").to_owned();
    let bot = st.db.agent_get(&agent_id).ok().and_then(|agent| agent["name"].as_str().map(str::to_owned)).unwrap_or_else(|| "Your bot".to_owned());
    let mut detail = format!("{bot} wants to call {tool} on {connector_name}.");
    if let Some(description) = tool_info.and_then(|info| info["description"].as_str()).map(|text| text.trim()).filter(|text| !text.is_empty()) {
        detail.push(' ');
        detail.push_str(&first_sentence(description, 200));
    }
    let preview = trim_to(&redact_text(&arguments.to_string(), None), 400);
    detail.push_str(&format!(" Arguments: {preview} Retinue sends it with the token saved for this connector. Allow it once, or allow this tool for this connector from now on."));
    let title = format!("{connector_name}: {tool}");
    let choices = vec!["allow_once".to_owned(), "allow".to_owned(), "deny".to_owned()];
    let id = Uuid::new_v4().to_string();
    let (sender, receiver) = oneshot::channel();
    {
        let mut map = pending().lock().unwrap_or_else(|poison| poison.into_inner());
        if map.values().any(|waiting| waiting.session_id == session_id && waiting.sender.is_some()) {
            return Err("The owner already has a card for this conversation. Wait for their answer.".to_owned());
        }
        map.insert(id.clone(), Pending { session_id: session_id.to_owned(), sender: Some(sender) });
    }
    let remember_label = format!("Always allow {connector_name}: {tool}");
    let marks = json!({
        "kind": "connector",
        "connectorId": connector_id,
        "tool": tool,
        "matchKey": format!("connector:{connector_id}:{tool}"),
        "rememberable": true,
        "rememberLabel": remember_label
    });
    if st.db.insert_permission(&id, session_id, &format!("connector:{connector_id}:{tool}"), &title, Some(&detail), &choices, &json!({}), &marks).is_err() {
        pending().lock().unwrap_or_else(|poison| poison.into_inner()).remove(&id);
        return Err("The owner's answer could not be asked for.".to_owned());
    }
    st.emit("permission.requested", json!({
        "id": id, "sessionId": session_id, "runtimeId": Value::Null, "kind": "connector", "category": "other",
        "title": title, "detail": detail, "risk": "unknown", "choices": choices, "expiresAt": Value::Null,
        "status": "pending", "rememberable": true, "rememberLabel": remember_label,
        "projectId": Value::Null, "projectName": Value::Null,
    })).await;
    if st.db.update_session_state(session_id, "waiting_permission").is_ok() {
        if let Ok(summary) = st.db.session_event_summary(session_id) {
            st.emit("session.changed", summary).await;
        }
    }
    let outcome = tokio::time::timeout(Duration::from_secs(WAIT_SECS), receiver).await;
    pending().lock().unwrap_or_else(|poison| poison.into_inner()).remove(&id);
    match outcome {
        Ok(Ok(true)) => Ok(()),
        Ok(Ok(false)) => Err("The owner did not allow this call.".to_owned()),
        _ => Err("The owner has not answered yet. The card stays in the conversation; ask again once they have.".to_owned()),
    }
}

/// The owner's answer to a connector card, from `permission.reply`. The card is
/// already claimed by the caller. "Always allow" writes the tool's standing rule.
pub(crate) async fn reply_approval(st: &AppState, id: &str, session_id: &str, choice: &str, remember: bool) -> Result<Value, DispatchError> {
    let allowed = matches!(choice.to_ascii_lowercase().as_str(), "allow" | "allow_once" | "accept" | "yes");
    let card = st.db.permission_get(id).ok();
    let connector = card.as_ref().and_then(|card| card["connectorId"].as_str()).map(str::to_owned);
    let tool = card.as_ref().and_then(|card| card["tool"].as_str()).map(str::to_owned);
    let mut remembered = false;
    if remember && allowed {
        if let (Some(connector), Some(tool)) = (connector.as_deref(), tool.as_deref()) {
            if st.db.connector_rule_set(connector, tool, "allow").is_ok() {
                remembered = true;
                announce(st, connector).await;
            }
        }
    }
    let waiting = {
        let mut map = pending().lock().unwrap_or_else(|poison| poison.into_inner());
        match map.get(id) {
            Some(waiting) if waiting.session_id != session_id => {
                return Err(derr("conflict", "That card belongs to another conversation.", StatusCode::CONFLICT));
            }
            _ => map.remove(id),
        }
    };
    if let Some(waiting) = waiting {
        if let Some(sender) = waiting.sender {
            let _ = sender.send(allowed);
        }
    }
    Ok(json!({"resolved": true, "allowed": allowed, "remembered": remembered}))
}

// ---------------------------------------------------------------------------
// The bot's tools

/// Two meta-tools rather than one per connector tool: the prompt carries one
/// line per connector, and schemas come back from search (report 03, idea 4).
pub fn tool_schemas() -> Vec<Value> {
    vec![
        json!({
            "name": "connector_search",
            "description": "Search the owner's connectors (Notion, Linear, GitHub and the like) for a tool that does what you need. Returns matching tool names with their descriptions and input schemas, grouped by connector. Search first, then call connector_call.",
            "inputSchema": {
                "type": "object",
                "properties": {"query": {"type": "string", "description": "What you want to do, in a few words."}},
                "required": ["query"],
                "additionalProperties": false
            }
        }),
        json!({
            "name": "connector_call",
            "description": "Call one tool on one of the owner's connectors. Search with connector_search first to get the tool's schema. The owner sees an approval card for each call unless they already allowed that tool; a tool they denied is refused. In an unattended run only a tool the connector marks read-only can be called.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "connector": {"type": "string", "description": "The connector's name or id, as connector_search showed it."},
                    "tool": {"type": "string", "description": "The tool's name."},
                    "arguments": {"type": "object", "description": "The tool's arguments, matching its input schema."}
                },
                "required": ["connector", "tool"],
                "additionalProperties": false
            }
        }),
    ]
}

/// One line per enabled connector, for the bot's instructions.
pub(crate) fn instructions(st: &AppState) -> Option<String> {
    let connectors = st.db.connector_list().ok()?;
    let enabled = connectors.iter().filter(|connector| connector["enabled"] == true).collect::<Vec<_>>();
    if enabled.is_empty() {
        return None;
    }
    let mut lines = vec![
        "## Connectors".to_owned(),
        "The owner added these services as connectors. Find a tool with connector_search, then call it with connector_call; the schema comes back from search. Each call needs the owner's approval unless they allowed that tool for that connector.".to_owned(),
    ];
    for connector in enabled {
        let name = connector["name"].as_str().unwrap_or("Connector");
        let count = connector["toolCount"].as_i64().map(|count| count.to_string()).unwrap_or_else(|| "unknown".to_owned());
        let unattended = if connector["unattended"].as_str() == Some("readOnly") { ", unattended: read-only tools only" } else { "" };
        lines.push(format!("- {name}: {count} tools{unattended}"));
    }
    Some(lines.join("\n"))
}

fn resolve_connector(connectors: &[Value], wanted: &str) -> Result<Value, String> {
    let wanted = wanted.trim();
    if let Some(found) = connectors.iter().find(|connector| connector["id"].as_str() == Some(wanted)) {
        return Ok(found.clone());
    }
    let lowered = wanted.to_lowercase();
    let matches = connectors.iter().filter(|connector| connector["name"].as_str().is_some_and(|name| name.trim().to_lowercase() == lowered)).collect::<Vec<_>>();
    match matches.len() {
        0 => Err(format!("No connector is called {wanted}. Use connector_search to see the ones you have.")),
        1 => Ok(matches[0].clone()),
        _ => Err(format!("More than one connector is called {wanted}. Use its id instead, or ask the owner which one.")),
    }
}

pub async fn search_tool(st: &AppState, _session_id: &str, args: &Value) -> Result<String, String> {
    let object = args.as_object().ok_or("connector_search input must be an object.")?;
    if let Some(key) = object.keys().find(|key| key.as_str() != "query") {
        return Err(format!("connector_search input has an unsupported field: {key}."));
    }
    let query = args["query"].as_str().map(str::trim).unwrap_or("");
    if query.is_empty() {
        return Err("query is required.".to_owned());
    }
    if query.chars().count() > QUERY_MAX {
        return Err(format!("query must be at most {QUERY_MAX} characters."));
    }
    let words = query.to_lowercase().split_whitespace().take(8).map(str::to_owned).collect::<Vec<_>>();
    let connectors = st.db.connector_list().map_err(|_| "The connector list could not be read.".to_owned())?;
    let enabled = connectors.iter().filter(|connector| connector["enabled"] == true).collect::<Vec<_>>();
    if enabled.is_empty() {
        return Ok("No connectors are turned on. The owner adds them in Retinue > Settings > Connectors.".to_owned());
    }
    let mut groups = Vec::new();
    let mut total = 0usize;
    for connector in &enabled {
        let Some(id) = connector["id"].as_str() else { continue };
        let name = connector["name"].as_str().unwrap_or("Connector");
        let tools = st.db.connector_tools(id).unwrap_or_default();
        let mut hits = Vec::new();
        for tool in &tools {
            let haystack = format!(
                "{} {} {}",
                tool["name"].as_str().unwrap_or(""),
                tool["title"].as_str().unwrap_or(""),
                tool["description"].as_str().unwrap_or("")
            ).to_lowercase();
            if words.iter().all(|word| haystack.contains(word.as_str())) {
                hits.push(tool.clone());
            }
        }
        total += hits.len();
        if hits.is_empty() {
            continue;
        }
        hits.truncate(SEARCH_LIMIT);
        groups.push(json!({"connector": name, "connectorId": id, "tools": hits}));
    }
    if groups.is_empty() {
        let names = enabled.iter().filter_map(|connector| connector["name"].as_str()).collect::<Vec<_>>().join(", ");
        return Ok(format!("No tool of {names} matches \"{query}\". Try a single word such as \"issue\", \"page\" or \"repository\"."));
    }
    let mut text = json!({"query": query, "connectors": groups}).to_string();
    if total > SEARCH_LIMIT {
        text.push_str(&format!("\n({total} tools matched; each connector shows at most {SEARCH_LIMIT}.)"));
    }
    Ok(text)
}

pub async fn call_tool(st: &AppState, session_id: &str, args: &Value) -> Result<String, String> {
    let object = args.as_object().ok_or("connector_call input must be an object.")?;
    if let Some(key) = object.keys().find(|key| !matches!(key.as_str(), "connector" | "tool" | "arguments")) {
        return Err(format!("connector_call input has an unsupported field: {key}."));
    }
    let wanted = args["connector"].as_str().map(str::trim).filter(|value| !value.is_empty()).ok_or("connector is required.")?;
    let tool = args["tool"].as_str().map(str::trim).filter(|value| !value.is_empty()).ok_or("tool is required.")?;
    if tool.chars().count() > TOOL_MAX {
        return Err(format!("tool must be at most {TOOL_MAX} characters."));
    }
    let arguments = match &args["arguments"] {
        Value::Null => json!({}),
        value @ Value::Object(_) => value.clone(),
        _ => return Err("arguments must be an object.".to_owned()),
    };
    let session = st.db.session_detail(session_id).map_err(|_| "This conversation is no longer available.".to_owned())?;
    let connectors = st.db.connector_list().map_err(|_| "The connector list could not be read.".to_owned())?;
    let connector = resolve_connector(&connectors, wanted)?;
    let id = connector["id"].as_str().unwrap_or("").to_owned();
    let name = connector["name"].as_str().unwrap_or("The connector").to_owned();
    if connector["enabled"] != true {
        return Err(format!("The connector {name} is turned off. The owner can turn it on in Settings."));
    }
    let rules = st.db.connector_rules(&id).unwrap_or_default();
    let rule = rules.get(tool).cloned().unwrap_or_else(|| "ask".to_owned());
    if rule == "deny" {
        return Err(format!("The owner denied {name}: {tool}. Do not call it; say what is blocked."));
    }
    let cached = st.db.connector_tools(&id).unwrap_or_default();
    let info = cached.iter().find(|entry| entry["name"].as_str() == Some(tool)).cloned();
    let unattended = crate::is_unattended(&session["link"]) || crate::questions::may_ask(st, &session).is_err();
    if unattended {
        if connector["unattended"].as_str() != Some("readOnly") {
            return Err(UNATTENDED_NOTE.to_owned());
        }
        match info.as_ref().and_then(|info| info["readOnly"].as_bool()) {
            Some(true) => {}
            _ => return Err(format!("{name}: {tool} is not marked read-only, so an unattended run cannot call it.")),
        }
    } else if rule != "allow" {
        ensure_allowed(st, session_id, &session, &connector, tool, info.as_ref(), &arguments).await?;
    }
    let token = credential(&id)?;
    if connector["auth"].as_str() == Some("token") && token.is_none() {
        return Err(format!("The token for {name} is missing from the credential store. Add it again in Settings."));
    }
    let url = connector["url"].as_str().unwrap_or("").to_owned();
    let outcome = async {
        let mut session = Session::open(&url, token.as_deref()).await?;
        session.call_tool(tool, &arguments).await
    }
    .await;
    match outcome {
        Ok(text) => {
            if connector["status"].as_str() != Some("ok") {
                let count = connector["toolCount"].as_i64();
                let _ = st.db.connector_set_status(&id, "ok", None, count);
                announce(st, &id).await;
            }
            Ok(text)
        }
        Err(message) => {
            let detail = trim_to(&redact_text(&message, token.as_deref()), DETAIL_MAX);
            let _ = st.db.connector_set_status(&id, "error", Some(&detail), None);
            announce(st, &id).await;
            Err(detail)
        }
    }
}
