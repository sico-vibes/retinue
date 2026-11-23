//! Model catalog cache.
//!
//! A catalog probe starts a provider CLI (Claude 2–8 s, Codex app-server 5 s,
//! OpenCode CLI and ACP model discovery), so the daemon caches the result and
//! never makes an execution path wait on one. This module owns the cache that
//! replaces the old 60-second `MODEL_CATALOG_CACHE` static:
//!
//! - entries are keyed by runtime plus the only two inputs that change a probe
//!   (the Claude workflows setting, and the model hint for Claude alone);
//! - concurrent misses on one key join a single probe;
//! - reads are fresh for 15 minutes, stale-but-served for up to 7 days, and a
//!   database hit counts as stale so a restart begins warm;
//! - a failed probe is remembered for 30 seconds so a broken CLI is not
//!   restarted on every call;
//! - window reads serve stale data immediately and only wait on a cache miss;
//! - `CachedOnly` reads never wait: they return what is cached (or nothing) and
//!   start one background refresh. OpenCode execution reads skip that refresh.

use crate::{
    adapter_for, claude_workflows_enabled, derr, model_presentation, provider_display_name,
    runtime_from, AppState, DispatchError,
};
use axum::http::StatusCode;
use retinue_agent_core::AdapterError;
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex as StdMutex},
    time::{Duration, Instant},
};
use tokio::sync::OnceCell;

/// A cached catalog is fresh for this long before a probe is preferred.
pub const FRESH_TTL: Duration = Duration::from_secs(15 * 60);
/// A persisted catalog may still be served for this long while it refreshes.
pub const STALE_TTL: Duration = Duration::from_secs(7 * 24 * 60 * 60);
/// A fallback or unvalidated catalog is memory-only for this long, never persisted.
pub const FALLBACK_TTL: Duration = Duration::from_secs(60);
/// A failed probe is remembered for this long so a broken CLI is not restarted.
pub const FAILURE_TTL: Duration = Duration::from_secs(30);

/// How a caller wants the cache treated.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    /// Serve a fresh value, otherwise probe (joining one already in flight).
    Fresh,
    /// Serve any usable value and refresh stale data in the background. Wait on a miss.
    Window,
    /// Always probe (joining one already in flight).
    Refresh,
    /// Never wait: return fresh or stale or `None`, and refresh in the background.
    CachedOnly,
    /// Execution may read the OpenCode cache without starting a competing CLI.
    Execution,
}

/// A catalog served from the cache. `stale` marks a value past its fresh window
/// (served while a background refresh runs); `cached_at` is when the daemon
/// stored it.
#[derive(Clone)]
pub struct Served {
    pub value: Value,
    pub stale: bool,
    pub cached_at: DateTime<Utc>,
}

#[derive(Clone)]
struct Entry {
    fetched: Instant,
    cached_at: DateTime<Utc>,
    value: Value,
    persisted: bool,
    /// Explicitly invalidated (or loaded from disk): serve as stale, never fresh.
    stale: bool,
}

#[derive(Clone)]
struct Failure {
    at: Instant,
    error: DispatchError,
}

/// The inputs that decide one probe and its cache key.
pub(crate) struct Probe {
    pub(crate) provider: String,
    pub(crate) workflows: bool,
    pub(crate) hint: Option<String>,
    pub(crate) key: String,
}

type ProbeFlight = Arc<OnceCell<Result<Served, DispatchError>>>;

/// The daemon-wide catalog cache, held as `AppState.catalog`.
pub struct CatalogCache {
    fresh: Duration,
    stale: Duration,
    fallback: Duration,
    failure: Duration,
    entries: StdMutex<HashMap<String, Entry>>,
    failures: StdMutex<HashMap<String, Failure>>,
    inflight: StdMutex<HashMap<String, ProbeFlight>>,
}

impl Default for CatalogCache {
    fn default() -> Self {
        Self::new(FRESH_TTL, STALE_TTL, FALLBACK_TTL, FAILURE_TTL)
    }
}

impl CatalogCache {
    /// Lifetimes are injectable so tests can exercise staleness without waiting.
    pub fn new(fresh: Duration, stale: Duration, fallback: Duration, failure: Duration) -> Self {
        Self {
            fresh,
            stale,
            fallback,
            failure,
            entries: StdMutex::new(HashMap::new()),
            failures: StdMutex::new(HashMap::new()),
            inflight: StdMutex::new(HashMap::new()),
        }
    }

    fn read_entry(&self, key: &str) -> Option<Entry> {
        self.entries.lock().ok()?.get(key).cloned()
    }

    fn fresh_ttl(&self, persisted: bool) -> Duration {
        if persisted { self.fresh } else { self.fallback }
    }

    fn stale_ttl(&self, persisted: bool) -> Duration {
        if persisted { self.stale } else { self.fallback }
    }

    fn served(entry: &Entry, stale: bool) -> Served {
        Served { value: entry.value.clone(), stale, cached_at: entry.cached_at }
    }

    /// A value still inside its fresh window.
    fn fresh_value(&self, key: &str) -> Option<Served> {
        let entry = self.read_entry(key)?;
        if entry.stale { return None; }
        (entry.fetched.elapsed() < self.fresh_ttl(entry.persisted)).then(|| Self::served(&entry, false))
    }

    /// A value still usable, fresh or stale. Stale entries are bounded by the
    /// wall-clock cache time because they may have been loaded from disk.
    fn usable_value(&self, key: &str) -> Option<Served> {
        let entry = self.read_entry(key)?;
        self.usable_value_of(&entry)
    }

    fn usable_value_of(&self, entry: &Entry) -> Option<Served> {
        if !entry.stale {
            let age = entry.fetched.elapsed();
            if age < self.fresh_ttl(entry.persisted) { return Some(Self::served(entry, false)); }
            if age < self.stale_ttl(entry.persisted) { return Some(Self::served(entry, true)); }
            return None;
        }
        let age = Utc::now().signed_duration_since(entry.cached_at).to_std().unwrap_or_default();
        (age < self.stale_ttl(entry.persisted)).then(|| Self::served(entry, true))
    }

    fn failure(&self, key: &str) -> Option<DispatchError> {
        let failure = self.failures.lock().ok()?.get(key).cloned()?;
        (failure.at.elapsed() < self.failure).then_some(failure.error)
    }

    fn remember_failure(&self, key: &str, error: DispatchError) {
        if let Ok(mut failures) = self.failures.lock() {
            failures.insert(key.to_owned(), Failure { at: Instant::now(), error });
        }
    }

    fn clear_failure(&self, key: &str) {
        if let Ok(mut failures) = self.failures.lock() { failures.remove(key); }
    }

    fn insert(&self, key: &str, value: Value, stale: bool) -> DateTime<Utc> {
        let cached_at = Utc::now();
        let persisted = persistable_catalog(&value);
        if let Ok(mut entries) = self.entries.lock() {
            entries.insert(key.to_owned(), Entry { fetched: Instant::now(), cached_at, value, persisted, stale });
        }
        cached_at
    }

    /// Look in the SQLite mirror. A database hit counts as stale.
    fn load_persisted(&self, st: &AppState, key: &str) -> Option<Served> {
        let (fetched_at, value) = st.db.catalog_cache_get(key).ok().flatten()?;
        let cached_at = DateTime::parse_from_rfc3339(&fetched_at)
            .map(|parsed| parsed.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now());
        let entry = Entry { fetched: Instant::now(), cached_at, value, persisted: true, stale: true };
        let served = self.usable_value_of(&entry)?;
        if let Ok(mut entries) = self.entries.lock() {
            // A concurrent probe may have finished while SQLite was being read.
            if let Some(current) = entries.get(key).and_then(|current| self.usable_value_of(current)) {
                return Some(current);
            }
            entries.insert(key.to_owned(), entry);
        }
        Some(served)
    }

    /// Mark every entry stale (keeping its value) and drop the negative cache.
    pub fn invalidate_all(&self) {
        if let Ok(mut entries) = self.entries.lock() {
            for entry in entries.values_mut() { entry.stale = true; }
        }
        if let Ok(mut failures) = self.failures.lock() { failures.clear(); }
    }
}

/// The cache key: runtime identity plus the Claude workflows setting, plus the
/// model hint for Claude alone (its `get_settings` output depends on the model).
fn cache_key(runtime: &Value, workflows: bool, hint: Option<&str>) -> String {
    let mut key = format!("{}|claude-workflows={workflows}", runtime_key(runtime));
    if runtime["provider"].as_str() == Some("claude") {
        key.push_str(&format!("|model={}", hint.unwrap_or("")));
    }
    key
}

fn runtime_key(runtime: &Value) -> String {
    format!(
        "{}|{}|{}|{}|{}",
        runtime["id"].as_str().unwrap_or(""),
        runtime["executablePath"].as_str().unwrap_or(""),
        runtime["version"].as_str().unwrap_or(""),
        serde_json::to_string(&runtime["launchArgs"]).unwrap_or_default(),
        runtime["profileId"].as_str().unwrap_or("")
    )
}

/// Claude's catalog probe carries the model and the workflows opt-in; Codex and
/// OpenCode ignore both.
pub(crate) fn catalog_probe_args(provider: &str, workflows: bool, model_hint: Option<&str>) -> Vec<String> {
    let mut args = Vec::new();
    if provider == "claude" {
        let model = model_hint.or_else(|| workflows.then_some("sonnet"));
        if let Some(model) = model { args.extend(["--model".into(), model.into()]); }
        if workflows { args.extend(["--settings".into(), r#"{"enableWorkflows":true}"#.into()]); }
    }
    args
}

/// Resolve the workflows flag, the effective model hint and the cache key a
/// call to `get` will use. Shared with tests so they can address the same key.
pub(crate) fn resolve_probe(st: &AppState, runtime: &Value, workflow_override: Option<bool>, model_hint: Option<&str>) -> Probe {
    let provider = runtime["provider"].as_str().unwrap_or("").to_owned();
    let workflows = provider == "claude"
        && workflow_override.unwrap_or_else(|| st.db.settings().map(|settings| claude_workflows_enabled(&settings)).unwrap_or(true));
    let hint = model_hint.map(str::to_owned)
        .or_else(|| (provider == "claude" && workflows).then_some("sonnet".to_owned()));
    let key = cache_key(runtime, workflows, hint.as_deref());
    Probe { provider, workflows, hint, key }
}

/// What is already cached for a runtime, fresh or stale. It never probes and never
/// starts a refresh, so `list_models` can be asked as often as a bot likes.
pub(crate) fn cached(st: &AppState, runtime: &Value) -> Option<Served> {
    let probe = resolve_probe(st, runtime, None, None);
    let served = st.catalog.usable_value(&probe.key).or_else(|| st.catalog.load_persisted(st, &probe.key));
    served.map(|mut served| {
        if probe.provider == "opencode" { keep_opencode_included_models(runtime, &mut served.value); }
        served
    })
}

/// Puts a catalog in the cache under the key `cached` reads, for tests.
#[cfg(test)]
pub(crate) fn seed(st: &AppState, runtime: &Value, value: Value) {
    let probe = resolve_probe(st, runtime, None, None);
    st.catalog.insert(&probe.key, value, false);
}

/// Fetch a catalog. `Fresh`, `Window` and `Refresh` yield a value or an error;
/// Cache-only modes yield `None` on a miss without waiting.
pub async fn get(
    st: &AppState,
    runtime: &Value,
    workflow_override: Option<bool>,
    model_hint: Option<&str>,
    mode: Mode,
) -> Result<Option<Served>, DispatchError> {
    let probe = resolve_probe(st, runtime, workflow_override, model_hint);
    let opencode = probe.provider == "opencode";
    let served = match mode {
        Mode::Fresh => match st.catalog.fresh_value(&probe.key) {
            Some(served) => Some(served),
            None => Some(fetch_or_stale(st, runtime, &probe, false).await?),
        },
        Mode::Refresh => Some(fetch_or_stale(st, runtime, &probe, true).await?),
        Mode::Window => {
            match st.catalog.usable_value(&probe.key).or_else(|| st.catalog.load_persisted(st, &probe.key)) {
                Some(served) => {
                    if served.stale && st.catalog.failure(&probe.key).is_none() {
                        spawn_refresh(st, runtime.clone(), probe);
                    }
                    Some(served)
                }
                None => Some(fetch_or_stale(st, runtime, &probe, false).await?),
            }
        }
        Mode::CachedOnly | Mode::Execution => {
            let served = st.catalog.usable_value(&probe.key).or_else(|| st.catalog.load_persisted(st, &probe.key));
            // A stale or missing read starts one background refresh, unless a
            // recent probe already failed and is negative-cached.
            if !(opencode && mode == Mode::Execution) && served.as_ref().is_none_or(|value| value.stale) && st.catalog.failure(&probe.key).is_none() {
                spawn_refresh(st, runtime.clone(), probe);
            }
            served
        }
    };
    // Filtered when served, not when stored, so a catalog cached before this
    // rule (or before OpenCode Go was linked or unlinked) follows it too.
    Ok(served.map(|mut served| {
        if opencode { keep_opencode_included_models(runtime, &mut served.value); }
        served
    }))
}

/// OpenCode Zen's paid models bill pay-as-you-go credits, which Retinue never
/// uses (owner, 16 November 2026): only Zen's free models (`-free`, and Big
/// Pickle) and, when it is linked, OpenCode Go. Other providers the owner set
/// up in OpenCode with their own keys are untouched.
pub(crate) fn opencode_paid_zen_model(id: &str) -> bool {
    match id.strip_prefix("opencode/") {
        Some(name) => !(name.ends_with("-free") || name == "big-pickle"),
        None => false,
    }
}

/// Whether OpenCode Go may be offered. A runtime whose sign-ins could not be
/// read leaves the choice to OpenCode, which lists Go models only when linked.
fn opencode_go_linked(runtime: &Value) -> bool {
    match runtime["gatewayAuthStates"].as_object() {
        Some(states) if !states.is_empty() => states.get("opencode-go").and_then(Value::as_str) == Some("authenticated"),
        _ => true,
    }
}

fn keep_opencode_included_models(runtime: &Value, catalog: &mut Value) {
    let go = opencode_go_linked(runtime);
    if let Some(models) = catalog["models"].as_array_mut() {
        models.retain(|model| {
            let id = model["id"].as_str().unwrap_or("");
            !opencode_paid_zen_model(id) && (go || !id.starts_with("opencode-go/"))
        });
    }
}

async fn fetch_or_stale(st: &AppState, runtime: &Value, probe: &Probe, force: bool) -> Result<Served, DispatchError> {
    match fetch(st, runtime, probe, force).await {
        Err(error) if probe.provider == "opencode" => st.catalog.usable_value(&probe.key)
            .or_else(|| st.catalog.load_persisted(st, &probe.key)).ok_or(error),
        result => result,
    }
}

/// Run (or join) one probe for `probe.key`, then cache its outcome.
async fn fetch(st: &AppState, runtime: &Value, probe: &Probe, force: bool) -> Result<Served, DispatchError> {
    let flight = {
        let mut inflight = st.catalog.inflight.lock().unwrap();
        // A cancelled caller may leave an uninitialised cell held by the map alone.
        if let Some(flight) = inflight.get(&probe.key).filter(|flight| Arc::strong_count(flight) > 1) {
            flight.clone()
        } else {
            if !force {
                if let Some(served) = st.catalog.fresh_value(&probe.key) { return Ok(served); }
                if let Some(error) = st.catalog.failure(&probe.key) { return Err(error); }
            }
            let flight = Arc::new(OnceCell::new());
            inflight.insert(probe.key.clone(), flight.clone());
            flight
        }
    };
    let result = flight.get_or_init(|| async {
        let result = run_probe(st, runtime, probe).await;
        st.catalog.inflight.lock().unwrap().remove(&probe.key);
        result
    }).await;
    result.clone()
}

async fn run_probe(st: &AppState, runtime: &Value, probe: &Probe) -> Result<Served, DispatchError> {
    match probe_runtime(st, runtime, probe).await {
        Ok(value) => {
            let previous = st.catalog.usable_value(&probe.key).or_else(|| st.catalog.load_persisted(st, &probe.key));
            let cached_at = st.catalog.insert(&probe.key, value.clone(), false);
            st.catalog.clear_failure(&probe.key);
            if persistable_catalog(&value) {
                let fetched_at = cached_at.to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
                let _ = st.db.catalog_cache_put(&probe.key, runtime["id"].as_str().unwrap_or(""), &probe.provider, &fetched_at, &value);
            }
            if previous.as_ref().is_none_or(|previous| previous.stale || model_signature(&previous.value) != model_signature(&value)) {
                st.emit("runtime.models.changed", json!({"runtimeId": runtime["id"]})).await;
            }
            Ok(Served { value, stale: false, cached_at })
        }
        Err(error) => {
            if probe.provider == "opencode" {
                if let Ok(mut entries) = st.catalog.entries.lock() {
                    if let Some(entry) = entries.get_mut(&probe.key) { entry.stale = true; }
                }
            }
            st.catalog.remember_failure(&probe.key, error.clone());
            Err(error)
        }
    }
}

async fn probe_runtime(st: &AppState, runtime: &Value, probe: &Probe) -> Result<Value, DispatchError> {
    let mut spec = runtime_from(runtime);
    spec.args.extend(catalog_probe_args(&probe.provider, probe.workflows, probe.hint.as_deref()));
    let adapter = adapter_for(&st.adapters, &probe.provider).ok_or_else(|| {
        derr("unsupported", "model catalogs are unsupported for this runtime", StatusCode::BAD_REQUEST)
    })?;
    let mut catalog = adapter.model_catalog_for_version(&spec, runtime["version"].as_str()).await.map_err(|error| match error {
        AdapterError::Unsupported(_) => derr("unsupported", "model catalogs are unsupported for this runtime", StatusCode::BAD_REQUEST),
        AdapterError::Process(_) => derr("provider_unavailable", "runtime model catalog is unavailable", StatusCode::BAD_GATEWAY),
        _ => derr("provider_error", "runtime returned an invalid model catalog", StatusCode::BAD_GATEWAY),
    })?;
    if probe.provider == "opencode" && catalog.models.is_empty() {
        return Err(derr("provider_unavailable", "OpenCode returned no models", StatusCode::BAD_GATEWAY));
    }
    model_presentation::apply(&mut catalog.models, catalog.validated, catalog.fallback);
    Ok(json!({
        "runtimeId": spec.runtime_id,
        "provider": probe.provider,
        "displayName": provider_display_name(&probe.provider),
        "models": catalog.models,
        "fetchedAt": catalog.fetched_at,
        "expiresAt": catalog.expires_at,
        "fallback": catalog.fallback,
        "source": catalog.source,
        "validated": catalog.validated,
    }))
}

fn persistable_catalog(value: &Value) -> bool {
    value["fallback"] != true && (value["validated"] == true || value["provider"] == "opencode")
}

fn spawn_refresh(st: &AppState, runtime: Value, probe: Probe) {
    let st = st.clone();
    tokio::spawn(async move {
        let _ = fetch(&st, &runtime, &probe, false).await;
    });
}

/// A status the UI would consider usable, matching `runtimeUsable`, and a
/// provider that is not signed out.
pub(crate) fn runtime_usable(runtime: &Value) -> bool {
    let status = runtime["status"].as_str().unwrap_or("").to_ascii_lowercase();
    let usable = ["online", "ready", "available", "connected"].contains(&status.as_str());
    let signed_out = runtime["authState"].as_str().unwrap_or("").eq_ignore_ascii_case("unauthenticated");
    usable && !signed_out
}

/// After discovery, warm every usable runtime two at a time. Each provider's
/// base key is warmed (Claude defaulting to `sonnet`); a warmed list that
/// differs from the previous one emits `runtime.models.changed`.
pub async fn prewarm_all(st: &AppState) {
    let usable = st.runtimes.read().await.iter().filter(|runtime| runtime_usable(runtime)).cloned().collect::<Vec<_>>();
    for pair in usable.chunks(2) {
        let mut tasks = Vec::new();
        for runtime in pair {
            let st = st.clone();
            let runtime = runtime.clone();
            tasks.push(tokio::spawn(async move { prewarm_one(&st, &runtime).await; }));
        }
        for task in tasks { let _ = task.await; }
    }
}

/// How many of the most recently used conversations are warmed at launch.
const PREWARM_RECENT: usize = 5;

/// Warm only what a launch needs now (21 November 2026, performance item 7):
/// every conversation with a turn live or waiting, and the few most recently
/// used ones, which are what the window opens first. Every other runtime is
/// warmed when the owner opens one of its conversations
/// ([`warm_when_opened`]) or by [`prewarm_all`] once the window's first requests
/// are done, so a launch does not start a CLI for a bot nobody is looking at.
/// Returns how many runtimes it warmed.
pub async fn prewarm_wanted(st: &AppState) -> usize {
    let sessions = st.db.session_list(false).unwrap_or_default();
    warm_runtimes(st, &wanted_runtime_ids(&sessions)).await
}

/// The runtime ids a launch warms now, from the conversation list (newest
/// first): every conversation with a turn live or waiting, and the runtimes of
/// the first [`PREWARM_RECENT`] rows otherwise. Pure, so the choice can be
/// tested without starting a CLI.
fn wanted_runtime_ids(sessions: &[Value]) -> Vec<String> {
    let mut wanted: Vec<String> = Vec::new();
    for (index, session) in sessions.iter().enumerate() {
        let live = matches!(session["state"].as_str(), Some("working" | "starting" | "waiting_permission"));
        if !live && index >= PREWARM_RECENT { continue; }
        if let Some(id) = session["runtimeId"].as_str().filter(|id| !id.is_empty()) {
            if !wanted.iter().any(|seen| seen == id) { wanted.push(id.to_owned()); }
        }
    }
    wanted
}

async fn warm_runtimes(st: &AppState, ids: &[String]) -> usize {
    if ids.is_empty() { return 0; }
    let runtimes = st.runtimes.read().await.iter()
        .filter(|runtime| runtime_usable(runtime) && runtime["id"].as_str().is_some_and(|id| ids.iter().any(|wanted| wanted == id)))
        .cloned().collect::<Vec<_>>();
    for runtime in &runtimes {
        prewarm_one(st, runtime).await;
    }
    runtimes.len()
}

/// Warm one conversation's runtime when the window shows it, if that runtime
/// has no catalog yet. It is deliberately not a refresh: every probe starts a
/// CLI, so a cached catalog is left alone and `prewarm_all` owns freshness.
pub fn warm_when_opened(st: &AppState, session_id: &str) {
    let st = st.clone();
    let session_id = session_id.to_owned();
    tokio::spawn(async move {
        let Some(runtime_id) = st.db.session_brief(&session_id).ok().and_then(|row| row["runtimeId"].as_str().map(str::to_owned)) else { return; };
        let runtime = st.runtimes.read().await.iter().find(|runtime| runtime["id"].as_str() == Some(runtime_id.as_str())).cloned();
        let Some(runtime) = runtime.filter(runtime_usable) else { return; };
        // Opening an OpenCode conversation can also activate it immediately.
        // Discovery and the explicit model picker own OpenCode refreshes.
        if runtime["provider"] == "opencode" || cached(&st, &runtime).is_some() { return; }
        prewarm_one(&st, &runtime).await;
    });
}

async fn prewarm_one(st: &AppState, runtime: &Value) {
    let _ = get(st, runtime, None, None, Mode::Fresh).await;
}

/// Compare catalogs by model ids and their effort/speed capabilities.
fn model_signature(value: &Value) -> Vec<(String, Vec<String>, Vec<String>)> {
    value["models"].as_array().map(|models| {
        models.iter().map(|model| {
            let thinking = model["supportedThinking"].as_array()
                .map(|levels| levels.iter().filter_map(Value::as_str).map(str::to_owned).collect())
                .unwrap_or_default();
            let tiers = model["serviceTiers"].as_array()
                .map(|tiers| tiers.iter().map(|tier| tier["id"].as_str().unwrap_or("").to_owned()).collect())
                .unwrap_or_default();
            (model["id"].as_str().unwrap_or("").to_owned(), thinking, tiers)
        }).collect()
    }).unwrap_or_default()
}

#[cfg(test)]
mod warm_tests {
    use super::*;
    use serde_json::json;

    fn session(runtime: &str, state: &str) -> Value {
        json!({"id": format!("s-{runtime}-{state}"), "runtimeId": runtime, "state": state})
    }

    #[test]
    fn a_launch_warms_the_recent_conversations_and_every_live_turn() {
        // Newest first, as `session_list` orders them. Five recent rows warm two
        // distinct runtimes; a sixth is not warmed just for being near.
        let rows = vec![
            session("rt-claude", "idle"), session("rt-claude", "completed"), session("rt-codex", "idle"),
            session("rt-claude", "idle"), session("rt-open", "idle"), session("rt-unused", "idle"),
        ];
        assert_eq!(wanted_runtime_ids(&rows), ["rt-claude", "rt-codex", "rt-open"]);
        // A live turn below that line is warmed anyway: its conversation is open.
        let mut rows = rows;
        rows.push(session("rt-live", "working"));
        assert_eq!(wanted_runtime_ids(&rows), ["rt-claude", "rt-codex", "rt-open", "rt-live"]);
        // A conversation with no runtime (a retired row) warms nothing.
        assert!(wanted_runtime_ids(&[session("", "working")]).is_empty());
        assert!(wanted_runtime_ids(&[]).is_empty());
    }
}

#[cfg(test)]
mod opencode_billing_tests {
    use super::*;
    use serde_json::json;

    fn catalog() -> Value {
        json!({"models":[
            {"id":"opencode/big-pickle"},{"id":"opencode/deepseek-v4.1-flash-free"},
            {"id":"opencode/deepseek-v4.1-flash"},{"id":"opencode/claude-opus-5-5"},
            {"id":"opencode-go/deepseek-v4.1-flash"},{"id":"ollama/llama4"}
        ]})
    }
    fn ids(value: &Value) -> Vec<&str> { value["models"].as_array().unwrap().iter().map(|m| m["id"].as_str().unwrap()).collect() }

    #[test]
    fn paid_zen_models_are_recognised() {
        assert!(opencode_paid_zen_model("opencode/deepseek-v4.1-flash"));
        assert!(opencode_paid_zen_model("opencode/claude-opus-5-5"));
        assert!(!opencode_paid_zen_model("opencode/deepseek-v4.1-flash-free"));
        assert!(!opencode_paid_zen_model("opencode/big-pickle"));
        assert!(!opencode_paid_zen_model("opencode-go/deepseek-v4.1-flash"));
        assert!(!opencode_paid_zen_model("ollama/llama4"));
    }

    #[test]
    fn only_free_zen_and_linked_go_models_are_offered() {
        // Owner, 16 November 2026: Zen's free models, or OpenCode Go when linked; never pay-as-you-go.
        let linked = json!({"gatewayAuthStates":{"opencode":"authenticated","opencode-go":"authenticated"}});
        let mut value = catalog();
        keep_opencode_included_models(&linked, &mut value);
        assert_eq!(ids(&value), ["opencode/big-pickle","opencode/deepseek-v4.1-flash-free","opencode-go/deepseek-v4.1-flash","ollama/llama4"]);

        let zen_only = json!({"gatewayAuthStates":{"opencode":"authenticated"}});
        let mut value = catalog();
        keep_opencode_included_models(&zen_only, &mut value);
        assert_eq!(ids(&value), ["opencode/big-pickle","opencode/deepseek-v4.1-flash-free","ollama/llama4"]);

        let signed_out_go = json!({"gatewayAuthStates":{"opencode-go":"unauthenticated"}});
        let mut value = catalog();
        keep_opencode_included_models(&signed_out_go, &mut value);
        assert!(!ids(&value).iter().any(|id| id.starts_with("opencode-go/")));

        // Unreadable sign-ins leave Go to OpenCode, which lists it only when linked.
        let unknown = json!({"gatewayAuthStates":{}});
        let mut value = catalog();
        keep_opencode_included_models(&unknown, &mut value);
        assert!(ids(&value).contains(&"opencode-go/deepseek-v4.1-flash"));
        assert!(!ids(&value).contains(&"opencode/deepseek-v4.1-flash"));
    }
}
