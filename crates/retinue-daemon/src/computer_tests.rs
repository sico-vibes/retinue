// The linked VPS computer (Harbor) and its employee tools. Included into
// `phase2a_tests` so it shares that module's fixtures: `state`, `dispatch`,
// `json!`, `Value`, `Arc` and `Uuid`. Test builds keep credentials in memory
// (`secrets::store`), and every link names its own credential, so tests that
// run in parallel never meet.

use std::sync::Mutex as TestMutex;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// A value no status, event or error text may ever contain.
const HARBOR_TOKEN: &str = "hrb_SECRET_SENTINEL_value";

/// A stand-in Harbor on loopback: one request per connection, answered from
/// `answer(method, path, body)`. Returns the address without a scheme.
async fn fake_harbor(answer: impl Fn(&str, &str, &Value) -> (u16, Value) + Send + Sync + 'static) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = format!("127.0.0.1:{}", listener.local_addr().unwrap().port());
    let answer = Arc::new(answer);
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else { return };
            let answer = answer.clone();
            tokio::spawn(async move {
                let mut data = Vec::new();
                let mut buffer = [0u8; 8192];
                loop {
                    let Ok(count) = stream.read(&mut buffer).await else { return };
                    if count == 0 { return; }
                    data.extend_from_slice(&buffer[..count]);
                    let text = String::from_utf8_lossy(&data).to_string();
                    let Some((head, body)) = text.split_once("\r\n\r\n") else { continue };
                    let length = head.lines().find_map(|line| line.to_ascii_lowercase().strip_prefix("content-length:").and_then(|value| value.trim().parse::<usize>().ok())).unwrap_or(0);
                    if body.len() < length { continue; }
                    let request_line = head.lines().next().unwrap_or("");
                    let method = request_line.split_whitespace().next().unwrap_or("").to_owned();
                    let path = request_line.split_whitespace().nth(1).unwrap_or("").to_owned();
                    let body: Value = serde_json::from_str(body).unwrap_or(Value::Null);
                    let (status, payload) = answer(&method, &path, &body);
                    let payload = payload.to_string();
                    let reply = format!("HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}", payload.len());
                    let _ = stream.write_all(reply.as_bytes()).await;
                    return;
                }
            });
        }
    });
    address
}

fn harbor_body(status: &str, desktop_url: Option<&str>) -> Value {
    json!({
        "computer": {
            "id": "c-1",
            "name": "harbor-bot-retinue-01",
            "kind": "webtop",
            "container": "harbor-bot-retinue-01",
            "status": status,
            "running": status == "running",
            "port": 3000,
            "since": "2026-11-13T08:00:00.000Z",
            "restarts": 2,
            "stats": {"cpu": "3.1%", "memory": "1.2GiB / 4GiB", "memoryPercent": "30%", "network": "1MB", "block": "2MB", "pids": 42},
            "disk_bytes": 1024,
        },
        "limits": {"cpu": "2", "memory": "4GiB", "pids": 1024},
        "desktop_url": desktop_url,
    })
}

/// A linked computer pointing at `address`, through the same two stores the RPC
/// uses. Returns the credential's name so a test can read the token back.
fn link_directly(st: &AppState, address: &str, token: &str, enabled: bool) -> String {
    let token_ref = format!("computer/test-{}", Uuid::new_v4());
    secrets::store::write(&token_ref, token).unwrap();
    st.db.computer_link_set(address, &token_ref).unwrap();
    st.db.computer_link_set_enabled(enabled).unwrap();
    token_ref
}

#[test]
fn computer_address_rules_allow_https_and_plain_http_only_on_loopback_and_tailnet() {
    for (raw, expected) in [
        ("https://harbor.example.com", "https://harbor.example.com"),
        ("https://harbor.example.com/", "https://harbor.example.com"),
        ("  https://harbor.example.com:8443/  ", "https://harbor.example.com:8443"),
        ("http://127.0.0.1:8080", "http://127.0.0.1:8080"),
        ("http://localhost:8080/", "http://localhost:8080"),
        ("http://100.64.1.2:8080", "http://100.64.1.2:8080"),
        ("http://bot.tail1234.ts.net", "http://bot.tail1234.ts.net"),
    ] {
        assert_eq!(computer::normalize_address(raw).unwrap(), expected, "{raw}");
    }
    for raw in [
        "http://harbor.example.com",
        "http://8.8.8.8",
        "http://192.168.1.10",
        "http://169.254.1.1",
        "ftp://harbor.example.com",
        "https://user:secret@harbor.example.com",
        "https://harbor.example.com/harbor/api/computer",
        "https://harbor.example.com?token=1",
        "https://harbor.example.com#fragment",
        "",
        "   ",
        "harbor.example.com",
        "not a url",
    ] {
        assert!(computer::normalize_address(raw).is_err(), "{raw} must be refused");
    }
}

#[test]
fn computer_status_maps_docker_states_and_drops_an_off_origin_desktop() {
    for (docker, state) in [("running", "awake"), ("exited", "asleep"), ("created", "asleep"), ("paused", "asleep"), ("missing", "missing"), ("restarting", "unknown"), ("", "unknown")] {
        let status = computer::map_status("https://harbor.example.com", true, &harbor_body(docker, Some("https://harbor.example.com/harbor/computer/view/")));
        assert_eq!(status["state"], state, "{docker}");
        assert_eq!(status["linked"], true);
        assert_eq!(status["reachable"], true);
        assert_eq!(status["cpuPercent"], 3.1);
        assert_eq!(status["memoryPercent"], 30.0);
        assert_eq!(status["memoryText"], "1.2GiB / 4GiB");
        assert_eq!(status["pids"], 42);
        assert_eq!(status["diskBytes"], 1024);
        assert_eq!(status["restarts"], 2);
        assert_eq!(status["limits"], json!({"cpu": "2", "memory": "4GiB"}));
        assert_eq!(status["name"], "harbor-bot-retinue-01");
        assert_eq!(status["problem"], Value::Null);
        assert_eq!(status["desktopUrl"], "https://harbor.example.com/harbor/computer/view/", "same origin is kept");
    }
    // Another host, another port and another scheme are all a different origin.
    for url in ["https://evil.example.com/view", "https://harbor.example.com:8443/view", "http://harbor.example.com/view"] {
        let status = computer::map_status("https://harbor.example.com", false, &harbor_body("running", Some(url)));
        assert_eq!(status["desktopUrl"], Value::Null, "{url} must be dropped");
    }
    let no_limits = json!({"computer": {"status": "running"}, "desktop_url": Value::Null});
    assert_eq!(computer::map_status("https://harbor.example.com", true, &no_limits)["limits"], Value::Null);
}

#[test]
fn computer_cap_and_routine_parsing_bound_what_reaches_an_agent() {
    let long = "x".repeat(70_000);
    let capped = computer::cap_output(long.clone());
    assert!(capped.len() <= 64 * 1024 + 64, "{}", capped.len());
    assert!(capped.ends_with("(output truncated at 64 KB)"));
    assert_eq!(computer::cap_output("short".into()), "short");

    let wrapped = json!({"routines": [
        {"id": "r-1", "name": "Report", "cron": "0 8 * * *", "command": "run.sh", "enabled": true, "lastRunAt": "2026-11-12T08:00:00.000Z", "createdAt": "2026-11-01T00:00:00.000Z", "lastResult": {"ok": false, "output": "x".repeat(3_000)}},
        {"id": 7, "name": "Bare", "cron": "* * * * *", "command": "true", "enabled": false, "lastResult": "printed text"},
    ]});
    let rows = computer::parse_routines(&wrapped);
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["id"], "r-1");
    assert_eq!(rows[0]["lastOk"], false);
    assert_eq!(rows[0]["lastOutput"].as_str().unwrap().chars().count(), 2_000, "cut to 2,000 characters");
    assert_eq!(rows[1]["id"], "7", "a numeric id becomes text");
    assert_eq!(rows[1]["lastOk"], Value::Null);
    assert_eq!(rows[1]["lastOutput"], "printed text");

    // A bare array is accepted too.
    let bare = json!([{"id": "r-2", "name": "Only", "cron": "0 0 * * *", "command": "c", "enabled": true}]);
    let rows = computer::parse_routines(&bare);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["lastOk"], Value::Null);
    assert_eq!(rows[0]["lastOutput"], Value::Null);
}

#[tokio::test]
async fn computer_link_accepts_a_200_and_keeps_the_token_out_of_every_reply_and_event() {
    let address = fake_harbor(|_, _, _| (200, harbor_body("running", Some("http://127.0.0.1:1/view")))).await;
    let (st, mut events) = state();
    let linked = dispatch(&st, "computer.link", json!({"address": format!("http://{address}"), "token": HARBOR_TOKEN, "confirm": true})).await.unwrap();
    assert_eq!(linked["linked"], true);
    assert_eq!(linked["address"], format!("http://{address}"));
    assert_eq!(linked["enabled"], false, "switched off until the owner turns it on");
    assert_eq!(linked["state"], "awake");
    assert_eq!(linked["name"], "harbor-bot-retinue-01");
    assert!(!linked.to_string().contains(HARBOR_TOKEN), "the reply carries no token");

    let link = st.db.computer_link().unwrap().unwrap();
    assert!(link.token_ref.starts_with("computer/"), "{}", link.token_ref);
    assert_eq!(secrets::store::read(&link.token_ref).unwrap().unwrap().as_str(), HARBOR_TOKEN, "the token is in the credential store");

    let event = events.recv().await.unwrap();
    assert_eq!(event.event_type, "computer.changed");
    assert_eq!(event.payload["state"], "awake");
    assert!(!event.payload.to_string().contains(HARBOR_TOKEN), "the event carries no token");

    // Reading it back names the address and never the token.
    let status = dispatch(&st, "computer.status", json!({})).await.unwrap();
    assert_eq!(status["linked"], true);
    assert_eq!(status["address"], format!("http://{address}"));
    assert!(!status.to_string().contains(HARBOR_TOKEN));
}

#[tokio::test]
async fn computer_link_refuses_a_401_and_stores_nothing() {
    let address = fake_harbor(|_, _, _| (401, json!({"error": {"code": "unauthorized", "message": "bad token"}}))).await;
    let (st, _) = state();
    assert_eq!(dispatch(&st, "computer.link", json!({"address": format!("http://{address}"), "token": HARBOR_TOKEN})).await.unwrap_err().1.code, "invalid_argument", "no confirm");
    let refused = dispatch(&st, "computer.link", json!({"address": format!("http://{address}"), "token": HARBOR_TOKEN, "confirm": true})).await.unwrap_err();
    assert_eq!(refused.1.code, "invalid_argument");
    assert_eq!(refused.1.message, "Harbor refused that token.");
    assert!(!refused.1.message.contains(HARBOR_TOKEN));
    assert!(st.db.computer_link().unwrap().is_none(), "nothing was stored");
}

#[tokio::test]
async fn computer_link_refuses_an_address_that_is_not_harbor() {
    let address = fake_harbor(|_, _, _| (200, json!({"hello": "not harbor"}))).await;
    let (st, _) = state();
    let refused = dispatch(&st, "computer.link", json!({"address": format!("http://{address}"), "token": HARBOR_TOKEN, "confirm": true})).await.unwrap_err();
    assert_eq!(refused.1.code, "invalid_argument");
    assert!(st.db.computer_link().unwrap().is_none());
}

#[tokio::test]
async fn computer_status_is_never_an_error_and_reports_an_unreachable_server_plainly() {
    let (st, _) = state();
    let unlinked = dispatch(&st, "computer.status", json!({})).await.unwrap();
    assert_eq!(unlinked["linked"], false);
    assert_eq!(unlinked["state"], "unknown");
    assert_eq!(unlinked["reachable"], false);

    // Port 1 on loopback refuses at once.
    link_directly(&st, "http://127.0.0.1:1", HARBOR_TOKEN, true);
    let offline = dispatch(&st, "computer.status", json!({})).await.unwrap();
    assert_eq!(offline["linked"], true);
    assert_eq!(offline["reachable"], false);
    assert_eq!(offline["state"], "unknown");
    assert!(offline["checkedAt"].as_str().is_some());
    let problem = offline["problem"].as_str().unwrap();
    assert!(!problem.is_empty() && problem.ends_with('.'), "{problem}");
    assert!(!offline.to_string().contains(HARBOR_TOKEN));
}

#[tokio::test]
async fn computer_sleep_calls_harbors_stop_and_a_write_is_announced() {
    let seen = Arc::new(TestMutex::new(Vec::<String>::new()));
    let recorder = seen.clone();
    let address = fake_harbor(move |method, path, _| {
        recorder.lock().unwrap().push(format!("{method} {path}"));
        if path.ends_with("/action/stop") { (200, json!({"computer": {"status": "exited"}})) } else { (200, harbor_body("running", None)) }
    })
    .await;
    let (st, mut events) = state();
    let status = dispatch(&st, "computer.link", json!({"address": format!("http://{address}"), "token": HARBOR_TOKEN, "confirm": true})).await.unwrap();
    assert_eq!(status["enabled"], false);
    let _ = events.recv().await;

    let status = dispatch(&st, "computer.setEnabled", json!({"enabled": true})).await.unwrap_err();
    assert_eq!(status.1.code, "invalid_argument", "turning it on needs confirm");
    dispatch(&st, "computer.setEnabled", json!({"enabled": true, "confirm": true})).await.unwrap();
    while events.try_recv().is_ok() {}

    let status = dispatch(&st, "computer.action", json!({"action": "sleep"})).await.unwrap();
    assert_eq!(status["enabled"], true);
    assert!(seen.lock().unwrap().iter().any(|line| line == "POST /harbor/api/computer/action/stop"), "{:?}", seen.lock().unwrap());
    assert_eq!(events.recv().await.unwrap().event_type, "computer.changed");

    for (action, expected) in [("wake", "/action/wake"), ("restart", "/action/restart")] {
        dispatch(&st, "computer.action", json!({"action": action})).await.unwrap();
        assert!(seen.lock().unwrap().iter().any(|line| line == &format!("POST /harbor/api/computer{expected}")), "{action}");
    }
    assert_eq!(dispatch(&st, "computer.action", json!({"action": "nope"})).await.unwrap_err().1.code, "invalid_argument");
}

#[tokio::test]
async fn computer_status_is_announced_only_when_it_changes() {
    let docker = Arc::new(TestMutex::new("running".to_owned()));
    let shared = docker.clone();
    let address = fake_harbor(move |_, _, _| (200, harbor_body(&shared.lock().unwrap(), None))).await;
    let (st, mut events) = state();
    link_directly(&st, &format!("http://{address}"), HARBOR_TOKEN, true);

    assert_eq!(dispatch(&st, "computer.status", json!({})).await.unwrap()["state"], "awake");
    let first = events.recv().await.unwrap();
    assert_eq!(first.event_type, "computer.changed");
    // The same state read again differs only in checkedAt, which is not a change.
    assert_eq!(dispatch(&st, "computer.status", json!({})).await.unwrap()["state"], "awake");
    assert!(events.try_recv().is_err(), "no event for an unchanged read");

    *docker.lock().unwrap() = "exited".to_owned();
    assert_eq!(dispatch(&st, "computer.status", json!({})).await.unwrap()["state"], "asleep");
    assert_eq!(events.recv().await.unwrap().payload["state"], "asleep");
}

#[tokio::test]
async fn computer_routines_and_logs_read_through_the_linked_server() {
    let address = fake_harbor(|_, path, _| {
        if path.starts_with("/harbor/api/computer/routines/") { return (200, json!({"ok": true, "output": "ran"})); }
        if path.starts_with("/harbor/api/computer/routines") { return (200, json!({"routines": [{"id": "r-1", "name": "Report", "cron": "0 8 * * *", "command": "run.sh", "enabled": true, "lastRunAt": "2026-11-12T08:00:00.000Z", "lastResult": {"ok": true, "output": "done"}}]})); }
        if path.starts_with("/harbor/api/computer/logs") { return (200, json!({"logs": "line one\nline two\n"})); }
        (200, harbor_body("running", None))
    })
    .await;
    let (st, _) = state();
    link_directly(&st, &format!("http://{address}"), HARBOR_TOKEN, true);

    let routines = dispatch(&st, "computer.routines", json!({})).await.unwrap();
    assert_eq!(routines["routines"][0]["id"], "r-1");
    assert_eq!(routines["routines"][0]["lastOk"], true);
    assert_eq!(routines["routines"][0]["lastOutput"], "done");
    assert_eq!(routines["routines"][0]["lastRunAt"], "2026-11-12T08:00:00.000Z");

    let run = dispatch(&st, "computer.routine.run", json!({"id": "r-1"})).await.unwrap();
    assert_eq!(run, json!({"ok": true, "output": "ran"}));
    assert_eq!(dispatch(&st, "computer.routine.run", json!({})).await.unwrap_err().1.code, "invalid_argument");

    let logs = dispatch(&st, "computer.logs", json!({"tail": 5000})).await.unwrap();
    assert_eq!(logs["logs"], "line one\nline two\n");
    assert_eq!(dispatch(&st, "computer.logs", json!({"tail": "lots"})).await.unwrap_err().1.code, "invalid_argument");
}

#[tokio::test]
async fn computer_mcp_server_is_offered_only_while_linked_and_enabled_and_carries_no_token() {
    let (st, _) = state();
    let dir = std::env::temp_dir().join(format!("retinue-computer-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    *st.team.data_dir.lock().unwrap() = Some(dir.clone());
    *st.team.address.lock().unwrap() = Some("127.0.0.1:9".to_owned());

    assert!(computer::mcp_server_for(&st, "session-one").is_none(), "nothing linked, no server");
    link_directly(&st, "http://127.0.0.1:1", HARBOR_TOKEN, false);
    assert!(computer::mcp_server_for(&st, "session-one").is_none(), "linked but switched off, no server");
    st.db.computer_link_set_enabled(true).unwrap();

    let spec = computer::mcp_server_for(&st, "session-one").expect("a linked, enabled computer is offered");
    assert_eq!(spec.name, "vps-computer");
    assert!(spec.env.is_empty(), "no token in the environment: {:?}", spec.env.keys().collect::<Vec<_>>());
    assert_eq!(spec.args.first().map(String::as_str), Some("computer-server"));
    assert_eq!(spec.args.get(1).map(String::as_str), Some("--endpoint"));
    let endpoint = std::path::PathBuf::from(spec.args.get(2).expect("an endpoint path"));
    let written = std::fs::read_to_string(&endpoint).unwrap();
    assert!(written.contains("\"address\":\"127.0.0.1:9\""), "{written}");
    assert!(!written.contains(HARBOR_TOKEN), "the endpoint file names the daemon, not Harbor");
    assert!(spec.args.iter().all(|arg| !arg.contains(HARBOR_TOKEN)), "argv carries no token");

    computer::forget_link(&st).await;
    assert!(computer::mcp_server_for(&st, "session-one").is_none(), "unlinked, no server");
    assert!(st.db.computer_link().unwrap().is_none());
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn computer_tools_refuse_after_unlink_and_while_switched_off() {
    let address = fake_harbor(|_, path, _| {
        if path.ends_with("/exec") { return (200, json!({"ok": true, "code": 0, "output": "hello"})); }
        (200, harbor_body("running", None))
    })
    .await;
    let (st, _) = state();
    st.db.create_session("computer-session", "rt-test", "codex", ".", "Chat").unwrap();
    st.db.update_session_state("computer-session", "idle").unwrap();
    link_directly(&st, &format!("http://{address}"), HARBOR_TOKEN, true);

    let status = computer::mcp_rpc(&st, "computer-session", "mcp.computer.call", &json!({"name": "computer_status", "arguments": {}})).await.unwrap();
    assert_eq!(status["isError"], Value::Null);
    let exec = computer::mcp_rpc(&st, "computer-session", "mcp.computer.call", &json!({"name": "computer_exec", "arguments": {"command": "echo hi"}})).await.unwrap();
    assert_eq!(exec["content"][0]["text"].as_str().unwrap().contains("hello"), true);

    dispatch(&st, "computer.unlink", json!({})).await.unwrap();
    for name in ["computer_status", "computer_exec"] {
        let refused = computer::mcp_rpc(&st, "computer-session", "mcp.computer.call", &json!({"name": name, "arguments": {"command": "echo hi"}})).await.unwrap();
        assert_eq!(refused["isError"], true, "{name}");
        assert!(refused["content"][0]["text"].as_str().unwrap().contains("No computer is linked"), "{refused}");
    }

    link_directly(&st, &format!("http://{address}"), HARBOR_TOKEN, false);
    let off = computer::mcp_rpc(&st, "computer-session", "mcp.computer.call", &json!({"name": "computer_status", "arguments": {}})).await.unwrap();
    assert_eq!(off["isError"], true);
    assert!(off["content"][0]["text"].as_str().unwrap().contains("switched off"), "{off}");
    assert!(!off.to_string().contains(HARBOR_TOKEN));
}

#[tokio::test]
async fn computer_command_tools_are_refused_in_an_unattended_conversation() {
    let address = fake_harbor(|_, path, _| {
        if path.ends_with("/exec") { return (200, json!({"ok": true, "code": 0, "output": "hello"})); }
        (200, harbor_body("running", None))
    })
    .await;
    let (st, _) = state();
    st.db.create_session("automation-run", "rt-test", "codex", ".", "Scheduled").unwrap();
    st.db.set_session_link("automation-run", &json!({"kind": "automation", "automationId": "auto-1"})).unwrap();
    st.db.update_session_state("automation-run", "idle").unwrap();
    link_directly(&st, &format!("http://{address}"), HARBOR_TOKEN, true);

    let status = computer::mcp_rpc(&st, "automation-run", "mcp.computer.call", &json!({"name": "computer_status", "arguments": {}})).await.unwrap();
    assert_ne!(status["isError"], true, "reading the state is not a command");

    for (name, args) in [("computer_exec", json!({"command": "echo hi"})), ("computer_create_routine", json!({"name": "n", "cron": "* * * * *", "command": "c"})), ("computer_run_routine", json!({"id": "r-1"}))] {
        let refused = computer::mcp_rpc(&st, "automation-run", "mcp.computer.call", &json!({"name": name, "arguments": args})).await.unwrap();
        assert_eq!(refused["isError"], true, "{name}");
        assert!(refused["content"][0]["text"].as_str().unwrap().contains("runs unattended"), "{refused}");
    }
}

#[tokio::test]
async fn computer_desktop_url_is_text_and_only_from_the_linked_origin() {
    // The fake answers with a URL on the address the test links, filled in after
    // the port is known; a second case answers from someone else's host.
    let origin = Arc::new(TestMutex::new(String::new()));
    let origin_in = origin.clone();
    let address = fake_harbor(move |_, _, _| {
        let origin = origin_in.lock().unwrap().clone();
        (200, harbor_body("running", Some(&format!("{origin}/harbor/computer/view/"))))
    })
    .await;
    *origin.lock().unwrap() = format!("http://{address}");
    let (st, _) = state();
    link_directly(&st, &format!("http://{address}"), HARBOR_TOKEN, true);
    let same = computer::mcp_rpc(&st, "computer-session", "mcp.computer.call", &json!({"name": "computer_desktop_url", "arguments": {}})).await.unwrap();
    let text = same["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("/harbor/computer/view/"), "{text}");
    assert!(!text.contains(HARBOR_TOKEN));

    let other = fake_harbor(|_, _, _| (200, harbor_body("running", Some("https://someone-else.example.com/view")))).await;
    st.db.computer_link_clear().unwrap();
    link_directly(&st, &format!("http://{other}"), HARBOR_TOKEN, true);
    let dropped = computer::mcp_rpc(&st, "computer-session", "mcp.computer.call", &json!({"name": "computer_desktop_url", "arguments": {}})).await.unwrap();
    assert!(dropped["content"][0]["text"].as_str().unwrap().contains("\"desktop_url\": null"), "{dropped}");
}

#[tokio::test]
async fn computer_bridge_lists_the_ten_tools_and_answers_unknown_ones() {
    let (st, _) = state();
    let listed = computer::mcp_rpc(&st, "computer-session", "mcp.computer.list", &json!({})).await.unwrap();
    let names = listed["tools"].as_array().unwrap().iter().map(|tool| tool["name"].as_str().unwrap().to_owned()).collect::<Vec<_>>();
    assert_eq!(names, vec![
        "computer_status", "computer_wake", "computer_sleep", "computer_restart", "computer_logs", "computer_exec",
        "computer_list_routines", "computer_create_routine", "computer_run_routine", "computer_desktop_url",
    ]);
    let unknown = computer::mcp_rpc(&st, "computer-session", "mcp.computer.call", &json!({"name": "computer_sudo", "arguments": {}})).await.unwrap();
    assert_eq!(unknown["isError"], true, "an unknown tool is not forwarded anywhere");
}
