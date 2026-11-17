// Wave 2 (nod adoption, 17 November 2026): the reach statement and the paused
// model rail, against a real stored conversation in an in-memory database.
//
// Included into the daemon's test module (`main.rs`), so `dispatch`, the state
// helper and every module are in scope.

/// The statement reads the conversation's own folder and mode, and a bot alone
/// still gets an answer.
#[tokio::test]
async fn reach_statement_reads_the_conversations_own_folder_and_mode() {
    let (st, _) = phase2a_tests::state();
    let agent = st.db.agent_create(&json!({"name":"Ada","runtimeId":"rt-test","approvalMode":"accept_edits"})).unwrap().0;
    let id = agent["id"].as_str().unwrap().to_owned();
    st.db.create_session_for_agent("ses-reach", "rt-test", "codex", "C:\\work\\project", "Reach", Some(&id)).unwrap();
    let answer = dispatch(&st, "reach.statement", json!({"sessionId":"ses-reach"})).await.unwrap();
    let reach = &answer["reach"];
    assert_eq!(reach["botName"], "Ada");
    assert_eq!(reach["sessionId"], "ses-reach");
    assert_eq!(reach["agentId"], id);
    assert_eq!(reach["folder"], "C:\\work\\project");
    assert_eq!(reach["isolation"], "shared");
    assert_eq!(reach["permissionMode"], "accept_edits");
    assert_eq!(reach["unattended"], false);
    assert_eq!(reach["tools"][0]["on"], true, "a conversation with a folder has file tools");
    assert!(reach["statement"].as_str().unwrap().contains("C:\\work\\project"));
    assert!(!reach["statement"].as_str().unwrap().contains('*'), "plain words, no markup");
    // A bot on its own still answers, and claims no conversation.
    let by_bot = dispatch(&st, "reach.statement", json!({"agentId": id})).await.unwrap();
    assert_eq!(by_bot["reach"]["botName"], "Ada");
    assert!(by_bot["reach"]["sessionId"].is_null());
    // Neither id is a caller error; an unknown conversation is not found.
    assert_eq!(dispatch(&st, "reach.statement", json!({})).await.unwrap_err().1.code, "invalid_argument");
    assert_eq!(dispatch(&st, "reach.statement", json!({"sessionId":"nope"})).await.unwrap_err().1.code, "not_found");
}

/// Two failures pause unattended work on that model, and never the owner's own
/// message; a completed turn clears it.
#[tokio::test]
async fn a_paused_model_refuses_unattended_work_and_never_the_owner() {
    let (st, _) = phase2a_tests::state();
    let agent = st.db.agent_create(&json!({"name":"Ada","runtimeId":"rt-test","model":"gpt-6-luna"})).unwrap().0;
    let id = agent["id"].as_str().unwrap().to_owned();
    st.db.create_session_for_agent("ses-paused", "rt-test", "codex", ".", "Run", Some(&id)).unwrap();
    st.db.set_session_link("ses-paused", &json!({"kind":"automation","automationId":"a","runId":"r"})).unwrap();
    st.db.update_session_state("ses-paused", "idle").unwrap();
    // One failure is a hiccup: nothing is paused yet.
    model_health::record_failure(&st, "codex", "gpt-6-luna", Some("The provider reported a turn error."));
    assert!(st.db.model_health_get("codex", "gpt-6-luna").unwrap().unwrap()["pausedUntil"].is_null());
    // The second in a row pauses it.
    model_health::record_failure(&st, "codex", "gpt-6-luna", Some("The provider reported a turn error."));
    let row = st.db.session_execution_row("ses-paused").unwrap();
    let refusal = model_health::refusal(&st, "ses-paused", &row["link"]).expect("unattended work is refused");
    assert_eq!(refusal.1.code, "model_paused");
    assert!(refusal.1.message.contains("paused until"), "{}", refusal.1.message);
    // The owner's own message in the same conversation always runs.
    assert!(model_health::refusal(&st, "ses-paused", &json!({"kind":"main"})).is_none());
    // A completed turn forgets the model entirely.
    model_health::note_success(&st, "ses-paused");
    assert!(st.db.model_health_get("codex", "gpt-6-luna").unwrap().is_none());
}

/// The ledger only counts the model's own trouble, so a cancelled or timed-out
/// turn never pauses scheduled work.
#[tokio::test]
async fn a_cancelled_or_timed_out_turn_is_not_recorded_against_the_model() {
    let (st, _) = phase2a_tests::state();
    let agent = st.db.agent_create(&json!({"name":"Ada","runtimeId":"rt-test","model":"gpt-6-luna"})).unwrap().0;
    let id = agent["id"].as_str().unwrap().to_owned();
    st.db.create_session_for_agent("ses-quiet", "rt-test", "codex", ".", "Quiet", Some(&id)).unwrap();
    for class in ["timeout", "context", "permission_denied", "cancelled"] {
        model_health::note_failure(&st, "ses-quiet", class, None);
    }
    assert!(st.db.model_health_all().unwrap().is_empty(), "nothing the model did wrong was recorded");
    model_health::note_failure(&st, "ses-quiet", "provider_unavailable", Some("The coding agent could not start."));
    let row = st.db.model_health_get("codex", "gpt-6-luna").unwrap().expect("a recorded failure");
    assert_eq!(row["failures"], 1);
    assert_eq!(row["reason"], "The coding agent could not start.");
}
