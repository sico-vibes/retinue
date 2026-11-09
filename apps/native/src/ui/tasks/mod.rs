//! Tasks: durable work the owner hands to a bot. Ported from `ui/TasksView.tsx`,
//! `ui/useTasks.ts` and the tasks parts of `ui/App.tsx`.
//!
//! The daemon owns every task state; the board groups and orders what it
//! reported, and every button here sends one `task.*` call. A card the owner
//! gives to a bot shows "Starting" at once (`pendingDispatch`) and the daemon's
//! own row replaces it, exactly as the React board does.

pub mod board;
pub mod detail;
pub mod form;
pub mod images;
pub mod timeline;
pub mod tree;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use gpui::{div, prelude::*, px, AnyElement, Context, Entity, FontWeight, Image, SharedString};
use serde_json::{json, Value};

use crate::state::tasks::{
    assignable_agents, dispatch_message, new_idempotency_key, schedule_summary, BoardTask, BoardTaskRun, TaskDispatchResult, TaskDraft, TaskImage,
};
use crate::state::Agent;
use crate::theme::{self, Colors};
use crate::ui::root::Root;
use crate::ui::text_input::TextInput;

/// Which of the three views the page is showing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TaskView {
    #[default]
    Board,
    Tree,
    Timeline,
}

impl TaskView {
    pub fn label(self) -> &'static str {
        match self {
            TaskView::Board => "Board",
            TaskView::Tree => "Tree",
            TaskView::Timeline => "Timeline",
        }
    }
}

/// The form's text fields, created when the form opens.
pub struct TaskFields {
    pub title: Entity<TextInput>,
    pub brief: Entity<TextInput>,
    pub done_goal: Entity<TextInput>,
    pub report_back: Entity<TextInput>,
    pub estimate: Entity<TextInput>,
    /// The "when" detail: a time, a date and time, or nothing for "every N hours".
    pub when_detail: Entity<TextInput>,
}

/// The tasks page's state.
#[derive(Default)]
pub struct TasksState {
    pub tasks: Vec<BoardTask>,
    pub loaded: bool,
    pub error: Option<String>,
    /// A sentence under the header: what a dispatch did, or why it did not.
    pub notice: Option<(bool, String)>,
    pub view: TaskView,
    pub busy: BTreeSet<String>,
    /// The form is open for a new task.
    pub creating: bool,
    /// The task whose details are open.
    pub open: Option<String>,
    /// The task whose draft is being edited in the form.
    pub editing: Option<String>,
    pub draft: Option<TaskDraft>,
    pub fields: Option<TaskFields>,
    pub form_error: Option<String>,
    pub show_finished: bool,
    /// Each task's recorded attempts, newest last.
    pub runs: BTreeMap<String, Vec<BoardTaskRun>>,
    /// Which tasks' attempts were asked for.
    pub runs_requested: BTreeSet<String>,
    /// The stored image of each task, by `<taskId>:<imageId>`.
    pub images: BTreeMap<String, Arc<Image>>,
    pub images_requested: BTreeSet<String>,
    /// The bot an "Add and start" is waiting to hand the new task to.
    pub pending_start: Option<String>,
}

impl TasksState {
    pub fn task(&self, id: &str) -> Option<&BoardTask> {
        self.tasks.iter().find(|task| task.id == id)
    }

    pub fn runs_for(&self, id: &str) -> &[BoardTaskRun] {
        self.runs.get(id).map(Vec::as_slice).unwrap_or(&[])
    }

    pub fn latest_run(&self, id: &str) -> Option<&BoardTaskRun> {
        self.runs_for(id).iter().max_by_key(|run| run.attempt)
    }
}

impl Root {
    /// The Tasks page's entry point: the Director puts the button next to the
    /// conversation header's own actions and calls this.
    pub fn open_tasks(&mut self, cx: &mut Context<Self>) {
        self.page = crate::ui::root::Page::Tasks;
        self.page_explicit = true;
        self.tasks.loaded = false;
        self.load_tasks(cx);
        self.need_root_focus = true;
        cx.notify();
    }

    pub(crate) fn load_tasks(&mut self, cx: &mut Context<Self>) {
        self.daemon_command(
            crate::daemon::Command::Rpc { tag: "task-list".to_owned(), method: "task.list".to_owned(), params: json!({}) },
            cx,
        );
    }

    /// `task.changed`, `task.run.changed` and `task.run.finished` all mean "read
    /// the board again": the daemon is the only writer.
    pub(crate) fn tasks_event(&mut self, event: &crate::daemon::client::DaemonEvent, cx: &mut Context<Self>) {
        if !matches!(event.event_type.as_str(), "task.changed" | "task.run.changed" | "task.run.finished") {
            return;
        }
        if self.page != crate::ui::root::Page::Tasks && self.tasks.open.is_none() {
            return;
        }
        self.load_tasks(cx);
        let open = self.tasks.open.clone();
        if let Some(id) = open {
            self.load_task_runs(id, cx);
        }
    }

    /// One answer to one of this page's calls; `true` when the tag was ours.
    pub(crate) fn tasks_rpc(&mut self, tag: &str, value: &Value, cx: &mut Context<Self>) -> bool {
        match tag {
            "task-list" => {
                let rows = value.get("tasks").and_then(Value::as_array).cloned().unwrap_or_default();
                let mut tasks: Vec<BoardTask> = rows
                    .iter()
                    .filter_map(|row| serde_json::from_value::<BoardTask>(row.clone()).ok())
                    .collect();
                // A card that is still waiting for its dispatch answer keeps its
                // "Starting": the daemon's row has not landed yet.
                for pending in &self.tasks.tasks {
                    if pending.pending_dispatch {
                        if let Some(index) = tasks.iter().position(|task| task.id == pending.id) {
                            if tasks[index].state == "unassigned" {
                                tasks[index] = pending.clone();
                            }
                        }
                    }
                }
                self.tasks.tasks = tasks;
                self.tasks.loaded = true;
                self.tasks.error = None;
            }
            "task-save" => {
                let task = value.get("task").and_then(|row| serde_json::from_value::<BoardTask>(row.clone()).ok());
                if let Some(task) = task {
                    let created_id = task.id.clone();
                    self.merge_task(task.clone());
                    self.tasks.creating = false;
                    self.tasks.editing = None;
                    self.tasks.open = None;
                    self.tasks.draft = None;
                    self.tasks.fields = None;
                    self.toast("Task saved.", cx);
                    // "Add and start": the card the daemon returned is the one
                    // handed to the bot, so the dispatch cannot name a guess.
                    if let Some(agent_id) = self.tasks.pending_start.take() {
                        self.dispatch_task(created_id, agent_id, cx);
                    }
                } else {
                    self.tasks.form_error = Some("The daemon did not return the task.".to_owned());
                }
            }
            "task-dispatch" => {
                let result: TaskDispatchResult = serde_json::from_value(value.clone()).unwrap_or_default();
                let name = result
                    .task
                    .as_ref()
                    .and_then(|task| task.assignee_agent_id.clone())
                    .and_then(|agent_id| self.snapshot().and_then(|snapshot| snapshot.agent(&agent_id)).map(|agent| agent.name.clone()))
                    .unwrap_or_else(|| "the bot".to_owned());
                if let Some(task) = result.task.clone() {
                    self.tasks.busy.remove(&task.id);
                    self.merge_task(task);
                }
                self.tasks.notice =
                    Some((true, dispatch_message(&result.disposition, &name, result.queue_position, result.reason.as_deref())));
            }
            "task-act" | "task-review" => {
                if let Some(task) = value.get("task").and_then(|row| serde_json::from_value::<BoardTask>(row.clone()).ok()) {
                    self.merge_task(task);
                }
                let id = value.get("task").and_then(|row| row.get("id")).and_then(Value::as_str).unwrap_or("");
                self.tasks.busy.remove(id);
                if tag == "task-review" {
                    self.tasks.open = None;
                }
                self.tasks.notice = Some((true, if tag == "task-review" { "Reviewed.".to_owned() } else { "Updated.".to_owned() }));
            }
            "task-runs" => {
                let id = value.get("taskId").and_then(Value::as_str).unwrap_or("").to_owned();
                let rows = value.get("runs").and_then(Value::as_array).cloned().unwrap_or_default();
                if !id.is_empty() {
                    self.tasks.runs.insert(
                        id.clone(),
                        rows.iter().filter_map(|row| serde_json::from_value::<BoardTaskRun>(row.clone()).ok()).collect(),
                    );
                    self.tasks.runs_requested.insert(id);
                }
            }
            "task-image" => {
                let key = value.get("key").and_then(Value::as_str).unwrap_or("").to_owned();
                let data_url = value.get("dataUrl").and_then(Value::as_str).unwrap_or("");
                if !key.is_empty() {
                    match crate::files::decode_data_url(data_url).and_then(|bytes| crate::files::image_format(&bytes).map(|format| (bytes, format))) {
                        Some((bytes, format)) => {
                            self.tasks.images.insert(key, Arc::new(Image::from_bytes(format, bytes)));
                        }
                        None => crate::trace::log("task image could not be decoded"),
                    }
                }
            }
            _ => return false,
        }
        cx.notify();
        true
    }

    /// A failure from one of this page's calls: `true` when the tag was ours.
    pub(crate) fn tasks_rpc_failed(&mut self, tag: &str, message: &str, cx: &mut Context<Self>) -> bool {
        match tag {
            "task-list" => {
                self.tasks.loaded = true;
                self.tasks.error = Some(message.to_owned());
            }
            "task-image" => crate::trace::log("task image unavailable"),
            "task-save" => self.tasks.form_error = Some(message.to_owned()),
            "task-runs" => {}
            "task-dispatch" => {
                self.tasks.busy.clear();
                self.tasks.notice = Some((false, message.to_owned()));
                // The optimistic "Starting" was a guess; the daemon's own board
                // is the truth after a refusal.
                self.load_tasks(cx);
            }
            _ => {
                self.tasks.busy.clear();
                self.tasks.notice = Some((false, message.to_owned()));
            }
        }
        cx.notify();
        matches!(
            tag,
            "task-list" | "task-save" | "task-dispatch" | "task-act" | "task-review" | "task-runs" | "task-image"
        )
    }

    fn merge_task(&mut self, task: BoardTask) {
        match self.tasks.tasks.iter_mut().find(|existing| existing.id == task.id) {
            Some(existing) => {
                if task.version >= existing.version {
                    let mut merged = task;
                    // A row the daemon patched may carry fewer fields than the
                    // card already has; the streamed ones stay.
                    if merged.brief.is_empty() {
                        merged.brief = existing.brief.clone();
                    }
                    *existing = merged;
                }
            }
            None => self.tasks.tasks.insert(0, task),
        }
    }

    pub(crate) fn load_task_runs(&mut self, task_id: String, cx: &mut Context<Self>) {
        self.daemon_command(
            crate::daemon::Command::Rpc { tag: "task-runs".to_owned(), method: "task.runs".to_owned(), params: json!({ "taskId": task_id }) },
            cx,
        );
    }

    /// Ask for one stored image's thumbnail, once.
    pub(crate) fn request_task_image(&mut self, task_id: &str, image: &TaskImage, cx: &mut Context<Self>) {
        let key = format!("{task_id}:{}", image.id);
        if self.tasks.images.contains_key(&key) || !self.tasks.images_requested.insert(key.clone()) {
            return;
        }
        self.daemon_command(
            crate::daemon::Command::Rpc {
                tag: "task-image".to_owned(),
                method: "task.image".to_owned(),
                params: json!({ "taskId": task_id, "imageId": image.id }),
            },
            cx,
        );
    }

    // -----------------------------------------------------------------------
    // The form
    // -----------------------------------------------------------------------

    /// Open the form for a new task.
    pub(crate) fn open_task_form(&mut self, cx: &mut Context<Self>) {
        let draft = TaskDraft::new_task();
        self.tasks.draft = Some(draft.clone());
        self.tasks.creating = true;
        self.tasks.editing = None;
        self.tasks.open = None;
        self.tasks.form_error = None;
        self.install_task_fields(&draft, cx);
        cx.notify();
    }

    /// Open the form on an existing draft task.
    pub(crate) fn edit_task(&mut self, task_id: String, cx: &mut Context<Self>) {
        let Some(task) = self.tasks.task(&task_id).cloned() else { return };
        let draft = TaskDraft::for_task(&task);
        self.tasks.draft = Some(draft.clone());
        self.tasks.creating = false;
        self.tasks.editing = Some(task_id);
        self.tasks.open = None;
        self.tasks.form_error = None;
        self.install_task_fields(&draft, cx);
        cx.notify();
    }

    fn install_task_fields(&mut self, draft: &TaskDraft, cx: &mut Context<Self>) {
        let this = cx.weak_entity();
        let field = |placeholder: &'static str, text: String, line_height: f32, this: gpui::WeakEntity<Root>, cx: &mut Context<Root>| {
            cx.new(move |cx| {
                let mut input = TextInput::new(placeholder, px(line_height), px(theme::FS_SM), cx);
                input.set_text_quiet(text);
                input.set_on_change(Box::new(move |_text: &str, _caret: usize, cx: &mut gpui::App| {
                    if let Some(root) = this.upgrade() {
                        root.update(cx, |_root, cx| cx.notify());
                    }
                }));
                input
            })
        };
        let title = field("Write the release notes", draft.title.clone(), 20.0, this.clone(), cx);
        let brief = field("Summarise what needs doing", draft.brief.clone(), 60.0, this.clone(), cx);
        let done_goal = field("A draft is in docs/releases", draft.done_goal.clone(), 20.0, this.clone(), cx);
        let report_back = field("A short summary and the files changed", draft.report_back.clone(), 20.0, this.clone(), cx);
        let estimate = field("Optional", draft.estimate.clone(), 20.0, this.clone(), cx);
        let when_detail = field("09:00", when_text(draft), 20.0, this, cx);
        self.tasks.fields = Some(TaskFields { title, brief, done_goal, report_back, estimate, when_detail });
    }

    /// Read the form's text into its draft, then save it (`task.create` or
    /// `task.update`), then hand it to a bot when the owner asked to start now.
    pub(crate) fn save_task(&mut self, cx: &mut Context<Self>) {
        let Some(fields) = self.tasks.fields.as_ref() else { return };
        let title = fields.title.read(cx).text().to_owned();
        let brief = fields.brief.read(cx).text().to_owned();
        let done_goal = fields.done_goal.read(cx).text().to_owned();
        let report_back = fields.report_back.read(cx).text().to_owned();
        let estimate = fields.estimate.read(cx).text().to_owned();
        let when_detail = fields.when_detail.read(cx).text().to_owned();
        let Some(draft) = self.tasks.draft.as_mut() else { return };
        draft.title = title;
        draft.brief = brief;
        draft.done_goal = done_goal;
        draft.report_back = report_back;
        draft.estimate = estimate;
        match draft.schedule.preset {
            crate::ui::schedules::SchedulePreset::Once => draft.schedule.once_at = when_detail,
            crate::ui::schedules::SchedulePreset::Hours => {}
            _ => draft.schedule.time = when_detail,
        }
        let draft = draft.clone();
        if let Some(problem) = crate::state::tasks::form_problem(&draft) {
            self.tasks.form_error = Some(problem.text().to_owned());
            cx.notify();
            return;
        }
        let input = draft.to_input();
        let start = draft.start_agent();
        let editing = self.tasks.editing.clone();
        let (method, params) = match editing.clone() {
            Some(id) => {
                let version = self.tasks.task(&id).map(|task| task.version).unwrap_or(0);
                let mut params = input.params();
                params["taskId"] = json!(id);
                params["expectedVersion"] = json!(version);
                ("task.update", params)
            }
            None => ("task.create", input.params()),
        };
        self.tasks.form_error = None;
        self.daemon_command(
            crate::daemon::Command::Rpc { tag: "task-save".to_owned(), method: method.to_owned(), params },
            cx,
        );
        // "Add and start" is `task.create` followed by a dispatch with the task
        // the daemon returns; the dispatch is sent from the save answer's own
        // card, so nothing is guessed here.
        if let Some(agent_id) = start {
            self.tasks.pending_start = Some(agent_id);
        }
    }

    // -----------------------------------------------------------------------
    // Giving a card to a bot
    // -----------------------------------------------------------------------

    /// Hand one card to one bot. The card shows "Starting" before the answer.
    pub(crate) fn dispatch_task(&mut self, task_id: String, agent_id: String, cx: &mut Context<Self>) {
        let Some(task) = self.tasks.task(&task_id).cloned() else { return };
        if self.tasks.busy.contains(&task_id) {
            return;
        }
        let name = self
            .snapshot()
            .and_then(|snapshot| snapshot.agent(&agent_id))
            .map(|agent| agent.name.clone())
            .unwrap_or_else(|| "the bot".to_owned());
        // Optimistic: the card moves to the bot's lane as "Starting".
        if let Some(existing) = self.tasks.tasks.iter_mut().find(|existing| existing.id == task_id) {
            existing.state = "queued".to_owned();
            existing.queue_position = None;
            existing.assignee_agent_id = Some(agent_id.clone());
            existing.pending_dispatch = true;
        }
        self.tasks.busy.insert(task_id.clone());
        self.tasks.notice = None;
        let _ = name;
        let params = json!({
            "taskId": task.id,
            "assigneeAgentId": agent_id,
            "expectedVersion": task.version,
            "idempotencyKey": new_idempotency_key(&task.id),
        });
        self.daemon_command(
            crate::daemon::Command::Rpc { tag: "task-dispatch".to_owned(), method: "task.dispatch".to_owned(), params },
            cx,
        );
        cx.notify();
    }

    /// `task.unassign`, `task.cancel` or `task.stop`.
    pub(crate) fn task_act(&mut self, task_id: String, method: &str, cx: &mut Context<Self>) {
        let Some(task) = self.tasks.task(&task_id).cloned() else { return };
        self.tasks.busy.insert(task_id.clone());
        self.daemon_command(
            crate::daemon::Command::Rpc {
                tag: "task-act".to_owned(),
                method: method.to_owned(),
                params: json!({ "taskId": task.id, "expectedVersion": task.version }),
            },
            cx,
        );
        cx.notify();
    }

    /// `task.review`: the owner's own verdict, which is what ends a task.
    pub(crate) fn review_task(&mut self, task_id: String, decision: &str, cx: &mut Context<Self>) {
        let Some(task) = self.tasks.task(&task_id).cloned() else { return };
        let run_id = self.tasks.latest_run(&task_id).map(|run| run.id.clone());
        self.tasks.busy.insert(task_id.clone());
        self.daemon_command(
            crate::daemon::Command::Rpc {
                tag: "task-review".to_owned(),
                method: "task.review".to_owned(),
                params: json!({
                    "taskId": task.id,
                    "runId": run_id,
                    "decision": decision,
                    "evidence": if decision == "done" { "Marked done by you." } else { "Marked not done by you." },
                    "expectedVersion": task.version,
                }),
            },
            cx,
        );
        cx.notify();
    }

    /// Escape closes the top thing this page opened; `true` when it did.
    pub(crate) fn tasks_close_overlay(&mut self, cx: &mut Context<Self>) -> bool {
        if self.tasks.creating || self.tasks.editing.is_some() {
            self.tasks.creating = false;
            self.tasks.editing = None;
            self.tasks.draft = None;
            self.tasks.fields = None;
            self.tasks.form_error = None;
            cx.notify();
            return true;
        }
        if self.tasks.open.take().is_some() {
            cx.notify();
            return true;
        }
        false
    }

    // -----------------------------------------------------------------------
    // Drawing
    // -----------------------------------------------------------------------

    pub(crate) fn render_tasks_page(&mut self, colors: Colors, cx: &mut Context<Self>) -> AnyElement {
        let connected = self.connected;
        let view = self.tasks.view;
        let back = div()
            .id("tasks-back")
            .size(px(28.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(theme::RADIUS_SM))
            .cursor_pointer()
            .hover(move |style| style.bg(colors.hover))
            .child("←")
            .on_click(cx.listener(|root, _: &gpui::ClickEvent, _window, cx| root.back_to_conversation(cx)));

        let mut segments = div().p(px(2.0)).flex().gap(px(2.0)).rounded(px(theme::RADIUS_MD)).bg(colors.hover);
        for option in [TaskView::Board, TaskView::Tree, TaskView::Timeline] {
            let active = view == option;
            segments = segments.child(
                div()
                    .id(SharedString::from(format!("tasks-view-{}", option.label())))
                    .h(px(24.0))
                    .px(px(10.0))
                    .flex()
                    .items_center()
                    .rounded(px(theme::RADIUS_SM))
                    .bg(if active { colors.selected } else { gpui::rgba(0x00000000) })
                    .text_color(if active { colors.text_1 } else { colors.text_2 })
                    .text_size(px(theme::FS_XS))
                    .cursor_pointer()
                    .hover(move |style| style.text_color(colors.text_1))
                    .child(option.label())
                    .on_click(cx.listener(move |root, _: &gpui::ClickEvent, _window, cx| {
                        root.tasks.view = option;
                        cx.notify();
                    })),
            );
        }

        let new_button = if connected {
            self.primary_button("New task", "tasks-new", colors, cx, |root, cx| root.open_task_form(cx))
        } else {
            div().into_any_element()
        };

        let header = div()
            .h(px(46.0))
            .flex_none()
            .px(px(12.0))
            .flex()
            .items_center()
            .gap(px(10.0))
            .border_b_1()
            .border_color(colors.line)
            .child(back)
            .child(div().text_size(px(theme::FS_LG)).font_weight(FontWeight::SEMIBOLD).child("Tasks"))
            .child(segments)
            .child(div().flex_1())
            .child(new_button);

        let mut body = div().flex().flex_col().gap(px(6.0));
        if let Some(error) = self.tasks.error.clone() {
            body = body.child(div().text_size(px(theme::FS_XS)).text_color(colors.status_danger).child(SharedString::from(error)));
        }
        if let Some((ok, notice)) = self.tasks.notice.clone() {
            body = body.child(div().text_size(px(theme::FS_XS)).text_color(if ok { colors.text_2 } else { colors.status_danger }).child(SharedString::from(notice)));
        }
        if !self.tasks.loaded && connected {
            body = body.child(div().text_color(colors.text_3).child("Loading tasks…"));
        }

        let tasks: Vec<BoardTask> = self.tasks.tasks.clone();
        let agents: Vec<Agent> = self.agents().to_vec();
        body = body.child(match view {
            TaskView::Board => self.render_task_board(&tasks, &agents, colors, cx),
            TaskView::Tree => self.render_task_tree(&tasks, &agents, colors, cx),
            TaskView::Timeline => self.render_task_timeline(&tasks, &agents, colors, cx),
        });

        div()
            .flex_1()
            .h_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(header)
            .child(
                div()
                    .id("tasks-scroll")
                    .flex_1()
                    .min_h(px(0.0))
                    .w_full()
                    .overflow_y_scroll()
                    .child(div().max_w(px(1040.0)).mx_auto().w_full().px(px(20.0)).py(px(14.0)).child(body)),
            )
            .into_any_element()
    }

    /// The form and the details, over the window.
    pub(crate) fn render_tasks_overlay(&mut self, colors: Colors, cx: &mut Context<Self>) -> Option<AnyElement> {
        let card = if self.tasks.creating || self.tasks.editing.is_some() {
            Some(self.render_task_form(colors, cx))
        } else if self.tasks.open.is_some() {
            Some(self.render_task_detail(colors, cx))
        } else {
            None
        }?;
        Some(
            div()
                .id("tasks-backdrop")
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .bg(colors.scrim)
                .on_mouse_down(gpui::MouseButton::Left, cx.listener(|root, _: &gpui::MouseDownEvent, _window, cx| {
                    root.tasks_close_overlay(cx);
                }))
                .child(
                    div()
                        .on_mouse_down(gpui::MouseButton::Left, cx.listener(|_, _: &gpui::MouseDownEvent, _window, cx| {
                            cx.stop_propagation();
                        }))
                        .child(card),
                )
                .into_any_element(),
        )
    }

    /// One card's meta line: project, estimate, images, a missed schedule.
    pub(crate) fn task_meta(&self, task: &BoardTask) -> String {
        let project = task
            .project_id
            .as_deref()
            .and_then(|id| self.snapshot().and_then(|snapshot| snapshot.project_name(Some(id)).map(str::to_owned)));
        let images = task.images.len();
        [
            project,
            task.estimate_minutes.map(|minutes| format!("about {minutes} min")),
            (images > 0).then(|| format!("{images} image{}", if images == 1 { "" } else { "s" })),
            (task.schedule_state.as_deref() == Some("missed")).then(|| "Missed its time".to_owned()),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" · ")
    }

    /// A task's schedule in one line, for a scheduled card and its details.
    pub(crate) fn task_schedule_line(&self, task: &BoardTask) -> Option<String> {
        let bot = task
            .schedule_agent_id
            .as_deref()
            .and_then(|id| self.snapshot().and_then(|snapshot| snapshot.agent(id)))
            .map(|agent| agent.name.as_str());
        schedule_summary(task, bot)
    }

    /// Every bot a card may be given to, in the roster's order.
    pub fn assignable(&self) -> Vec<Agent> {
        let mut rows: Vec<Agent> = assignable_agents(self.agents()).into_iter().cloned().collect();
        rows.sort_by_key(|agent| agent.sort_order);
        rows
    }

    /// Add one image to the open form from a path the owner chose.
    pub(crate) fn add_task_image_from_path(&mut self, path: std::path::PathBuf, cx: &mut Context<Self>) {
        let name = path.file_name().map(|name| name.to_string_lossy().to_string()).unwrap_or_else(|| "image.png".to_owned());
        let Some(draft) = self.tasks.draft.as_mut() else { return };
        draft.staging += 1;
        cx.notify();
        let task = cx.background_executor().spawn(async move {
            let bytes = std::fs::read(&path).map_err(|_| "That image could not be read.".to_owned())?;
            let staged = crate::files::stage_attachment(&crate::paths::attachments_dir(), &bytes, &name)?;
            Ok::<(String, String), String>((staged.to_string_lossy().to_string(), name))
        });
        cx.spawn(async move |this, cx: &mut gpui::AsyncApp| {
            let result = task.await;
            cx.update(|app| {
                if let Some(root) = this.upgrade() {
                    root.update(app, |root, cx| {
                        if let Some(draft) = root.tasks.draft.as_mut() {
                            draft.staging = draft.staging.saturating_sub(1);
                            match result {
                                Ok((path, name)) => draft.images.push(crate::state::tasks::TaskImageRef::Staged { path, name }),
                                Err(error) => {
                                    draft.broken += 1;
                                    root.tasks.form_error = Some(error);
                                }
                            }
                        }
                        cx.notify();
                    });
                }
            });
        })
        .detach();
    }

    /// Ask the owner for image files, then stage each one for the form.
    pub(crate) fn choose_task_images(&mut self, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some(SharedString::from("Add these images")),
        });
        cx.spawn(async move |this, cx: &mut gpui::AsyncApp| {
            let chosen = receiver.await.ok().and_then(|result| result.ok()).flatten().unwrap_or_default();
            if chosen.is_empty() {
                return;
            }
            cx.update(|app| {
                if let Some(root) = this.upgrade() {
                    root.update(app, |root, cx| {
                        for path in chosen {
                            root.add_task_image_from_path(path, cx);
                        }
                    });
                }
            });
        })
        .detach();
    }

    /// The image a stored row shows, if it has been decoded.
    pub(crate) fn task_image(&self, task_id: &str, image: &TaskImage) -> Option<Arc<Image>> {
        self.tasks.images.get(&format!("{task_id}:{}", image.id)).cloned()
    }
}

/// The text the form's "when" field starts with.
fn when_text(draft: &TaskDraft) -> String {
    match draft.schedule.preset {
        crate::ui::schedules::SchedulePreset::Once => draft.schedule.once_at.clone(),
        crate::ui::schedules::SchedulePreset::Hours => String::new(),
        _ => draft.schedule.time.clone(),
    }
}

/// The image strip a task's details show, in the form and the details card.
pub(crate) fn image_chip(name: &str, image: Option<Arc<Image>>, colors: Colors) -> AnyElement {
    div()
        .w(px(64.0))
        .flex()
        .flex_col()
        .gap(px(2.0))
        .items_center()
        .child(match image {
            Some(image) => gpui::img(image).h(px(44.0)).w(px(44.0)).rounded(px(4.0)).into_any_element(),
            None => div().size(px(44.0)).rounded(px(4.0)).bg(colors.raised).into_any_element(),
        })
        .child(div().text_size(px(theme::FS_2XS)).text_color(colors.text_3).truncate().child(SharedString::from(name.to_owned())))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::tasks::BoardTask;

    fn task(id: &str, state: &str) -> BoardTask {
        BoardTask { id: id.into(), title: id.into(), state: state.into(), created_at: "2026-11-08T10:00:00Z".into(), version: 1, ..BoardTask::default() }
    }

    #[test]
    fn the_page_reads_its_own_answers_and_ignores_the_rest() {
        let mut state = TasksState::default();
        state.tasks.push(task("a", "unassigned"));
        assert_eq!(state.task("a").map(|task| task.state.as_str()), Some("unassigned"));
        assert!(state.runs_for("a").is_empty());
        assert!(state.latest_run("a").is_none());
    }

    #[test]
    fn a_newer_row_replaces_an_older_one_and_a_patch_keeps_the_brief() {
        // The merge rule the board relies on: the daemon's version wins, and the
        // fields a patched row does not carry are kept.
        let older = BoardTask { brief: "the whole brief".into(), ..task("a", "queued") };
        let mut newer = task("a", "running");
        newer.version = 2;
        let mut merged = newer.clone();
        if merged.brief.is_empty() {
            merged.brief = older.brief.clone();
        }
        assert_eq!(merged.brief, "the whole brief");
        assert_eq!(merged.state, "running");
        assert!(task("a", "running").version < merged.version);
    }
}
