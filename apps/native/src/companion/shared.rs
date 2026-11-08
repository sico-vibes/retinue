//! What the main window and the companion both read, and the one place the
//! companion talks to the daemon.
//!
//! There is exactly one daemon connection: [`crate::daemon::start`] runs in the
//! main window's `Root`. The companion gets the same snapshot and the same
//! events through this entity, which `Root` feeds with three lines in
//! `on_update` (see the report). Commands go back through the same `Root`, so a
//! second connection is never opened and the daemon's ordering is untouched.

#![allow(dead_code)]  // A ported surface: the helpers are complete and
// slice 6's settings page and slice 7 wire the rest.

use gpui::{AnyWindowHandle, App, WeakEntity};

use crate::daemon::{Command, DaemonState};
use crate::state::{apply_event, Agent, Session, Snapshot};
use crate::ui::root::Root;

pub struct Shared {
    snapshot: Option<Snapshot>,
    connected: bool,
    daemon_state: DaemonState,
    /// The conversation the main window has open, which the companion follows.
    session_id: Option<String>,
    /// The bot the main window has focused.
    agent_id: Option<String>,
    root: Option<WeakEntity<Root>>,
    /// The main window, so "Open in Bitbots" can bring it forward.
    pub main_window: Option<AnyWindowHandle>,
    /// The companion window, so a dictation or a click can activate it.
    pub companion_window: Option<AnyWindowHandle>,
    /// Bumped on every change, so a view can tell whether it is behind.
    pub revision: u64,
}

impl Shared {
    pub fn new() -> Self {
        Self {
            snapshot: None,
            connected: false,
            daemon_state: DaemonState::Starting,
            session_id: None,
            agent_id: None,
            root: None,
            main_window: None,
            companion_window: None,
            revision: 0,
        }
    }

    /// The main window's model, so a command can be sent through its daemon.
    pub fn attach_root(&mut self, root: WeakEntity<Root>) {
        self.root = Some(root);
    }

    // ---- what the views read --------------------------------------------------

    pub fn snapshot(&self) -> Option<&Snapshot> {
        self.snapshot.as_ref()
    }

    pub fn connected(&self) -> bool {
        self.connected
    }

    pub fn daemon_state(&self) -> &DaemonState {
        &self.daemon_state
    }

    pub fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }

    pub fn agent_id(&self) -> Option<&str> {
        self.agent_id.as_deref()
    }

    pub fn session(&self, id: &str) -> Option<&Session> {
        self.snapshot.as_ref()?.session(id)
    }

    pub fn session_of(&self, id: Option<&str>) -> Option<&Session> {
        let id = id?;
        self.session(id)
    }

    pub fn agent(&self, id: &str) -> Option<&Agent> {
        self.snapshot.as_ref()?.agent(id)
    }

    /// The open conversation's bot, else the focused one, else the manager.
    pub fn focused_agent(&self) -> Option<&Agent> {
        let snapshot = self.snapshot.as_ref()?;
        let from_session = self
            .session_id
            .as_deref()
            .and_then(|id| snapshot.session(id))
            .and_then(|session| session.agent_id.as_deref());
        from_session
            .or(self.agent_id.as_deref())
            .and_then(|id| snapshot.agent(id))
            .or_else(|| snapshot.manager())
    }

    // ---- what `Root` pushes in ------------------------------------------------

    /// The snapshot arrived (`Update::Snapshot`).
    pub fn mirror_snapshot(&mut self, snapshot: Snapshot, cx: &mut gpui::Context<Self>) {
        self.snapshot = Some(snapshot);
        self.bump(cx);
    }

    /// One ordered event (`Update::Event`), reduced by the same code the main
    /// window uses, so both windows can never disagree about a conversation.
    pub fn mirror_event(&mut self, event: &crate::daemon::client::DaemonEvent, cx: &mut gpui::Context<Self>) {
        if let Some(snapshot) = self.snapshot.as_mut() {
            apply_event(snapshot, event);
            // These flags are the main window's business; keep them from
            // piling up in the mirror.
            snapshot.dirty_members.clear();
            snapshot.refresh_wanted = false;
            self.bump(cx);
        }
    }

    pub fn set_connected(&mut self, connected: bool, cx: &mut gpui::Context<Self>) {
        if self.connected != connected {
            self.connected = connected;
            self.bump(cx);
        }
    }

    pub fn set_daemon_state(&mut self, state: DaemonState, cx: &mut gpui::Context<Self>) {
        if self.daemon_state != state {
            self.daemon_state = state;
            self.bump(cx);
        }
    }

    /// The main window's selection, so the companion shows the same conversation
    /// and "Open in Bitbots" knows where to go.
    pub fn set_selection(&mut self, session_id: Option<String>, agent_id: Option<String>, cx: &mut gpui::Context<Self>) {
        if self.session_id != session_id || self.agent_id != agent_id {
            self.session_id = session_id;
            self.agent_id = agent_id;
            self.bump(cx);
        }
    }

    fn bump(&mut self, cx: &mut gpui::Context<Self>) {
        self.revision = self.revision.wrapping_add(1);
        cx.notify();
    }

    // ---- what the companion asks for -----------------------------------------

    /// Send one command through the main window's daemon connection.
    pub fn send(&mut self, command: Command, cx: &mut gpui::Context<Self>) -> bool {
        let Some(root) = self.root.as_ref().and_then(WeakEntity::upgrade) else { return false };
        root.update(cx, |root, cx| root.daemon_command(command, cx));
        true
    }

    /// Open a conversation in the main window and bring that window forward.
    pub fn open_in_main(&mut self, agent_id: Option<String>, session_id: String, cx: &mut gpui::Context<Self>) {
        if let Some(root) = self.root.as_ref().and_then(WeakEntity::upgrade) {
            root.update(cx, |root, cx| {
                if let Some(agent_id) = agent_id.clone() {
                    root.open_conversation(agent_id, session_id.clone(), cx);
                }
            });
        }
        self.activate_main(cx);
    }

    pub fn activate_main(&self, cx: &mut App) {
        if let Some(handle) = self.main_window {
            let _ = handle.update(cx, |_, window, _| window.activate_window());
        }
    }

    pub fn activate_companion(&self, cx: &mut App) {
        if let Some(handle) = self.companion_window {
            let _ = handle.update(cx, |_, window, _| window.activate_window());
        }
    }

    /// Send a message to the open conversation, or start one when the bot has
    /// none: the companion's composer and its file drop both land here.
    pub fn send_prompt(&mut self, text: String, cx: &mut gpui::Context<Self>) {
        if let Some(session_id) = self.session_id.clone() {
            // A turn already running takes the message into its queue, as the
            // main window's composer does.
            let busy = self.snapshot.as_ref().and_then(|snapshot| snapshot.session(&session_id)).is_some_and(|session| session.is_busy());
            let method = if busy { crate::daemon::PromptMethod::Queue } else { crate::daemon::PromptMethod::Prompt };
            self.send(
                Command::SendPrompt {
                    session_id,
                    text,
                    attachments: Vec::new(),
                    method,
                    client_message_id: uuid::Uuid::new_v4().to_string(),
                },
                cx,
            );
            return;
        }
        let Some(agent_id) = self.focused_agent().map(|agent| agent.id.clone()) else { return };
        if let Some(root) = self.root.as_ref().and_then(WeakEntity::upgrade) {
            root.update(cx, |root, cx| {
                root.pending_prompt = Some(text);
                root.new_conversation(agent_id, None, cx);
            });
        }
    }
}

impl Default for Shared {
    fn default() -> Self {
        Self::new()
    }
}
