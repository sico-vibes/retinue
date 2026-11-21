// 21 November 2026: the composer's follow-up suggestion.
//
// What these tests prove: the setting is on by default and validated at both
// layers; every skip condition is named; a completed turn asks a separate CLI
// process exactly once and holds one line; the owner's own send, a dismissal and
// a second turn all clear it; one generation per conversation and a late answer
// that arrives after a send is dropped; a call that runs too long is stopped;
// and the suggestion text is never a row in the database.
//
// Included into the daemon's own test module, so `state`, `dispatch`, `json!`,
// `Value`, `Instant`, `Duration`, `PathBuf`, `broadcast` and `EventEnvelope` are
// in scope from `phase2a_tests`.

/// A provider CLI replaced by a script the test controls. It records its
/// arguments, keeps the prompt it was given, answers with `answer.txt`, and can
/// be told to take about five seconds.
struct SuggestionStub {
    dir: PathBuf,
}

impl SuggestionStub {
    fn install(label: &str) -> Self {
        let dir = p1b_dir(&format!("followup-{label}"));
        #[cfg(windows)]
        let (file, script) = ("claude.cmd", [
            "@echo off",
            "echo %* >> \"%~dp0calls.txt\"",
            "findstr \"^\" > \"%~dp0stdin.txt\"",
            "if exist \"%~dp0slow.flag\" ping -n 6 127.0.0.1 > nul",
            "if exist \"%~dp0answer.txt\" type \"%~dp0answer.txt\"",
            "exit /b 0",
        ].join("\r\n"));
        #[cfg(unix)]
        let (file, script) = ("claude", [
            "#!/bin/sh",
            "d=$(dirname \"$0\")",
            "echo \"$@\" >> \"$d/calls.txt\"",
            "cat > \"$d/stdin.txt\"",
            "[ -e \"$d/slow.flag\" ] && sleep 5",
            "[ -e \"$d/answer.txt\" ] && cat \"$d/answer.txt\"",
            "exit 0",
        ].join("\n"));
        std::fs::write(dir.join(file), script).unwrap();
        #[cfg(unix)]
        { use std::os::unix::fs::PermissionsExt; std::fs::set_permissions(dir.join(file), std::fs::Permissions::from_mode(0o755)).unwrap(); }
        Self { dir }
    }

    fn program(&self) -> PathBuf {
        if cfg!(windows) { self.dir.join("claude.cmd") } else { self.dir.join("claude") }
    }

    fn answer(&self, text: &str) { std::fs::write(self.dir.join("answer.txt"), text).unwrap(); }
    fn slow(&self) { std::fs::write(self.dir.join("slow.flag"), "x").unwrap(); }
    fn calls(&self) -> String { std::fs::read_to_string(self.dir.join("calls.txt")).unwrap_or_default() }
    fn stdin(&self) -> String { std::fs::read_to_string(self.dir.join("stdin.txt")).unwrap_or_default() }
}

impl Drop for SuggestionStub {
    fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.dir); }
}

/// A state with one Claude conversation, a stub CLI as its runtime, a Haiku in
/// the catalog and the window looking at it.
struct Fixture {
    st: AppState,
    events: broadcast::Receiver<EventEnvelope>,
    stub: SuggestionStub,
    runtime: Value,
}

const SESSION: &str = "follow-up-session";
const TURN: &str = "follow-up-turn";

impl Fixture {
    fn new(label: &str, answer: &str) -> Self {
        let (st, events) = state();
        let stub = SuggestionStub::install(label);
        stub.answer(answer);
        let runtime = json!({"id":"rt-follow-up","provider":"claude","status":"online","executablePath": stub.program().to_string_lossy()});
        let fixture = Self { st, events, stub, runtime };
        fixture.st.db.create_session(SESSION, "rt-follow-up", "claude", ".", "Chat").unwrap();
        fixture.st.db.update_session_state(SESSION, "idle").unwrap();
        fixture.st.db.create_turn(TURN, SESSION).unwrap();
        fixture.st.db.append_message_with_data(SESSION, TURN, "user", "Make the parser handle empty files", &json!({"meta":{"kind":"text"}})).unwrap();
        fixture.st.db.append_message(SESSION, TURN, "assistant", "Done. The parser returns an empty list now.").unwrap();
        fixture
    }

    async fn ready(&self) {
        *self.st.runtimes.write().await = vec![self.runtime.clone()];
        crate::catalog::seed(&self.st, &self.runtime, json!({"models":[{"id":"claude-haiku-5-5","displayName":"Haiku"},{"id":"claude-sonnet-5-5","displayName":"Sonnet"}]}));
        self.st.attention.report_focus(Some(SESSION.to_owned()), true, Instant::now());
    }

    async fn ask(&self) { crate::follow_up::after_turn(&self.st, SESSION, TURN).await; }
}

/// The next event of one type, waiting up to ten seconds for it.
async fn next_kind(events: &mut broadcast::Receiver<EventEnvelope>, kind: &str) -> Value {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        assert!(!remaining.is_zero(), "no {kind} event arrived");
        match tokio::time::timeout(remaining, events.recv()).await {
            Ok(Ok(event)) if event.event_type == kind => return event.payload,
            Ok(Ok(_)) => continue,
            _ => panic!("no {kind} event arrived"),
        }
    }
}

/// Waits for `done`, polling for up to five seconds.
async fn until(done: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if done() { return; }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("the condition never became true");
}

#[tokio::test]
async fn the_setting_is_on_by_default_and_only_a_boolean_is_stored() {
    let (st, mut events) = state();
    assert_eq!(st.db.settings().unwrap().get("chat.followUpSuggestions"), None, "nothing is stored for a new install");
    assert!(crate::follow_up::enabled(&st), "an absent key means on");
    assert_eq!(dispatch(&st, "settings.set", json!({"key":"chat.followUpSuggestions","value":"no"})).await.unwrap_err().1.code, "invalid_argument");
    assert!(st.db.set_setting("chat.followUpSuggestions", &json!("no")).is_err(), "the storage layer refuses it too");
    dispatch(&st, "settings.set", json!({"key":"chat.followUpSuggestions","value":false})).await.unwrap();
    assert!(!crate::follow_up::enabled(&st));
    let changed = next_kind(&mut events, "settings.changed").await;
    assert_eq!(changed, json!({"key":"chat.followUpSuggestions","value":false}), "the window hears the change live");
    dispatch(&st, "settings.set", json!({"key":"chat.followUpSuggestions","value":true})).await.unwrap();
    assert!(crate::follow_up::enabled(&st));
}

#[tokio::test]
async fn every_skip_condition_is_named() {
    let (st, _events) = state();
    st.db.create_session("skip", "rt-test", "codex", ".", "Chat").unwrap();
    st.db.update_session_state("skip", "idle").unwrap();
    st.db.create_turn("skip-turn", "skip").unwrap();
    st.db.append_message_with_data("skip", "skip-turn", "user", "Hello", &json!({"meta":{"kind":"text"}})).unwrap();
    let now = Instant::now();
    let reason = |meta: &Value| {
        let summary = st.db.session_event_summary("skip").unwrap();
        crate::follow_up::skip_reason(&st, "skip", &summary, meta, now)
    };
    let meta = json!({"kind":"text"});

    assert_eq!(reason(&meta), Some("unfocused"), "a conversation nobody is looking at costs nothing");
    st.attention.report_focus(Some("skip".to_owned()), true, now);
    assert_eq!(reason(&meta), None, "with the window on it, everything else is clear");

    dispatch(&st, "settings.set", json!({"key":"chat.followUpSuggestions","value":false})).await.unwrap();
    assert_eq!(reason(&meta), Some("off"));
    dispatch(&st, "settings.set", json!({"key":"chat.followUpSuggestions","value":true})).await.unwrap();

    st.db.set_session_link("skip", &json!({"kind":"automation","automationId":"a","runId":"r"})).unwrap();
    assert_eq!(reason(&meta), Some("unattended"));
    st.db.set_session_link("skip", &json!({"kind":"task","unattended":true})).unwrap();
    assert_eq!(reason(&meta), Some("unattended"), "a scheduled task run is unattended too");
    st.db.set_session_link("skip", &json!({"kind":"side","peerAgentId":"peer","originSessionId":"origin"})).unwrap();
    assert_eq!(reason(&meta), Some("side"));
    st.db.set_session_link("skip", &json!({"startedFrom":{"deviceId":"d","deviceKind":"phone","machineName":"Laptop"}})).unwrap();
    assert_eq!(reason(&meta), Some("remote"));
    st.db.set_session_link("skip", &Value::Null).unwrap();
    assert_eq!(reason(&json!({"kind":"channel","channel":"telegram"})), Some("channel"));

    st.db.set_session_model_lock("skip", &json!({"model":null,"thinking":null,"planMode":true})).unwrap();
    assert_eq!(reason(&meta), Some("plan"));
    st.db.set_session_model_lock("skip", &json!({"model":null,"thinking":null})).unwrap();

    st.db.update_session_state("skip", "limited").unwrap();
    assert_eq!(reason(&meta), Some("limited"));
    st.db.update_session_state("skip", "idle").unwrap();

    let snapshot = json!({"provider":"codex","runtimeId":"rt-test","fetchedAt":"2026-11-21T00:00:00Z","ordinaryUsageAllowed":true,
        "limits":[{"label":"5h","primary":{"usedPercent":95,"remainingPercent":5,"resetsAt":null,"windowDurationMins":300},"secondary":null}]});
    st.db.save_quota_snapshot("codex", "rt-test", &snapshot, "2026-11-21T00:00:00Z").unwrap();
    assert_eq!(reason(&meta), Some("near_limit"), "a quota at 95% is near its limit");

    st.db.save_quota_snapshot("codex", "rt-test", &json!({"limits":[{"label":"5h","primary":{"usedPercent":10}}]}), "2026-11-21T00:00:00Z").unwrap();
    assert_eq!(reason(&meta), None, "a quiet quota blocks nothing");

    st.db.insert_question("q1", "skip", "mcp:q1", "A question", &json!([{"id":"a","question":"Which one?","options":[{"label":"One"}]}]), None).unwrap();
    assert_eq!(reason(&meta), Some("question"), "a bot waiting for an answer is not given a new prompt");
}

#[tokio::test]
async fn a_completed_turn_asks_once_and_holds_one_line_the_database_never_keeps() {
    let mut fixture = Fixture::new("asked", "\"Add a test for the empty case\"\n");
    fixture.ready().await;
    fixture.ask().await;
    let suggestion = next_kind(&mut fixture.events, "session.suggestion").await;
    assert_eq!(suggestion["sessionId"], json!(SESSION));
    assert_eq!(suggestion["turnId"], json!(TURN));
    assert_eq!(suggestion["text"], json!("Add a test for the empty case"), "quotes are trimmed");

    // The reloaded window asks for it, and gets the same turn and text.
    let read = dispatch(&fixture.st, "session.suggestion.get", json!({"sessionId":SESSION})).await.unwrap();
    assert_eq!(read, json!({"turnId":TURN,"text":"Add a test for the empty case"}));
    assert_eq!(dispatch(&fixture.st, "session.suggestion.get", json!({"sessionId":"nobody"})).await.unwrap(), Value::Null);
    assert_eq!(dispatch(&fixture.st, "session.suggestion.get", json!({})).await.unwrap_err().1.code, "invalid_argument");

    // One non-persisted Claude call, the cheapest model named, and only the last
    // message and reply in the prompt.
    until(|| !fixture.stub.calls().is_empty()).await;
    let calls = fixture.stub.calls();
    assert!(calls.contains("--no-session-persistence"), "{calls}");
    assert!(calls.contains("--output-format json"), "{calls}");
    assert!(calls.contains("--model claude-haiku-5-5"), "the cheapest model the runtime reports: {calls}");
    let sent = fixture.stub.stdin();
    assert!(sent.contains("Make the parser handle empty files"), "the owner's last message is included");
    assert!(sent.contains("Done. The parser returns an empty list now."), "the last reply is included");
    assert!(!sent.contains("turn-1"), "no ids or whole transcript leave for this call");

    // Nothing about the suggestion is a row: the marker takes the sequence, and
    // the text itself exists only in the event and in memory.
    let replay = fixture.st.db.replay_events(0, 1_000).unwrap();
    assert!(replay.iter().any(|event| event["type"] == "session.suggestion.ready"), "the content-free marker is the stored half");
    assert!(
        !replay.iter().any(|event| event.to_string().contains("Add a test for the empty case")),
        "the suggestion text is never stored"
    );

    // The owner sends or dismisses: the text goes, and the window is told.
    let dismiss = dispatch(&fixture.st, "session.suggestion.dismiss", json!({"sessionId":SESSION})).await.unwrap();
    assert_eq!(dismiss, json!({"sessionId":SESSION,"cleared":true}));
    assert_eq!(next_kind(&mut fixture.events, "session.suggestion.cleared").await, json!({"sessionId":SESSION}));
    assert_eq!(dispatch(&fixture.st, "session.suggestion.get", json!({"sessionId":SESSION})).await.unwrap(), Value::Null);
    let again = dispatch(&fixture.st, "session.suggestion.dismiss", json!({"sessionId":SESSION})).await.unwrap();
    assert_eq!(again["cleared"], false, "dismissing nothing announces nothing");
}

#[tokio::test]
async fn one_generation_per_conversation_and_a_send_drops_the_late_answer() {
    let mut fixture = Fixture::new("inflight", "Rewrite the header");
    fixture.ready().await;
    fixture.stub.slow();
    fixture.ask().await;
    until(|| !fixture.stub.calls().is_empty()).await;
    // A second turn completing while the first answer is still being written
    // does not start a second call.
    fixture.ask().await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(fixture.stub.calls().lines().count(), 1, "one in flight per conversation");
    assert_eq!(dispatch(&fixture.st, "session.suggestion.get", json!({"sessionId":SESSION})).await.unwrap(), Value::Null);

    // The owner sends: the late answer is dropped, and nothing is announced.
    assert!(!crate::follow_up::clear(&fixture.st, SESSION).await, "there was nothing to show, so nothing to clear");
    let late = tokio::time::timeout(Duration::from_millis(1_200), fixture.events.recv()).await;
    assert!(late.is_err(), "a cancelled generation sends no suggestion: {late:?}");
    assert_eq!(dispatch(&fixture.st, "session.suggestion.get", json!({"sessionId":SESSION})).await.unwrap(), Value::Null);
}

#[tokio::test]
async fn a_call_that_runs_too_long_is_stopped_and_answers_nothing() {
    let fixture = Fixture::new("timeout", "This answer arrives too late");
    fixture.ready().await;
    fixture.stub.slow();
    let summary = fixture.st.db.session_event_summary(SESSION).unwrap();
    let ask = crate::follow_up::ask_for(&fixture.st, &summary, &fixture.runtime, "Ask the question".to_owned()).expect("claude has a clean one-shot call");
    let started = Instant::now();
    let answer = crate::follow_up::answer_with(&fixture.st, &ask, Duration::from_millis(400)).await;
    assert!(answer.is_none(), "a call past its limit answers nothing");
    assert!(started.elapsed() < Duration::from_secs(4), "and is stopped rather than waited for: {:?}", started.elapsed());
}

#[tokio::test]
async fn a_runtime_with_no_clean_side_call_is_skipped() {
    let (st, _events) = state();
    for provider in ["opencode", "pi", "grok"] {
        let summary = json!({"provider":provider,"agentId":""});
        let runtime = json!({"id":"rt-other","provider":provider,"executablePath":"opencode"});
        assert!(crate::follow_up::ask_for(&st, &summary, &runtime, "Ask".to_owned()).is_none(), "{provider} has no stdin one-shot, so it gets no suggestion");
    }
}
