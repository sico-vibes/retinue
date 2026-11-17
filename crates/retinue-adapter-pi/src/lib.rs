//! Pi as a coding agent: `pi --mode rpc` over JSONL stdin and stdout.
//!
//! Pi (`@earendil-works/pi-coding-agent`, MIT, 1.5.2 on 16 November 2026) is not
//! an ACP agent: it has its own long-lived RPC mode (`docs/rpc.md`), so this
//! adapter speaks that protocol instead of reusing `retinue-adapter-acp`.
//!
//! What this adapter carries, and what it deliberately does not:
//!
//! - **One child per conversation.** `pi --mode rpc --session-id <id>` is
//!   started when the conversation starts and kept until it closes. The id is
//!   derived from Retinue's session id once ([`protocol::pi_session_id`]), so a
//!   later resume names the same Pi session.
//! - **Turns.** A `prompt` command is accepted (or refused) at once; the turn
//!   ends on `agent_settled`, the one record that means Pi will not continue on
//!   its own. `agent_end` is not the end: a retry can follow it.
//! - **Streaming and tool steps.** `message_update` carries text and thinking
//!   deltas; `tool_execution_start/update/end` become the tool rows.
//! - **Usage when Pi reports it.** Per-response buckets from `message_update`,
//!   summed over the turn, plus the context window from `get_session_stats`.
//!   A bucket Pi did not report stays unknown.
//! - **No approvals.** Pi has no per-tool approval prompt: its own security page
//!   says it "does not ask for approval before every tool call". There is no
//!   card to show, so `permissions` is false and `reply_permission` refuses.
//! - **No MCP.** Pi has no per-run MCP configuration flag in 1.5.2, so the team
//!   bridge's tools are not attached. A bot on Pi works alone; this is reported
//!   in `docs/plans/vault-channels-pi.md`'s lane report.
//! - **Instructions are fixed at start.** Pi reads its appended system prompt
//!   when the process starts (`--append-system-prompt <file>`), so a changed
//!   text is reported as not applied instead of being silently ignored.
//!
//! The keys Retinue holds for Pi (`Retinue/pi/<provider>`) reach this child
//! through `ExecOptions::env`, which the daemon fills from the credential store.
//! This adapter never writes a key anywhere, and never logs an environment value.

pub mod protocol;

use async_trait::async_trait;
use base64::Engine as _;
use chrono::{Duration as ChronoDuration, SecondsFormat, Utc};
use retinue_agent_core::*;
use retinue_process::{prepare_command, ProcessTree, StdinWriter};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::{Child, Command},
    sync::{oneshot, Mutex},
};

/// A command's answer is expected well inside this: Pi answers a command as soon
/// as it has accepted it, before any model work.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
/// A catalogue probe starts a Node process and reads its bundled catalogue.
const CATALOG_TIMEOUT: Duration = Duration::from_secs(20);
/// The largest image this adapter will hand to a prompt.
const MAX_ATTACHMENT_BYTES: u64 = 8 * 1024 * 1024;
/// A provider error sentence is shown to the owner, so it is bounded here.
const ERROR_TEXT_MAX: usize = 400;
const INSTRUCTION_DIR: &str = "retinue-pi-instructions";

/// How the turn ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Settled {
    Completed,
    Aborted,
    /// Pi went away before it settled: the turn did not finish.
    Died,
}

/// One running Pi process.
struct Conn {
    stdin: Mutex<StdinWriter>,
    child: Mutex<Child>,
    tree: ProcessTree,
    alive: AtomicBool,
    /// Command responses waiting for their record, by the id we sent.
    pending: Mutex<HashMap<String, oneshot::Sender<Result<Value, AdapterError>>>>,
    next: AtomicU64,
    events: EventSender,
    provider_session_id: Mutex<Option<String>>,
    turn: Mutex<Turn>,
    applied_model: Mutex<Option<String>>,
    applied_thinking: Mutex<Option<String>>,
    /// The instruction text this process was started with, and the file it was
    /// written to (deleted when the conversation closes).
    spawned_instructions: Option<String>,
    instruction_file: Mutex<Option<PathBuf>>,
}

#[derive(Default)]
struct Turn {
    id: Option<String>,
    settle: Option<oneshot::Sender<Settled>>,
    /// The provider's own error text for this turn, already sanitised.
    error: Option<String>,
    /// Usage reported for the response running right now, and the sum of the
    /// responses that already finished in this turn.
    response_usage: Option<protocol::Usage>,
    usage: protocol::Usage,
}

impl Turn {
    /// Folds the response's usage into the turn's total. Called when a response
    /// finishes, and again at the end for a response with no `message_end`.
    fn close_response(&mut self) {
        let Some(response) = self.response_usage.take() else { return };
        let add = |total: &mut Option<u64>, value: Option<u64>| {
            if let Some(value) = value {
                *total = Some(total.unwrap_or(0).saturating_add(value));
            }
        };
        add(&mut self.usage.input, response.input);
        add(&mut self.usage.output, response.output);
        add(&mut self.usage.cache_read, response.cache_read);
        add(&mut self.usage.cache_write, response.cache_write);
        add(&mut self.usage.total, response.total);
        if let Some(cost) = response.cost_usd {
            self.usage.cost_usd = Some(self.usage.cost_usd.unwrap_or(0.0) + cost);
        }
    }
}

/// The adapter. One entry per open conversation; a closed conversation removes
/// its entry and with it the child process.
#[derive(Default)]
pub struct PiAdapter {
    sessions: Mutex<HashMap<String, Arc<Conn>>>,
}

impl PiAdapter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Start Pi for one conversation (or one catalogue probe) and take stdio.
    async fn open(
        runtime: &RuntimeSpec,
        project: &Path,
        options: &ExecOptions,
        session_id: Option<&str>,
        events: EventSender,
    ) -> Result<Arc<Conn>, AdapterError> {
        let instructions = options.instructions.as_deref().filter(|text| !text.trim().is_empty()).map(str::to_owned);
        let instruction_file = match instructions.as_deref() {
            Some(text) => Some(write_instruction_file(text).map_err(|_| AdapterError::Process("The instructions could not be written for Pi to read.".into()))?),
            None => None,
        };
        let args = launch_args(runtime, options, session_id, instruction_file.as_deref());
        let mut cmd = Command::new(&runtime.executable);
        prepare_command(&mut cmd);
        cmd.args(&args)
            .current_dir(project)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        // The daemon has already filtered what may appear here (the owner's bot
        // environment, an account's folder layout, and Retinue's own Pi keys).
        for (key, value) in &options.env {
            cmd.env(key, value);
        }
        let spawn = cmd.spawn();
        let mut child = match spawn {
            Ok(child) => child,
            Err(error) => {
                remove_instruction_file(instruction_file.as_deref());
                return Err(AdapterError::Process(format!("Pi could not be started: {error}")));
            }
        };
        let tree = match ProcessTree::attach(&mut child) {
            Ok(tree) => tree,
            Err(error) => {
                let _ = child.start_kill();
                remove_instruction_file(instruction_file.as_deref());
                return Err(AdapterError::Process(format!("Pi could not be supervised: {error}")));
            }
        };
        let (Some(stdin), Some(stdout), Some(stderr)) = (child.stdin.take(), child.stdout.take(), child.stderr.take()) else {
            let _ = tree.terminate();
            remove_instruction_file(instruction_file.as_deref());
            return Err(AdapterError::Process("Pi did not give Retinue its input and output pipes.".into()));
        };
        drain_stderr(stderr);
        let conn = Arc::new(Conn {
            stdin: Mutex::new(StdinWriter::new(stdin)),
            child: Mutex::new(child),
            tree,
            alive: AtomicBool::new(true),
            pending: Mutex::new(HashMap::new()),
            next: AtomicU64::new(1),
            events,
            provider_session_id: Mutex::new(None),
            turn: Mutex::new(Turn::default()),
            applied_model: Mutex::new(None),
            applied_thinking: Mutex::new(None),
            spawned_instructions: instructions,
            instruction_file: Mutex::new(instruction_file),
        });
        let reader = Arc::downgrade(&conn);
        tokio::spawn(async move { read_stdout(reader, stdout).await });
        Ok(conn)
    }

    /// Send one command and wait for its response record.
    async fn request(conn: &Arc<Conn>, kind: &str, extra: Value, limit: Duration) -> Result<Value, AdapterError> {
        if !conn.alive.load(Ordering::Acquire) {
            return Err(AdapterError::Process("Pi is not running.".into()));
        }
        let id = format!("retinue-{}", conn.next.fetch_add(1, Ordering::Relaxed));
        let (sender, receiver) = oneshot::channel();
        conn.pending.lock().await.insert(id.clone(), sender);
        let mut bytes = serde_json::to_vec(&protocol::command(&id, kind, extra)).map_err(|error| AdapterError::Protocol(error.to_string()))?;
        bytes.push(b'\n');
        if let Err(error) = write(&conn.stdin, &bytes).await {
            conn.pending.lock().await.remove(&id);
            return Err(error);
        }
        match tokio::time::timeout(limit, receiver).await {
            Ok(Ok(Ok(data))) => Ok(data),
            Ok(Ok(Err(error))) => Err(error),
            Ok(Err(_)) => Err(AdapterError::Process("Pi exited before it answered.".into())),
            Err(_) => {
                conn.pending.lock().await.remove(&id);
                Err(AdapterError::Timeout)
            }
        }
    }

    /// Read the session's id and history size, then remember them.
    async fn initialize(conn: &Arc<Conn>, wanted: &str) -> Result<u64, AdapterError> {
        let state = Self::request(conn, "get_state", json!({}), REQUEST_TIMEOUT).await?;
        let (reported, messages, _model) = protocol::state_of(&state);
        let id = reported.unwrap_or_else(|| wanted.to_owned());
        *conn.provider_session_id.lock().await = Some(id);
        Ok(messages.unwrap_or(0))
    }

    /// Apply the conversation's model and effort to this process. `set_model`
    /// answers with the model Pi switched to, so the evidence is Pi's own echo;
    /// a refused effort is reported as not applied and the turn still runs.
    async fn apply_options(conn: &Arc<Conn>, options: &ExecOptions, turn_id: &str) -> Result<(), AdapterError> {
        let mut outcomes = BTreeMap::new();
        if let Some(model) = options.model.as_deref().filter(|model| !model.is_empty()) {
            if conn.applied_model.lock().await.as_deref() != Some(model) {
                let (provider, id) = protocol::split_model(model).ok_or_else(|| AdapterError::Rejected(format!("{model} is not a Pi model: Pi needs provider/model.")))?;
                let echoed = Self::request(conn, "set_model", json!({"provider": provider, "modelId": id}), REQUEST_TIMEOUT)
                    .await
                    .map_err(|error| match error {
                        AdapterError::Protocol(message) => AdapterError::Rejected(safe_error(&message)),
                        other => other,
                    })?;
                let echoed_id = protocol::model_id(echoed["provider"].as_str().unwrap_or(""), echoed["id"].as_str().unwrap_or(""));
                let applied = echoed_id == model;
                outcomes.insert(
                    "model".to_owned(),
                    SettingOutcome {
                        requested: Some(json!(model)),
                        applied: Some(applied),
                        evidence_kind: if applied { EvidenceKind::ProviderEcho } else { EvidenceKind::None },
                        evidence_value: Some(json!(echoed_id)),
                        reason: (!applied).then(|| "Pi switched to a different model than the one asked for.".to_owned()),
                    },
                );
                *conn.applied_model.lock().await = Some(model.to_owned());
            }
        }
        if let Some(level) = options.thinking.as_deref().filter(|level| !level.is_empty()) {
            if conn.applied_thinking.lock().await.as_deref() != Some(level) {
                match Self::request(conn, "set_thinking_level", json!({"level": level}), REQUEST_TIMEOUT).await {
                    Ok(_) => {
                        outcomes.insert(
                            "thinking".to_owned(),
                            SettingOutcome { requested: Some(json!(level)), applied: Some(true), evidence_kind: EvidenceKind::ProviderEcho, evidence_value: Some(json!(level)), reason: None },
                        );
                        *conn.applied_thinking.lock().await = Some(level.to_owned());
                    }
                    Err(_) => {
                        outcomes.insert(
                            "thinking".to_owned(),
                            SettingOutcome {
                                requested: Some(json!(level)),
                                applied: Some(false),
                                evidence_kind: EvidenceKind::None,
                                evidence_value: None,
                                reason: Some("Pi does not take that thinking level for this model.".to_owned()),
                            },
                        );
                    }
                }
            }
        }
        // Instructions are a start-time flag, so a change is reported rather
        // than pretended.
        let wanted = options.instructions.as_deref().filter(|text| !text.trim().is_empty());
        if wanted != conn.spawned_instructions.as_deref() {
            outcomes.insert(
                "instructions".to_owned(),
                SettingOutcome {
                    requested: wanted.map(|_| json!(true)).or(Some(json!(false))),
                    applied: Some(false),
                    evidence_kind: EvidenceKind::None,
                    evidence_value: None,
                    reason: Some("Pi reads its instructions when it starts. Start a new conversation to give it these.".to_owned()),
                },
            );
        }
        if !outcomes.is_empty() {
            let _ = conn.events.send(AgentEvent::ExecApplied { turn_id: turn_id.to_owned(), outcomes }).await;
        }
        Ok(())
    }

    async fn close(conn: &Arc<Conn>) {
        conn.alive.store(false, Ordering::Release);
        {
            let mut stdin = conn.stdin.lock().await;
            let mut child = conn.child.lock().await;
            stdin.close(&mut child, &conn.tree).await;
        }
        let file = conn.instruction_file.lock().await.take();
        remove_instruction_file(file.as_deref());
        fail_waiters(conn, "Pi was closed.").await;
    }
}

#[async_trait]
impl AgentAdapter for PiAdapter {
    /// Pi switches model inside a live session (`set_model` on the running RPC
    /// process keeps the conversation), so a model change loses no context.
    fn model_switch(&self) -> ModelSwitch {
        ModelSwitch::KeepsSession
    }

    async fn model_catalog(&self, runtime: &RuntimeSpec) -> Result<ModelCatalog, AdapterError> {
        // A throwaway process: `--no-session` keeps a catalogue read out of the
        // owner's Pi sessions. Nothing is listening on the event channel.
        let (events, _unused) = tokio::sync::mpsc::channel(1);
        let project = runtime.cwd.clone().unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
        let conn = Self::open(runtime, &project, &ExecOptions::default(), None, events).await?;
        let result = Self::request(&conn, "get_available_models", json!({}), CATALOG_TIMEOUT).await;
        Self::close(&conn).await;
        let data = result?;
        let models = protocol::parse_models(&data);
        let now = Utc::now();
        Ok(ModelCatalog {
            models,
            fetched_at: now.to_rfc3339_opts(SecondsFormat::Millis, true),
            expires_at: (now + ChronoDuration::minutes(15)).to_rfc3339_opts(SecondsFormat::Millis, true),
            fallback: false,
            source: "pi --mode rpc: get_available_models".to_owned(),
            validated: true,
        })
    }

    async fn probe(&self, runtime: &RuntimeSpec) -> Result<ProbeResult, AdapterError> {
        let mut cmd = Command::new(&runtime.executable);
        prepare_command(&mut cmd);
        cmd.args(&runtime.args).arg("--version").stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).kill_on_drop(true);
        let mut child = cmd.spawn().map_err(|error| AdapterError::Process(error.to_string()))?;
        let tree = ProcessTree::attach(&mut child).map_err(|error| AdapterError::Process(error.to_string()))?;
        let result = tokio::time::timeout(Duration::from_secs(5), child.wait_with_output()).await;
        let _ = tree.terminate();
        let output = result.map_err(|_| AdapterError::Process("Pi did not answer --version.".into()))?.map_err(|error| AdapterError::Process(error.to_string()))?;
        let version = String::from_utf8_lossy(&output.stdout).lines().next().map(str::trim).filter(|line| !line.is_empty() && line.len() <= 40).map(str::to_owned);
        Ok(ProbeResult { provider: runtime.provider.clone(), version, protocol: "pi-rpc".into(), authenticated: None, capabilities: capabilities() })
    }

    async fn new_session(&self, runtime: &RuntimeSpec, req: NewSessionRequest, events: EventSender) -> Result<SessionHandle, AdapterError> {
        let wanted = protocol::pi_session_id(&req.session_id);
        let conn = Self::open(runtime, &req.project_path, &req.exec_options, Some(&wanted), events).await?;
        let setup = async {
            Self::initialize(&conn, &wanted).await?;
            Self::apply_options(&conn, &req.exec_options, "").await
        }
        .await;
        if let Err(error) = setup {
            Self::close(&conn).await;
            return Err(error);
        }
        let provider_session_id = conn.provider_session_id.lock().await.clone().unwrap_or(wanted);
        let _ = conn.events.send(AgentEvent::SessionStarted { provider_session_id: provider_session_id.clone() }).await;
        self.sessions.lock().await.insert(req.session_id.clone(), conn);
        Ok(SessionHandle { session_id: req.session_id, provider_session_id, capabilities: capabilities() })
    }

    async fn resume_session(&self, runtime: &RuntimeSpec, req: ResumeSessionRequest, events: EventSender) -> Result<SessionHandle, AdapterError> {
        let wanted = protocol::pi_session_id(&req.provider_session_id);
        let conn = Self::open(runtime, &req.project_path, &req.exec_options, Some(&wanted), events).await?;
        let setup = async {
            let messages = Self::initialize(&conn, &wanted).await?;
            // Pi creates a session with that id when it has none, so an empty
            // history is the honest signal that the earlier context is gone.
            if messages == 0 {
                return Err(AdapterError::ResumeRejected);
            }
            Self::apply_options(&conn, &req.exec_options, "").await
        }
        .await;
        if let Err(error) = setup {
            Self::close(&conn).await;
            return Err(match error {
                AdapterError::Protocol(_) | AdapterError::Process(_) => AdapterError::ResumeFailed(match &error {
                    AdapterError::Protocol(message) | AdapterError::Process(message) => safe_error(message),
                    _ => "Pi did not resume the session.".to_owned(),
                }),
                other => other,
            });
        }
        self.sessions.lock().await.insert(req.session_id.clone(), conn);
        Ok(SessionHandle { session_id: req.session_id, provider_session_id: req.provider_session_id, capabilities: capabilities() })
    }

    async fn prompt(&self, handle: &SessionHandle, req: PromptRequest) -> Result<(), AdapterError> {
        let conn = self
            .sessions
            .lock()
            .await
            .get(&handle.session_id)
            .cloned()
            .ok_or_else(|| AdapterError::Process("This conversation's Pi process is not running.".into()))?;
        if !conn.alive.load(Ordering::Acquire) {
            return Err(AdapterError::Process("This conversation's Pi process has stopped.".into()));
        }
        let images = attachment_images(&req.attachments).await?;
        Self::apply_options(&conn, &req.exec_options, &req.turn_id).await?;

        // The settle channel exists before the prompt is sent, so a run that
        // finishes instantly cannot be missed.
        let (settle, settled) = oneshot::channel();
        {
            let mut turn = conn.turn.lock().await;
            *turn = Turn { id: Some(req.turn_id.clone()), settle: Some(settle), ..Turn::default() };
        }
        let mut command = json!({"message": req.text});
        if !images.is_empty() {
            command["images"] = Value::Array(images);
        }
        let accepted = match Self::request(&conn, "prompt", command, REQUEST_TIMEOUT).await {
            Ok(data) => data,
            Err(error) => {
                let mut turn = conn.turn.lock().await;
                turn.settle = None;
                turn.id = None;
                return Err(match error {
                    AdapterError::Protocol(message) => AdapterError::Rejected(safe_error(&message)),
                    other => other,
                });
            }
        };
        // `handled` means an extension consumed the prompt without starting a
        // run; Retinue starts no extensions, so this is reported as a turn that
        // ended rather than one that never began.
        let started = accepted["disposition"].as_str() != Some("handled");
        if started {
            let _ = conn.events.send(AgentEvent::PromptAccepted).await;
        }
        let outcome = match settled.await {
            Ok(outcome) => outcome,
            Err(_) => Settled::Died,
        };
        let (error, usage, model) = {
            let mut turn = conn.turn.lock().await;
            turn.close_response();
            let taken = std::mem::take(&mut *turn);
            (taken.error, taken.usage, conn.applied_model.lock().await.clone())
        };
        match outcome {
            Settled::Aborted => {
                let _ = conn.events.send(AgentEvent::TurnCancelled).await;
            }
            Settled::Died => {
                let _ = conn.events.send(AgentEvent::Error { message: "Pi stopped before the turn finished.".into() }).await;
                emit_usage(&conn, &req.turn_id, model, usage).await;
                return Err(AdapterError::TurnErrorReported("Pi stopped before the turn finished.".into()));
            }
            Settled::Completed => {
                match error {
                    Some(error) => {
                        let _ = conn.events.send(AgentEvent::Error { message: error }).await;
                    }
                    None => {
                        let _ = conn.events.send(AgentEvent::TurnCompleted).await;
                    }
                }
            }
        }
        emit_usage(&conn, &req.turn_id, model, usage).await;
        Ok(())
    }

    /// `steer` queues the text into the running turn; it is delivered after the
    /// current assistant turn's tool calls and before the next model call.
    async fn steer(&self, handle: &SessionHandle, req: SteerRequest) -> Result<SteerOutcome, AdapterError> {
        let Some(conn) = self.sessions.lock().await.get(&handle.session_id).cloned() else { return Ok(SteerOutcome::Stale) };
        if !conn.alive.load(Ordering::Acquire) {
            return Ok(SteerOutcome::Stale);
        }
        if conn.turn.lock().await.id.as_deref() != Some(req.turn_id.as_str()) {
            return Ok(SteerOutcome::Stale);
        }
        match Self::request(&conn, "steer", json!({"message": req.text}), REQUEST_TIMEOUT).await {
            Ok(_) => Ok(SteerOutcome::Accepted),
            // A refusal means Pi is no longer running that turn: the caller
            // queues the text instead of losing it.
            Err(AdapterError::Protocol(_)) => Ok(SteerOutcome::Stale),
            Err(error) => Err(error),
        }
    }

    async fn cancel(&self, handle: &SessionHandle) -> Result<(), AdapterError> {
        let Some(conn) = self.sessions.lock().await.get(&handle.session_id).cloned() else { return Ok(()) };
        if !conn.alive.load(Ordering::Acquire) {
            return Ok(());
        }
        // `abort` waits for the session to become idle before it answers, and a
        // long tool call can hold that for longer than a stop should take, so the
        // reply is given a short window: the abort is already in flight and the
        // settlement that follows is what ends the turn.
        match Self::request(&conn, "abort", json!({}), Duration::from_secs(3)).await {
            Ok(_) | Err(AdapterError::Timeout) | Err(AdapterError::Process(_)) => Ok(()),
            Err(error) => Err(error),
        }
    }

    async fn reply_permission(&self, _provider_request_id: &str, _choice: &str) -> Result<(), AdapterError> {
        Err(AdapterError::Unsupported("Pi has no approval prompt to answer.".into()))
    }

    async fn close_session(&self, handle: &SessionHandle) -> Result<(), AdapterError> {
        let conn = self.sessions.lock().await.remove(&handle.session_id);
        if let Some(conn) = conn {
            Self::close(&conn).await;
        }
        Ok(())
    }

    async fn shutdown(&self) {
        let connections = std::mem::take(&mut *self.sessions.lock().await);
        for conn in connections.into_values() {
            Self::close(&conn).await;
        }
    }
}

fn capabilities() -> AgentCapabilities {
    AgentCapabilities { resume: true, cancel: true, permissions: false, usage: true, files: false }
}

/// The arguments that put Pi into RPC mode for one conversation.
///
/// `--no-approve` is deliberate: a project folder can carry `.pi/settings.json`
/// and `.pi/extensions`, and non-interactive Pi cannot ask whether to load them,
/// so a bot never silently loads code from the folder it works in. Context files
/// (`AGENTS.md`, `CLAUDE.md`) still load, which is what a coding agent needs.
fn launch_args(runtime: &RuntimeSpec, options: &ExecOptions, session_id: Option<&str>, instruction_file: Option<&Path>) -> Vec<String> {
    let mut args = runtime.args.clone();
    args.extend(options.extra_args.iter().cloned());
    args.push("--mode".into());
    args.push("rpc".into());
    match session_id {
        Some(id) => {
            args.push("--session-id".into());
            args.push(id.to_owned());
        }
        None => args.push("--no-session".into()),
    }
    args.push("--no-approve".into());
    if let Some(file) = instruction_file {
        args.push("--append-system-prompt".into());
        args.push(file.to_string_lossy().into_owned());
    }
    if let Some(model) = options.model.as_deref().filter(|model| !model.is_empty()) {
        // A start-time model means the first prompt does not need `set_model`.
        args.push("--model".into());
        args.push(model.to_owned());
    }
    args
}

/// Queue one record. `StdinWriter::write` answers `WouldBlock` when its buffer
/// is full rather than waiting, so a short retry loop gives Pi time to read.
async fn write(stdin: &Mutex<StdinWriter>, bytes: &[u8]) -> Result<(), AdapterError> {
    let stdin = stdin.lock().await;
    for _ in 0..40 {
        match stdin.write(bytes).await {
            Ok(()) => return Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => tokio::time::sleep(Duration::from_millis(10)).await,
            Err(error) => return Err(AdapterError::Process(format!("writing to Pi: {error}"))),
        }
    }
    Err(AdapterError::Process("Pi is not reading its input.".into()))
}

/// Read Pi's stdout as JSONL: one response or one event per line. The reader
/// holds no strong reference, so closing the conversation ends it.
async fn read_stdout(conn: std::sync::Weak<Conn>, stdout: tokio::process::ChildStdout) {
    let mut lines = BufReader::new(stdout).lines();
    loop {
        let line = match lines.next_line().await {
            Ok(Some(line)) => line,
            _ => break,
        };
        let Some(conn) = conn.upgrade() else { return };
        match protocol::parse_record(&line) {
            Some(protocol::Record::Response(response)) => {
                let Some(id) = response.id else { continue };
                let Some(sender) = conn.pending.lock().await.remove(&id) else { continue };
                let _ = sender.send(if response.success {
                    Ok(response.data)
                } else {
                    Err(AdapterError::Protocol(response.error.unwrap_or_else(|| format!("Pi refused {}.", response.command))))
                });
            }
            Some(protocol::Record::Event(event)) => handle_event(&conn, event).await,
            None => continue,
        }
    }
    if let Some(conn) = conn.upgrade() {
        conn.alive.store(false, Ordering::Release);
        fail_waiters(&conn, "Pi exited.").await;
        let settle = conn.turn.lock().await.settle.take();
        if let Some(settle) = settle {
            let _ = settle.send(Settled::Died);
        }
    }
}

/// Fail every command and the turn waiting on this process.
async fn fail_waiters(conn: &Arc<Conn>, reason: &str) {
    let pending = std::mem::take(&mut *conn.pending.lock().await);
    for (_, sender) in pending {
        let _ = sender.send(Err(AdapterError::Process(reason.to_owned())));
    }
}

async fn handle_event(conn: &Arc<Conn>, event: Value) {
    if let Some(text) = protocol::text_delta(&event) {
        if !text.is_empty() {
            let _ = conn.events.send(AgentEvent::AssistantDelta { text: text.to_owned() }).await;
        }
    }
    if let Some(text) = protocol::thinking_delta(&event) {
        if !text.is_empty() {
            let _ = conn.events.send(AgentEvent::ThinkingDelta { text: text.to_owned() }).await;
        }
    }
    if let Some(error) = protocol::stream_error(&event) {
        conn.turn.lock().await.error = Some(safe_error(&error));
    }
    if event["type"].as_str() == Some("message_update") {
        if let Some(usage) = protocol::usage(&event["usage"]) {
            conn.turn.lock().await.response_usage = Some(usage);
        }
    }
    if let Some((id, name, arguments)) = protocol::tool_start(&event) {
        let raw = tool_raw(name, arguments);
        let title = tool_activity(name, &raw, name).label;
        let _ = conn
            .events
            .send(AgentEvent::ToolStarted { tool_call_id: id.to_owned(), kind: name.to_owned(), title, raw, parent_tool_call_id: None })
            .await;
    }
    if let Some((id, partial)) = protocol::tool_update(&event) {
        let _ = conn.events.send(AgentEvent::ToolUpdated { tool_call_id: id.to_owned(), raw: partial.clone() }).await;
    }
    if let Some((id, result, is_error)) = protocol::tool_end(&event) {
        let raw = json!({"result": result, "isError": is_error});
        let _ = conn.events.send(AgentEvent::ToolCompleted { tool_call_id: id.to_owned(), raw, parent_tool_call_id: None }).await;
    }
    if let Some(finished) = protocol::finished_message(&event) {
        if let Some(error) = finished.error {
            conn.turn.lock().await.error = Some(safe_error(&error));
        }
        if !finished.text.is_empty() {
            let _ = conn.events.send(AgentEvent::AssistantMessage { text: finished.text }).await;
        }
        conn.turn.lock().await.close_response();
    }
    if protocol::compaction_finished(&event) {
        let _ = conn.events.send(AgentEvent::ContextChanged { reason: ContextChangeReason::Compaction, context_lost: false }).await;
    }
    if let Some(aborted) = protocol::settled(&event) {
        let settle = conn.turn.lock().await.settle.take();
        if let Some(settle) = settle {
            let _ = settle.send(if aborted { Settled::Aborted } else { Settled::Completed });
        }
    }
}

/// A tool row's input, with the tool's name added so the shared presentation
/// helper (`tool_activity`) can classify it.
fn tool_raw(name: &str, arguments: &Value) -> Value {
    match arguments {
        Value::Object(map) => {
            let mut raw = map.clone();
            raw.insert("name".into(), json!(name));
            Value::Object(raw)
        }
        other => json!({"name": name, "input": other}),
    }
}

/// Report what this turn used. Per-response buckets are summed by the caller;
/// the context window comes from Pi's own session statistics.
async fn emit_usage(conn: &Arc<Conn>, turn_id: &str, model: Option<String>, usage: protocol::Usage) {
    let stats = PiAdapter::request(conn, "get_session_stats", json!({}), Duration::from_secs(5)).await.ok();
    let (context_used, context_size) = stats.as_ref().map(protocol::context_usage).unwrap_or((None, None));
    // Only the turn's own per-response buckets are reported. `get_session_stats`
    // counts the whole session, so its totals are used for the context window
    // and never presented as this turn's usage.
    let reported = (usage != protocol::Usage::default()).then_some(usage);
    let Some(reported) = reported else {
        if context_used.is_none() && context_size.is_none() {
            return;
        }
        let _ = conn
            .events
            .send(AgentEvent::UsageReport {
                turn_id: turn_id.to_owned(),
                report: UsageReport {
                    input_tokens: None,
                    output_tokens: None,
                    cache_read_tokens: None,
                    cache_write_tokens: None,
                    reasoning_tokens: None,
                    usage_status: "unreported".into(),
                    evidence_note: None,
                    provider_update_id: None,
                    context_used,
                    context_size,
                    model,
                    cost_minor: None,
                    cost_currency: None,
                    reported_cost_decimal: None,
                    cost_is_cumulative: false,
                },
            })
            .await;
        return;
    };
    let _ = conn
        .events
        .send(AgentEvent::UsageReport {
            turn_id: turn_id.to_owned(),
            report: UsageReport {
                input_tokens: reported.input,
                output_tokens: reported.output,
                cache_read_tokens: reported.cache_read,
                cache_write_tokens: reported.cache_write,
                reasoning_tokens: None,
                usage_status: "reported".into(),
                evidence_note: None,
                provider_update_id: None,
                context_used,
                context_size,
                model,
                cost_minor: None,
                // Pi's price table is in US dollars per million tokens, and its
                // own total is what is reported here; no conversion happens.
                cost_currency: reported.cost_usd.map(|_| "USD".to_owned()),
                reported_cost_decimal: reported.cost_usd.map(|cost| format!("{cost:.6}")),
                cost_is_cumulative: false,
            },
        })
        .await;
}

/// The prompt's images, read now and encoded for Pi's `ImageContent` records.
async fn attachment_images(attachments: &[PromptAttachment]) -> Result<Vec<Value>, AdapterError> {
    let mut images = Vec::with_capacity(attachments.len());
    for attachment in attachments {
        if !attachment.media_type.starts_with("image/") {
            return Err(AdapterError::Unsupported("Only images can be attached to a Pi prompt.".into()));
        }
        let metadata = tokio::fs::metadata(&attachment.path)
            .await
            .map_err(|_| AdapterError::Unsupported(format!("The image {} could not be read.", attachment.name)))?;
        if metadata.len() > MAX_ATTACHMENT_BYTES {
            return Err(AdapterError::Unsupported(format!("The image {} is too large to send.", attachment.name)));
        }
        let bytes = tokio::fs::read(&attachment.path)
            .await
            .map_err(|_| AdapterError::Unsupported(format!("The image {} could not be read.", attachment.name)))?;
        images.push(json!({
            "type": "image",
            "data": base64::engine::general_purpose::STANDARD.encode(bytes),
            "mimeType": attachment.media_type,
        }));
    }
    Ok(images)
}

/// Pi's stdout is diagnostics only, never protocol data, and never shown.
fn drain_stderr(stderr: tokio::process::ChildStderr) {
    tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        let mut remaining = 8192usize;
        while remaining > 0 {
            match lines.next_line().await {
                Ok(Some(line)) => remaining = remaining.saturating_sub(line.len()),
                _ => break,
            }
        }
    });
}

/// The instructions Pi appends to its system prompt: a private file, because a
/// start-time flag takes a path far better than a long command line.
fn write_instruction_file(text: &str) -> std::io::Result<PathBuf> {
    use std::io::Write;
    let dir = instruction_dir();
    std::fs::create_dir_all(&dir)?;
    sweep_instruction_files(&dir);
    let path = dir.join(format!("instruction-{}.txt", uuid::Uuid::new_v4()));
    let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&path)?;
    if let Err(error) = file.write_all(text.as_bytes()).and_then(|_| file.flush()) {
        drop(file);
        let _ = std::fs::remove_file(&path);
        return Err(error);
    }
    Ok(path)
}

fn instruction_dir() -> PathBuf {
    std::env::temp_dir().join(INSTRUCTION_DIR)
}

/// Files left by a crash are removed once they are a day old; a live process
/// inside that day keeps its own file.
fn sweep_instruction_files(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("txt") {
            continue;
        }
        let old = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .ok()
            .and_then(|modified| modified.elapsed().ok())
            .is_some_and(|age| age >= Duration::from_secs(24 * 60 * 60));
        if old {
            let _ = std::fs::remove_file(path);
        }
    }
}

fn remove_instruction_file(path: Option<&Path>) {
    if let Some(path) = path {
        let _ = std::fs::remove_file(path);
    }
}

/// Provider error text is shown to the owner: no control characters, no
/// key-shaped word, and a bounded length.
fn safe_error(text: &str) -> String {
    let cleaned = text.chars().filter(|ch| !ch.is_control() || *ch == ' ').map(|ch| if ch == '\n' || ch == '\r' { ' ' } else { ch }).collect::<String>();
    let redacted = cleaned
        .split(' ')
        .map(|word| if looks_like_secret(word) { "[hidden]" } else { word })
        .collect::<Vec<_>>()
        .join(" ");
    redacted.chars().take(ERROR_TEXT_MAX).collect::<String>().trim().to_owned()
}

/// A key-shaped word: 32 characters or more of one key-ish run, as
/// `retinue_security` treats one. Deliberately blunt; the cost of hiding a
/// harmless word is small beside printing a key.
fn looks_like_secret(word: &str) -> bool {
    let run = word.chars().filter(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.')).count();
    run >= 32 && word.chars().filter(|ch| ch.is_ascii_digit()).count() >= 4
}
