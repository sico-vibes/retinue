//! The Pi adapter against a fake Pi, the way the ACP adapter's tests drive a
//! fake peer: the real protocol, no installed CLI, no key, no network.
//!
//! `fake_pi_peer` (`src/bin/`) answers `pi --mode rpc`. It is started here as an
//! ordinary child, so what is tested is the adapter's own spawn, reading,
//! mapping and closing.

use retinue_adapter_pi::PiAdapter;
use retinue_agent_core::*;
use serde_json::json;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::sync::mpsc;

const EVENT_TIMEOUT: Duration = Duration::from_secs(10);

fn peer() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_fake_pi_peer"))
}

fn runtime(scenario: &str, extra: &[&str]) -> RuntimeSpec {
    let mut args = vec!["--scenario".to_owned(), scenario.to_owned()];
    args.extend(extra.iter().map(|arg| (*arg).to_owned()));
    RuntimeSpec { runtime_id: format!("fake-pi-{scenario}"), provider: "pi".into(), executable: peer(), args, cwd: Some(std::env::temp_dir()) }
}

fn temp_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("retinue-pi-test-{}-{}-{}", std::process::id(), name, std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()))
}

async fn open(adapter: &Arc<PiAdapter>, runtime: &RuntimeSpec, session: &str) -> (SessionHandle, mpsc::Receiver<AgentEvent>) {
    let (sender, receiver) = mpsc::channel(128);
    let handle = adapter
        .new_session(
            runtime,
            NewSessionRequest { session_id: session.into(), project_path: std::env::temp_dir(), exec_options: ExecOptions::default() },
            sender,
        )
        .await
        .expect("the fake peer answers get_state");
    (handle, receiver)
}

/// Every event up to and including the first one `stop` accepts.
async fn until(receiver: &mut mpsc::Receiver<AgentEvent>, stop: impl Fn(&AgentEvent) -> bool) -> Vec<AgentEvent> {
    let mut events = Vec::new();
    loop {
        let event = tokio::time::timeout(EVENT_TIMEOUT, receiver.recv()).await.expect("an event arrived in time").expect("the channel stayed open");
        let done = stop(&event);
        events.push(event);
        if done {
            return events;
        }
    }
}

/// Runs one prompt to its terminal event.
async fn run_prompt(adapter: &Arc<PiAdapter>, handle: &SessionHandle, receiver: &mut mpsc::Receiver<AgentEvent>, text: &str) -> Vec<AgentEvent> {
    let adapter = adapter.clone();
    let handle = handle.clone();
    let text = text.to_owned();
    let turn = tokio::spawn(async move {
        adapter
            .prompt(&handle, PromptRequest { turn_id: format!("turn-{text}"), text, exec_options: ExecOptions::default(), attachments: Vec::new() })
            .await
    });
    let events = until(receiver, |event| {
        matches!(event, AgentEvent::TurnCompleted | AgentEvent::TurnCancelled | AgentEvent::Error { .. })
    })
    .await;
    turn.await.expect("the prompt task finished").expect("the turn ran");
    events
}

fn deltas(events: &[AgentEvent]) -> String {
    events
        .iter()
        .filter_map(|event| match event {
            AgentEvent::AssistantDelta { text } => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

fn usage(events: &[AgentEvent]) -> Option<UsageReport> {
    events.iter().rev().find_map(|event| match event {
        AgentEvent::UsageReport { report, .. } => Some(report.clone()),
        _ => None,
    })
}

#[tokio::test]
async fn a_streamed_turn_becomes_deltas_a_message_and_a_completed_turn() {
    let adapter = Arc::new(PiAdapter::new());
    let (handle, mut receiver) = open(&adapter, &runtime("stream", &[]), "session-stream").await;
    // The session started, and the provider session id is the one Pi was given,
    // which is Retinue's own id made safe for Pi: a resume names the same
    // session without keeping a second id.
    let opened = until(&mut receiver, |event| matches!(event, AgentEvent::SessionStarted { .. })).await;
    match opened.last().unwrap() {
        AgentEvent::SessionStarted { provider_session_id } => assert_eq!(provider_session_id, "session-stream"),
        other => panic!("expected a started session, got {other:?}"),
    }
    let mut events = run_prompt(&adapter, &handle, &mut receiver, "hello").await;
    // Usage follows the terminal event: the daemon's convention is that a caller
    // taking the next event still observes completion.
    events.extend(until(&mut receiver, |event| matches!(event, AgentEvent::UsageReport { .. })).await);

    assert_eq!(deltas(&events), "Hello there");
    assert!(events.iter().any(|event| matches!(event, AgentEvent::ThinkingDelta { text } if text == "Considering")));
    assert!(events.iter().any(|event| matches!(event, AgentEvent::AssistantMessage { text } if text == "Hello there")), "the finished message is authoritative");
    assert!(events.iter().any(|event| matches!(event, AgentEvent::PromptAccepted)), "the daemon records delivery from this");
    let report = usage(&events).expect("Pi reported usage");
    assert_eq!(report.input_tokens, Some(100));
    assert_eq!(report.output_tokens, Some(4));
    assert_eq!(report.cache_write_tokens, Some(0), "the per-response bucket Pi reported, not the session total");
    assert_eq!(report.context_used, Some(60000));
    assert_eq!(report.context_size, Some(200000));
    assert_eq!(report.reported_cost_decimal.as_deref(), Some("0.002100"));
    assert_eq!(report.cost_currency.as_deref(), Some("USD"));
    assert!(!report.cost_is_cumulative, "a per-response price is not a session total");
    adapter.close_session(&handle).await.unwrap();
}

#[tokio::test]
async fn a_tool_step_becomes_one_row_from_start_to_finish() {
    let adapter = Arc::new(PiAdapter::new());
    let (handle, mut receiver) = open(&adapter, &runtime("tool", &[]), "session-tool").await;
    let events = run_prompt(&adapter, &handle, &mut receiver, "list the folder").await;

    let started = events.iter().find_map(|event| match event {
        AgentEvent::ToolStarted { tool_call_id, kind, title, raw, .. } => Some((tool_call_id.clone(), kind.clone(), title.clone(), raw.clone())),
        _ => None,
    });
    let (id, kind, title, raw) = started.expect("the tool call became a row");
    assert_eq!(id, "call_1");
    assert_eq!(kind, "bash");
    assert_eq!(title, "Ran ls -la", "the shared presentation helper names the work");
    assert_eq!(raw["command"], "ls -la");
    assert!(events.iter().any(|event| matches!(event, AgentEvent::ToolUpdated { tool_call_id, .. } if tool_call_id == "call_1")));
    let completed = events.iter().find_map(|event| match event {
        AgentEvent::ToolCompleted { tool_call_id, raw, .. } if tool_call_id == "call_1" => Some(raw.clone()),
        _ => None,
    });
    assert_eq!(completed.expect("the row completed")["isError"], false);
    adapter.close_session(&handle).await.unwrap();
}

#[tokio::test]
async fn a_failed_turn_reports_the_providers_error_without_any_key_shaped_word() {
    let adapter = Arc::new(PiAdapter::new());
    let (handle, mut receiver) = open(&adapter, &runtime("error", &[]), "session-error").await;
    let events = run_prompt(&adapter, &handle, &mut receiver, "hello").await;
    let message = events
        .iter()
        .find_map(|event| match event {
            AgentEvent::Error { message } => Some(message.clone()),
            _ => None,
        })
        .expect("a failed turn ends with an error");
    assert!(message.contains("401"), "the provider's own reason is kept: {message}");
    assert!(message.contains("[hidden]"), "a key-shaped word is hidden: {message}");
    assert!(!message.contains("sk-test"), "the key never reaches the transcript: {message}");
    assert!(events.iter().any(|event| matches!(event, AgentEvent::AssistantMessage { text } if text == "I could not start.")));
}

#[tokio::test]
async fn a_refused_prompt_is_a_rejection_and_not_a_turn() {
    let adapter = Arc::new(PiAdapter::new());
    let (handle, _receiver) = open(&adapter, &runtime("refuse", &[]), "session-refuse").await;
    let error = adapter
        .prompt(&handle, PromptRequest { turn_id: "turn-1".into(), text: "hello".into(), exec_options: ExecOptions::default(), attachments: Vec::new() })
        .await
        .expect_err("Pi refused the prompt");
    assert!(matches!(&error, AdapterError::Rejected(message) if message.contains("not signed in")), "got {error:?}");
    adapter.close_session(&handle).await.unwrap();
}

#[tokio::test]
async fn a_cancel_ends_the_turn_as_cancelled() {
    let adapter = Arc::new(PiAdapter::new());
    let (handle, mut receiver) = open(&adapter, &runtime("hang", &[]), "session-cancel").await;
    let running = {
        let adapter = adapter.clone();
        let handle = handle.clone();
        tokio::spawn(async move {
            adapter
                .prompt(&handle, PromptRequest { turn_id: "turn-cancel".into(), text: "wait".into(), exec_options: ExecOptions::default(), attachments: Vec::new() })
                .await
        })
    };
    // The prompt is accepted before the cancel arrives.
    until(&mut receiver, |event| matches!(event, AgentEvent::PromptAccepted)).await;
    adapter.cancel(&handle).await.unwrap();
    let events = until(&mut receiver, |event| matches!(event, AgentEvent::TurnCancelled | AgentEvent::TurnCompleted | AgentEvent::Error { .. })).await;
    assert!(matches!(events.last(), Some(AgentEvent::TurnCancelled)), "got {:?}", events.last());
    running.await.expect("the prompt task finished").expect("a cancelled turn is not an error");
    adapter.close_session(&handle).await.unwrap();
}

#[tokio::test]
async fn steering_reaches_the_running_turn_and_is_stale_after_it() {
    let adapter = Arc::new(PiAdapter::new());
    let (handle, mut receiver) = open(&adapter, &runtime("hang", &[]), "session-steer").await;
    let running = {
        let adapter = adapter.clone();
        let handle = handle.clone();
        tokio::spawn(async move {
            adapter
                .prompt(&handle, PromptRequest { turn_id: "turn-steer".into(), text: "start".into(), exec_options: ExecOptions::default(), attachments: Vec::new() })
                .await
        })
    };
    until(&mut receiver, |event| matches!(event, AgentEvent::PromptAccepted)).await;
    assert_eq!(adapter.steer(&handle, SteerRequest { turn_id: "turn-steer".into(), text: "also this".into() }).await.unwrap(), SteerOutcome::Accepted);
    // A turn id that is not the running one is stale: the caller queues the text.
    assert_eq!(adapter.steer(&handle, SteerRequest { turn_id: "another-turn".into(), text: "nope".into() }).await.unwrap(), SteerOutcome::Stale);
    adapter.cancel(&handle).await.unwrap();
    running.await.unwrap().unwrap();
    assert_eq!(adapter.steer(&handle, SteerRequest { turn_id: "turn-steer".into(), text: "late".into() }).await.unwrap(), SteerOutcome::Stale);
    adapter.close_session(&handle).await.unwrap();
}

#[tokio::test]
async fn a_resume_with_no_history_is_rejected_so_the_caller_starts_fresh() {
    let adapter = Arc::new(PiAdapter::new());
    let (sender, _receiver) = mpsc::channel(8);
    let error = adapter
        .resume_session(
            &runtime("emptyhistory", &[]),
            ResumeSessionRequest { session_id: "session-resume".into(), provider_session_id: "gone".into(), project_path: std::env::temp_dir(), exec_options: ExecOptions::default() },
            sender,
        )
        .await
        .expect_err("an empty history is not a resumed conversation");
    assert!(matches!(error, AdapterError::ResumeRejected), "got {error:?}");
}

#[tokio::test]
async fn the_catalogue_carries_provider_model_ids_and_their_effort_levels() {
    let adapter = PiAdapter::new();
    let catalog = adapter.model_catalog(&runtime("stream", &[])).await.expect("the fake answers get_available_models");
    assert!(catalog.validated && !catalog.fallback);
    assert!(catalog.source.contains("get_available_models"));
    let ids = catalog.models.iter().map(|model| model.id.as_str()).collect::<Vec<_>>();
    assert_eq!(ids, ["anthropic/claude-sonnet-4", "opencode-go/glm-5.2"]);
    assert_eq!(catalog.models[0].context_window, Some(200000));
    assert!(catalog.models[0].supported_thinking.contains(&"high".to_owned()));
    assert_eq!(catalog.models[1].supported_thinking, ["off"]);
}

#[tokio::test]
async fn a_model_change_is_sent_to_a_live_session_and_the_session_keeps_its_context() {
    let record = temp_path("model-record");
    let adapter = Arc::new(PiAdapter::new());
    let runtime = runtime("stream", &["--record", record.to_str().unwrap()]);
    let (handle, mut receiver) = open(&adapter, &runtime, "session-model").await;
    assert_eq!(adapter.model_switch(), ModelSwitch::KeepsSession, "Pi switches model inside the session");
    let options = ExecOptions { model: Some("opencode-go/glm-5.2".into()), thinking: Some("high".into()), ..ExecOptions::default() };
    let adapter_for_turn = adapter.clone();
    let handle_for_turn = handle.clone();
    let turn = tokio::spawn(async move {
        adapter_for_turn.prompt(&handle_for_turn, PromptRequest { turn_id: "turn-model".into(), text: "hello".into(), exec_options: options, attachments: Vec::new() }).await
    });
    let events = until(&mut receiver, |event| matches!(event, AgentEvent::TurnCompleted | AgentEvent::Error { .. })).await;
    turn.await.unwrap().unwrap();
    let applied = events.iter().find_map(|event| match event {
        AgentEvent::ExecApplied { outcomes, .. } => Some(outcomes.clone()),
        _ => None,
    });
    let outcomes = applied.expect("the settings were reported");
    assert_eq!(outcomes["model"].applied, Some(true));
    assert_eq!(outcomes["model"].evidence_kind, EvidenceKind::ProviderEcho);
    assert_eq!(outcomes["thinking"].applied, Some(true));
    let sent = std::fs::read_to_string(&record).unwrap();
    assert!(sent.contains("\"type\":\"set_model\""), "{sent}");
    assert!(sent.contains("glm-5.2"), "{sent}");
    assert!(sent.contains("\"type\":\"set_thinking_level\""), "{sent}");
    let _ = std::fs::remove_file(record);
    adapter.close_session(&handle).await.unwrap();
}

#[tokio::test]
async fn the_owners_keys_reach_the_child_and_the_cli_is_started_the_way_retinue_intends() {
    let record = temp_path("env-record");
    let adapter = Arc::new(PiAdapter::new());
    let runtime = runtime("stream", &["--record-env", record.to_str().unwrap()]);
    let mut options = ExecOptions::default();
    options.env.insert("OPENAI_API_KEY".into(), "sk-fixture-value-never-in-a-log".into());
    let instructions = "You are an employee of Retinue.";
    options.instructions = Some(instructions.into());
    let (sender, _receiver) = mpsc::channel(8);
    let handle = adapter
        .new_session(
            &runtime,
            NewSessionRequest { session_id: "session-env".into(), project_path: std::env::temp_dir(), exec_options: options },
            sender,
        )
        .await
        .unwrap();
    let recorded = std::fs::read_to_string(&record).unwrap();
    let env_line = recorded.lines().find(|line| line.starts_with("OPENAI_API_KEY=")).expect("the child recorded its own environment");
    assert_eq!(env_line, format!("OPENAI_API_KEY=set:{}", "sk-fixture-value-never-in-a-log".len()), "the key reached the child");
    assert!(recorded.contains("ANTHROPIC_API_KEY=absent"));
    let args = recorded.lines().find(|line| line.starts_with("ARGS=")).expect("the child recorded its arguments");
    assert!(args.contains("--mode rpc"), "{args}");
    assert!(args.contains("--session-id session-env"), "{args}");
    assert!(args.contains("--no-approve"), "a project's own Pi extensions are never loaded: {args}");
    assert!(args.contains("--append-system-prompt"), "{args}");
    // The instruction file exists while the conversation runs, holds the text,
    // and is gone once the conversation closes.
    let path = Path::new(args.split("--append-system-prompt ").nth(1).unwrap().trim());
    assert_eq!(std::fs::read_to_string(path).unwrap(), instructions);
    assert!(!recorded.contains("sk-fixture-value-never-in-a-log "), "the key is never an argument");
    adapter.close_session(&handle).await.unwrap();
    assert!(!path.exists(), "the instruction file is deleted with the conversation");
    let _ = std::fs::remove_file(record);
}

#[tokio::test]
async fn a_child_that_dies_mid_turn_is_an_error_and_not_a_completed_turn() {
    let adapter = Arc::new(PiAdapter::new());
    let (handle, mut receiver) = open(&adapter, &runtime("exit", &[]), "session-exit").await;
    let adapter_for_turn = adapter.clone();
    let handle_for_turn = handle.clone();
    let turn = tokio::spawn(async move {
        adapter_for_turn.prompt(&handle_for_turn, PromptRequest { turn_id: "turn-exit".into(), text: "hello".into(), exec_options: ExecOptions::default(), attachments: Vec::new() }).await
    });
    let events = until(&mut receiver, |event| matches!(event, AgentEvent::TurnCompleted | AgentEvent::TurnCancelled | AgentEvent::Error { .. })).await;
    assert!(matches!(events.last(), Some(AgentEvent::Error { .. })), "got {:?}", events.last());
    let error = turn.await.unwrap().expect_err("a died turn is reported to the caller");
    assert!(matches!(error, AdapterError::TurnErrorReported(_)), "got {error:?}");
}

#[tokio::test]
async fn a_prompt_with_an_image_sends_it_as_image_content() {
    let record = temp_path("image-record");
    let adapter = Arc::new(PiAdapter::new());
    let (handle, mut receiver) = open(&adapter, &runtime("stream", &["--record", record.to_str().unwrap()]), "session-image").await;
    let image = temp_path("image.png");
    std::fs::write(&image, b"\x89PNG\r\n\x1a\n-fixture").unwrap();
    let adapter_for_turn = adapter.clone();
    let handle_for_turn = handle.clone();
    let path = image.clone();
    let turn = tokio::spawn(async move {
        adapter_for_turn
            .prompt(
                &handle_for_turn,
                PromptRequest {
                    turn_id: "turn-image".into(),
                    text: "look".into(),
                    exec_options: ExecOptions::default(),
                    attachments: vec![PromptAttachment { path, name: "image.png".into(), media_type: "image/png".into() }],
                },
            )
            .await
    });
    until(&mut receiver, |event| matches!(event, AgentEvent::TurnCompleted | AgentEvent::Error { .. })).await;
    turn.await.unwrap().unwrap();
    let sent = std::fs::read_to_string(&record).unwrap();
    assert!(sent.contains("\"images\":[{"), "{sent}");
    assert!(sent.contains("\"mimeType\":\"image/png\""), "{sent}");
    let _ = std::fs::remove_file(record);
    let _ = std::fs::remove_file(image);
    adapter.close_session(&handle).await.unwrap();
}

#[tokio::test]
async fn a_missing_image_is_refused_rather_than_dropped() {
    let adapter = Arc::new(PiAdapter::new());
    let (handle, _receiver) = open(&adapter, &runtime("stream", &[]), "session-missing-image").await;
    let error = adapter
        .prompt(
            &handle,
            PromptRequest {
                turn_id: "turn-missing".into(),
                text: "look".into(),
                exec_options: ExecOptions::default(),
                attachments: vec![PromptAttachment { path: PathBuf::from("C:/definitely/not/here.png"), name: "gone.png".into(), media_type: "image/png".into() }],
            },
        )
        .await
        .expect_err("an unreadable image is an objection");
    assert!(matches!(&error, AdapterError::Unsupported(message) if message.contains("gone.png")), "got {error:?}");
    adapter.close_session(&handle).await.unwrap();
}

#[tokio::test]
async fn an_unknown_agent_command_is_not_answered_by_pi_and_the_session_closes_quietly() {
    // Nothing listens on the events channel after the receiver is dropped: a
    // closed conversation must not keep the child alive through a stuck send.
    let adapter = Arc::new(PiAdapter::new());
    let (handle, receiver) = open(&adapter, &runtime("stream", &[]), "session-quiet").await;
    drop(receiver);
    adapter.close_session(&handle).await.unwrap();
    // Closing twice is harmless.
    adapter.close_session(&handle).await.unwrap();
    assert_eq!(json!({}), json!({}));
}
