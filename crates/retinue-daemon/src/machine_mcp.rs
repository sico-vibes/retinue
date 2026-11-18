//! 18 November 2026: the Retinue MCP's multi-device tools. Every one is a thin
//! wrapper over a `machine.*` RPC through `dispatch`, so an outside session can
//! never reach more than the window can: the same link, the same device token,
//! the same tier checks and the same far-gateway redaction.
//!
//! The catalog entry (name, level, schema) lives in `external.rs` `catalog()`;
//! this module is the bodies. Levels: reading is Viewer, starting and driving
//! work is Chat, answering an approval and using another machine's projects or
//! devices is Operator. The link's own tier applies underneath, and the far
//! machine is the authority in the end.
//!
//! Nothing here writes a message's text to a log: `external.rs` records the
//! client, the tool and a bot's name, and nothing else. No reply carries a
//! device token, a cookie, a certificate fingerprint, a password, an
//! environment value, or a path outside the folder a far machine shares.

use crate::{dispatch, AppState, DispatchError};
use serde_json::{json, Value};
use std::time::{Duration, Instant};

/// The longest `wait_on_machine` waits.
const MAX_WAIT_SECS: u64 = 600;
/// How often `wait_on_machine` reads the far conversation when no change arrives.
const WAIT_STEP: Duration = Duration::from_secs(2);
/// The least gap between two far reads, even when changes arrive in a burst.
const WAIT_FLOOR: Duration = Duration::from_millis(500);
/// The most items one page may hold, as `read_conversation`.
const MAX_PAGE: usize = 200;
/// The most bytes one page may hold, as `read_conversation`.
const READ_MAX_BYTES: usize = 128 * 1024;
/// The most text one wait reply carries.
const MAX_REPLY_CHARS: usize = 60_000;

fn fail(error: DispatchError) -> String {
    error.1.message
}

fn text_arg<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(Value::as_str).map(str::trim).filter(|text| !text.is_empty())
}

fn require(args: &Value, key: &str) -> Result<String, String> {
    text_arg(args, key).map(str::to_owned).ok_or_else(|| format!("{key} is required."))
}

/// The link row `machine.list` gives, so a reply can name the machine and say
/// whether it is current.
async fn link_row(st: &AppState, machine: &str) -> Result<Value, String> {
    let listed = dispatch(st, "machine.list", json!({})).await.map_err(fail)?;
    listed["machines"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .find(|row| row["id"].as_str() == Some(machine))
        .ok_or_else(|| "That machine isn't linked.".to_owned())
}

/// A permission card as these tools show it: the card's own words, capped, and
/// nothing of the provider's payload or the remembered rule.
fn permission_view(card: &Value) -> Value {
    json!({
        "id": card["id"],
        "sessionId": card["sessionId"],
        "kind": card["kind"],
        "title": card["title"].as_str().map(|title| crate::external::cap(title, 300)),
        "detail": card["detail"].as_str().map(|detail| crate::external::cap(detail, 2_000)),
        "choices": card["choices"],
    })
}

/// The newest assistant message's text, for `wait_on_machine`.
fn last_reply(detail: &Value) -> Option<String> {
    detail["messages"]
        .as_array()?
        .iter()
        .rev()
        .filter(|message| message["role"] == "assistant")
        .find_map(|message| message["content"].as_str().map(str::trim).filter(|text| !text.is_empty()).map(str::to_owned))
}

// ---------------------------------------------------------------------------
// Viewer: the links and their conversations

pub(crate) async fn list_machines(st: &AppState, _args: &Value) -> Result<Value, String> {
    let listed = dispatch(st, "machine.list", json!({})).await.map_err(fail)?;
    let mut machines = Vec::new();
    for row in listed["machines"].as_array().cloned().unwrap_or_default() {
        let Some(id) = row["id"].as_str() else { continue };
        // Memory only: it never calls the far machine, so an offline link answers too.
        let stream = dispatch(st, "machine.stream.status", json!({"machineId": id}))
            .await
            .unwrap_or_else(|_| json!({"connected": false, "lastSequence": 0}));
        let bots: Vec<Value> = row["bots"]
            .as_array()
            .map(|bots| bots.iter().map(|bot| json!({"id": bot["id"], "name": bot["name"]})).collect())
            .unwrap_or_default();
        // The address, the certificate fingerprint and how this computer signed in stay here.
        machines.push(json!({
            "id": id,
            "name": row["name"],
            "state": row["state"],
            "detail": row["detail"],
            "kind": row["kind"],
            "live": row["live"],
            "version": row["version"],
            "tier": row["tier"],
            "stream": {"connected": stream["connected"], "lastSequence": stream["lastSequence"]},
            "bots": bots,
        }));
    }
    Ok(json!({"machines": machines}))
}

pub(crate) async fn machine_conversations(st: &AppState, args: &Value) -> Result<Value, String> {
    let machine = require(args, "machineId")?;
    let row = link_row(st, &machine).await?;
    let listed = dispatch(st, "machine.sessions", json!({"machineId": machine})).await.map_err(fail)?;
    let state = row["state"].clone();
    Ok(json!({
        "machineId": machine,
        "machineName": row["name"].clone(),
        "state": state,
        "stale": state.as_str() != Some("online"),
        "conversations": listed["sessions"].clone(),
    }))
}

pub(crate) async fn read_machine_conversation(st: &AppState, args: &Value) -> Result<Value, String> {
    let machine = require(args, "machineId")?;
    let session = require(args, "sessionId")?;
    let read = dispatch(st, "machine.conversation", json!({"machineId": machine, "sessionId": session})).await.map_err(fail)?;
    let detail = &read["session"];
    // A machine link keeps the far work log (title, command, output); a phone's does not.
    let items = crate::external::conversation_items(detail, true);
    let limit = args.get("limit").and_then(Value::as_u64).map_or(50, |value| value as usize).clamp(1, MAX_PAGE);
    let mut page = crate::external::conversation_page(items, None, text_arg(args, "beforeMessageId"), limit, READ_MAX_BYTES)?;
    page["machineId"] = json!(machine);
    page["sessionId"] = json!(session);
    page["state"] = detail["state"].clone();
    page["title"] = detail["title"].clone();
    Ok(page)
}

/// Re-subscribed before the first read, so a change that lands during it is kept.
async fn wait_for_change(events: &mut tokio::sync::broadcast::Receiver<retinue_protocol::EventEnvelope>, machine: &str, session: &str) {
    loop {
        match events.recv().await {
            Ok(event)
                if event.event_type == "machine.session.changed"
                    && event.payload["machineId"].as_str() == Some(machine)
                    && event.payload["sessionId"].as_str() == Some(session) =>
            {
                return
            }
            Ok(_) => {}
            // A burst outran the channel: read again after a moment rather than spin.
            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => tokio::time::sleep(Duration::from_millis(100)).await,
            Err(tokio::sync::broadcast::error::RecvError::Closed) => tokio::time::sleep(Duration::from_secs(1)).await,
        }
    }
}

pub(crate) async fn wait_on_machine(st: &AppState, args: &Value) -> Result<Value, String> {
    let machine = require(args, "machineId")?;
    let session = require(args, "sessionId")?;
    let secs = args.get("timeoutSecs").and_then(Value::as_u64).unwrap_or(120).clamp(1, MAX_WAIT_SECS);
    let deadline = Instant::now() + Duration::from_secs(secs);
    let mut events = st.events.subscribe();
    loop {
        // One far read: the conversation as its own gateway answers it, redacted there.
        let read = dispatch(st, "machine.conversation", json!({"machineId": machine, "sessionId": session})).await.map_err(fail)?;
        let detail = read["session"].clone();
        let state = detail["state"].as_str().unwrap_or("").to_owned();
        if !crate::external::is_busy_state(&state) {
            let mut done = json!({"machineId": machine, "sessionId": session, "done": true, "state": state});
            if let Some(reply) = last_reply(&detail) {
                done["reply"] = json!(crate::external::cap(&reply, MAX_REPLY_CHARS));
            }
            if let Some(error) = detail["turns"].as_array().and_then(|turns| turns.last()).and_then(crate::external::turn_error) {
                done["error"] = error;
            }
            return Ok(done);
        }
        // A waiting card would hold the turn until it is answered: say so rather than time out.
        if state == "waiting_permission" {
            let cards = dispatch(st, "machine.permissions", json!({"machineId": machine, "sessionId": session})).await;
            let waiting: Vec<Value> = cards
                .map(|cards| cards["permissions"].as_array().cloned().unwrap_or_default().iter().map(permission_view).collect())
                .unwrap_or_default();
            if !waiting.is_empty() {
                return Ok(json!({
                    "machineId": machine, "sessionId": session, "done": false, "state": state,
                    "approvalPending": true, "approvals": waiting,
                }));
            }
        }
        let now = Instant::now();
        if now >= deadline {
            return Ok(json!({"machineId": machine, "sessionId": session, "done": false, "state": state, "timedOut": true}));
        }
        let step = WAIT_STEP.min(deadline.saturating_duration_since(now));
        tokio::select! {
            _ = tokio::time::sleep(step) => {}
            _ = wait_for_change(&mut events, &machine, &session) => { tokio::time::sleep(WAIT_FLOOR).await; }
        }
    }
}

pub(crate) async fn machine_approvals(st: &AppState, args: &Value) -> Result<Value, String> {
    let machine = require(args, "machineId")?;
    let mut params = json!({"machineId": machine});
    if let Some(session) = text_arg(args, "sessionId") {
        params["sessionId"] = json!(session);
    }
    let far = dispatch(st, "machine.permissions", params).await.map_err(fail)?;
    let permissions: Vec<Value> = far["permissions"].as_array().cloned().unwrap_or_default().iter().map(permission_view).collect();
    Ok(json!({"machineId": machine, "permissions": permissions}))
}

pub(crate) async fn machine_folders(st: &AppState, args: &Value) -> Result<Value, String> {
    let machine = require(args, "machineId")?;
    let mut params = json!({"machineId": machine});
    if let Some(path) = text_arg(args, "path") {
        params["path"] = json!(path);
    }
    dispatch(st, "machine.project.browse", params).await.map_err(fail)
}

pub(crate) async fn machine_clone_status(st: &AppState, args: &Value) -> Result<Value, String> {
    let machine = require(args, "machineId")?;
    let job = require(args, "jobId")?;
    dispatch(st, "machine.clone.status", json!({"machineId": machine, "jobId": job})).await.map_err(fail)
}

// ---------------------------------------------------------------------------
// Chat: starting and driving work there

pub(crate) async fn start_on_machine(st: &AppState, args: &Value) -> Result<Value, String> {
    let machine = require(args, "machineId")?;
    let bot = require(args, "botId")?;
    let text = crate::external::valid_text(args)?;
    let mut params = json!({"machineId": machine, "agentId": bot});
    if let Some(project) = text_arg(args, "projectId") {
        params["projectId"] = json!(project);
    }
    let started = dispatch(st, "machine.start", params).await.map_err(fail)?;
    let session = started["sessionId"].as_str().unwrap_or_default().to_owned();
    match dispatch(st, "machine.prompt", json!({"machineId": machine, "sessionId": session, "text": text})).await {
        Ok(answer) => Ok(json!({"machineId": machine, "sessionId": session, "started": true, "message": answer})),
        Err(error) => Err(format!("The conversation was started on that machine, but the message did not reach it: {}", error.1.message)),
    }
}

pub(crate) async fn message_on_machine(st: &AppState, args: &Value) -> Result<Value, String> {
    let machine = require(args, "machineId")?;
    let session = require(args, "sessionId")?;
    let text = crate::external::valid_text(args)?;
    let answer = dispatch(st, "machine.prompt", json!({"machineId": machine, "sessionId": session, "text": text})).await.map_err(fail)?;
    Ok(json!({"machineId": machine, "sessionId": session, "result": answer}))
}

pub(crate) async fn steer_on_machine(st: &AppState, args: &Value) -> Result<Value, String> {
    let machine = require(args, "machineId")?;
    let session = require(args, "sessionId")?;
    let text = crate::external::valid_text(args)?;
    let answer = dispatch(st, "machine.steer", json!({"machineId": machine, "sessionId": session, "text": text})).await.map_err(fail)?;
    Ok(json!({"machineId": machine, "sessionId": session, "result": answer}))
}

pub(crate) async fn stop_on_machine(st: &AppState, args: &Value) -> Result<Value, String> {
    let machine = require(args, "machineId")?;
    let session = require(args, "sessionId")?;
    let answer = dispatch(st, "machine.cancel", json!({"machineId": machine, "sessionId": session})).await.map_err(fail)?;
    Ok(json!({"machineId": machine, "sessionId": session, "result": answer}))
}

// ---------------------------------------------------------------------------
// Operator: answers, projects, devices

pub(crate) async fn answer_machine_approval(st: &AppState, args: &Value) -> Result<Value, String> {
    let machine = require(args, "machineId")?;
    let permission = require(args, "permissionId")?;
    let choice = require(args, "choice")?;
    if !matches!(choice.as_str(), "allow_once" | "deny") {
        return Err("choice must be allow_once or deny.".to_owned());
    }
    // The card's own word: `allow_once` becomes the first allow word it offers, as
    // this machine's own `answer_approval` does for a local card.
    let wanted: &[&str] = if choice == "allow_once" {
        &["allow_once", "allow", "accept", "yes"]
    } else {
        &["deny", "reject", "decline", "no", "deny_once", "deny_always"]
    };
    let pending = dispatch(st, "machine.permissions", json!({"machineId": machine})).await.map_err(fail)?;
    let card = pending["permissions"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .find(|card| card["id"].as_str() == Some(permission.as_str()));
    let choices: Vec<&str> = card.as_ref().and_then(|card| card["choices"].as_array()).map(|choices| choices.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
    let answer = wanted.iter().find(|word| choices.contains(*word)).copied().unwrap_or(choice.as_str());
    let far = dispatch(st, "machine.permission.reply", json!({"machineId": machine, "permissionId": permission, "choice": answer})).await.map_err(fail)?;
    Ok(json!({"machineId": machine, "permissionId": permission, "choice": choice, "answeredWith": answer, "result": far}))
}

pub(crate) async fn machine_add_project(st: &AppState, args: &Value) -> Result<Value, String> {
    let machine = require(args, "machineId")?;
    let path = require(args, "path")?;
    let mut params = json!({"machineId": machine, "path": path});
    if let Some(name) = text_arg(args, "name") {
        params["name"] = json!(name);
    }
    dispatch(st, "machine.project.create", params).await.map_err(fail)
}

pub(crate) async fn machine_clone(st: &AppState, args: &Value) -> Result<Value, String> {
    let machine = require(args, "machineId")?;
    let url = require(args, "url")?;
    // Without a folder, clone into the far machine's own shared folder, which is
    // the only one a device may write in anyway.
    let parent = match text_arg(args, "folder") {
        Some(folder) => folder.to_owned(),
        None => {
            let browsed = dispatch(st, "machine.project.browse", json!({"machineId": machine})).await.map_err(fail)?;
            browsed["base"].as_str().filter(|base| !base.is_empty()).map(str::to_owned).ok_or_else(|| {
                "That machine did not say which folder it shares for new projects.".to_owned()
            })?
        }
    };
    // `confirm` is the Operator level's own grant for this tool; the far side still
    // confines the folder (the gateway stamps the call as a device's).
    let mut params = json!({"machineId": machine, "url": url, "parent": parent, "confirm": true});
    if let Some(name) = text_arg(args, "name") {
        params["name"] = json!(name);
    }
    dispatch(st, "machine.project.clone", params).await.map_err(fail)
}

pub(crate) async fn machine_devices(st: &AppState, args: &Value) -> Result<Value, String> {
    let machine = require(args, "machineId")?;
    // Reads only. Approving, renaming and revoking a device stay in the app.
    let devices = dispatch(st, "machine.devices.list", json!({"machineId": machine})).await.map_err(fail)?;
    let pending = dispatch(st, "machine.pair.pending", json!({"machineId": machine})).await.map_err(fail)?;
    Ok(json!({"machineId": machine, "devices": devices["devices"], "pending": pending["pending"]}))
}

pub(crate) async fn machine_remote_status(st: &AppState, args: &Value) -> Result<Value, String> {
    let machine = require(args, "machineId")?;
    let far = dispatch(st, "machine.remote.status", json!({"machineId": machine})).await.map_err(fail)?;
    // The far certificate fingerprint, its URLs and its `native` block stay there.
    Ok(json!({
        "machineId": machine,
        "enabled": far["enabled"],
        "method": far["method"],
        "port": far["port"],
        "hostName": far["hostName"],
        "deviceCount": far["deviceCount"],
        "connectedCount": far["connectedCount"],
    }))
}

// ---------------------------------------------------------------------------
// Diagnosis

/// One plain sentence, the one the Machines page would show for this link.
fn summary_of(name: &str, state: &str, connected: bool) -> String {
    match state {
        "revoked" => format!("{name} removed this computer's pairing. Pair it again."),
        "online" if connected => format!("{name} is linked and live."),
        "online" => format!("{name} answers but its live socket is not open."),
        _ => format!("{name} can't be reached right now."),
    }
}

pub(crate) async fn diagnose_machines(st: &AppState, _args: &Value) -> Result<Value, String> {
    let listed = dispatch(st, "machine.list", json!({})).await.map_err(fail)?;
    let mut reports = Vec::new();
    for row in listed["machines"].as_array().cloned().unwrap_or_default() {
        let Some(id) = row["id"].as_str().map(str::to_owned) else { continue };
        let state = row["state"].as_str().unwrap_or("").to_owned();
        let name = row["name"].as_str().unwrap_or("That machine").to_owned();
        // A ping carries no credential and no Origin; a revoked link is never addressed.
        let probed = match row["url"].as_str().filter(|url| !url.is_empty()) {
            Some(url) if state != "revoked" => dispatch(st, "machine.probe", json!({"url": url}))
                .await
                .unwrap_or_else(|_| json!({"reachable": false, "trusted": false, "isRetinue": false})),
            _ => json!({"reachable": false, "trusted": false, "isRetinue": false}),
        };
        let stream = dispatch(st, "machine.stream.status", json!({"machineId": id}))
            .await
            .unwrap_or_else(|_| json!({"connected": false, "lastSequence": 0}));
        // Reads only, and only while the link is up: each refusal is what the window would show.
        let mut probes = Vec::new();
        if state == "online" {
            for (what, method, counting) in [
                ("Browsing its folders", "machine.project.browse", "entries"),
                ("Listing the paired devices", "machine.devices.list", "devices"),
                ("Reading the pending approvals", "machine.permissions", "permissions"),
            ] {
                match dispatch(st, method, json!({"machineId": id})).await {
                    Ok(answer) => probes.push(json!({"what": what, "ok": true, "count": answer[counting].as_array().map(Vec::len)})),
                    Err(error) => probes.push(json!({"what": what, "ok": false, "code": error.1.code, "message": error.1.message})),
                }
            }
        }
        let connected = stream["connected"].as_bool().unwrap_or(false);
        reports.push(json!({
            "id": id,
            "name": name,
            "state": state,
            "detail": row["detail"],
            "kind": row["kind"],
            "version": row["version"],
            "tier": row["tier"],
            "reachable": probed["reachable"],
            "isRetinue": probed["isRetinue"],
            "certificateTrusted": probed["trusted"],
            "pinnedCertificate": row["fingerprint"].is_string(),
            "stream": {"connected": stream["connected"], "lastSequence": stream["lastSequence"]},
            "probes": probes,
            "summary": summary_of(&name, &state, connected),
        }));
    }
    Ok(json!({"machines": reports}))
}
