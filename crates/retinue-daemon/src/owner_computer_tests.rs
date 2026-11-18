// The owner's own Windows computer (Phase C). Included into the daemon's test
// module so it shares that module's fixtures: `state`, `dispatch`, `json!`,
// `Value`, `Arc` and `Uuid`. Every test here stops before a real driver runs,
// except the two that touch the driver's own files.

use crate::owner_computer::{Limits, OwnerComputer};
use std::time::Duration;

/// A conversation a bot can be asked in: no link, no archive, Ask mode.
fn attended(st: &AppState, id: &str) {
    st.db.create_session(id, "rt-test", "codex", ".", "Chat").unwrap();
    st.db.update_session_state(id, "idle").unwrap();
}

fn owner_tool_names() -> Vec<String> {
    owner_computer::tool_schemas().iter().filter_map(|tool| tool["name"].as_str().map(str::to_owned)).collect()
}

#[test]
fn off_by_default_and_no_screenshot_tool_exists() {
    let (st, _) = state();
    assert!(!owner_computer::enabled(&st), "the switch is off until the owner turns it on");
    let names = owner_tool_names();
    for expected in ["owner_apps", "owner_screen_state", "owner_focus", "owner_click", "owner_type", "owner_key", "owner_scroll", "owner_stop"] {
        assert!(names.iter().any(|name| name == expected), "{expected} is offered");
    }
    assert!(!names.iter().any(|name| name == "owner_screenshot"), "no frame of the owner's screen in this cut: {names:?}");
    assert!(owner_computer::tool_schemas().iter().all(|tool| tool["inputSchema"]["additionalProperties"] == json!(false)));
}

#[tokio::test]
async fn the_server_is_not_offered_at_all_while_the_switch_is_off() {
    let (st, _) = state();
    let dir = std::env::temp_dir().join(format!("retinue-owner-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    *st.team.data_dir.lock().unwrap() = Some(dir.clone());
    *st.team.address.lock().unwrap() = Some("127.0.0.1:9".to_owned());

    assert!(owner_computer::mcp_server_for(&st, "session-one").is_none(), "off means no server, so no tools");
    assert!(owner_computer::mcp_rpc(&st, "session-one", "mcp.owner.list", &json!({})).await.unwrap()["tools"].as_array().unwrap().is_empty());

    st.db.set_setting(owner_computer::SETTING, &json!(true)).unwrap();
    // Off Windows there is no owner's computer to drive: the switch can be on and the tools are
    // still refused, by design, rather than offered where they cannot work. Both branches are
    // written with `cfg!` so a Windows check still typechecks the Linux answer.
    if cfg!(windows) {
        let spec = owner_computer::mcp_server_for(&st, "session-one").expect("on means the tools exist");
        assert_eq!(spec.name, "owner-computer");
        assert!(spec.env.is_empty(), "no token in the environment");
        assert_eq!(spec.args.first().map(String::as_str), Some("owner-server"));
        let endpoint = std::path::PathBuf::from(spec.args.get(2).expect("an endpoint path"));
        let written = std::fs::read_to_string(&endpoint).unwrap();
        assert!(written.contains("\"address\":\"127.0.0.1:9\""), "{written}");
        assert!(!written.contains("daemon-capability-sentinel"), "{written}");
    } else {
        assert!(owner_computer::mcp_server_for(&st, "session-one").is_none(), "a host that is not Windows offers no owner computer");
        assert!(owner_computer::mcp_rpc(&st, "session-one", "mcp.owner.list", &json!({})).await.unwrap()["tools"].as_array().unwrap().is_empty());
        let call = owner_computer::mcp_rpc(&st, "session-one", "mcp.owner.call", &json!({"name":"owner_apps","arguments":{}})).await.unwrap();
        assert_eq!(call["isError"], true);
        assert_eq!(call["content"][0]["text"], "The owner's computer is a Windows computer. This host cannot control one.");
        assert!(!dir.join("owner-computer").exists(), "no endpoint file is written for a host that cannot use one");
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn a_call_needs_the_owners_card_and_the_card_is_never_rememberable() {
    let (st, mut events) = state();
    let mut st = st;
    st.owner_computer = Arc::new(OwnerComputer::with_limits(Limits { consent: Duration::from_millis(30), ..Limits::default() }));
    st.db.set_setting(owner_computer::SETTING, &json!(true)).unwrap();
    attended(&st, "consent-one");

    let refused = owner_computer::call_tool(&st, "consent-one", "owner_apps", &json!({})).await.unwrap_err();
    assert!(refused.contains("has not answered"), "{refused}");

    let mut card = None;
    while let Ok(event) = events.try_recv() {
        if event.event_type == "permission.requested" {
            card = Some(event.payload);
        }
    }
    let card = card.expect("a card was raised");
    assert_eq!(card["kind"], "owner_computer");
    assert_eq!(card["rememberable"], false, "the computer is never a remembered rule");
    assert_eq!(card["choices"], json!(["allow", "allow_once", "deny"]));
    assert!(card["title"].as_str().unwrap().contains("control this computer"), "{card}");
    assert!(card["detail"].as_str().unwrap().contains("cannot take a picture"), "the card says what it will not do: {card}");

    // The owner answers: the grant starts, and no rule is written.
    let id = card["id"].as_str().unwrap().to_owned();
    let answered = owner_computer::reply_consent(&st, &id, "consent-one", "allow").await.unwrap();
    assert_eq!(answered["allowed"], json!(true));
    assert!(st.owner_computer.live("consent-one").is_ok(), "the answer started the grant");
    let live = events.recv().await.unwrap();
    assert_eq!(live.event_type, "owner_computer.grant");
    assert_eq!(live.payload["state"], "live");
    assert!(live.payload["endsAt"].as_str().is_some());
    assert!(st.db.permission_rules(None).unwrap().is_empty(), "nothing was remembered");
}

#[tokio::test]
async fn the_grant_expires_and_the_expiry_is_announced() {
    let (st, mut events) = state();
    let mut st = st;
    st.owner_computer = Arc::new(OwnerComputer::with_limits(Limits { grant: Duration::from_millis(10), ..Limits::default() }));
    attended(&st, "grants-one");
    st.owner_computer.begin("grants-one", "agent-one", false, "turn-one");
    assert!(st.owner_computer.live("grants-one").is_ok());
    tokio::time::sleep(Duration::from_millis(25)).await;
    let reason = st.owner_computer.live("grants-one").unwrap_err();
    assert!(reason.contains("five minutes"), "{reason}");
    assert!(st.owner_computer.live("grants-one").is_err(), "the grant is gone, not renewed");
    owner_computer::end_grant(&st, "grants-one", None).await;
    let mut ended = false;
    while let Ok(event) = events.try_recv() {
        if event.event_type == "owner_computer.grant" && event.payload["state"] == "ended" {
            assert_eq!(event.payload["sessionId"], "grants-one");
            assert_eq!(event.payload["endsAt"], Value::Null);
            ended = true;
        }
    }
    assert!(ended, "the bar is told the grant ended");
}

#[tokio::test]
async fn the_idle_clock_ends_a_grant_after_no_activity() {
    let _ = state();
    let held = OwnerComputer::with_limits(Limits { idle: Duration::from_millis(5), ..Limits::default() });
    held.begin("idle-one", "agent-one", false, "turn-one");
    assert!(held.live("idle-one").is_ok());
    tokio::time::sleep(Duration::from_millis(20)).await;
    let reason = held.live("idle-one").unwrap_err();
    assert!(reason.contains("no action"), "{reason}");
}

#[tokio::test]
async fn one_turn_gets_fifteen_minutes_across_renewals() {
    let _ = state();
    let held = OwnerComputer::with_limits(Limits { turn: Duration::from_millis(10), ..Limits::default() });
    assert!(held.turn_open("turn-one", "turn-a"), "the first call opens the turn");
    held.begin("turn-one", "agent-one", false, "turn-a");
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert!(!held.turn_open("turn-one", "turn-a"), "a renewal in the same turn is still inside the cap");
    assert!(held.turn_open("turn-one", "turn-b"), "a new turn starts its own clock");
}

#[tokio::test]
async fn full_access_and_unattended_runs_are_refused_before_any_card() {
    let (st, mut events) = state();
    st.db.set_setting(owner_computer::SETTING, &json!(true)).unwrap();
    let agent = st.db.agent_create(&json!({"name":"Ada","runtimeId":"rt-test","approvalMode":"bypass"})).unwrap().0;
    st.db.create_session_for_agent("full-one", "rt-test", "codex", ".", "Chat", agent["id"].as_str()).unwrap();
    st.db.update_session_state("full-one", "idle").unwrap();
    let refused = owner_computer::call_tool(&st, "full-one", "owner_apps", &json!({})).await.unwrap_err();
    assert!(refused.contains("Full access"), "{refused}");

    st.db.create_session("task-one", "rt-test", "codex", ".", "Task").unwrap();
    st.db.set_session_link("task-one", &json!({"kind":"task","unattended":true})).unwrap();
    let refused = owner_computer::call_tool(&st, "task-one", "owner_apps", &json!({})).await.unwrap_err();
    assert!(refused.contains("nobody is watching"), "{refused}");

    st.db.create_session("side-one", "rt-test", "codex", ".", "Side").unwrap();
    st.db.set_session_link("side-one", &json!({"kind":"side","peerAgentId":"peer","originSessionId":"full-one"})).unwrap();
    let refused = owner_computer::call_tool(&st, "side-one", "owner_apps", &json!({})).await.unwrap_err();
    assert!(refused.contains("nobody is watching"), "{refused}");

    // A turn a teammate's reply woke is nobody's own turn, link or not.
    attended(&st, "woken-one");
    st.team.set_depth("woken-one", 1);
    let refused = owner_computer::call_tool(&st, "woken-one", "owner_apps", &json!({})).await.unwrap_err();
    assert!(refused.contains("nobody is watching"), "{refused}");

    let mut cards = 0;
    while let Ok(event) = events.try_recv() {
        if event.event_type == "permission.requested" { cards += 1; }
    }
    assert_eq!(cards, 0, "a refused conversation is never offered a card");
}

#[tokio::test]
async fn a_switched_off_computer_refuses_every_call() {
    let (st, _) = state();
    attended(&st, "off-one");
    let refused = owner_computer::call_tool(&st, "off-one", "owner_apps", &json!({})).await.unwrap_err();
    assert!(refused.contains("switched off"), "{refused}");
}

#[tokio::test]
async fn the_missing_driver_is_a_clean_refusal_that_names_the_fetch_script() {
    let (st, _) = state();
    st.db.set_setting(owner_computer::SETTING, &json!(true)).unwrap();
    attended(&st, "driver-one");
    st.owner_computer.begin("driver-one", "agent-one", false, "turn-one");
    // The data folder is empty and the daemon's own folder holds no driver here.
    let dir = std::env::temp_dir().join(format!("retinue-owner-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    *st.team.data_dir.lock().unwrap() = Some(dir.clone());
    let refused = owner_computer::call_tool(&st, "driver-one", "owner_apps", &json!({})).await.unwrap_err();
    assert!(refused.contains("fetch-owner-computer-driver.mjs"), "{refused}");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_wrong_binary_is_refused_by_the_hash_and_the_pinned_constant_is_a_real_digest() {
    let dir = std::env::temp_dir().join(format!("retinue-driver-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let swapped = dir.join("open-computer-use.exe");
    std::fs::write(&swapped, b"not the pinned release").unwrap();
    let refused = owner_computer::verify_driver(&swapped).unwrap_err();
    assert!(refused.contains("as Retinue pinned it"), "{refused}");
    for digest in [owner_computer::DRIVER_EXE_SHA256, owner_computer::DRIVER_ZIP_SHA256] {
        assert_eq!(digest.len(), 64);
        assert!(digest.chars().all(|ch| ch.is_ascii_hexdigit()));
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn every_observation_forces_the_limits_and_never_a_screenshot() {
    for tool in ["owner_screen_state", "owner_focus"] {
        let arguments = owner_computer::driver_arguments(tool, "Notepad", None, None, None, None, None);
        assert_eq!(arguments["tool"], "get_app_state");
        assert_eq!(arguments["arguments"]["include_screenshot"], json!(false), "a missing field would default to true");
        assert_eq!(arguments["arguments"]["include_ax"], json!(true));
        assert_eq!(arguments["arguments"]["text_limit"], json!(500));
        assert_eq!(arguments["arguments"]["max_tree_nodes"], json!(1200));
        assert_eq!(arguments["arguments"]["max_tree_depth"], json!(64));
    }
}

#[test]
fn actions_are_element_addressed_on_the_accessibility_path_only() {
    let click = owner_computer::driver_arguments("owner_click", "Notepad", Some(7), None, None, None, None);
    assert_eq!(click["tool"], "click");
    assert_eq!(click["arguments"]["element_index"], json!("7"));
    assert_eq!(click["arguments"]["click_method"], json!("auto"));
    assert!(click["arguments"].get("x").is_none(), "no pixel clicks");
    let key = owner_computer::driver_arguments("owner_key", "Notepad", None, None, Some("Enter"), None, None);
    assert_eq!(key["arguments"]["key_method"], json!("auto"), "window messages, never real keys");
    let typed = owner_computer::driver_arguments("owner_type", "Notepad", Some(2), Some("hello"), None, None, None);
    assert_eq!(typed["tool"], "set_value", "a verified write, not a blind keystroke");
    assert_eq!(typed["arguments"]["value"], json!("hello"));
    assert!(owner_computer::key_is_single("Enter") && !owner_computer::key_is_single("Ctrl+C") && !owner_computer::key_is_single("Ctrl C"));
}

#[test]
fn the_global_pointer_switch_is_stripped_from_the_childs_environment() {
    std::env::set_var(owner_computer::GLOBAL_POINTER_VAR, "1");
    let env = owner_computer::driver_env();
    assert!(!env.contains_key(owner_computer::GLOBAL_POINTER_VAR), "real pointer and keyboard input stay off");
    std::env::remove_var(owner_computer::GLOBAL_POINTER_VAR);
}

#[test]
fn the_starter_deny_list_covers_the_four_places_that_matter_and_an_owner_can_add_to_it() {
    let (st, _) = state();
    let words = owner_computer::deny_list(&st);
    for app in ["1Password", "Bitwarden", "KeePass", "HSBC Online Banking. Personal.", "Outlook", "Windows PowerShell"] {
        assert!(owner_computer::deny_hit(&words, app).is_some(), "{app} is refused");
    }
    for app in ["Notepad", "Google Chrome", "File Explorer"] {
        assert!(owner_computer::deny_hit(&words, app).is_none(), "{app} is allowed");
    }
    st.db.set_setting(owner_computer::DENY_LIST_SETTING, &json!(["Discord"])).unwrap();
    assert_eq!(owner_computer::deny_hit(&owner_computer::deny_list(&st), "Discord"), Some("discord".to_owned()), "the owner's own entry is added");
    assert!(owner_computer::valid_deny_list(&json!(["Discord", "Slack"])));
    assert!(!owner_computer::valid_deny_list(&json!("Discord")));
    assert!(!owner_computer::valid_deny_list(&json!([7])));
    assert!(!owner_computer::valid_deny_list(&json!([format!("{}", "x".repeat(65))])));
    assert!(!owner_computer::valid_deny_list(&json!((0..41).map(|index| format!("w{index}")).collect::<Vec<_>>())));
}

#[test]
fn password_values_are_masked_and_a_normal_value_is_kept() {
    let tree = [
        "\t0 window Sign in Frame: {x: 0, y: 0, width: 800, height: 600}",
        "\t\t2 edit Password Value: hunter2 Secondary Actions: SetValue Frame: {x: 1, y: 2, width: 3, height: 4}",
        "\t\t3 password edit Value: •••• Frame: {x: 1, y: 2, width: 3, height: 4}",
        "\t\t5 edit Value: a bare field Frame: {x: 1, y: 2, width: 3, height: 4}",
        "\t\t4 document Text editor Value: a harmless note Frame: {x: 1, y: 2, width: 3, height: 4}",
    ]
    .join("\n");
    let masked = owner_computer::mask_tree(&tree);
    assert!(!masked.contains("hunter2"), "{masked}");
    assert!(!masked.contains("a bare field"), "a field the app did not name is treated as a secret: {masked}");
    assert!(masked.contains("a harmless note"), "a named document value is still readable: {masked}");
    assert!(masked.contains("••••"));
    let button = "\t\t6 button Save Frame: {x: 1, y: 2, width: 3, height: 4}";
    assert_eq!(owner_computer::mask_tree(button), button);
}

#[tokio::test]
async fn an_action_needs_a_fresh_read_and_a_stale_index_is_refused() {
    let _ = state();
    let held = OwnerComputer::default();
    held.begin("stale-one", "agent-one", false, "turn-one");
    let refused = held.check_fresh("stale-one", "Notepad").unwrap_err();
    assert!(refused.contains("Read the screen"), "{refused}");
    assert!(!held.note("stale-one", "Notepad", true), "a read clears staleness");
    assert!(held.check_fresh("stale-one", "Notepad").is_ok());
    assert!(!held.note("stale-one", "Notepad", false), "an action marks it");
    let refused = held.check_fresh("stale-one", "Notepad").unwrap_err();
    assert!(refused.contains("screen changed"), "an action makes the indexes stale: {refused}");
}

#[tokio::test]
async fn a_once_grant_is_spent_by_the_call_it_allowed() {
    let _ = state();
    let held = OwnerComputer::default();
    held.begin("once-one", "agent-one", true, "turn-one");
    assert!(held.note("once-one", "Notepad", true), "the call that used it is the last");
    held.end("once-one");
    assert!(held.live("once-one").is_err());
}

#[test]
fn the_result_carries_a_count_and_never_the_typed_text() {
    let _ = state();
    let secret = "correct horse battery staple";
    let line = owner_computer::summarise("owner_type", "Notepad", &json!({"element": 2, "text": secret}), false);
    assert_eq!(line, "Typed 28 characters into Notepad.\n");
    assert!(!line.contains(secret), "the transcript carries a count, not the text");
}

#[tokio::test]
async fn stop_ends_every_grant_and_announces_each_one() {
    let (st, mut events) = state();
    attended(&st, "stop-one");
    attended(&st, "stop-two");
    st.owner_computer.begin("stop-one", "agent-one", false, "turn-one");
    st.owner_computer.begin("stop-two", "agent-one", false, "turn-one");
    let stopped = dispatch(&st, "owner_computer.stop", json!({})).await.unwrap();
    assert_eq!(stopped, json!({"stopped": true}));
    assert!(st.owner_computer.live("stop-one").is_err());
    assert!(st.owner_computer.live("stop-two").is_err());
    let mut ended = 0;
    while let Ok(event) = events.try_recv() {
        if event.event_type == "owner_computer.grant" && event.payload["state"] == "ended" { ended += 1; }
    }
    assert_eq!(ended, 2, "each grant's end is announced for the bar");
    // A second Stop has nothing to do, and says so rather than failing.
    assert_eq!(dispatch(&st, "owner_computer.stop", json!({})).await.unwrap(), json!({"stopped": false}));
}

#[tokio::test]
async fn turning_the_switch_off_releases_every_grant_at_once() {
    let (st, mut events) = state();
    attended(&st, "switch-one");
    st.owner_computer.begin("switch-one", "agent-one", false, "turn-one");
    assert!(st.owner_computer.live("switch-one").is_ok());
    dispatch(&st, "settings.set", json!({"key": owner_computer::SETTING, "value": true})).await.unwrap();
    dispatch(&st, "settings.set", json!({"key": owner_computer::SETTING, "value": false})).await.unwrap();
    assert!(st.owner_computer.live("switch-one").is_err(), "the switch going off ends the grant with no card and no warning");
    let mut ended = false;
    while let Ok(event) = events.try_recv() {
        if event.event_type == "owner_computer.grant" && event.payload["state"] == "ended" { ended = true; }
    }
    assert!(ended);
}

#[tokio::test]
async fn the_switch_and_the_deny_list_are_validated_settings() {
    let (st, _) = state();
    assert_eq!(dispatch(&st, "settings.set", json!({"key": owner_computer::SETTING, "value": "yes"})).await.unwrap_err().1.code, "invalid_argument");
    dispatch(&st, "settings.set", json!({"key": owner_computer::SETTING, "value": true})).await.unwrap();
    assert_eq!(st.db.settings().unwrap()[owner_computer::SETTING], json!(true));
    assert_eq!(dispatch(&st, "settings.set", json!({"key": owner_computer::DENY_LIST_SETTING, "value": "Discord"})).await.unwrap_err().1.code, "invalid_argument");
    dispatch(&st, "settings.set", json!({"key": owner_computer::DENY_LIST_SETTING, "value": ["Discord"]})).await.unwrap();
    assert_eq!(st.db.settings().unwrap()[owner_computer::DENY_LIST_SETTING], json!(["Discord"]));
    // The switch is a machine capability, so a backup never carries it.
    assert!(!backup::exportable_setting_for_test(owner_computer::SETTING, &json!(true)));
    assert!(!backup::exportable_setting_for_test(owner_computer::DENY_LIST_SETTING, &json!(["Discord"])));
}

#[test]
fn the_app_list_never_names_a_denied_app() {
    let words = vec!["1password".to_owned(), "outlook".to_owned(), "powershell".to_owned()];
    let listed = ["Notepad -- notepad [running, pid=1, window=Notes]", "1Password -- 1Password [running, pid=2, window=1Password]", "WindowsTerminal -- WindowsTerminal [running, pid=3, window=PowerShell]"].join("\n");
    let shown = owner_computer::hide_denied_apps(&listed, &words).to_lowercase();
    assert!(shown.contains("notepad"), "{shown}");
    assert!(!shown.contains("1password"), "{shown}");
    assert!(!shown.contains("powershell"), "{shown}");
}

#[test]
fn the_window_title_is_read_from_the_drivers_own_header() {
    let text = "App=Notepad (pid 1)\nWindow: \"Notes - Notepad\", App: Notepad.\n\t0 window Notes";
    assert_eq!(owner_computer::window_title(text).as_deref(), Some("Notes - Notepad"));
    assert_eq!(owner_computer::window_title("nothing here"), None);
}
