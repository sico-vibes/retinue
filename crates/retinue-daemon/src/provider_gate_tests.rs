// Providers behind switches (11 November 2026, owner decision): Claude Code,
// Codex and OpenCode are always on; every other provider is off by default and
// the owner turns it on in Settings.
// Included into `phase2a_tests`, so it shares that module's fixtures: `state`,
// `dispatch`, `a_project`, `json!`.

#[tokio::test]
async fn main_providers_are_always_on_and_the_optional_ones_start_off() {
    let (st, _events) = state();
    assert_eq!(provider_gate::optional_providers(), ["pi", "qwen", "cursor", "grok", "antigravity"]);
    // A main provider cannot be turned off by the provider-level switch, and the
    // refusal says what to do instead.
    for (main, name) in [("claude", "Claude Code"), ("codex", "Codex"), ("opencode", "OpenCode")] {
        assert!(provider_gate::is_main_provider(main), "{main}");
        let refused = dispatch(&st, "settings.set", json!({"key":"runtimes.disabled","value":[main]})).await.unwrap_err();
        assert_eq!(refused.1.code, "invalid_argument", "{main}");
        assert_eq!(refused.1.message, format!("{name} is always on and cannot be turned off. Turn off one installed copy instead."));
    }
    assert!(disabled_runtimes(&st.db).is_empty(), "a refused write stores nothing");
    // A list an older install saved (or a backup carried) cannot switch a main
    // provider off either: it is dropped when the list is read.
    st.db.set_setting("runtimes.disabled", &json!(["claude", "qwen"])).unwrap();
    assert_eq!(disabled_runtimes(&st.db), vec!["qwen".to_owned()]);
    // The one-time defaults switch every optional provider off, leave the main
    // three alone, and record that they ran.
    let (fresh, _events) = state();
    provider_gate::apply_defaults(&fresh.db).unwrap();
    assert_eq!(disabled_runtimes(&fresh.db), provider_gate::optional_providers().into_iter().map(str::to_owned).collect::<Vec<_>>());
    assert_eq!(fresh.db.settings().unwrap()[provider_gate::DEFAULTS_MARKER], true);
    // Applied once: a provider the owner switched back on is left alone.
    fresh.db.set_setting("runtimes.disabled", &json!(["qwen"])).unwrap();
    provider_gate::apply_defaults(&fresh.db).unwrap();
    assert_eq!(disabled_runtimes(&fresh.db), vec!["qwen".to_owned()]);
    // Every published runtime row says whether it is a main provider and whether
    // it is offered.
    load_saved_runtimes(&fresh).await;
    let row = fresh.runtimes.read().await.iter().find(|row| row["id"] == "rt-test").cloned().unwrap();
    assert_eq!(row["main"], true);
    assert_eq!(row["enabled"], true);
    // A migration marker is the daemon's own: a client cannot write one.
    assert_eq!(dispatch(&fresh, "settings.set", json!({"key":"migration.providerTogglesApplied","value":false})).await.unwrap_err().1.code, "invalid_argument");
}

// Nothing the owner already uses may disappear: a bot or a conversation on an
// optional provider keeps that provider on.
#[tokio::test]
async fn an_install_that_already_uses_an_optional_provider_keeps_it_on() {
    let (st, _events) = state();
    st.db.upsert_runtime(&json!({"id":"rt_qwen_abcd","provider":"qwen","status":"online","executablePath":"x","launchArgs":["--acp"]})).unwrap();
    st.db.agent_create(&json!({"name":"Qwen employee","runtimeId":"rt_qwen_abcd"})).unwrap();
    provider_gate::apply_defaults(&st.db).unwrap();
    let disabled = disabled_runtimes(&st.db);
    assert!(!disabled.iter().any(|id| id == "qwen"), "an employee on it keeps it on: {disabled:?}");
    assert!(disabled.iter().any(|id| id == "cursor") && disabled.iter().any(|id| id == "antigravity"), "{disabled:?}");
    // A conversation on another optional provider counts too, archived or not.
    let (second, _events) = state();
    second.db.create_session("s-cursor", "rt_cursor_1", "cursor", ".", "Older chat").unwrap();
    provider_gate::apply_defaults(&second.db).unwrap();
    let disabled = disabled_runtimes(&second.db);
    assert!(!disabled.iter().any(|id| id == "cursor"), "{disabled:?}");
    assert!(disabled.iter().any(|id| id == "qwen"), "{disabled:?}");
}

// A provider added after the one-time defaults ran (Pi, 16 November 2026) starts
// off too, with its own marker so the switch is written once.
#[tokio::test]
async fn a_provider_added_later_starts_off_once_and_never_over_what_the_owner_uses() {
    let (st, _events) = state();
    // An install whose defaults ran before Pi existed.
    st.db.set_setting(provider_gate::DEFAULTS_MARKER, &json!(true)).unwrap();
    st.db.set_setting("runtimes.disabled", &json!([])).unwrap();
    provider_gate::apply_new_provider_defaults(&st.db).unwrap();
    assert_eq!(disabled_runtimes(&st.db), vec!["pi".to_owned()]);
    // Once only: an owner who switched it on keeps it on.
    st.db.set_setting("runtimes.disabled", &json!([])).unwrap();
    provider_gate::apply_new_provider_defaults(&st.db).unwrap();
    assert!(disabled_runtimes(&st.db).is_empty(), "the marker means one write, not one per start");
    // A bot already on Pi keeps it on, the same rule the first defaults follow.
    let (second, _events) = state();
    second.db.upsert_runtime(&json!({"id":"rt_pi_abcd","provider":"pi","status":"online","executablePath":"x","launchArgs":[]})).unwrap();
    second.db.agent_create(&json!({"name":"Pi employee","runtimeId":"rt_pi_abcd"})).unwrap();
    provider_gate::apply_new_provider_defaults(&second.db).unwrap();
    assert!(!disabled_runtimes(&second.db).iter().any(|id| id == "pi"), "an employee on it keeps it on");
}

#[tokio::test]
async fn a_switched_off_provider_is_absent_from_choices_and_listed_with_its_state() {
    let (st, _events) = state();
    // One installed copy of Grok, switched off. Written through the store so the
    // test does not depend on which CLIs this machine happens to have.
    st.db.upsert_runtime(&json!({"id":"rt_grok_1","provider":"grok","status":"online","executablePath":"x","launchArgs":["agent","stdio"]})).unwrap();
    st.db.set_setting("runtimes.disabled", &json!(["grok"])).unwrap();
    let disabled = disabled_runtimes(&st.db);
    let mut roster = vec![
        json!({"id":"rt-test","provider":"codex","status":"offline"}),
        json!({"id":"rt_grok_1","provider":"grok","status":"online","executablePath":"x","launchArgs":["agent","stdio"]}),
    ];
    for row in &mut roster { stamp_runtime_state(row, &disabled); }
    *st.runtimes.write().await = roster;

    // Settings can list it with its switch, and says it is off and found here.
    let listed = dispatch(&st, "provider.list", json!({})).await.unwrap();
    let providers = listed["providers"].as_array().unwrap();
    let grok = providers.iter().find(|row| row["provider"] == "grok").expect("Grok is listed");
    assert_eq!(grok["main"], false);
    assert_eq!(grok["optional"], true);
    assert_eq!(grok["enabled"], false);
    assert_eq!(grok["found"], true);
    assert_eq!(grok["runtimeIds"], json!(["rt_grok_1"]));
    assert!(grok["installHint"].as_str().is_some_and(|hint| !hint.is_empty()), "{grok}");
    assert!(grok["signInCommand"].as_str() == Some("grok login"));
    // A provider that is not installed is still a row, with no runtime.
    assert_eq!(providers.len(), 8);
    // Pi (16 November 2026) is the eighth: its own runtime, not an ACP row, with
    // its install hint beside the rows in `retinue-runtime`.
    let pi = providers.iter().find(|row| row["provider"] == "pi").expect("Pi is listed");
    assert_eq!(pi["optional"], true);
    assert_eq!(pi["found"], false);
    assert!(pi["installHint"].as_str().is_some_and(|hint| hint.contains("pi-coding-agent")), "{pi}");
    for main in ["claude", "codex", "opencode"] {
        let row = providers.iter().find(|row| row["provider"] == main).unwrap();
        assert_eq!(row["main"], true, "{main}");
        assert_eq!(row["enabled"], true, "{main}");
        assert_eq!(row["installHint"], Value::Null, "{main} has its guide in the window");
    }
    // Not offered for a new conversation, a new bot, or a hire.
    let conversation = dispatch(&st, "session.new", json!({"projectPath":".","runtimeId":"rt_grok_1"})).await.unwrap_err();
    assert_eq!(conversation.1.code, "provider_unavailable");
    assert_eq!(conversation.1.message, "This coding agent is turned off in Settings \u{203a} Providers.");
    let bot = dispatch(&st, "agent.create", json!({"name":"Grok bot","runtimeId":"rt_grok_1"})).await.unwrap_err();
    assert_eq!(bot.1.code, "provider_unavailable");
    let project = a_project(&st, "Hiring");
    let hired = dispatch(&st, "project.hire", json!({"projectId":project,"presetId":"ada","runtimeId":"rt_grok_1"})).await.unwrap_err();
    assert_eq!(hired.1.code, "provider_unavailable");
    // A bot that already exists on it keeps it, and its own save is not refused.
    let existing = st.db.agent_create(&json!({"name":"Existing","runtimeId":"rt_grok_1"})).unwrap().0;
    let saved = dispatch(&st, "agent.update", json!({"agentId":existing["id"],"runtimeId":"rt_grok_1","description":"Keeps the code readable"})).await.unwrap();
    assert_eq!(saved["agent"]["description"], "Keeps the code readable");
    assert_eq!(saved["agent"]["runtimeId"], "rt_grok_1");
    // Turning it on again makes it a choice: the same row now reports enabled.
    st.db.set_setting("runtimes.disabled", &json!([])).unwrap();
    {
        let mut runtimes = st.runtimes.write().await;
        for row in runtimes.iter_mut() { stamp_runtime_state(row, &[]); }
    }
    let listed = dispatch(&st, "provider.list", json!({})).await.unwrap();
    let grok = listed["providers"].as_array().unwrap().iter().find(|row| row["provider"] == "grok").cloned().unwrap();
    assert_eq!(grok["enabled"], true);
    assert!(dispatch(&st, "agent.create", json!({"name":"Grok bot","runtimeId":"rt_grok_1"})).await.is_ok(), "the refusal follows the switch");
}

// A provider the owner turned off is not a failure (owner report, 15 November
// 2026): the one-time probe of a provider with no saved row may fail, but the
// row that is stored and published carries no probe failure, so `runtime.list`,
// Settings and the Doctor agree that the agent is simply off.
#[tokio::test]
async fn a_turned_off_provider_keeps_no_probe_failure_on_its_row() {
    let (st, _events) = state();
    st.db.set_setting("runtimes.disabled", &json!(["cursor"])).unwrap();
    let failed: retinue_runtime::DiscoveredRuntime = serde_json::from_value(json!({
        "id": "rt_cursor_1", "hostId": "host_windows_local", "provider": "cursor", "displayName": "Cursor CLI",
        "protocolFamily": "acp", "executablePath": "C:/tools/cursor.exe", "launchArgs": ["--acp"],
        "version": "1.2.3", "authState": "unknown", "gatewayAuthStates": {},
        "status": "error", "capabilities": {},
        "probeError": "The CLI does not advertise the required protocol.", "probeStep": "Protocol check"
    })).unwrap();
    let values = publish_runtime_discovery(&st, vec![failed]).await.unwrap();
    let row = values.iter().find(|row| row["id"] == "rt_cursor_1").expect("the off row is published");
    assert_eq!(row["status"], "disabled");
    assert_eq!(row["enabled"], false);
    assert!(row.get("probeError").is_none(), "{row}");
    assert!(row.get("probeStep").is_none(), "{row}");
    // What the probe found is kept, so Settings can still say it is installed.
    assert_eq!(row["version"], "1.2.3");
    assert_eq!(row["executablePath"], "C:/tools/cursor.exe");
    // The stored row and `runtime.list` read the same way.
    let stored = st.db.runtimes().unwrap().into_iter().find(|row| row["id"] == "rt_cursor_1").unwrap();
    assert!(stored.get("probeError").is_none() && stored.get("probeStep").is_none(), "{stored}");
    let listed = dispatch(&st, "runtime.list", json!({})).await.unwrap();
    let listed = listed["runtimes"].as_array().unwrap().iter().find(|row| row["id"] == "rt_cursor_1").cloned().unwrap();
    assert_eq!(listed["status"], "disabled");
    assert!(listed.get("probeError").is_none(), "{listed}");
    // A row an older build saved with the failure on it is cleaned the next time
    // it is stamped, and turning the provider back on leaves it for the next scan.
    let mut stale = stored.clone();
    stale["probeError"] = json!("The CLI does not advertise the required protocol.");
    stale["probeStep"] = json!("Protocol check");
    stamp_runtime_state(&mut stale, &["cursor".to_owned()]);
    assert!(stale.get("probeError").is_none() && stale.get("probeStep").is_none(), "{stale}");
    stamp_runtime_state(&mut stale, &[]);
    assert_eq!(stale["status"], "offline");
    assert!(stale.get("probeError").is_none(), "{stale}");
}
