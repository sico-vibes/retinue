//! Steering a running OpenCode turn: the `_session/steering` extension is used
//! only when the agent advertises it, a turn that has ended is `Stale` (the
//! caller queues the text), and an agent without the extension is refused.
use retinue_adapter_acp::AcpAdapter;
use retinue_agent_core::{AdapterError, AgentAdapter, AgentEvent, ExecOptions, NewSessionRequest, PromptRequest, RuntimeSpec, SessionHandle, SteerOutcome, SteerRequest};
use serde_json::Value;
use std::{path::{Path, PathBuf}, sync::Arc, time::{Duration, Instant}};
use tokio::sync::mpsc;

fn peer() -> PathBuf {
    if let Some(path) = std::env::var_os("CARGO_BIN_EXE_fake_acp_peer") { return PathBuf::from(path); }
    let exe = std::env::current_exe().expect("test executable");
    let debug_dir = exe.parent().and_then(|dir| dir.parent()).expect("cargo debug dir");
    let windows = debug_dir.join("fake-acp-peer.exe");
    if windows.exists() { windows } else { debug_dir.join("fake-acp-peer") }
}

fn temp_dir(label: &str) -> PathBuf {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let dir = std::env::temp_dir().join(format!("retinue-acp-steer-{label}-{}-{}", std::process::id(), NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

async fn open(adapter: &AcpAdapter, args: Vec<String>) -> (SessionHandle, mpsc::Receiver<AgentEvent>) {
    let (tx, rx) = mpsc::channel(64);
    let runtime = RuntimeSpec { runtime_id: "fixture".into(), provider: "opencode".into(), executable: peer(), args, cwd: None };
    let handle = adapter
        .new_session(&runtime, NewSessionRequest { session_id: "steer".into(), project_path: std::env::temp_dir(), exec_options: ExecOptions::default() }, tx)
        .await
        .unwrap();
    (handle, rx)
}

fn prompt_request() -> PromptRequest {
    PromptRequest { attachments: Vec::new(), turn_id: "turn-steer".into(), text: "count to ten".into(), exec_options: ExecOptions::default() }
}

fn steer_request() -> SteerRequest {
    SteerRequest { turn_id: "turn-steer".into(), text: "stop; answer two".into() }
}

fn trace(record: &Path) -> Vec<String> {
    std::fs::read(record)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .map(|value| value["rpc"].as_array().cloned().unwrap_or_default().iter().filter_map(|entry| entry["method"].as_str().map(str::to_owned)).collect())
        .unwrap_or_default()
}

async fn wait_for_call(record: &Path, method: &str) {
    let started = Instant::now();
    while !trace(record).iter().any(|call| call == method) {
        assert!(started.elapsed() < Duration::from_secs(10), "{method} was never sent");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// `--usage cancel` holds the turn open (no answer until `session/cancel`),
/// which is the one state a steer can be delivered into.
fn held_turn_args(record: &Path, extra: &[&str]) -> Vec<String> {
    let mut args = vec![
        "--record".into(), record.to_string_lossy().into_owned(),
        "--usage".into(), "cancel".into(),
    ];
    args.extend(extra.iter().map(|arg| (*arg).to_owned()));
    args
}

#[tokio::test]
async fn a_steer_reaches_the_running_turn_through_the_extension() {
    let dir = temp_dir("accepted");
    let record = dir.join("record.json");
    let adapter = Arc::new(AcpAdapter::default());
    let (handle, _events) = open(&adapter, held_turn_args(&record, &["--steering"])).await;
    let running = {
        let (adapter, handle) = (Arc::clone(&adapter), handle.clone());
        tokio::spawn(async move { adapter.prompt(&handle, prompt_request()).await })
    };
    wait_for_call(&record, "session/prompt").await;
    let outcome = adapter.steer(&handle, steer_request()).await.expect("the agent advertises steering");
    assert_eq!(outcome, SteerOutcome::Accepted);
    assert!(trace(&record).iter().any(|call| call == "_session/steering"), "the text went to the extension");
    // End the held turn so the spawned prompt settles.
    adapter.cancel(&handle).await.unwrap();
    tokio::time::timeout(Duration::from_secs(10), running).await.expect("the prompt call ends once the turn is stopped").unwrap().expect("a cancelled turn is not a failure");
    let _ = adapter.close_session(&handle).await;
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn steering_after_the_turn_ended_is_stale_and_sends_nothing() {
    let dir = temp_dir("stale");
    let record = dir.join("record.json");
    let adapter = AcpAdapter::default();
    let (handle, _events) = open(&adapter, held_turn_args(&record, &["--steering"])).await;
    // Nothing is running, so the turn id cannot match: the caller keeps the
    // text for the queue instead of it being dropped.
    let outcome = adapter.steer(&handle, steer_request()).await.unwrap();
    assert_eq!(outcome, SteerOutcome::Stale);
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(!trace(&record).iter().any(|call| call == "_session/steering"), "a turn that is gone is not asked");
    let _ = adapter.close_session(&handle).await;
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn an_agent_that_reports_no_running_turn_is_stale() {
    let dir = temp_dir("prompt-required");
    let record = dir.join("record.json");
    let adapter = Arc::new(AcpAdapter::default());
    let (handle, _events) = open(&adapter, held_turn_args(&record, &["--steering", "--steering-prompt-required"])).await;
    let running = {
        let (adapter, handle) = (Arc::clone(&adapter), handle.clone());
        tokio::spawn(async move { adapter.prompt(&handle, prompt_request()).await })
    };
    wait_for_call(&record, "session/prompt").await;
    // The agent's own turn ended between the adapter's check and the request.
    let outcome = adapter.steer(&handle, steer_request()).await.unwrap();
    assert_eq!(outcome, SteerOutcome::Stale);
    assert!(trace(&record).iter().any(|call| call == "_session/steering"));
    adapter.cancel(&handle).await.unwrap();
    let _ = tokio::time::timeout(Duration::from_secs(10), running).await.expect("the prompt call ends").unwrap();
    let _ = adapter.close_session(&handle).await;
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn an_agent_without_the_extension_refuses_steering() {
    let dir = temp_dir("refused");
    let record = dir.join("record.json");
    let adapter = AcpAdapter::default();
    let (handle, events) = open(&adapter, held_turn_args(&record, &[])).await;
    drop(events);
    let refused = adapter.steer(&handle, steer_request()).await.expect_err("no advertisement means no steering");
    assert!(matches!(refused, AdapterError::Unsupported(_)), "{refused:?}");
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(!trace(&record).iter().any(|call| call == "_session/steering"));
    let _ = adapter.close_session(&handle).await;
    let _ = std::fs::remove_dir_all(dir);
}
