// OpenUI guide tool, `manager.troubleshoot` and the CEO's `open_issue` card.
// Included into `phase2a_tests` so it shares that module's fixtures.

async fn ceo_session(st: &AppState) -> String {
    let boss = dispatch(st, "manager.ensure", json!({"runtimeId":"rt-test"})).await.unwrap()["manager"].clone();
    let boss_id = boss["id"].as_str().unwrap().to_owned();
    st.db.create_session_for_agent("ceo-ts", "rt-test", "codex", ".", "CEO", Some(&boss_id)).unwrap();
    st.db.update_session_state("ceo-ts", "idle").unwrap();
    "ceo-ts".into()
}

#[tokio::test]
async fn ui_guide_is_offered_to_every_bot_and_returns_the_guide() {
    let (st, _events) = state();
    let ceo = ceo_session(&st).await;
    let bot = st.db.agent_create(&json!({"name":"Plain","runtimeId":"rt-test"})).unwrap().0["id"].as_str().unwrap().to_owned();
    st.db.create_session_for_agent("plain-ts", "rt-test", "codex", ".", "Chat", Some(&bot)).unwrap();
    st.db.update_session_state("plain-ts", "idle").unwrap();
    for session in [ceo.as_str(), "plain-ts"] {
        let tools = tool_names(&team::mcp_rpc(&st, session, "mcp.tools.list", &json!({})).await.unwrap());
        assert!(tools.contains(&"ui_guide".to_owned()), "{session} is offered ui_guide");
        let result = call_tool(&st, session, "ui_guide", json!({})).await;
        assert_ne!(result["isError"], true, "{result}");
        assert_eq!(result["content"][0]["text"], team::OPENUI_GUIDE);
    }
    let plain = tool_names(&team::mcp_rpc(&st, "plain-ts", "mcp.tools.list", &json!({})).await.unwrap());
    assert!(!plain.contains(&"open_issue".to_owned()), "open_issue is the CEO's only");
    assert_eq!(call_tool(&st, "plain-ts", "open_issue", json!({"title":"t","body":"b"})).await["isError"], true);
}

#[tokio::test]
async fn the_ceo_issue_card_posts_nothing_until_approved_and_decline_posts_nothing() {
    let (st, _events) = state();
    let ceo = ceo_session(&st).await;
    let tools = tool_names(&team::mcp_rpc(&st, &ceo, "mcp.tools.list", &json!({})).await.unwrap());
    assert!(tools.contains(&"open_issue".to_owned()));
    let made = call_tool(&st, &ceo, "open_issue", json!({"title":"Crash on start","body":"Steps:\nopen C:\\Users\\Someone\\x\npassword: hunter2"})).await;
    assert_ne!(made["isError"], true, "{made}");
    let pending = st.db.proposals_pending().unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0]["kind"], "task");
    assert_eq!(pending[0]["payload"]["action"], "issue");
    let body = pending[0]["payload"]["body"].as_str().unwrap();
    assert!(!body.contains("hunter2"), "secrets are cleaned before the card shows them");
    let card = st.db.permission_get(pending[0]["id"].as_str().unwrap()).unwrap();
    assert_eq!(card["title"], "Open a GitHub issue: Crash on start");
    assert_eq!(card["detail"], body);
    // Empty, too long and unknown fields are refused before any card exists.
    assert_eq!(call_tool(&st, &ceo, "open_issue", json!({"title":"","body":"b"})).await["isError"], true);
    assert_eq!(call_tool(&st, &ceo, "open_issue", json!({"title":"t","body":"x".repeat(8001)})).await["isError"], true);
    assert_eq!(call_tool(&st, &ceo, "open_issue", json!({"title":"t","body":"b","repo":"other/repo"})).await["isError"], true);
    assert_eq!(st.db.proposals_pending().unwrap().len(), 1);
    // Declining leaves nothing posted and the proposal rejected.
    let id = pending[0]["id"].as_str().unwrap().to_owned();
    let rejected = dispatch(&st, "proposal.decide", json!({"id":id,"decision":"reject"})).await.unwrap();
    assert_eq!(rejected["decision"], "rejected");
    assert!(st.db.proposals_pending().unwrap().is_empty());
}

#[tokio::test]
async fn troubleshoot_refuses_without_a_ceo_or_a_failed_turn_and_reports_facts_only() {
    let (st, _events) = state();
    let bot = st.db.agent_create(&json!({"name":"Ada","role":"Backend engineer","runtimeId":"rt-test"})).unwrap().0["id"].as_str().unwrap().to_owned();
    st.db.create_session_for_agent("failed-ts", "rt-test", "codex", ".", "Fix it", Some(&bot)).unwrap();
    st.db.create_turn("turn-ok", "failed-ts").unwrap();
    st.db.update_turn_outcome("turn-ok", "completed", None).unwrap();
    st.db.create_turn("turn-bad", "failed-ts").unwrap();
    st.db.update_turn_outcome("turn-bad", "error", Some("provider_error")).unwrap();
    st.db.merge_turn_data("turn-bad", &json!({"hint":{"kind":"overloaded","retryAfterSecs":30}})).unwrap();
    for (id, title, tool_state) in [("t1", "read_file", "completed"), ("t2", "run_tests", "failed"), ("t3", "edit_file", "completed"), ("t4", "grep", "failed")] {
        st.db.upsert_tool_in_turn("failed-ts", Some("turn-bad"), id, "execute", title, tool_state, &json!({"output":"SECRET OUTPUT"}), None).unwrap();
    }
    let no_ceo = dispatch(&st, "manager.troubleshoot", json!({"sessionId":"failed-ts"})).await.unwrap_err();
    assert_eq!(no_ceo.1.code, "conflict");
    assert!(no_ceo.1.message.contains("no CEO"));
    dispatch(&st, "manager.ensure", json!({"runtimeId":"rt-test"})).await.unwrap();
    assert_eq!(dispatch(&st, "manager.troubleshoot", json!({})).await.unwrap_err().1.code, "invalid_argument");
    assert_eq!(dispatch(&st, "manager.troubleshoot", json!({"sessionId":"missing"})).await.unwrap_err().1.code, "not_found");
    assert_eq!(dispatch(&st, "manager.troubleshoot", json!({"sessionId":"failed-ts","turnId":"nope"})).await.unwrap_err().1.code, "not_found");
    let session = st.db.session_detail("failed-ts").unwrap();
    let turn = troubleshoot::pick_turn(&session, None).unwrap();
    assert_eq!(turn["id"], "turn-bad", "the latest failed turn is the default");
    let text = troubleshoot::report(&st, &session, &turn).await;
    for needle in ["Employee: Ada (Backend engineer)", "Coding agent: codex", "Conversation id: failed-ts", "Turn id: turn-bad", "Failure class: provider_error", "Hint: overloaded, retry after 30 s", "Retinue version:"] {
        assert!(text.contains(needle), "{needle} in {text}");
    }
    assert!(!text.contains("SECRET OUTPUT") && !text.contains("read_file"), "tool output is never included, only the last three calls: {text}");
    assert!(text.contains("run_tests: failed") && text.contains("grep: failed"));
}
