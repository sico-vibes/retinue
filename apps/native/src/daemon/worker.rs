use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;

use serde_json::{json, Value};

use super::client::{follow_events, Client, DaemonError};
use super::supervisor::{find_daemon, spawn_daemon};
use crate::state::{Session, Snapshot};

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
    Shutdown,
}

/// How the daemon is doing, for the window's own status line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DaemonState {
    Starting,
    Running,
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
    Members { project_id: String, agent_ids: Vec<String> },
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

async fn run(db_path: PathBuf, commands: async_channel::Receiver<Command>, updates: async_channel::Sender<Update>) {
    let _ = updates.send(Update::Daemon(DaemonState::Starting)).await;

    let executable = match find_daemon() {
        Ok(executable) => executable,
        Err(message) => {
            crate::trace::log(format!("daemon missing: {message}"));
            let _ = updates.send(Update::Daemon(DaemonState::Missing(message))).await;
            idle(commands).await;
            return;
        }
    };
    crate::trace::log(format!("daemon found: {}", executable.display()));
    if let Some(folder) = db_path.parent() {
        let _ = std::fs::create_dir_all(folder);
    }
    let process = match spawn_daemon(&executable, &db_path) {
        Ok(process) => process,
        Err(message) => {
            crate::trace::log(format!("daemon did not start: {message}"));
            let _ = updates.send(Update::Daemon(DaemonState::Missing(message))).await;
            idle(commands).await;
            return;
        }
    };
    // The address is fine to log; the capability never is (its Debug is redacted).
    crate::trace::log(format!("daemon started at {}", process.connection.address()));

    let client = match Client::new(process.connection.clone()) {
        Ok(client) => client,
        Err(message) => {
            let _ = updates.send(Update::Daemon(DaemonState::Missing(message))).await;
            process.shutdown();
            idle(commands).await;
            return;
        }
    };
    let cursor = Arc::new(AtomicU64::new(0));
    let _ = updates.send(Update::Daemon(DaemonState::Running)).await;
    fetch_snapshot(&client, &cursor, &updates).await;

    let events = tokio::spawn(follow_events(process.connection.clone(), cursor.clone(), updates.clone()));
    serve_commands(&client, &cursor, &commands, &updates).await;
    crate::trace::log("worker stopping: killing the daemon child");
    events.abort();
    process.shutdown();
    crate::trace::log("daemon child stopped");
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
                        "snapshot: {} employees and bots, {} conversations, {} projects, {} waiting approvals",
                        snapshot.agents.len(),
                        snapshot.sessions.len(),
                        snapshot.projects.len(),
                        snapshot.permissions.len()
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

async fn serve_commands(
    client: &Client,
    cursor: &Arc<AtomicU64>,
    commands: &async_channel::Receiver<Command>,
    updates: &async_channel::Sender<Update>,
) {
    while let Ok(command) = commands.recv().await {
        let keep_going = match command {
            Command::Shutdown => false,
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
                for project_id in project_ids {
                    // An older daemon has no hiring table; the sidebar then falls
                    // back to conversations and default projects, as it does today.
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
                        let _ = updates.send(Update::Members { project_id, agent_ids }).await;
                    }
                }
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
        };
        if !keep_going {
            break;
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
