//! Installing and updating the coding-agent CLIs from inside Retinue
//! (21 November 2026; design `docs/research/cli-install-and-update.md`, section 8).
//!
//! The owner's rule is that an install never shows a terminal window. So the
//! daemon does it, in two modes per provider:
//!
//! * **Adopt**: the owner already has the CLI. Retinue works out what owns it
//!   from the executable's real path and runs *that* owner's update, windowless
//!   (a global npm package under the user's profile, a WinGet link under the
//!   user's profile, or the CLI's own `update`/`upgrade`). A system-wide install
//!   is refused with one sentence: Retinue never raises an admin prompt.
//! * **Manage**: Retinue downloads a pinned, digest-checked build into
//!   `<data>/agents/<provider>/<version>/` and launches it by path. No admin, no
//!   PATH edit, and a version Retinue can state exactly.
//!
//! Which one a row uses is `agents.<provider>.mode`, defaulting to Adopt when a
//! CLI was found and Manage otherwise.
//!
//! **The rules this module enforces** (`provider_install_tests.rs` proves them
//! against a local stub origin, never the network): HTTPS only, from the fixed
//! host list in [`ALLOWED_HOSTS`]; redirects only to those hosts; the publisher's
//! digest compared while the bytes stream and **before** anything is unpacked or
//! run; archive entries canonicalised and refused if they leave the staging
//! folder (no `..`, no absolute names, no symlinks or hard links); a total
//! unpacked-size cap; a 30-minute limit; one install per provider; the receipt
//! (`current.json`) written last through a temp file and a rename; the previous
//! version kept for one "Go back"; a running bot's binary never overwritten, and
//! an Adopt update refused while that provider has a running turn.
//!
//! **Never remote.** `runtime.install.*` are `never` in
//! `apps/desktop/src/remote/tiers.json` and the event is `drop`; the Retinue MCP
//! has no install tool; no `machine.*` pass-through exists. A headless host
//! reaches them from its own terminal through `console.agent.install`
//! (`retinued agents install <provider>`), which needs the console token file.
//!
//! **Nothing here logs a URL, a path outside `<data>`, or a file's contents.**
//! The activity line is provider, version, host, phase and outcome, and the
//! event carries `{provider, phase, version, bytesDone, bytesTotal}` only.

use crate::{derr, diagnostics, sandbox, AppState, DispatchError};
use axum::http::StatusCode;
use serde_json::{json, Value};
use sha2::{Digest as _, Sha256, Sha512};
use std::collections::HashMap;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant};

/// The one event an install announces.
pub(crate) const EVENT: &str = "runtime.install.changed";
/// The whole operation's budget. T3 Code's reason, which applies here: native
/// updaters download hundreds of megabytes without printing progress, and killing
/// one midway leaves a truncated file the next run would trust.
const INSTALL_LIMIT: Duration = Duration::from_secs(30 * 60);
/// A per-request timeout; the job's own limit bounds the whole operation.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(120);
/// No provider's download is bigger than this (the largest measured is Codex at
/// 407 MB, and Claude at 368 MB).
const MAX_DOWNLOAD_BYTES: u64 = 512 * 1024 * 1024;
/// What one archive may unpack to (a zip-bomb guard; Codex's 215 MB tarball
/// unpacks to a 407 MB executable).
const MAX_UNPACKED_BYTES: u64 = 1024 * 1024 * 1024;
/// The most of an installer's own output ever kept as its "last line".
const MAX_LINE_CHARS: usize = 200;
/// A publisher answer (a JSON document or a checksum file) is a few KiB.
const MAX_META_BYTES: u64 = 4 * 1024 * 1024;

/// Every host a download may come from. `github.com` covers a release asset;
/// GitHub answers the redirect to one of its asset hosts, which are named here so
/// a redirect can only go to them.
const ALLOWED_HOSTS: [&str; 6] = [
    "downloads.claude.ai",
    "registry.npmjs.org",
    "api.github.com",
    "github.com",
    "objects.githubusercontent.com",
    "release-assets.githubusercontent.com",
];

/// The test's own origin, the `GH_STUB`/`TEST_ROOTS` pattern this repository
/// already uses: when it is set, every publisher address is that origin and an
/// `http://127.0.0.1` URL is allowed. Empty in production, where nothing reads it.
pub(crate) static TEST_ORIGIN: StdMutex<Option<String>> = StdMutex::new(None);

// ---------------------------------------------------------------------------
// The per-provider table

/// How a managed copy is obtained.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Source {
    /// Claude's own release manifest: a bare executable per platform, with a
    /// sha256 and a size beside it.
    ClaudeManifest,
    /// A GitHub release asset, with the publisher's `SHA256SUMS` beside it (or
    /// the API's own `digest` field when the release carries one).
    GitHubAsset { repo: &'static str, asset: &'static str, sums: &'static str, tag_prefix: &'static str },
    /// npm's platform package: the real binary, with its own sha512 `integrity`.
    NpmPlatform { launcher: &'static str, platform: &'static str },
}

/// One provider's row: what it is called, what owns it, and how it is obtained.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Row {
    pub provider: &'static str,
    pub display_name: &'static str,
    /// The executable name discovery looks for on PATH.
    pub command: &'static str,
    /// The page a provider with no digest story sends the owner to.
    pub page: &'static str,
    /// The npm package that owns a global install of it, when one does.
    pub npm: Option<&'static str>,
    /// The WinGet package id, when one has been checked. None means the plan says
    /// "managed by WinGet" and refuses, rather than guessing an id.
    pub winget: Option<&'static str>,
    /// How a managed copy is obtained; `None` means the plan answers a link only.
    pub source: Option<Source>,
    /// The executable names an extracted archive may hold, in order.
    pub entries: &'static [&'static str],
}

/// The one table. A provider Retinue discovers but cannot obtain here answers a
/// link, which is the "settle the digest story first" half of the design.
///
/// The publisher's asset name for this platform is a constant rather than a
/// field, so one table serves Windows and Linux: every managed provider publishes
/// both (measured 28 November 2026 against the release APIs and the npm
/// registry).
///
/// The publisher's names for one platform: Codex's asset, Pi's asset, OpenCode's
/// npm platform package. Both answers are tested on every platform.
pub(crate) const fn platform_assets(windows: bool) -> (&'static str, &'static str, &'static str) {
    if windows {
        ("codex-x86_64-pc-windows-msvc.exe", "pi-windows-x64.zip", "opencode-windows-x64")
    } else {
        ("codex-x86_64-unknown-linux-musl.tar.gz", "pi-linux-x64.tar.gz", "opencode-linux-x64")
    }
}
// Test builds use the Windows names on every platform: the stub host in
// `provider_install_tests.rs` serves those files (zips and a bare .exe), and the
// tests exercise the download, check and publish pipeline, not the name.
const ASSETS: (&str, &str, &str) = platform_assets(cfg!(any(windows, test)));
const CODEX_ASSET: &str = ASSETS.0;
const PI_ASSET: &str = ASSETS.1;
const OPENCODE_PLATFORM: &str = ASSETS.2;

pub(crate) const ROWS: &[Row] = &[
    Row {
        provider: "claude",
        display_name: "Claude Code",
        command: "claude",
        page: "https://docs.claude.com/en/docs/claude-code/setup",
        npm: Some("@anthropic-ai/claude-code"),
        winget: None,
        source: Some(Source::ClaudeManifest),
        entries: &["claude.exe", "claude"],
    },
    Row {
        provider: "codex",
        display_name: "Codex",
        command: "codex",
        page: "https://github.com/openai/codex",
        npm: Some("@openai/codex"),
        winget: None,
        source: Some(Source::GitHubAsset { repo: "openai/codex", asset: CODEX_ASSET, sums: "codex-package_SHA256SUMS", tag_prefix: "rust-v" }),
        entries: &["codex.exe", "codex", "codex-x86_64-unknown-linux-musl"],
    },
    Row {
        provider: "opencode",
        display_name: "OpenCode",
        command: "opencode",
        page: "https://github.com/sst/opencode",
        npm: Some("opencode-ai"),
        winget: None,
        source: Some(Source::NpmPlatform { launcher: "opencode-ai", platform: OPENCODE_PLATFORM }),
        entries: &["opencode.exe", "opencode"],
    },
    Row {
        provider: "pi",
        display_name: "Pi",
        command: "pi",
        page: "https://pi.dev",
        npm: Some("@earendil-works/pi-coding-agent"),
        winget: None,
        source: Some(Source::GitHubAsset { repo: "earendil-works/pi", asset: PI_ASSET, sums: "SHA256SUMS", tag_prefix: "v" }),
        entries: &["pi.exe", "pi", "pi-linux-x64"],
    },
    Row { provider: "cursor", display_name: "Cursor CLI", command: "agent", page: "https://cursor.com/cli", npm: None, winget: None, source: None, entries: &[] },
    Row { provider: "grok", display_name: "Grok CLI", command: "grok", page: "https://x.ai/cli", npm: Some("@xai-official/grok"), winget: None, source: None, entries: &[] },
    Row { provider: "antigravity", display_name: "Antigravity", command: "agy", page: "https://antigravity.google/docs/getting-started", npm: None, winget: None, source: None, entries: &[] },
    Row { provider: "qwen", display_name: "Qwen Code", command: "qwen", page: "https://github.com/QwenLM/qwen-code", npm: Some("@qwen-code/qwen-code"), winget: None, source: None, entries: &[] },
];

fn row_for(provider: &str) -> Option<&'static Row> {
    ROWS.iter().find(|row| row.provider == provider)
}

/// The archive format of one download.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    /// The download is the executable itself.
    File,
    Zip,
    TarGz,
}

/// One publisher digest: the algorithm, and the value as lowercase hex.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Pin {
    pub algorithm: &'static str,
    pub hex: String,
}

impl Pin {
    /// The first 12 hex characters, which is all the consent card and the event
    /// ever name.
    pub(crate) fn prefix(&self) -> String {
        self.hex.chars().take(12).collect()
    }
}

// ---------------------------------------------------------------------------
// The settings that hold the owner's choice

pub(crate) fn mode_key(provider: &str) -> String {
    format!("agents.{provider}.mode")
}
pub(crate) fn consent_key(provider: &str) -> String {
    format!("agents.{provider}.consent")
}
pub(crate) fn auto_update_key(provider: &str) -> String {
    format!("agents.{provider}.autoUpdate")
}

/// The three modes, as stored.
pub(crate) const MODE_ADOPT: &str = "adopt";
pub(crate) const MODE_MANAGE: &str = "manage";
/// Not stored: a provider whose digest story is not settled answers a link.
pub(crate) const MODE_LINK: &str = "link";

/// `agents.<provider>.mode` and `agents.<provider>.autoUpdate` are the two keys a
/// client may write; the consent record is written by the installer alone.
pub(crate) fn validate_setting(key: &str, value: &Value) -> Result<(), DispatchError> {
    let Some(rest) = key.strip_prefix("agents.") else { return Ok(()) };
    let Some((provider, field)) = rest.split_once('.') else {
        return Err(derr("invalid_argument", "an agent install setting is agents.<provider>.mode or agents.<provider>.autoUpdate", StatusCode::BAD_REQUEST));
    };
    if row_for(provider).is_none() {
        return Err(derr("invalid_argument", "that coding agent is not one Retinue can install", StatusCode::BAD_REQUEST));
    }
    match field {
        "mode" => {
            if !value.as_str().is_some_and(|mode| matches!(mode, MODE_ADOPT | MODE_MANAGE)) {
                return Err(derr("invalid_argument", "agents.<provider>.mode must be adopt or manage", StatusCode::BAD_REQUEST));
            }
            Ok(())
        }
        "autoUpdate" => {
            if !value.is_boolean() {
                return Err(derr("invalid_argument", "agents.<provider>.autoUpdate requires a JSON boolean", StatusCode::BAD_REQUEST));
            }
            Ok(())
        }
        // The consent record names the version and the digest an install was
        // confirmed for. Only `runtime.install` writes it; a client that could set
        // it would be approving its own download.
        "consent" => Err(derr("invalid_argument", "the install consent record is written by runtime.install", StatusCode::BAD_REQUEST)),
        _ => Err(derr("invalid_argument", "an agent install setting is agents.<provider>.mode or agents.<provider>.autoUpdate", StatusCode::BAD_REQUEST)),
    }
}

/// The stored choice, or the default: Adopt when a CLI was found, Manage when not.
fn chosen_mode(st: &AppState, row: &Row) -> &'static str {
    let stored = st.db.settings().ok().and_then(|settings| settings[mode_key(row.provider)].as_str().map(str::to_owned));
    match stored.as_deref() {
        Some(MODE_MANAGE) => MODE_MANAGE,
        Some(MODE_ADOPT) => MODE_ADOPT,
        _ if retinue_runtime::resolve_command(row.command).is_some() => MODE_ADOPT,
        _ => MODE_MANAGE,
    }
}

fn auto_update_on(st: &AppState, provider: &str) -> bool {
    st.db.settings().ok().and_then(|settings| settings[auto_update_key(provider)].as_bool()).unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Jobs

/// One install in flight, or the last one for a provider this boot.
pub(crate) struct Job {
    pub id: String,
    pub provider: String,
    pub mode: &'static str,
    pub phase: &'static str,
    pub version: Option<String>,
    pub bytes_done: u64,
    pub bytes_total: u64,
    /// The installer's own last line, redacted and bounded. Never a path.
    pub line: Option<String>,
    /// One fixed failure sentence.
    pub error: Option<String>,
    cancel: Arc<AtomicBool>,
}

impl Job {
    fn finished(&self) -> bool {
        matches!(self.phase, "done" | "failed" | "cancelled")
    }
    pub(crate) fn value(&self) -> Value {
        json!({
            "jobId": self.id,
            "provider": self.provider,
            "mode": self.mode,
            "phase": self.phase,
            "version": self.version,
            "bytesDone": self.bytes_done,
            "bytesTotal": self.bytes_total,
            "line": self.line,
            "error": self.error,
        })
    }
}

/// Every job, and which provider each one holds. One install per provider at a
/// time; a second request answers the first one's job id rather than starting.
#[derive(Default)]
pub(crate) struct Installs {
    jobs: StdMutex<HashMap<String, Job>>,
    active: StdMutex<HashMap<String, String>>,
}

impl Installs {
    fn start(&self, provider: &str, mode: &'static str, version: Option<String>, total: u64) -> Result<String, String> {
        let mut active = self.active.lock().unwrap();
        if let Some(running) = active.get(provider) {
            return Err(running.clone());
        }
        let id = uuid::Uuid::new_v4().to_string();
        let job = Job { id: id.clone(), provider: provider.to_owned(), mode, phase: "downloading", version, bytes_done: 0, bytes_total: total, line: None, error: None, cancel: Arc::new(AtomicBool::new(false)) };
        self.jobs.lock().unwrap().insert(id.clone(), job);
        active.insert(provider.to_owned(), id.clone());
        Ok(id)
    }

    pub(crate) fn with<R>(&self, id: &str, read: impl FnOnce(&Job) -> R) -> Option<R> {
        self.jobs.lock().unwrap().get(id).map(read)
    }

    fn update(&self, id: &str, change: impl FnOnce(&mut Job)) -> Option<Value> {
        let mut jobs = self.jobs.lock().unwrap();
        let job = jobs.get_mut(id)?;
        change(job);
        let provider = job.provider.clone();
        let finished = job.finished();
        let value = job.value();
        drop(jobs);
        if finished {
            self.release(&provider, id);
        }
        Some(value)
    }

    fn release(&self, provider: &str, id: &str) {
        let mut active = self.active.lock().unwrap();
        if active.get(provider).map(String::as_str) == Some(id) {
            active.remove(provider);
        }
    }

    fn cancel_signal(&self, id: &str) -> Option<Arc<AtomicBool>> {
        self.jobs.lock().unwrap().get(id).map(|job| job.cancel.clone())
    }
}

// ---------------------------------------------------------------------------
// Routing

pub(crate) async fn rpc(st: &AppState, method: &str, p: &Value) -> Result<Value, DispatchError> {
    match method {
        "runtime.install.plan" => plan_reply(st, provider_param(p)?).await,
        "runtime.install" => install(st, p).await,
        "runtime.install.status" => status(st, p),
        "runtime.install.cancel" => cancel(st, p),
        "runtime.install.remove" => remove(st, p).await,
        _ => Err(derr("unsupported", "method is not supported", StatusCode::NOT_FOUND)),
    }
}

fn provider_param(p: &Value) -> Result<&str, DispatchError> {
    p["provider"].as_str().map(str::trim).filter(|provider| !provider.is_empty()).ok_or_else(|| derr("invalid_argument", "provider is required", StatusCode::BAD_REQUEST))
}

/// `runtime.install.plan`: what installing would do, and everything the consent
/// card names. It reads the publisher's own release data, so it needs the
/// network; nothing is written.
pub(crate) async fn plan_reply(st: &AppState, provider: &str) -> Result<Value, DispatchError> {
    Ok(json!({"plan": build_plan(st, provider).await?.value()}))
}

// ---------------------------------------------------------------------------
// The plan

/// What the owner is being asked to allow, and everything the install needs.
pub(crate) struct Plan {
    provider: &'static str,
    display_name: &'static str,
    mode: &'static str,
    version: Option<String>,
    pin: Option<Pin>,
    bytes: Option<u64>,
    source_host: Option<&'static str>,
    url: Option<String>,
    kind: Option<Kind>,
    /// The executable inside a [`Kind::File`] download.
    file_name: Option<String>,
    entries: &'static [&'static str],
    target: PathBuf,
    free_bytes: Option<u64>,
    runs_turn: bool,
    supported: bool,
    reason: Option<&'static str>,
    page: &'static str,
    installed_version: Option<String>,
    installed_digest: Option<String>,
    damaged: bool,
    previous_version: Option<String>,
    owner: Option<&'static str>,
    adopt: Option<Adopt>,
    auto_update: bool,
}

/// The command that updates the owner's own copy.
#[derive(Clone, Debug)]
pub(crate) struct Adopt {
    pub program: PathBuf,
    pub args: Vec<String>,
    /// What to call the owner in one sentence, when it cannot be run at all.
    pub refusal: Option<&'static str>,
}

impl Plan {
    fn value(&self) -> Value {
        json!({
            "provider": self.provider,
            "displayName": self.display_name,
            "mode": self.mode,
            "version": self.version,
            "sourceHost": self.source_host,
            "bytes": self.bytes,
            "digestPrefix": self.pin.as_ref().map(Pin::prefix),
            "digestAlgorithm": self.pin.as_ref().map(|pin| pin.algorithm),
            "target": self.target.display().to_string(),
            "freeBytes": self.free_bytes,
            "runsTurn": self.runs_turn,
            "supported": self.supported,
            "reason": self.reason,
            "page": self.page,
            "owner": self.owner,
            "installed": {
                "version": self.installed_version,
                "digestPrefix": self.installed_digest.as_ref().map(|digest| digest.chars().take(12).collect::<String>()),
                "damaged": self.damaged,
            },
            "previousVersion": self.previous_version,
            "autoUpdate": self.auto_update,
        })
    }
}

/// Where Retinue's own copies live, or `None` from a daemon that has no data
/// folder yet (a unit test's bare state).
pub(crate) fn managed_root(st: &AppState) -> Option<PathBuf> {
    sandbox::data_folder(st).map(|data| data.join(retinue_runtime::MANAGED_FOLDER))
}

/// Whether this build can obtain a managed copy. Windows and Linux ship assets;
/// the stub origin stands in for a host during tests on any platform, and a host
/// other than those two still answers a link.
fn platform_supported() -> bool {
    cfg!(windows) || cfg!(target_os = "linux") || test_origin().is_some()
}

/// Whether the named provider has a turn running, from the daemon's own two maps
/// (a session's runtime, and the turns that are live).
pub(crate) async fn provider_has_running_turn(st: &AppState, provider: &str) -> bool {
    let active = st.active_turns.lock().await;
    if active.is_empty() {
        return false;
    }
    let sessions = st.sessions.lock().await;
    sessions.iter().any(|(id, session)| active.contains_key(id) && session.runtime.provider == provider)
}

pub(crate) async fn build_plan(st: &AppState, provider: &str) -> Result<Plan, DispatchError> {
    let row = row_for(provider).ok_or_else(|| derr("invalid_argument", "that coding agent is not one Retinue can install", StatusCode::BAD_REQUEST))?;
    let root = managed_root(st);
    let folder = root.as_ref().map(|root| retinue_runtime::managed_folder(root, provider)).unwrap_or_else(|| PathBuf::from(retinue_runtime::MANAGED_FOLDER).join(provider));
    let managed = root.as_ref().and_then(|root| retinue_runtime::managed_entry(root, provider));
    let receipt_exists = folder.join(retinue_runtime::MANAGED_RECEIPT).is_file();
    let damaged = receipt_exists && managed.is_none();
    let previous_version = managed.as_ref().and_then(|_| read_previous_version(&folder));
    let mut plan = Plan {
        provider: row.provider,
        display_name: row.display_name,
        mode: chosen_mode(st, row),
        version: None,
        pin: None,
        bytes: None,
        source_host: None,
        url: None,
        kind: None,
        file_name: None,
        entries: row.entries,
        target: folder.clone(),
        free_bytes: sandbox::data_folder(st).and_then(|data| crate::upkeep::free_bytes(&data)),
        runs_turn: provider_has_running_turn(st, provider).await,
        supported: true,
        reason: None,
        page: row.page,
        installed_version: managed.as_ref().map(|entry| entry.version.clone()),
        installed_digest: managed.as_ref().map(|entry| entry.digest.clone()),
        damaged,
        previous_version,
        owner: None,
        adopt: None,
        auto_update: auto_update_on(st, row.provider),
    };
    // A provider whose digest story is not settled: the window shows the page.
    let Some(source) = row.source else {
        plan.mode = MODE_LINK;
        plan.supported = false;
        plan.reason = Some("Retinue does not download this coding agent yet. Open its install page instead.");
        return Ok(plan);
    };
    if plan.mode == MODE_ADOPT {
        let Some(found) = retinue_runtime::resolve_command(row.command) else {
            // Nothing to adopt: the sensible default is Manage, and the row says so.
            plan.mode = MODE_MANAGE;
            plan.owner = Some("none");
            return self::managed_plan(plan, source).await;
        };
        let adopt = adopt_command(row, &found.executable);
        let owner = classify(&found.executable);
        plan.owner = Some(owner);
        plan.version = Some(latest_version(row).await?);
        plan.source_host = Some(adopt_host(row, owner));
        plan.supported = adopt.refusal.is_none();
        plan.reason = adopt.refusal;
        plan.adopt = Some(adopt);
        // An owner-owned update writes into the owner's only copy, so it must
        // never run beside a turn (design 8.6).
        return Ok(plan);
    }
    self::managed_plan(plan, source).await
}

async fn managed_plan(mut plan: Plan, source: Source) -> Result<Plan, DispatchError> {
    plan.mode = MODE_MANAGE;
    if !platform_supported() {
        plan.supported = false;
        plan.reason = Some("One-click installs are for Windows and Linux in this version.");
        return Ok(plan);
    }
    let resolved = match source {
        Source::ClaudeManifest => claude_release().await?,
        Source::GitHubAsset { repo, asset, sums, tag_prefix } => github_release(repo, asset, sums, tag_prefix).await?,
        Source::NpmPlatform { launcher, platform } => npm_release(launcher, platform).await?,
    };
    plan.version = Some(resolved.version);
    plan.pin = Some(resolved.pin);
    plan.bytes = resolved.bytes;
    plan.source_host = Some(resolved.host);
    plan.url = Some(resolved.url);
    plan.kind = Some(resolved.kind);
    plan.file_name = resolved.file_name;
    let needed = plan.bytes.unwrap_or(0) * 2;
    if let (Some(free), true) = (plan.free_bytes, plan.bytes.is_some()) {
        if free < needed {
            plan.supported = false;
            plan.reason = Some("There is not enough free disk space for this download and its unpacked copy.");
        }
    }
    Ok(plan)
}

/// The version of a managed release from the publisher's own channel.
struct Release {
    version: String,
    pin: Pin,
    bytes: Option<u64>,
    host: &'static str,
    url: String,
    kind: Kind,
    file_name: Option<String>,
}

// ---------------------------------------------------------------------------
// The publishers

fn platform_key() -> &'static str {
    if cfg!(windows) {
        "win32-x64"
    } else {
        "linux-x64"
    }
}

async fn claude_release() -> Result<Release, DispatchError> {
    let client = client()?;
    let base = origin("downloads.claude.ai");
    let version = get_text(&client, &format!("{base}/claude-code-releases/stable"), MAX_META_BYTES).await?.trim().to_owned();
    if !retinue_runtime::safe_version_folder(&version) {
        return Err(publisher_error("The publisher's version could not be read."));
    }
    let manifest = get_json(&client, &format!("{base}/claude-code-releases/{version}/manifest.json")).await?;
    let entry = &manifest["platforms"][platform_key()];
    let binary = entry["binary"].as_str().filter(|name| retinue_runtime::safe_relative_entry(name)).ok_or_else(|| publisher_error("The publisher lists no build for this computer."))?;
    let checksum = entry["checksum"].as_str().filter(|hex| is_hex(hex)).ok_or_else(|| publisher_error("The publisher publishes no checksum for this download."))?;
    let size = entry["size"].as_u64();
    if size.is_some_and(|bytes| bytes > MAX_DOWNLOAD_BYTES) {
        return Err(publisher_error("The publisher's download is larger than Retinue will fetch."));
    }
    let file_name = binary.rsplit(['/', '\\']).next().unwrap_or(binary).to_owned();
    Ok(Release {
        url: format!("{base}/claude-code-releases/{version}/{}/{binary}", platform_key()),
        version,
        pin: Pin { algorithm: "sha256", hex: checksum.to_ascii_lowercase() },
        bytes: size,
        host: "downloads.claude.ai",
        kind: Kind::File,
        file_name: Some(file_name),
    })
}

async fn github_release(repo: &'static str, asset_name: &'static str, sums_name: &'static str, tag_prefix: &'static str) -> Result<Release, DispatchError> {
    let client = client()?;
    let api = origin("api.github.com");
    let release = get_json(&client, &format!("{api}/repos/{repo}/releases/latest")).await?;
    let tag = release["tag_name"].as_str().ok_or_else(|| publisher_error("The publisher's release could not be read."))?;
    let version = tag.strip_prefix(tag_prefix).unwrap_or(tag).to_owned();
    if !retinue_runtime::safe_version_folder(&version) {
        return Err(publisher_error("The publisher's version could not be read."));
    }
    let assets = release["assets"].as_array().ok_or_else(|| publisher_error("The publisher's release could not be read."))?;
    let asset = assets.iter().find(|asset| asset["name"].as_str() == Some(asset_name)).ok_or_else(|| publisher_error("The publisher's release has no build for this computer."))?;
    let url = asset["browser_download_url"].as_str().ok_or_else(|| publisher_error("The publisher's release could not be read."))?.to_owned();
    let size = asset["size"].as_u64();
    if size.is_some_and(|bytes| bytes > MAX_DOWNLOAD_BYTES) {
        return Err(publisher_error("The publisher's download is larger than Retinue will fetch."));
    }
    // GitHub's own `digest` field is the publisher's checksum; else the release's
    // SHA256SUMS beside it.
    let pin = match asset["digest"].as_str().and_then(|digest| digest.strip_prefix("sha256:")) {
        Some(hex) if is_hex(hex) => Pin { algorithm: "sha256", hex: hex.to_ascii_lowercase() },
        _ => {
            let sums = assets.iter().find(|asset| asset["name"].as_str() == Some(sums_name)).ok_or_else(|| publisher_error("The publisher publishes no checksum for this download."))?;
            let sums_url = sums["browser_download_url"].as_str().ok_or_else(|| publisher_error("The publisher publishes no checksum for this download."))?;
            let text = get_text(&client, sums_url, MAX_META_BYTES).await?;
            let hex = sums_lookup(&text, asset_name).ok_or_else(|| publisher_error("The publisher publishes no checksum for this download."))?;
            Pin { algorithm: "sha256", hex }
        }
    };
    let kind = if asset_name.ends_with(".zip") { Kind::Zip } else if asset_name.ends_with(".tar.gz") { Kind::TarGz } else { Kind::File };
    Ok(Release { url, version, pin, bytes: size, host: "api.github.com", kind, file_name: (kind == Kind::File).then(|| asset_name.to_owned()) })
}

async fn npm_release(launcher: &'static str, platform: &'static str) -> Result<Release, DispatchError> {
    let client = client()?;
    let registry = origin("registry.npmjs.org");
    let latest = get_json(&client, &format!("{registry}/{launcher}/latest")).await?;
    let version = latest["version"].as_str().ok_or_else(|| publisher_error("The publisher's version could not be read."))?.to_owned();
    if !retinue_runtime::safe_version_folder(&version) {
        return Err(publisher_error("The publisher's version could not be read."));
    }
    // The real binary is a platform package, pinned by the launcher's own
    // optionalDependencies (design 8.2, finding 1).
    let platform_version = latest["optionalDependencies"][platform].as_str().filter(|value| retinue_runtime::safe_version_folder(value)).unwrap_or(&version);
    let package = get_json(&client, &format!("{registry}/{platform}/{platform_version}")).await?;
    let integrity = package["dist"]["integrity"].as_str().ok_or_else(|| publisher_error("The publisher publishes no checksum for this download."))?;
    let pin = integrity_pin(integrity).ok_or_else(|| publisher_error("The publisher publishes no checksum for this download."))?;
    let url = package["dist"]["tarball"].as_str().ok_or_else(|| publisher_error("The publisher's release could not be read."))?.to_owned();
    check_url(&url)?;
    // npm states an unpacked size, not the tarball's own, so the plan says the size is not stated rather than showing the wrong one.
    Ok(Release { url, version, pin, bytes: None, host: "registry.npmjs.org", kind: Kind::TarGz, file_name: None })
}

/// The version an owner-owned install should move to. npm publishes it for every
/// CLI Retinue can adopt; a failure is one fixed sentence.
async fn latest_version(row: &'static Row) -> Result<String, DispatchError> {
    let Some(package) = row.npm else {
        return Err(publisher_error("Retinue cannot check this coding agent's latest version."));
    };
    let client = client()?;
    let latest = get_json(&client, &format!("{}/{package}/latest", origin("registry.npmjs.org"))).await?;
    latest["version"].as_str().map(str::to_owned).ok_or_else(|| publisher_error("The publisher's version could not be read."))
}

// ---------------------------------------------------------------------------
// The network, under the allowlist

fn test_origin() -> Option<String> {
    TEST_ORIGIN.lock().ok().and_then(|origin| origin.clone())
}

/// The base for a publisher's host: the real one in production, the stub origin
/// in a test.
fn origin(host: &str) -> String {
    test_origin().unwrap_or_else(|| format!("https://{host}"))
}

fn client() -> Result<reqwest::Client, DispatchError> {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 5 {
                attempt.error("too many redirects")
            } else if allowed_url(attempt.url()) {
                attempt.follow()
            } else {
                attempt.error("a redirect left the allowlisted hosts")
            }
        }))
        // No proxy: a publisher is reached directly, so no third party sees the
        // request and no environment value points somewhere else.
        .no_proxy()
        .timeout(REQUEST_TIMEOUT)
        .build()
        .map_err(|_| derr("internal", "The download client could not start.", StatusCode::INTERNAL_SERVER_ERROR))
}

/// HTTPS from a named host, and in a test the stub origin (never HTTPS). No
/// proxy: the builder above sets none, and reqwest reads no proxy here because
/// the environment's proxy variables are not consulted for these URLs.
pub(crate) fn allowed_url(url: &reqwest::Url) -> bool {
    if let Some(test) = test_origin() {
        if url.as_str().starts_with(&test) {
            return true;
        }
    }
    url.scheme() == "https" && url.host_str().is_some_and(|host| ALLOWED_HOSTS.contains(&host))
}

fn check_url(url: &str) -> Result<(), DispatchError> {
    let parsed = reqwest::Url::parse(url).map_err(|_| publisher_error("The publisher's address could not be read."))?;
    if allowed_url(&parsed) {
        Ok(())
    } else {
        Err(publisher_error("The publisher's download is not from an address Retinue accepts."))
    }
}

fn publisher_error(message: &'static str) -> DispatchError {
    derr("provider_error", message, StatusCode::BAD_GATEWAY)
}

/// One publisher answer as text, capped.
async fn get_text(client: &reqwest::Client, url: &str, cap: u64) -> Result<String, DispatchError> {
    check_url(url)?;
    let mut response = client.get(url).send().await.map_err(|_| derr("provider_unavailable", "Could not reach the publisher.", StatusCode::BAD_GATEWAY))?;
    if !response.status().is_success() {
        return Err(derr("provider_unavailable", "The publisher answered with an error.", StatusCode::BAD_GATEWAY));
    }
    let mut text = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| derr("provider_unavailable", "The publisher's answer stopped partway through.", StatusCode::BAD_GATEWAY))? {
        if text.len() as u64 + chunk.len() as u64 > cap {
            return Err(publisher_error("The publisher's answer was larger than Retinue reads."));
        }
        text.extend_from_slice(&chunk);
    }
    Ok(String::from_utf8_lossy(&text).into_owned())
}

async fn get_json(client: &reqwest::Client, url: &str) -> Result<Value, DispatchError> {
    let text = get_text(client, url, MAX_META_BYTES).await?;
    serde_json::from_str(&text).map_err(|_| publisher_error("The publisher's answer could not be read."))
}

/// The hex digest a `SHA256SUMS` line gives for one file, either `<hex>  <name>`
/// or `<hex> *<name>`.
pub(crate) fn sums_lookup(text: &str, name: &str) -> Option<String> {
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        let (Some(hex), Some(file)) = (parts.next(), parts.next()) else { continue };
        if file.trim_start_matches('*') == name && is_hex(hex) && hex.len() == 64 {
            return Some(hex.to_ascii_lowercase());
        }
    }
    None
}

/// npm's `integrity` (`sha512-<base64>`, and sha256) as a [`Pin`]. The first
/// entry wins; npm lists the strongest first.
pub(crate) fn integrity_pin(integrity: &str) -> Option<Pin> {
    use base64::Engine as _;
    for entry in integrity.split_whitespace() {
        let Some((algorithm, value)) = entry.split_once('-') else { continue };
        let algorithm = match algorithm {
            "sha512" => "sha512",
            "sha256" => "sha256",
            _ => continue,
        };
        let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(value) else { continue };
        let expected = if algorithm == "sha512" { 64 } else { 32 };
        if bytes.len() != expected {
            continue;
        }
        return Some(Pin { algorithm, hex: hex_encode(&bytes) });
    }
    None
}

fn is_hex(text: &str) -> bool {
    !text.is_empty() && text.len() <= 128 && text.chars().all(|c| c.is_ascii_hexdigit())
}

pub(crate) fn hex_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(char::from_digit((byte >> 4) as u32, 16).unwrap_or('0'));
        out.push(char::from_digit((byte & 0xf) as u32, 16).unwrap_or('0'));
    }
    out
}

// ---------------------------------------------------------------------------
// Adopting the owner's own copy

/// The npm global folder under the user's own profile, a WinGet link folder, a
/// user-scope install folder, or a machine-wide one. Windows paths and Unix
/// paths are read by the same rule, with the separators normalised first.
pub(crate) fn classify(executable: &Path) -> &'static str {
    let path = executable.to_string_lossy().replace('\\', "/").to_ascii_lowercase();
    let npm_owned = [
        "/appdata/roaming/npm/", // npm's global bin on Windows
        "/appdata/local/pnpm/",
        "/.bun/bin/",            // bun's global bin, both platforms
        "/.npm-global/",
        "/.config/yarn/global/",
        "/.local/share/pnpm/",
        "/.volta/bin/",
        "/.nvm/",
        "/.fnm/",
    ];
    if npm_owned.iter().any(|needle| path.contains(*needle)) {
        "npm"
    } else if path.contains("/microsoft/winget/") {
        "winget"
    } else if path.contains("/program files")
        || path.contains("/programdata/chocolatey/")
        || path.contains("/windows/system32")
        // On Linux a copy under /usr, /opt, /nix or /snap belongs to the system
        // package manager (or root); /var/lib covers a distro's own installs.
        || path.starts_with("/usr/")
        || path.starts_with("/opt/")
        || path.starts_with("/nix/store/")
        || path.starts_with("/snap/")
        || path.starts_with("/var/lib/")
    {
        "system"
    } else {
        "own"
    }
}

/// Where an owner-owned update fetches from, in the words the card shows: the
/// registry when npm owns the copy, WinGet's own source when WinGet does, and
/// otherwise the provider's own release host from the table. A card always names
/// a host, never "the publisher".
fn adopt_host(row: &Row, owner: &str) -> &'static str {
    match owner {
        "npm" => "registry.npmjs.org",
        "winget" => "WinGet",
        _ => match row.source {
            Some(Source::ClaudeManifest) => "downloads.claude.ai",
            Some(Source::GitHubAsset { .. }) => "api.github.com",
            Some(Source::NpmPlatform { .. }) => "registry.npmjs.org",
            None => "the publisher",
        },
    }
}

/// The command that updates the owner's own copy, or the one sentence saying why
/// Retinue will not run one. Never elevated, never machine-wide.
pub(crate) fn adopt_command(row: &Row, executable: &Path) -> Adopt {
    match classify(executable) {
        "npm" => match row.npm.map(|package| (retinue_runtime::resolve_command("npm"), package)) {
            Some((Some(npm), package)) => Adopt { program: npm.executable, args: vec!["install".into(), "-g".into(), format!("{package}@latest")], refusal: None },
            _ => Adopt { program: executable.to_path_buf(), args: vec![], refusal: Some("npm could not be found, so Retinue cannot update this install. Choose Manage instead.") },
        },
        // A WinGet-owned link is updated by WinGet itself. Without a checked
        // package id Retinue will not guess one, so it says so.
        "winget" => match row.winget {
            Some(id) => match retinue_runtime::resolve_command("winget") {
                Some(winget) => Adopt { program: winget.executable, args: vec!["upgrade".into(), "--id".into(), id.into(), "--scope".into(), "user".into(), "--silent".into(), "--accept-package-agreements".into(), "--accept-source-agreements".into()], refusal: None },
                None => Adopt { program: executable.to_path_buf(), args: vec![], refusal: Some("WinGet could not be found, so Retinue cannot update this install. Choose Manage instead.") },
            },
            None => Adopt { program: executable.to_path_buf(), args: vec![], refusal: Some("This install is managed by WinGet. Update it from WinGet; Retinue will not run another installer on it.") },
        },
        "system" => Adopt { program: executable.to_path_buf(), args: vec![], refusal: Some("This install belongs to a system-wide package manager; Retinue will not raise an admin prompt.") },
        // The CLI's own updater, which both Orca and T3 Code use when nothing
        // else owns the copy.
        _ => match row.provider {
            "claude" => Adopt { program: executable.to_path_buf(), args: vec!["update".into()], refusal: None },
            "codex" => Adopt { program: executable.to_path_buf(), args: vec!["update".into()], refusal: None },
            "opencode" => Adopt { program: executable.to_path_buf(), args: vec!["upgrade".into()], refusal: None },
            _ => match row.npm {
                Some(package) => match retinue_runtime::resolve_command("npm") {
                    Some(npm) => Adopt { program: npm.executable, args: vec!["install".into(), "-g".into(), format!("{package}@latest")], refusal: None },
                    None => Adopt { program: executable.to_path_buf(), args: vec![], refusal: Some("npm could not be found, so Retinue cannot update this install. Choose Manage instead.") },
                },
                None => Adopt { program: executable.to_path_buf(), args: vec![], refusal: Some("Retinue cannot tell what installed this coding agent. Choose Manage instead.") },
            },
        },
    }
}

// ---------------------------------------------------------------------------
// Starting an install

async fn install(st: &AppState, p: &Value) -> Result<Value, DispatchError> {
    let provider = provider_param(p)?;
    if p["confirm"] != true {
        return Err(derr("invalid_argument", "Confirm the install first: runtime.install needs confirm: true.", StatusCode::BAD_REQUEST));
    }
    let plan = build_plan(st, provider).await?;
    if !plan.supported {
        return Err(derr("provider_unavailable", plan.reason.unwrap_or("This coding agent cannot be installed here."), StatusCode::CONFLICT));
    }
    let Some(version) = plan.version.clone() else {
        return Err(derr("provider_unavailable", "The publisher's version could not be read.", StatusCode::BAD_GATEWAY));
    };
    // Consent is bound to what the owner was shown: the version, and the first 12
    // hex characters of the digest. A publisher that moves the release between the
    // card and the click is a plan change, not an install.
    //
    // The one exception is `agents.<provider>.autoUpdate` with a consent already
    // stored for this same source host: then a caller may omit both, and the
    // daemon still installs only the version and digest its own plan just read.
    // The switch changes who sees a card, never what is checked.
    let shown = p["version"].as_str().is_some() || p["digestPrefix"].as_str().is_some();
    if !shown && !auto_update_allowed(st, &plan) {
        return Err(derr("invalid_argument", "runtime.install needs the version and digestPrefix the plan showed, or auto-update on for this source.", StatusCode::BAD_REQUEST));
    }
    if shown {
        if p["version"].as_str() != Some(version.as_str()) {
            return Err(derr("conflict", "The plan changed since it was shown. Review it again.", StatusCode::CONFLICT));
        }
        if let Some(pin) = plan.pin.as_ref() {
            let digest = p["digestPrefix"].as_str().unwrap_or("");
            if !digest.eq_ignore_ascii_case(&pin.prefix()) {
                return Err(derr("conflict", "The plan changed since it was shown. Review it again.", StatusCode::CONFLICT));
            }
        }
    }
    if plan.mode == MODE_ADOPT && plan.runs_turn {
        return Err(derr("conflict", "A bot is using this coding agent. Try again when its turn has finished.", StatusCode::CONFLICT));
    }
    if plan.mode == MODE_MANAGE {
        if let (Some(free), Some(bytes)) = (plan.free_bytes, plan.bytes) {
            if free < bytes.saturating_mul(2) {
                return Err(derr("conflict", "There is not enough free disk space for this download and its unpacked copy.", StatusCode::CONFLICT));
            }
        }
    }
    let job_id = match st.agent_install.start(provider, plan.mode, Some(version.clone()), plan.bytes.unwrap_or(0)) {
        Ok(id) => id,
        // One install per provider: a second click joins the first.
        Err(running) => return Ok(json!({"jobId": running, "provider": provider, "joined": true})),
    };
    record_consent(st, &plan)?;
    let state = st.clone();
    let job = job_id.clone();
    let mode = plan.mode;
    tokio::spawn(async move { run_job(state, job, plan).await });
    Ok(json!({"jobId": job_id, "provider": provider, "mode": mode, "version": version}))
}

/// `agents.<provider>.consent`: the version and digest the owner confirmed. With
/// `agents.<provider>.autoUpdate` off (the default) a later version needs its own
/// card; the installer writes this, never a client.
fn record_consent(st: &AppState, plan: &Plan) -> Result<(), DispatchError> {
    let consent = json!({
        "version": plan.version,
        "digest": plan.pin.as_ref().map(|pin| pin.hex.clone()),
        "sourceHost": plan.source_host,
        "at": chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        "autoUpdate": auto_update_on(st, plan.provider),
    });
    st.db.set_setting(&consent_key(plan.provider), &consent).map_err(|_| derr("internal", "The install could not be recorded.", StatusCode::INTERNAL_SERVER_ERROR))?;
    Ok(())
}

/// Whether a plan may be installed without the caller repeating the version and
/// digest: only when the owner turned `autoUpdate` on for this provider, and a
/// consent is already stored for the same source host. A different host always
/// needs its own card, whatever the switch says.
fn auto_update_allowed(st: &AppState, plan: &Plan) -> bool {
    if !plan.auto_update {
        return false;
    }
    let Some(host) = plan.source_host else { return false };
    let settings = st.db.settings().unwrap_or(Value::Null);
    settings[consent_key(plan.provider)]["sourceHost"].as_str() == Some(host)
}

fn status(st: &AppState, p: &Value) -> Result<Value, DispatchError> {
    let id = p["jobId"].as_str().filter(|id| !id.is_empty()).ok_or_else(|| derr("invalid_argument", "jobId is required", StatusCode::BAD_REQUEST))?;
    st.agent_install.with(id, Job::value).map(|job| json!({"job": job})).ok_or_else(|| derr("not_found", "That install is not known here.", StatusCode::NOT_FOUND))
}

fn cancel(st: &AppState, p: &Value) -> Result<Value, DispatchError> {
    let id = p["jobId"].as_str().filter(|id| !id.is_empty()).ok_or_else(|| derr("invalid_argument", "jobId is required", StatusCode::BAD_REQUEST))?;
    let signal = st.agent_install.cancel_signal(id).ok_or_else(|| derr("not_found", "That install is not known here.", StatusCode::NOT_FOUND))?;
    signal.store(true, Ordering::SeqCst);
    Ok(json!({"cancellationRequested": true}))
}

async fn remove(st: &AppState, p: &Value) -> Result<Value, DispatchError> {
    let provider = provider_param(p)?.to_owned();
    if p["confirm"] != true {
        return Err(derr("invalid_argument", "Confirm removing the copy first: runtime.install.remove needs confirm: true.", StatusCode::BAD_REQUEST));
    }
    let _row = row_for(&provider).ok_or_else(|| derr("invalid_argument", "that coding agent is not one Retinue can install", StatusCode::BAD_REQUEST))?;
    if provider_has_running_turn(st, &provider).await {
        return Err(derr("conflict", "A bot is using this coding agent. Stop it before removing Retinue's copy.", StatusCode::CONFLICT));
    }
    let root = managed_root(st).ok_or_else(|| derr("unsupported", "Retinue has no data folder here.", StatusCode::CONFLICT))?;
    let folder = retinue_runtime::managed_folder(&root, &provider);
    if !folder.is_dir() {
        return Err(derr("not_found", "Retinue did not install this coding agent.", StatusCode::NOT_FOUND));
    }
    // The receipt first: discovery stops using the copy before anything is deleted.
    let _ = std::fs::remove_file(folder.join(retinue_runtime::MANAGED_RECEIPT));
    let mut kept = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&folder) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            if std::fs::remove_dir_all(&path).is_err() {
                kept.push(entry.file_name().to_string_lossy().into_owned());
            }
        }
    }
    // The provider folder itself goes once nothing is left in it; a folder that is
    // still in use stays, and the next remove or sweep finishes the job.
    if kept.is_empty() {
        let _ = std::fs::remove_dir_all(&folder);
    }
    rescan(st).await;
    Ok(json!({"removed": true, "provider": provider, "kept": kept}))
}

// ---------------------------------------------------------------------------
// Running the job

async fn run_job(st: AppState, id: String, plan: Plan) {
    let outcome = tokio::time::timeout(INSTALL_LIMIT, async {
        match plan.mode {
            MODE_ADOPT => adopt(&st, &id, &plan).await,
            _ => managed(&st, &id, &plan).await,
        }
    })
    .await;
    match outcome {
        Ok(Ok(())) => {
            if let Some(value) = st.agent_install.update(&id, |job| job.phase = "done") {
                announce(&st, value).await;
            }
        }
        Ok(Err(Failure::Cancelled)) => {
            if let Some(value) = st.agent_install.update(&id, |job| { job.phase = "cancelled"; job.error = Some("The install was cancelled. Nothing was changed.".into()); }) {
                announce(&st, value).await;
            }
        }
        Ok(Err(Failure::Message(message))) => {
            diagnostics::record_failure("agent_install", "failed");
            if let Some(value) = st.agent_install.update(&id, |job| { job.phase = "failed"; job.error = Some(message.into()); }) {
                announce(&st, value).await;
            }
        }
        Err(_) => {
            diagnostics::record_failure("agent_install", "timeout");
            if let Some(value) = st.agent_install.update(&id, |job| { job.phase = "failed"; job.error = Some("The install took longer than 30 minutes and was stopped.".into()); }) {
                announce(&st, value).await;
            }
        }
    }
    if let Some(root) = managed_root(&st) {
        sweep_provider(&root, plan.provider);
    }
}

/// A job's one event: provider, phase, version and bytes. No path, no URL.
async fn announce(st: &AppState, job: Value) {
    st.emit(EVENT, json!({
        "provider": job["provider"],
        "phase": job["phase"],
        "version": job["version"],
        "bytesDone": job["bytesDone"],
        "bytesTotal": job["bytesTotal"],
    }))
    .await;
}

enum Failure {
    Cancelled,
    Message(&'static str),
}

impl Failure {
    fn from_dispatch(error: DispatchError) -> Self {
        // Only this module's own fixed sentences leave the job; a publisher's
        // text never does.
        let _ = error;
        Failure::Message("The publisher's address could not be read. Nothing was installed.")
    }
}

fn set_phase(st: &AppState, id: &str, phase: &'static str, line: Option<String>) -> Option<Value> {
    st.agent_install.update(id, |job| {
        job.phase = phase;
        if line.is_some() {
            job.line = line;
        }
    })
}

async fn progress(st: &AppState, id: &str, done: u64, total: u64) {
    let changed = st.agent_install.update(id, |job| {
        job.bytes_done = done;
        job.bytes_total = total.max(job.bytes_total);
    });
    if let Some(value) = changed {
        announce(st, value).await;
    }
}

// ---------------------------------------------------------------------------
// Manage: download, check, unpack, publish

async fn managed(st: &AppState, id: &str, plan: &Plan) -> Result<(), Failure> {
    let Some(folder) = managed_root(st).map(|root| retinue_runtime::managed_folder(&root, plan.provider)) else {
        return Err(Failure::Message("Retinue has no data folder here."));
    };
    let staging = folder.join("staging").join(uuid::Uuid::new_v4().to_string());
    std::fs::create_dir_all(&staging).map_err(|_| Failure::Message("The install folder could not be created."))?;
    let _ = sandbox::restrict(&staging, true);
    let outcome = stage(st, id, plan, &folder, &staging).await;
    // Any failure, a cancel and any timeout leave the staging folder behind, and a
    // success leaves nothing in it once the version folder is renamed into place.
    // Either way it goes now; an in-use file is left for the sweep.
    let _ = std::fs::remove_dir_all(&staging);
    outcome
}

async fn stage(st: &AppState, id: &str, plan: &Plan, folder: &Path, staging: &Path) -> Result<(), Failure> {
    let (Some(url), Some(pin), Some(kind), Some(version)) = (plan.url.as_deref(), plan.pin.as_ref(), plan.kind, plan.version.as_deref()) else {
        return Err(Failure::Message("The publisher's answer could not be read."));
    };
    let download = staging.join("file.part");
    if let Some(value) = set_phase(st, id, "downloading", None) {
        announce(st, value).await;
    }
    download_to(st, id, url, pin, plan.bytes, &download).await?;
    if let Some(value) = set_phase(st, id, "checking", None) {
        announce(st, value).await;
    }
    if let Some(value) = set_phase(st, id, "installing", None) {
        announce(st, value).await;
    }
    let unpacked = staging.join("unpacked");
    std::fs::create_dir_all(&unpacked).map_err(|_| Failure::Message("The install folder could not be created."))?;
    let entry_name = extract(&download, kind, &unpacked, plan.entries, plan.file_name.as_deref()).map_err(Failure::Message)?;
    // The executable answers `--version` before it is published: a build that
    // cannot start is not installed.
    let relative = entry_name.clone();
    publish(st, id, plan, folder, staging, &unpacked, &relative, version, pin).await
}

/// Streams the download, hashing as it goes, and compares the digest before
/// anything is unpacked.
async fn download_to(st: &AppState, id: &str, url: &str, pin: &Pin, expected: Option<u64>, path: &Path) -> Result<(), Failure> {
    check_url(url).map_err(Failure::from_dispatch)?;
    let client = client().map_err(Failure::from_dispatch)?;
    let mut response = client.get(url).send().await.map_err(|_| Failure::Message("Could not reach the publisher. Nothing was installed."))?;
    if !response.status().is_success() {
        return Err(Failure::Message("The publisher answered with an error. Nothing was installed."));
    }
    let total = response.content_length().or(expected).unwrap_or(0);
    if total > MAX_DOWNLOAD_BYTES {
        return Err(Failure::Message("The publisher's download is larger than Retinue will fetch. Nothing was installed."));
    }
    let Some(mut hasher) = Hasher::new(pin.algorithm) else {
        return Err(Failure::Message("The publisher's checksum type is not one Retinue checks."));
    };
    let file = std::fs::File::create(path).map_err(|_| Failure::Message("The download could not be written."))?;
    let mut file = std::io::BufWriter::new(file);
    use std::io::Write as _;
    let mut done: u64 = 0;
    let mut last = Instant::now();
    loop {
        if cancel_requested(st, id) {
            return Err(Failure::Cancelled);
        }
        let chunk = match response.chunk().await {
            Ok(Some(chunk)) => chunk,
            Ok(None) => break,
            Err(_) => return Err(Failure::Message("The download stopped before it finished. Nothing was installed.")),
        };
        done += chunk.len() as u64;
        if done > MAX_DOWNLOAD_BYTES {
            return Err(Failure::Message("The publisher's download is larger than Retinue will fetch. Nothing was installed."));
        }
        hasher.update(&chunk);
        if file.write_all(&chunk).is_err() {
            return Err(Failure::Message("The download could not be written."));
        }
        if last.elapsed() >= Duration::from_millis(500) {
            last = Instant::now();
            progress(st, id, done, total).await;
        }
    }
    if let Err(error) = file.flush() {
        let _ = error;
        return Err(Failure::Message("The download could not be written."));
    }
    if let (Some(expected), Some(actual)) = (expected, Some(done)) {
        if expected != actual {
            return Err(Failure::Message("The download is a different size than the publisher listed. Nothing was installed."));
        }
    }
    progress(st, id, done, total.max(done)).await;
    if hasher.finish_hex() != pin.hex.to_ascii_lowercase() {
        return Err(Failure::Message("The download did not match the publisher's checksum. Nothing was installed."));
    }
    Ok(())
}

fn cancel_requested(st: &AppState, id: &str) -> bool {
    st.agent_install.cancel_signal(id).is_some_and(|signal| signal.load(Ordering::SeqCst))
}

/// The digest computed while the bytes stream.
enum Hasher {
    Sha256(Sha256),
    Sha512(Sha512),
}

impl Hasher {
    fn new(algorithm: &str) -> Option<Self> {
        match algorithm {
            "sha256" => Some(Hasher::Sha256(Sha256::new())),
            "sha512" => Some(Hasher::Sha512(Sha512::new())),
            _ => None,
        }
    }
    fn update(&mut self, bytes: &[u8]) {
        match self {
            Hasher::Sha256(hasher) => hasher.update(bytes),
            Hasher::Sha512(hasher) => hasher.update(bytes),
        }
    }
    fn finish_hex(self) -> String {
        match self {
            Hasher::Sha256(hasher) => hex_encode(&hasher.finalize()),
            Hasher::Sha512(hasher) => hex_encode(&hasher.finalize()),
        }
    }
}

/// Unpacks an archive into `into` and answers the extracted executable's relative
/// path. Every entry is joined, canonicalised and refused if it leaves `into`; a
/// symlink, a hard link and an absolute or `..` name are refused; a total size cap
/// applies. No script from an archive is ever run.
pub(crate) fn extract(archive: &Path, kind: Kind, into: &Path, entries: &[&str], file_name: Option<&str>) -> Result<String, &'static str> {
    match kind {
        Kind::File => {
            let name = file_name.ok_or("The publisher's answer could not be read.")?;
            let name = name.rsplit(['/', '\\']).next().unwrap_or(name);
            if !retinue_runtime::safe_relative_entry(name) {
                return Err("The publisher's answer could not be read.");
            }
            let target = into.join(name);
            std::fs::rename(archive, &target).or_else(|_| std::fs::copy(archive, &target).map(|_| ())).map_err(|_| "The install folder could not be written.")?;
            Ok(name.to_owned())
        }
        Kind::Zip => extract_zip(archive, into, entries),
        Kind::TarGz => extract_tar_gz(archive, into, entries),
    }
}

fn extract_zip(archive: &Path, into: &Path, entries: &[&str]) -> Result<String, &'static str> {
    let file = std::fs::File::open(archive).map_err(|_| "The download could not be read.")?;
    let mut zip = zip::ZipArchive::new(file).map_err(|_| "The download is not an archive Retinue can unpack.")?;
    let mut total: u64 = 0;
    let mut found: Option<String> = None;
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index).map_err(|_| "The download is not an archive Retinue can unpack.")?;
        let raw = entry.name().to_owned();
        let relative = safe_entry_name(&raw).ok_or("The archive holds a folder that leaves the install folder. Nothing was installed.")?;
        if entry.unix_mode().is_some_and(|mode| mode & 0o170000 == 0o120000) {
            return Err("The archive holds a link, which Retinue does not unpack. Nothing was installed.");
        }
        if relative.is_empty() || raw.ends_with('/') {
            std::fs::create_dir_all(into.join(&relative)).map_err(|_| "The install folder could not be written.")?;
            continue;
        }
        total = total.saturating_add(entry.size());
        if total > MAX_UNPACKED_BYTES {
            return Err("The archive is larger than Retinue unpacks. Nothing was installed.");
        }
        let target = into.join(&relative);
        let parent = target.parent().ok_or("The archive holds a folder that leaves the install folder. Nothing was installed.")?;
        std::fs::create_dir_all(parent).map_err(|_| "The install folder could not be written.")?;
        let canonical_parent = std::fs::canonicalize(parent).map_err(|_| "The install folder could not be written.")?;
        let canonical_into = std::fs::canonicalize(into).map_err(|_| "The install folder could not be written.")?;
        if !canonical_parent.starts_with(&canonical_into) {
            return Err("The archive holds a folder that leaves the install folder. Nothing was installed.");
        }
        let mut out = std::fs::File::create(&target).map_err(|_| "The install folder could not be written.")?;
        std::io::copy(&mut entry, &mut out).map_err(|_| "The archive could not be unpacked.")?;
        if found.is_none() && matches_entry(&target, entries) {
            found = Some(relative);
        }
    }
    found.ok_or("The download did not contain the coding agent. Nothing was installed.")
}

fn extract_tar_gz(archive: &Path, into: &Path, entries: &[&str]) -> Result<String, &'static str> {
    let file = std::fs::File::open(archive).map_err(|_| "The download could not be read.")?;
    let mut tar = flate2::read::GzDecoder::new(file);
    let mut total: u64 = 0;
    let mut found: Option<String> = None;
    let canonical_into = std::fs::canonicalize(into).map_err(|_| "The install folder could not be written.")?;
    loop {
        let mut header = [0u8; 512];
        match tar.read_exact(&mut header) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(_) => return Err("The archive could not be unpacked."),
        }
        if header.iter().all(|byte| *byte == 0) {
            break;
        }
        let name = tar_name(&header);
        let size = tar_size(&header).ok_or("The archive could not be unpacked.")?;
        let typeflag = header[156];
        let padded = size.div_ceil(512) * 512;
        match typeflag {
            b'0' | 0 | b'5' => {}
            // A directory is a file kind to us; everything else (a symbolic link,
            // a hard link, a device) is refused outright.
            b'1' | b'2' => return Err("The archive holds a link, which Retinue does not unpack. Nothing was installed."),
            _ => {
                skip_bytes(&mut tar, padded).map_err(|_| "The archive could not be unpacked.")?;
                continue;
            }
        }
        let relative = safe_entry_name(&name).ok_or("The archive holds a folder that leaves the install folder. Nothing was installed.")?;
        if relative.is_empty() || typeflag == b'5' {
            skip_bytes(&mut tar, padded).map_err(|_| "The archive could not be unpacked.")?;
            std::fs::create_dir_all(into.join(&relative)).map_err(|_| "The install folder could not be written.")?;
            continue;
        }
        total = total.saturating_add(size);
        if total > MAX_UNPACKED_BYTES {
            return Err("The archive is larger than Retinue unpacks. Nothing was installed.");
        }
        let target = into.join(&relative);
        let parent = target.parent().ok_or("The archive holds a folder that leaves the install folder. Nothing was installed.")?;
        std::fs::create_dir_all(parent).map_err(|_| "The install folder could not be written.")?;
        let canonical_parent = std::fs::canonicalize(parent).map_err(|_| "The install folder could not be written.")?;
        if !canonical_parent.starts_with(&canonical_into) {
            return Err("The archive holds a folder that leaves the install folder. Nothing was installed.");
        }
        let mut out = std::fs::File::create(&target).map_err(|_| "The install folder could not be written.")?;
        let mut remaining = size;
        let mut buffer = vec![0u8; 64 * 1024];
        while remaining > 0 {
            let want = remaining.min(buffer.len() as u64) as usize;
            tar.read_exact(&mut buffer[..want]).map_err(|_| "The archive could not be unpacked.")?;
            std::io::Write::write_all(&mut out, &buffer[..want]).map_err(|_| "The install folder could not be written.")?;
            remaining -= want as u64;
        }
        skip_bytes(&mut tar, padded - size).map_err(|_| "The archive could not be unpacked.")?;
        if found.is_none() && matches_entry(&target, entries) {
            found = Some(relative);
        }
    }
    found.ok_or("The download did not contain the coding agent. Nothing was installed.")
}

fn skip_bytes(reader: &mut impl Read, count: u64) -> std::io::Result<()> {
    let mut remaining = count;
    let mut buffer = [0u8; 1024];
    while remaining > 0 {
        let want = remaining.min(buffer.len() as u64) as usize;
        reader.read_exact(&mut buffer[..want])?;
        remaining -= want as u64;
    }
    Ok(())
}

/// A tar entry's name: the ustar `prefix` plus `name`, when it is a ustar header.
fn tar_name(header: &[u8; 512]) -> String {
    let field = |start: usize, end: usize| -> String {
        let bytes = &header[start..end];
        let end = bytes.iter().position(|byte| *byte == 0).unwrap_or(bytes.len());
        String::from_utf8_lossy(&bytes[..end]).into_owned()
    };
    let name = field(0, 100);
    if header[257..262] == *b"ustar" {
        let prefix = field(345, 500);
        if !prefix.is_empty() {
            return format!("{prefix}/{name}");
        }
    }
    name
}

fn tar_size(header: &[u8; 512]) -> Option<u64> {
    let bytes = &header[124..136];
    let end = bytes.iter().position(|byte| *byte == 0).unwrap_or(bytes.len());
    let text = std::str::from_utf8(&bytes[..end]).ok()?.trim();
    if text.is_empty() {
        return Some(0);
    }
    u64::from_str_radix(text, 8).ok()
}

/// An entry name as a relative path inside the staging folder. Empty for `.`, and
/// `None` for anything absolute, with a drive or with `..`.
pub(crate) fn safe_entry_name(raw: &str) -> Option<String> {
    let normalised = raw.replace('\\', "/");
    let trimmed = normalised.trim_start_matches("./");
    if trimmed.is_empty() || trimmed == "." {
        return Some(String::new());
    }
    if trimmed.len() > 400 || trimmed.starts_with('/') || trimmed.contains(':') {
        return None;
    }
    let mut parts = Vec::new();
    for part in trimmed.split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." || part.contains('\0') {
            return None;
        }
        parts.push(part);
    }
    let joined = parts.join("/");
    if joined.is_empty() || Path::new(&joined).is_absolute() || Path::new(&joined).components().any(|component| !matches!(component, Component::Normal(_))) {
        return None;
    }
    Some(joined)
}

fn matches_entry(target: &Path, entries: &[&str]) -> bool {
    let Some(name) = target.file_name().map(|name| name.to_string_lossy().to_ascii_lowercase()) else { return false };
    entries.iter().any(|candidate| candidate.eq_ignore_ascii_case(&name))
}

// ---------------------------------------------------------------------------
// Publishing a version folder, and the receipt

#[allow(clippy::too_many_arguments)]
async fn publish(st: &AppState, id: &str, plan: &Plan, folder: &Path, staging: &Path, unpacked: &Path, relative: &str, version: &str, pin: &Pin) -> Result<(), Failure> {
    let target = folder.join(version);
    let provider_root = folder.parent().unwrap_or(folder);
    if target.exists() {
        // Repair only: a folder the receipt does not name is an interrupted
        // install, and is replaced. One the receipt names is already installed.
        if retinue_runtime::managed_entry(provider_root, plan.provider).is_some() {
            return Err(Failure::Message("That version is already installed."));
        }
        std::fs::remove_dir_all(&target).map_err(|_| Failure::Message("The previous folder could not be replaced. Nothing was installed."))?;
    }
    let executable = unpacked.join(relative);
    if !executable.is_file() {
        return Err(Failure::Message("The download did not contain the coding agent. Nothing was installed."));
    }
    let previous = read_previous_version(folder);
    let bytes = std::fs::metadata(&executable).map(|metadata| metadata.len()).unwrap_or(0);
    // A version folder is never launched until the receipt names it, and the
    // receipt is written after the rename, through a temp file.
    std::fs::rename(unpacked, &target).map_err(|_| Failure::Message("The install folder could not be written."))?;
    make_executable(&target.join(relative));
    let _ = std::fs::remove_dir_all(staging.join("file.part"));
    let receipt = json!({
        "version": version,
        "entry": relative,
        "digest": pin.hex,
        "bytes": bytes,
        "installedAt": chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        "sourceHost": plan.source_host,
        "previous": previous,
    });
    write_receipt(folder, &receipt)?;
    if let Some(value) = st.agent_install.update(id, |job| { job.bytes_done = job.bytes_total; }) {
        announce(st, value).await;
    }
    // The new copy is current: rescan, then let any idle session of this
    // provider go so its next process is the new version. A running turn is left
    // alone and keeps the old one until it ends (design 8.6).
    rescan(st).await;
    close_idle_sessions(st, plan.provider).await;
    Ok(())
}

/// The publisher's binary is not run until it is executable, and an archive
/// does not carry the owner's execute bit through this crate's own unpacker. On
/// Unix the installed file is made `0755`; Windows has no such bit, and a test
/// proves the decision rather than the mode.
pub(crate) fn make_executable(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(metadata) = std::fs::metadata(path) {
            let mut permissions = metadata.permissions();
            permissions.set_mode(metadata.permissions().mode() | 0o755);
            let _ = std::fs::set_permissions(path, permissions);
        }
    }
    #[cfg(not(unix))]
    let _ = path;
}

/// `current.json`, temp-and-rename. The receipt is the only thing discovery reads,
/// so a half-written one is never visible.
fn write_receipt(folder: &Path, receipt: &Value) -> Result<(), Failure> {
    std::fs::create_dir_all(folder).map_err(|_| Failure::Message("The install folder could not be written."))?;
    let temp = folder.join(format!("{}.tmp", retinue_runtime::MANAGED_RECEIPT));
    let path = folder.join(retinue_runtime::MANAGED_RECEIPT);
    std::fs::write(&temp, receipt.to_string()).map_err(|_| Failure::Message("The install could not be recorded."))?;
    let _ = sandbox::restrict(&temp, false);
    std::fs::rename(&temp, &path).map_err(|_| Failure::Message("The install could not be recorded."))?;
    Ok(())
}

/// The version the receipt names as the one to go back to.
fn read_previous_version(folder: &Path) -> Option<String> {
    let text = std::fs::read_to_string(folder.join(retinue_runtime::MANAGED_RECEIPT)).ok()?;
    let receipt: Value = serde_json::from_str(&text).ok()?;
    receipt["previous"].as_str().filter(|version| retinue_runtime::safe_version_folder(version)).map(str::to_owned)
}

/// The rescan after a copy is published or removed: discovery, the catalog and one
/// `runtime.changed`.
///
/// A test's stub origin means "no real network and no real CLIs", so the rescan is
/// skipped there: it would probe this machine's own installs, and no test may.
async fn rescan(st: &AppState) {
    if test_origin().is_none() {
        crate::refresh_runtime_discovery_now(st).await;
    }
}

/// Closes this provider's idle sessions, so their shared adapter process exits and
/// the next one starts the new version. A session with a running turn is left.
pub(crate) async fn close_idle_sessions(st: &AppState, provider: &str) {
    let idle: Vec<String> = {
        let active = st.active_turns.lock().await;
        let sessions = st.sessions.lock().await;
        sessions.iter().filter(|(id, session)| session.runtime.provider == provider && !active.contains_key(*id)).map(|(id, _)| id.clone()).collect()
    };
    for id in idle {
        let _ = crate::close_session(st, &id).await;
    }
}

/// At daemon start and after each install: remove the version folders beyond the
/// current one and the previous one, and any staging folder left by a crash. An
/// in-use file is left alone and reported, never forced.
pub(crate) fn sweep_at_startup(st: &AppState) {
    let Some(root) = managed_root(st) else { return };
    let Ok(providers) = std::fs::read_dir(&root) else { return };
    for entry in providers.flatten() {
        if entry.path().is_dir() {
            sweep_provider(&root, &entry.file_name().to_string_lossy());
        }
    }
}

/// The same sweep for one provider, which is what a finished install does: another
/// provider's install may be in flight beside it.
pub(crate) fn sweep_provider(root: &Path, provider: &str) {
    let folder = retinue_runtime::managed_folder(root, provider);
    let previous = read_previous_version(&folder);
    let current = retinue_runtime::managed_entry(root, provider).map(|entry| entry.version);
    let keep: Vec<String> = current.into_iter().chain(previous).collect();
    let _ = sweep_versions(&folder, &keep);
    let _ = std::fs::remove_dir_all(folder.join("staging"));
}

/// Removes every version folder whose name is not kept, and any stray file. The
/// names that could not be removed come back, so a caller can say so.
pub(crate) fn sweep_versions(folder: &Path, keep: &[String]) -> Vec<String> {
    let mut kept = Vec::new();
    let Ok(entries) = std::fs::read_dir(folder) else { return kept };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !entry.path().is_dir() || name == "staging" || keep.iter().any(|kept| kept == &name) {
            continue;
        }
        // A folder whose files are in use cannot go yet; the next start retries.
        if std::fs::remove_dir_all(entry.path()).is_err() {
            kept.push(name);
        }
    }
    kept
}

// ---------------------------------------------------------------------------
// Adopt: run the installer that owns the owner's own copy

async fn adopt(st: &AppState, id: &str, plan: &Plan) -> Result<(), Failure> {
    let Some(command) = plan.adopt.as_ref() else {
        return Err(Failure::Message("Retinue cannot tell what installed this coding agent. Choose Manage instead."));
    };
    if let Some(refusal) = command.refusal {
        return Err(Failure::Message(refusal));
    }
    if let Some(value) = set_phase(st, id, "installing", None) {
        announce(st, value).await;
    }
    let _ = run_update(st, id, command).await?;
    // The CLI's own word, which is the only honest answer: a package manager
    // behind the release exits cleanly on the old version.
    let target = plan.version.clone().unwrap_or_default();
    match installed_version_of(plan.provider).await {
        Some(version) if version_matches(&version, &target) => Ok(()),
        _ => Err(Failure::Message("The CLI still reports the old version. Nothing was changed.")),
    }
}

fn version_matches(installed: &str, target: &str) -> bool {
    installed == target || installed.starts_with(&format!("{target}.")) || target.starts_with(&format!("{installed}."))
}

/// Runs the owner's updater windowless, with piped stdio (nothing answers it, so a
/// prompt sees EOF rather than waiting), and keeps only its last line.
async fn run_update(st: &AppState, id: &str, command: &Adopt) -> Result<String, Failure> {
    let mut cmd = tokio::process::Command::new(&command.program);
    retinue_process::prepare_command(&mut cmd);
    cmd.args(&command.args).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).kill_on_drop(true);
    let mut child = cmd.spawn().map_err(|_| Failure::Message("The update could not be started."))?;
    let _tree = retinue_process::ProcessTree::attach(&mut child).map_err(|_| Failure::Message("The update could not be started."))?;
    // Both pipes are drained at once: an installer that writes more than a pipe
    // buffer to stdout while its stderr is read first would block for ever.
    let (out, errors) = tokio::join!(drain_tail(child.stdout.take()), drain_tail(child.stderr.take()));
    let status = child.wait().await.map_err(|_| Failure::Message("The update could not be completed."))?;
    let line = last_line(if errors.trim().is_empty() { &out } else { &errors });
    if let Some(value) = set_phase(st, id, "installing", Some(line.clone())) {
        announce(st, value).await;
    }
    if !status.success() {
        return Err(Failure::Message("The update program reported a failure. Nothing was changed."));
    }
    Ok(line)
}

/// Reads one child stream to the end and keeps its last 8 KiB. Bytes are kept as
/// bytes: a chunk may split a character, and the conversion happens once at the end.
async fn drain_tail<R: tokio::io::AsyncRead + Unpin>(stream: Option<R>) -> String {
    use tokio::io::AsyncReadExt as _;
    let Some(mut stream) = stream else { return String::new() };
    let mut tail: Vec<u8> = Vec::new();
    let mut buffer = [0u8; 4096];
    loop {
        match stream.read(&mut buffer).await {
            Ok(0) | Err(_) => break,
            Ok(read) => {
                tail.extend_from_slice(&buffer[..read]);
                if tail.len() > 16 * 1024 {
                    tail.drain(..tail.len() - 8 * 1024);
                }
            }
        }
    }
    String::from_utf8_lossy(&tail).into_owned()
}

/// The last non-empty line of an installer's output, redacted and bounded. Never a
/// path, never a secret.
pub(crate) fn last_line(text: &str) -> String {
    let line = text.lines().map(str::trim).rfind(|line| !line.is_empty()).unwrap_or("");
    crate::diagnostics::redact(line).chars().take(MAX_LINE_CHARS).collect()
}

async fn installed_version_of(provider: &str) -> Option<String> {
    let row = row_for(provider)?;
    let found = retinue_runtime::resolve_command(row.command)?;
    retinue_runtime::version_of(&found.executable, &found.args).await
}

// ---------------------------------------------------------------------------
// The startup sweep, once

pub(crate) fn startup(st: &AppState) {
    sweep_at_startup(st);
    // Keep-up-to-date (22 November 2026): a provider whose `autoUpdate` switch is
    // on is checked a few minutes after start and then every six hours. Only a
    // runtime is needed; a unit test's bare state never calls `startup`.
    if tokio::runtime::Handle::try_current().is_ok() {
        let state = st.clone();
        tokio::spawn(async move {
            tokio::time::sleep(AUTO_FIRST_CHECK).await;
            loop {
                let _ = auto_update_once(&state).await;
                tokio::time::sleep(AUTO_CHECK_EVERY).await;
            }
        });
    }
}

/// How long after start the first keep-up-to-date check waits, so it never
/// competes with the app's own start.
const AUTO_FIRST_CHECK: Duration = Duration::from_secs(5 * 60);
/// How often a keep-up-to-date provider is checked afterwards.
const AUTO_CHECK_EVERY: Duration = Duration::from_secs(6 * 60 * 60);

/// One keep-up-to-date pass. A provider is updated only when its switch is on,
/// the owner already consented to a download from the plan's source host (so a
/// new host still needs its card), something is installed to update, the
/// publisher's version is newer, and (for the owner's own copy) no bot is using
/// it. Answers the providers whose install it started.
pub(crate) async fn auto_update_once(st: &AppState) -> Vec<String> {
    let mut started = Vec::new();
    for row in ROWS.iter() {
        if row.source.is_none() || !auto_update_on(st, row.provider) {
            continue;
        }
        let Ok(plan) = build_plan(st, row.provider).await else { continue };
        if !plan.supported || (plan.mode == MODE_ADOPT && plan.runs_turn) || !auto_update_allowed(st, &plan) {
            continue;
        }
        let Some(latest) = plan.version.clone() else { continue };
        let current = if plan.mode == MODE_MANAGE { plan.installed_version.clone() } else { installed_version_of(row.provider).await };
        if !is_newer(&latest, current.as_deref()) {
            continue;
        }
        if let Ok(reply) = install(st, &json!({"provider": row.provider, "confirm": true})).await {
            if reply["joined"] != true {
                started.push(row.provider.to_owned());
            }
        }
    }
    started
}

/// Whether the publisher's version is newer than what is installed, comparing the
/// first dotted number in each (`2.1.322 (Claude Code)` reads as 2.1.322). Nothing
/// installed, or a version that cannot be read, is never "newer": keep-up-to-date
/// updates a copy, it never installs one.
pub(crate) fn is_newer(latest: &str, installed: Option<&str>) -> bool {
    fn parts(text: &str) -> Option<Vec<u64>> {
        let start = text.find(|c: char| c.is_ascii_digit())?;
        let run: String = text[start..].chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
        let numbers = run.split('.').filter(|part| !part.is_empty()).map(|part| part.parse::<u64>().ok()).collect::<Option<Vec<_>>>()?;
        (!numbers.is_empty()).then_some(numbers)
    }
    let (Some(next), Some(current)) = (parts(latest), installed.and_then(parts)) else { return false };
    let width = next.len().max(current.len());
    let padded = |numbers: &[u64]| numbers.iter().copied().chain(std::iter::repeat(0)).take(width).collect::<Vec<_>>();
    padded(&next) > padded(&current)
}
