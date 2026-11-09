//! Changes: the right-hand session rail, the review screen, the diff body, the
//! strip a conversation shows after a turn changed files, and the pull request
//! dialog. Ported from `ui/SessionRail.tsx`, `ui/SessionRailExtras.tsx`,
//! `ui/ChangesView.tsx`, `ui/DiffBody.tsx`, `ui/ChangeBanner.tsx` and
//! `ui/PullRequestDialog.tsx`.
//!
//! The daemon decides what is allowed and takes the folder from the conversation:
//! this window only ever names a conversation and a path the repository itself
//! listed. Nothing here writes to the repository directly.

pub mod banner;
pub mod diff;
pub mod rail;
pub mod review;

use std::collections::{BTreeMap, BTreeSet};

use gpui::{div, prelude::*, px, AnyElement, Context, Entity, SharedString};
use serde_json::{json, Value};

use crate::state::changes::{
    normalize_repository, Checkpoints, PullRequestCreated, PullRequestDraft, RepositoryState, Workspace,
};
use crate::theme::{self, Colors};
use crate::ui::root::Root;
use crate::ui::text_input::TextInput;

/// `session.environment`: where the conversation's folder is, and its branch.
#[derive(Clone, Debug, Default)]
pub struct Environment {
    pub project_path: Option<String>,
    pub git_repo: bool,
    pub branch: Option<String>,
    pub detached: bool,
}

impl Environment {
    pub fn from_json(value: &Value) -> Option<Self> {
        if !value.is_object() {
            return None;
        }
        let text = |key: &str| value.get(key).and_then(Value::as_str).filter(|text| !text.is_empty()).map(str::to_owned);
        Some(Environment {
            project_path: text("projectPath"),
            git_repo: value.get("gitRepo").and_then(Value::as_bool).unwrap_or(false),
            branch: text("branch"),
            detached: value.get("detached").and_then(Value::as_bool).unwrap_or(false),
        })
    }
}

/// Which layout the diff body uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiffMode {
    Unified,
    Split,
}

/// The review screen's state.
pub struct ReviewState {
    pub session_id: String,
    pub selected: Option<String>,
    pub repository: Option<RepositoryState>,
    /// Whether the repository was asked for this time.
    pub asked_repository: bool,
    pub diff: Option<String>,
    pub diff_path: Option<String>,
    pub diff_error: Option<String>,
    pub mode: DiffMode,
    /// The list the unified body scrolls with, and how many rows it holds.
    pub list: Option<gpui::ListState>,
    pub list_rows: usize,
    pub busy: bool,
    /// The file whose discard is waiting to be confirmed.
    pub discarding: Option<String>,
    pub failure: Option<String>,
    /// One comment per file, sent to the bot as an ordinary message.
    pub comment_input: Option<Entity<TextInput>>,
    pub comments: Vec<(String, String)>,
    pub sending: bool,
}

/// The pull request dialog's state.
pub struct PullRequestState {
    pub session_id: String,
    pub repository: RepositoryState,
    pub draft: Option<PullRequestDraft>,
    pub draft_asked: bool,
    pub ask_instead: bool,
    pub as_draft: bool,
    pub commit_first: bool,
    pub pending: bool,
    pub failure: Option<String>,
    pub created: Option<PullRequestCreated>,
    pub title_input: Option<Entity<TextInput>>,
    pub body_input: Option<Entity<TextInput>>,
    pub message_input: Option<Entity<TextInput>>,
}

/// Everything this slice keeps for the rail, the review screen and the dialog.
#[derive(Default)]
pub struct ChangesState {
    /// Whether the rail is on screen in the conversation view.
    pub rail_open: bool,
    /// The rail cards the owner folded.
    pub rail_collapsed: BTreeSet<String>,
    pub repository: BTreeMap<String, RepositoryState>,
    pub repository_failed: BTreeSet<String>,
    pub workspace: BTreeMap<String, Workspace>,
    pub environment: BTreeMap<String, Environment>,
    pub checkpoints: BTreeMap<String, Checkpoints>,
    /// What was read for the open conversation, so the rail asks once.
    pub read_for: Option<String>,
    /// The conversations whose checkpoints were asked for, so the change banner
    /// asks once even with the rail hidden.
    pub checkpoints_requested: BTreeSet<String>,
    pub review: Option<ReviewState>,
    pub pull_request: Option<PullRequestState>,
    /// The files the change banner was dismissed for, per conversation.
    pub banner_dismissed: BTreeMap<String, Vec<String>>,
    pub banner_open: bool,
    /// The change a single-file undo is waiting to confirm.
    pub undo_confirm: Option<String>,
    pub notice: Option<(bool, String)>,
}

impl ChangesState {
    pub fn repository(&self, session_id: &str) -> Option<&RepositoryState> {
        self.repository.get(session_id)
    }
}

impl Root {
    /// Show or hide the session rail. The conversation header's button calls
    /// this; so does the shortcut.
    pub fn toggle_rail(&mut self, cx: &mut Context<Self>) {
        self.changes.rail_open = !self.changes.rail_open;
        if self.changes.rail_open {
            self.read_rail(cx);
        }
        cx.notify();
    }

    /// Read everything the rail shows for the open conversation, once.
    pub(crate) fn read_rail(&mut self, cx: &mut Context<Self>) {
        let Some(session_id) = self.session_id.clone() else { return };
        self.changes.read_for = Some(session_id.clone());
        self.request_repository(session_id.clone(), cx);
        self.request_workspace(session_id.clone(), cx);
        self.request_checkpoints(session_id.clone(), cx);
        self.request_environment(session_id.clone(), cx);
        // The rail's Schedules card reads the automations once, quietly.
        if !self.schedules.loaded {
            self.load_automations(cx);
        }
    }

    pub(crate) fn request_repository(&mut self, session_id: String, cx: &mut Context<Self>) {
        self.daemon_command(
            crate::daemon::Command::Rpc {
                tag: "rail-repository".to_owned(),
                method: "session.repository".to_owned(),
                params: json!({ "sessionId": session_id }),
            },
            cx,
        );
    }

    fn request_workspace(&mut self, session_id: String, cx: &mut Context<Self>) {
        self.daemon_command(
            crate::daemon::Command::Rpc {
                tag: "rail-workspace".to_owned(),
                method: "session.workspace".to_owned(),
                params: json!({ "sessionId": session_id }),
            },
            cx,
        );
    }

    fn request_environment(&mut self, session_id: String, cx: &mut Context<Self>) {
        self.daemon_command(
            crate::daemon::Command::Rpc {
                tag: "rail-environment".to_owned(),
                method: "session.environment".to_owned(),
                params: json!({ "sessionId": session_id }),
            },
            cx,
        );
    }

    pub(crate) fn request_checkpoints(&mut self, session_id: String, cx: &mut Context<Self>) {
        self.changes.checkpoints_requested.insert(session_id.clone());
        self.daemon_command(
            crate::daemon::Command::Rpc {
                tag: "rail-checkpoints".to_owned(),
                method: "session.checkpoints".to_owned(),
                params: json!({ "sessionId": session_id }),
            },
            cx,
        );
    }

    /// A turn ended or a file changed: the rail's three readings are stale.
    pub(crate) fn changes_event(&mut self, event: &crate::daemon::client::DaemonEvent, cx: &mut Context<Self>) {
        if !matches!(
            event.event_type.as_str(),
            "turn.completed" | "turn.error" | "tool.changed" | "command.changed" | "session.changed" | "file.changed"
        ) {
            return;
        }
        let watching = self.changes.rail_open || self.changes.checkpoints.contains_key(&self.session_id.clone().unwrap_or_default());
        if !watching || self.changes.review.is_some() {
            return;
        }
        let Some(session_id) = self.session_id.clone() else { return };
        self.request_repository(session_id.clone(), cx);
        self.request_checkpoints(session_id, cx);
    }

    /// One answer to one of this slice's calls; `true` when the tag was ours.
    pub(crate) fn changes_rpc(&mut self, tag: &str, value: &Value, cx: &mut Context<Self>) -> bool {
        match tag {
            "rail-repository" => {
                let session_id = value.get("sessionId").and_then(Value::as_str).map(str::to_owned).or_else(|| self.session_id.clone());
                let Some(session_id) = session_id else { return true };
                match normalize_repository(value) {
                    Some(state) => {
                        self.changes.repository.insert(session_id.clone(), state.clone());
                        self.changes.repository_failed.remove(&session_id);
                        // The review screen reads the same answer: one call serves
                        // both, and the two can never disagree.
                        if let Some(review) = self.changes.review.as_mut().filter(|review| review.session_id == session_id) {
                            review.asked_repository = true;
                            if review.selected.is_none() {
                                review.selected = state.files.first().map(|file| file.repository_path.clone());
                            }
                            review.repository = Some(state);
                        }
                    }
                    None => {
                        self.changes.repository_failed.insert(session_id.clone());
                        if let Some(review) = self.changes.review.as_mut().filter(|review| review.session_id == session_id) {
                            review.asked_repository = true;
                            review.failure = Some("The repository could not be read just now.".to_owned());
                        }
                    }
                }
            }
            "rail-workspace" => {
                if let Some(session_id) = value.get("sessionId").and_then(Value::as_str).map(str::to_owned) {
                    if let Ok(workspace) = serde_json::from_value::<Workspace>(value.clone()) {
                        self.changes.workspace.insert(session_id, workspace);
                    }
                }
            }
            "rail-environment" => {
                if let Some(environment) = Environment::from_json(value) {
                    if let Some(session_id) = self.session_id.clone() {
                        self.changes.environment.insert(session_id, environment);
                    }
                }
            }
            "rail-checkpoints" => {
                if let Ok(checkpoints) = serde_json::from_value::<Checkpoints>(value.clone()) {
                    if !checkpoints.session_id.is_empty() {
                        self.changes.checkpoints.insert(checkpoints.session_id.clone(), checkpoints);
                    }
                }
            }
            "review-diff" => {
                if let Some(review) = self.changes.review.as_mut() {
                    review.diff = value.get("diff").and_then(Value::as_str).map(str::to_owned);
                    review.diff_error = None;
                    let rows = review.diff.as_deref().map(|diff| crate::state::changes::parse_unified_diff(diff).lines.len()).unwrap_or(0);
                    match review.list.as_mut() {
                        Some(_) if review.list_rows == rows => {}
                        Some(list) => {
                            list.reset(rows);
                            review.list_rows = rows;
                        }
                        None => {
                            review.list = Some(crate::ui::changes::diff::diff_list(rows));
                            review.list_rows = rows;
                        }
                    }
                }
            }
            "review-stage" => {
                if let Some(review) = self.changes.review.as_mut() {
                    review.busy = false;
                    let files = value.get("files").and_then(Value::as_array).cloned().unwrap_or_default();
                    if let Some(repository) = review.repository.clone() {
                        let merged = json!({
                            "projectPath": repository.project_path,
                            "repoRoot": repository.repo_root,
                            "gitRepo": repository.git_repo,
                            "branch": repository.branch,
                            "detached": repository.detached,
                            "addedLines": repository.added_lines,
                            "removedLines": repository.removed_lines,
                            "truncated": repository.truncated,
                            "files": files,
                        });
                        if let Some(state) = normalize_repository(&merged) {
                            review.repository = Some(state);
                        }
                    }
                }
            }
            "review-revert" | "file-revert" => {
                self.changes.undo_confirm = None;
                if let Some(review) = self.changes.review.as_mut() {
                    review.busy = false;
                    review.discarding = None;
                    review.diff = None;
                    review.diff_path = None;
                }
                let name = value.get("name").and_then(Value::as_str).unwrap_or("The file").to_owned();
                self.changes.notice = Some((true, format!("{name} is back as it was before this change. The agent was not told.")));
                if let Some(session_id) = self.session_id.clone() {
                    self.request_repository(session_id.clone(), cx);
                    self.request_checkpoints(session_id, cx);
                }
            }
            "pr-draft" => {
                if let Some(dialog) = self.changes.pull_request.as_mut() {
                    dialog.draft_asked = true;
                    dialog.draft = serde_json::from_value::<PullRequestDraft>(value.clone()).ok();
                    if let Some(draft) = dialog.draft.as_ref() {
                        let title = draft.title.clone();
                        let body = draft.body.clone();
                        if let Some(input) = dialog.title_input.clone() {
                            input.update(cx, |input, cx| input.set_text(title.clone(), cx));
                        }
                        if let Some(input) = dialog.body_input.clone() {
                            input.update(cx, |input, cx| input.set_text(body, cx));
                        }
                        if let Some(input) = dialog.message_input.clone() {
                            input.update(cx, |input, cx| input.set_text(title, cx));
                        }
                    }
                }
            }
            "pr-create" => {
                let created = serde_json::from_value::<PullRequestCreated>(value.clone()).unwrap_or_default();
                if let Some(dialog) = self.changes.pull_request.as_mut() {
                    dialog.pending = false;
                    dialog.created = Some(created);
                }
                if let Some(session_id) = self.session_id.clone() {
                    self.request_repository(session_id, cx);
                }
            }
            _ => return false,
        }
        cx.notify();
        true
    }

    /// A failure from one of this slice's calls: `true` when the tag was ours.
    pub(crate) fn changes_rpc_failed(&mut self, tag: &str, message: &str, cx: &mut Context<Self>) -> bool {
        match tag {
            "rail-repository" => {
                if let Some(session_id) = self.session_id.clone() {
                    self.changes.repository_failed.insert(session_id);
                }
            }
            "rail-workspace" | "rail-environment" | "rail-checkpoints" => {}
            "review-diff" => {
                if let Some(review) = self.changes.review.as_mut() {
                    review.diff_error = Some(message.to_owned());
                    review.diff = None;
                }
            }
            "review-stage" | "review-revert" | "file-revert" => {
                if let Some(review) = self.changes.review.as_mut() {
                    review.busy = false;
                    review.failure = Some(message.to_owned());
                }
                self.changes.notice = Some((false, message.to_owned()));
            }
            "pr-draft" => {
                if let Some(dialog) = self.changes.pull_request.as_mut() {
                    dialog.draft_asked = true;
                    dialog.draft = None;
                }
            }
            "pr-create" => {
                if let Some(dialog) = self.changes.pull_request.as_mut() {
                    dialog.pending = false;
                    dialog.failure = Some(message.to_owned());
                }
            }
            _ => return false,
        }
        cx.notify();
        true
    }

    /// The conversation changed: the rail's readings belong to the old one.
    pub(crate) fn changes_select_session(&mut self, cx: &mut Context<Self>) {
        self.changes.read_for = None;
        self.changes.checkpoints_requested.clear();
        self.changes.review = None;
        self.changes.pull_request = None;
        self.changes.undo_confirm = None;
        if self.changes.rail_open {
            self.read_rail(cx);
        }
    }

    /// The review screen (`ChangesView`), over the window.
    pub fn open_changes_view(&mut self, path: Option<String>, cx: &mut Context<Self>) {
        let Some(session_id) = self.session_id.clone() else { return };
        let this = cx.weak_entity();
        let comment_input = cx.new(move |cx| {
            let mut input = TextInput::new("What should the bot change here?", px(20.0), px(theme::FS_SM), cx);
            input.set_on_change(Box::new(move |_text: &str, _caret: usize, cx: &mut gpui::App| {
                if let Some(root) = this.upgrade() {
                    root.update(cx, |_root, cx| cx.notify());
                }
            }));
            input
        });
        self.changes.review = Some(ReviewState {
            session_id: session_id.clone(),
            selected: path,
            repository: None,
            asked_repository: true,
            diff: None,
            diff_path: None,
            diff_error: None,
            mode: DiffMode::Unified,
            list: None,
            list_rows: 0,
            busy: false,
            discarding: None,
            failure: None,
            comment_input: Some(comment_input),
            comments: Vec::new(),
            sending: false,
        });
        self.request_repository(session_id, cx);
        cx.notify();
    }

    /// The pull request dialog.
    pub fn open_pull_request(&mut self, cx: &mut Context<Self>) {
        let Some(session_id) = self.session_id.clone() else { return };
        let Some(repository) = self.changes.repository(&session_id).cloned() else { return };
        let this = cx.weak_entity();
        let field = |placeholder: &'static str, this: gpui::WeakEntity<Root>, cx: &mut Context<Root>| {
            cx.new(move |cx| {
                let mut input = TextInput::new(placeholder, px(20.0), px(theme::FS_SM), cx);
                input.set_on_change(Box::new(move |_text: &str, _caret: usize, cx: &mut gpui::App| {
                    if let Some(root) = this.upgrade() {
                        root.update(cx, |_root, cx| cx.notify());
                    }
                }));
                input
            })
        };
        let title_input = field("Title", this.clone(), cx);
        let body_input = field("Description", this.clone(), cx);
        let message_input = field("Commit message", this, cx);
        self.changes.pull_request = Some(PullRequestState {
            session_id: session_id.clone(),
            repository,
            draft: None,
            draft_asked: false,
            ask_instead: false,
            as_draft: false,
            commit_first: true,
            pending: false,
            failure: None,
            created: None,
            title_input: Some(title_input),
            body_input: Some(body_input),
            message_input: Some(message_input),
        });
        self.daemon_command(
            crate::daemon::Command::Rpc { tag: "pr-draft".to_owned(), method: "session.pr.draft".to_owned(), params: json!({ "sessionId": session_id }) },
            cx,
        );
        cx.notify();
    }

    /// Escape closes the top-most overlay this slice owns.
    pub(crate) fn changes_close_overlay(&mut self, cx: &mut Context<Self>) -> bool {
        if self.tasks_close_overlay(cx) {
            return true;
        }
        if self.schedules_close_overlay(cx) {
            return true;
        }
        if self.changes.pull_request.take().is_some() {
            cx.notify();
            return true;
        }
        if self.changes.review.take().is_some() {
            self.changes.undo_confirm = None;
            cx.notify();
            return true;
        }
        false
    }

    /// Everything this slice paints over the window, in one place so the root
    /// view needs one line.
    pub(crate) fn render_slice4_overlays(&mut self, colors: Colors, cx: &mut Context<Self>) -> Option<AnyElement> {
        if let Some(element) = self.render_tasks_overlay(colors, cx) {
            return Some(element);
        }
        if let Some(element) = self.render_schedules_overlay(colors, cx) {
            return Some(element);
        }
        if let Some(element) = self.render_changes_overlay(colors, cx) {
            return Some(element);
        }
        None
    }

    fn render_changes_overlay(&mut self, colors: Colors, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.changes.pull_request.is_some() {
            return self.render_pull_request_dialog(colors, cx);
        }
        if self.changes.review.is_some() {
            let card = self.render_changes_view(colors, cx);
            return Some(
                div()
                    .id("changes-backdrop")
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(colors.scrim)
                    .on_mouse_down(gpui::MouseButton::Left, cx.listener(|root, _: &gpui::MouseDownEvent, _window, cx| {
                        root.changes.review = None;
                        cx.notify();
                    }))
                    .child(
                        div()
                            .w(px(1_040.0))
                            .h(px(700.0))
                            .max_w(gpui::relative(0.96))
                            .rounded(px(theme::RADIUS_LG))
                            .overflow_hidden()
                            .bg(colors.overlay)
                            .border_1()
                            .border_color(colors.line_strong)
                            .on_mouse_down(gpui::MouseButton::Left, cx.listener(|_, _: &gpui::MouseDownEvent, _window, cx| {
                                cx.stop_propagation();
                            }))
                            .child(card),
                    )
                    .into_any_element(),
            );
        }
        None
    }

    /// The daemon events this slice follows, in one place.
    pub(crate) fn slice4_event(&mut self, event: &crate::daemon::client::DaemonEvent, cx: &mut Context<Self>) {
        self.tasks_event(event, cx);
        self.schedules_event(event, cx);
        self.changes_event(event, cx);
    }

    /// One RPC answer for this slice, in one place.
    pub(crate) fn slice4_rpc(&mut self, tag: &str, value: &Value, cx: &mut Context<Self>) -> bool {
        if self.tasks_rpc(tag, value, cx) {
            return true;
        }
        if self.schedules_rpc(tag, value, cx) {
            return true;
        }
        self.changes_rpc(tag, value, cx)
    }

    /// One failed RPC for this slice, in one place.
    pub(crate) fn slice4_rpc_failed(&mut self, tag: &str, message: &str, cx: &mut Context<Self>) -> bool {
        if self.tasks_rpc_failed(tag, message, cx) {
            return true;
        }
        if self.schedules_rpc_failed(tag, message, cx) {
            return true;
        }
        self.changes_rpc_failed(tag, message, cx)
    }

    /// Keys the terminal dock and the rail answer, before the window's own.
    pub(crate) fn slice4_key(&mut self, event: &gpui::KeyDownEvent, window: &mut gpui::Window, cx: &mut Context<Self>) -> bool {
        let keystroke = &event.keystroke;
        let ctrl = keystroke.modifiers.control || keystroke.modifiers.platform;
        let key = keystroke.key.to_lowercase();
        if ctrl && (key == "`" || key == "backquote") {
            self.toggle_terminal(window, cx);
            return true;
        }
        // A keystroke belongs to the terminal while the dock has the caret and a
        // tab is open: the shell is the thing being typed into.
        if self.terminal_captures_keys(window, cx) {
            return self.terminal_key(event, cx);
        }
        false
    }

    /// The dock's own overlay-free close, for the header's button.
    pub fn hide_terminal(&mut self, cx: &mut Context<Self>) {
        self.terminal.open = false;
        cx.notify();
    }

    /// A row of the rail: the small text the cards share.
    pub(crate) fn rail_row(&self, label: String, colors: Colors) -> AnyElement {
        div()
            .flex()
            .items_center()
            .gap(px(6.0))
            .text_size(px(theme::FS_XS))
            .text_color(colors.text_2)
            .child(div().flex_1().min_w(px(0.0)).truncate().child(SharedString::from(label)))
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_environment_reply_is_read_without_guessing() {
        let environment = Environment::from_json(&json!({
            "projectPath": "D:/work/app",
            "gitRepo": true,
            "branch": "main",
            "detached": false,
            "host": "this computer"
        }))
        .unwrap();
        assert_eq!(environment.project_path.as_deref(), Some("D:/work/app"));
        assert!(environment.git_repo);
        assert_eq!(environment.branch.as_deref(), Some("main"));
        assert!(!environment.detached);
        assert!(Environment::from_json(&json!("nope")).is_none());
    }

    #[test]
    fn the_rail_starts_hidden_and_a_selection_clears_its_readings() {
        let state = ChangesState::default();
        assert!(!state.rail_open);
        assert!(state.repository("s1").is_none());
    }
}
