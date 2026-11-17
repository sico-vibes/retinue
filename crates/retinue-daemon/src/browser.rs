//! Browsers on the linked cloud computer.
//!
//! Employees get a browser of their own, inside the linked computer's container:
//! Chromium driven by Playwright, in Retinue's own profile folder
//! (`/config/retinue/browser/profile/<conversation>`), never the owner's signed-in
//! browser and never a profile with imported cookies or extensions. One context per
//! conversation: started on first use, closed when the conversation's turn ends or
//! after ten idle minutes (see `sweep_once`).
//!
//! The tools are a second app-owned MCP server, `vps-browser`, offered only while
//! the computer is linked and switched on for employees — the same gate as
//! `vps-computer`, checked again at every call, not when the conversation started.
//! A call is refused when nobody is watching: a scheduled run, an unattended task,
//! and a side conversation woken by a teammate's message all get no browser.
//!
//! The daemon is the policy. The container runs one small Node server
//! (`SERVER_SCRIPT`) that owns Chromium; every tool call here
//! 1. validates what the bot asked for (`validate_url`, `ref_of`, `key_of`),
//! 2. for a click, a type or a key, asks the page what the target is first, and
//!    pauses before acting when it is a password field, a 2FA prompt, a payment
//!    field or a publish button (`pause_reason`),
//! 3. refuses a page that redirected the browser into a local, link-local or
//!    metadata address (`landed_on_a_page`), and
//! 4. returns `needs_owner` with one short reason, so the bot asks the owner
//!    instead of carrying on.
//!
//! A screenshot follows the generated-image path (`media::requested`): one image
//! per call, stored in the media folder, carried to the transcript as
//! `generated_image`; the bytes are never in the reply, in an event, or in a log.
//! What this module records is content-free: the action class, the host, and for
//! typing only a count ("typed 12 characters").
//!
//! `browser.status` is desktop only: every method is `never` in `tiers.json` and
//! `browser.changed` is `drop`, so nothing here is reachable from the remote
//! gateway. `computer.status` gains `browser: {ready, missing}` from
//! `add_status`, which the dispatcher calls after `computer::rpc`; `computer.rs`
//! itself is unchanged.
//!
//! The permission category: `retinue-agent-core`'s `classify_permission` maps each
//! of these tools to `BROWSER` and marks it `allowed: false` (`browser_permission`
//! there), so the owner's card says what the tool would do and no mode, remembered
//! rule or `accept_edits` covers browsing. These tools are not in the adapters'
//! pre-approved list, so the card is raised in the first place.

use crate::{derr, AppState, DispatchError};
use axum::http::StatusCode;
use base64::Engine as _;
use chrono::{SecondsFormat, Utc};
use retinue_agent_core::McpServerSpec;
use retinue_storage::ComputerLink;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    net::IpAddr,
    path::{Path, PathBuf},
    sync::Mutex as StdMutex,
    time::{Duration, Instant},
};
use uuid::Uuid;
use zeroize::Zeroizing;

/// The MCP server's name, as the provider sees it.
pub const SERVER_NAME: &str = "vps-browser";
/// Where the linked computer's API lives.
const API: &str = "/harbor/api/computer";
/// The container folder Retinue owns: the profile, the server script and its log.
const CONTAINER_DIR: &str = "/config/retinue/browser";
/// The port the container's browser server listens on, on loopback only.
const CONTAINER_PORT: u16 = 47_810;
/// The marker the health check and the script agree on; a stale script is replaced.
const SCRIPT_MARKER: &str = "retinue-browser-2";
/// The line the bootstrap echoes when the server answers.
const READY_MARK: &str = "BROWSER_READY";
const RUNNING_MARK: &str = "BROWSER_RUNNING";
const MISSING_MARK: &str = "MISSING_PLAYWRIGHT";
/// The install step named when Playwright is not in the container.
pub const INSTALL_COMMAND: &str = "cd /config/retinue/browser && npm i playwright && npx playwright install --with-deps chromium";

/// How long one container command may take. Opening a page and loading Playwright
/// are the slow ones; everything else is milliseconds.
const ACTION_TIMEOUT: Duration = Duration::from_secs(90);
const BOOT_TIMEOUT: Duration = Duration::from_secs(120);
const PROBE_TIMEOUT: Duration = Duration::from_secs(10);
/// Extra time beyond the command's own limit, so the container answers first.
const EXEC_MARGIN: Duration = Duration::from_secs(15);
/// Harbor's ceiling for one `exec`.
const EXEC_MAX_MS: u64 = 300_000;
/// Readiness is cached this long, so a status poll does not probe the container.
const READINESS_TTL: Duration = Duration::from_secs(30);
/// A context untouched for this long is closed.
const IDLE_LIMIT: Duration = Duration::from_secs(600);
const SWEEP_INTERVAL: Duration = Duration::from_secs(30);
/// Caps on what a bot may ask for and what comes back.
const URL_MAX: usize = 2_048;
const TEXT_MAX: usize = 2_000;
const KEY_MAX: usize = 40;
const SNAPSHOT_MAX_NODES: usize = 400;
const SNAPSHOT_MAX_CHARS: usize = 12_000;
/// A screenshot is a PNG of a 1280 px page; this is the ceiling before it is
/// handed to the media layer, which caps it again at 20 MB.
const SCREENSHOT_MAX_BYTES: usize = 8 * 1024 * 1024;
const TOKEN_MAX: usize = 2_048;
/// The endpoint folder and files the bridge reads, inside the data folder.
const ENDPOINT_FOLDER: &str = "browser";

const NOT_LINKED: &str = "No computer is linked in Retinue, so this conversation has no browser. Ask the owner to link one in Settings.";
const SWITCHED_OFF: &str = "The computer is switched off in Retinue, so this conversation has no browser. Ask the owner to switch it on.";
const UNATTENDED: &str = "This conversation has nobody watching it (a scheduled run, an unattended task, or a turn woken by a teammate), so it cannot use the browser.";
const NOT_READY: &str = "The browser is not ready inside the bot computer.";
const STALE: &str = "That element is out of date. Take a new snapshot and use the reference it lists.";
/// A page that redirected the browser into an address it may not read.
const REDIRECTED: &str = "That page sent the browser to a local or scripted address, which Retinue does not open, so its contents are not returned.";
const UNREACHABLE: &str = "The computer did not answer. It may be off, or the address may have changed.";
const TOKEN_REFUSED: &str = "Harbor refused the computer's token. The owner needs to link the computer again.";

/// The script the container runs: Playwright, one persistent profile per
/// conversation, actions addressed by the references a snapshot stamps on the
/// page. It holds no policy of its own; the daemon decides what may happen.
const SERVER_SCRIPT: &str = r##"// retinue-browser-2: Playwright inside the bot computer, driven by the daemon.
// A filled or password field's value is never part of a snapshot's name: the
// daemon lists the references it filled, and the page says "(filled)" instead.
import http from 'node:http';
import fs from 'node:fs';
import { chromium } from 'playwright';

const PORT = 47810;
const ROOT = '/config/retinue/browser';
const MAX_BYTES = 8388608;
const contexts = new Map();

function close(id) {
  const entry = contexts.get(id);
  if (!entry) return;
  contexts.delete(id);
  entry.context.close().catch(() => {});
}

async function entryFor(id) {
  let entry = contexts.get(id);
  if (!entry) {
    fs.mkdirSync(`${ROOT}/profile/${id}`, { recursive: true });
    const context = await chromium.launchPersistentContext(`${ROOT}/profile/${id}`, {
      headless: true,
      viewport: { width: 1280, height: 800 },
      acceptDownloads: false,
      bypassCSP: false,
    });
    const page = context.pages()[0] ?? (await context.newPage());
    entry = { context, page };
    contexts.set(id, entry);
  }
  return entry;
}

async function snapshot(entry, maskedRefs) {
  const hidden = Array.isArray(maskedRefs) ? maskedRefs : [];
  const nodes = await entry.page.evaluate((masked) => {
    const selector = 'a,button,input,select,textarea,summary,h1,h2,[role],[contenteditable="true"]';
    return Array.from(document.querySelectorAll(selector)).slice(0, 400).map((el, index) => {
      const ref = `e${index}`;
      el.setAttribute('data-retinue-ref', ref);
      const box = el.getBoundingClientRect();
      const type = (el.getAttribute('type') || '').toLowerCase();
      const autocomplete = (el.getAttribute('autocomplete') || '').toLowerCase();
      const value = typeof el.value === 'string' ? el.value : '';
      const label = (el.getAttribute('aria-label') || el.getAttribute('placeholder') || el.getAttribute('name') || '').trim();
      // A password, a card field, a code and anything the daemon filled never
      // come back as their value (16 November 2026).
      const sealed = type === 'password' || autocomplete.startsWith('cc-') || autocomplete === 'one-time-code' || masked.includes(ref);
      const text = (el.innerText || '').trim();
      const name = sealed ? (label || (type === 'password' ? 'password field' : 'field')) + (value ? ' (filled)' : '') : (el.getAttribute('aria-label') || text || value || el.getAttribute('placeholder') || el.name || '');
      return {
        ref,
        tag: el.tagName.toLowerCase(),
        role: el.getAttribute('role') || '',
        type,
        name: name.trim().slice(0, 200),
        autocomplete,
        visible: box.width > 0 && box.height > 0,
      };
    });
  }, hidden);
  let tree = '';
  try {
    tree = await entry.page.locator(':root').ariaSnapshot();
  } catch {
    tree = '';
  }
  return { url: entry.page.url(), title: await entry.page.title(), nodes, tree };
}

// A page callback is serialised and runs in the page, so it cannot call any
// helper defined here; the forms below repeat the same fields inline.
async function targetOf(entry, ref, maskedRefs) {
  const hidden = Array.isArray(maskedRefs) ? maskedRefs : [];
  if (!ref) {
    return entry.page.evaluate((masked) => {
      const el = document.activeElement;
      if (!el) return null;
      const type = (el.getAttribute('type') || '').toLowerCase();
      const autocomplete = (el.getAttribute('autocomplete') || '').toLowerCase();
      const value = typeof el.value === 'string' ? el.value : '';
      const sealed = type === 'password' || autocomplete.startsWith('cc-') || autocomplete === 'one-time-code' || masked.includes(el.getAttribute('data-retinue-ref') || '');
      const label = (el.getAttribute('aria-label') || el.getAttribute('placeholder') || el.getAttribute('name') || '').trim();
      const name = sealed ? (label || (type === 'password' ? 'password field' : 'field')) + (value ? ' (filled)' : '') : (el.getAttribute('aria-label') || el.innerText || value || el.getAttribute('placeholder') || el.name || '');
      return { tag: el.tagName.toLowerCase(), role: el.getAttribute('role') || '', type, name: name.trim().slice(0, 200), autocomplete };
    }, hidden);
  }
  const locator = entry.page.locator(`[data-retinue-ref="${ref}"]`);
  if ((await locator.count()) !== 1) return { stale: true };
  return locator.evaluate((el, masked) => {
    const type = (el.getAttribute('type') || '').toLowerCase();
    const autocomplete = (el.getAttribute('autocomplete') || '').toLowerCase();
    const value = typeof el.value === 'string' ? el.value : '';
    const sealed = type === 'password' || autocomplete.startsWith('cc-') || autocomplete === 'one-time-code' || masked.includes(el.getAttribute('data-retinue-ref') || '');
    const label = (el.getAttribute('aria-label') || el.getAttribute('placeholder') || el.getAttribute('name') || '').trim();
    const name = sealed ? (label || (type === 'password' ? 'password field' : 'field')) + (value ? ' (filled)' : '') : (el.getAttribute('aria-label') || el.innerText || value || el.getAttribute('placeholder') || el.name || '');
    return { tag: el.tagName.toLowerCase(), role: el.getAttribute('role') || '', type, name: name.trim().slice(0, 200), autocomplete };
  }, hidden);
}

const actions = {
  async open(entry, body) {
    await entry.page.goto(body.url, { waitUntil: 'domcontentloaded', timeout: 45000 });
    return await snapshot(entry, body.mask);
  },
  async snapshot(entry, body) {
    return await snapshot(entry, body.mask);
  },
  async describe(entry, body) {
    const target = await targetOf(entry, body.ref, body.mask);
    if (target && target.stale) return { stale: true };
    return { target, url: entry.page.url() };
  },
  async click(entry, body) {
    const target = await targetOf(entry, body.ref, body.mask);
    if (!target || target.stale) return { stale: true };
    await entry.page.locator(`[data-retinue-ref="${body.ref}"]`).click({ timeout: 15000 });
    await entry.page.waitForLoadState('domcontentloaded').catch(() => {});
    return { target, ...(await snapshot(entry, body.mask)) };
  },
  async type(entry, body) {
    const target = await targetOf(entry, body.ref, body.mask);
    if (!target || target.stale) return { stale: true };
    await entry.page.locator(`[data-retinue-ref="${body.ref}"]`).fill(String(body.text ?? ''), { timeout: 15000 });
    return { target };
  },
  // The vault's own fill: the value is written and nothing about it, or about
  // the field's new value, comes back.
  async fill(entry, body) {
    const locator = entry.page.locator(`[data-retinue-ref="${body.ref}"]`);
    if ((await locator.count()) !== 1) return { stale: true };
    await locator.fill(String(body.text ?? ''), { timeout: 15000 });
    return { filled: true };
  },
  async press(entry, body) {
    await entry.page.keyboard.press(String(body.key));
    await entry.page.waitForLoadState('domcontentloaded').catch(() => {});
    return await snapshot(entry, body.mask);
  },
  async back(entry, body) {
    await entry.page.goBack({ waitUntil: 'domcontentloaded', timeout: 20000 }).catch(() => {});
    return await snapshot(entry, body.mask);
  },
  async screenshot(entry) {
    const buffer = await entry.page.screenshot({ type: 'png' });
    if (buffer.length > MAX_BYTES) return { tooLarge: true };
    return { mediaType: 'image/png', base64: buffer.toString('base64'), url: entry.page.url() };
  },
};

const server = http.createServer(async (request, response) => {
  const send = (status, value) => {
    const text = JSON.stringify(value);
    response.writeHead(status, { 'content-type': 'application/json', 'content-length': Buffer.byteLength(text) });
    response.end(text);
  };
  if (request.method === 'GET' && request.url === '/health') {
    return send(200, { ok: true, version: 'retinue-browser-2', contexts: contexts.size });
  }
  if (request.method !== 'POST' || request.url !== '/act') return send(404, { ok: false, error: 'not found' });
  let body = '';
  for await (const chunk of request) {
    body += chunk;
    if (body.length > 4000000) return send(413, { ok: false, error: 'too large' });
  }
  let parsed;
  try {
    parsed = JSON.parse(body);
  } catch {
    return send(400, { ok: false, error: 'unreadable request' });
  }
  const conversation = String(parsed.conversation || '');
  if (!/^[A-Za-z0-9_-]{1,80}$/.test(conversation)) return send(400, { ok: false, error: 'no conversation' });
  if (parsed.action === 'close') {
    close(conversation);
    return send(200, { ok: true });
  }
  const action = actions[parsed.action];
  if (!action) return send(400, { ok: false, error: 'unknown action' });
  try {
    const entry = await entryFor(conversation);
    send(200, { ok: true, ...(await action(entry, parsed)) });
  } catch (error) {
    const message = String((error && error.message) || error).slice(0, 300);
    send(200, { ok: false, error: message });
  }
});

server.listen(PORT, '127.0.0.1');
"##;

/// What the daemon remembers between calls: the conversations with an open
/// context, and the last readiness it read and announced.
#[derive(Default)]
pub struct Browser {
    open: StdMutex<BTreeMap<String, Opened>>,
    readiness: StdMutex<Option<(Instant, bool, Value)>>,
    announced: StdMutex<Option<Value>>,
    /// The values this conversation has had typed into a page by the vault, and
    /// the references they went into. Memory only, dropped with the context.
    known: StdMutex<BTreeMap<String, Known>>,
}

/// One conversation's filled values and filled references. A value is only ever
/// compared against text that is about to leave the daemon.
#[derive(Default)]
struct Known {
    values: Vec<Zeroizing<String>>,
    refs: Vec<String>,
}

/// At most this many values and references are remembered per conversation.
const KNOWN_MAX: usize = 16;

/// One conversation's context, and when it was last used.
struct Opened {
    last_used: Instant,
    /// Tests only: idle regardless of the clock. A Windows `Instant` counts from
    /// boot, so a CI runner up for less than ten minutes cannot backdate one.
    #[cfg(test)]
    stale: bool,
}

impl Opened {
    fn idle(&self) -> bool {
        #[cfg(test)]
        if self.stale { return true; }
        self.last_used.elapsed() >= IDLE_LIMIT
    }
}

fn now_iso() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn data_dir(st: &AppState) -> Option<PathBuf> {
    st.team.data_dir.lock().ok().and_then(|dir| dir.clone())
}

/// One bounded, control-free line of the container's own text.
fn one_line(text: &str, limit: usize) -> String {
    let line = text.lines().map(str::trim).find(|line| !line.is_empty()).unwrap_or("");
    line.chars().filter(|ch| !ch.is_control()).take(limit).collect()
}

// ---------------------------------------------------------------------------
// The gates

/// Whether the owner is there: a side conversation is woken by a teammate's
/// message and a scheduled run by the clock, so neither may drive a browser. An
/// archived conversation, an unattended task and a dead origin chain are the
/// same refusal through `questions::may_ask`.
fn owner_is_watching(st: &AppState, session_id: &str) -> bool {
    let Ok(session) = st.db.session_detail(session_id) else { return false };
    if session["link"]["kind"] == "side" { return false; }
    crate::questions::may_ask(st, &session).is_ok()
}

/// The gates every call passes: linked, switched on, and watched — read when the
/// call arrives, never when the conversation started.
fn gate(st: &AppState, session_id: &str) -> Result<ComputerLink, String> {
    let Some(link) = st.db.computer_link().ok().flatten() else { return Err(NOT_LINKED.into()) };
    if !link.enabled {
        return Err(SWITCHED_OFF.into());
    }
    if !owner_is_watching(st, session_id) {
        return Err(UNATTENDED.into());
    }
    Ok(link)
}

// ---------------------------------------------------------------------------
// What a bot may ask for

/// A page address the browser may open: http or https, a public host, no
/// credentials in the address, and nothing that is not a page at all.
pub(crate) fn validate_url(raw: &str) -> Result<String, String> {
    let text = raw.trim();
    if text.is_empty() {
        return Err("Give the page's address, like https://example.com.".into());
    }
    if text.chars().count() > URL_MAX {
        return Err("That address is too long to open.".into());
    }
    if text.chars().any(|ch| ch.is_control() || ch.is_whitespace()) {
        return Err("That address holds a space or a control character, which a web address cannot.".into());
    }
    let lower = text.to_ascii_lowercase();
    for scheme in ["file:", "javascript:", "vbscript:", "data:", "about:", "blob:", "chrome:", "chrome-extension:", "view-source:", "ftp:"] {
        if lower.starts_with(scheme) {
            return Err(format!("Retinue's browser opens pages over http:// and https:// only, not {}.", scheme.trim_end_matches(':')));
        }
    }
    let url = reqwest::Url::parse(text).map_err(|_| "That address is not a web address Retinue can open, with http:// or https:// in front.".to_string())?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(format!("Retinue's browser opens pages over http:// and https:// only, not {}.", url.scheme()));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("Leave the user name and password out of the address.".into());
    }
    let host = url.host_str().ok_or_else(|| "That address names no site.".to_string())?;
    if host_is_private(host) {
        return Err("That address is on the computer's own network, so the browser does not open it.".into());
    }
    Ok(url.to_string())
}

/// Whether a host is loopback, private, link-local, a cloud metadata address, or
/// a name on the container's own network rather than a site on the web.
fn host_is_private(host: &str) -> bool {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    if host.is_empty() {
        return true;
    }
    for suffix in ["localhost", ".localhost", ".local", ".internal", ".home.arpa", ".ts.net"] {
        if host == suffix.trim_start_matches('.') || host.ends_with(suffix) {
            return true;
        }
    }
    let bare = host.trim_start_matches('[').trim_end_matches(']');
    match bare.parse::<IpAddr>() {
        Ok(IpAddr::V4(v4)) => {
            let octets = v4.octets();
            octets[0] == 0
                || octets[0] == 10
                || octets[0] == 127
                || (octets[0] == 169 && octets[1] == 254)
                || (octets[0] == 172 && (16..=31).contains(&octets[1]))
                || (octets[0] == 192 && octets[1] == 168)
                // 100.64.0.0/10: the range Tailscale hands out.
                || (octets[0] == 100 && (64..=127).contains(&octets[1]))
                || (octets[0] == 192 && octets[1] == 0 && octets[2] == 0)
                || (octets[0] == 198 && (octets[1] == 18 || octets[1] == 19))
                || octets[0] >= 224
        }
        Ok(IpAddr::V6(v6)) => {
            let first = v6.segments()[0];
            v6.is_loopback()
                || v6.is_unspecified()
                || (first & 0xfe00) == 0xfc00
                || (first & 0xffc0) == 0xfe80
                || v6.to_ipv4_mapped().is_some_and(|v4| host_is_private(&v4.to_string()))
        }
        Err(_) => {
            // A name with no dot is a name on the container's own network, and a
            // bare number is an address written to look like a name.
            !host.contains('.') || host.parse::<u64>().is_ok()
        }
    }
}

/// An element reference from a snapshot: `e` and its number.
pub(crate) fn ref_of(raw: &str) -> Result<String, String> {
    let text = raw.trim();
    let digits = text.strip_prefix('e').filter(|digits| !digits.is_empty() && digits.len() <= 6 && digits.bytes().all(|byte| byte.is_ascii_digit()));
    digits.map(|_| text.to_owned()).ok_or_else(|| "ref must be one of the references a snapshot listed, like e12.".to_owned())
}

/// One key name, as Playwright spells it: `Enter`, `Escape`, `Control+A`.
pub(crate) fn key_of(raw: &str) -> Result<String, String> {
    let text = raw.trim();
    if text.is_empty() || text.chars().count() > KEY_MAX {
        return Err("key must be one key name, like Enter or Control+A.".into());
    }
    if !text.chars().all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '+' | '-' | '_' | ' ')) {
        return Err("key must be one key name, like Enter or Control+A.".into());
    }
    Ok(text.to_owned())
}

/// What typing may carry: one banner's worth of text, no NUL.
pub(crate) fn text_of(raw: &str) -> Result<String, String> {
    if raw.chars().count() > TEXT_MAX {
        return Err(format!("text must be at most {TEXT_MAX} characters; use a shorter one or a file."));
    }
    if raw.contains('\0') {
        return Err("text cannot hold a NUL character.".into());
    }
    if raw.trim().is_empty() {
        return Err("text is empty.".into());
    }
    Ok(raw.to_owned())
}

/// The conversation key the container uses for its context folder.
fn conversation_of(session_id: &str) -> Result<String, String> {
    let key: String = session_id.chars().filter(|ch| ch.is_ascii_alphanumeric() || *ch == '-' || *ch == '_').take(80).collect();
    if key.is_empty() {
        return Err("This conversation cannot use the browser.".into());
    }
    Ok(key)
}

// ---------------------------------------------------------------------------
// The page: what it shows, and what must pause

/// One element a page showed: what it is and what it says.
#[derive(Clone, Debug, Default)]
pub(crate) struct Element {
    tag: String,
    role: String,
    kind: String,
    name: String,
    autocomplete: String,
}

fn node_field(node: &Value, key: &str) -> String {
    node[key].as_str().unwrap_or("").to_ascii_lowercase()
}

pub(crate) fn element_of(node: &Value) -> Element {
    Element { tag: node_field(node, "tag"), role: node_field(node, "role"), kind: node_field(node, "type"), name: node_field(node, "name"), autocomplete: node_field(node, "autocomplete") }
}

pub(crate) fn elements_from(nodes: &Value) -> Vec<Element> {
    nodes.as_array().map(|rows| rows.iter().take(SNAPSHOT_MAX_NODES).map(element_of).collect()).unwrap_or_default()
}

fn mentions(text: &str, words: &[&str]) -> bool {
    words.iter().any(|word| text.contains(word))
}

/// Why the owner has to be asked before this page (or this element) is used, or
/// `None` when nothing in it is a sign-in, a 2FA prompt, a payment field or a
/// publish button. Conservative by design: a false pause costs one question, a
/// missed one lets a bot act as the owner.
pub(crate) fn pause_reason(elements: &[Element]) -> Option<String> {
    let is_button = |element: &Element| element.role == "button" || element.tag == "button" || element.kind == "submit";
    if elements.iter().any(|element| element.kind == "password" || mentions(&element.autocomplete, &["current-password", "new-password"])) {
        return Some("This page asks for a password.".into());
    }
    if elements.iter().any(|element| {
        element.autocomplete.starts_with("cc-")
            || mentions(&element.name, &["card number", "cardholder", "cvc", "cvv", "expiry date", "expiration date", "security code"])
    }) {
        return Some("This page asks for payment details.".into());
    }
    if elements.iter().any(|element| {
        element.autocomplete == "one-time-code"
            || mentions(&element.name, &["one-time code", "one time code", "verification code", "two-factor", "2fa", "authenticator", "code we sent", "code we texted", "enter the code"])
    }) {
        return Some("This page asks for a verification code.".into());
    }
    let names_a_field = elements.iter().any(|element| {
        element.kind == "email" || mentions(&element.name, &["email", "e-mail", "username", "user name", "sign in", "log in", "login"])
    });
    let names_a_button = elements.iter().any(|element| is_button(element) && mentions(&element.name, &["sign in", "log in", "login", "continue", "next"]));
    let names_a_heading = elements.iter().any(|element| (element.role == "heading" || matches!(element.tag.as_str(), "h1" | "h2" | "h3")) && mentions(&element.name, &["sign in", "log in", "sign-in", "log-in"]));
    if (names_a_field && names_a_button) || names_a_heading {
        return Some("This page is a sign-in form.".into());
    }
    if elements.iter().any(|element| is_button(element) && mentions(&element.name, &["post", "publish", "submit", "send", "comment", "reply", "share", "place order", "pay now", "confirm order"])) {
        return Some("This page would post or submit something.".into());
    }
    None
}

fn needs_owner(reason: &str) -> Value {
    json!({
        "needs_owner": true,
        "reason": reason,
        "instruction": "Stop and ask the owner with ask_user, then end your turn. Do not retry this step yourself.",
    })
}

/// The host of a page address, for the one line this module records.
fn host_of(url: &str) -> String {
    reqwest::Url::parse(url).ok().and_then(|url| url.host_str().map(str::to_owned)).unwrap_or_default()
}

/// Percent-encodes a value the way a form or a query string would, so a value
/// that leaked into an address is still replaced. Letters, digits and `-_.~`
/// stay as they are.
fn percent_encode(value: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(value.len() * 3);
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(char::from(byte));
        } else {
            out.push('%');
            out.push(char::from(HEX[usize::from(byte >> 4)]));
            out.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Talking to the container

/// One link's address and token, for the length of one call.
struct Container {
    address: String,
    token: Zeroizing<String>,
}

impl Container {
    /// The one HTTP client every call uses: no redirect (it would carry the
    /// bearer token to another origin), no proxy, Retinue's User-Agent.
    fn client() -> Result<reqwest::Client, String> {
        reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .connect_timeout(PROBE_TIMEOUT)
            .user_agent(concat!("Retinue/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|_| "The request to the computer could not be prepared.".to_owned())
    }

    /// One command inside the container. The output is Harbor's own text; a
    /// failed command comes back as its one bounded line.
    async fn exec(&self, command: String, limit: Duration) -> Result<String, String> {
        let client = Self::client()?;
        let body = json!({"command": command, "timeout_ms": u64::try_from(limit.as_millis()).unwrap_or(EXEC_MAX_MS).min(EXEC_MAX_MS)});
        let response = client
            .post(format!("{}{API}/exec", self.address))
            .header(reqwest::header::AUTHORIZATION, format!("Bearer {}", self.token.as_str()))
            .header("X-Harbor-Client", "mcp")
            .header(reqwest::header::ACCEPT, "application/json")
            .json(&body)
            .timeout(limit + EXEC_MARGIN)
            .send()
            .await
            .map_err(|_| UNREACHABLE.to_owned())?;
        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Err(TOKEN_REFUSED.into());
        }
        if !response.status().is_success() {
            return Err(format!("The computer answered HTTP {}.", response.status().as_u16()));
        }
        let answer: Value = response.json().await.map_err(|_| "The computer sent an answer Retinue could not read.".to_owned())?;
        let ok = answer["ok"].as_bool().or_else(|| answer["code"].as_i64().map(|code| code == 0)).unwrap_or(false);
        let output = answer["output"].as_str().unwrap_or("").to_owned();
        if !ok {
            let line = one_line(&output, 300);
            return Err(if line.is_empty() { "The command did not finish in the computer.".into() } else { line });
        }
        Ok(output)
    }
}

fn container_for(link: &ComputerLink) -> Result<Container, String> {
    let token = match crate::secrets::store::read(&link.token_ref) {
        Ok(Some(token)) => token,
        Ok(None) => return Err("The computer's token is missing from the credential store. Ask the owner to link the computer again.".into()),
        Err(_) => return Err("The credential store is unavailable, so the computer can't be reached.".into()),
    };
    Ok(Container { address: link.address.clone(), token })
}

/// The command that hands one JSON request to the container's browser server.
/// The body is base64, so the typed text never appears as words in the command
/// the computer runs.
fn action_command(request: &Value) -> String {
    let encoded = base64::engine::general_purpose::STANDARD.encode(request.to_string());
    format!("printf '%s' '{encoded}' | base64 -d | curl -sS --max-time 90 -H 'content-type: application/json' --data-binary @- http://127.0.0.1:{CONTAINER_PORT}/act")
}

/// Writes the server script when it is missing or old, starts it, and echoes
/// `BROWSER_READY` once it answers.
fn bootstrap_command() -> String {
    let encoded = base64::engine::general_purpose::STANDARD.encode(SERVER_SCRIPT);
    format!(
        "D={CONTAINER_DIR}; mkdir -p \"$D/profile\"; if [ ! -f \"$D/server.mjs\" ] || ! grep -q {SCRIPT_MARKER} \"$D/server.mjs\" 2>/dev/null; then printf '%s' '{encoded}' | base64 -d > \"$D/server.mjs\"; chmod 600 \"$D/server.mjs\"; fi; if ! curl -fsS --max-time 2 http://127.0.0.1:{CONTAINER_PORT}/health >/dev/null 2>&1; then cd \"$D\" && (nohup node server.mjs > server.log 2>&1 &); sleep 2; fi; curl -fsS --max-time 3 http://127.0.0.1:{CONTAINER_PORT}/health >/dev/null 2>&1 && echo {READY_MARK}"
    )
}

/// Whether Playwright is in the container and the server answers.
fn probe_command() -> String {
    format!(
        "D={CONTAINER_DIR}; if [ ! -d \"$D/node_modules/playwright\" ]; then echo {MISSING_MARK}; elif curl -fsS --max-time 2 http://127.0.0.1:{CONTAINER_PORT}/health >/dev/null 2>&1; then echo {RUNNING_MARK}; else echo BROWSER_STOPPED; fi"
    )
}

fn not_ready() -> String {
    format!("{NOT_READY} Playwright is not installed there. Run this once in the computer: {INSTALL_COMMAND}")
}

async fn bootstrap(container: &Container) -> Result<(), String> {
    match container.exec(bootstrap_command(), BOOT_TIMEOUT).await {
        Ok(output) if output.contains(READY_MARK) => Ok(()),
        // An unreachable computer or a refused token is said plainly; anything
        // else means the container's Node could not start, which is the install
        // step the owner has to run.
        Err(reason) if reason == UNREACHABLE || reason == TOKEN_REFUSED => Err(reason),
        _ => Err(not_ready()),
    }
}

/// One action. Starting the server is the caller's first fallback, so a stopped
/// container is not a failed tool call.
async fn act(container: &Container, request: &Value) -> Result<Value, String> {
    let output = container.exec(action_command(request), ACTION_TIMEOUT).await?;
    let text = output.trim();
    if text.is_empty() {
        return Err(NOT_READY.into());
    }
    serde_json::from_str::<Value>(text).map_err(|_| "The browser sent an answer Retinue could not read.".to_owned())
}

/// One action, starting the container's server once if it is not running.
async fn act_with_server(container: &Container, request: Value) -> Result<Value, String> {
    match act(container, &request).await {
        Ok(value) => Ok(value),
        Err(_) => {
            bootstrap(container).await?;
            act(container, &request).await
        }
    }
}

// ---------------------------------------------------------------------------
// The conversation's context

impl Browser {
    /// Whether a context is already tracked for this conversation.
    fn track(&self, session_id: &str) {
        let now = Instant::now();
        let mut open = self.open.lock().unwrap_or_else(|poison| poison.into_inner());
        open.insert(session_id.to_owned(), Opened { last_used: now, #[cfg(test)] stale: false });
    }

    fn forget(&self, session_id: &str) {
        let mut open = self.open.lock().unwrap_or_else(|poison| poison.into_inner());
        open.remove(session_id);
        self.known.lock().unwrap_or_else(|poison| poison.into_inner()).remove(session_id);
    }

    #[cfg(test)]
    pub(crate) fn forget_for_tests(&self, session_id: &str) {
        self.forget(session_id);
    }

    /// Remembers a value the vault typed into this conversation's page, and the
    /// reference it went into, so neither is ever echoed back.
    pub(crate) fn remember(&self, session_id: &str, value: &str, reference: Option<&str>) {
        if value.is_empty() {
            return;
        }
        let mut known = self.known.lock().unwrap_or_else(|poison| poison.into_inner());
        let entry = known.entry(session_id.to_owned()).or_default();
        if entry.values.len() >= KNOWN_MAX {
            entry.values.remove(0);
        }
        entry.values.push(Zeroizing::new(value.to_owned()));
        if let Some(reference) = reference.filter(|reference| !reference.is_empty()) {
            if entry.refs.len() >= KNOWN_MAX {
                entry.refs.remove(0);
            }
            if !entry.refs.iter().any(|known| known == reference) {
                entry.refs.push(reference.to_owned());
            }
        }
    }

    /// The references whose value must never be shown for this conversation.
    fn masked_refs(&self, session_id: &str) -> Vec<String> {
        self.known.lock().unwrap_or_else(|poison| poison.into_inner()).get(session_id).map(|entry| entry.refs.clone()).unwrap_or_default()
    }

    /// Replaces every value this conversation filled with `[redacted]`, in any
    /// text about to reach the bot: element names, the page's text, a title, a
    /// URL. The plain form and its percent-encoded and JSON-escaped forms are
    /// matched; a value the page draws in another shape cannot be (report 04).
    pub(crate) fn redact(&self, session_id: &str, text: &str) -> String {
        let values = {
            let known = self.known.lock().unwrap_or_else(|poison| poison.into_inner());
            known.get(session_id).map(|entry| entry.values.clone()).unwrap_or_default()
        };
        if values.is_empty() || text.is_empty() {
            return text.to_owned();
        }
        let mut ordered: Vec<String> = values.iter().map(|value| value.to_string()).filter(|value| !value.is_empty()).collect();
        // Longest first, so a value that contains another is replaced whole.
        ordered.sort_by_key(|value| std::cmp::Reverse(value.len()));
        let mut out = text.to_owned();
        for value in ordered {
            out = out.replace(&value, "[redacted]");
            let escaped = serde_json::to_string(&value).map(|quoted| quoted.trim_matches('"').to_owned()).unwrap_or_default();
            if !escaped.is_empty() && escaped != value {
                out = out.replace(&escaped, "[redacted]");
            }
            let encoded = percent_encode(&value);
            if !encoded.is_empty() && encoded != value {
                out = out.replace(&encoded, "[redacted]");
            }
        }
        out
    }

    fn contexts(&self) -> usize {
        self.open.lock().map(|open| open.len()).unwrap_or(0)
    }

    #[cfg(test)]
    pub(crate) fn contexts_for_tests(&self) -> usize {
        self.contexts()
    }

    #[cfg(test)]
    pub(crate) fn age_for_tests(&self, session_id: &str, by: Duration) {
        let mut open = self.open.lock().unwrap();
        if let Some(entry) = open.get_mut(session_id) {
            match entry.last_used.checked_sub(by) {
                Some(earlier) => entry.last_used = earlier,
                None => entry.stale = by >= IDLE_LIMIT,
            }
        }
    }

    /// Forgets the cached readiness, so a test can make the container answer
    /// differently without waiting out the half-minute.
    #[cfg(test)]
    pub(crate) fn reset_readiness_for_tests(&self) {
        if let Ok(mut cached) = self.readiness.lock() {
            *cached = None;
        }
    }
}

/// The conversation's turn is over when it is no longer working: the context is
/// closed rather than left holding a page between turns.
fn turn_is_over(st: &AppState, session_id: &str) -> bool {
    match st.db.session_detail(session_id) {
        Ok(session) => matches!(session["state"].as_str().unwrap_or("idle"), "idle" | "completed" | "cancelled" | "error" | "closed"),
        Err(_) => true,
    }
}

/// Closes one conversation's context. A failure is not worth reporting: the
/// container may be asleep, and the sweep will try again.
async fn close_context(st: &AppState, session_id: &str) {
    st.browser.forget(session_id);
    let Ok(Some(link)) = st.db.computer_link() else { return };
    if !link.enabled {
        return;
    }
    let Ok(container) = container_for(&link) else { return };
    let Ok(conversation) = conversation_of(session_id) else { return };
    let request = json!({"action": "close", "conversation": conversation});
    let _ = act(&container, &request).await;
}

/// The idle and turn-end sweep: a context that lost its turn, or has been
/// untouched for ten minutes, is closed.
pub(crate) async fn sweep_loop(st: AppState) {
    let mut ticker = tokio::time::interval(SWEEP_INTERVAL);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            _ = ticker.tick() => sweep_once(&st).await,
            _ = st.stopping.notified() => return,
        }
    }
}

/// One pass of the sweep, so a test can make time pass without waiting.
pub(crate) async fn sweep_once(st: &AppState) {
    let stale: Vec<String> = {
        let mut open = match st.browser.open.lock() {
            Ok(open) => open,
            Err(poison) => poison.into_inner(),
        };
        let expired: Vec<String> = open
            .iter()
            .filter(|(session_id, entry)| entry.idle() || turn_is_over(st, session_id))
            .map(|(session_id, _)| session_id.clone())
            .collect();
        for session_id in &expired {
            open.remove(session_id);
        }
        expired
    };
    for session_id in stale {
        close_context(st, &session_id).await;
    }
}

// ---------------------------------------------------------------------------
// Readiness: `browser.status` and the field on `computer.status`

/// Reads whether the browser can start now. An unreachable or asleep computer
/// is not ready, and a container without Playwright names the install command.
async fn probe(st: &AppState) -> (bool, Value) {
    let Some(link) = st.db.computer_link().ok().flatten().filter(|link| link.enabled) else { return (false, Value::Null) };
    let Ok(container) = container_for(&link) else { return (false, Value::Null) };
    match container.exec(probe_command(), PROBE_TIMEOUT).await {
        Ok(output) if output.contains(RUNNING_MARK) => (true, Value::Null),
        Ok(output) if output.contains(MISSING_MARK) => (false, json!(INSTALL_COMMAND)),
        _ => (false, Value::Null),
    }
}

/// The readiness, cached for half a minute so a status poll is cheap.
async fn readiness(st: &AppState) -> (bool, Value) {
    {
        let cached = st.browser.readiness.lock().unwrap_or_else(|poison| poison.into_inner());
        if let Some((at, ready, missing)) = cached.as_ref() {
            if at.elapsed() < READINESS_TTL {
                return (*ready, missing.clone());
            }
        }
    }
    let (ready, missing) = probe(st).await;
    let mut cached = st.browser.readiness.lock().unwrap_or_else(|poison| poison.into_inner());
    *cached = Some((Instant::now(), ready, missing.clone()));
    (ready, missing)
}

/// Announces readiness only when it changed, so a poll never floods the window.
/// The first read only teaches the daemon what to compare against: the window
/// already has that value from the reply it asked for, and emitting then would
/// turn an ordinary `computer.status` into a change nobody made.
async fn announce(st: &AppState, browser: Value) {
    let changed = {
        let mut announced = st.browser.announced.lock().unwrap_or_else(|poison| poison.into_inner());
        let changed = match announced.as_ref() {
            None => false,
            Some(previous) => previous != &browser,
        };
        if changed || announced.is_none() {
            *announced = Some(browser.clone());
        }
        changed
    };
    if changed {
        st.emit("browser.changed", browser).await;
    }
}

/// Adds `browser: {ready, missing}` to a `computer.status` reply. A status that
/// is not an awake, switched-on computer reads not ready without a probe.
pub async fn add_status(st: &AppState, status: &mut Value) {
    let awake = status["linked"] == true && status["enabled"] == true && status["state"] == "awake";
    let browser = if awake {
        let (ready, missing) = readiness(st).await;
        json!({"ready": ready, "missing": missing})
    } else {
        json!({"ready": false, "missing": Value::Null})
    };
    if let Some(object) = status.as_object_mut() {
        object.insert("browser".into(), browser.clone());
    }
    announce(st, browser).await;
}

// ---------------------------------------------------------------------------
// RPC

pub async fn rpc(st: &AppState, method: &str, _p: &Value) -> Result<Value, DispatchError> {
    match method {
        "browser.status" => {
            let (ready, missing) = readiness(st).await;
            announce(st, json!({"ready": ready, "missing": missing})).await;
            Ok(json!({"ready": ready, "missing": missing, "contexts": st.browser.contexts(), "checkedAt": now_iso()}))
        }
        _ => Err(derr("not_found", "unknown browser method", StatusCode::NOT_FOUND)),
    }
}

// ---------------------------------------------------------------------------
// The tools

/// The seven tools a conversation gets while the computer is linked and on.
pub fn tool_schemas() -> Vec<Value> {
    vec![
        json!({"name":"browser_open","description":"Open a web page in your own browser on the bot computer. Pages are reached over http and https only; a page with a sign-in form, a password field, a verification code, payment details or a post button pauses and asks the owner.","inputSchema":{"type":"object","properties":{"url":{"type":"string","description":"The page's address, starting with http:// or https://."}},"required":["url"],"additionalProperties":false}}),
        json!({"name":"browser_snapshot","description":"Read the page you are on: its address and title, the elements you can act on with a reference for each, and its text. Use the references with browser_click and browser_type, and take a new snapshot after the page changes.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}}),
        json!({"name":"browser_click","description":"Click one element by the reference a snapshot listed. A sign-in, payment or post button pauses and asks the owner instead.","inputSchema":{"type":"object","properties":{"ref":{"type":"string","description":"A reference from the last snapshot, like e12."}},"required":["ref"],"additionalProperties":false}}),
        json!({"name":"browser_type","description":"Type into one field by the reference a snapshot listed. The text is not recorded; only how many characters were typed. A password or payment field pauses and asks the owner instead.","inputSchema":{"type":"object","properties":{"ref":{"type":"string","description":"A reference from the last snapshot, like e3."},"text":{"type":"string","description":"The text to type, at most 2,000 characters."}},"required":["ref","text"],"additionalProperties":false}}),
        json!({"name":"browser_press","description":"Press one key on the page, like Enter, Escape or Control+A.","inputSchema":{"type":"object","properties":{"key":{"type":"string","description":"One key name, like Enter or Control+A."}},"required":["key"],"additionalProperties":false}}),
        json!({"name":"browser_back","description":"Go back one page in the browser's history.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}}),
        json!({"name":"browser_screenshot","description":"Take one picture of the page and attach it to this conversation as an image. Use it only when the page's elements do not answer the question.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}}),
    ]
}

/// Whether the browser is still on a page the bot may see: a public http(s)
/// address. A page that redirects into a local, link-local or cloud-metadata
/// address is refused here, because only the address the bot asked for passes
/// `validate_url`.
fn landed_on_a_page(url: &str) -> bool {
    let Ok(parsed) = reqwest::Url::parse(url) else { return false };
    matches!(parsed.scheme(), "http" | "https") && parsed.host_str().is_some_and(|host| !host_is_private(host))
}

/// What a page-bearing reply ends with: the owner must be asked, or the bot gets
/// the page.
fn page_outcome(reply: &Value, tool: &str, st: &AppState, session_id: &str) -> Result<Value, String> {
    if let Some(reason) = pause_reason(&elements_from(&reply["nodes"])) {
        return Ok(needs_owner(&reason));
    }
    let url = reply["url"].as_str().unwrap_or("");
    // Whatever the page did, the container now holds a context for this
    // conversation: record it here, before any refusal, so the sweep closes it.
    st.browser.track(session_id);
    if !landed_on_a_page(url) {
        return Err(REDIRECTED.into());
    }
    let host = host_of(url);
    activity(tool, &host, 0);
    let mut elements = Vec::new();
    for node in reply["nodes"].as_array().map(Vec::as_slice).unwrap_or_default().iter().take(SNAPSHOT_MAX_NODES) {
        if node["visible"] == false {
            continue;
        }
        elements.push(json!({
            "ref": node["ref"],
            "role": node["role"],
            "name": st.browser.redact(session_id, node["name"].as_str().unwrap_or("")),
        }));
    }
    let tree = reply["tree"].as_str().unwrap_or("").chars().take(SNAPSHOT_MAX_CHARS).collect::<String>();
    Ok(json!({
        "ok": true,
        "url": st.browser.redact(session_id, url),
        "title": st.browser.redact(session_id, reply["title"].as_str().unwrap_or("")),
        "elements": elements,
        "text": st.browser.redact(session_id, &tree),
        "hint": "References stay valid until the page changes; take a new snapshot after that.",
    }))
}

/// The one content-free line each action leaves: the action class, the host, and
/// for typing only a count. Never the page's text, the typed text or a reference.
fn activity(tool: &str, host: &str, typed: usize) {
    match tool {
        "browser_open" => tracing::info!("vps-browser: opened {host}"),
        "browser_click" => tracing::info!("vps-browser: clicked on {host}"),
        "browser_type" => tracing::info!("vps-browser: typed {typed} characters"),
        "browser_fill" => tracing::info!("vps-browser: filled a saved value"),
        "browser_press" => tracing::info!("vps-browser: pressed a key on {host}"),
        "browser_back" => tracing::info!("vps-browser: went back on {host}"),
        "browser_snapshot" => tracing::debug!("vps-browser: read {host}"),
        "browser_screenshot" => tracing::info!("vps-browser: took a screenshot of {host}"),
        _ => {}
    }
}

/// The failure the browser server reported, worded for the bot.
fn server_failure(reply: &Value, typing: bool) -> String {
    if reply["stale"] == true {
        return STALE.into();
    }
    // Playwright's message can name the selector and the page's own text; for
    // typing it is never passed on, so no echo of the value can reach the log.
    if typing {
        return "The page would not accept typing in that field.".into();
    }
    let message = one_line(reply["error"].as_str().unwrap_or(""), 200);
    if message.is_empty() {
        "The page did not answer that action.".into()
    } else {
        format!("The page refused that action: {message}")
    }
}

/// One tool call: the gates, then the action, then the pause.
async fn call_tool(st: &AppState, session_id: &str, name: &str, args: &Value) -> Result<Value, String> {
    if !tool_schemas().iter().any(|tool| tool["name"].as_str() == Some(name)) {
        return Err(format!("Unknown tool {name}."));
    }
    let link = gate(st, session_id)?;
    let conversation = conversation_of(session_id)?;
    // Every argument is judged before anything reaches the computer.
    let url = if name == "browser_open" { Some(validate_url(args["url"].as_str().unwrap_or(""))?) } else { None };
    let reference = match name {
        "browser_click" | "browser_type" => Some(ref_of(args["ref"].as_str().unwrap_or(""))?),
        _ => None,
    };
    let key = if name == "browser_press" { Some(key_of(args["key"].as_str().unwrap_or(""))?) } else { None };
    let text = if name == "browser_type" { Some(text_of(args["text"].as_str().unwrap_or(""))?) } else { None };
    let container = container_for(&link)?;
    // The references the vault filled: the page masks their value, and it is
    // redacted from anything that comes back.
    let mask = || json!(st.browser.masked_refs(session_id));
    match name {
        "browser_open" => {
            let request = json!({"action": "open", "conversation": conversation, "url": url.unwrap_or_default(), "mask": mask()});
            let reply = act_with_server(&container, request).await?;
            if reply["ok"] == false {
                return Err(server_failure(&reply, false));
            }
            page_outcome(&reply, name, st, session_id)
        }
        "browser_snapshot" => {
            let reply = act_with_server(&container, json!({"action": "snapshot", "conversation": conversation, "mask": mask()})).await?;
            if reply["ok"] == false {
                return Err(server_failure(&reply, false));
            }
            page_outcome(&reply, name, st, session_id)
        }
        "browser_click" => {
            let reference = reference.unwrap_or_default();
            let described = act_with_server(&container, json!({"action": "describe", "conversation": conversation, "ref": reference, "mask": mask()})).await?;
            if described["stale"] == true {
                return Err(STALE.into());
            }
            // The pause happens before the click, never after it.
            if let Some(reason) = pause_reason(&[element_of(&described["target"])]) {
                return Ok(needs_owner(&reason));
            }
            let reply = act_with_server(&container, json!({"action": "click", "conversation": conversation, "ref": reference, "mask": mask()})).await?;
            if reply["ok"] == false {
                return Err(server_failure(&reply, false));
            }
            page_outcome(&reply, name, st, session_id)
        }
        "browser_type" => {
            let reference = reference.unwrap_or_default();
            let text = text.unwrap_or_default();
            let described = act_with_server(&container, json!({"action": "describe", "conversation": conversation, "ref": reference, "mask": mask()})).await?;
            if described["stale"] == true {
                return Err(STALE.into());
            }
            if let Some(reason) = pause_reason(&[element_of(&described["target"])]) {
                return Ok(needs_owner(&reason));
            }
            let typed = text.chars().count();
            let reply = act_with_server(&container, json!({"action": "type", "conversation": conversation, "ref": reference, "text": text, "mask": mask()})).await?;
            if reply["ok"] == false {
                return Err(server_failure(&reply, true));
            }
            st.browser.track(session_id);
            activity(name, "", typed);
            let element = element_of(&described["target"]);
            Ok(json!({"ok": true, "typed": typed, "field": st.browser.redact(session_id, &element.name)}))
        }
        "browser_press" => {
            let key = key.unwrap_or_default();
            // The active element is described before a key is pressed, so a form's
            // own submit is caught here too.
            let described = act_with_server(&container, json!({"action": "describe", "conversation": conversation, "ref": "", "mask": mask()})).await?;
            if let Some(reason) = pause_reason(&[element_of(&described["target"])]) {
                return Ok(needs_owner(&reason));
            }
            let reply = act_with_server(&container, json!({"action": "press", "conversation": conversation, "key": key, "mask": mask()})).await?;
            if reply["ok"] == false {
                return Err(server_failure(&reply, false));
            }
            page_outcome(&reply, name, st, session_id)
        }
        "browser_back" => {
            let reply = act_with_server(&container, json!({"action": "back", "conversation": conversation, "mask": mask()})).await?;
            if reply["ok"] == false {
                return Err(server_failure(&reply, false));
            }
            page_outcome(&reply, name, st, session_id)
        }
        "browser_screenshot" => {
            let reply = act_with_server(&container, json!({"action": "screenshot", "conversation": conversation})).await?;
            if reply["ok"] == false {
                return Err(server_failure(&reply, false));
            }
            if reply["tooLarge"] == true {
                return Err("That page is too large to send as one picture.".into());
            }
            if reply["mediaType"].as_str() != Some("image/png") {
                return Err("The browser did not send a PNG.".into());
            }
            let encoded = reply["base64"].as_str().unwrap_or("");
            if encoded.is_empty() || encoded.len() > SCREENSHOT_MAX_BYTES / 3 * 4 + 64 {
                return Err("That page is too large to send as one picture.".into());
            }
            let url = reply["url"].as_str().unwrap_or("");
            st.browser.track(session_id);
            if !landed_on_a_page(url) {
                return Err(REDIRECTED.into());
            }
            let host = host_of(url);
            let call_id = format!("browser-{}", Uuid::new_v4());
            let turn = st.db.session_latest_turn(session_id).ok().flatten().filter(|turn| turn["completedAt"].is_null()).and_then(|turn| turn["id"].as_str().map(str::to_owned));
            // The generated-image path: one row, one notice, no path in the meta
            // and never the bytes in this reply.
            crate::media::requested(st, session_id, turn.as_deref(), "browser", &call_id, None, None, Some(encoded.to_owned()), None).await;
            // The media layer owns the row's id, and that id is what the window
            // looks the picture up by; the call id is only how the row is found.
            let Some(row) = st.db.media_by_call(session_id, &call_id).ok().flatten() else {
                return Err("The picture could not be stored on this computer.".into());
            };
            let media_id = row["id"].as_str().filter(|id| !id.is_empty()).unwrap_or(&call_id).to_owned();
            if row["status"] != "ready" {
                return Err("The picture could not be stored on this computer.".into());
            }
            activity(name, &host, 0);
            Ok(json!({
                "ok": true,
                "mediaId": media_id,
                "caption": format!("Screenshot of {host}"),
                "note": "The picture is attached to this conversation as an image.",
            }))
        }
        other => Err(format!("Unknown tool {other}.")),
    }
}

/// The vault's fill (16 November 2026): a saved value goes into one element of
/// this conversation's page, and the bot never sees it. The gates are the
/// browser's own, then the page's origin must be exactly the item's origin, and
/// the container writes the value. The reply carries no page and no value.
pub(crate) async fn fill_secret(st: &AppState, session_id: &str, origin: &str, reference: &str, value: &str) -> Result<(), crate::vault::FillRefusal> {
    use crate::vault::FillRefusal;
    let link = gate(st, session_id).map_err(FillRefusal::Refused)?;
    let conversation = conversation_of(session_id).map_err(FillRefusal::Refused)?;
    let container = container_for(&link).map_err(FillRefusal::Refused)?;
    let described = act_with_server(
        &container,
        json!({"action": "describe", "conversation": conversation, "ref": reference, "mask": st.browser.masked_refs(session_id)}),
    )
    .await
    .map_err(FillRefusal::Refused)?;
    if described["stale"] == true {
        return Err(FillRefusal::Refused(STALE.into()));
    }
    // The page must be the item's own origin. A page that cannot be read is a
    // mismatch, never an allowance.
    let page = described["url"].as_str().and_then(crate::vault::origin_of_url);
    let wanted = crate::vault::origin_of_url(origin);
    if page.is_none() || wanted.is_none() || page != wanted {
        return Err(FillRefusal::WrongOrigin);
    }
    // Once this request is sent the value may be in the page, so an error from
    // here on is "uncertain", never "refused".
    let reply = act_with_server(&container, json!({"action": "fill", "conversation": conversation, "ref": reference, "text": value})).await;
    match reply {
        Ok(reply) if reply["ok"] == false => Err(FillRefusal::Uncertain(server_failure(&reply, true))),
        Ok(reply) if reply["filled"] == true => {
            st.browser.track(session_id);
            st.browser.remember(session_id, value, Some(reference));
            activity("browser_fill", "", value.chars().count());
            Ok(())
        }
        Ok(_) => Err(FillRefusal::Uncertain("The page did not confirm the fill.".into())),
        Err(text) => Err(FillRefusal::Uncertain(text)),
    }
}

fn value_text(value: &Value) -> String {    match value {
        Value::String(text) => text.clone(),
        other => serde_json::to_string_pretty(other).unwrap_or_default(),
    }
}

/// MCP calls from `retinued browser-server`, authorised by the conversation's own
/// capability (the same one the team and computer bridges use).
pub async fn mcp_rpc(st: &AppState, session_id: &str, method: &str, params: &Value) -> Result<Value, DispatchError> {
    match method {
        "mcp.browser.list" => Ok(json!({"tools": tool_schemas()})),
        "mcp.browser.call" => {
            let name = params["name"].as_str().unwrap_or("");
            if name.is_empty() || name.len() > 40 || !name.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'_') {
                return Ok(json!({"content":[{"type":"text","text":"Unknown tool."}],"isError":true}));
            }
            // One content-free line per call: the tool's own name, never its
            // arguments, its output or the page.
            tracing::debug!("vps-browser tool call: {name}");
            Ok(match call_tool(st, session_id, name, &params["arguments"]).await {
                Ok(value) => json!({"content":[{"type":"text","text":crate::computer::cap_output(value_text(&value))}]}),
                Err(text) => json!({"content":[{"type":"text","text":crate::computer::cap_output(text)}],"isError":true}),
            })
        }
        _ => Err(derr("not_found", "unknown browser method", StatusCode::NOT_FOUND)),
    }
}

// ---------------------------------------------------------------------------
// The MCP server a new conversation gets

fn endpoint_path(dir: &Path, session_id: &str) -> PathBuf {
    let safe: String = session_id.chars().filter(|ch| ch.is_ascii_alphanumeric() || *ch == '-' || *ch == '_').take(80).collect();
    let safe = if safe.is_empty() { Uuid::new_v4().to_string() } else { safe };
    dir.join(ENDPOINT_FOLDER).join(format!("mcp-{safe}.json"))
}

/// Creates the endpoint file empty, restricts it to the current user, and only
/// then writes the address and capability, so it is never readable by others.
fn write_endpoint(path: &Path, address: &str, token: &str) -> Result<(), String> {
    use std::io::Write;
    if token.len() > TOKEN_MAX || token.is_empty() {
        return Err("the conversation's capability is not usable".to_owned());
    }
    let body = json!({"address": address, "token": token}).to_string();
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

/// Deletes every endpoint file in the folder: a stale file names a capability a
/// restart ended.
pub(crate) fn sweep_endpoints(data_dir: &Path) {
    let dir = data_dir.join(ENDPOINT_FOLDER);
    let Ok(entries) = std::fs::read_dir(&dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() {
            let _ = std::fs::remove_file(&path);
        }
    }
}

/// The `vps-browser` server for one conversation, or `None` when the computer is
/// not linked, is switched off, or the endpoint file could not be written. The
/// spec carries no token: the bridge reads the endpoint file, and every call is
/// made here.
pub fn mcp_server_for(st: &AppState, session_id: &str) -> Option<McpServerSpec> {
    st.db.computer_link().ok().flatten().filter(|link| link.enabled)?;
    let address = st.team.address.lock().ok().and_then(|address| address.clone())?;
    let dir = data_dir(st)?;
    let executable = std::env::current_exe().ok()?;
    let endpoint = endpoint_path(&dir, session_id);
    if write_endpoint(&endpoint, &address, &st.team.token_for(session_id)).is_err() {
        tracing::warn!("the browser bridge's endpoint file could not be written");
        return None;
    }
    Some(McpServerSpec {
        name: SERVER_NAME.into(),
        command: executable,
        args: vec!["browser-server".into(), "--endpoint".into(), endpoint.to_string_lossy().into_owned()],
        env: BTreeMap::new(),
    })
}
