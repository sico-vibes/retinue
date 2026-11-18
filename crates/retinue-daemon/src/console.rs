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
//! **Never logged.** The pairing code, the console token and the device token are
//! printed to the terminal and nowhere else: not the log, not the database, not
//! an event, not `console.json` (which holds only the console token).

use crate::paths;
use crate::{derr, remote, AppState, DispatchError};
use axum::http::StatusCode;
use retinue_protocol::PROTOCOL_VERSION;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Mutex as StdMutex;
use std::time::Duration;

/// The file a headless daemon writes so the console client can reach it.
const CONSOLE_FILE: &str = "console.json";

#[derive(Default)]
pub struct Console {
    inner: StdMutex<Option<Live>>,
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

    /// The tests' own headless start: the token without a file, so no test writes
    /// into the owner's data folder.
    #[cfg(test)]
    pub fn start_in_memory(&self, token: &str) {
        *self.inner.lock().unwrap() = Some(Live { token: token.to_string(), file: PathBuf::from("no-file") });
    }
}

/// Called once at startup, after the listener has an address. A desktop-launched
/// daemon does nothing here.
pub fn startup(st: &AppState) {
    if !paths::headless() {
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
        _ => Err(derr("unsupported", "method is not supported", StatusCode::NOT_FOUND)),
    }
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

/// What `retinued pair …` and `retinued remote …` print and return. Fixed text
/// only: nothing here reads a secret out of the file.
pub async fn run_cli(args: &[String]) -> i32 {
    let command = args.first().map(String::as_str).unwrap_or("");
    let rest = &args[1..];
    match command {
        "status" => status_command().await,
        "pair" => pair_command(rest).await,
        "remote" => remote_command(rest).await,
        other => {
            eprintln!("Unknown command: {other}");
            usage();
            2
        }
    }
}

fn usage() {
    eprintln!(
        "usage:\n  retinued pair                     print a code and wait for a device\n  retinued pair list                show the waiting request\n  retinued pair confirm --tier T    approve it (viewer, chat or operator)\n  retinued pair deny                refuse it\n  retinued pair cancel              throw the open code away\n  retinued remote on [--method M]   turn the gateway on (tailscale or native)\n  retinued remote off               turn it off\n  retinued status                   show this host's version, folders and Remote state"
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
