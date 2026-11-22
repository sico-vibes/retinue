//! The console: how a host with no window is turned on and paired (17 November
//! 2026, multi-device piece 8).
//!
//! A headless Linux host (`retinued` under systemd) has no desktop shell, so the
//! three decisions the owner makes at the computer — turn the gateway on, make a
//! pairing code, approve a waiting device with a tier — had no way to be made
//! (`docs/research/multi-device/04-linux-headless.md`, section 5). They do now,
//! from the host's own terminal:
//!
//! ```text
//! retinued remote on --method tailscale
//! retinued pair
//! retinued pair confirm --tier chat
//! ```
//!
//! **The token.** When the daemon starts without a desktop shell it mints a
//! console token, writes it to `<data>/console.json` (a user-only file, in a
//! user-only folder) and accepts it on the loopback API **only** for the
//! `console.*` methods below. It is deliberately not the daemon capability: that
//! one stays in memory and reaches only the shell that started the daemon, so a
//! file on disk can never act as the daemon. A desktop-launched daemon writes no
//! console file at all, and the `console.*` methods answer `unauthorized` for
//! every caller.
//!
//! **The decisions.** Each one goes through the same function the desktop's
//! Settings page calls (`remote::enable`, `remote::disable`, `remote::rpc` for
//! `remote.pair.begin` / `remote.pair.confirm` / `remote.pair.cancel`), so the
//! tier rules, the audit lines, the events and the sentences are identical. This
//! module adds no policy of its own.
//!
//! **Setting the host up (19 November 2026).** Pairing alone left the host
//! incomplete: "projects from other machines" is a Settings switch and a folder,
//! and no bot can be created without a window. The console answers both with an
//! explicit allowlist — `projects.devices.enabled` and `projects.devices.folder` —
//! and with `console.bot.create`, which goes through `dispatch("agent.create")`
//! exactly as the desktop does, except that a bot made here always asks before it
//! acts. `console.agents` and `console.bot.list` are the reads beside them. The
//! allowlist is the whole reach: a key it does not name is refused, so the console
//! can never write a preference, a credential or a secret.
//!
//! **Never logged.** The pairing code, the console token and the device token are
//! printed to the terminal and nowhere else: not the log, not the database, not
//! an event, not `console.json` (which holds only the console token).

use crate::paths;
use crate::project_share;
use crate::{derr, remote, AppState, DispatchError};
use axum::http::StatusCode;
use retinue_protocol::PROTOCOL_VERSION;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex as StdMutex;
use std::time::Duration;

/// The file a headless daemon writes so the console client can reach it.
const CONSOLE_FILE: &str = "console.json";

/// What `console.settings.get` and `.set` reach, and nothing else. Two keys, both
/// this machine's own: what another machine may do here, and the one folder it may
/// do it in. A key outside this list is refused rather than ignored, so the console
/// cannot read a secret and cannot write the desktop's preferences behind its back.
const SETTINGS: [&str; 2] = [project_share::SETTING_ENABLED, project_share::SETTING_FOLDER];

#[derive(Default)]
pub struct Console {
    inner: StdMutex<Option<Live>>,
    /// Whether a desktop shell started this daemon, recorded at startup
    /// (`paths::headless()`). It cannot change while the daemon runs, and a
    /// sentence that asked the environment on every call would read the test
    /// process as a host with no window.
    headless: AtomicBool,
}

struct Live {
    token: String,
    file: PathBuf,
}

impl Console {
    /// Answers whether a presented bearer is this boot's console token.
    /// Constant-time, like the daemon capability.
    pub fn accepts(&self, bearer: &str) -> bool {
        let live = self.inner.lock().unwrap();
        match live.as_ref() {
            Some(live) => retinue_security::capability_matches(&live.token, bearer),
            None => false,
        }
    }

    /// For the tests and the Doctor: a console is live only on a headless host.
    #[cfg(test)]
    pub fn is_live(&self) -> bool {
        self.inner.lock().unwrap().is_some()
    }

    /// Whether this daemon was started without a desktop shell, as the systemd unit
    /// starts it. A host with no window has no Settings page, so a refusal that
    /// would send the owner there names the console command instead.
    pub fn headless_host(&self) -> bool {
        self.headless.load(Ordering::Relaxed)
    }

    /// The tests' own headless start: the token without a file, so no test writes
    /// into the owner's data folder.
    #[cfg(test)]
    pub fn start_in_memory(&self, token: &str) {
        self.headless.store(true, Ordering::Relaxed);
        *self.inner.lock().unwrap() = Some(Live { token: token.to_string(), file: PathBuf::from("no-file") });
    }
}

/// Called once at startup, after the listener has an address. A desktop-launched
/// daemon does nothing here.
pub fn startup(st: &AppState) {
    let headless = paths::headless();
    st.console.headless.store(headless, Ordering::Relaxed);
    if !headless {
        return;
    }
    let Some(address) = st.team.address.lock().ok().and_then(|address| address.clone()) else { return };
    let Some(dir) = crate::sandbox::data_folder(st) else { return };
    let token = retinue_security::new_capability();
    let file = dir.join(CONSOLE_FILE);
    if write_file(&file, &json!({"address": address, "token": token, "pid": std::process::id()})).is_err() {
        // Without the file the owner simply has no console; the daemon is
        // unaffected, and the log line names no path.
        tracing::warn!("the console file could not be written");
        return;
    }
    *st.console.inner.lock().unwrap() = Some(Live { token, file });
}

/// Removes the file on a clean exit, so a later `retinued pair` says "not
/// running" instead of guessing.
pub fn shutdown(st: &AppState) {
    let live = st.console.inner.lock().unwrap().take();
    if let Some(live) = live {
        let _ = std::fs::remove_file(&live.file);
    }
}

/// Writes the console file owner-only: created empty, restricted, then filled, so
/// the token is never in a file another account can read (`remote::native.rs`
/// uses the same order for the certificate key).
fn write_file(path: &std::path::Path, value: &Value) -> Result<(), String> {
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)
        .map_err(|_| "The console file could not be created.".to_string())?;
    crate::sandbox::restrict(path, false).map_err(|_| "The console file could not be restricted.".to_string())?;
    file.write_all(value.to_string().as_bytes()).map_err(|_| "The console file could not be written.".to_string())?;
    file.flush().map_err(|_| "The console file could not be written.".to_string())
}

// ---------------------------------------------------------------------------
// Routing

/// The `console.*` family. Only `main_rpc` reaches it, and only with the console
/// token, so no paired device and no bot can call it.
pub async fn route(st: &AppState, method: &str, p: &Value) -> Result<Value, DispatchError> {
    match method {
        "console.status" => Ok(status(st)),
        "console.remote.enable" => {
            let status = match p["method"].as_str() {
                Some(method @ ("tailscale" | "native")) => {
                    // Choosing the method is refused by the daemon while the other
                    // one is on; the console repeats the window's own rule rather
                    // than writing the setting behind its back.
                    remote::rpc(st, "remote.method.set", &json!({"method": method})).await?;
                    remote::rpc(st, "remote.enable", &json!({})).await?
                }
                Some(_) => return Err(derr("invalid_argument", "method must be tailscale or native", StatusCode::BAD_REQUEST)),
                None => remote::rpc(st, "remote.enable", &json!({})).await?,
            };
            Ok(json!({"remote": status}))
        }
        "console.remote.disable" => {
            let status = remote::rpc(st, "remote.disable", &json!({})).await?;
            Ok(json!({"remote": status}))
        }
        "console.pair.begin" => remote::rpc(st, "remote.pair.begin", &json!({})).await,
        "console.pair.cancel" => remote::rpc(st, "remote.pair.cancel", &json!({})).await,
        "console.pair.pending" => {
            let pending = st.remote.pairing.lock().unwrap().live_pending();
            Ok(json!({"pending": pending}))
        }
        "console.pair.confirm" | "console.pair.deny" => {
            let approve = method == "console.pair.confirm";
            let pairing_id = match p["pairingId"].as_str().filter(|id| !id.is_empty()).map(str::to_owned) {
                Some(id) => id,
                // The terminal has no list to click: the one waiting request is
                // the one meant, and refusing when there is none is honest.
                None => st
                    .remote
                    .pairing
                    .lock()
                    .unwrap()
                    .live_pending()
                    .and_then(|pending| pending["pairingId"].as_str().map(str::to_owned))
                    .ok_or_else(|| derr("conflict", "No device is waiting.", StatusCode::CONFLICT))?,
            };
            let mut params = json!({"pairingId": pairing_id, "approve": approve});
            if approve {
                // `remote.pair.confirm` itself refuses a missing or unknown tier
                // with the window's sentence; passing it through keeps one wording.
                params["tier"] = p.get("tier").cloned().unwrap_or(Value::Null);
            }
            remote::rpc(st, "remote.pair.confirm", &params).await
        }
        // 19 November 2026: what a window was needed for. The settings are the two
        // keys in `SETTINGS` and no others; a bot is created through the desktop's
        // own `agent.create`, so the validation, the events and the hire path are
        // the same code.
        "console.settings.get" => settings_get(st, p),
        "console.settings.set" => settings_set(st, p).await,
        "console.agents" => Ok(agents(st).await),
        // 21 November 2026: installing and updating a CLI on a host with no
        // window. The plan is the card: the terminal shows the version, host,
        // size and digest prefix, and `--yes` is the owner's consent.
        "console.agent.plan" => crate::agent_install::plan_reply(st, provider_arg(p)?).await,
        "console.agent.install" => console_agent_install(st, p).await,
        "console.agent.status" => crate::agent_install::rpc(st, "runtime.install.status", p).await,
        "console.bot.list" => bot_list(st).await,
        "console.bot.create" => bot_create(st, p).await,
        _ => Err(derr("unsupported", "method is not supported", StatusCode::NOT_FOUND)),
    }
}

/// The refusal for a key the console may not touch. It names the allowlist, so the
/// owner learns what the terminal can do without reading the code.
fn not_allowlisted() -> DispatchError {
    derr(
        "invalid_argument",
        &format!(
            "The console changes only {} — everything else belongs to the desktop's Settings page.",
            SETTINGS.join(" and ")
        ),
        StatusCode::BAD_REQUEST,
    )
}

/// `console.settings.get`: the allowlisted settings, and only those. Reading a key
/// outside the list is refused as well, because otherwise the read would be a way
/// to see whatever this host holds.
fn settings_get(st: &AppState, p: &Value) -> Result<Value, DispatchError> {
    let settings = st.db.settings().map_err(|error| derr("internal", &error.to_string(), StatusCode::INTERNAL_SERVER_ERROR))?;
    let mut shown = serde_json::Map::new();
    match p["key"].as_str().map(str::trim).filter(|key| !key.is_empty()) {
        Some(key) => {
            if !SETTINGS.contains(&key) {
                return Err(not_allowlisted());
            }
            shown.insert(key.to_string(), settings.get(key).cloned().unwrap_or(Value::Null));
        }
        None => {
            for key in SETTINGS {
                shown.insert(key.to_string(), settings.get(key).cloned().unwrap_or(Value::Null));
            }
        }
    }
    Ok(json!({"settings": Value::Object(shown), "allowlist": SETTINGS}))
}

/// `console.settings.set`: one allowlisted key, through the same `settings.set` the
/// desktop calls, so the value is validated once, the events are the desktop's and
/// nothing is written twice. The folder is checked on the disk here, because a
/// terminal has no picker: it must exist and be a folder, and the value stored is
/// its canonical path. One content-free activity line records the change.
async fn settings_set(st: &AppState, p: &Value) -> Result<Value, DispatchError> {
    let key = p["key"].as_str().map(str::trim).unwrap_or("");
    if !SETTINGS.contains(&key) {
        return Err(not_allowlisted());
    }
    let mut value = p.get("value").cloned().unwrap_or(Value::Null);
    if key == project_share::SETTING_FOLDER {
        if let Some(raw) = value.as_str().map(str::trim).filter(|path| !path.is_empty()) {
            let path = std::fs::canonicalize(raw).map_err(|_| derr("invalid_argument", "That folder is not on this computer.", StatusCode::BAD_REQUEST))?;
            if !path.is_dir() {
                return Err(derr("invalid_argument", "That folder is not on this computer.", StatusCode::BAD_REQUEST));
            }
            value = json!(project_share::display(&path));
        }
    }
    crate::dispatch(st, "settings.set", json!({"key": key, "value": value.clone()})).await?;
    // Content-free: no value and no path, because the activity list is the host's
    // own record and never carries one.
    let line = match (key, value.as_bool()) {
        (project_share::SETTING_ENABLED, Some(true)) => "Projects from other machines were turned on from the console.",
        (project_share::SETTING_ENABLED, Some(false)) => "Projects from other machines were turned off from the console.",
        _ if key == project_share::SETTING_FOLDER => "The folder for projects from other machines was set from the console.",
        _ => "A setting was changed from the console.",
    };
    remote::audit(st, None, "console", line).await;
    Ok(json!({"saved": true, "key": key, "value": value}))
}

/// The provider a console install names.
fn provider_arg(p: &Value) -> Result<&str, DispatchError> {
    p["provider"].as_str().map(str::trim).filter(|provider| !provider.is_empty()).ok_or_else(|| derr("invalid_argument", "provider is required: retinued agents install <provider>", StatusCode::BAD_REQUEST))
}

/// `console.agent.install`: without `confirm`, the plan the terminal shows; with
/// it, the install, consented to by the version and digest that same plan named.
/// The split exists because a terminal has no card to click: showing the plan and
/// asking for `--yes` is the consent step.
async fn console_agent_install(st: &AppState, p: &Value) -> Result<Value, DispatchError> {
    let provider = provider_arg(p)?;
    let plan = crate::agent_install::plan_reply(st, provider).await?;
    if p["confirm"] != true {
        return Ok(json!({"plan": plan["plan"], "confirmed": false}));
    }
    let params = json!({
        "provider": provider,
        "confirm": true,
        "version": plan["plan"]["version"],
        "digestPrefix": plan["plan"]["digestPrefix"],
    });
    crate::agent_install::rpc(st, "runtime.install", &params).await
}

/// `console.agents`: what the runtime discovery found on this host, whether each is
/// signed in, and how to sign one in. The rows are `provider_gate::list`'s, which
/// the desktop's Agents page reads: no executable path, no launch arguments, and
/// nothing read out of a sign-in.
async fn agents(st: &AppState) -> Value {
    let listed = crate::provider_gate::list(st).await;
    let rows = listed["providers"].as_array().cloned().unwrap_or_default();
    let agents = rows
        .into_iter()
        .map(|row| {
            json!({
                "provider": row["provider"],
                "name": row["displayName"],
                "installed": row["found"],
                "version": row["version"],
                "status": row["status"],
                "enabled": row["enabled"],
                "authState": row["authState"],
                // Three answers, not two: `true` signed in, `false` known signed
                // out or failed, `null` not known. An unanswered probe must never
                // read as "not signed in" (21 November 2026).
                "signedIn": signed_in(&row["authState"]),
                "signInCommand": sign_in_for(row["provider"].as_str().unwrap_or(""), &row["signInCommand"]),
                "runtimeIds": row["runtimeIds"],
            })
        })
        .collect::<Vec<_>>();
    json!({"agents": agents})
}

/// `true` only for a known signed-in row; `null` when the answer is unknown.
pub(crate) fn signed_in(auth_state: &Value) -> Value {
    match auth_state.as_str() {
        Some("authenticated") => json!(true),
        Some("unauthenticated" | "sign_in_failed") => json!(false),
        _ => Value::Null,
    }
}

/// Whether the row's sign-in command should be offered: only when the state says
/// the owner must sign in. An unknown state gets the command nowhere.
pub(crate) fn sign_in_needed(auth_state: &Value) -> bool {
    matches!(auth_state.as_str(), Some("unauthenticated" | "sign_in_failed"))
}

/// How to sign in from this host's terminal. The discovery's own `signInCommand`
/// covers the ACP providers and Pi; Claude Code, Codex and OpenCode are not table
/// rows there, so their commands are named here, in the same words as
/// `apps/desktop/src/ui/providerGuides.ts` and `sign_in_hint` (`main.rs`). Without
/// them a terminal would report "not signed in" and no next step.
fn sign_in_for(provider: &str, reported: &Value) -> Value {
    match reported.as_str() {
        Some(command) => json!(command),
        None => match provider {
            "claude" => json!("claude"),
            "codex" => json!("codex login"),
            "opencode" => json!("opencode auth login"),
            _ => Value::Null,
        },
    }
}

/// What one bot's row looks like in a console reply: who it is and what it runs on,
/// never its instructions, its environment or its conversations.
fn bot_summary(agent: &Value) -> Value {
    json!({
        "id": agent["id"],
        "name": agent["name"],
        "runtimeId": agent["runtimeId"],
        "model": agent["model"],
        "thinking": agent["thinking"],
        "approvalMode": agent["approvalMode"],
        "projectId": agent["projectId"],
    })
}

/// `console.bot.list`: every active bot, with the coding agent it runs on.
async fn bot_list(st: &AppState) -> Result<Value, DispatchError> {
    let listed = crate::dispatch(st, "agent.list", json!({})).await?;
    let runtimes = st.runtimes.read().await.clone();
    let bots = listed["agents"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|agent| {
            let provider = agent["runtimeId"]
                .as_str()
                .and_then(|id| runtimes.iter().find(|row| row["id"] == id))
                .and_then(|row| row["provider"].as_str())
                .unwrap_or("");
            let mut summary = bot_summary(&agent);
            summary["provider"] = json!(provider);
            summary
        })
        .collect::<Vec<_>>();
    Ok(json!({"bots": bots}))
}

/// The runtime an `--agent` names: an exact runtime id, else a running copy of that
/// provider, else any installed copy of it — the same choice the hire dialog offers
/// first.
async fn runtime_for(st: &AppState, agent: &str) -> Result<Value, DispatchError> {
    let runtimes = st.runtimes.read().await.clone();
    runtimes
        .iter()
        .find(|row| row["id"] == agent)
        .or_else(|| runtimes.iter().find(|row| row["provider"] == agent && row["status"] == "online"))
        .or_else(|| runtimes.iter().find(|row| row["provider"] == agent))
        .cloned()
        .ok_or_else(|| {
            derr(
                "invalid_argument",
                "That coding agent is not installed on this computer. `retinued agents` lists the ones that are.",
                StatusCode::BAD_REQUEST,
            )
        })
}

/// The project a folder belongs to. A folder with no row yet is registered through
/// `project.create`, the desktop's own New project path, so the row, its repository
/// identity and its event are the same; then the bot is placed there by
/// `agent.create`'s own team placement.
async fn project_for(st: &AppState, folder: &str) -> Result<String, DispatchError> {
    let path = std::fs::canonicalize(folder.trim()).map_err(|_| derr("invalid_argument", "That project folder is not on this computer.", StatusCode::BAD_REQUEST))?;
    if !path.is_dir() {
        return Err(derr("invalid_argument", "That project folder is not on this computer.", StatusCode::BAD_REQUEST));
    }
    let listed = crate::team::team_rpc(st, "project.list", &json!({})).await?;
    let existing = listed["projects"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|project| project["path"].as_str().is_some_and(|stored| std::fs::canonicalize(stored.trim()).is_ok_and(|other| other == path)))
        .and_then(|project| project["id"].as_str().map(str::to_owned));
    if let Some(id) = existing {
        return Ok(id);
    }
    let name = path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
    let created = crate::team::team_rpc(st, "project.create", &json!({"name": name, "path": project_share::display(&path)})).await?;
    created["project"]["id"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| derr("internal", "The project could not be added.", StatusCode::INTERNAL_SERVER_ERROR))
}

/// `console.bot.create`: a bot, created through `dispatch("agent.create")`, so the
/// name, the runtime, the model and the effort are validated exactly as the
/// desktop's form validates them, and the history, the events and the team
/// placement are the same. Two deliberate differences, both about a terminal: the
/// coding agent must be one the discovery reports installed here, and the bot always
/// asks before it acts — the window's new-bot default is Full access, and a card
/// that says so cannot be shown where there is no window.
async fn bot_create(st: &AppState, p: &Value) -> Result<Value, DispatchError> {
    let name = p["name"].as_str().map(str::trim).unwrap_or("");
    if name.is_empty() {
        return Err(derr("invalid_argument", "A bot needs a name: retinued bot add --name <name> --agent <codex|opencode|claude>.", StatusCode::BAD_REQUEST));
    }
    let asked = p["agent"].as_str().map(str::trim).unwrap_or("");
    if asked.is_empty() {
        return Err(derr("invalid_argument", "Name the coding agent it runs on: retinued bot add --name <name> --agent <codex|opencode|claude>.", StatusCode::BAD_REQUEST));
    }
    let runtime = runtime_for(st, asked).await?;
    let mut fields = json!({
        "name": name,
        "runtimeId": runtime["id"],
        "approvalMode": "ask",
    });
    if let Some(model) = p["model"].as_str().map(str::trim).filter(|value| !value.is_empty()) {
        fields["model"] = json!(model);
    }
    if let Some(effort) = p["effort"].as_str().map(str::trim).filter(|value| !value.is_empty()) {
        fields["thinking"] = json!(effort);
    }
    if let Some(folder) = p["project"].as_str().map(str::trim).filter(|value| !value.is_empty()) {
        fields["projectId"] = json!(project_for(st, folder).await?);
    }
    // Boxed: `agent.create` reaches the whole hire path, and an inline future that
    // size overflows a worker thread's stack in a debug build.
    let reply = Box::pin(crate::dispatch(st, "agent.create", fields.clone())).await?;
    let mut summary = bot_summary(&reply["agent"]);
    summary["provider"] = runtime["provider"].clone();
    Ok(json!({"agent": summary}))
}

fn status(st: &AppState) -> Value {
    let data = crate::sandbox::data_folder(st);
    let secrets = data.as_ref().map(|dir| dir.join("secrets"));
    json!({
        "version": env!("CARGO_PKG_VERSION"),
        "platform": std::env::consts::OS,
        "headless": paths::headless(),
        "dataFolder": data.as_ref().map(|dir| dir.display().to_string()),
        "database": paths::database().display().to_string(),
        "secrets": {
            "backend": crate::secrets::store::backend_name(),
            "folder": secrets.map(|dir| dir.display().to_string()),
        },
        "remote": st.remote.status_value(st),
    })
}

// ---------------------------------------------------------------------------
// The console client: `retinued <command>`

/// What `retinued settings …`, `retinued agents`, `retinued bot …`, `retinued pair …`
/// and `retinued remote …` print and return. Fixed text only: nothing here reads a
/// secret out of the file.
pub async fn run_cli(args: &[String]) -> i32 {
    let command = args.first().map(String::as_str).unwrap_or("");
    let rest = &args[1..];
    match command {
        "status" => status_command().await,
        "pair" => pair_command(rest).await,
        "remote" => remote_command(rest).await,
        "settings" => settings_command(rest).await,
        "agents" => agents_command(rest).await,
        "bot" => bot_command(rest).await,
        other => {
            eprintln!("Unknown command: {other}");
            usage();
            2
        }
    }
}

fn usage() {
    eprintln!(
        "usage:\n  retinued status                   show this host's version, folders and Remote state\n  retinued remote on [--method M]   turn the gateway on (tailscale or native)\n  retinued remote off               turn it off\n  retinued pair                     print a code and wait for a device\n  retinued pair list                show the waiting request\n  retinued pair confirm --tier T    approve it (viewer, chat or operator)\n  retinued pair deny                refuse it\n  retinued pair cancel              throw the open code away\n  retinued settings get [key]       show projects.devices.enabled and projects.devices.folder\n  retinued settings set <key> <value>   change one of those two\n  retinued agents                   what coding agents are installed and signed in
  retinued agents install <provider> [--yes]   install or update one, with no window\n  retinued bot add --name N --agent A [--model M] [--effort E] [--project F]\n  retinued bot list                 show this host's bots"
    );
}

/// The daemon's address and console token, or `None` with the reason printed.
fn endpoint() -> Option<(String, String)> {
    let Some(dir) = paths::data_folder() else {
        eprintln!("Retinue's data folder could not be found.");
        return None;
    };
    let path = dir.join(CONSOLE_FILE);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(_) => {
            eprintln!("No headless daemon is running here. Start it first (its log says if it failed).");
            return None;
        }
    };
    let parsed: Value = serde_json::from_str(&text).ok()?;
    let address = parsed["address"].as_str()?.to_string();
    let token = parsed["token"].as_str()?.to_string();
    Some((address, token))
}

/// One loopback call. No redirects, a short timeout, and the reply's sentence on
/// failure.
async fn call(address: &str, token: &str, method: &str, params: Value) -> Result<Value, String> {
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|_| "The console client could not start.".to_string())?;
    let reply = client
        .post(format!("http://{address}/v1/rpc"))
        .bearer_auth(token)
        .json(&json!({"v": PROTOCOL_VERSION, "id": uuid::Uuid::new_v4().to_string(), "method": method, "params": params}))
        .send()
        .await
        .map_err(|_| "The daemon did not answer. Is it still running?".to_string())?;
    let body: Value = reply.json().await.map_err(|_| "The daemon's answer could not be read.".to_string())?;
    if body["ok"] == true {
        Ok(body["result"].clone())
    } else {
        Err(body["error"]["message"].as_str().unwrap_or("The daemon refused that.").to_string())
    }
}

async fn status_command() -> i32 {
    let Some((address, token)) = endpoint() else { return 3 };
    match call(&address, &token, "console.status", json!({})).await {
        Ok(status) => {
            let remote = &status["remote"];
            println!("retinued {} ({})", status["version"].as_str().unwrap_or("?"), status["platform"].as_str().unwrap_or("?"));
            println!("data folder  {}", status["dataFolder"].as_str().unwrap_or("-"));
            println!("secrets      {} in {}", status["secrets"]["backend"].as_str().unwrap_or("-"), status["secrets"]["folder"].as_str().unwrap_or("-"));
            println!(
                "remote       {} ({}){}",
                if remote["enabled"] == true { "on" } else { "off" },
                remote["method"].as_str().unwrap_or("-"),
                match remote["url"].as_str() { Some(url) => format!(" at {url}"), None => String::new() }
            );
            if let Some(error) = remote["error"].as_str() {
                println!("             {error}");
            }
            // The Native method's certificate: the owner compares this with the
            // one the linking machine shows before sending a password.
            if let Some(fingerprint) = remote["native"]["fingerprint"].as_str() {
                println!("certificate  {fingerprint}");
            }
            0
        }
        Err(message) => {
            eprintln!("{message}");
            1
        }
    }
}

async fn remote_command(rest: &[String]) -> i32 {
    let action = rest.first().map(String::as_str).unwrap_or("");
    let method = rest.iter().position(|arg| arg == "--method").and_then(|at| rest.get(at + 1));
    if let Some(at) = rest.iter().position(|arg| arg == "--method") {
        if rest.get(at + 1).is_none() {
            eprintln!("--method needs tailscale or native.");
            return 2;
        }
    }
    let Some((address, token)) = endpoint() else { return 3 };
    let (method_name, params) = match action {
        "on" => ("console.remote.enable", match method { Some(method) => json!({"method": method}), None => json!({}) }),
        "off" => ("console.remote.disable", json!({})),
        _ => {
            eprintln!("Usage: retinued remote on|off [--method tailscale|native]");
            return 2;
        }
    };
    match call(&address, &token, method_name, params).await {
        Ok(reply) => {
            let remote = &reply["remote"];
            println!(
                "Remote is {} ({}).{}",
                if remote["enabled"] == true { "on" } else { "off" },
                remote["method"].as_str().unwrap_or("-"),
                match remote["url"].as_str() { Some(url) => format!(" {url}"), None => String::new() }
            );
            if let Some(error) = remote["error"].as_str() {
                println!("{error}");
                return 1;
            }
            0
        }
        Err(message) => {
            eprintln!("{message}");
            if message.contains("Turn Remote on") {
                eprintln!("Start it with: retinued remote on --method tailscale");
            }
            1
        }
    }
}

/// `retinued settings get [key]` and `retinued settings set <key> <value>`. Everything
/// after the key is the value, so a folder with a space works quoted or not;
/// `true`/`false` become JSON booleans and everything else is sent as text, which the
/// daemon's own validation answers.
async fn settings_command(rest: &[String]) -> i32 {
    let action = rest.first().map(String::as_str).unwrap_or("");
    let (method, params) = match action {
        "get" => {
            let key = rest.get(1).map(String::as_str).filter(|key| !key.is_empty());
            let params = match key {
                Some(key) => json!({"key": key}),
                None => json!({}),
            };
            ("console.settings.get", params)
        }
        "set" => {
            let key = rest.get(1).map(String::as_str).unwrap_or("");
            if key.is_empty() || rest.len() < 3 {
                eprintln!("Usage: retinued settings set <key> <value>");
                return 2;
            }
            // Everything after the key is the value, so an unquoted folder with a
            // space is not silently cut to its first word.
            let text = rest[2..].join(" ");
            let value = match text.to_ascii_lowercase().as_str() {
                "true" => json!(true),
                "false" => json!(false),
                _ => json!(text),
            };
            ("console.settings.set", json!({"key": key, "value": value}))
        }
        other => {
            eprintln!("Unknown settings command: {other}");
            usage();
            return 2;
        }
    };
    let Some((address, token)) = endpoint() else { return 3 };
    match call(&address, &token, method, params).await {
        Ok(reply) if method == "console.settings.get" => {
            for (key, value) in reply["settings"].as_object().into_iter().flatten() {
                println!("{:<28} {}", key, setting_word(value));
            }
            0
        }
        Ok(reply) => {
            println!("{} is now {}.", reply["key"].as_str().unwrap_or("The setting"), setting_word(&reply["value"]));
            0
        }
        Err(message) => {
            eprintln!("{message}");
            1
        }
    }
}

/// One setting value for the terminal. `(not set)` rather than a bare blank, so a
/// missing folder is not mistaken for an empty one.
fn setting_word(value: &Value) -> String {
    match value {
        Value::Bool(true) => "true".to_string(),
        Value::Bool(false) => "false".to_string(),
        Value::Null => "(not set)".to_string(),
        Value::String(text) if text.is_empty() => "(not set)".to_string(),
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

/// `retinued agents`: what the discovery found, and whether each is signed in. Never
/// a credential: the state word and the CLI's own sign-in command, both of which the
/// desktop's Agents page already shows.
///
/// `retinued agents install <provider> [--yes]` (21 November 2026) is the headless
/// host's install and update. Without `--yes` it prints the same plan card the
/// window shows and changes nothing; with it, the daemon downloads the version
/// that card named, checks the publisher's digest and installs it under
/// `<data>/agents/`.
async fn agents_command(rest: &[String]) -> i32 {
    let action = rest.first().map(String::as_str).unwrap_or("");
    match action {
        "" | "list" => agents_list().await,
        "install" => match rest.get(1).map(String::as_str).filter(|provider| !provider.is_empty()) {
            Some(provider) => agents_install(provider, rest.iter().any(|arg| arg == "--yes")).await,
            None => {
                eprintln!("Usage: retinued agents install <claude|codex|opencode|pi> [--yes]");
                2
            }
        },
        other => {
            eprintln!("Unknown agents command: {other}");
            usage();
            2
        }
    }
}

/// The plan card, in words a terminal can show. No URL: the host and the digest
/// prefix are what the owner checks.
fn plan_lines(plan: &Value) -> Vec<String> {
    let name = plan["displayName"].as_str().unwrap_or("This coding agent");
    let mode = plan["mode"].as_str().unwrap_or("-");
    let version = plan["version"].as_str().unwrap_or("-");
    let mut lines = vec![format!("{name}: {mode}, version {version}")];
    if let Some(host) = plan["sourceHost"].as_str() {
        lines.push(format!("  from {host}"));
    }
    match plan["bytes"].as_u64() {
        Some(bytes) => lines.push(format!("  {:.1} MB", bytes as f64 / (1024.0 * 1024.0))),
        None => lines.push("  size not stated by the publisher".into()),
    }
    if let (Some(prefix), Some(algorithm)) = (plan["digestPrefix"].as_str(), plan["digestAlgorithm"].as_str()) {
        lines.push(format!("  {algorithm} {prefix}…"));
    }
    if let Some(free) = plan["freeBytes"].as_u64() {
        lines.push(format!("  {:.1} GB free", free as f64 / (1024.0 * 1024.0 * 1024.0)));
    }
    if let Some(page) = plan["page"].as_str().filter(|_| plan["supported"] != true) {
        lines.push(format!("  {page}"));
    }
    if let Some(reason) = plan["reason"].as_str() {
        lines.push(format!("  {reason}"));
    }
    if plan["runsTurn"] == true {
        lines.push("  a bot is using this coding agent".into());
    }
    lines
}

async fn agents_install(provider: &str, confirmed: bool) -> i32 {
    let Some((address, token)) = endpoint() else { return 3 };
    let method = if confirmed { "console.agent.install" } else { "console.agent.plan" };
    let params = if confirmed { json!({"provider": provider, "confirm": true}) } else { json!({"provider": provider}) };
    let reply = match call(&address, &token, method, params).await {
        Ok(reply) => reply,
        Err(message) => {
            eprintln!("{message}");
            return 1;
        }
    };
    if !confirmed {
        let plan = &reply["plan"];
        for line in plan_lines(plan) {
            println!("{line}");
        }
        if plan["supported"] == true {
            println!("Run again with --yes to install it.");
        }
        return 0;
    }
    let job = reply["jobId"].as_str().unwrap_or("").to_string();
    if job.is_empty() {
        println!("{}", reply["state"].as_str().unwrap_or("The install did not start."));
        return 1;
    }
    println!("Installing {provider}…");
    let mut last = String::new();
    let deadline = std::time::Instant::now() + Duration::from_secs(30 * 60 + 30);
    loop {
        match call(&address, &token, "console.agent.status", json!({"jobId": job})).await {
            Ok(reply) => {
                let row = &reply["job"];
                let phase = row["phase"].as_str().unwrap_or("");
                let line = match (phase, row["bytesTotal"].as_u64(), row["bytesDone"].as_u64()) {
                    ("downloading", Some(total), Some(done)) if total > 0 => format!("Downloading {:.0}%", done as f64 * 100.0 / total as f64),
                    ("downloading", _, Some(done)) => format!("Downloading {:.1} MB", done as f64 / (1024.0 * 1024.0)),
                    ("checking", _, _) => "Checking the download".into(),
                    ("installing", _, _) => "Installing".into(),
                    (other, _, _) => other.to_string(),
                };
                if line != last {
                    println!("  {line}");
                    last = line;
                }
                match phase {
                    "done" => {
                        println!("Done · {}", row["version"].as_str().unwrap_or("-"));
                        return 0;
                    }
                    "failed" | "cancelled" => {
                        eprintln!("{}", row["error"].as_str().unwrap_or("The install did not finish."));
                        if let Some(line) = row["line"].as_str().filter(|line| !line.is_empty()) {
                            eprintln!("  {line}");
                        }
                        return 1;
                    }
                    _ => {}
                }
            }
            Err(message) => {
                eprintln!("{message}");
                return 1;
            }
        }
        if std::time::Instant::now() >= deadline {
            eprintln!("The install did not answer in time.");
            return 1;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

/// `retinued agents` and `retinued agents list`.
async fn agents_list() -> i32 {
    let Some((address, token)) = endpoint() else { return 3 };
    let reply = match call(&address, &token, "console.agents", json!({})).await {
        Ok(reply) => reply,
        Err(message) => {
            eprintln!("{message}");
            return 1;
        }
    };
    let rows = reply["agents"].as_array().cloned().unwrap_or_default();
    let installed = rows.iter().filter(|row| row["installed"] == true).collect::<Vec<_>>();
    if installed.is_empty() {
        println!("No coding agent is installed for this host's user. Install one, then restart retinued.");
        return 0;
    }
    for row in installed {
        println!("{:<12} {:<16} {}", row["provider"].as_str().unwrap_or("-"), row["version"].as_str().unwrap_or("-"), sign_in_word(&row["authState"]));
        // A sign-in command only where the state says a sign-in is needed; an
        // unknown answer is not a signed-out one.
        if sign_in_needed(&row["authState"]) {
            if let Some(command) = row["signInCommand"].as_str() {
                println!("             sign in with: {command}");
            }
        }
    }
    0
}

/// The sign-in state as the daemon recorded it, in words.
fn sign_in_word(auth_state: &Value) -> &'static str {
    match auth_state.as_str() {
        Some("authenticated") => "signed in",
        Some("unauthenticated") => "not signed in",
        Some("sign_in_failed") => "sign-in failed",
        _ => "sign-in not known yet",
    }
}

/// `retinued bot add …` and `retinued bot list`.
async fn bot_command(rest: &[String]) -> i32 {
    let action = rest.first().map(String::as_str).unwrap_or("");
    let flag = |name: &str| rest.iter().position(|arg| arg == name).and_then(|at| rest.get(at + 1)).cloned();
    let Some((address, token)) = endpoint() else { return 3 };
    match action {
        "list" => match call(&address, &token, "console.bot.list", json!({})).await {
            Ok(reply) => {
                let bots = reply["bots"].as_array().cloned().unwrap_or_default();
                if bots.is_empty() {
                    println!("No bots on this host yet. Add one with: retinued bot add --name <name> --agent <codex|opencode|claude>");
                    return 0;
                }
                println!("{:<24} {:<12} {:<28} Id", "Name", "Agent", "Model");
                for bot in bots {
                    let agent = bot["provider"].as_str().filter(|name| !name.is_empty()).or_else(|| bot["runtimeId"].as_str()).unwrap_or("-");
                    println!(
                        "{:<24} {:<12} {:<28} {}",
                        bot["name"].as_str().unwrap_or("-"),
                        agent,
                        bot["model"].as_str().unwrap_or("-"),
                        bot["id"].as_str().unwrap_or("-")
                    );
                }
                0
            }
            Err(message) => {
                eprintln!("{message}");
                1
            }
        },
        "add" => {
            let (Some(name), Some(agent)) = (flag("--name"), flag("--agent")) else {
                eprintln!("Usage: retinued bot add --name <name> --agent <codex|opencode|claude> [--model <id>] [--effort <level>] [--project <folder>]");
                return 2;
            };
            let mut params = json!({"name": name, "agent": agent});
            for (flag_name, key) in [("--model", "model"), ("--effort", "effort"), ("--project", "project")] {
                if let Some(value) = flag(flag_name) {
                    params[key] = json!(value);
                }
            }
            match call(&address, &token, "console.bot.create", params).await {
                Ok(reply) => {
                    let bot = &reply["agent"];
                    let on = bot["provider"].as_str().filter(|name| !name.is_empty()).or_else(|| bot["runtimeId"].as_str()).unwrap_or("a coding agent");
                    let model = match bot["model"].as_str() {
                        Some(model) => format!(", model {model}"),
                        None => String::new(),
                    };
                    let project = match bot["projectId"].as_str() {
                        Some(_) => ", in its project",
                        None => "",
                    };
                    println!(
                        "Added {} on {}{model}{project}, id {}. It asks before it acts.",
                        bot["name"].as_str().unwrap_or(&name),
                        on,
                        bot["id"].as_str().unwrap_or("-")
                    );
                    0
                }
                Err(message) => {
                    eprintln!("{message}");
                    1
                }
            }
        }
        other => {
            eprintln!("Unknown bot command: {other}");
            usage();
            2
        }
    }
}

async fn pair_command(rest: &[String]) -> i32 {
    let action = rest.first().map(String::as_str).unwrap_or("");
    let Some((address, token)) = endpoint() else { return 3 };
    match action {
        "list" => {
            match call(&address, &token, "console.pair.pending", json!({})).await {
                Ok(reply) => {
                    match pending_summary(&reply["pending"]) {
                        Some(line) => println!("{line}"),
                        None => println!("No device is waiting."),
                    }
                    0
                }
                Err(message) => {
                    eprintln!("{message}");
                    1
                }
            }
        }
        "confirm" | "deny" => {
            let approve = action == "confirm";
            let tier = rest.iter().position(|arg| arg == "--tier").and_then(|at| rest.get(at + 1)).cloned();
            if approve && !matches!(tier.as_deref(), Some("viewer" | "chat" | "operator")) {
                eprintln!("Approve with a tier: retinued pair confirm --tier viewer|chat|operator");
                return 2;
            }
            let method = if approve { "console.pair.confirm" } else { "console.pair.deny" };
            let params = if approve { json!({"approve": true, "tier": tier}) } else { json!({"approve": false}) };
            match call(&address, &token, method, params).await {
                Ok(reply) if reply["approved"] == true => {
                    let device = &reply["device"];
                    println!(
                        "Approved {} ({}, {}) at {}.",
                        device["name"].as_str().unwrap_or("a device"),
                        device["kind"].as_str().unwrap_or("-"),
                        device["tier"].as_str().unwrap_or("-"),
                        device["platform"].as_str().unwrap_or("-")
                    );
                    0
                }
                Ok(_) => {
                    println!("Refused. The device was not paired.");
                    0
                }
                Err(message) => {
                    eprintln!("{message}");
                    1
                }
            }
        }
        "cancel" => match call(&address, &token, "console.pair.cancel", json!({})).await {
            Ok(reply) => {
                println!("{}", if reply["cancelled"] == true { "The code is no longer valid." } else { "No code was open." });
                0
            }
            Err(message) => {
                eprintln!("{message}");
                1
            }
        },
        // `retinued pair`: make a code, print it once, then wait for a device.
        "" => wait_for_a_device(address, token).await,
        other => {
            eprintln!("Unknown pair command: {other}");
            usage();
            2
        }
    }
}

/// Prints the code, then waits for a device to use it, then says exactly how to
/// approve it. The code is never written anywhere.
async fn wait_for_a_device(address: String, token: String) -> i32 {
    let begun = match call(&address, &token, "console.pair.begin", json!({})).await {
        Ok(begun) => begun,
        Err(message) => {
            eprintln!("{message}");
            if message.contains("Turn Remote on") {
                eprintln!("Start it with: retinued remote on --method tailscale");
            }
            return 1;
        }
    };
    let code = begun["code"].as_str().unwrap_or("").to_string();
    println!("On the computer you are pairing, open:");
    println!("  {}", begun["url"].as_str().unwrap_or(""));
    println!("and enter this code:");
    println!("  {code}");
    println!("It expires at {}. Nothing is approved until you run:", begun["expiresAt"].as_str().unwrap_or("-"));
    println!("  retinued pair confirm --tier chat");
    println!("Waiting for a device…");
    let deadline = std::time::Instant::now() + Duration::from_secs(5 * 60 + 10);
    loop {
        match call(&address, &token, "console.pair.pending", json!({})).await {
            Ok(reply) => {
                if let Some(line) = pending_summary(&reply["pending"]) {
                    println!("{line}");
                    println!("Approve with: retinued pair confirm --tier viewer|chat|operator");
                    println!("or refuse with: retinued pair deny");
                    return 0;
                }
            }
            Err(message) => {
                eprintln!("{message}");
                return 1;
            }
        }
        if std::time::Instant::now() >= deadline {
            println!("The code expired. Run `retinued pair` again.");
            return 0;
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

/// One line describing a waiting request. No code, no token.
fn pending_summary(pending: &Value) -> Option<String> {
    if !pending.is_object() {
        return None;
    }
    Some(format!(
        "{} ({}, {}, {} sign-in) is waiting. Request id {}.",
        pending["name"].as_str().unwrap_or("A device"),
        pending["kind"].as_str().unwrap_or("-"),
        pending["platform"].as_str().unwrap_or("-"),
        pending["method"].as_str().unwrap_or("-"),
        pending["pairingId"].as_str().unwrap_or("-")
    ))
}
