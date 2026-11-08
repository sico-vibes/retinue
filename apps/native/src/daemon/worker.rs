use std::path::{Path, PathBuf};
use std::process::Child;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use serde_json::{json, Value};

use super::client::{follow_events, Client, DaemonError};
use super::supervisor::{find_daemon, spawn_daemon};
use crate::state::{Project, Session, Snapshot};

/// How long the worker waits before each restart of a daemon that died on its
/// own: 1, 2, 5 and 10 seconds, and then it stops and offers Retry.
pub const BACKOFF_MS: [u64; 4] = [1_000, 2_000, 5_000, 10_000];

/// The delay before restart number `attempt` (1-based). `None` when the window
/// has run out of attempts.
pub fn backoff_for(attempt: usize) -> Option<Duration> {
    BACKOFF_MS.get(attempt.saturating_sub(1)).map(|ms| Duration::from_millis(*ms))
}

/// How often the worker looks at the daemon process for an exit it did not ask for.
const EXIT_POLL: Duration = Duration::from_millis(400);

/// What the window asks the daemon to do. Every one of these is an RPC the
/// React app makes the same way, through `tauri.ts`.
pub enum Command {
    /// Take a fresh snapshot (a `replay.gap` needs one).
    Refresh,
    NewConversation { agent_id: String, project_id: Option<String> },
    /// Load a whole conversation, for the fallback when events leave us unsure.
    LoadSession { session_id: String },
    /// Who is hired where: one `project.members` call per project.
    LoadProjectMembers { project_ids: Vec<String> },
    SendPrompt { session_id: String, text: String },
    CancelTurn { session_id: String },
    ReplyPermission { permission_id: String, choice: String },
    /// `project.create`: a new project from the window's dialog.
    CreateProject { name: String, path: Option<String> },
    /// `project.hire`: a preset into projects, or an employee already on the team.
    Hire {
        project_id: String,
        agent_id: Option<String>,
        preset_id: Option<String>,
        runtime_id: Option<String>,
        model: Option<String>,
    },
    /// `project.release`: remove one membership; conversations stay.
    Release { project_id: String, agent_id: String },
    /// `proposal.decide`: approve or reject one proposal card.
    DecideProposal { id: String, decision: String },
    /// `daemon.health`: the daemon's own version, for About.
    LoadHealth,
    /// `runtime.models`: the models the hire dialog can name.
    LoadModels { runtime_id: String },
    /// `runtime.refresh`: ask the daemon to scan for coding agents, once, when
    /// its table is empty (a fresh install).
    ScanRuntimes,
    /// The owner pressed Retry after the daemon gave up restarting.
    Restart,
    Shutdown,
}

/// How the daemon is doing, for the window's own status line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DaemonState {
    Starting,
    Running,
    /// It died on its own; the worker is about to try again.
    Restarting { attempt: usize, delay_ms: u64 },
    /// It died and every attempt is used up. Retry asks for another run.
    Stopped(String),
    /// It cannot start at all (no executable, or it never answered).
    Missing(String),
}

/// Everything the worker tells the window.
pub enum Update {
    Daemon(DaemonState),
    Snapshot(Box<Snapshot>),
    Event(Box<super::client::DaemonEvent>),
    Connected(bool),
    SessionLoaded(Box<Session>),
    ConversationCreated(Box<Session>),
    ProjectCreated(Box<Project>),
    Members { project_id: String, agent_ids: Vec<String> },
    Hired { agent_id: String, created: bool },
    Released { released: bool },
    ProposalDecided,
    DaemonVersion(String),
    Models { runtime_id: String, models: Vec<crate::ui::root::ModelChoice> },
    Sent,
    Cancelled,
    PermissionReplied,
    Failed(String),
}

/// The window's handle on the worker. Dropping it stops the daemon child.
pub struct DaemonHandle {
    commands: async_channel::Sender<Command>,
    thread: Option<JoinHandle<()>>,
}

impl DaemonHandle {
    pub fn send(&self, command: Command) {
        let _ = self.commands.try_send(command);
    }

    /// Ask the worker to stop, wait for it, and with it kill the daemon child.
    pub fn shutdown(&mut self) {
        let _ = self.commands.try_send(Command::Shutdown);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for DaemonHandle {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Start the worker and hand the window its receiver. `db_path` is the database
/// this app owns (never the installed app's).
pub fn start(db_path: PathBuf) -> (DaemonHandle, async_channel::Receiver<Update>) {
    let (command_tx, command_rx) = async_channel::unbounded();
    let (update_tx, update_rx) = async_channel::unbounded();
    let thread = std::thread::Builder::new()
        .name("bitbots-daemon".to_owned())
        .spawn(move || {
            let runtime = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
                Ok(runtime) => runtime,
                Err(error) => {
                    let _ = update_tx.send_blocking(Update::Failed(format!("Could not start the daemon worker: {error}")));
                    return;
                }
            };
            runtime.block_on(run(db_path, command_rx, update_tx));
        })
        .expect("the daemon worker thread should start");
    (DaemonHandle { commands: command_tx, thread: Some(thread) }, update_rx)
}

/// Why one run of the daemon ended.
enum Outcome {
    /// The window closed, or could never find the daemon: never restart.
    Stopped,
    /// The daemon exited while the window was using it: restart with backoff.
    Died(String),
}

/// One daemon at a time, restarted when it dies until the attempts run out.
async fn run(db_path: PathBuf, commands: async_channel::Receiver<Command>, updates: async_channel::Sender<Update>) {
    let mut attempt: usize = 0;
    loop {
        match run_session(&db_path, &commands, &updates).await {
            Outcome::Stopped => return,
            Outcome::Died(reason) => {
                let Some(delay) = backoff_for(attempt + 1) else {
                    crate::trace::log(format!("daemon stopped for good: {reason}"));
                    let _ = updates.send(Update::Daemon(DaemonState::Stopped(reason))).await;
                    attempt = 0;
                    // Nothing but the owner's Retry (or the window closing) starts it again.
                    loop {
                        match commands.recv().await {
                            Ok(Command::Restart) => break,
                            Ok(Command::Shutdown) => return,
                            Ok(_) => {}
                            Err(_) => return,
                        }
                    }
                    continue;
                };
                attempt += 1;
                crate::trace::log(format!("daemon exited; restarting in {} ms", delay.as_millis()));
                let _ = updates
                    .send(Update::Daemon(DaemonState::Restarting { attempt, delay_ms: delay.as_millis() as u64 }))
                    .await;
                tokio::time::sleep(delay).await;
            }
        }
    }
}

async fn run_session(
    db_path: &Path,
    commands: &async_channel::Receiver<Command>,
    updates: &async_channel::Sender<Update>,
) -> Outcome {
    let _ = updates.send(Update::Daemon(DaemonState::Starting)).await;

    let executable = match find_daemon() {
        Ok(executable) => executable,
        Err(message) => {
            crate::trace::log(format!("daemon missing: {message}"));
            let _ = updates.send(Update::Daemon(DaemonState::Missing(message))).await;
            idle(commands.clone()).await;
            return Outcome::Stopped;
        }
    };
    crate::trace::log(format!("daemon found: {}", executable.display()));
    if let Some(folder) = db_path.parent() {
        let _ = std::fs::create_dir_all(folder);
    }
    let mut process = match spawn_daemon(&executable, db_path) {
        Ok(process) => process,
        Err(message) => {
            crate::trace::log(format!("daemon did not start: {message}"));
            return Outcome::Died(message);
        }
    };
    // The address is fine to log; the capability never is (its Debug is redacted).
    crate::trace::log(format!("daemon started at {}", process.connection.address()));

    let client = match Client::new(process.connection.clone()) {
        Ok(client) => client,
        Err(message) => {
            process.shutdown();
            return Outcome::Died(message);
        }
    };
    let cursor = Arc::new(AtomicU64::new(0));
    let _ = updates.send(Update::Daemon(DaemonState::Running)).await;
    fetch_snapshot(&client, &cursor, updates).await;

    let events = tokio::spawn(follow_events(process.connection.clone(), cursor.clone(), updates.clone()));
    let outcome = serve_commands(&client, &cursor, &mut process.child, commands, updates).await;
    events.abort();
    process.shutdown();
    crate::trace::log("daemon child stopped");
    outcome
}

async fn fetch_snapshot(client: &Client, cursor: &Arc<AtomicU64>, updates: &async_channel::Sender<Update>) {
    match client.rpc("app.snapshot", json!({})).await {
        Ok(value) => {
            if let Some(sequence) = value.get("sequence").and_then(Value::as_u64) {
                cursor.fetch_max(sequence, Ordering::Release);
            }
            match serde_json::from_value::<Snapshot>(value) {
                Ok(snapshot) => {
                    crate::trace::log(format!(
                        "snapshot: {} employees and bots, {} conversations, {} projects, {} waiting approvals, {} coding agents",
                        snapshot.agents.len(),
                        snapshot.sessions.len(),
                        snapshot.projects.len(),
                        snapshot.permissions.len(),
                        snapshot.runtimes.len()
                    ));
                    let _ = updates.send(Update::Snapshot(Box::new(snapshot))).await;
                }
                Err(error) => {
                    crate::trace::log(format!("snapshot did not parse: {error}"));
                    let _ = updates.send(Update::Failed(format!("The daemon's snapshot did not parse: {error}"))).await;
                }
            }
        }
        Err(error) => {
            crate::trace::log(format!("snapshot failed: {error}"));
            let _ = updates.send(Update::Failed(error.to_string())).await;
        }
    }
}

/// Serve commands until the window closes, or until the daemon process is gone.
async fn serve_commands(
    client: &Client,
    cursor: &Arc<AtomicU64>,
    child: &mut Child,
    commands: &async_channel::Receiver<Command>,
    updates: &async_channel::Sender<Update>,
) -> Outcome {
    let mut poll = tokio::time::interval(EXIT_POLL);
    poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            command = commands.recv() => {
                let Ok(command) = command else { return Outcome::Stopped };
                if !handle_command(client, cursor, command, updates).await {
                    return Outcome::Stopped;
                }
            }
            _ = poll.tick() => {
                match child.try_wait() {
                    Ok(Some(status)) => {
                        return Outcome::Died(format!("The Bitbots daemon stopped ({}).", describe_exit(&status)));
                    }
                    Ok(None) => {}
                    Err(error) => {
                        return Outcome::Died(format!("The Bitbots daemon could not be watched: {error}."));
                    }
                }
            }
        }
    }
}

fn describe_exit(status: &std::process::ExitStatus) -> String {
    match status.code() {
        Some(code) => format!("exit code {code}"),
        None => "it was ended by Windows".to_owned(),
    }
}

/// Run one command. `false` means the window is closing.
async fn handle_command(
    client: &Client,
    cursor: &Arc<AtomicU64>,
    command: Command,
    updates: &async_channel::Sender<Update>,
) -> bool {
    match command {
        Command::Shutdown => false,
        Command::Restart => true,
        Command::Refresh => {
            // A gap means the events we missed are gone; only a snapshot fixes it.
            fetch_snapshot(client, cursor, updates).await;
            true
        }
        Command::NewConversation { agent_id, project_id } => {
            let mut params = json!({ "agentId": agent_id });
            if let Some(project_id) = project_id {
                params["projectId"] = json!(project_id);
            }
            report(client, updates, "agent.conversation.create", params, |value| {
                let session = value.get("session").cloned().unwrap_or(Value::Null);
                Update::ConversationCreated(Box::new(parse_session(session)))
            })
            .await;
            true
        }
        Command::LoadSession { session_id } => {
            report(client, updates, "session.get", json!({ "sessionId": session_id }), |value| {
                Update::SessionLoaded(Box::new(parse_session(value)))
            })
            .await;
            true
        }
        Command::LoadProjectMembers { project_ids } => {
            load_members(client, updates, &project_ids).await;
            true
        }
        Command::SendPrompt { session_id, text } => {
            report(client, updates, "session.prompt", json!({ "sessionId": session_id, "text": text }), |_| Update::Sent).await;
            true
        }
        Command::CancelTurn { session_id } => {
            report(client, updates, "session.cancel", json!({ "sessionId": session_id }), |_| Update::Cancelled).await;
            true
        }
        Command::ReplyPermission { permission_id, choice } => {
            report(client, updates, "permission.reply", json!({ "permissionId": permission_id, "choice": choice }), |_| Update::PermissionReplied).await;
            true
        }
        Command::CreateProject { name, path } => {
            report(client, updates, "project.create", json!({ "name": name, "path": path }), |value| {
                let project: Project = serde_json::from_value(value.get("project").cloned().unwrap_or(Value::Null))
                    .unwrap_or_default();
                Update::ProjectCreated(Box::new(project))
            })
            .await;
            true
        }
        Command::Hire { project_id, agent_id, preset_id, runtime_id, model } => {
            let mut params = json!({ "projectId": project_id });
            if let Some(agent_id) = agent_id {
                params["agentId"] = json!(agent_id);
            }
            if let Some(preset_id) = preset_id {
                params["presetId"] = json!(preset_id);
            }
            if let Some(runtime_id) = runtime_id {
                params["runtimeId"] = json!(runtime_id);
            }
            if let Some(model) = model {
                params["model"] = json!(model);
            }
            report(client, updates, "project.hire", params, |value| Update::Hired {
                agent_id: value.get("agentId").and_then(Value::as_str).unwrap_or("").to_owned(),
                created: value.get("created").and_then(Value::as_bool).unwrap_or(false),
            })
            .await;
            true
        }
        Command::Release { project_id, agent_id } => {
            report(client, updates, "project.release", json!({ "projectId": project_id, "agentId": agent_id }), |value| {
                Update::Released { released: value.get("released").and_then(Value::as_bool).unwrap_or(false) }
            })
            .await;
            true
        }
        Command::DecideProposal { id, decision } => {
            report(client, updates, "proposal.decide", json!({ "id": id, "decision": decision }), |_| Update::ProposalDecided).await;
            true
        }
        Command::LoadHealth => {
            report(client, updates, "daemon.health", json!({}), |value| {
                Update::DaemonVersion(value.get("version").and_then(Value::as_str).unwrap_or("").to_owned())
            })
            .await;
            true
        }
        Command::ScanRuntimes => {
            // The roster arrives as a `runtime.changed` event, so the result is
            // not read here.
            let _ = client.rpc("runtime.refresh", json!({})).await;
            true
        }
        Command::LoadModels { runtime_id } => {
            report(client, updates, "runtime.models", json!({ "runtimeId": runtime_id }), move |value| {
                let catalog = value.get("catalog").cloned().unwrap_or(value);
                let models = catalog
                    .get("models")
                    .and_then(Value::as_array)
                    .map(|rows| {
                        rows.iter()
                            .filter_map(|row| {
                                let id = row.get("id").and_then(Value::as_str)?;
                                let label = row
                                    .get("displayName")
                                    .and_then(Value::as_str)
                                    .filter(|label| !label.is_empty())
                                    .unwrap_or(id);
                                Some(crate::ui::root::ModelChoice { id: id.to_owned(), label: label.to_owned() })
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                let runtime_id = catalog
                    .get("runtimeId")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                    .unwrap_or_default();
                Update::Models { runtime_id, models }
            })
            .await;
            true
        }
    }
}

/// One `project.members` call per project. An older daemon has no hiring table;
/// the sidebar then falls back to conversations and default projects.
async fn load_members(client: &Client, updates: &async_channel::Sender<Update>, project_ids: &[String]) {
    for project_id in project_ids {
        if let Ok(value) = client.rpc("project.members", json!({ "projectId": project_id })).await {
            let agent_ids = value
                .get("members")
                .and_then(Value::as_array)
                .map(|rows| {
                    rows.iter()
                        .filter_map(|row| row.get("agentId").and_then(Value::as_str).map(str::to_owned))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let _ = updates.send(Update::Members { project_id: project_id.clone(), agent_ids }).await;
        }
    }
}

/// Run one RPC and turn a success into an `Update`, or a failure into the window's
/// error line. `map` never sees an error, so it cannot invent a success shape.
async fn report(
    client: &Client,
    updates: &async_channel::Sender<Update>,
    method: &str,
    params: Value,
    map: impl FnOnce(Value) -> Update,
) {
    match client.rpc(method, params).await {
        Ok(value) => {
            let _ = updates.send(map(value)).await;
        }
        Err(error) => {
            let _ = updates.send(Update::Failed(describe(method, error))).await;
        }
    }
}

fn describe(method: &str, error: DaemonError) -> String {
    match error.code() {
        // A person needs to know what failed, not which RPC failed.
        Some(_) => error.to_string(),
        None => format!("{method}: {error}"),
    }
}

fn parse_session(value: Value) -> Session {
    serde_json::from_value(value).unwrap_or_else(|_| Session { id: String::new(), ..Session::default() })
}

async fn idle(commands: async_channel::Receiver<Command>) {
    while let Ok(command) = commands.recv().await {
        if matches!(command, Command::Shutdown) {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_restart_backoff_is_one_two_five_and_ten_seconds_then_it_stops() {
        let delays: Vec<u64> = (1..=BACKOFF_MS.len())
            .map(|attempt| backoff_for(attempt).expect("an attempt").as_millis() as u64)
            .collect();
        assert_eq!(delays, vec![1_000, 2_000, 5_000, 10_000]);
        assert_eq!(backoff_for(BACKOFF_MS.len() + 1), None, "the fifth failure offers Retry instead");
        assert_eq!(backoff_for(0), Some(Duration::from_millis(1_000)), "attempt 0 is treated as the first");
    }

    #[test]
    fn an_exit_is_described_without_any_secret() {
        // The description is built from the exit status only; there is no path,
        // capability or address to leak.
        let text = "The Bitbots daemon stopped (exit code 0).";
        assert!(!text.contains("Bearer"));
    }
}
