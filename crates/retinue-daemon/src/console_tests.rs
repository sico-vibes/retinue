// 17 November 2026 (multi-device piece 8): the console of a host with no window.
//
// Every method is called through `main_rpc`, which is where the token is checked
// and the route is taken: calling `console::route` alone would prove the body but
// not that the app can reach it (wave 2's `task.checkBack` lesson).

use axum::body::Bytes;
use axum::extract::{ConnectInfo, State};
use axum::http::{header, HeaderMap};
use axum::response::IntoResponse;
use axum::Json;
use serde_json::json;

/// One console call through the real router, returning the status and the body.
async fn console_post(st: &AppState, bearer: Option<&str>, method: &str, params: Value) -> (u16, Value) {
    let mut headers = HeaderMap::new();
    if let Some(bearer) = bearer {
        headers.insert(header::AUTHORIZATION, format!("Bearer {bearer}").parse().unwrap());
    }
    let request = RpcRequest { v: 1, id: "console-test".into(), method: method.into(), params };
    let reply = crate::main_rpc(State(st.clone()), ConnectInfo("127.0.0.1:5555".parse().unwrap()), headers, Json(request)).await.into_response();
    let status = reply.status().as_u16();
    let body = axum::body::to_bytes(reply.into_body(), 1 << 20).await.unwrap_or_else(|_| Bytes::new());
    (status, serde_json::from_slice(&body).unwrap_or(Value::Null))
}

/// A state whose console is live, as a headless start makes it. No file is
/// written: the daemon's own `console::startup` is exercised by the live host.
fn console_state() -> (AppState, String) {
    let (st, _) = state();
    let token = "console-token-for-tests".to_string();
    st.console.start_in_memory(&token);
    (st, token)
}

#[tokio::test]
async fn console_methods_need_the_console_token_and_never_the_capability() {
    let (st, token) = console_state();
    assert!(st.console.is_live());
    assert_eq!(console_post(&st, None, "console.status", json!({})).await.0, 401, "no token");
    assert_eq!(console_post(&st, Some("not-the-token"), "console.status", json!({})).await.0, 401, "a wrong token");
    assert_eq!(console_post(&st, Some(st.token.as_str()), "console.status", json!({})).await.0, 401, "the daemon capability is not the console token");
    assert_eq!(console_post(&st, Some(&token), "console.status", json!({})).await.0, 200, "the console token");
    assert_eq!(console_post(&st, Some(&token), "console.nope", json!({})).await.0, 404, "only the console methods");
    // A desktop-launched daemon has no console at all: the same call is refused.
    let (plain, _) = state();
    assert!(!plain.console.is_live());
    assert_eq!(console_post(&plain, Some(&token), "console.status", json!({})).await.0, 401, "nothing accepts a console token without a headless start");
}

#[tokio::test]
async fn console_status_reports_the_folder_the_store_and_the_gateway_without_a_secret() {
    let (st, token) = console_state();
    let dir = std::env::temp_dir().join(format!("retinue-console-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    *st.team.data_dir.lock().unwrap() = Some(dir.clone());
    let (status, body) = console_post(&st, Some(&token), "console.status", json!({})).await;
    assert_eq!(status, 200);
    let result = &body["result"];
    assert_eq!(result["dataFolder"].as_str().unwrap(), dir.display().to_string());
    assert!(result["database"].as_str().unwrap().ends_with("retinue.db"));
    assert_eq!(result["secrets"]["backend"].as_str().unwrap(), crate::secrets::store::backend_name());
    assert!(result["secrets"]["folder"].as_str().unwrap().ends_with("secrets"));
    assert_eq!(result["remote"]["enabled"], json!(false), "Remote starts off");
    assert_eq!(result["platform"].as_str().unwrap(), std::env::consts::OS);
    assert!(result["version"].as_str().is_some());
    // Nothing in the reply names a secret or the capability.
    let text = body.to_string();
    assert!(!text.contains(st.token.as_str()), "the daemon capability never reaches a console reply");
    assert!(!text.contains(&token), "the console token is not echoed back");
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn a_waiting_request_is_listed_and_confirmed_with_a_tier_from_the_console() {
    let (st, token) = console_state();
    // Nothing waiting yet.
    let (status, body) = console_post(&st, Some(&token), "console.pair.pending", json!({})).await;
    assert_eq!(status, 200);
    assert!(body["result"]["pending"].is_null());
    assert_eq!(
        console_post(&st, Some(&token), "console.pair.confirm", json!({"approve": true, "tier": "chat"})).await.0,
        409,
        "approving nothing is a conflict, not a silent success"
    );
    // A Native sign-in waiting for its tier, as the gateway makes one.
    st.remote.pairing.lock().unwrap().begin_native("LAPTOP-7", "machine", "linux", chrono::Utc::now());
    let (status, body) = console_post(&st, Some(&token), "console.pair.pending", json!({})).await;
    assert_eq!(status, 200);
    let pending = &body["result"]["pending"];
    assert_eq!(pending["name"], json!("LAPTOP-7"));
    assert_eq!(pending["kind"], json!("machine"));
    assert_eq!(pending["platform"], json!("linux"));
    assert_eq!(pending["method"], json!("native"));
    assert!(pending["pairingId"].as_str().is_some_and(|id| !id.is_empty()));
    // Approving without a tier is refused with the window's own sentence.
    let (status, body) = console_post(&st, Some(&token), "console.pair.confirm", json!({"approve": true})).await;
    assert_eq!(status, 400);
    assert!(body["error"]["message"].as_str().unwrap().contains("Choose what this device may do"));
    // With a tier it is approved, the device is stored, and the ledger says so.
    let (status, body) = console_post(&st, Some(&token), "console.pair.confirm", json!({"approve": true, "tier": "chat"})).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["result"]["approved"], json!(true));
    let device_id = body["result"]["device"]["id"].as_str().unwrap().to_owned();
    assert_eq!(body["result"]["device"]["tier"], json!("chat"));
    assert_eq!(body["result"]["device"]["platform"], json!("linux"), "the request keeps the platform it reported");
    let stored = st.db.remote_device_get(&device_id).unwrap().expect("the device is stored");
    assert_eq!(stored["tier"], json!("chat"));
    let ledger = st.db.remote_activity_list(20).unwrap();
    assert!(ledger.iter().any(|entry| entry["kind"] == json!("paired")), "the approval is in the activity list");
    // The offer is spent: a second approval is refused, and nothing is waiting.
    assert_eq!(console_post(&st, Some(&token), "console.pair.confirm", json!({"approve": true, "tier": "chat"})).await.0, 409);
    let (_, body) = console_post(&st, Some(&token), "console.pair.pending", json!({})).await;
    assert!(body["result"]["pending"].is_null(), "a decided request is not offered again");
}

#[tokio::test]
async fn the_console_refuses_denies_and_cancels_with_the_window_s_own_sentences() {
    let (st, token) = console_state();
    st.remote.pairing.lock().unwrap().begin_native("PHONE", "phone", "android", chrono::Utc::now());
    let (status, body) = console_post(&st, Some(&token), "console.pair.deny", json!({})).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["result"]["approved"], json!(false));
    assert_eq!(st.db.remote_devices().unwrap().len(), 0, "a refusal stores nothing");
    // Cancelling after a decision is what the window's own `remote.pair.cancel`
    // answers (the offer object is still there, so it says it cancelled), and the
    // console does not invent a third answer. "Nothing to cancel" is a state with
    // no offer at all, below.
    let (status, body) = console_post(&st, Some(&token), "console.pair.cancel", json!({})).await;
    assert_eq!(status, 200);
    assert_eq!(body["result"]["cancelled"], json!(true), "the decided offer is thrown away the same way the window does it");
    let (fresh, fresh_token) = console_state();
    let (status, body) = console_post(&fresh, Some(&fresh_token), "console.pair.cancel", json!({})).await;
    assert_eq!(status, 200);
    assert_eq!(body["result"]["cancelled"], json!(false), "with no offer at all there is nothing to cancel");
    // Pairing a code needs Remote on, and the sentence is the window's.
    let (status, body) = console_post(&st, Some(&token), "console.pair.begin", json!({})).await;
    assert_eq!(status, 409);
    assert!(body["error"]["message"].as_str().unwrap().contains("Turn Remote on before pairing a device"));
    // Turning Remote on is a console decision too, and a bad method is refused.
    let (status, _) = console_post(&st, Some(&token), "console.remote.enable", json!({"method": "carrier-pigeon"})).await;
    assert_eq!(status, 400);
}

#[tokio::test]
async fn a_live_offer_expires_out_of_the_console_list() {
    // The console shows only what can still be approved: an expired request is
    // not listed, because approving it would only be refused.
    let mut pairing = crate::remote::pairing::PairingState::default();
    let past = chrono::Utc::now() - chrono::Duration::minutes(10);
    pairing.begin_native("LATE", "machine", "linux", past);
    assert!(pairing.live_pending().is_none(), "an expired request is not offered");
    pairing.begin_native("NOW", "machine", "linux", chrono::Utc::now());
    assert_eq!(pairing.live_pending().unwrap()["name"], json!("NOW"));
}

// ---------------------------------------------------------------------------
// 19 November 2026: setting the host up from its own terminal — the two settings
// a paired device needs on this computer, and the bots the host cannot make
// without a window.

/// The five methods that change or read something a window was needed for, with
/// the params each one can be called with. None of them is content-bearing.
fn setting_up_calls() -> Vec<(&'static str, Value)> {
    vec![
        ("console.settings.get", json!({})),
        ("console.settings.set", json!({"key": "projects.devices.enabled", "value": true})),
        ("console.agents", json!({})),
        ("console.bot.list", json!({})),
        ("console.bot.create", json!({"name": "Rover", "agent": "codex"})),
    ]
}

#[tokio::test]
async fn setting_the_host_up_needs_the_console_token_and_never_the_capability() {
    let (st, token) = console_state();
    for (method, params) in setting_up_calls() {
        assert_eq!(console_post(&st, None, method, params.clone()).await.0, 401, "{method} without a token");
        assert_eq!(console_post(&st, Some("not-the-token"), method, params.clone()).await.0, 401, "{method} with a wrong token");
        assert_eq!(console_post(&st, Some(st.token.as_str()), method, params.clone()).await.0, 401, "{method}: the daemon capability is not the console token");
    }
    // A desktop-launched daemon writes no console file, so even the right token is refused.
    let (plain, _) = state();
    for (method, params) in setting_up_calls() {
        assert_eq!(console_post(&plain, Some(&token), method, params).await.0, 401, "{method} on a daemon with a window");
    }
}

#[tokio::test]
async fn the_console_changes_only_the_two_settings_it_lists_and_never_a_secret() {
    let (st, token) = console_state();
    // A key outside the allowlist is refused on the way in and on the way out, and the
    // sentence names both keys the console may touch.
    for method in ["console.settings.get", "console.settings.set"] {
        for key in ["appearance.style", "vault.credential", "remote.enabled", "profile.name"] {
            let (status, body) = console_post(&st, Some(&token), method, json!({"key": key, "value": "stone"})).await;
            assert_eq!(status, 400, "{method} {key}: {body}");
            let message = body["error"]["message"].as_str().unwrap();
            assert!(
                message.contains("projects.devices.enabled") && message.contains("projects.devices.folder"),
                "the refusal names the allowlist: {message}"
            );
            assert!(!message.contains(key), "the refusal names no value and repeats no key: {message}");
        }
    }
    // The switch, through the desktop's own validation, so a bad value gets its sentence.
    let (status, body) = console_post(&st, Some(&token), "console.settings.set", json!({"key": "projects.devices.enabled", "value": "yes"})).await;
    assert_eq!(status, 400);
    assert_eq!(body["error"]["message"], "projects.devices.enabled requires a JSON boolean");
    let (status, body) = console_post(&st, Some(&token), "console.settings.set", json!({"key": "projects.devices.enabled", "value": true})).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["result"], json!({"saved": true, "key": "projects.devices.enabled", "value": true}));
    assert_eq!(dispatch(&st, "settings.get", json!({})).await.unwrap()["projects.devices.enabled"], true);
    // The folder: it must be a folder on this computer, and what is stored is its canonical path.
    let base = std::env::temp_dir().join(format!("retinue-console-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(base.join("work")).unwrap();
    let file = base.join("notes.txt");
    std::fs::write(&file, "not a folder").unwrap();
    for refused in [file.display().to_string(), base.join("gone").display().to_string()] {
        let (status, body) = console_post(&st, Some(&token), "console.settings.set", json!({"key": "projects.devices.folder", "value": refused})).await;
        assert_eq!(status, 400, "{refused}: {body}");
        assert_eq!(body["error"]["message"], "That folder is not on this computer.");
    }
    let (status, body) = console_post(&st, Some(&token), "console.settings.set", json!({"key": "projects.devices.folder", "value": base.join("work").display().to_string()})).await;
    assert_eq!(status, 200, "{body}");
    let stored = body["result"]["value"].as_str().unwrap().to_string();
    assert_eq!(stored, crate::project_share::display(&std::fs::canonicalize(base.join("work")).unwrap()), "the stored folder is canonical");
    // Reading returns the allowlist and nothing else, and no reply carries a credential.
    let (status, body) = console_post(&st, Some(&token), "console.settings.get", json!({})).await;
    assert_eq!(status, 200);
    assert_eq!(body["result"]["allowlist"], json!(["projects.devices.enabled", "projects.devices.folder"]));
    assert_eq!(body["result"]["settings"]["projects.devices.folder"], json!(stored));
    let text = body.to_string();
    assert!(!text.contains(st.token.as_str()) && !text.contains(&token), "no token in a console reply");
    // Each change is one content-free line in this host's own activity list: no path, no value.
    let ledger = st.db.remote_activity_list(20).unwrap();
    let lines = ledger.iter().filter(|entry| entry["kind"] == json!("console")).collect::<Vec<_>>();
    assert_eq!(lines.len(), 2, "one line per change: {ledger:?}");
    assert!(lines.iter().any(|entry| entry["summary"].as_str().unwrap().contains("turned on")));
    assert!(lines.iter().any(|entry| entry["summary"].as_str().unwrap().contains("folder")));
    assert!(
        !ledger.iter().any(|entry| entry["summary"].as_str().is_some_and(|summary| summary.contains(&base.display().to_string()) || summary.contains("true"))),
        "the activity line carries no path and no value: {ledger:?}"
    );
    let _ = std::fs::remove_dir_all(&base);
}

#[tokio::test]
async fn a_bot_created_from_the_console_is_an_ordinary_bot_that_asks_first() {
    let (st, token) = console_state();
    // The catalog is the desktop's too: the model and the effort are checked against it.
    let runtime = st.runtimes.read().await[0].clone();
    crate::catalog::seed(&st, &runtime, json!({
        "runtimeId":"rt-test","provider":"codex","fallback":false,"validated":true,
        "models":[{"id":"beta","supportedThinking":["high"],"defaultThinking":"high","serviceTiers":[]}]
    }));
    let (status, body) = console_post(&st, Some(&token), "console.bot.create", json!({"name":"Runner","agent":"codex","model":"beta","effort":"high"})).await;
    assert_eq!(status, 200, "{body}");
    let created = body["result"]["agent"].clone();
    let id = created["id"].as_str().unwrap().to_owned();
    assert_eq!(created["name"], "Runner");
    assert_eq!(created["runtimeId"], "rt-test");
    assert_eq!(created["provider"], "codex");
    assert_eq!(created["model"], "beta");
    assert_eq!(created["thinking"], "high");
    assert_eq!(created["approvalMode"], "ask", "a terminal has no card to approve Full access with");
    // An effort the catalog does not offer is normalised to the model's default, exactly as the
    // desktop's form normalises it.
    let (status, body) = console_post(&st, Some(&token), "console.bot.create", json!({"name":"Loud","agent":"codex","model":"beta","effort":"low"})).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["result"]["agent"]["thinking"], "high");
    // The desktop's own name rule still runs: a taken name is a conflict, not a second bot.
    let (status, body) = console_post(&st, Some(&token), "console.bot.create", json!({"name":"Runner","agent":"codex","model":"beta"})).await;
    assert_eq!(status, 409, "{body}");
    // A coding agent the discovery did not find is refused before anything is written.
    let (status, body) = console_post(&st, Some(&token), "console.bot.create", json!({"name":"Ghost","agent":"cursor"})).await;
    assert_eq!(status, 400);
    assert!(body["error"]["message"].as_str().unwrap().contains("not installed on this computer"), "{body}");
    assert!(!dispatch(&st, "agent.list", json!({})).await.unwrap()["agents"].as_array().unwrap().iter().any(|agent| agent["name"] == json!("Ghost")));
    // `--project`: a folder with no project row is registered the desktop's way, and the
    // bot is placed in it through `agent.create`'s own team placement.
    let folder = std::env::temp_dir().join(format!("retinue-console-project-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&folder).unwrap();
    let (status, body) = console_post(&st, Some(&token), "console.bot.create", json!({"name":"Placed","agent":"rt-test","model":"beta","project": folder.display().to_string()})).await;
    assert_eq!(status, 200, "{body}");
    let project_id = body["result"]["agent"]["projectId"].as_str().unwrap().to_owned();
    let projects = dispatch(&st, "project.list", json!({})).await.unwrap();
    let row = projects["projects"].as_array().unwrap().iter().find(|project| project["id"] == json!(project_id)).cloned().unwrap();
    assert_eq!(std::fs::canonicalize(row["path"].as_str().unwrap()).unwrap(), std::fs::canonicalize(&folder).unwrap());
    // The console's bot is an ordinary row: the same shape as one the desktop makes.
    let desktop = dispatch(&st, "agent.create", json!({"name":"Desk","runtimeId":"rt-test","model":"beta"})).await.unwrap()["agent"].clone();
    assert_eq!(desktop["approvalMode"], "bypass", "the window's own default is unchanged");
    let listed = dispatch(&st, "agent.list", json!({})).await.unwrap();
    let rows = listed["agents"].as_array().unwrap();
    let console_row = rows.iter().find(|agent| agent["id"] == json!(id)).cloned().expect("the console's bot is listed");
    let keys = |row: &Value| { let mut keys = row.as_object().unwrap().keys().cloned().collect::<Vec<_>>(); keys.sort(); keys };
    assert_eq!(keys(&console_row), keys(&desktop), "the same row shape, whoever made it");
    assert_eq!(dispatch(&st, "agent.get", json!({"agentId": id})).await.unwrap()["agent"]["id"], json!(id));
    // And `console.bot.list` names it with the coding agent it runs on.
    let (status, body) = console_post(&st, Some(&token), "console.bot.list", json!({})).await;
    assert_eq!(status, 200);
    let bots = body["result"]["bots"].as_array().unwrap();
    let mine = bots.iter().find(|bot| bot["id"] == json!(id)).unwrap();
    assert_eq!(mine["provider"], "codex");
    assert_eq!(mine["model"], "beta");
    assert!(!mine.as_object().unwrap().contains_key("instructions"), "a console reply carries no instructions");
    let _ = std::fs::remove_dir_all(&folder);
}

#[tokio::test]
async fn agents_lists_what_is_installed_and_never_a_credential() {
    let (st, token) = console_state();
    let (status, body) = console_post(&st, Some(&token), "console.agents", json!({})).await;
    assert_eq!(status, 200, "{body}");
    let rows = body["result"]["agents"].as_array().unwrap();
    let codex = rows.iter().find(|row| row["provider"] == json!("codex")).expect("the discovery's own codex row");
    assert_eq!(codex["installed"], true);
    assert_eq!(codex["signedIn"], false, "an offline row is not a signed-in one");
    assert_eq!(codex["runtimeIds"], json!(["rt-test"]));
    // The row says how to sign in, even where the discovery has no table command of
    // its own (Codex is one of the three main providers); it never carries the
    // executable path or a token.
    assert_eq!(codex["signInCommand"], "codex login");
    let text = body.to_string();
    assert!(!text.contains(st.token.as_str()) && !text.contains(&token));
    assert!(!text.to_ascii_lowercase().contains("executablepath"), "the launcher's own path stays out: {text}");
    // A provider the discovery reports signed in says so.
    *st.runtimes.write().await = vec![json!({"id":"rt-test","provider":"codex","status":"online","authState":"authenticated"})];
    let (_, body) = console_post(&st, Some(&token), "console.agents", json!({})).await;
    let codex = body["result"]["agents"].as_array().unwrap().iter().find(|row| row["provider"] == json!("codex")).cloned().unwrap();
    assert_eq!(codex["signedIn"], true);
}

#[tokio::test]
async fn a_host_with_no_window_is_told_the_console_command_and_a_desktop_is_not() {
    // The daemon records at startup whether a desktop shell started it: a plain state
    // is a window's daemon, `console_state` is the headless one.
    let (desktop, _) = state();
    assert!(!desktop.console.headless_host());
    let (status, body) = console_post(&desktop, Some(desktop.token.as_str()), "project.browse", json!({})).await;
    assert_eq!(status, 403, "{body}");
    assert_eq!(body["error"]["message"], "Projects from other machines are off on this computer. Turn them on in Settings › Machines.");
    dispatch(&desktop, "settings.set", json!({"key":"projects.devices.enabled","value":true})).await.unwrap();
    let (_, body) = console_post(&desktop, Some(desktop.token.as_str()), "project.browse", json!({})).await;
    assert_eq!(body["error"]["message"], "Choose the folder other machines may use for new projects in Settings › Machines first.");
    // The same two refusals on a host with no window name the command that works there.
    let (headless, _token) = console_state();
    assert!(headless.console.headless_host());
    let (status, body) = console_post(&headless, Some(headless.token.as_str()), "project.browse", json!({})).await;
    assert_eq!(status, 403, "{body}");
    assert_eq!(body["error"]["message"], "Projects from other machines are off on this computer. Turn them on at that computer with `retinued settings set projects.devices.enabled true`.");
    dispatch(&headless, "settings.set", json!({"key":"projects.devices.enabled","value":true})).await.unwrap();
    let (_, body) = console_post(&headless, Some(headless.token.as_str()), "project.browse", json!({})).await;
    assert_eq!(body["error"]["message"], "Choose the folder other machines may use for new projects first: `retinued settings set projects.devices.folder <folder>`.");
}

