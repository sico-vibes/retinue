// 18 November 2026: the Retinue MCP's tools for the linked machines.
//
// Included into `machines_tests`, so it shares that module's two-daemon fixtures: `host` is
// another machine behind its real Native gateway, `local` is this one, and `live_link` pairs them
// through the real pairing with the live socket following. Every tool is called the way an outside
// session calls it, through `external::route("ext.tools.call")`, so the catalog, its levels, the
// bearer-checked route and the activity line are all exercised as well as the tool's own body.

/// Switches the Retinue MCP on at `tier`, as Settings does. The endpoint file needs an address
/// and a data folder, which `local` gives; the address is never contacted here.
async fn mcp_on(st: &AppState, tier: &str) {
    *st.team.address.lock().unwrap() = Some("127.0.0.1:1".to_owned());
    crate::external::rpc(st, "external.enable", &json!({"tier": tier, "confirm": true})).await.unwrap();
}

/// One tool as an outside session sees it: `(is_error, payload)`.
async fn mcp(st: &AppState, name: &str, args: Value) -> (bool, Value) {
    let reply = crate::external::route(st, "ext.tools.call", &json!({"name": name, "arguments": args})).await.unwrap();
    let text = reply["content"][0]["text"].as_str().unwrap_or("").to_owned();
    (reply["isError"] == true, serde_json::from_str(&text).unwrap_or(Value::String(text)))
}

/// The tools the MCP offers at the level this machine is set to.
async fn mcp_names(st: &AppState) -> Vec<String> {
    let listed = crate::external::route(st, "ext.tools.list", &json!({})).await.unwrap();
    listed["tools"].as_array().unwrap().iter().filter_map(|tool| tool["name"].as_str().map(str::to_owned)).collect()
}

/// Makes the far machine answer every prompt at once with the test adapter, so a round trip is
/// tested without starting a real coding agent (the same arrangement `external_tests` uses).
async fn stub_far_agent(host: &mut AppState) {
    let runtime = json!({"id": "rt-test", "provider": "codex", "status": "online", "executablePath": "unused-test-runtime", "launchArgs": []});
    host.db.upsert_runtime(&runtime).unwrap();
    *host.runtimes.write().await = vec![runtime];
    host.adapters = Arc::new(crate::Adapters {
        acp: host.adapters.acp.clone(),
        codex: Arc::new(crate::phase2a_tests::TeamAdapter::default()),
        claude: host.adapters.claude.clone(),
    });
}

/// `live_link`, except that the far machine answers every prompt with the test adapter. Its
/// gateway serves a clone of the state it was started with, so the stub has to be in place
/// before `remote::enable` binds the listener: swapping it afterwards only changes this copy.
async fn live_link_with_stub_agent(tag: &str, tier: &str) -> (Host, String, AppState, PathBuf) {
    let (mut st, host_dir) = local(&format!("{tag}-host"));
    stub_far_agent(&mut st).await;
    let other = host_on(st, host_dir).await;
    let bot = add_bot(&other.st, "Rae");
    other.st.db.create_session_for_agent("s-live", "rt-test", "codex", ".", "First light", Some(&bot)).unwrap();
    other.st.db.update_session_state("s-live", "idle").unwrap();
    let (me, dir) = local(&format!("{tag}-me"));
    me.machines.streams.set_timing(stream_timing());
    link_native(&me, &other, tier).await;
    let other_id = host_id(&other);
    machines::refresh_one(&me, &other_id).await.unwrap();
    crate::machine_stream::ensure(&me);
    wait_until("the live socket to open", || crate::machine_stream::live(&me, &other_id)).await;
    (other, bot, me, dir)
}

/// A live session on the far machine, so an answered card has somewhere to go: `permission.reply`
/// refuses when no adapter session is open there.
async fn live_session_on(host: &AppState, sid: &str, agent: &str) {
    use retinue_agent_core::{AgentCapabilities, ApprovalMode, RuntimeSpec, SessionHandle};
    host.db.create_session_for_agent(sid, "rt-test", "codex", ".", "Waiting", Some(agent)).unwrap();
    let adapter: Arc<dyn retinue_agent_core::AgentAdapter> = Arc::new(crate::phase2a_tests::TeamAdapter::default());
    let handle = SessionHandle { session_id: sid.into(), provider_session_id: "native".into(), capabilities: AgentCapabilities::default() };
    let runtime = RuntimeSpec { runtime_id: "rt-test".into(), provider: "codex".into(), executable: PathBuf::from("fake"), args: vec![], cwd: Some(PathBuf::from(".")) };
    host.sessions.lock().await.insert(sid.to_owned(), crate::ActiveSession { handle, runtime, adapter, launch_approval_mode: ApprovalMode::Ask });
}

/// One approval card waiting on the far machine, as a provider would leave it.
fn card_on(host: &AppState, session: &str, id: &str) {
    host.db
        .insert_permission(
            id,
            session,
            "req-1",
            "Run the tests",
            Some("npm test"),
            &["allow".to_string(), "deny".to_string()],
            &json!({"request": {"tool_name": "Bash", "input": {"command": "npm test"}}}),
            &json!({"kind": "shell", "matchKey": "shell:npm test", "rememberable": false, "rememberLabel": null, "projectId": null, "projectName": null}),
        )
        .unwrap();
}

// ---------------------------------------------------------------------------
// The level each tool needs

#[tokio::test]
async fn the_machine_tools_appear_at_the_level_that_may_call_them() {
    let (me, dir) = local("mcp-levels");
    mcp_on(&me, "viewer").await;
    let viewer = mcp_names(&me).await;
    for name in [
        "list_machines", "machine_conversations", "read_machine_conversation", "wait_on_machine",
        "machine_approvals", "machine_folders", "machine_clone_status", "diagnose_machines",
    ] {
        assert!(viewer.contains(&name.to_owned()), "{name} reads, so Viewer offers it");
    }
    for name in ["start_on_machine", "message_on_machine", "steer_on_machine", "stop_on_machine"] {
        assert!(!viewer.contains(&name.to_owned()), "{name} drives work, so it needs Chat");
    }
    for name in ["answer_machine_approval", "machine_add_project", "machine_clone", "machine_devices", "machine_remote_status"] {
        assert!(!viewer.contains(&name.to_owned()), "{name} needs Operator");
    }
    // Nothing of the vault, the owner's own computer or the linked cloud computer is ever offered.
    for name in &viewer {
        assert!(
            !(name.starts_with("vault.") || name.starts_with("owner_") || name.starts_with("computer_")),
            "{name} must never be reachable through the MCP"
        );
    }
    // Called anyway, a tool above the level is refused by its own sentence and does nothing.
    let (error, message) = mcp(&me, "start_on_machine", json!({"machineId": "m", "botId": "b", "text": "hi"})).await;
    assert!(error);
    assert_eq!(message, json!("This tool needs the Chat level in Retinue › Settings › Connections."));

    crate::external::rpc(&me, "external.setTier", &json!({"tier": "chat"})).await.unwrap();
    let chat = mcp_names(&me).await;
    for name in ["start_on_machine", "message_on_machine", "steer_on_machine", "stop_on_machine"] {
        assert!(chat.contains(&name.to_owned()), "{name} at Chat");
    }
    for name in ["answer_machine_approval", "machine_add_project", "machine_clone", "machine_devices", "machine_remote_status"] {
        assert!(!chat.contains(&name.to_owned()), "{name} still needs Operator");
    }
    let (error, message) = mcp(&me, "machine_devices", json!({"machineId": "m"})).await;
    assert!(error);
    assert_eq!(message, json!("This tool needs the Operator level in Retinue › Settings › Connections."));

    crate::external::rpc(&me, "external.setTier", &json!({"tier": "operator"})).await.unwrap();
    let operator = mcp_names(&me).await;
    for name in [
        "list_machines", "machine_conversations", "read_machine_conversation", "start_on_machine",
        "message_on_machine", "steer_on_machine", "stop_on_machine", "wait_on_machine", "machine_approvals",
        "answer_machine_approval", "machine_folders", "machine_add_project", "machine_clone",
        "machine_clone_status", "machine_devices", "machine_remote_status", "diagnose_machines",
    ] {
        assert!(operator.contains(&name.to_owned()), "{name} at Operator");
    }
    // With no machine linked, the two lists answer plainly rather than failing.
    let (error, listed) = mcp(&me, "list_machines", json!({})).await;
    assert!(!error);
    assert_eq!(listed, json!({"machines": []}));
    let (error, report) = mcp(&me, "diagnose_machines", json!({})).await;
    assert!(!error);
    assert_eq!(report, json!({"machines": []}));
    let (error, message) = mcp(&me, "machine_conversations", json!({"machineId": "nobody"})).await;
    assert!(error);
    assert_eq!(message, json!("That machine isn't linked."));
    let _ = std::fs::remove_dir_all(dir);
}

// ---------------------------------------------------------------------------
// Reading a live link, and diagnosing it

#[tokio::test]
async fn list_machines_and_diagnose_read_a_live_link() {
    let (other, _bot, me, dir) = live_link("mcp-live", "operator").await;
    mcp_on(&me, "viewer").await;

    let (error, listed) = mcp(&me, "list_machines", json!({})).await;
    assert!(!error, "{listed}");
    let machine = &listed["machines"][0];
    assert_eq!(machine["id"], json!(host_id(&other)));
    assert_eq!(machine["state"], "online");
    assert_eq!(machine["live"], true);
    assert_eq!(machine["tier"], "operator");
    assert!(machine["version"].as_str().is_some(), "the far version is read through the link: {machine}");
    assert_eq!(machine["stream"]["connected"], true);
    assert_eq!(machine["bots"][0]["name"], "Rae");
    // The far address, its certificate fingerprint and how this computer signed in stay here.
    let mut keys: Vec<&str> = machine.as_object().unwrap().keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(keys, ["bots", "detail", "id", "kind", "live", "name", "state", "stream", "tier", "version"]);
    let text = listed.to_string();
    assert!(!text.contains(&other.url()), "the far address stayed here: {text}");
    assert!(!text.contains(&other.fingerprint()), "the certificate fingerprint stayed here: {text}");

    let (error, report) = mcp(&me, "diagnose_machines", json!({})).await;
    assert!(!error, "{report}");
    let report = &report["machines"][0];
    assert_eq!(report["reachable"], true, "the ping needs no credential and no Origin: {report}");
    assert_eq!(report["isRetinue"], true);
    assert_eq!(report["certificateTrusted"], false, "the test certificate is self-signed");
    assert_eq!(report["pinnedCertificate"], true);
    assert_eq!(report["summary"], json!(format!("{} is linked and live.", machines::machine_name(&other.st))));
    let probes = report["probes"].as_array().unwrap();
    assert_eq!(probes.len(), 3, "three reads, none of them a write: {report}");
    // The owner there has not shared a folder, so the browse is refused in that machine's own words.
    assert_eq!(probes[0]["what"], "Browsing its folders");
    assert_eq!(probes[0]["ok"], false);
    assert_eq!(probes[0]["code"], "forbidden_remote");
    assert!(
        probes[0]["message"].as_str().unwrap().starts_with("Projects from other machines are off on this computer."),
        "{report}"
    );
    assert_eq!(probes[1]["what"], "Listing the paired devices");
    assert_eq!(probes[1]["ok"], true, "{report}");
    assert!(probes[1]["count"].as_u64().unwrap() >= 1, "this computer is one of its devices: {report}");
    assert_eq!(probes[2]["what"], "Reading the pending approvals");
    assert_eq!(probes[2]["ok"], true, "{report}");
    assert_eq!(probes[2]["count"], 0);
    // Neither the address nor the fingerprint is in the diagnosis either.
    let text = report.to_string();
    assert!(!text.contains(&other.url()), "{text}");
    assert!(!text.contains(&other.fingerprint()), "{text}");
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn diagnose_machines_reports_an_offline_link_plainly() {
    let (me, dir) = local("mcp-diagnose");
    mcp_on(&me, "viewer").await;
    // A machine that answers a ping, but whose pairing this computer removed: it must not be asked.
    let server = raw_server(|request| is_ping(request).then(|| http("200 OK", &[], PING))).await;
    me.db
        .machine_link_upsert(&json!({
            "id": "revoked-machine", "name": "Old laptop", "url": server.url(), "auth": "code",
            "fingerprint": server.fingerprint(), "tier": "viewer", "pairedAt": "2026-11-17T00:00:00.000Z", "state": "revoked"
        }))
        .unwrap();
    // A machine that is simply away: its address answers nothing.
    let closed = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let closed_url = format!("https://127.0.0.1:{}", closed.local_addr().unwrap().port());
    drop(closed);
    me.db
        .machine_link_upsert(&json!({
            "id": "away-machine", "name": "Home PC", "url": closed_url, "auth": "code",
            "fingerprint": "AA:BB:CC", "tier": "chat", "pairedAt": "2026-11-17T00:00:00.000Z", "state": "offline"
        }))
        .unwrap();

    let (error, report) = mcp(&me, "diagnose_machines", json!({})).await;
    assert!(!error, "{report}");
    let reports = report["machines"].as_array().unwrap();
    let revoked = reports.iter().find(|row| row["id"] == "revoked-machine").unwrap();
    assert_eq!(revoked["reachable"], false);
    assert_eq!(revoked["summary"], "Old laptop removed this computer's pairing. Pair it again.");
    assert!(revoked["probes"].as_array().unwrap().is_empty(), "nothing is read from a link that was removed: {revoked}");
    assert!(server.seen().is_empty(), "a revoked link was pinged");
    let away = reports.iter().find(|row| row["id"] == "away-machine").unwrap();
    assert_eq!(away["reachable"], false);
    assert_eq!(away["state"], "offline");
    assert_eq!(away["summary"], "Home PC can't be reached right now.");
    assert!(away["probes"].as_array().unwrap().is_empty(), "nothing is read from an away machine: {away}");
    let _ = std::fs::remove_dir_all(dir);
}

// ---------------------------------------------------------------------------
// A far conversation, its work log, and where it was started from

#[tokio::test]
async fn a_far_conversation_reads_with_its_work_log_and_the_device_that_started_it() {
    let (other, bot, me, dir) = live_link("mcp-read", "operator").await;
    mcp_on(&me, "viewer").await;
    other
        .st
        .db
        .upsert_tool("s-live", "t-1", "shell", "Ran npm test", "completed", &json!({"input": {"command": "npm test"}, "output": "ok 3 tests"}))
        .unwrap();

    let (error, read) = mcp(&me, "read_machine_conversation", json!({"machineId": host_id(&other), "sessionId": "s-live"})).await;
    assert!(!error, "{read}");
    assert_eq!(read["machineId"], json!(host_id(&other)));
    assert_eq!(read["sessionId"], "s-live");
    assert_eq!(read["state"], "idle");
    assert_eq!(read["title"], "First light");
    let tool = read["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["kind"] == "tool")
        .expect("the far work log's tool step");
    assert_eq!(tool["title"], "Ran npm test");
    assert_eq!(tool["command"], "npm test");
    assert_eq!(tool["output"], "ok 3 tests");

    // A conversation this computer started carries the three-key mark on its row, so the list says
    // where it came from without a far read per conversation.
    let started = dispatch(&me, "machine.start", json!({"machineId": host_id(&other), "agentId": bot})).await.unwrap();
    let started_id = started["sessionId"].as_str().unwrap().to_owned();
    machines::refresh_one(&me, &host_id(&other)).await.unwrap();
    let (error, rows) = mcp(&me, "machine_conversations", json!({"machineId": host_id(&other)})).await;
    assert!(!error, "{rows}");
    assert_eq!(rows["machineName"], json!(machines::machine_name(&other.st)));
    assert_eq!(rows["stale"], false);
    let row = rows["conversations"].as_array().unwrap().iter().find(|row| row["id"] == json!(started_id)).expect("the new conversation in the list");
    assert_eq!(row["startedFrom"]["deviceKind"], "machine");
    assert_eq!(row["startedFrom"]["machineName"], json!(machines::machine_name(&me)));
    assert_eq!(row["startedFrom"].as_object().unwrap().len(), 3, "an id, a kind and a name, and no more: {row}");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_row_keeps_the_device_a_conversation_was_started_from() {
    let agents = json!({"agents": [{"id": "a", "name": "A"}]});
    let session = |id: &str, extra: Value| {
        let mut row = json!({"id": id, "agentId": "a", "state": "idle", "updatedAt": "2026-11-18T09:00:00Z", "archived": false});
        for (key, value) in extra.as_object().unwrap() {
            row[key] = value.clone();
        }
        row
    };
    let mark = json!({"deviceId": "dev-1", "deviceKind": "machine", "machineName": "Laptop"});
    let sessions = json!({"sessions": [
        session("from-a-device", json!({"link": {"startedFrom": mark}})),
        session("at-the-machine", json!({})),
        session("an-automation", json!({"link": {"kind": "automation"}})),
    ]});
    let rows = machines::sessions_from(&agents, &sessions);
    assert_eq!(rows[0]["startedFrom"], mark);
    assert_eq!(rows[1]["startedFrom"], Value::Null);
    assert_eq!(rows[2]["startedFrom"], Value::Null, "only a paired device's mark is copied");
}

#[test]
fn a_tool_step_keeps_its_command_and_output_only_for_a_machine() {
    let detail = json!({"messages": [], "tools": [{
        "id": "t-1", "kind": "shell", "title": "Ran npm test", "state": "completed",
        "startedAt": "2026-11-18T09:00:00Z", "detail": {"command": "npm test", "output": "ok 3 tests"}
    }], "turns": []});
    let far = crate::external::conversation_items(&detail, true);
    assert_eq!(far[0]["title"], "Ran npm test");
    assert_eq!(far[0]["command"], "npm test");
    assert_eq!(far[0]["output"], "ok 3 tests");
    let local = crate::external::conversation_items(&detail, false);
    assert_eq!(local[0]["text"], "Ran npm test (completed)", "a conversation on this computer keeps the line only");
    assert!(local[0].get("command").is_none() && local[0].get("output").is_none());
}

// ---------------------------------------------------------------------------
// Working there

#[tokio::test]
async fn start_and_message_on_a_machine_and_wait_return_the_reply() {
    let (other, bot, me, dir) = live_link_with_stub_agent("mcp-chat", "chat").await;
    mcp_on(&me, "chat").await;
    let machine = host_id(&other);

    let (error, started) = mcp(&me, "start_on_machine", json!({"machineId": machine, "botId": bot, "text": "Summarise the build"})).await;
    assert!(!error, "{started}");
    assert_eq!(started["started"], true);
    assert_eq!(started["machineId"], json!(machine));
    let session = started["sessionId"].as_str().unwrap().to_owned();
    let (error, waited) = mcp(&me, "wait_on_machine", json!({"machineId": machine, "sessionId": session, "timeoutSecs": 30})).await;
    assert!(!error, "{waited}");
    assert_eq!(waited["done"], true, "{waited}");
    // A finished turn leaves the far conversation in its own `completed` word, which is what
    // `wait_on_machine` returns unchanged: it is not a busy state, so the wait is over.
    assert_eq!(waited["state"], "completed", "{waited}");
    assert!(waited["reply"].as_str().unwrap().contains("handled: Summarise the build"), "{waited}");

    let (error, sent) = mcp(&me, "message_on_machine", json!({"machineId": machine, "sessionId": session, "text": "And once more"})).await;
    assert!(!error, "{sent}");
    assert_eq!(sent["result"]["accepted"], true, "{sent}");
    let (error, again) = mcp(&me, "wait_on_machine", json!({"machineId": machine, "sessionId": session, "timeoutSecs": 30})).await;
    assert!(!error, "{again}");
    assert_eq!(again["done"], true);
    assert!(again["reply"].as_str().unwrap().contains("handled: And once more"), "{again}");

    // A conversation that is already idle answers at once, and the turn's text is never in a log.
    let lines = me.db.remote_activity_list(20).unwrap();
    let joined = serde_json::to_string(&lines).unwrap();
    assert!(!joined.contains("Summarise the build") && !joined.contains("And once more"), "message text reached the activity list: {joined}");
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn answer_machine_approval_maps_the_card_word_and_needs_the_operator_level() {
    let (other, bot, me, dir) = live_link("mcp-approvals", "operator").await;
    live_session_on(&other.st, "s-perm", &bot).await;
    other.st.db.update_session_state("s-perm", "waiting_permission").unwrap();
    card_on(&other.st, "s-perm", "card-1");
    mcp_on(&me, "operator").await;
    let machine = host_id(&other);

    let (error, cards) = mcp(&me, "machine_approvals", json!({"machineId": machine})).await;
    assert!(!error, "{cards}");
    let card = &cards["permissions"][0];
    assert_eq!(card["id"], "card-1");
    assert_eq!(card["sessionId"], "s-perm");
    assert_eq!(card["kind"], "shell");
    assert_eq!(card["detail"], "npm test");
    assert_eq!(card["choices"], json!(["allow", "deny"]));
    let text = cards.to_string();
    assert!(!text.contains("tool_name") && !text.contains("matchKey"), "the provider payload and the rule key stayed there: {text}");

    // Approvals from remote are off there until its owner turns them on: its own sentence, unchanged.
    let (error, refused) = mcp(&me, "answer_machine_approval", json!({"machineId": machine, "permissionId": "card-1", "choice": "allow_once"})).await;
    assert!(error);
    assert!(refused.as_str().unwrap().contains("approvals from remote are turned off on this host"), "{refused}");

    dispatch(&other.st, "remote.security.set", json!({"approvalsFromRemote": true})).await.unwrap();
    let (error, answered) = mcp(&me, "answer_machine_approval", json!({"machineId": machine, "permissionId": "card-1", "choice": "allow_once"})).await;
    assert!(!error, "{answered}");
    assert_eq!(answered["choice"], "allow_once");
    assert_eq!(answered["answeredWith"], "allow", "the card's own word, as answer_approval does for a local card");
    assert_eq!(other.st.db.permission_status("card-1").unwrap().as_deref(), Some("resolved"), "the far card is answered");

    // A choice the enum does not offer is refused before anything is sent.
    let (error, message) = mcp(&me, "answer_machine_approval", json!({"machineId": machine, "permissionId": "card-1", "choice": "yes"})).await;
    assert!(error);
    assert_eq!(message, json!("choice must be allow_once or deny."));
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn folders_and_projects_stay_inside_the_far_machines_shared_folder() {
    let (other, _bot, me, dir) = live_link("mcp-projects", "operator").await;
    mcp_on(&me, "operator").await;
    let machine = host_id(&other);

    // Off there: the refusal is that machine's own sentence, unchanged.
    let (error, refused) = mcp(&me, "machine_folders", json!({"machineId": machine})).await;
    assert!(error);
    assert!(
        refused.as_str().unwrap().starts_with("Projects from other machines are off on this computer."),
        "{refused}"
    );

    // On: one level at a time inside the folder its owner named, and a folder there becomes a project.
    let base = shared_dir("mcp-projects-base");
    share(&other.st, &base);
    std::fs::create_dir_all(base.join("work")).unwrap();
    std::fs::write(base.join("notes.txt"), "hello\n").unwrap();
    let (error, listed) = mcp(&me, "machine_folders", json!({"machineId": machine})).await;
    assert!(!error, "{listed}");
    assert_eq!(listed["entries"], json!([{"name": "notes.txt", "kind": "file"}, {"name": "work", "kind": "folder"}]));
    let (error, project) = mcp(&me, "machine_add_project", json!({"machineId": machine, "path": base.join("work").to_string_lossy()})).await;
    assert!(!error, "{project}");
    assert_eq!(project["project"]["name"], "work");

    // A folder outside the shared one is refused, and `machine_clone` never starts a clone there.
    let outside = shared_dir("mcp-projects-outside");
    let (error, refused) = mcp(&me, "machine_clone", json!({"machineId": machine, "url": "https://example.invalid/repo.git", "folder": outside.to_string_lossy()})).await;
    assert!(error);
    assert_eq!(refused, json!("That folder is outside the folder this computer shares with other machines."));
    let _ = std::fs::remove_dir_all(dir);
    let _ = std::fs::remove_dir_all(base);
    let _ = std::fs::remove_dir_all(outside);
}

// ---------------------------------------------------------------------------
// Nothing secret crosses

#[tokio::test]
async fn no_machine_reply_carries_a_token_an_address_or_a_fingerprint() {
    let (other, bot, me, dir) = live_link("mcp-secrets", "operator").await;
    live_session_on(&other.st, "s-perm", &bot).await;
    card_on(&other.st, "s-perm", "card-1");
    mcp_on(&me, "operator").await;
    let machine = host_id(&other);

    let mut replies: Vec<Value> = Vec::new();
    for (name, args) in [
        ("list_machines", json!({})),
        ("machine_conversations", json!({"machineId": machine})),
        ("read_machine_conversation", json!({"machineId": machine, "sessionId": "s-live"})),
        ("machine_approvals", json!({"machineId": machine})),
        ("machine_folders", json!({"machineId": machine})),
        ("machine_devices", json!({"machineId": machine})),
        ("machine_remote_status", json!({"machineId": machine})),
        ("diagnose_machines", json!({})),
    ] {
        let (_, reply) = mcp(&me, name, args).await;
        replies.push(reply);
    }
    let text = serde_json::to_string(&replies).unwrap();
    assert!(!text.contains(&other.url()), "the far address crossed: {text}");
    assert!(!text.contains(&other.fingerprint()), "the certificate fingerprint crossed: {text}");
    assert!(!text.contains(PASSWORD), "the far password crossed: {text}");
    for path in token_files(&dir) {
        let token = std::fs::read_to_string(&path).unwrap();
        let token = token.trim();
        assert!(token.len() > 16, "the fixture must hold a device token: {path:?}");
        assert!(!text.contains(token), "the device token crossed: {text}");
    }
    // The device rows name devices; no token hash is among them.
    let devices = replies.iter().find(|reply| reply["devices"].is_array()).unwrap();
    for device in devices["devices"].as_array().unwrap() {
        assert!(device.get("tokenHash").is_none() && device.get("token").is_none(), "{device}");
    }
    // The ledger says who used which tool, and nothing else.
    let lines = me.db.remote_activity_list(50).unwrap();
    assert!(!lines.is_empty(), "every call left one line");
    for line in &lines {
        let summary = line["summary"].as_str().unwrap_or_default();
        assert!(summary.starts_with("MCP used "), "{summary}");
        assert!(!summary.contains(&other.url()) && !summary.contains(PASSWORD), "{summary}");
    }
    let _ = std::fs::remove_dir_all(dir);
}
