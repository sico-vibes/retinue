//! The owner's own Windows computer, and the `owner_*` tools bots get for it.
//!
//! This is Phase C of `docs/research/owner-computer-driver-2026-11-14.md`
//! (section 3) and the "browsers and computer control" amendment in `AGENTS.md`
//! (14 November 2026). Nothing here runs unless the owner turns it on, approves a
//! card in that conversation, and is sitting in a conversation that can answer a
//! card at all.
//!
//! The driver. `open-computer-use` v1.1.4 (MIT) is a stdio MCP server that drives
//! Windows UI Automation through an embedded PowerShell bridge. Retinue runs it as
//! a supervised child, one process per tool call, pinned by SHA-256 (the release
//! zip and the extracted `.exe`), staged next to the daemon by
//! `scripts/fetch-owner-computer-driver.mjs`. The child is started from a cleared
//! environment holding only paths, so
//! `OPEN_COMPUTER_USE_ALLOW_GLOBAL_POINTER_FALLBACKS` can never be inherited: the
//! driver's real-pointer and real-keyboard modes stay off, and every action stays
//! on UI Automation patterns or window messages, which do not steal the owner's
//! foreground. The exe itself is never committed; its licence, version and both
//! hashes are in `docs/operations/owner-computer-driver.md`.
//!
//! The gate, in the order a call passes it:
//!
//! 1. `experimental.ownerComputer` is on (off by default, never exported in a
//!    backup, never remote). Off: this module offers no MCP server at all, so no
//!    tool exists to call.
//! 2. The conversation is attended: not archived, not an automation or an
//!    unattended board task, not a side conversation another bot is driving, and
//!    not Full access. Full access removes approval prompts, and this is the one
//!    capability that must keep asking.
//! 3. The app is not on the deny-list (password managers, banking, mail,
//!    terminals), matched on the app name and then on the window title.
//! 4. A grant is live for this conversation, or the owner answers a consent card
//!    ("Allow for this conversation", "Once", "No"). The grant is memory only and
//!    is never rememberable as a rule. A grant lasts 5 minutes, 60 seconds of no
//!    action ends it, and no grant outlives 15 minutes of one turn.
//! 5. An action needs a screen read first, of that app, with no action since:
//!    element indexes do not survive an observation, so a stale handle is refused
//!    rather than retried against a different control.
//!
//! What never leaves this module: the typed text (the result carries a count), the
//! accessibility tree (used in the turn, never persisted, never logged), a
//! screenshot (there is no screenshot tool in this cut), and nothing at all in a
//! backup, a diagnostics bundle or a push payload. One content-free sentence per
//! call says the app, the action class and a count.
//!
//! Every method is `never` in `tiers.json` and `owner_computer.grant` is `drop`, so
//! nothing here is reachable from a paired device or the Retinue MCP.

use crate::{derr, AppState, DispatchError};
use chrono::{SecondsFormat, Utc};
use retinue_agent_core::McpServerSpec;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
    sync::Mutex as StdMutex,
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, Command},
    sync::oneshot,
};
use uuid::Uuid;

/// The MCP server's name, as the provider sees it.
pub const SERVER_NAME: &str = "owner-computer";
/// The owner's switch. Absent or false means this whole module is off.
pub const SETTING: &str = "experimental.ownerComputer";
/// The editable deny-list: app names and window-title words, matched as
/// lower-case substrings. Absent means the starter list below.
pub const DENY_LIST_SETTING: &str = "ownerComputer.denyList";
/// The driver's own switch for real pointer and real keyboard input. Retinue
/// never sets it and strips it from the child's environment.
pub const GLOBAL_POINTER_VAR: &str = "OPEN_COMPUTER_USE_ALLOW_GLOBAL_POINTER_FALLBACKS";
/// The pinned driver: the v1.1.4 release assets, taken at fetch time and recorded
/// in `docs/operations/owner-computer-driver.md`.
pub const DRIVER_VERSION: &str = "1.1.4";
pub const DRIVER_ZIP_SHA256: &str = "6635efc579ee3ac1f8b26cfb67e31ccb0ee88f1aa5c587359d17daa780e6817a";
pub const DRIVER_EXE_SHA256: &str = "c8ca36d8871f02e621829f6024a794f186c779217a02b9b5cc60161917e90854";
/// The driver's file name, next to the daemon or under `<data>/owner-computer/`.
const DRIVER_FILE: &str = "open-computer-use.exe";
/// Where this module's endpoint files live, inside the data folder.
const ENDPOINT_FOLDER: &str = "owner-computer";

/// An app name may be at most this long.
const APP_MAX: usize = 200;
/// How much text one element may carry back: the driver's own default.
const TEXT_LIMIT: u64 = 500;
/// The accessibility tree's node and depth budgets: the driver's own defaults.
const TREE_NODES: u64 = 1_200;
const TREE_DEPTH: u64 = 64;
/// The most typed text one call may carry.
const TEXT_MAX: usize = 4_000;
/// The most one tool result may carry back, as the computer module does.
const OUTPUT_MAX: usize = 64 * 1024;

const OFF: &str = "The owner's computer is switched off in Retinue, so no bot can see or use it. The owner turns it on in Settings › Integrations › Computer.";
const UNATTENDED: &str = "The owner's computer cannot be used in a run nobody is watching: a scheduled run, an unattended task, or a conversation another bot is driving. Retinue asks with a card, and a card needs the owner there to answer it.";
const FULL_ACCESS: &str = "Full access does not cover the owner's computer. Ask in a conversation set to Ask or Auto, where the owner can still say no.";
const ARCHIVED: &str = "This conversation is archived.";
const STALE: &str = "The screen changed since you last read it, and element indexes do not survive a change. Read the screen of that app again, then act.";
const NO_READ: &str = "Read the screen of that app first: element indexes come from a read, and an action needs a current one.";
const NO_DRIVER: &str = "The computer driver is not installed, so Retinue cannot drive this computer. Run `node scripts/fetch-owner-computer-driver.mjs` to fetch and check it.";
const GRANT_NOTE: &str = "The owner's computer grant is live. Read the screen again before the next action: anything that changes the screen makes element indexes stale.";
/// The wording a timed-out driver call ends with. A fill that ends this way may
/// have reached the app, so the vault records it as `uncertain` rather than
/// claiming it did not happen.
pub(crate) const MAY_HAVE_ARRIVED: &str = "may or may not have reached the app";

/// The grant's three clocks, the card's wait, and the two call deadlines. The
/// tests shorten them rather than waiting out the real ones.
#[derive(Clone, Copy)]
pub struct Limits {
    /// A grant lasts this long from the moment the owner allowed it.
    pub grant: Duration,
    /// No grant outlives this much of one turn, renewals included.
    pub turn: Duration,
    /// This long with no call ends the grant.
    pub idle: Duration,
    /// How long a tool call waits for the owner's card.
    pub consent: Duration,
    /// A read as a whole: the driver start, the tree walk and the answer.
    pub observe: Duration,
    /// An action as a whole.
    pub action: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            grant: Duration::from_secs(5 * 60),
            turn: Duration::from_secs(15 * 60),
            idle: Duration::from_secs(60),
            consent: Duration::from_secs(100),
            observe: Duration::from_secs(10),
            action: Duration::from_secs(15),
        }
    }
}

/// What one conversation may do, for as long as the owner's grant lasts.
pub(crate) struct Grant {
    agent_id: String,
    /// "Once": exactly the call that raised the card, then nothing.
    once: bool,
    started: Instant,
    last_action: Instant,
    /// Per app: whether an action has landed since the last read of it.
    stale: HashMap<String, bool>,
    /// Apps read at least once in this grant.
    read: Vec<String>,
}

/// A card the owner has not answered yet, keyed by the permission id.
struct Pending {
    session_id: String,
    agent_id: String,
    waiter: Option<oneshot::Sender<bool>>,
}

/// The daemon's memory of grants, cards in flight and the driver process a call
/// is running. Nothing here is written anywhere.
#[derive(Default)]
pub struct OwnerComputer {
    limits: Option<Limits>,
    grants: StdMutex<HashMap<String, Grant>>,
    pending: StdMutex<HashMap<String, Pending>>,
    /// The first grant of the current turn, per conversation, so renewals cannot
    /// keep one turn going past `Limits::turn`.
    floors: StdMutex<HashMap<String, (String, Instant)>>,
    /// The driver child a call is running, per conversation, so Stop can kill it.
    pids: StdMutex<HashMap<String, u32>>,
}

fn lock<T>(mutex: &StdMutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poison| poison.into_inner())
}

impl OwnerComputer {
    /// The tests' constructor: the same state with shorter clocks.
    #[cfg(test)]
    pub(crate) fn with_limits(limits: Limits) -> Self {
        Self { limits: Some(limits), ..Self::default() }
    }

    pub(crate) fn limits(&self) -> Limits {
        self.limits.unwrap_or_default()
    }

    /// Whether this conversation holds a live grant. An expired one is removed
    /// here, so a caller that gets `Err` may announce the end exactly once.
    pub(crate) fn live(&self, session_id: &str) -> Result<(), &'static str> {
        let limits = self.limits();
        let mut grants = lock(&self.grants);
        let Some(grant) = grants.get(session_id) else { return Err("no grant") };
        let now = Instant::now();
        if now.duration_since(grant.started) > limits.grant {
            grants.remove(session_id);
            return Err("The computer grant ran out after five minutes. Ask again and the owner can allow another five.");
        }
        if now.duration_since(grant.last_action) > limits.idle {
            grants.remove(session_id);
            return Err("A minute passed with no action, so the computer grant ended. Ask again if you still need it.");
        }
        Ok(())
    }

    /// Ends this conversation's grant and returns it, so the caller can announce
    /// the end with the bot's id.
    pub(crate) fn end(&self, session_id: &str) -> Option<Grant> {
        lock(&self.grants).remove(session_id)
    }

    /// Ends every grant at once: the switch went off, or Stop named no
    /// conversation.
    pub(crate) fn release_all(&self) -> Vec<(String, Grant)> {
        let ended = lock(&self.grants).drain().collect::<Vec<_>>();
        // A card answered after this point must not start a grant the owner no
        // longer wants: without a waiter the answer is a no-op.
        lock(&self.pending).clear();
        ended
    }

    /// Starts a grant, and records the turn's first grant for the turn cap.
    pub(crate) fn begin(&self, session_id: &str, agent_id: &str, once: bool, turn: &str) {
        let now = Instant::now();
        {
            let mut floors = lock(&self.floors);
            match floors.get_mut(session_id) {
                Some(entry) if entry.0 == turn => {}
                Some(entry) => *entry = (turn.to_owned(), now),
                None => {
                    floors.insert(session_id.to_owned(), (turn.to_owned(), now));
                }
            }
        }
        lock(&self.grants).insert(
            session_id.to_owned(),
            Grant { agent_id: agent_id.to_owned(), once, started: now, last_action: now, stale: HashMap::new(), read: Vec::new() },
        );
    }

    /// Whether this turn is still inside its own cap, measuring from the turn's
    /// first grant.
    pub(crate) fn turn_open(&self, session_id: &str, turn: &str) -> bool {
        let now = Instant::now();
        let mut floors = lock(&self.floors);
        let at = match floors.get(session_id) {
            Some((known, at)) if known == turn => *at,
            _ => {
                floors.insert(session_id.to_owned(), (turn.to_owned(), now));
                now
            }
        };
        now.duration_since(at) <= self.limits().turn
    }

    fn remember_child(&self, session_id: &str, pid: Option<u32>) {
        match pid {
            Some(pid) => {
                lock(&self.pids).insert(session_id.to_owned(), pid);
            }
            None => {
                lock(&self.pids).remove(session_id);
            }
        }
    }

    fn child_of(&self, session_id: &str) -> Option<u32> {
        lock(&self.pids).get(session_id).copied()
    }

    /// An action needs a read of that app that no action has overtaken.
    pub(crate) fn check_fresh(&self, session_id: &str, app: &str) -> Result<(), String> {
        let grants = lock(&self.grants);
        let Some(grant) = grants.get(session_id) else { return Err("The computer grant ended. Ask again.".to_owned()) };
        if !grant.read.iter().any(|known| known == app) {
            return Err(NO_READ.to_owned());
        }
        if grant.stale.get(app).copied().unwrap_or(false) {
            return Err(STALE.to_owned());
        }
        Ok(())
    }

    /// Notes one call against the grant: a read clears staleness for its app, an
    /// action marks it, and either way the idle clock moves. Returns true when a
    /// "once" grant has now been spent.
    pub(crate) fn note(&self, session_id: &str, app: &str, observation: bool) -> bool {
        let mut grants = lock(&self.grants);
        let Some(grant) = grants.get_mut(session_id) else { return false };
        grant.last_action = Instant::now();
        if !app.is_empty() {
            if observation {
                grant.stale.insert(app.to_owned(), false);
                if !grant.read.iter().any(|known| known == app) {
                    grant.read.push(app.to_owned());
                }
            } else {
                grant.stale.insert(app.to_owned(), true);
            }
        }
        grant.once
    }
}

fn conflict(message: &str) -> DispatchError {
    derr("conflict", message, axum::http::StatusCode::CONFLICT)
}

fn data_dir(st: &AppState) -> Option<PathBuf> {
    st.team.data_dir.lock().ok().and_then(|dir| dir.clone())
}

// ---------------------------------------------------------------------------
// The switch, the deny-list and the tools

/// Whether the owner turned the feature on. Absent is off.
pub fn enabled(st: &AppState) -> bool {
    st.db.settings().ok().and_then(|settings| settings[SETTING].as_bool()).unwrap_or(false)
}

/// The starter deny-list: password managers, banking, mail and terminals, the
/// four places a mis-aimed click or a read tree does the most damage. The owner's
/// own entries are added to it, and a match is a fail-closed refusal.
pub const STARTER_DENY_LIST: [&str; 26] = [
    // Password managers.
    "1password", "bitwarden", "keepass", "lastpass", "dashlane", "nordpass", "keeper", "keychain",
    // Banking and payments.
    "bank", "hsbc", "barclays", "lloyds", "natwest", "monzo", "revolut", "paypal", "stripe", "coinbase",
    // Mail.
    "outlook", "thunderbird", "gmail", "proton mail",
    // Terminals and remote shells.
    "command prompt", "powershell", "mobaxterm", "putty",
];

/// How many entries the deny-list setting may hold, and how long each may be.
pub const DENY_LIST_MAX: usize = 40;
const DENY_WORD_MAX: usize = 64;

/// The deny-list in force: the starter list plus the owner's own entries.
pub fn deny_list(st: &AppState) -> Vec<String> {
    let mut words = STARTER_DENY_LIST.iter().map(|word| (*word).to_owned()).collect::<Vec<_>>();
    if let Some(entries) = st.db.settings().ok().and_then(|settings| settings[DENY_LIST_SETTING].as_array().cloned()) {
        for entry in entries.iter().filter_map(Value::as_str) {
            let word = entry.trim().to_lowercase();
            if !word.is_empty() {
                words.push(word);
            }
        }
    }
    words
}

/// Which deny-list word an app name or window title matches, if any. Lower-case
/// substring matching, so an over-broad list is the safe direction.
pub fn deny_hit(words: &[String], text: &str) -> Option<String> {
    let haystack = text.to_lowercase();
    if haystack.is_empty() {
        return None;
    }
    words.iter().find(|word| !word.is_empty() && haystack.contains(word.as_str())).cloned()
}

/// `ownerComputer.denyList`: at most 40 entries of 1 to 64 characters each.
pub fn valid_deny_list(value: &Value) -> bool {
    value.as_array().is_some_and(|entries| {
        entries.len() <= DENY_LIST_MAX
            && entries.iter().all(|entry| {
                entry.as_str().is_some_and(|text| {
                    let text = text.trim();
                    !text.is_empty() && text.chars().count() <= DENY_WORD_MAX && !text.chars().any(char::is_control)
                })
            })
    })
}

/// The tools, as the provider sees them. There is deliberately no
/// `owner_screenshot`: a frame of the owner's screen needs a per-frame approval
/// card, which this cut does not have, so the tool does not exist to be asked for.
pub fn tool_schemas() -> Vec<Value> {
    let app = |description: &str| json!({"type": "string", "description": description});
    let element = |description: &str| json!({"type": "integer", "minimum": 0, "description": description});
    vec![
        json!({"name":"owner_apps","description":"List the apps with windows on the owner's computer, by name. Only while the owner has allowed this conversation to use the computer.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}}),
        json!({"name":"owner_screen_state","description":"Read one app's window as an accessibility tree: the controls, their names and their element indexes. This is text, not a picture. Read before you act, and read again after anything changes the screen. Password fields never carry their value.","inputSchema":{"type":"object","properties":{"app":app("The app's name, as owner_apps lists it.")},"required":["app"],"additionalProperties":false}}),
        json!({"name":"owner_focus","description":"Make one app the target of the actions that follow, and read its window. It reads the window; it does not bring the app to the front, so the owner's own typing and focus stay where they are.","inputSchema":{"type":"object","properties":{"app":app("The app's name, as owner_apps lists it.")},"required":["app"],"additionalProperties":false}}),
        json!({"name":"owner_click","description":"Click one element of an app, addressed by its index from the latest read of that app. UI Automation first, then a message to the window; the owner's pointer does not move.","inputSchema":{"type":"object","properties":{"app":app("The app's name."),"element":element("The element index from the latest read of this app.")},"required":["app","element"],"additionalProperties":false}}),
        json!({"name":"owner_type","description":"Put text into one editable element of an app, addressed by its index from the latest read. The value is written through the control's own value pattern and read back to check it. The text is never recorded: only its length is.","inputSchema":{"type":"object","properties":{"app":app("The app's name."),"element":element("The element index from the latest read of this app."),"text":{"type":"string","description":"The text to put in the control, at most 4,000 characters."}},"required":["app","element","text"],"additionalProperties":false}}),
        json!({"name":"owner_key","description":"Press one key in an app, such as Enter, Escape, Tab or Down. One key at a time: key combinations are refused.","inputSchema":{"type":"object","properties":{"app":app("The app's name."),"key":{"type":"string","description":"One key name, for example Enter, Escape, Tab, Down or F5."}},"required":["app","key"],"additionalProperties":false}}),
        json!({"name":"owner_scroll","description":"Scroll one element of an app, addressed by its index from the latest read.","inputSchema":{"type":"object","properties":{"app":app("The app's name."),"element":element("The element index from the latest read of this app."),"direction":{"type":"string","enum":["up","down","left","right"],"description":"Which way to scroll."},"pages":{"type":"number","description":"How much to scroll, in pages. Defaults to 1."}},"required":["app","element","direction"],"additionalProperties":false}}),
        json!({"name":"owner_stop","description":"Stop using the owner's computer now: it ends this conversation's grant immediately and cancels the running turn. Use it as soon as the work with the computer is done; the owner can also press Stop.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}}),
    ]
}

fn known_tool(name: &str) -> bool {
    tool_schemas().iter().any(|tool| tool["name"].as_str() == Some(name))
}

// ---------------------------------------------------------------------------
// The driver

/// The child's environment: paths only, rebuilt from scratch. Clearing is the
/// point, not a tidy-up: a child inherits the owner's own variables, and
/// `OPEN_COMPUTER_USE_ALLOW_GLOBAL_POINTER_FALLBACKS` would turn real pointer and
/// real keyboard input on for a bot.
pub(crate) fn driver_env() -> BTreeMap<String, String> {
    const KEEP: [&str; 17] =
        ["PATH", "Path", "PATHEXT", "SystemRoot", "SYSTEMROOT", "windir", "TEMP", "TMP", "USERPROFILE", "LOCALAPPDATA", "APPDATA", "COMSPEC", "ProgramData", "ProgramFiles", "ProgramW6432", "HOMEDRIVE", "PSModulePath"];
    let mut env = BTreeMap::new();
    for key in KEEP {
        if let Ok(value) = std::env::var(key) {
            env.insert(key.to_owned(), value);
        }
    }
    env.remove(GLOBAL_POINTER_VAR);
    env
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|_| "The computer driver could not be read.".to_owned())?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok(hasher.finalize().iter().map(|byte| format!("{byte:02x}")).collect())
}

/// Refuses any driver whose bytes are not the pinned release. A wrong or swapped
/// binary is a refusal, never a warning.
pub fn verify_driver(path: &Path) -> Result<(), String> {
    let actual = sha256_file(path)?;
    if actual != DRIVER_EXE_SHA256 {
        return Err(format!(
            "The computer driver at {} is not open-computer-use {DRIVER_VERSION} as Retinue pinned it (that release's zip is SHA-256 {DRIVER_ZIP_SHA256}), so it was not started. Fetch it again with `node scripts/fetch-owner-computer-driver.mjs`.",
            path.display()
        ));
    }
    Ok(())
}

/// Where the staged driver is: beside the daemon, as the installer puts its
/// sidecars, then under the data folder for a development build.
pub fn driver_path(st: &AppState) -> Option<PathBuf> {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(siblings) = exe.parent() {
            let candidate = siblings.join(DRIVER_FILE);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    let candidate = data_dir(st)?.join(ENDPOINT_FOLDER).join(DRIVER_FILE);
    candidate.is_file().then_some(candidate)
}

/// What one Retinue tool becomes for the driver, with every limit forced rather
/// than left to a default. `include_screenshot` is written out explicitly because
/// the driver's own default is `true`, and a picture of the owner's screen must
/// never be what happens when a field goes missing.
pub fn driver_arguments(tool: &str, app: &str, element: Option<u64>, text: Option<&str>, key: Option<&str>, direction: Option<&str>, pages: Option<f64>) -> Value {
    let element_index = element.map(|index| index.to_string()).unwrap_or_default();
    match tool {
        "owner_apps" => json!({"tool": "list_apps", "arguments": {}}),
        "owner_screen_state" | "owner_focus" => json!({
            "tool": "get_app_state",
            "arguments": {
                "app": app,
                "include_ax": true,
                "include_screenshot": false,
                "text_limit": TEXT_LIMIT,
                "max_tree_nodes": TREE_NODES,
                "max_tree_depth": TREE_DEPTH,
            }
        }),
        "owner_click" => json!({
            "tool": "click",
            "arguments": {"app": app, "element_index": element_index, "click_count": 1, "mouse_button": "left", "click_method": "auto"}
        }),
        "owner_type" => json!({"tool": "set_value", "arguments": {"app": app, "element_index": element_index, "value": text.unwrap_or("")}}),
        "owner_key" => json!({"tool": "press_key", "arguments": {"app": app, "key": key.unwrap_or(""), "key_method": "auto"}}),
        "owner_scroll" => {
            let mut arguments = json!({"app": app, "element_index": element_index, "direction": direction.unwrap_or("down")});
            if let Some(pages) = pages {
                arguments["pages"] = json!(pages);
            }
            json!({"tool": "scroll", "arguments": arguments})
        }
        _ => json!({}),
    }
}

/// One key at a time: a combination cannot travel as a window message, and
/// Retinue refuses it itself rather than leaving the answer to the driver.
pub fn key_is_single(key: &str) -> bool {
    let key = key.trim();
    !key.is_empty() && key.chars().count() <= 40 && !key.contains('+') && key.split_whitespace().count() == 1
}

/// Kills the driver process tree. Stopping is a kill, not a request: a hung UI
/// Automation call only ends when its process does. The blocking standard-library
/// `Command` on purpose: Tokio's `.status()` is a future that does nothing until
/// awaited, so the kill never ran (clippy `let_underscore_future`, 14 November 2026).
fn kill_tree(pid: u32) {
    let mut command = std::process::Command::new("taskkill");
    command.args(["/PID", &pid.to_string(), "/T", "/F"]).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    retinue_process::hide_std_window(&mut command);
    let _ = command.status();
}

/// One driver call: start the pinned child, handshake, call one tool, read one
/// answer, and let the child end. A child per call keeps a hung app's bill to one
/// call and leaves nothing behind when a grant ends.
async fn driver_call(exe: &Path, tool: &str, arguments: Value, limit: Duration) -> Result<String, String> {
    verify_driver(exe)?;
    let mut command = Command::new(exe);
    retinue_process::hide_window(&mut command);
    command
        .arg("mcp")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .env_clear()
        .envs(driver_env())
        .kill_on_drop(true);
    let mut child = command.spawn().map_err(|_| "The computer driver could not be started.".to_owned())?;
    match tokio::time::timeout(limit, exchange(&mut child, tool, arguments)).await {
        Ok(Ok(answer)) => Ok(answer),
        Ok(Err(message)) => {
            if let Some(pid) = child.id() {
                kill_tree(pid);
            }
            Err(message)
        }
        Err(_) => {
            if let Some(pid) = child.id() {
                kill_tree(pid);
            }
            Err(format!("The computer did not answer in time, so Retinue stopped it. The call was abandoned; a click or a keystroke {MAY_HAVE_ARRIVED}."))
        }
    }
}

async fn exchange(child: &mut Child, tool: &str, arguments: Value) -> Result<String, String> {
    let mut stdin = child.stdin.take().ok_or_else(|| "The computer driver has no input.".to_owned())?;
    let stdout = child.stdout.take().ok_or_else(|| "The computer driver has no output.".to_owned())?;
    let mut lines = BufReader::new(stdout).lines();
    let hello = json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"retinue","version":env!("CARGO_PKG_VERSION")}}});
    let call = json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":tool,"arguments":arguments}});
    for message in [hello, call] {
        stdin.write_all(format!("{message}\n").as_bytes()).await.map_err(|_| "The computer driver stopped reading.".to_owned())?;
        stdin.flush().await.map_err(|_| "The computer driver stopped reading.".to_owned())?;
    }
    // At most a few lines back: the answer to id 2, or the first thing that says
    // why there is none.
    for _ in 0..4 {
        let Some(line) = lines.next_line().await.map_err(|_| "The computer driver stopped early.".to_owned())? else { break };
        let Ok(message) = serde_json::from_str::<Value>(&line) else { continue };
        if message["id"] != json!(2) {
            continue;
        }
        if let Some(error) = message.get("error") {
            return Err(format!("The computer said: {}", error["message"].as_str().unwrap_or("it could not do that")));
        }
        let result = &message["result"];
        let text = result["content"].as_array().and_then(|items| items.iter().find_map(|item| (item["type"] == "text").then(|| item["text"].as_str().unwrap_or("").to_owned())));
        if result["isError"] == json!(true) {
            return Err(text.unwrap_or_else(|| "The computer could not do that.".to_owned()));
        }
        return Ok(text.unwrap_or_default());
    }
    Err("The computer answered nothing.".to_owned())
}

/// Cuts a long result the way the computer module does.
fn cap_output(text: String) -> String {
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

// ---------------------------------------------------------------------------
// Masking

/// Words in an element's own name or class that mean "this holds a secret".
const SECRET_HINTS: [&str; 13] = ["password", "passcode", "passphrase", "pin code", "secret", "token", "cvv", "cvc", "ssn", "security code", "one-time", " otp", "2fa"];

/// Replaces the value of one tree line, keeping everything after it.
fn blank_value(line: &str) -> String {
    let Some(at) = line.find(" Value: ") else { return line.to_owned() };
    let head = &line[..at];
    let rest = &line[at + " Value: ".len()..];
    let end = [" Secondary Actions: ", " Frame: "].iter().filter_map(|marker| rest.find(marker)).min().unwrap_or(rest.len());
    format!("{head} Value: ••••{}", &rest[end..])
}

/// One line of the driver's tree, with a value that could be a secret blanked.
///
/// The pinned driver (v1.1.4) does not report UI Automation's `IsPassword`, so
/// Retinue cannot mask by that flag; it masks on what it can see, and it fails
/// closed: a value goes when the line's own text names a secret, when the value is
/// nothing but mask characters, or when the control is a password control type.
pub fn mask_line(line: &str) -> String {
    let Some(at) = line.find(" Value: ") else { return line.to_owned() };
    let head = &line[..at];
    let rest = &line[at + " Value: ".len()..];
    let end = [" Secondary Actions: ", " Frame: "].iter().filter_map(|marker| rest.find(marker)).min().unwrap_or(rest.len());
    let value = &rest[..end];
    let lowered = head.to_lowercase();
    let sensitive = SECRET_HINTS.iter().any(|hint| lowered.contains(hint)) || value.chars().all(|ch| ch == '•' || ch == '*' || ch == ' ');
    if !sensitive {
        return line.to_owned();
    }
    blank_value(line)
}

/// A field the app did not name is treated as a secret: a bare `edit` with a
/// value is the shape a password box takes when an app hides its purpose.
pub fn nameless_edit(line: &str) -> String {
    let Some(at) = line.find(" Value: ") else { return line.to_owned() };
    let head = line[..at].trim_start();
    let tokens = head.split_whitespace().collect::<Vec<_>>();
    let bare_edit = tokens.len() == 2 && tokens[1].contains("edit");
    if !bare_edit {
        return line.to_owned();
    }
    blank_value(line)
}

/// A whole tree, masked line by line.
pub fn mask_tree(text: &str) -> String {
    text.split('\n').map(|line| nameless_edit(&mask_line(line))).collect::<Vec<_>>().join("\n")
}

// ---------------------------------------------------------------------------
// The gate a call passes

/// Why this conversation may not use the owner's computer, if it may not.
pub(crate) fn attendance(st: &AppState, session: &Value) -> Result<(), String> {
    if session["archived"] == true {
        return Err(ARCHIVED.to_owned());
    }
    if crate::is_unattended(&session["link"]) {
        return Err(UNATTENDED.to_owned());
    }
    if session["link"]["kind"].as_str() == Some("side") {
        return Err(UNATTENDED.to_owned());
    }
    // A turn a teammate's reply woke is a teammate's turn: nobody at the keyboard
    // asked for it, so it cannot take the owner's computer.
    if st.team.depth_of(session["id"].as_str().unwrap_or("")) > 0 {
        return Err(UNATTENDED.to_owned());
    }
    let mode = crate::exec_options_for_session(st, session).map(|options| options.approval_mode.as_str().to_owned()).unwrap_or_default();
    if mode == "bypass" {
        return Err(FULL_ACCESS.to_owned());
    }
    Ok(())
}

/// Ends a conversation's grant, kills any driver a call is running, and announces
/// the end for the UI's bar and the companion pill.
pub(crate) async fn end_grant(st: &AppState, session_id: &str, grant: Option<Grant>) {
    if let Some(pid) = st.owner_computer.child_of(session_id) {
        kill_tree(pid);
        st.owner_computer.remember_child(session_id, None);
    }
    let agent_id = grant
        .as_ref()
        .map(|grant| grant.agent_id.clone())
        .or_else(|| st.db.session_detail(session_id).ok().and_then(|session| session["agentId"].as_str().map(str::to_owned)))
        .unwrap_or_default();
    st.emit("owner_computer.grant", json!({"sessionId": session_id, "agentId": agent_id, "state": "ended", "endsAt": Value::Null})).await;
}

/// Announces a live grant to the UI, which draws the bar and the pill.
async fn announce(st: &AppState, session_id: &str, agent_id: &str) {
    let ends_at = (Utc::now() + chrono::Duration::milliseconds(st.owner_computer.limits().grant.as_millis() as i64)).to_rfc3339_opts(SecondsFormat::Millis, true);
    st.emit("owner_computer.grant", json!({"sessionId": session_id, "agentId": agent_id, "state": "live", "endsAt": ends_at})).await;
}

/// What the card says the bot is about to do, in one sentence.
fn card_note(tool: &str, app: &str) -> String {
    match tool {
        "owner_apps" => "List the apps on your computer.".to_owned(),
        "owner_screen_state" | "owner_focus" => format!("Read the screen of {app}."),
        "owner_click" => format!("Click an element of {app}."),
        "owner_type" => format!("Put text into {app}."),
        "owner_key" => format!("Press a key in {app}."),
        "owner_scroll" => format!("Scroll {app}."),
        _ => "Use this computer.".to_owned(),
    }
}

/// The consent card, or `Ok(())` when a live grant already covers the call.
async fn ensure_grant(st: &AppState, session_id: &str, session: &Value, note: &str) -> Result<(), String> {
    if st.owner_computer.live(session_id).is_ok() {
        return Ok(());
    }
    let agent_id = session["agentId"].as_str().unwrap_or("").to_owned();
    let turn = st.active_turns.lock().await.get(session_id).cloned().unwrap_or_default();
    if !st.owner_computer.turn_open(session_id, &turn) {
        let grant = st.owner_computer.end(session_id);
        end_grant(st, session_id, grant).await;
        return Err("This turn has had the computer for fifteen minutes, which is the most one turn gets. Say what you did, and the owner can allow another turn.".to_owned());
    }
    let bot = st.db.agent_get(&agent_id).ok().and_then(|agent| agent["name"].as_str().map(str::to_owned)).unwrap_or_else(|| "Your bot".to_owned());
    let limits = st.owner_computer.limits();
    let id = Uuid::new_v4().to_string();
    let (sender, receiver) = oneshot::channel();
    {
        let mut pending = lock(&st.owner_computer.pending);
        // One card per conversation: a second would only stack cards.
        if pending.values().any(|waiting| waiting.session_id == session_id && waiting.waiter.is_some()) {
            return Err("The owner already has a card for this conversation. Wait for their answer.".to_owned());
        }
        pending.insert(id.clone(), Pending { session_id: session_id.to_owned(), agent_id: agent_id.clone(), waiter: Some(sender) });
    }
    let title = format!("{bot} wants to control this computer");
    let detail = format!("{note} It can read one app's window as text and act on what it finds; it cannot take a picture of your screen. It stops after five minutes, and you can stop it at any time.");
    let choices = vec!["allow".to_owned(), "allow_once".to_owned(), "deny".to_owned()];
    // Never rememberable: the permission store refuses a rule for this category,
    // because a remembered grant would outlive the moment the owner said yes.
    let marks = json!({"kind": "owner_computer", "rememberable": false});
    if st.db.insert_permission(&id, session_id, &format!("owner:{id}"), &title, Some(&detail), &choices, &json!({}), &marks).is_err() {
        lock(&st.owner_computer.pending).remove(&id);
        return Err("The owner's answer could not be asked for.".to_owned());
    }
    st.emit(
        "permission.requested",
        json!({
            "id": id, "sessionId": session_id, "runtimeId": Value::Null, "kind": "owner_computer", "category": "other",
            "title": title, "detail": detail, "risk": "unknown", "choices": choices, "expiresAt": Value::Null,
            "status": "pending", "rememberable": false, "rememberLabel": Value::Null,
            "projectId": Value::Null, "projectName": Value::Null,
        }),
    )
    .await;
    // The card is answered elsewhere, so this call waits here. An answer that
    // arrives after the wait still starts the grant, and the next call uses it.
    match tokio::time::timeout(limits.consent, receiver).await {
        Ok(Ok(true)) => {
            announce(st, session_id, &agent_id).await;
            Ok(())
        }
        Ok(Ok(false)) => Err("The owner did not allow the computer.".to_owned()),
        _ => Err("The owner has not answered yet. The card stays in the conversation; ask again once they have.".to_owned()),
    }
}

/// The owner's answer to the consent card, from `permission.reply`. The card is
/// already claimed by the caller, so this only records what was allowed.
pub(crate) async fn reply_consent(st: &AppState, id: &str, session_id: &str, choice: &str) -> Result<Value, DispatchError> {
    let Some(pending) = lock(&st.owner_computer.pending).remove(id) else {
        return Ok(json!({"resolved": true, "allowed": false}));
    };
    if pending.session_id != session_id {
        return Err(conflict("That card belongs to another conversation."));
    }
    let lowered = choice.to_ascii_lowercase();
    let allowed = matches!(lowered.as_str(), "allow" | "allow_once" | "accept" | "yes");
    if allowed && !enabled(st) {
        return Err(conflict("The owner's computer was switched off, so nothing was allowed."));
    }
    let turn = st.active_turns.lock().await.get(session_id).cloned().unwrap_or_default();
    if allowed {
        st.owner_computer.begin(session_id, &pending.agent_id, lowered == "allow_once", &turn);
        announce(st, session_id, &pending.agent_id).await;
    }
    if let Some(waiter) = pending.waiter {
        let _ = waiter.send(allowed);
    }
    Ok(json!({"resolved": true, "allowed": allowed}))
}

// ---------------------------------------------------------------------------
// Tool calls

/// One tool call, with every gate in front of it.
pub(crate) async fn call_tool(st: &AppState, session_id: &str, name: &str, args: &Value) -> Result<String, String> {
    if !enabled(st) {
        return Err(OFF.to_owned());
    }
    if !known_tool(name) {
        return Err(format!("Unknown tool {name}."));
    }
    // Stop is always available: ending a grant never needs permission.
    if name == "owner_stop" {
        let grant = st.owner_computer.end(session_id);
        end_grant(st, session_id, grant).await;
        let _ = crate::cancel_session_turn(st, session_id).await;
        return Ok("Stopped: this conversation no longer has the owner's computer.".to_owned());
    }
    let session = st.db.session_detail(session_id).map_err(|_| "This conversation is no longer available.".to_owned())?;
    attendance(st, &session)?;
    let app = args["app"].as_str().map(str::trim).unwrap_or("").to_owned();
    if name != "owner_apps" && (app.is_empty() || app.chars().count() > APP_MAX) {
        return Err(format!("app is required, up to {APP_MAX} characters."));
    }
    // The deny-list runs before the card and before the driver: a refused app must
    // never be read, clicked or typed into.
    if !app.is_empty() {
        if let Some(word) = deny_hit(&deny_list(st), &app) {
            return Err(format!("{app} is on the computer deny-list ({word}), so Retinue did not touch it. The owner can change the list in Settings."));
        }
    }
    ensure_grant(st, session_id, &session, &card_note(name, &app)).await?;
    let exe = driver_path(st).ok_or_else(|| NO_DRIVER.to_owned())?;
    let limits = st.owner_computer.limits();
    let observation = matches!(name, "owner_apps" | "owner_screen_state" | "owner_focus");
    let (driver_tool, deadline) = match name {
        "owner_apps" => (driver_arguments(name, "", None, None, None, None, None), limits.observe),
        "owner_screen_state" | "owner_focus" => (driver_arguments(name, &app, None, None, None, None, None), limits.observe),
        "owner_click" => {
            let element = args["element"].as_u64().ok_or("element is required: the index from the latest read of this app.")?;
            st.owner_computer.check_fresh(session_id, &app)?;
            (driver_arguments(name, &app, Some(element), None, None, None, None), limits.action)
        }
        "owner_type" => {
            let element = args["element"].as_u64().ok_or("element is required: the index from the latest read of this app.")?;
            let text = args["text"].as_str().ok_or("text is required.")?;
            if text.chars().count() > TEXT_MAX {
                return Err(format!("text must be at most {TEXT_MAX} characters."));
            }
            st.owner_computer.check_fresh(session_id, &app)?;
            (driver_arguments(name, &app, Some(element), Some(text), None, None, None), limits.action)
        }
        "owner_key" => {
            let key = args["key"].as_str().ok_or("key is required.")?;
            if !key_is_single(key) {
                return Err("One key at a time, for example Enter or Tab. Key combinations are refused.".to_owned());
            }
            st.owner_computer.check_fresh(session_id, &app)?;
            (driver_arguments(name, &app, None, None, Some(key), None, None), limits.action)
        }
        "owner_scroll" => {
            let element = args["element"].as_u64().ok_or("element is required: the index from the latest read of this app.")?;
            let direction = args["direction"].as_str().filter(|value| ["up", "down", "left", "right"].contains(value)).ok_or("direction must be up, down, left or right.")?;
            let pages = args["pages"].as_f64().filter(|pages| (0.1..=50.0).contains(pages));
            st.owner_computer.check_fresh(session_id, &app)?;
            (driver_arguments(name, &app, Some(element), None, None, Some(direction), pages), limits.action)
        }
        other => return Err(format!("Unknown tool {other}.")),
    };
    let tool = driver_tool["tool"].as_str().unwrap_or("").to_owned();
    let answer = driver_call(&exe, &tool, driver_tool["arguments"].clone(), deadline).await;
    // A failed call is still activity: the idle clock must not end a grant in the
    // middle of an app that keeps refusing.
    let spent = st.owner_computer.note(session_id, &app, observation);
    if spent {
        let grant = st.owner_computer.end(session_id);
        end_grant(st, session_id, grant).await;
    }
    let text = answer?;
    // A window title is part of the answer; a refused app's title must not be
    // handed back just because the call reached the driver first.
    if let Some(title) = window_title(&text) {
        if let Some(word) = deny_hit(&deny_list(st), &title) {
            let grant = st.owner_computer.end(session_id);
            end_grant(st, session_id, grant).await;
            return Err(format!("That window is on the computer deny-list ({word}), so Retinue did not read it. The owner can change the list in Settings."));
        }
    }
    let mut body = if observation { format!("{}\n\n{GRANT_NOTE}", mask_tree(&text)) } else { text };
    // The app list is filtered, not only refused: a denied app is not named to the
    // model at all, so it cannot be aimed at later.
    if name == "owner_apps" {
        body = hide_denied_apps(&body, &deny_list(st));
    }
    Ok(cap_output(format!("{}{body}", summarise(name, &app, args, observation))))
}

/// The app name as the driver's own text reports it, for the deny-list check.
pub fn window_title(text: &str) -> Option<String> {
    let line = text.lines().find(|line| line.starts_with("Window: "))?;
    let rest = line.trim_start_matches("Window: ").trim();
    let title = rest.split(", App:").next().unwrap_or("").trim().trim_matches('"');
    (!title.is_empty()).then(|| title.to_owned())
}

/// Drops every line of the app list that names a denied app or window title, so a
/// denied app is never even named to the model.
pub fn hide_denied_apps(text: &str, words: &[String]) -> String {
    text.lines().filter(|line| deny_hit(words, line).is_none()).collect::<Vec<_>>().join("\n")
}

/// The content-free first line: the app, the action class and a count. Never the
/// typed text, never the tree.
pub fn summarise(name: &str, app: &str, args: &Value, observation: bool) -> String {
    match name {
        "owner_apps" => "Apps with windows on the owner's computer:\n".to_owned(),
        "owner_screen_state" | "owner_focus" if observation => format!("Screen of {app}, as text:\n"),
        "owner_click" => format!("Clicked element {} in {app}.\n", args["element"].as_u64().unwrap_or(0)),
        "owner_type" => format!("Typed {} characters into {app}.\n", args["text"].as_str().map(|text| text.chars().count()).unwrap_or(0)),
        "owner_key" => format!("Pressed {} in {app}.\n", args["key"].as_str().unwrap_or("a key")),
        "owner_scroll" => format!("Scrolled {app}.\n"),
        _ => format!("Used {app}.\n"),
    }
}

// ---------------------------------------------------------------------------
// RPCs the window calls

/// `owner_computer.stop {sessionId?}`: ends one conversation's grant, or every
/// grant at once, killing any driver a call is running, and cancels the turn.
pub async fn rpc(st: &AppState, method: &str, p: &Value) -> Result<Value, DispatchError> {
    match method {
        "owner_computer.stop" => {
            let only = p.get("sessionId").and_then(Value::as_str).filter(|id| !id.is_empty()).map(str::to_owned);
            let stopped = match only {
                Some(session_id) => {
                    let grant = st.owner_computer.end(&session_id);
                    let had = grant.is_some();
                    end_grant(st, &session_id, grant).await;
                    let _ = crate::cancel_session_turn(st, &session_id).await;
                    had
                }
                None => {
                    let ended = st.owner_computer.release_all();
                    let any = !ended.is_empty();
                    for (session_id, grant) in ended {
                        end_grant(st, &session_id, Some(grant)).await;
                    }
                    any
                }
            };
            Ok(json!({"stopped": stopped}))
        }
        _ => Err(derr("not_found", "unknown owner computer method", axum::http::StatusCode::NOT_FOUND)),
    }
}

/// Releases every grant at once: the owner turned the switch off.
pub async fn switch_off(st: &AppState) {
    for (session_id, grant) in st.owner_computer.release_all() {
        end_grant(st, &session_id, Some(grant)).await;
    }
}

// ---------------------------------------------------------------------------
// MCP calls from `retinued owner-server`, and the server a conversation gets

pub async fn mcp_rpc(st: &AppState, session_id: &str, method: &str, params: &Value) -> Result<Value, DispatchError> {
    // 17 November 2026 (headless Linux): there is no owner computer to drive off
    // Windows, so the tools are empty and a call says so in one plain sentence
    // rather than failing inside a PowerShell script that is not there.
    if !cfg!(windows) {
        return Ok(match method {
            "mcp.owner.list" => json!({"tools": []}),
            "mcp.owner.call" => json!({"content":[{"type":"text","text":"The owner's computer is a Windows computer. This host cannot control one."}],"isError":true}),
            _ => return Err(derr("not_found", "unknown owner computer method", axum::http::StatusCode::NOT_FOUND)),
        });
    }
    match method {
        // The switch is read again on every list: a bridge a conversation started
        // before the owner turned it off must advertise nothing.
        "mcp.owner.list" => {
            if !enabled(st) {
                return Ok(json!({"tools": []}));
            }
            Ok(json!({"tools": tool_schemas()}))
        }
        "mcp.owner.call" => {
            let name = params["name"].as_str().unwrap_or("");
            if name.is_empty() || name.len() > 40 || !name.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'_') {
                return Ok(json!({"content":[{"type":"text","text":"Unknown tool."}],"isError":true}));
            }
            // One content-free line per call: the tool's own name and nothing else.
            tracing::debug!("owner-computer tool call: {name}");
            let result = call_tool(st, session_id, name, &params["arguments"]).await;
            Ok(match result {
                Ok(text) => json!({"content":[{"type":"text","text":cap_output(text)}]}),
                Err(text) => json!({"content":[{"type":"text","text":cap_output(text)}],"isError":true}),
            })
        }
        _ => Err(derr("not_found", "unknown owner computer method", axum::http::StatusCode::NOT_FOUND)),
    }
}

fn endpoint_path(dir: &Path, session_id: &str) -> PathBuf {
    let safe = session_id.chars().filter(|ch| ch.is_ascii_alphanumeric() || *ch == '-' || *ch == '_').take(80).collect::<String>();
    let safe = if safe.is_empty() { Uuid::new_v4().to_string() } else { safe };
    dir.join(ENDPOINT_FOLDER).join(format!("mcp-{safe}.json"))
}

/// The endpoint file is created empty, restricted to the owner, and only then
/// written, exactly as the computer bridge's is.
fn write_endpoint(path: &Path, address: &str, token: &str) -> Result<(), String> {
    use std::io::Write;
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

/// Deletes every endpoint file in the folder, at startup: a stale file names a
/// capability a restart ended.
pub(crate) fn sweep_endpoints(data_dir: &Path) {
    let dir = data_dir.join(ENDPOINT_FOLDER);
    let Ok(entries) = std::fs::read_dir(&dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() && path.extension().is_some_and(|extension| extension == "json") {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// The `owner-computer` server for one conversation, or `None` when the owner's
/// switch is off — in which case the tools do not exist at all — or the endpoint
/// file could not be written. The spec carries no token: the bridge reads the
/// endpoint file, and every decision is made here.
pub fn mcp_server_for(st: &AppState, session_id: &str) -> Option<McpServerSpec> {
    // 17 November 2026: not offered on a host that has no Windows computer to
    // drive, so a Linux bot never sees tools that cannot work.
    if !cfg!(windows) {
        return None;
    }
    if !enabled(st) {
        return None;
    }
    let address = st.team.address.lock().ok().and_then(|address| address.clone())?;
    let dir = data_dir(st)?;
    let executable = std::env::current_exe().ok()?;
    let endpoint = endpoint_path(&dir, session_id);
    if write_endpoint(&endpoint, &address, &st.team.token_for(session_id)).is_err() {
        tracing::warn!("the owner computer bridge's endpoint file could not be written");
        return None;
    }
    Some(McpServerSpec {
        name: SERVER_NAME.into(),
        command: executable,
        args: vec!["owner-server".into(), "--endpoint".into(), endpoint.to_string_lossy().into_owned()],
        env: BTreeMap::new(),
    })
}
