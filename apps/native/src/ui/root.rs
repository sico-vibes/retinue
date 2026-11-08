//! The window: one main view that owns the daemon connection, the selection and
//! the three columns (sidebar, conversations, conversation).

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use gpui::{
    div, prelude::*, px, App, AsyncApp, Context, Entity, ListAlignment, ListState,
    SharedString, Window,
};

use crate::daemon::{start, Command, DaemonHandle, DaemonState, Update};
use crate::paths;
use crate::settings_file;
use crate::state::{apply_event, Agent, Session, Snapshot};
use crate::theme::{self, haze_dark, Colors};
use crate::ui::text_input::TextInput;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SidebarView {
    Projects,
    Team,
}

/// The first-run question: the installed Bitbots has a database, this app does
/// not. Asked once, in the window, before any daemon starts.
pub struct FirstRunQuestion {
    pub source: PathBuf,
    pub target: PathBuf,
    pub error: Option<String>,
}

pub struct Root {
    pub(crate) daemon: Option<DaemonHandle>,
    pub(crate) snapshot: Option<Snapshot>,
    pub(crate) daemon_state: DaemonState,
    pub(crate) connected: bool,
    pub(crate) error: Option<String>,
    pub(crate) notice: Option<String>,
    pub(crate) agent_id: Option<String>,
    pub(crate) session_id: Option<String>,
    pub(crate) view: SidebarView,
    pub(crate) folded_projects: BTreeSet<String>,
    /// An employee's own fold choice, once the owner has made one.
    pub(crate) employee_folds: BTreeMap<String, bool>,
    pub(crate) members: Option<crate::state::projects::MembersByProject>,
    pub(crate) composer: Entity<TextInput>,
    pub(crate) list: ListState,
    pub(crate) list_session: Option<String>,
    pub(crate) list_count: usize,
    pub(crate) pending_prompt: Option<String>,
    pub(crate) first_run: Option<FirstRunQuestion>,
}

impl Root {
    pub fn new(db_path: PathBuf, cx: &mut Context<Self>) -> Self {
        let installed = paths::installed_db_path();
        let needs_question = !db_path.exists() && installed.is_file();
        // The live check has nobody at the keyboard to answer the prompt, so it
        // names its answer up front. It takes the same code path the buttons do.
        let first_run_choice = std::env::var("BITBOTS_NATIVE_FIRST_RUN").ok();
        let first_run = match first_run_choice.as_deref() {
            Some("empty") => None,
            Some("copy") => {
                if needs_question {
                    if let Err(error) = copy_database(&installed, &db_path) {
                        Some(FirstRunQuestion { source: installed.clone(), target: db_path.clone(), error: Some(error) })
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            _ => needs_question.then(|| FirstRunQuestion {
                source: installed.clone(),
                target: db_path.clone(),
                error: None,
            }),
        };

        let (daemon, updates) = if first_run.is_none() {
            let (handle, updates) = start(db_path);
            (Some(handle), Some(updates))
        } else {
            (None, None)
        };

        let this = cx.weak_entity();
        let composer = cx.new(|cx| {
            let mut input = TextInput::new("Message your bot…", px(20.0), px(theme::FS_MD), cx);
            let this = this.clone();
            input.set_on_submit(Box::new(move |window, cx: &mut App| {
                if let Some(root) = this.upgrade() {
                    root.update(cx, |root, cx| root.send(window, cx));
                }
            }));
            input
        });

        if let Some(updates) = updates {
            cx.spawn(async move |this, cx: &mut AsyncApp| {
                while let Ok(update) = updates.recv().await {
                    let mut slot = Some(update);
                    if this.upgrade().is_none() {
                        break;
                    }
                    cx.update(|app| {
                        let Some(root) = this.upgrade() else { return };
                        if let Some(update) = slot.take() {
                            root.update(app, |root, cx| root.on_update(update, cx));
                        }
                    });
                }
            })
            .detach();
        }

        Self {
            daemon,
            snapshot: None,
            daemon_state: DaemonState::Starting,
            connected: false,
            error: None,
            notice: None,
            agent_id: None,
            session_id: None,
            view: SidebarView::Projects,
            folded_projects: settings_file::read_folded_projects(&paths::settings_file_path()),
            employee_folds: BTreeMap::new(),
            members: None,
            composer,
            list: ListState::new(0, ListAlignment::Bottom, px(600.0)),
            list_session: None,
            list_count: 0,
            pending_prompt: None,
            first_run,
        }
    }

    /// The composer's focus handle, so the window can put the caret in it on open.
    pub fn composer_handle(&self, cx: &App) -> gpui::FocusHandle {
        self.composer.read(cx).handle()
    }

    // ---- data the panels read -------------------------------------------------

    pub(crate) fn snapshot(&self) -> Option<&Snapshot> {
        self.snapshot.as_ref()
    }

    pub(crate) fn agents(&self) -> &[Agent] {
        self.snapshot.as_ref().map(|snapshot| snapshot.agents.as_slice()).unwrap_or(&[])
    }

    pub(crate) fn sessions(&self) -> &[Session] {
        self.snapshot.as_ref().map(|snapshot| snapshot.sessions.as_slice()).unwrap_or(&[])
    }

    pub(crate) fn selected_agent(&self) -> Option<&Agent> {
        let id = self.agent_id.as_deref()?;
        self.snapshot.as_ref()?.agent(id)
    }

    pub(crate) fn selected_session(&self) -> Option<&Session> {
        let id = self.session_id.as_deref()?;
        self.snapshot.as_ref()?.session(id)
    }

    /// The bots the sidebar lists: employees, not the CEO, not sub-bots.
    pub(crate) fn employees(&self) -> Vec<&Agent> {
        self.agents().iter().filter(|agent| agent.is_employee()).collect()
    }

    pub(crate) fn approval_session_ids(&self) -> BTreeSet<String> {
        self.snapshot
            .as_ref()
            .map(|snapshot| snapshot.permissions.iter().filter_map(|permission| permission.session_id.clone()).collect())
            .unwrap_or_default()
    }

    /// Whether a waiting approval belongs to a conversation of this employee.
    pub(crate) fn employee_needs_you(&self, agent_id: &str, approvals: &BTreeSet<String>) -> bool {
        self.sessions()
            .iter()
            .any(|session| session.agent_id.as_deref() == Some(agent_id) && approvals.contains(&session.id))
    }

    // ---- talking to the daemon ------------------------------------------------

    fn daemon_command(&mut self, command: Command, _cx: &mut Context<Self>) {
        if let Some(daemon) = self.daemon.as_ref() {
            daemon.send(command);
        } else {
            self.error = Some("The local Bitbots daemon is not running.".to_owned());
        }
    }

    fn on_update(&mut self, update: Update, cx: &mut Context<Self>) {
        match update {
            Update::Daemon(state) => {
                if let DaemonState::Missing(message) = &state {
                    self.error = Some(message.clone());
                }
                self.daemon_state = state;
            }
            Update::Connected(connected) => {
                self.connected = connected;
                if connected {
                    self.error = None;
                }
            }
            Update::Snapshot(snapshot) => self.apply_snapshot(*snapshot, cx),
            Update::Event(event) => {
                if event.event_type == "replay.gap" {
                    // The events we missed are gone; only a fresh snapshot fixes it.
                    self.daemon_command(Command::Refresh, cx);
                } else if let Some(snapshot) = self.snapshot.as_mut() {
                    apply_event(snapshot, &event);
                    self.sync_list(cx);
                }
            }
            Update::SessionLoaded(session) => {
                if let Some(snapshot) = self.snapshot.as_mut() {
                    snapshot.sessions.retain(|existing| existing.id != session.id);
                    snapshot.sessions.push(*session);
                    self.sync_list(cx);
                }
            }
            Update::Members { project_id, agent_ids } => {
                self.members.get_or_insert_with(BTreeMap::new).insert(project_id, agent_ids);
            }
            Update::ConversationCreated(session) => {
                let session = *session;
                if session.id.is_empty() {
                    self.error = Some("The daemon did not return the new conversation.".to_owned());
                } else {
                    let id = session.id.clone();
                    if let Some(snapshot) = self.snapshot.as_mut() {
                        snapshot.sessions.retain(|existing| existing.id != id);
                        snapshot.sessions.push(session);
                    }
                    self.select_session(Some(id.clone()), cx);
                    if let Some(text) = self.pending_prompt.take() {
                        self.daemon_command(Command::SendPrompt { session_id: id, text }, cx);
                    }
                }
            }
            Update::Sent => {
                self.error = None;
            }
            Update::Cancelled | Update::PermissionReplied => {
                self.error = None;
            }
            Update::Failed(message) => {
                self.error = Some(message);
                if self.composer.read(cx).is_empty() {
                    if let Some(text) = self.pending_prompt.take() {
                        self.composer.update(cx, |input, cx| input.set_text(text, cx));
                    }
                }
            }
        }
        cx.notify();
    }

    fn apply_snapshot(&mut self, snapshot: Snapshot, cx: &mut Context<Self>) {
        let keep_agent = self.agent_id.clone().filter(|id| snapshot.agent(id).is_some());
        let keep_session = self.session_id.clone().filter(|id| snapshot.session(id).is_some());
        self.agent_id = keep_agent.or_else(|| snapshot.manager().map(|agent| agent.id.clone())).or_else(|| {
            snapshot.agents.iter().find(|agent| agent.is_employee()).map(|agent| agent.id.clone())
        });
        self.session_id = keep_session;
        self.snapshot = Some(snapshot);
        self.sync_list(cx);
        // Who is hired where is not in the snapshot; ask once per launch.
        if self.members.is_none() {
            let project_ids: Vec<String> = self
                .snapshot
                .as_ref()
                .map(|snapshot| snapshot.projects.iter().map(|project| project.id.clone()).collect())
                .unwrap_or_default();
            if !project_ids.is_empty() {
                self.members = Some(BTreeMap::new());
                self.daemon_command(Command::LoadProjectMembers { project_ids }, cx);
            }
        }
    }

    fn select_session(&mut self, session_id: Option<String>, cx: &mut Context<Self>) {
        self.session_id = session_id;
        self.sync_list(cx);
        cx.notify();
    }

    /// Keep the message list in step with the conversation on screen: a changed
    /// conversation resets it (to the bottom), more messages append, so a long
    /// conversation stays smooth and sticks to the bottom while text streams.
    fn sync_list(&mut self, _cx: &mut Context<Self>) {
        let session_id = self.session_id.clone();
        let count = self
            .selected_session()
            .map(crate::ui::conversation::row_count)
            .unwrap_or(0);
        if self.list_session != session_id {
            self.list_session = session_id;
            self.list_count = count;
            self.list.reset(count);
        } else if count != self.list_count {
            if count > self.list_count {
                let old = self.list_count;
                self.list.splice(old..old, count - old);
            } else {
                self.list.reset(count);
            }
            self.list_count = count;
        }
    }

    // ---- actions --------------------------------------------------------------

    pub(crate) fn select_agent(&mut self, agent_id: String, cx: &mut Context<Self>) {
        if self.agent_id.as_deref() != Some(agent_id.as_str()) {
            self.agent_id = Some(agent_id);
            self.session_id = None;
            self.sync_list(cx);
        }
        cx.notify();
    }

    pub(crate) fn open_conversation(&mut self, agent_id: String, session_id: String, cx: &mut Context<Self>) {
        self.agent_id = Some(agent_id);
        self.session_id = Some(session_id.clone());
        self.sync_list(cx);
        // The snapshot only carries the newest messages of an at-rest
        // conversation; a full one comes from `session.get`.
        if self.selected_session().map(|session| session.partial).unwrap_or(false) {
            self.daemon_command(Command::LoadSession { session_id }, cx);
        }
        cx.notify();
    }

    pub(crate) fn toggle_project_fold(&mut self, key: String, cx: &mut Context<Self>) {
        if !self.folded_projects.remove(&key) {
            self.folded_projects.insert(key);
        }
        let _ = settings_file::write_folded_projects(&paths::settings_file_path(), &self.folded_projects);
        cx.notify();
    }

    pub(crate) fn set_employee_fold(&mut self, key: String, open: bool, cx: &mut Context<Self>) {
        self.employee_folds.insert(key, open);
        cx.notify();
    }

    pub(crate) fn set_view(&mut self, view: SidebarView, cx: &mut Context<Self>) {
        self.view = view;
        cx.notify();
    }

    /// Send the composer's text: to the open conversation, or to a new one when
    /// the bot has none yet.
    pub(crate) fn send(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.composer.read(cx).text().trim().to_owned();
        if text.is_empty() {
            return;
        }
        let Some(agent_id) = self.agent_id.clone() else {
            self.error = Some("Choose an employee first.".to_owned());
            return;
        };
        self.error = None;
        match self.session_id.clone() {
            Some(session_id) => self.daemon_command(Command::SendPrompt { session_id, text }, cx),
            None => {
                self.pending_prompt = Some(text);
                self.new_conversation(agent_id, None, cx);
            }
        }
        self.composer.update(cx, |input, cx| input.clear(window, cx));
        self.focus_composer(window, cx);
        cx.notify();
    }

    pub(crate) fn new_conversation(&mut self, agent_id: String, project_id: Option<String>, cx: &mut Context<Self>) {
        self.agent_id = Some(agent_id.clone());
        self.daemon_command(Command::NewConversation { agent_id, project_id }, cx);
    }

    pub(crate) fn stop_turn(&mut self, cx: &mut Context<Self>) {
        if let Some(session_id) = self.session_id.clone() {
            self.daemon_command(Command::CancelTurn { session_id }, cx);
        }
    }

    pub(crate) fn reply_permission(&mut self, permission_id: String, choice: String, cx: &mut Context<Self>) {
        self.daemon_command(Command::ReplyPermission { permission_id, choice }, cx);
    }

    fn focus_composer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let handle = self.composer.read(cx).handle();
        window.focus(&handle, cx);
    }

    // ---- first run ------------------------------------------------------------

    fn answer_first_run(&mut self, copy: bool, cx: &mut Context<Self>) {
        let Some(question) = self.first_run.take() else { return };
        if copy {
            if let Err(error) = copy_database(&question.source, &question.target) {
                self.first_run = Some(FirstRunQuestion { error: Some(error), ..question });
                cx.notify();
                return;
            }
        }
        let (handle, updates) = start(question.target);
        self.daemon = Some(handle);
        cx.spawn(async move |this, cx: &mut AsyncApp| {
            while let Ok(update) = updates.recv().await {
                let mut slot = Some(update);
                if this.upgrade().is_none() {
                    break;
                }
                cx.update(|app| {
                    let Some(root) = this.upgrade() else { return };
                    if let Some(update) = slot.take() {
                        root.update(app, |root, cx| root.on_update(update, cx));
                    }
                });
            }
        })
        .detach();
        cx.notify();
    }
}

/// Copy the installed Bitbots database with SQLite's online backup API, which is
/// safe while Bitbots is running. The database file only: never secrets, tokens
/// or the credential store.
pub fn copy_database(source: &std::path::Path, target: &std::path::Path) -> Result<(), String> {
    if target.exists() {
        return Ok(());
    }
    if let Some(folder) = target.parent() {
        std::fs::create_dir_all(folder).map_err(|error| format!("Could not prepare the app's folder: {error}"))?;
    }
    crate::trace::log(format!("copying the Bitbots database from {}", source.display()));
    let from = rusqlite::Connection::open(source)
        .map_err(|error| format!("Could not open the Bitbots database: {error}"))?;
    let mut to = rusqlite::Connection::open(target)
        .map_err(|error| format!("Could not create the preview database: {error}"))?;
    {
        let backup = rusqlite::backup::Backup::new(&from, &mut to)
            .map_err(|error| format!("Could not start the database copy: {error}"))?;
        backup
            .run_to_completion(100, std::time::Duration::from_millis(5), None)
            .map_err(|error| format!("Could not finish the database copy: {error}"))?;
    }
    let bytes = std::fs::metadata(target).map(|meta| meta.len()).unwrap_or(0);
    crate::trace::log(format!("database copied: {bytes} bytes"));
    Ok(())
}

impl Render for Root {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = haze_dark();
        let first_run_error = self.first_run.as_ref().and_then(|question| question.error.clone());
        if self.first_run.is_some() {
            return div()
                .size_full()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .bg(colors.backdrop_fallback)
                .font_family(theme::UI_FONT)
                .text_color(colors.text_1)
                .child(self.render_first_run(first_run_error, colors, cx))
                .into_any_element();
        }

        let missing = match &self.daemon_state {
            DaemonState::Missing(message) => Some(message.clone()),
            _ => None,
        };
        if let Some(message) = missing {
            if self.snapshot.is_none() {
                return div()
                    .size_full()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .bg(colors.backdrop_fallback)
                    .font_family(theme::UI_FONT)
                    .text_color(colors.text_1)
                    .child(self.render_problem(&message, colors))
                    .into_any_element();
            }
        }

        let sidebar = self.render_sidebar(colors, cx);
        let conversations = self.render_conversations(colors, cx);
        let pane = self.render_conversation_pane(colors, cx);

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(colors.backdrop_fallback)
            .font_family(theme::UI_FONT)
            .text_color(colors.text_1)
            .text_size(px(theme::FS_SM))
            .child(
                div()
                    .flex_1()
                    .w_full()
                    .flex()
                    .overflow_hidden()
                    .child(sidebar)
                    .child(conversations)
                    .child(pane),
            )
            .when_some(self.error.clone(), |this, message| this.child(self.render_error_bar(&message, colors, cx)))
            .when_some(self.notice.clone(), |this, message| this.child(self.render_notice_bar(&message, colors, cx)))
            .into_any_element()
    }
}

impl Root {
    fn render_first_run(&mut self, error: Option<String>, colors: Colors, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(px(460.0))
            .p(px(20.0))
            .flex()
            .flex_col()
            .gap(px(10.0))
            .rounded(px(theme::RADIUS_LG))
            .bg(colors.panel)
            .border_1()
            .border_color(colors.line)
            .child(div().text_size(px(theme::FS_XL)).font_weight(gpui::FontWeight::SEMIBOLD).child("Start Bitbots (Rust preview)"))
            .child(
                div()
                    .text_color(colors.text_2)
                    .child("The installed Bitbots has data on this computer. This preview keeps its own database, so the two never share one."),
            )
            .when_some(error, |this, error| this.child(div().text_color(colors.status_danger).child(error)))
            .child(self.button("Start with a copy of my Bitbots data", colors, true, cx))
            .child(self.button("Start empty", colors, false, cx))
            .child(div().text_color(colors.text_3).text_size(px(theme::FS_XS)).child("Only the database file is copied, never tokens or credentials."))
    }

    fn render_problem(&mut self, message: &str, colors: Colors) -> impl IntoElement {
        div()
            .w(px(520.0))
            .p(px(20.0))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .rounded(px(theme::RADIUS_LG))
            .bg(colors.panel)
            .border_1()
            .border_color(colors.line)
            .child(div().text_size(px(theme::FS_XL)).font_weight(gpui::FontWeight::SEMIBOLD).child("The daemon did not start"))
            .child(div().text_color(colors.text_2).child(SharedString::from(message.to_owned())))
    }

    fn render_error_bar(&mut self, message: &str, colors: Colors, cx: &mut Context<Self>) -> impl IntoElement {
        let close = cx.listener(|root, _: &gpui::ClickEvent, _window, cx| {
            root.error = None;
            cx.notify();
        });
        div()
            .w_full()
            .px(px(12.0))
            .py(px(7.0))
            .flex()
            .items_center()
            .justify_between()
            .gap(px(10.0))
            .bg(colors.raised)
            .border_t_1()
            .border_color(colors.line)
            .child(div().text_color(colors.status_danger).text_size(px(theme::FS_XS)).truncate().child(SharedString::from(message.to_owned())))
            .child(
                div()
                    .id("dismiss-error")
                    .text_color(colors.text_2)
                    .text_size(px(theme::FS_XS))
                    .cursor_pointer()
                    .child("Dismiss")
                    .on_click(close),
            )
    }

    fn render_notice_bar(&mut self, message: &str, colors: Colors, cx: &mut Context<Self>) -> impl IntoElement {
        let close = cx.listener(|root, _: &gpui::ClickEvent, _window, cx| {
            root.notice = None;
            cx.notify();
        });
        div()
            .w_full()
            .px(px(12.0))
            .py(px(7.0))
            .flex()
            .items_center()
            .justify_between()
            .gap(px(10.0))
            .bg(colors.raised)
            .border_t_1()
            .border_color(colors.line)
            .child(div().text_color(colors.text_2).text_size(px(theme::FS_XS)).truncate().child(SharedString::from(message.to_owned())))
            .child(
                div()
                    .id("dismiss-notice")
                    .text_color(colors.text_3)
                    .text_size(px(theme::FS_XS))
                    .cursor_pointer()
                    .child("Dismiss")
                    .on_click(close),
            )
    }

    pub(crate) fn button(&self, label: &str, colors: Colors, primary: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let (background, foreground) = if primary {
            (colors.accent, colors.base)
        } else {
            (colors.raised, colors.text_1)
        };
        let copy = primary;
        div()
            .id(SharedString::from(format!("first-run-{label}")))
            .h(px(30.0))
            .px(px(12.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(theme::RADIUS_SM))
            .bg(background)
            .text_color(foreground)
            .text_size(px(theme::FS_XS))
            .font_weight(gpui::FontWeight::MEDIUM)
            .cursor_pointer()
            .hover(move |style| style.bg(if copy { colors.selected_strong } else { colors.selected }))
            .child(SharedString::from(label.to_owned()))
            .on_click(cx.listener(move |root, _: &gpui::ClickEvent, _window, cx| root.answer_first_run(copy, cx)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_copy_is_skipped_when_the_target_already_exists() {
        let folder = std::env::temp_dir().join(format!("bitbots-native-copy-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let target = folder.join("bloblex.db");
        std::fs::write(&target, b"already here").unwrap();
        // The source does not even exist: the target wins and nothing is touched.
        let source = folder.join("missing.db");
        assert_eq!(copy_database(&source, &target), Ok(()));
        assert_eq!(std::fs::read(&target).unwrap(), b"already here");
        let _ = std::fs::remove_dir_all(&folder);
    }
}
