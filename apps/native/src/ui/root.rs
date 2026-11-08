//! The window: one main view that owns the daemon connection, the selection, the
//! three columns (sidebar, conversations, main area) and the dialogs.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::time::Instant;

use gpui::{
    div, prelude::*, px, App, AsyncApp, Context, Entity, ListAlignment, ListState, SharedString,
    Subscription, Window, WindowAppearance,
};

use crate::daemon::{start, Command, DaemonHandle, DaemonState, Update};
use crate::paths;
use crate::settings_file;
use crate::state::{apply_event, Agent, Session, Snapshot};
use crate::state::projects::MembersByProject;
use crate::theme::{self, Colors, Density, DensityTokens, ThemeChoice};
use crate::ui::motion;
use crate::ui::text_input::TextInput;

/// The sidebar's own list: the Projects tree or the roster.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SidebarView {
    Projects,
    Team,
}

/// What the main area shows: the conversation, the Team page or Settings.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Conversation,
    Team,
    Settings,
}

/// The Team page's two tabs.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TeamTab {
    Team,
    Hire,
}

impl TeamTab {
    pub fn label(self) -> &'static str {
        match self {
            TeamTab::Team => "Your team",
            TeamTab::Hire => "Hire",
        }
    }
}

/// The first-run question: the installed Bitbots has a database, this app does
/// not. Asked once, in the window, before any daemon starts.
pub struct FirstRunQuestion {
    pub source: PathBuf,
    pub target: PathBuf,
    pub error: Option<String>,
}

/// A project's "+" menu, shown under its header.
#[derive(Clone)]
pub struct ProjectMenu {
    pub key: String,
    pub project_id: Option<String>,
    pub project_name: String,
}

/// The new-project dialog: a name and a folder (`project.create`).
pub struct NewProjectDialog {
    pub folder: Option<String>,
    pub error: Option<String>,
    pub busy: bool,
}

/// The hire dialog: a preset or an employee, the projects to hire into, and the
/// coding agent and model the new employee starts on.
pub struct HireDialog {
    pub preset_id: Option<String>,
    pub agent_id: Option<String>,
    pub name: String,
    pub role: String,
    pub portrait: String,
    pub color: String,
    pub projects: BTreeSet<String>,
    pub runtime_id: String,
    pub model: Option<String>,
    pub busy: bool,
    pub error: Option<String>,
}

/// One coding-agent model in the hire dialog's list.
#[derive(Clone, Debug, Default)]
pub struct ModelChoice {
    pub id: String,
    pub label: String,
}

/// A confirmation dialog: one sentence, a primary action, a cancel.
pub struct Confirm {
    pub title: String,
    pub body: String,
    pub confirm_label: String,
    pub action: ConfirmAction,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ConfirmAction {
    CopyDataAgain,
}

/// After a new project: how to staff it (`ui/StaffProjectDialog.tsx`).
pub struct StaffDialog {
    pub project_id: String,
    pub project_name: String,
}

pub struct Root {
    pub(crate) db_path: PathBuf,
    pub(crate) daemon: Option<DaemonHandle>,
    pub(crate) snapshot: Option<Snapshot>,
    pub(crate) daemon_state: DaemonState,
    pub(crate) connected: bool,
    pub(crate) error: Option<String>,
    pub(crate) notice: Option<String>,
    pub(crate) daemon_version: Option<String>,
    pub(crate) agent_id: Option<String>,
    pub(crate) session_id: Option<String>,
    pub(crate) view: SidebarView,
    pub(crate) page: Page,
    pub(crate) team_tab: TeamTab,
    pub(crate) team_project: Option<String>,
    pub(crate) folded_projects: BTreeSet<String>,
    /// An employee's own fold choice, once the owner has made one.
    pub(crate) employee_folds: BTreeMap<String, bool>,
    /// Runs of tool steps the owner opened in the conversation.
    pub(crate) expanded_steps: BTreeSet<String>,
    /// Rows that are closing: the key and when the 180 ms finishes.
    pub(crate) closing: BTreeMap<String, Instant>,
    pub(crate) menu: Option<ProjectMenu>,
    pub(crate) hovered: Option<String>,
    pub(crate) members: Option<MembersByProject>,
    /// Whether this window already asked the daemon to scan for coding agents.
    pub(crate) runtimes_asked: bool,
    /// Whether the live check has already opened its named conversation.
    pub(crate) opened_env_session: bool,
    pub(crate) system_is_light: bool,
    pub(crate) theme_choice: ThemeChoice,
    pub(crate) density: Density,
    pub(crate) appearance_subscription: Option<Subscription>,
    pub(crate) composer: Entity<TextInput>,
    pub(crate) team_search: Entity<TextInput>,
    pub(crate) project_name: Entity<TextInput>,
    pub(crate) models: BTreeMap<String, Vec<ModelChoice>>,
    pub(crate) models_requested: BTreeSet<String>,
    pub(crate) list: ListState,
    pub(crate) list_session: Option<String>,
    pub(crate) list_count: usize,
    pub(crate) pending_prompt: Option<String>,
    pub(crate) first_run: Option<FirstRunQuestion>,
    pub(crate) new_project: Option<NewProjectDialog>,
    pub(crate) staff: Option<StaffDialog>,
    pub(crate) hire: Option<HireDialog>,
    pub(crate) confirm: Option<Confirm>,
    pub(crate) data_busy: bool,
}

impl Root {
    pub fn new(db_path: PathBuf, system_is_light: bool, cx: &mut Context<Self>) -> Self {
        let settings_path = paths::settings_file_path();
        let settings = settings_file::read(&settings_path);
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
            let (handle, updates) = start(db_path.clone());
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
        let team_search = cx.new(|cx| TextInput::new(crate::ui::team::SEARCH_PLACEHOLDER, px(18.0), px(theme::FS_SM), cx));
        let project_name = cx.new(|cx| TextInput::new("Project name", px(20.0), px(theme::FS_MD), cx));

        if let Some(updates) = updates {
            Self::follow(updates, &this, cx);
        }
        if let Some(daemon) = daemon.as_ref() {
            daemon.send(Command::LoadHealth);
        }

        Self {
            db_path,
            daemon,
            snapshot: None,
            daemon_state: DaemonState::Starting,
            connected: false,
            error: None,
            notice: None,
            daemon_version: None,
            agent_id: None,
            session_id: None,
            view: SidebarView::Projects,
            page: Page::Conversation,
            team_tab: TeamTab::Team,
            team_project: None,
            folded_projects: settings.folded_projects.into_iter().collect(),
            employee_folds: BTreeMap::new(),
            expanded_steps: BTreeSet::new(),
            closing: BTreeMap::new(),
            menu: None,
            hovered: None,
            members: None,
            runtimes_asked: false,
            opened_env_session: false,
            system_is_light,
            theme_choice: settings.theme,
            density: settings.density,
            appearance_subscription: None,
            composer,
            team_search,
            project_name,
            models: BTreeMap::new(),
            models_requested: BTreeSet::new(),
            list: ListState::new(0, ListAlignment::Bottom, px(600.0)),
            list_session: None,
            list_count: 0,
            pending_prompt: None,
            first_run,
            new_project: None,
            staff: None,
            hire: None,
            confirm: None,
            data_busy: false,
        }
    }

    fn follow(
        updates: async_channel::Receiver<Update>,
        this: &gpui::WeakEntity<Self>,
        cx: &mut Context<Self>,
    ) {
        let this = this.clone();
        cx.spawn(async move |_, cx: &mut AsyncApp| {
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

    /// The composer's focus handle, so the window can put the caret in it on open.
    pub fn composer_handle(&self, cx: &App) -> gpui::FocusHandle {
        self.composer.read(cx).handle()
    }

    /// Hold the appearance observer: Windows' light/dark switch follows the
    /// theme while "System" is chosen.
    pub fn watch_appearance(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.appearance_subscription.is_some() {
            return;
        }
        let this = cx.weak_entity();
        let subscription = window.observe_window_appearance(move |window, cx| {
            let light = window.appearance() != WindowAppearance::Dark
                && window.appearance() != WindowAppearance::VibrantDark;
            if let Some(root) = this.upgrade() {
                root.update(cx, |root, cx| root.on_appearance_changed(light, cx));
            }
        });
        self.appearance_subscription = Some(subscription);
        motion::refresh_animations();
    }

    pub(crate) fn on_appearance_changed(&mut self, system_is_light: bool, cx: &mut Context<Self>) {
        // GPUI tells us about the window's appearance, which also changes when
        // the owner flips Windows' "Show animations".
        motion::refresh_animations();
        if self.system_is_light != system_is_light {
            self.system_is_light = system_is_light;
            cx.notify();
        }
    }

    // ---- data the panels read -------------------------------------------------

    pub(crate) fn colors(&self) -> Colors {
        theme::colors(self.theme_choice, self.system_is_light)
    }

    pub(crate) fn tokens(&self) -> DensityTokens {
        self.density.tokens()
    }

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
        let mut employees: Vec<&Agent> = self.agents().iter().filter(|agent| agent.is_employee()).collect();
        employees.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        employees
    }

    /// A bot's provider, through the runtime it runs on.
    pub(crate) fn provider_of(&self, agent: &Agent) -> Option<String> {
        self.snapshot().and_then(|snapshot| snapshot.provider_of(agent))
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

    /// The projects a bot is hired into, from `project.members`.
    pub(crate) fn projects_for(&self, agent_id: &str) -> Vec<String> {
        let Some(members) = self.members.as_ref() else { return Vec::new() };
        members
            .iter()
            .filter(|(_, agents)| agents.iter().any(|id| id == agent_id))
            .map(|(project_id, _)| project_id.clone())
            .collect()
    }

    // ---- talking to the daemon ------------------------------------------------

    pub(crate) fn daemon_command(&mut self, command: Command, _cx: &mut Context<Self>) {
        if let Some(daemon) = self.daemon.as_ref() {
            daemon.send(command);
        } else {
            self.error = Some("The local Bitbots daemon is not running.".to_owned());
        }
    }

    fn reload_members(&mut self, project_ids: Vec<String>, cx: &mut Context<Self>) {
        if project_ids.is_empty() {
            return;
        }
        self.members.get_or_insert_with(BTreeMap::new);
        self.daemon_command(Command::LoadProjectMembers { project_ids }, cx);
    }

    fn on_update(&mut self, update: Update, cx: &mut Context<Self>) {
        match update {
            Update::Daemon(state) => {
                if let DaemonState::Missing(message) | DaemonState::Stopped(message) = &state {
                    self.error = Some(message.clone());
                }
                if matches!(state, DaemonState::Running) {
                    self.daemon_command(Command::LoadHealth, cx);
                }
                self.daemon_state = state;
            }
            Update::DaemonVersion(version) => self.daemon_version = Some(version),
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
                    let dirty = std::mem::take(&mut snapshot.dirty_members);
                    let refresh = std::mem::take(&mut snapshot.refresh_wanted);
                    self.sync_list(cx);
                    if !dirty.is_empty() {
                        self.reload_members(dirty, cx);
                    }
                    if refresh {
                        self.refresh_members_and_snapshot(cx);
                    }
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
                crate::trace::log(format!("project.members: {} employees", agent_ids.len()));
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
                    self.page = Page::Conversation;
                    self.select_session(Some(id.clone()), cx);
                    if let Some(text) = self.pending_prompt.take() {
                        self.daemon_command(Command::SendPrompt { session_id: id, text }, cx);
                    }
                }
            }
            Update::ProjectCreated(project) => {
                let project = *project;
                if project.id.is_empty() {
                    self.error = Some("The daemon did not return the new project.".to_owned());
                    if let Some(dialog) = self.new_project.as_mut() {
                        dialog.busy = false;
                        dialog.error = Some("The project was not created.".to_owned());
                    }
                } else {
                    if let Some(snapshot) = self.snapshot.as_mut() {
                        snapshot.projects.retain(|existing| existing.id != project.id);
                        snapshot.projects.push(project.clone());
                    }
                    self.new_project = None;
                    self.project_name.update(cx, |input, cx| input.set_text(String::new(), cx));
                    // After creating: who should work on it? The CEO, by hand, or not now.
                    self.staff = Some(StaffDialog { project_id: project.id.clone(), project_name: project.name.clone() });
                    self.notice = Some(format!("Created project {}.", project.name));
                    self.daemon_command(Command::Refresh, cx);
                }
            }
            Update::Hired { agent_id, created } => {
                if let Some(dialog) = self.hire.as_mut() {
                    dialog.busy = false;
                }
                self.hire = None;
                self.error = None;
                if !agent_id.is_empty() {
                    if self.agent_id.is_none() {
                        self.agent_id = Some(agent_id);
                    }
                    self.notice = Some(if created {
                        "Hired. Their conversations start with Ask before acting.".to_owned()
                    } else {
                        "Added to the project. Their conversations start with Ask before acting.".to_owned()
                    });
                }
                self.refresh_members_and_snapshot(cx);
            }
            Update::Released { released } => {
                self.error = None;
                if released {
                    self.notice = Some("Released from that project. Conversations stay where they are.".to_owned());
                }
                self.refresh_members_and_snapshot(cx);
            }
            Update::ProposalDecided => {
                self.error = None;
                self.refresh_members_and_snapshot(cx);
            }
            Update::Models { runtime_id, models } => {
                self.models.insert(runtime_id, models);
            }
            Update::Sent => {
                self.error = None;
            }
            Update::Cancelled | Update::PermissionReplied => {
                self.error = None;
            }
            Update::Failed(message) => {
                self.error = Some(message.clone());
                if let Some(dialog) = self.hire.as_mut() {
                    dialog.busy = false;
                    dialog.error = Some(message.clone());
                }
                if let Some(dialog) = self.new_project.as_mut() {
                    dialog.busy = false;
                    dialog.error = Some(message.clone());
                }
                if self.composer.read(cx).is_empty() {
                    if let Some(text) = self.pending_prompt.take() {
                        self.composer.update(cx, |input, cx| input.set_text(text, cx));
                    }
                }
            }
        }
        cx.notify();
    }

    /// Who is hired where lives outside the snapshot, and a hire can create an
    /// agent, so both are reloaded together.
    fn refresh_members_and_snapshot(&mut self, cx: &mut Context<Self>) {
        self.daemon_command(Command::Refresh, cx);
        let project_ids: Vec<String> = self
            .snapshot()
            .map(|snapshot| snapshot.projects.iter().map(|project| project.id.clone()).collect())
            .unwrap_or_default();
        self.reload_members(project_ids, cx);
    }

    fn apply_snapshot(&mut self, snapshot: Snapshot, cx: &mut Context<Self>) {
        let keep_agent = self.agent_id.clone().filter(|id| snapshot.agent(id).is_some());
        let keep_session = self.session_id.clone().filter(|id| snapshot.session(id).is_some());
        self.agent_id = keep_agent.or_else(|| snapshot.manager().map(|agent| agent.id.clone())).or_else(|| {
            snapshot.agents.iter().find(|agent| agent.is_employee()).map(|agent| agent.id.clone())
        });
        self.session_id = keep_session;
        // The live check has nobody at the keyboard to pick a conversation, so it
        // names one up front. It takes the same path a click does.
        if !self.opened_env_session {
            self.opened_env_session = true;
            if let Some(id) = std::env::var("BITBOTS_NATIVE_OPEN_SESSION").ok().filter(|id| !id.is_empty()) {
                if let Some(session) = snapshot.session(&id) {
                    crate::trace::log(format!("live check: opening conversation {id}"));
                    self.session_id = Some(id.clone());
                    if let Some(agent_id) = session.agent_id.clone() {
                        self.agent_id = Some(agent_id);
                    }
                }
            }
        }
        let project_ids: Vec<String> = snapshot.projects.iter().map(|project| project.id.clone()).collect();
        let no_runtimes = snapshot.runtimes.is_empty();
        self.snapshot = Some(snapshot);
        self.sync_list(cx);
        // A fresh install has no coding agents in the table yet; ask for one scan
        // so every bot can show its agent's logo.
        if no_runtimes && !self.runtimes_asked {
            self.runtimes_asked = true;
            self.daemon_command(Command::ScanRuntimes, cx);
        }
        // Who is hired where is not in the snapshot; ask once per launch and
        // again whenever a project appears.
        self.members.get_or_insert_with(BTreeMap::new);
        self.reload_members(project_ids, cx);
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
            // Opening or switching a conversation lands on its last line, not
            // wherever the previous one was left.
            self.scroll_to_end();
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

    /// Put the message list at its last row.
    fn scroll_to_end(&self) {
        self.list.scroll_to(gpui::ListOffset { item_ix: self.list_count, offset_in_item: px(0.0) });
    }

    // ---- navigation -----------------------------------------------------------

    pub(crate) fn select_agent(&mut self, agent_id: String, cx: &mut Context<Self>) {
        self.menu = None;
        if self.agent_id.as_deref() != Some(agent_id.as_str()) {
            self.agent_id = Some(agent_id);
            self.session_id = None;
            self.sync_list(cx);
        }
        self.page = Page::Conversation;
        cx.notify();
    }

    pub(crate) fn open_conversation(&mut self, agent_id: String, session_id: String, cx: &mut Context<Self>) {
        self.agent_id = Some(agent_id);
        self.session_id = Some(session_id.clone());
        self.page = Page::Conversation;
        self.sync_list(cx);
        // The snapshot only carries the newest messages of an at-rest
        // conversation; a full one comes from `session.get`.
        if self.selected_session().map(|session| session.partial).unwrap_or(false) {
            self.daemon_command(Command::LoadSession { session_id }, cx);
        }
        cx.notify();
    }

    pub(crate) fn set_view(&mut self, view: SidebarView, cx: &mut Context<Self>) {
        self.view = view;
        cx.notify();
    }

    /// "Open team page": the Team page in the main area, on the tab asked for.
    pub(crate) fn open_team(&mut self, tab: TeamTab, project_id: Option<String>, cx: &mut Context<Self>) {
        self.page = Page::Team;
        self.team_tab = tab;
        self.team_project = project_id;
        self.menu = None;
        cx.notify();
    }

    pub(crate) fn open_settings(&mut self, cx: &mut Context<Self>) {
        self.page = Page::Settings;
        self.menu = None;
        cx.notify();
    }

    pub(crate) fn back_to_conversation(&mut self, cx: &mut Context<Self>) {
        self.page = Page::Conversation;
        if self.session_id.is_none() {
            self.sync_list(cx);
        }
        cx.notify();
    }

    pub(crate) fn set_theme(&mut self, choice: ThemeChoice, cx: &mut Context<Self>) {
        self.theme_choice = choice;
        self.save_appearance();
        cx.notify();
    }

    pub(crate) fn set_density(&mut self, density: Density, cx: &mut Context<Self>) {
        self.density = density;
        self.save_appearance();
        cx.notify();
    }

    fn save_appearance(&self) {
        let _ = settings_file::write_appearance(&paths::settings_file_path(), self.theme_choice, self.density);
    }

    // ---- the tree -------------------------------------------------------------

    pub(crate) fn toggle_project_fold(&mut self, key: String, cx: &mut Context<Self>) {
        if !self.folded_projects.remove(&key) {
            self.folded_projects.insert(key.clone());
            self.start_closing(key, cx);
        }
        let _ = settings_file::write_folded_projects(&paths::settings_file_path(), &self.folded_projects);
        cx.notify();
    }

    /// Open or close one run of tool steps in the conversation.
    pub(crate) fn toggle_steps(&mut self, key: String, cx: &mut Context<Self>) {
        if !self.expanded_steps.remove(&key) {
            self.expanded_steps.insert(key);
        }
        cx.notify();
    }

    pub(crate) fn set_employee_fold(&mut self, key: String, open: bool, cx: &mut Context<Self>) {        self.employee_folds.insert(key.clone(), open);
        if !open {
            self.start_closing(key, cx);
        }
        cx.notify();
    }

    /// Remember that a row is closing, so its height can animate out, and ask for
    /// one repaint when the 180 ms are over.
    fn start_closing(&mut self, key: String, cx: &mut Context<Self>) {
        if !motion::animations_enabled() {
            return;
        }
        let until = Instant::now() + std::time::Duration::from_millis(motion::COLLAPSE_MS + 40);
        self.closing.insert(key, until);
        cx.spawn(async move |this, cx: &mut AsyncApp| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(motion::COLLAPSE_MS + 60))
                .await;
            cx.update(|app| {
                if let Some(root) = this.upgrade() {
                    root.update(app, |root, cx| {
                        let now = Instant::now();
                        root.closing.retain(|_, until| *until > now);
                        cx.notify();
                    });
                }
            });
        })
        .detach();
    }

    /// Whether a row is still animating closed.
    pub(crate) fn is_closing(&self, key: &str) -> bool {
        self.closing.get(key).map(|until| *until > Instant::now()).unwrap_or(false)
    }

    pub(crate) fn toggle_menu(&mut self, menu: ProjectMenu, cx: &mut Context<Self>) {
        self.menu = match self.menu.as_ref().map(|open| open.key.clone()) {
            Some(key) if key == menu.key => None,
            _ => Some(menu),
        };
        cx.notify();
    }

    pub(crate) fn set_hovered(&mut self, key: Option<String>, cx: &mut Context<Self>) {
        if self.hovered != key {
            self.hovered = key;
            cx.notify();
        }
    }

    // ---- actions --------------------------------------------------------------

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
        self.page = Page::Conversation;
        self.menu = None;
        self.daemon_command(Command::NewConversation { agent_id, project_id }, cx);
    }

    /// The CEO's kickoff from `askCeoToStaff`: a new conversation in the project
    /// that asks it to interview the owner and propose who to hire.
    pub(crate) fn ask_ceo_to_staff(&mut self, project_id: String, cx: &mut Context<Self>) {
        let Some(ceo) = self.snapshot().and_then(|snapshot| snapshot.manager()).map(|agent| agent.id.clone()) else {
            self.error = Some("Set up your CEO first.".to_owned());
            cx.notify();
            return;
        };
        let name = self
            .snapshot()
            .and_then(|snapshot| snapshot.project_name(Some(&project_id)))
            .unwrap_or("this project")
            .to_owned();
        self.staff = None;
        self.pending_prompt = Some(format!(
            "Help me staff {name}. Interview me briefly about the project, then propose who to hire."
        ));
        self.new_conversation(ceo, Some(project_id), cx);
    }

    pub(crate) fn stop_turn(&mut self, cx: &mut Context<Self>) {
        if let Some(session_id) = self.session_id.clone() {
            self.daemon_command(Command::CancelTurn { session_id }, cx);
        }
    }

    pub(crate) fn reply_permission(&mut self, permission_id: String, choice: String, cx: &mut Context<Self>) {
        self.daemon_command(Command::ReplyPermission { permission_id, choice }, cx);
    }

    pub(crate) fn decide_proposal(&mut self, id: String, approve: bool, cx: &mut Context<Self>) {
        let decision = if approve { "approve" } else { "reject" }.to_owned();
        self.daemon_command(Command::DecideProposal { id, decision }, cx);
    }

    /// "Hire all N": every waiting proposal of the same group, this one first.
    pub(crate) fn decide_proposals(&mut self, ids: Vec<String>, cx: &mut Context<Self>) {
        for id in ids {
            self.daemon_command(Command::DecideProposal { id, decision: "approve".to_owned() }, cx);
        }
    }

    fn focus_composer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let handle = self.composer.read(cx).handle();
        window.focus(&handle, cx);
    }

    // ---- new project ----------------------------------------------------------

    pub(crate) fn open_new_project(&mut self, cx: &mut Context<Self>) {
        self.menu = None;
        self.new_project = Some(NewProjectDialog { folder: None, error: None, busy: false });
        cx.notify();
    }

    /// The Windows folder picker, through GPUI's own dialog.
    pub(crate) fn choose_folder(&mut self, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(SharedString::from("Use this folder")),
        });
        cx.spawn(async move |this, cx: &mut AsyncApp| {
            let chosen = receiver.await.ok().and_then(|result| result.ok()).flatten().and_then(|paths| paths.into_iter().next());
            cx.update(|app| {
                if let Some(root) = this.upgrade() {
                    root.update(app, |root, cx| {
                        let Some(chosen) = chosen.clone() else { return };
                        let folder = chosen.to_string_lossy().to_string();
                        if let Some(dialog) = root.new_project.as_mut() {
                            // The folder names the project, as the app's own
                            // "create from folder" does; the field can change it.
                            if root.project_name.read(cx).is_empty() {
                                let name = chosen
                                    .file_name()
                                    .map(|name| name.to_string_lossy().to_string())
                                    .unwrap_or_else(|| "Project".to_owned());
                                root.project_name.update(cx, |input, cx| input.set_text(name, cx));
                            }
                            dialog.folder = Some(folder);
                        }
                        cx.notify();
                    });
                }
            });
        })
        .detach();
    }

    pub(crate) fn create_project(&mut self, cx: &mut Context<Self>) {
        let name = self.project_name.read(cx).text().trim().to_owned();
        let Some(dialog) = self.new_project.as_mut() else { return };
        if name.is_empty() {
            dialog.error = Some("Give the project a name.".to_owned());
            cx.notify();
            return;
        }
        dialog.busy = true;
        dialog.error = None;
        let path = dialog.folder.clone();
        self.daemon_command(Command::CreateProject { name, path }, cx);
        cx.notify();
    }

    // ---- hiring ---------------------------------------------------------------

    pub(crate) fn open_hire(&mut self, preset: Option<String>, agent: Option<String>, project_id: Option<String>, cx: &mut Context<Self>) {
        let mut dialog = HireDialog {
            preset_id: preset.clone(),
            agent_id: agent.clone(),
            name: "this employee".to_owned(),
            role: String::new(),
            portrait: String::new(),
            color: "violet".to_owned(),
            projects: BTreeSet::new(),
            runtime_id: String::new(),
            model: None,
            busy: false,
            error: None,
        };
        if let Some(preset_id) = preset.as_deref() {
            if let Some(found) = crate::state::team::presets().iter().find(|item| item.id == preset_id) {
                dialog.name = found.nickname.clone();
                dialog.role = found.role.clone();
                dialog.portrait = found.portrait.clone();
                dialog.color = found.color.clone();
            }
        }
        if let Some(agent_id) = agent.as_deref() {
            if let Some(found) = self.snapshot().and_then(|snapshot| snapshot.agent(agent_id)) {
                dialog.name = found.name.clone();
                dialog.role = found.role_or_default().to_owned();
                dialog.portrait = found.icon().unwrap_or("logo").to_owned();
                dialog.color = found.color.clone();
            }
        }
        let usable: Vec<String> = self
            .snapshot()
            .map(|snapshot| snapshot.usable_runtimes().into_iter().map(|runtime| runtime.id.clone()).collect())
            .unwrap_or_default();
        dialog.runtime_id = usable.first().cloned().unwrap_or_default();
        if let Some(project_id) = project_id {
            dialog.projects.insert(project_id);
        }
        let runtime_id = dialog.runtime_id.clone();
        if !runtime_id.is_empty() {
            self.request_models(runtime_id, cx);
        }
        self.hire = Some(dialog);
        cx.notify();
    }

    pub(crate) fn request_models(&mut self, runtime_id: String, cx: &mut Context<Self>) {
        if runtime_id.is_empty() {
            return;
        }
        if self.models.contains_key(&runtime_id) || !self.models_requested.insert(runtime_id.clone()) {
            return;
        }
        self.daemon_command(Command::LoadModels { runtime_id }, cx);
    }

    pub(crate) fn hire_toggle_project(&mut self, project_id: String, cx: &mut Context<Self>) {
        if let Some(dialog) = self.hire.as_mut() {
            if !dialog.projects.remove(&project_id) {
                dialog.projects.insert(project_id);
            }
            cx.notify();
        }
    }

    pub(crate) fn hire_set_runtime(&mut self, runtime_id: String, cx: &mut Context<Self>) {
        if let Some(dialog) = self.hire.as_mut() {
            dialog.runtime_id = runtime_id.clone();
            dialog.model = None;
            self.request_models(runtime_id, cx);
            cx.notify();
        }
    }

    pub(crate) fn hire_set_model(&mut self, model: Option<String>, cx: &mut Context<Self>) {
        if let Some(dialog) = self.hire.as_mut() {
            dialog.model = model;
            cx.notify();
        }
    }

    pub(crate) fn hire_confirm(&mut self, cx: &mut Context<Self>) {
        let Some(dialog) = self.hire.as_mut() else { return };
        if dialog.projects.is_empty() {
            dialog.error = Some("Choose at least one project.".to_owned());
            cx.notify();
            return;
        }
        dialog.busy = true;
        dialog.error = None;
        let projects: Vec<String> = dialog.projects.iter().cloned().collect();
        let first = projects[0].clone();
        let preset_id = dialog.preset_id.clone();
        let agent_id = dialog.agent_id.clone();
        let runtime_id = Some(dialog.runtime_id.clone()).filter(|id| !id.is_empty());
        let model = dialog.model.clone();
        // One call per project; the daemon reuses the same employee for a preset
        // hired twice, so the second call adds a membership rather than a twin.
        if projects.len() == 1 {
            self.daemon_command(
                Command::Hire { project_id: first, agent_id, preset_id, runtime_id, model },
                cx,
            );
        } else {
            // The first call may create the employee; the rest reuse its preset.
            self.daemon_command(
                Command::Hire { project_id: first, agent_id: agent_id.clone(), preset_id: preset_id.clone(), runtime_id: runtime_id.clone(), model },
                cx,
            );
            for project_id in projects.into_iter().skip(1) {
                self.daemon_command(
                    Command::Hire { project_id, agent_id: agent_id.clone(), preset_id: preset_id.clone(), runtime_id: None, model: None },
                    cx,
                );
            }
        }
        cx.notify();
    }

    pub(crate) fn close_hire(&mut self, cx: &mut Context<Self>) {
        self.hire = None;
        cx.notify();
    }

    pub(crate) fn release(&mut self, project_id: String, agent_id: String, cx: &mut Context<Self>) {
        self.daemon_command(Command::Release { project_id, agent_id }, cx);
    }

    // ---- Settings: the data section -------------------------------------------

    pub(crate) fn ask_copy_data_again(&mut self, cx: &mut Context<Self>) {
        self.confirm = Some(Confirm {
            title: "Copy my Bitbots data again".to_owned(),
            body: "This stops the daemon, backs up the preview's own database beside itself, then copies the installed Bitbots database and restarts. Only the database file is copied, never tokens or credentials.".to_owned(),
            confirm_label: "Copy again".to_owned(),
            action: ConfirmAction::CopyDataAgain,
        });
        cx.notify();
    }

    pub(crate) fn close_confirm(&mut self, cx: &mut Context<Self>) {
        self.confirm = None;
        cx.notify();
    }

    pub(crate) fn run_confirm(&mut self, cx: &mut Context<Self>) {
        let Some(confirm) = self.confirm.take() else { return };
        match confirm.action {
            ConfirmAction::CopyDataAgain => self.copy_data_again(cx),
        }
    }

    fn copy_data_again(&mut self, cx: &mut Context<Self>) {
        self.data_busy = true;
        self.error = None;
        // 1. Stop the daemon: SQLite's backup API reads the installed file, and
        //    the preview's own file must not be open when it is replaced.
        if let Some(mut daemon) = self.daemon.take() {
            daemon.shutdown();
        }
        self.snapshot = None;
        self.members = None;
        let installed = paths::installed_db_path();
        let target = self.db_path.clone();
        // 2. The real database is 770 MB, so the copy runs off the UI thread and
        //    Settings keeps saying "Copying…" until it is done.
        let task = cx
            .background_executor()
            .spawn(async move { install_copy(&installed, &target) });
        cx.spawn(async move |this, cx: &mut AsyncApp| {
            let result = task.await;
            cx.update(|app| {
                if let Some(root) = this.upgrade() {
                    root.update(app, |root, cx| root.finish_copy_data_again(result, cx));
                }
            });
        })
        .detach();
        cx.notify();
    }

    fn finish_copy_data_again(&mut self, result: Result<PathBuf, String>, cx: &mut Context<Self>) {
        self.data_busy = false;
        match result {
            Ok(backup) => {
                self.notice = Some(format!(
                    "Copied the Bitbots data. The earlier database is at {}.",
                    backup.display()
                ));
            }
            Err(error) => self.error = Some(error),
        }
        // Back to whatever the copy left behind.
        if self.first_run.is_none() {
            let (handle, updates) = start(self.db_path.clone());
            self.daemon = Some(handle);
            let this = cx.weak_entity();
            Self::follow(updates, &this, cx);
        }
        cx.notify();
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
        let (handle, updates) = start(self.db_path.clone());
        self.daemon = Some(handle);
        let this = cx.weak_entity();
        Self::follow(updates, &this, cx);
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

/// "Copy my Bitbots data again": stop (the caller does that), move the old file
/// **and its write-ahead log and shared-memory sidecars** aside, copy the
/// installed one, and say where the backup went.
///
/// The sidecars matter: the daemon is killed rather than closed cleanly, so a
/// `-wal` can survive, and SQLite would replay it onto the fresh copy.
pub fn install_copy(source: &std::path::Path, target: &std::path::Path) -> Result<PathBuf, String> {
    if !source.is_file() {
        return Err("The installed Bitbots has no database on this computer.".to_owned());
    }
    let mut backup = target.with_extension("db.before-copy");
    if backup.exists() || sidecar(&backup, "-wal").exists() {
        let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
        backup = target.with_file_name(format!("bloblex.db.before-copy-{stamp}"));
    }
    if target.exists() {
        std::fs::rename(target, &backup).map_err(|error| format!("Could not set the old database aside: {error}"))?;
    }
    for suffix in ["-wal", "-shm"] {
        let from = sidecar(target, suffix);
        if from.exists() {
            std::fs::rename(&from, sidecar(&backup, suffix))
                .map_err(|error| format!("Could not set the old database's {suffix} file aside: {error}"))?;
        }
    }
    if let Some(folder) = target.parent() {
        std::fs::create_dir_all(folder).map_err(|error| format!("Could not prepare the app's folder: {error}"))?;
    }
    let from = rusqlite::Connection::open(source).map_err(|error| format!("Could not open the Bitbots database: {error}"))?;
    let mut to = rusqlite::Connection::open(target).map_err(|error| format!("Could not create the preview database: {error}"))?;
    {
        let copy = rusqlite::backup::Backup::new(&from, &mut to)
            .map_err(|error| format!("Could not start the database copy: {error}"))?;
        copy.run_to_completion(100, std::time::Duration::from_millis(5), None)
            .map_err(|error| format!("Could not finish the database copy: {error}"))?;
    }
    crate::trace::log(format!("database copied again from {}", source.display()));
    Ok(backup)
}

/// `<path>-wal`, `<path>-shm`: SQLite's sidecar names.
fn sidecar(path: &std::path::Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(suffix);
    PathBuf::from(name)
}

impl Render for Root {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.watch_appearance(window, cx);
        // Any window close stops the daemon child: `Root` owns the handle.
        let colors = self.colors();
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

        let sidebar = self.render_sidebar(colors, window, cx);
        let conversations = if self.page == Page::Settings {
            div().into_any_element()
        } else {
            self.render_conversations(colors, cx)
        };
        let pane = match self.page {
            Page::Conversation => self.render_conversation_pane(colors, window, cx),
            Page::Team => self.render_team_page(colors, window, cx),
            Page::Settings => self.render_settings_page(colors, cx),
        };
        let overlays = self.render_dialogs(colors, cx);

        div()
            .relative()
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
            .when_some(self.daemon_banner(cx), |this, banner| this.child(banner))
            .when_some(self.error.clone(), |this, message| this.child(self.render_error_bar(&message, colors, cx)))
            .when_some(self.notice.clone(), |this, message| this.child(self.render_notice_bar(&message, colors, cx)))
            .when_some(overlays, |this, overlays| this.child(overlays))
            .into_any_element()
    }
}

impl Root {
    /// The daemon's own state, when it is worth a line under the window.
    fn daemon_banner(&mut self, cx: &mut Context<Self>) -> Option<gpui::AnyElement> {
        let colors = self.colors();
        match &self.daemon_state {
            DaemonState::Restarting { attempt, delay_ms } => Some(
                div()
                    .w_full()
                    .px(px(12.0))
                    .py(px(6.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .bg(colors.raised)
                    .border_t_1()
                    .border_color(colors.line)
                    .text_size(px(theme::FS_XS))
                    .text_color(colors.status_working)
                    .child(SharedString::from(format!(
                        "The Bitbots daemon stopped. Restarting in {} s (attempt {attempt} of {}).",
                        delay_ms.div_ceil(1000),
                        crate::daemon::BACKOFF_MS.len()
                    )))
                    .into_any_element(),
            ),
            DaemonState::Stopped(reason) => {
                let reason = reason.clone();
                let retry = self.render_retry(colors, cx);
                Some(
                    div()
                        .w_full()
                        .px(px(12.0))
                        .py(px(6.0))
                        .flex()
                        .items_center()
                        .gap(px(10.0))
                        .bg(colors.raised)
                        .border_t_1()
                        .border_color(colors.line)
                        .text_size(px(theme::FS_XS))
                        .text_color(colors.status_danger)
                        .child(div().flex_1().truncate().child(SharedString::from(format!(
                            "{reason} It stopped {} times, so Bitbots will not start it again by itself.",
                            crate::daemon::BACKOFF_MS.len()
                        ))))
                        .child(retry)
                        .into_any_element(),
                )
            }
            _ => None,
        }
    }

    /// The retry button lives beside the banner, but needs a listener, so it is
    /// rendered from here.
    fn render_retry(&mut self, colors: Colors, cx: &mut Context<Self>) -> gpui::AnyElement {
        div()
            .id("retry-daemon")
            .h(px(22.0))
            .px(px(8.0))
            .flex()
            .items_center()
            .rounded(px(theme::RADIUS_SM))
            .bg(colors.raised)
            .text_color(colors.text_1)
            .text_size(px(theme::FS_2XS))
            .cursor_pointer()
            .hover(|style| style.bg(colors.selected))
            .child("Retry")
            .on_click(cx.listener(|root, _: &gpui::ClickEvent, _window, cx| {
                if let Some(daemon) = root.daemon.as_ref() {
                    daemon.send(Command::Restart);
                }
                root.error = None;
                cx.notify();
            }))
            .into_any_element()
    }

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
            (colors.accent, colors.text_inverse)
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

    #[test]
    fn copying_again_sets_the_old_database_aside_and_says_where() {
        let folder = std::env::temp_dir().join(format!("bitbots-native-again-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let source = folder.join("installed.db");
        let target = folder.join("preview.db");
        let from = rusqlite::Connection::open(&source).unwrap();
        from.execute_batch("CREATE TABLE t (id INTEGER); INSERT INTO t VALUES (1);").unwrap();
        drop(from);
        let old = rusqlite::Connection::open(&target).unwrap();
        old.execute_batch("CREATE TABLE old (id INTEGER);").unwrap();
        drop(old);
        let backup = install_copy(&source, &target).expect("the copy runs");
        assert!(backup.exists(), "the old database is kept beside the new one");
        assert_eq!(backup.file_name().unwrap(), "preview.db.before-copy");
        let copied = rusqlite::Connection::open(&target).unwrap();
        let count: i64 = copied.query_row("SELECT COUNT(*) FROM t", [], |row| row.get(0)).unwrap();
        assert_eq!(count, 1, "the installed database's rows are there");
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn copying_again_moves_the_write_ahead_log_and_shared_memory_beside_the_backup() {
        let folder = std::env::temp_dir().join(format!("bitbots-native-sidecars-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let source = folder.join("installed.db");
        let target = folder.join("preview.db");
        let from = rusqlite::Connection::open(&source).unwrap();
        from.execute_batch("CREATE TABLE t (id INTEGER); INSERT INTO t VALUES (7);").unwrap();
        drop(from);
        // The daemon is killed rather than closed, so its sidecars can survive.
        std::fs::write(&target, b"old database").unwrap();
        std::fs::write(sidecar(&target, "-wal"), b"stale write-ahead log").unwrap();
        std::fs::write(sidecar(&target, "-shm"), b"stale shared memory").unwrap();

        let backup = install_copy(&source, &target).expect("the copy runs");

        assert_eq!(backup.file_name().unwrap(), "preview.db.before-copy");
        assert_eq!(std::fs::read(&backup).unwrap(), b"old database", "the old database is beside the new one");
        assert_eq!(
            std::fs::read(sidecar(&backup, "-wal")).unwrap(),
            b"stale write-ahead log",
            "the write-ahead log travels with it, under the backup's name"
        );
        assert_eq!(std::fs::read(sidecar(&backup, "-shm")).unwrap(), b"stale shared memory");
        assert!(!sidecar(&target, "-wal").exists(), "no wal is left to be replayed onto the new copy");
        assert!(!sidecar(&target, "-shm").exists());
        let copied = rusqlite::Connection::open(&target).unwrap();
        let count: i64 = copied.query_row("SELECT COUNT(*) FROM t", [], |row| row.get(0)).unwrap();
        assert_eq!(count, 1, "the new copy is the installed database");
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn copying_again_without_an_installed_database_says_so() {
        let folder = std::env::temp_dir().join(format!("bitbots-native-none-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let error = install_copy(&folder.join("missing.db"), &folder.join("preview.db")).unwrap_err();
        assert!(error.contains("no database"));
        let _ = std::fs::remove_dir_all(&folder);
    }
}
