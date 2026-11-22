// One runtime per provider, and a turned-off provider left alone (owner brief,
// 21 November 2026). Included into `phase2a_tests`, so it shares that module's
// fixtures: `state`, `dispatch`, `publish_runtime_discovery`, `json!`.

fn retire_fixture(id: &str, provider: &str, executable: &str) -> retinue_runtime::DiscoveredRuntime {
    serde_json::from_value(json!({
        "id": id, "hostId": "host_windows_local", "provider": provider, "displayName": provider,
        "protocolFamily": "claude_stream", "executablePath": executable, "launchArgs": [],
        "version": "1.0.0", "authState": "authenticated", "gatewayAuthStates": {},
        "account": null, "status": "online", "capabilities": {}, "probeError": null, "probeStep": null
    })).unwrap()
}

fn stored_runtime(st: &AppState, id: &str) -> Value {
    st.db.runtimes().unwrap().into_iter().find(|row| row["id"] == id).unwrap_or_else(|| panic!("{id} is kept in the store"))
}

#[tokio::test]
async fn runtime_retire_at_startup_hides_an_unsupported_provider_and_keeps_its_bot() {
    let (st, mut events) = state();
    // Both rows still point to a real executable. The removed provider must
    // retire regardless of what is installed or which providers are switched off.
    let executable = std::env::current_exe().unwrap();
    let gemini = "rt_gemini_3f3b386c";
    let antigravity = "rt_antigravity_saved";
    for (id, provider) in [(gemini, "gemini"), (antigravity, "antigravity")] {
        st.db.upsert_runtime(&json!({
            "id": id, "provider": provider, "protocolFamily": "acp",
            "executablePath": executable, "launchArgs": [], "status": "online"
        })).unwrap();
    }
    st.db.set_setting("runtimes.disabled", &json!(["antigravity"])).unwrap();
    let (bot, _) = st.db.agent_create(&json!({"name": "Legacy bot", "runtimeId": gemini})).unwrap();
    let bot_id = bot["id"].as_str().unwrap();
    let project = std::env::current_dir().unwrap();
    let session = "legacy-gemini-session";
    st.db.create_session_for_agent(session, gemini, "gemini", &project.to_string_lossy(), "Legacy chat", Some(bot_id)).unwrap();
    st.db.set_session_state_quietly(session, "idle").unwrap();
    let legacy = stored_runtime(&st, gemini);
    st.emit("runtime.changed", json!({"runtimes": [legacy.clone()]})).await;
    assert_eq!(events.try_recv().unwrap().event_type, "runtime.changed");

    load_saved_runtimes(&st).await.unwrap();
    assert_eq!(stored_runtime(&st, gemini)[RETIRED], true);
    let listed = dispatch(&st, "runtime.list", json!({})).await.unwrap();
    let snapshot = dispatch(&st, "app.snapshot", json!({})).await.unwrap();
    for result in [listed, snapshot] {
        let rows = result["runtimes"].as_array().unwrap();
        assert!(rows.iter().all(|row| row["id"] != gemini));
        assert!(rows.iter().any(|row| row["id"] == antigravity));
    }

    let new_chat = new_session(&st, json!({"agentId": bot_id, "projectPath": project})).await.unwrap_err();
    assert_eq!(new_chat.1.code, "not_found");
    assert!(new_chat.1.message.contains("Choose a coding agent"));
    let send = dispatch(&st, "session.prompt", json!({"sessionId": session, "text": "Hello"})).await.unwrap_err();
    assert_eq!(send.1.code, "provider_unavailable", "{}", send.1.message);
    assert!(send.1.message.contains("no longer supported"));
    assert_eq!(st.db.session_detail(session).unwrap()["runtimeId"], gemini);

    publish_runtime_value(&st, legacy).await;
    let executable = executable.to_string_lossy();
    let codex = "rt_codex_3f3b386c";
    let scanned = publish_runtime_discovery(&st, vec![
        retire_fixture(gemini, "gemini", &executable),
        retire_fixture(codex, "codex", &executable),
    ]).await.unwrap();
    assert!(scanned.iter().any(|row| row["id"] == antigravity));
    assert!(scanned.iter().any(|row| row["id"] == codex));
    assert_eq!(stored_runtime(&st, gemini)[RETIRED], true);
    assert_eq!(st.db.agent_get(bot_id).unwrap()["runtimeId"], gemini);
    let mut runtime_changes = 0;
    while let Ok(event) = events.try_recv() {
        if event.event_type == "runtime.changed" {
            runtime_changes += 1;
            assert!(event.payload["runtimes"].as_array().unwrap().iter().all(|row| row["id"] != gemini));
        }
        assert!(event.event_type != "agent.changed" || event.payload["action"] != "moved");
    }
    assert_eq!(runtime_changes, 2);
    let replay = dispatch(&st, "events.replay", json!({"afterSequence": 0})).await.unwrap();
    let runtime_events = replay["events"].as_array().unwrap().iter().filter(|event| event["type"] == "runtime.changed").collect::<Vec<_>>();
    assert_eq!(runtime_events.len(), 3);
    assert_eq!(runtime_events[0]["payload"]["runtimes"], json!([]));
}

#[tokio::test]
async fn one_provider_never_shows_two_runtimes_and_the_stale_row_is_retired() {
    // Owner report (21 November 2026): the Agents page listed "OpenCode 2.3.2"
    // and a stale "OpenCode 2.0.28" shim, both signed in. The shim's executable
    // is gone, so its runtime id is never current again.
    let (st, _events) = state();
    st.db.upsert_runtime(&json!({
        "id": "rt_opencode_old", "hostId": "host_windows_local", "provider": "opencode",
        "protocolFamily": "claude_stream", "executablePath": "C:/Users/owner/.local/bin/opencode.cmd",
        "launchArgs": [], "version": "2.0.28", "authState": "authenticated", "status": "online"
    })).unwrap();
    let values = publish_runtime_discovery(&st, vec![retire_fixture("rt_opencode_new", "opencode", "C:/Program Files/opencode/opencode.exe")]).await.unwrap();
    assert_eq!(values.iter().filter(|row| row["provider"] == "opencode").count(), 1, "{values:?}");
    assert!(values.iter().any(|row| row["id"] == "rt_opencode_new"));
    // The replaced row is kept for old conversations but hidden from every list.
    assert_eq!(stored_runtime(&st, "rt_opencode_old")[RETIRED], true);
    let listed = dispatch(&st, "runtime.list", json!({})).await.unwrap();
    assert!(!listed["runtimes"].as_array().unwrap().iter().any(|row| row["id"] == "rt_opencode_old"));
    let snapshot = st.db.snapshot().unwrap();
    assert!(!snapshot["runtimes"].as_array().unwrap().iter().any(|row| row["id"] == "rt_opencode_old"));
    // A row discovery brings back at the same path is live again.
    let revived = publish_runtime_discovery(&st, vec![retire_fixture("rt_opencode_old", "opencode", "C:/Users/owner/.local/bin/opencode.cmd")]).await.unwrap();
    let row = revived.iter().find(|row| row["id"] == "rt_opencode_old").expect("revived");
    assert_eq!(row["retired"], Value::Null);
}

#[tokio::test]
async fn a_saved_runtime_whose_executable_is_gone_is_retired_live() {
    // A provider that is off is not probed (see the next test), so the one
    // evidence left is the disk: a row whose executable no longer exists is not
    // offered for the switch to turn back on.
    let (st, _events) = state();
    st.db.set_setting("runtimes.disabled", &json!(["grok"])).unwrap();
    st.db.upsert_runtime(&json!({
        "id": "rt_grok_saved", "provider": "grok", "protocolFamily": "acp",
        "executablePath": "C:/definitely/not/here/grok.exe", "launchArgs": [], "status": "online"
    })).unwrap();
    let values = publish_runtime_discovery(&st, vec![retire_fixture("rt_codex_new", "codex", "C:/tools/codex.exe")]).await.unwrap();
    assert!(!values.iter().any(|row| row["id"] == "rt_grok_saved"), "{values:?}");
    assert_eq!(stored_runtime(&st, "rt_grok_saved")[RETIRED], true);
    assert!(!st.db.snapshot().unwrap()["runtimes"].as_array().unwrap().iter().any(|row| row["id"] == "rt_grok_saved"));
}

#[tokio::test]
async fn a_provider_switched_off_with_two_saved_copies_keeps_one() {
    let (st, _events) = state();
    st.db.set_setting("runtimes.disabled", &json!(["grok"])).unwrap();
    let executable = std::env::current_exe().unwrap().to_string_lossy().into_owned();
    for id in ["rt_grok_a", "rt_grok_b"] {
        st.db.upsert_runtime(&json!({
            "id": id, "provider": "grok", "protocolFamily": "acp", "executablePath": executable,
            "launchArgs": [], "status": "online"
        })).unwrap();
    }
    let values = publish_runtime_discovery(&st, vec![retire_fixture("rt_codex_new", "codex", "C:/tools/codex.exe")]).await.unwrap();
    assert_eq!(values.iter().filter(|row| row["provider"] == "grok").count(), 1, "{values:?}");
    let kept = values.iter().find(|row| row["provider"] == "grok").unwrap();
    assert_eq!(kept["status"], "disabled");
    let retired = ["rt_grok_a", "rt_grok_b"].into_iter().filter(|id| stored_runtime(&st, id)[RETIRED] == true).count();
    assert_eq!(retired, 1, "exactly one copy is retired");
}

#[tokio::test]
async fn a_bot_on_a_retired_runtime_is_moved_and_announced() {
    let (st, mut events) = state();
    st.db.upsert_runtime(&json!({
        "id": "rt_opencode_old", "provider": "opencode", "protocolFamily": "claude_stream",
        "executablePath": "C:/gone/opencode.cmd", "launchArgs": [], "status": "online"
    })).unwrap();
    let (bot, _) = st.db.agent_create(&json!({
        "name": "Mover", "runtimeId": "rt_opencode_old", "model": "opencode/big-pickle", "thinking": "high"
    })).unwrap();
    let id = bot["id"].as_str().unwrap().to_owned();
    publish_runtime_discovery(&st, vec![retire_fixture("rt_opencode_new", "opencode", "C:/Program Files/opencode/opencode.exe")]).await.unwrap();
    let moved = st.db.agent_get(&id).unwrap();
    assert_eq!(moved["runtimeId"], "rt_opencode_new");
    // No catalog is cached in a test, so both are kept and the event says the
    // check could not be made rather than claiming the runtime offers them.
    assert_eq!(moved["model"], "opencode/big-pickle");
    assert_eq!(moved["thinking"], "high");
    let mut runtime_changed = false;
    let mut agent_moved = None;
    while let Ok(event) = events.try_recv() {
        if event.event_type == "runtime.changed" { runtime_changed = true; }
        if event.event_type == "agent.changed" && event.payload["action"] == "moved" { agent_moved = Some(event.payload); }
    }
    assert!(runtime_changed, "the window is told the runtime list changed");
    let payload = agent_moved.expect("the window is told the bot moved");
    assert_eq!(payload["agentId"], id.as_str());
    assert_eq!(payload["fromRuntimeId"], "rt_opencode_old");
    assert_eq!(payload["runtimeId"], "rt_opencode_new");
    assert_eq!(payload["checked"], false);
    assert_eq!(payload["modelKept"], true);
    assert_eq!(payload["effortKept"], true);
}

#[test]
fn a_moved_bot_keeps_its_model_and_effort_only_where_the_new_runtime_offers_them() {
    let catalog = json!({
        "validated": true, "fallback": false, "provider": "opencode",
        "models": [
            {"id": "opencode/big-pickle", "displayName": "Big Pickle", "supportedThinking": ["low", "high"], "defaultThinking": "low", "serviceTiers": [], "defaultServiceTier": null, "hostDependent": false, "isDefault": true},
            {"id": "opencode/other", "displayName": "Other", "supportedThinking": ["low"], "defaultThinking": "low", "serviceTiers": [], "defaultServiceTier": null, "hostDependent": false}
        ]
    });
    // Offered: both stay.
    let kept = moved_bot_options("opencode", &json!({"model": "opencode/big-pickle", "thinking": "high"}), Some(&catalog));
    assert!(kept.checked && kept.model_kept && kept.effort_kept, "{:?}", (kept.model, kept.thinking));
    assert_eq!((kept.model.as_deref(), kept.thinking.as_deref()), (Some("opencode/big-pickle"), Some("high")));
    // The model is gone: the catalog's default replaces it, and the event says so.
    let replaced = moved_bot_options("opencode", &json!({"model": "opencode/retired", "thinking": "high"}), Some(&catalog));
    assert!(replaced.checked && !replaced.model_kept);
    assert!(replaced.model.as_deref().is_some_and(|model| model != "opencode/retired"));
    // The model is offered but the effort is not: the model's default replaces it.
    let effort = moved_bot_options("opencode", &json!({"model": "opencode/big-pickle", "thinking": "max"}), Some(&catalog));
    assert!(effort.checked && effort.model_kept && !effort.effort_kept);
    assert_eq!(effort.thinking.as_deref(), Some("low"));
    // Nothing cached: both are kept, and the event says it could not check.
    let unchecked = moved_bot_options("opencode", &json!({"model": "opencode/retired", "thinking": "max"}), None);
    assert!(!unchecked.checked && unchecked.model_kept && unchecked.effort_kept);
}

#[test]
fn discovery_skips_exactly_the_providers_the_owner_turned_off() {
    // Owner report, 21 November 2026: Cursor CLI and Grok CLI were off and still
    // checked, and still warned. The list handed to discovery is the only switch.
    let (st, _events) = state();
    st.db.set_setting("runtimes.disabled", &json!(["cursor", "grok"])).unwrap();
    assert_eq!(providers_to_skip(&st.db), vec!["cursor".to_owned(), "grok".to_owned()]);
    // A main provider can never be off, whatever an old list or backup says.
    st.db.set_setting("runtimes.disabled", &json!(["claude", "cursor"])).unwrap();
    assert!(!providers_to_skip(&st.db).iter().any(|id| id == "claude"), "{:?}", providers_to_skip(&st.db));
    // A runtime id is not a provider id: one off copy does not stop the provider's
    // scan (the row is then stamped off, not probed on its own).
    st.db.set_setting("runtimes.disabled", &json!(["rt_cursor_old"])).unwrap();
    assert!(providers_to_skip(&st.db).is_empty());
}

#[tokio::test]
async fn turning_a_provider_off_clears_its_probe_result_and_turning_it_on_publishes_the_new_scan() {
    let (st, mut events) = state();
    st.db.set_setting("runtimes.disabled", &json!(["cursor"])).unwrap();
    let executable = std::env::current_exe().unwrap().to_string_lossy().into_owned();
    // An older build left a protocol failure on the off row.
    st.db.upsert_runtime(&json!({
        "id": "rt_cursor_saved", "provider": "cursor", "protocolFamily": "acp",
        "executablePath": executable, "launchArgs": ["--acp"], "status": "error",
        "probeStep": "Protocol check", "probeError": "The CLI does not advertise the required protocol."
    })).unwrap();
    // Off: discovery is not asked, so it sends no cursor row; the saved row is
    // kept, quiet, and stamped off for the switch.
    let off = publish_runtime_discovery(&st, vec![retire_fixture("rt_codex_new", "codex", "C:/tools/codex.exe")]).await.unwrap();
    let kept = off.iter().find(|row| row["id"] == "rt_cursor_saved").expect("kept for the switch");
    assert_eq!(kept["status"], "disabled");
    assert_eq!(kept["enabled"], false);
    assert!(kept.get("probeError").is_none(), "{kept}");
    // Back on: the scan's own answer is published, failure and all, with no restart.
    st.db.set_setting("runtimes.disabled", &json!([])).unwrap();
    let mut failed = retire_fixture("rt_cursor_saved", "cursor", "C:/tools/cursor.exe");
    failed.status = "error".into();
    failed.probe_step = Some("Protocol check".into());
    failed.probe_error = Some("The CLI does not advertise the required protocol.".into());
    let on = publish_runtime_discovery(&st, vec![retire_fixture("rt_codex_new", "codex", "C:/tools/codex.exe"), failed]).await.unwrap();
    let row = on.iter().find(|row| row["id"] == "rt_cursor_saved").expect("published again");
    assert_eq!(row["enabled"], true);
    assert_eq!(row["status"], "error");
    assert_eq!(row["probeError"], "The CLI does not advertise the required protocol.");
    // Each scan reached the window with no restart.
    let mut scans = 0;
    while let Ok(event) = events.try_recv() { if event.event_type == "runtime.changed" { scans += 1; } }
    assert_eq!(scans, 2, "one runtime.changed per scan");
}
