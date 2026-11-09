//! The task board's model, ported from `ui/taskModel.ts` and `ui/useTasks.ts`.
//!
//! The daemon owns every task state; this module only groups, orders and words
//! what it reported. Nothing here decides anything: the assignee, the queue and
//! the claim all come from `task.*` answers.

use serde::Deserialize;
use serde_json::{json, Value};

/// The states a task can be in (`TaskState` in `types.ts`).
pub const TASK_STATES: [&str; 8] = [
    "unassigned",
    "queued",
    "running",
    "needs_you",
    "awaiting_review",
    "done",
    "failed",
    "cancelled",
];

pub fn task_state_label(state: &str) -> &str {
    match state {
        "claimed" => "Starting",
        "ambiguous" => "Needs a look",
        "unassigned" => "Draft",
        "queued" => "Queued",
        "running" => "Working",
        "needs_you" => "Needs you",
        "awaiting_review" => "Ready for review",
        "done" => "Done",
        "failed" => "Failed",
        "cancelled" => "Cancelled",
        other => other,
    }
}

/// One image the daemon keeps for a task (`BoardTaskImage`).
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct TaskImage {
    pub id: String,
    pub name: String,
    pub media_type: Option<String>,
    pub bytes: Option<i64>,
}

/// The schedule a task carries: a cron expression in a zone, or one instant.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct TaskSchedule {
    /// `cron` or `once`.
    pub kind: String,
    pub cron: Option<String>,
    pub timezone: Option<String>,
    pub at: Option<String>,
}

/// One card on the board (`BoardTask` in `types.ts`). Every list is guarded so a
/// `null` where a list is expected cannot throw the whole answer away.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct BoardTask {
    pub id: String,
    pub title: String,
    pub brief: String,
    pub done_goal: String,
    pub report_back: String,
    pub project_id: Option<String>,
    pub source_url: Option<String>,
    pub origin_session_id: Option<String>,
    pub parent_task_id: Option<String>,
    pub root_task_id: Option<String>,
    pub assignee_agent_id: Option<String>,
    pub estimate_minutes: Option<i64>,
    pub max_runtime_secs: Option<i64>,
    pub state: String,
    pub queue_position: Option<i64>,
    pub version: i64,
    pub created_at: String,
    pub updated_at: String,
    pub assigned_at: Option<String>,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    #[serde(default, deserialize_with = "crate::state::serde_defaults::null_as_default")]
    pub images: Vec<TaskImage>,
    pub schedule: Option<TaskSchedule>,
    pub schedule_agent_id: Option<String>,
    pub schedule_state: Option<String>,
    pub schedule_next_at: Option<String>,
    pub schedule_last_at: Option<String>,
    pub schedule_last_outcome: Option<String>,
    /// A run that started without the owner present, so it kept the unattended
    /// rails (never Full access).
    pub unattended: bool,
    /// UI only: the owner dropped this card on a bot and the daemon's answer has
    /// not arrived yet. Never sent by the daemon.
    #[serde(skip)]
    pub pending_dispatch: bool,
}

impl BoardTask {
    /// What the card says: a just-dropped card says "Starting" until the
    /// daemon's own row replaces it.
    pub fn state_label(&self) -> &str {
        if self.pending_dispatch {
            return "Starting";
        }
        task_state_label(&self.state)
    }

    pub fn is_finished(&self) -> bool {
        is_finished(&self.state)
    }

    pub fn is_on_lane(&self) -> bool {
        self.assignee_agent_id.is_some() && !self.is_finished() && self.state != "unassigned"
    }

    /// A task a schedule will start (once) or copy (repeat).
    pub fn is_scheduled(&self) -> bool {
        self.state == "unassigned" && self.schedule_state.as_deref() == Some("active") && self.schedule.is_some()
    }


}

pub fn is_finished(state: &str) -> bool {
    matches!(state, "done" | "failed" | "cancelled")
}

/// One attempt at a task (`BoardTaskRun`).
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct BoardTaskRun {
    pub id: String,
    pub task_id: String,
    pub attempt: i64,
    pub session_id: Option<String>,
    pub assignee_agent_id: Option<String>,
    pub assignee_agent_name: Option<String>,
    pub state: String,
    pub started_at: Option<String>,
    pub ended_at: Option<String>,
}

/// `TaskDispatchResult`: what happened when the owner gave a card to a bot.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct TaskDispatchResult {
    pub task: Option<BoardTask>,
    pub run: Option<BoardTaskRun>,
    /// `started` or `queued`.
    pub disposition: String,
    pub queue_position: Option<i64>,
    /// `bot_busy`, `global_limit` or `global_pause`.
    pub reason: Option<String>,
    pub replayed: bool,
}

/// Why a queued card is waiting, in the daemon's own words.
pub fn dispatch_message(disposition: &str, agent: &str, position: Option<i64>, reason: Option<&str>) -> String {
    if disposition == "started" {
        return format!("Started on {agent}.");
    }
    let why = match reason {
        Some("global_pause") => "Tasks are paused",
        Some("global_limit") => "The task limit is reached",
        _ => "The bot is busy",
    };
    match position {
        Some(position) => format!("{why}. Queued at position {} for {agent}.", position + 1),
        None => format!("{why}. Queued for {agent}."),
    }
}

/// "Ran out of work": a key the daemon will not replay a second time.
pub fn new_idempotency_key(task_id: &str) -> String {
    format!("{}:{}", task_id, uuid::Uuid::new_v4())
}

/// The order a lane sorts by: attention first, then work, then the queue in order.
fn lane_order(state: &str) -> i64 {
    match state {
        "needs_you" => 0,
        "awaiting_review" => 1,
        "running" => 2,
        "queued" => 3,
        _ => 9,
    }
}

/// The cards on one bot's lane: assigned, not finished, in lane order.
pub fn lane_tasks(tasks: &[BoardTask], agent_id: &str) -> Vec<BoardTask> {
    let mut rows: Vec<BoardTask> = tasks
        .iter()
        .filter(|task| task.assignee_agent_id.as_deref() == Some(agent_id) && task.is_on_lane())
        .cloned()
        .collect();
    rows.sort_by(|a, b| {
        lane_order(&a.state)
            .cmp(&lane_order(&b.state))
            .then_with(|| a.queue_position.unwrap_or(0).cmp(&b.queue_position.unwrap_or(0)))
            .then_with(|| a.created_at.cmp(&b.created_at))
            .then_with(|| a.id.cmp(&b.id))
    });
    rows
}

/// The unassigned drafts, newest first, without the ones a schedule owns.
pub fn draft_tasks(tasks: &[BoardTask]) -> Vec<BoardTask> {
    let mut rows: Vec<BoardTask> = tasks
        .iter()
        .filter(|task| task.state == "unassigned" && !task.is_scheduled())
        .cloned()
        .collect();
    rows.sort_by(|a, b| b.created_at.cmp(&a.created_at).then_with(|| a.id.cmp(&b.id)));
    rows
}

/// The tasks a schedule will start, soonest first.
pub fn scheduled_tasks(tasks: &[BoardTask]) -> Vec<BoardTask> {
    let mut rows: Vec<BoardTask> = tasks.iter().filter(|task| task.is_scheduled()).cloned().collect();
    rows.sort_by(|a, b| a.schedule_next_at.clone().unwrap_or_default().cmp(&b.schedule_next_at.clone().unwrap_or_default()));
    rows
}

/// The finished cards, most recently finished first.
pub fn finished_tasks(tasks: &[BoardTask]) -> Vec<BoardTask> {
    let mut rows: Vec<BoardTask> = tasks.iter().filter(|task| task.is_finished()).cloned().collect();
    rows.sort_by(|a, b| {
        let left = a.completed_at.clone().unwrap_or_else(|| a.updated_at.clone());
        let right = b.completed_at.clone().unwrap_or_else(|| b.updated_at.clone());
        right.cmp(&left).then_with(|| a.id.cmp(&b.id))
    });
    rows
}

/// The board's columns, in the daemon's state order, each with its cards.
pub fn state_columns(tasks: &[BoardTask]) -> Vec<(&'static str, Vec<BoardTask>)> {
    TASK_STATES
        .iter()
        .map(|state| {
            let mut rows: Vec<BoardTask> = tasks.iter().filter(|task| task.state == *state).cloned().collect();
            rows.sort_by(|a, b| a.created_at.cmp(&b.created_at).then_with(|| a.id.cmp(&b.id)));
            (*state, rows)
        })
        .filter(|(_, rows)| !rows.is_empty())
        .collect()
}

/// `SCHEDULE_OUTCOME_LABEL`: what the daemon recorded about a scheduled run.
pub fn schedule_outcome_label(outcome: &str) -> &str {
    match outcome {
        "started" => "Started",
        "queued" => "Queued behind other work",
        "skipped_late" => "Skipped: more than five minutes late",
        "skipped_paused" => "Skipped: tasks were paused",
        "skipped_overlap" => "Skipped: the last run was still going",
        "failed_to_start" => "Could not start",
        "invalid_schedule" => "The schedule is not valid",
        other => other,
    }
}

/// Shown under every time control: the daemon skips a missed run rather than
/// running it late (`SCHEDULE_GRACE_SECS`).
pub const SCHEDULE_DISCLAIMER: &str =
    "Runs only while Retinue is open and this computer is on and awake. A run that is missed by more than five minutes is skipped, not run late.";

/// The time limits the form offers, as the React form does.
pub const TIME_LIMIT_CHOICES: [(i64, &str); 7] = [
    (900, "15 minutes"),
    (1800, "30 minutes"),
    (3600, "1 hour"),
    (7200, "2 hours"),
    (14400, "4 hours"),
    (28800, "8 hours"),
    (86400, "24 hours"),
];

/// One line for a scheduled card: "Every day at 09:00 · Codex".
pub fn schedule_summary(task: &BoardTask, bot_name: Option<&str>) -> Option<String> {
    let schedule = task.schedule.as_ref()?;
    let described = crate::ui::schedules::describe_schedule(&crate::ui::schedules::DaemonSchedule::from_task(schedule));
    Some([Some(described), bot_name.map(str::to_owned)].into_iter().flatten().collect::<Vec<_>>().join(" · "))
}

/// How a lane is doing, in one word and a tone (`laneAvailability`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Availability {
    /// `free`, `working` or `attention`.
    pub tone: &'static str,
    pub label: String,
    pub queued: usize,
}

pub fn lane_availability(lane: &[BoardTask], now_ms: i64) -> Availability {
    let attention = lane
        .iter()
        .filter(|task| task.state == "needs_you" || task.state == "awaiting_review")
        .count();
    let running = lane.iter().find(|task| task.state == "running");
    let queued = lane.iter().filter(|task| task.state == "queued").count();
    if let Some(running) = running {
        let label = match remaining_minutes(running, now_ms) {
            Some(left) => format!("Busy · {left} min left"),
            None => "Working".to_owned(),
        };
        return Availability { tone: "working", label, queued };
    }
    if attention > 0 {
        return Availability {
            tone: "attention",
            label: if attention == 1 { "Waiting for you".to_owned() } else { format!("{attention} waiting for you") },
            queued,
        };
    }
    Availability { tone: "free", label: "Free".to_owned(), queued }
}

/// Remaining minutes against the owner's own estimate; `None` when either the
/// estimate or the observed start is missing.
pub fn remaining_minutes(task: &BoardTask, now_ms: i64) -> Option<i64> {
    let estimate = task.estimate_minutes?;
    let started = parse_time(task.started_at.as_deref())?;
    Some((estimate - (now_ms - started) / 60_000).max(0))
}

/// "under a minute", "12 min", "1 h 20 min" from two observed instants.
pub fn elapsed_label(from: Option<&str>, to: Option<&str>, now_ms: i64) -> Option<String> {
    let start = parse_time(from)?;
    let end = match to {
        Some(value) => parse_time(Some(value))?,
        None => now_ms,
    };
    if end < start {
        return None;
    }
    let minutes = (end - start) / 60_000;
    Some(duration_text(minutes))
}

pub fn duration_text(minutes: i64) -> String {
    if minutes < 1 {
        return "under a minute".to_owned();
    }
    if minutes < 60 {
        return format!("{minutes} min");
    }
    let hours = minutes / 60;
    if minutes % 60 == 0 {
        format!("{hours} h")
    } else {
        format!("{hours} h {} min", minutes % 60)
    }
}

/// The queue is numbered from 0 on the wire; the board shows it from 1.
pub fn queue_label(position: Option<i64>) -> Option<i64> {
    position.map(|value| value + 1)
}

/// A stored instant as milliseconds since the epoch, in UTC.
pub fn parse_time(value: Option<&str>) -> Option<i64> {
    let value = value?.trim();
    if value.is_empty() {
        return None;
    }
    chrono::DateTime::parse_from_rfc3339(value).ok().map(|at| at.timestamp_millis())
}

/// A stored instant as people read it: "Nov 8, 14:32", or empty when the
/// daemon did not record one.
pub fn when_text(value: Option<&str>) -> String {
    let Some(text) = value else { return String::new() };
    match chrono::DateTime::parse_from_rfc3339(text.trim()) {
        Ok(at) => at.with_timezone(&chrono::Local).format("%b %-d, %H:%M").to_string(),
        Err(_) => String::new(),
    }
}

/// The bots a card may be given to: everyone not archived and not hidden.
pub fn assignable_agents(agents: &[crate::state::Agent]) -> Vec<&crate::state::Agent> {
    agents.iter().filter(|agent| !agent.archived && !agent.hidden).collect()
}

/// One image on a task being written: a kept one by its stored id, or a new one
/// staged on disk by the window.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TaskImageRef {
    Stored(String),
    Staged { path: String, name: String },
}

impl TaskImageRef {
    pub fn to_json(&self) -> Value {
        match self {
            TaskImageRef::Stored(id) => json!({ "id": id }),
            TaskImageRef::Staged { path, name } => json!({ "path": path, "name": name }),
        }
    }
}

/// What the form sends: `TaskInput` in `useTasks.ts`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TaskInput {
    pub title: String,
    pub brief: String,
    pub done_goal: String,
    pub report_back: String,
    pub project_id: Option<String>,
    pub estimate_minutes: Option<i64>,
    pub max_runtime_secs: i64,
    pub attachments: Vec<TaskImageRef>,
    /// `None` is "now": the task runs when it is given to a bot.
    pub schedule: Option<crate::ui::schedules::DaemonSchedule>,
    pub schedule_agent_id: Option<String>,
}

impl TaskInput {
    pub fn params(&self) -> Value {
        json!({
            "title": self.title,
            "brief": self.brief,
            "doneGoal": self.done_goal,
            "reportBack": self.report_back,
            "projectId": self.project_id,
            "estimateMinutes": self.estimate_minutes,
            "maxRuntimeSecs": self.max_runtime_secs,
            "attachments": self.attachments.iter().map(TaskImageRef::to_json).collect::<Vec<_>>(),
            "schedule": self.schedule.as_ref().map(crate::ui::schedules::DaemonSchedule::to_json),
            "scheduleAgentId": self.schedule_agent_id,
        })
    }
}

/// The form's fields, as the window holds them while they are being typed.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TaskDraft {
    pub title: String,
    pub brief: String,
    pub done_goal: String,
    pub report_back: String,
    pub project_id: String,
    pub bot_id: String,
    /// `now`, `once` or `repeat`.
    pub when: String,
    pub schedule: crate::ui::schedules::ScheduleDraft,
    pub limit_secs: i64,
    pub estimate: String,
    pub images: Vec<TaskImageRef>,
    /// Images still being written to the staging folder.
    pub staging: usize,
    /// Images that could not be attached.
    pub broken: usize,
}

/// Why the form will not save, or `None` when it will.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskFormProblem {
    NoTitle,
    TitleTooLong,
    BriefTooLong,
    BadEstimate,
    NeedsBot,
    NeedsTime,
    ImagesStaging,
    ImageBroken,
}

impl TaskFormProblem {
    pub fn text(self) -> &'static str {
        match self {
            TaskFormProblem::NoTitle => "Give the task a title.",
            TaskFormProblem::TitleTooLong => "Keep the title to 120 characters.",
            TaskFormProblem::BriefTooLong => "Keep the brief to 12,000 characters.",
            TaskFormProblem::BadEstimate => "The estimate is a whole number of minutes.",
            TaskFormProblem::NeedsBot => "Choose the bot that runs this task.",
            TaskFormProblem::NeedsTime => "Choose a valid time.",
            TaskFormProblem::ImagesStaging => "Preparing your images…",
            TaskFormProblem::ImageBroken => "Remove the image that could not be attached.",
        }
    }
}

/// The form's own rules, in the order the React form asks them: the title first,
/// then the images, then the bot and the time, then the estimate.
pub fn form_problem(draft: &TaskDraft) -> Option<TaskFormProblem> {
    let title = draft.title.trim();
    if title.is_empty() {
        return Some(TaskFormProblem::NoTitle);
    }
    if title.chars().count() > 120 {
        return Some(TaskFormProblem::TitleTooLong);
    }
    if draft.brief.chars().count() > 12_000 {
        return Some(TaskFormProblem::BriefTooLong);
    }
    if draft.staging > 0 {
        return Some(TaskFormProblem::ImagesStaging);
    }
    if draft.broken > 0 {
        return Some(TaskFormProblem::ImageBroken);
    }
    let timed = draft.when != "now";
    if timed && draft.bot_id.trim().is_empty() {
        return Some(TaskFormProblem::NeedsBot);
    }
    if timed && crate::ui::schedules::draft_to_schedule(&draft.schedule).is_none() {
        return Some(TaskFormProblem::NeedsTime);
    }
    if let Some(problem) = estimate_problem(&draft.estimate) {
        return Some(problem);
    }
    None
}

pub fn estimate_problem(text: &str) -> Option<TaskFormProblem> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    match text.parse::<i64>() {
        Ok(value) if (1..=10_080).contains(&value) => None,
        _ => Some(TaskFormProblem::BadEstimate),
    }
}

impl TaskDraft {
    /// A new task's form: the daemon's own default time limit, and "now".
    pub fn new_task() -> Self {
        TaskDraft {
            when: "now".to_owned(),
            schedule: crate::ui::schedules::default_draft(&crate::ui::schedules::local_timezone()),
            limit_secs: 1800,
            images: Vec::new(),
            ..TaskDraft::default()
        }
    }

    /// The form filled from an existing draft task, as the edit dialog shows it.
    pub fn for_task(task: &BoardTask) -> Self {
        let when = match task.schedule.as_ref().map(|schedule| schedule.kind.as_str()) {
            Some("once") => "once",
            Some("cron") => "repeat",
            _ => "now",
        };
        let schedule = match task.schedule.as_ref() {
            Some(schedule) => crate::ui::schedules::schedule_to_draft(
                &crate::ui::schedules::DaemonSchedule::from_task(schedule),
                &crate::ui::schedules::local_timezone(),
            ),
            None => crate::ui::schedules::default_draft(&crate::ui::schedules::local_timezone()),
        };
        TaskDraft {
            title: task.title.clone(),
            brief: task.brief.clone(),
            done_goal: task.done_goal.clone(),
            report_back: task.report_back.clone(),
            project_id: task.project_id.clone().unwrap_or_default(),
            bot_id: task
                .schedule_agent_id
                .clone()
                .or_else(|| task.assignee_agent_id.clone())
                .unwrap_or_default(),
            when: when.to_owned(),
            schedule,
            limit_secs: task.max_runtime_secs.unwrap_or(1800),
            estimate: task.estimate_minutes.map(|value| value.to_string()).unwrap_or_default(),
            images: task.images.iter().map(|image| TaskImageRef::Stored(image.id.clone())).collect(),
            staging: 0,
            broken: 0,
        }
    }

    pub fn estimate_minutes(&self) -> Option<i64> {
        self.estimate.trim().parse::<i64>().ok()
    }

    /// The bot to hand the task to straight away: "Now" with a bot chosen.
    pub fn start_agent(&self) -> Option<String> {
        if self.when == "now" && !self.bot_id.trim().is_empty() {
            Some(self.bot_id.clone())
        } else {
            None
        }
    }

    /// What the form sends once it is valid.
    pub fn to_input(&self) -> TaskInput {
        let timed = self.when != "now";
        TaskInput {
            title: self.title.trim().to_owned(),
            brief: self.brief.trim().to_owned(),
            done_goal: self.done_goal.trim().to_owned(),
            report_back: self.report_back.trim().to_owned(),
            project_id: Some(self.project_id.clone()).filter(|id| !id.is_empty()),
            estimate_minutes: self.estimate_minutes(),
            max_runtime_secs: self.limit_secs,
            attachments: self.images.clone(),
            schedule: if timed { crate::ui::schedules::draft_to_schedule(&self.schedule) } else { None },
            schedule_agent_id: if timed { Some(self.bot_id.clone()) } else { None },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(id: &str, state: &str, created: &str) -> BoardTask {
        BoardTask { id: id.into(), title: id.into(), state: state.into(), created_at: created.into(), version: 1, ..BoardTask::default() }
    }

    #[test]
    fn drafts_are_the_unassigned_tasks_without_a_schedule() {
        let mut scheduled = task("s", "unassigned", "2026-11-01T09:00:00Z");
        scheduled.schedule = Some(TaskSchedule { kind: "cron".into(), cron: Some("0 9 * * *".into()), timezone: Some("UTC".into()), at: None });
        scheduled.schedule_state = Some("active".into());
        let tasks = vec![task("a", "unassigned", "2026-11-01T10:00:00Z"), scheduled, task("b", "running", "2026-11-01T11:00:00Z")];
        let drafts = draft_tasks(&tasks);
        assert_eq!(drafts.iter().map(|task| task.id.as_str()).collect::<Vec<_>>(), vec!["a"], "newest first, and a scheduled task is not a draft");
        assert_eq!(scheduled_tasks(&tasks).len(), 1);
    }

    #[test]
    fn a_lane_puts_attention_first_then_the_queue_in_order() {
        let mut running = task("running", "running", "2026-11-01T10:00:00Z");
        running.assignee_agent_id = Some("ada".into());
        let mut queued = task("queued", "queued", "2026-11-01T09:00:00Z");
        queued.assignee_agent_id = Some("ada".into());
        queued.queue_position = Some(1);
        let mut queued_first = task("queued-first", "queued", "2026-11-01T08:00:00Z");
        queued_first.assignee_agent_id = Some("ada".into());
        queued_first.queue_position = Some(0);
        let mut review = task("review", "awaiting_review", "2026-11-01T12:00:00Z");
        review.assignee_agent_id = Some("ada".into());
        let mut other_bot = task("someone-else", "running", "2026-11-01T07:00:00Z");
        other_bot.assignee_agent_id = Some("bob".into());
        let mut done = task("done", "done", "2026-11-01T06:00:00Z");
        done.assignee_agent_id = Some("ada".into());
        let tasks = vec![queued, running, queued_first, review, other_bot, done];
        let lane = lane_tasks(&tasks, "ada");
        assert_eq!(
            lane.iter().map(|task| task.id.as_str()).collect::<Vec<_>>(),
            vec!["review", "running", "queued-first", "queued"],
            "review first, then the running card, then the queue by position"
        );
    }

    #[test]
    fn finished_cards_are_newest_first_by_their_finish_time() {
        let mut a = task("a", "done", "2026-11-01T08:00:00Z");
        a.completed_at = Some("2026-11-01T12:00:00Z".into());
        let mut b = task("b", "failed", "2026-11-01T09:00:00Z");
        b.completed_at = Some("2026-11-01T11:00:00Z".into());
        let finished = finished_tasks(&[a, b]);
        assert_eq!(finished.iter().map(|task| task.id.as_str()).collect::<Vec<_>>(), vec!["a", "b"]);
    }

    #[test]
    fn the_board_columns_skip_the_states_with_no_cards() {
        let tasks = vec![task("a", "unassigned", "2026-11-01T08:00:00Z"), task("b", "running", "2026-11-01T09:00:00Z")];
        let columns = state_columns(&tasks);
        assert_eq!(columns.iter().map(|(state, _)| *state).collect::<Vec<_>>(), vec!["unassigned", "running"]);
        assert_eq!(columns[1].1.len(), 1);
    }

    #[test]
    fn a_lane_says_whether_it_is_free_working_or_waiting() {
        let mut review = task("review", "awaiting_review", "2026-11-01T09:00:00Z");
        review.assignee_agent_id = Some("ada".into());
        let free = lane_availability(&[], 0);
        assert_eq!((free.tone, free.label.as_str()), ("free", "Free"));
        let attention = lane_availability(&[review.clone()], 0);
        assert_eq!(attention.tone, "attention");
        assert_eq!(attention.label, "Waiting for you");
        let mut running = review;
        running.state = "running".into();
        let working = lane_availability(&[running], 0);
        assert_eq!((working.tone, working.label.as_str()), ("working", "Working"));
    }

    #[test]
    fn elapsed_time_is_read_from_the_instants_the_daemon_recorded() {
        let start = "2026-11-08T10:00:00Z";
        let end = "2026-11-08T11:20:00Z";
        assert_eq!(elapsed_label(Some(start), Some(end), 0).as_deref(), Some("1 h 20 min"));
        assert_eq!(elapsed_label(Some(start), None, parse_time(Some("2026-11-08T10:20:00Z")).unwrap()).as_deref(), Some("20 min"));
        assert_eq!(elapsed_label(Some(start), Some(start), 0).as_deref(), Some("under a minute"));
        assert_eq!(elapsed_label(None, Some(end), 0), None);
    }

    #[test]
    fn the_queue_shows_from_one() {
        assert_eq!(queue_label(Some(0)), Some(1));
        assert_eq!(queue_label(None), None);
    }

    #[test]
    fn a_dispatch_says_where_it_went() {
        assert_eq!(dispatch_message("started", "Ada", None, None), "Started on Ada.");
        assert_eq!(
            dispatch_message("queued", "Ada", Some(0), Some("global_pause")),
            "Tasks are paused. Queued at position 1 for Ada."
        );
        assert_eq!(dispatch_message("queued", "Ada", None, None), "The bot is busy. Queued for Ada.");
    }

    #[test]
    fn the_form_needs_a_title_and_a_bot_only_when_it_is_timed() {
        let mut draft = TaskDraft::new_task();
        assert_eq!(form_problem(&draft), Some(TaskFormProblem::NoTitle));
        draft.title = "Write the notes".into();
        assert_eq!(form_problem(&draft), None, "an untimed task needs no bot");
        draft.estimate = "0".into();
        assert_eq!(form_problem(&draft), Some(TaskFormProblem::BadEstimate));
        draft.estimate = "45".into();
        draft.when = "repeat".into();
        assert_eq!(form_problem(&draft), Some(TaskFormProblem::NeedsBot));
        draft.bot_id = "ada".into();
        assert_eq!(form_problem(&draft), None);
        let input = draft.to_input();
        assert_eq!(input.estimate_minutes, Some(45));
        assert!(input.schedule.is_some());
        assert_eq!(input.schedule_agent_id.as_deref(), Some("ada"));
    }

    #[test]
    fn a_timed_task_never_starts_on_save() {
        let mut draft = TaskDraft::new_task();
        draft.title = "Nightly tests".into();
        draft.bot_id = "ada".into();
        draft.when = "repeat".into();
        assert_eq!(draft.start_agent(), None, "a scheduled task waits for its time");
        draft.when = "now".into();
        assert_eq!(draft.start_agent().as_deref(), Some("ada"));
    }

    #[test]
    fn an_image_that_is_still_staging_holds_the_form() {
        let mut draft = TaskDraft::new_task();
        draft.title = "Look at this".into();
        draft.staging = 1;
        assert_eq!(form_problem(&draft), Some(TaskFormProblem::ImagesStaging));
        draft.staging = 0;
        draft.broken = 1;
        assert_eq!(form_problem(&draft), Some(TaskFormProblem::ImageBroken));
    }

    #[test]
    fn an_edited_draft_task_reopens_with_its_own_fields() {
        let mut task = task("draft", "unassigned", "2026-11-01T09:00:00Z");
        task.title = "Write the release notes".into();
        task.brief = "Summarise the release.".into();
        task.max_runtime_secs = Some(3600);
        task.estimate_minutes = Some(30);
        task.images = vec![TaskImage { id: "img-1".into(), name: "shot.png".into(), media_type: None, bytes: None }];
        let draft = TaskDraft::for_task(&task);
        assert_eq!(draft.title, "Write the release notes");
        assert_eq!(draft.limit_secs, 3600);
        assert_eq!(draft.estimate, "30");
        assert_eq!(draft.images, vec![TaskImageRef::Stored("img-1".into())]);
        let input = draft.to_input();
        assert_eq!(input.attachments, vec![TaskImageRef::Stored("img-1".into())]);
        assert_eq!(input.params()["maxRuntimeSecs"], json!(3600));
    }

    #[test]
    fn a_scheduled_card_says_when_and_for_whom() {
        let mut task = task("s", "unassigned", "2026-11-01T09:00:00Z");
        task.schedule = Some(TaskSchedule { kind: "cron".into(), cron: Some("0 9 * * *".into()), timezone: Some("UTC".into()), at: None });
        task.schedule_agent_id = Some("ada".into());
        assert_eq!(schedule_summary(&task, Some("Ada")).as_deref(), Some("Every day at 09:00 · Ada"));
    }
}
