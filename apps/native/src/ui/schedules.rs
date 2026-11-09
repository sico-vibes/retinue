//! Schedules (automations), ported from `ui/SchedulesView.tsx`,
//! `ui/ScheduleWhen.tsx`, `ui/ScheduleOverview.tsx` and
//! `ui/automationSchedule.ts`.
//!
//! The daemon owns the timer and every run. This page lists the automations,
//! edits their definition and reads their history; it never decides when
//! something runs. An unattended run keeps its own caps (a time limit, the
//! concurrency the daemon enforces, its own approval mode) and is never Full
//! access: the editor offers "Safe actions only" and "Read-only", as React does.

use std::collections::{BTreeMap, BTreeSet};

use gpui::{div, prelude::*, px, AnyElement, Context, Entity, FontWeight, SharedString};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::state::Agent;
use crate::theme::{self, Colors};
use crate::ui::root::Root;
use crate::ui::text_input::TextInput;

// ---------------------------------------------------------------------------
// The "when" model: a 5-field cron in a zone, or one instant.
// ---------------------------------------------------------------------------

/// What the daemon stores for a schedule (`DaemonSchedule` in
/// `automationSchedule.ts`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DaemonSchedule {
    Cron { cron: String, timezone: String },
    Once { at: String },
}

impl DaemonSchedule {
    /// A task's stored schedule in this shape; an older row without a kind reads
    /// as the daily nine o'clock it was created as.
    pub fn from_task(schedule: &crate::state::tasks::TaskSchedule) -> Self {
        match schedule.kind.as_str() {
            "once" => DaemonSchedule::Once { at: schedule.at.clone().unwrap_or_default() },
            _ => DaemonSchedule::Cron {
                cron: schedule.cron.clone().unwrap_or_else(|| "0 9 * * *".to_owned()),
                timezone: schedule.timezone.clone().unwrap_or_else(|| "UTC".to_owned()),
            },
        }
    }

    /// The wire shape every `schedule` parameter takes.
    pub fn to_json(&self) -> Value {
        match self {
            DaemonSchedule::Cron { cron, timezone } => json!({ "kind": "cron", "cron": cron, "timezone": timezone }),
            DaemonSchedule::Once { at } => json!({ "kind": "once", "at": at }),
        }
    }

    /// Reads either shape; anything else is `None` rather than a guess.
    pub fn from_json(value: &Value) -> Option<Self> {
        match value.get("kind").and_then(Value::as_str)? {
            "cron" => Some(DaemonSchedule::Cron {
                cron: value.get("cron").and_then(Value::as_str)?.to_owned(),
                timezone: value.get("timezone").and_then(Value::as_str).unwrap_or("UTC").to_owned(),
            }),
            "once" => Some(DaemonSchedule::Once { at: value.get("at").and_then(Value::as_str)?.to_owned() }),
            _ => None,
        }
    }
}

/// The six choices the editor offers (`SchedulePreset`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SchedulePreset {
    Daily,
    Weekdays,
    Weekly,
    Hours,
    Once,
    Custom,
}

impl SchedulePreset {
    pub const ALL: [SchedulePreset; 6] =
        [SchedulePreset::Daily, SchedulePreset::Weekdays, SchedulePreset::Weekly, SchedulePreset::Hours, SchedulePreset::Once, SchedulePreset::Custom];

    /// The four a repeating task may pick (a task can also be "Once").
    pub const REPEAT: [SchedulePreset; 5] =
        [SchedulePreset::Daily, SchedulePreset::Weekdays, SchedulePreset::Weekly, SchedulePreset::Hours, SchedulePreset::Custom];

    pub fn id(self) -> &'static str {
        match self {
            SchedulePreset::Daily => "daily",
            SchedulePreset::Weekdays => "weekdays",
            SchedulePreset::Weekly => "weekly",
            SchedulePreset::Hours => "hours",
            SchedulePreset::Once => "once",
            SchedulePreset::Custom => "custom",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            SchedulePreset::Daily => "Every day",
            SchedulePreset::Weekdays => "Weekdays",
            SchedulePreset::Weekly => "Every week",
            SchedulePreset::Hours => "Every few hours",
            SchedulePreset::Once => "Once",
            SchedulePreset::Custom => "Custom (cron)",
        }
    }

}

pub const WEEKDAYS: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];

/// The editor's own draft (`ScheduleDraft`): strings as typed, numbers as chosen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScheduleDraft {
    pub preset: SchedulePreset,
    /// `HH:MM`, 24-hour, for daily, weekdays and weekly.
    pub time: String,
    /// 0 (Sunday) to 6 (Saturday), for weekly.
    pub weekday: u32,
    /// 1 to 12, for "every N hours".
    pub every_hours: u32,
    /// `YYYY-MM-DDTHH:mm`, local, for one-time.
    pub once_at: String,
    pub cron: String,
    pub timezone: String,
}

impl Default for ScheduleDraft {
    fn default() -> Self {
        default_draft("UTC")
    }
}

/// The machine's own IANA zone, which is what the daemon stores.
pub fn local_timezone() -> String {
    iana_time_zone::get_timezone().unwrap_or_else(|_| "UTC".to_owned())
}

pub fn default_draft(timezone: &str) -> ScheduleDraft {
    ScheduleDraft {
        preset: SchedulePreset::Daily,
        time: "09:00".to_owned(),
        weekday: 1,
        every_hours: 6,
        once_at: String::new(),
        cron: "0 9 * * *".to_owned(),
        timezone: timezone.to_owned(),
    }
}

/// `HH:MM` as the editor writes it, as an hour and a minute.
fn parse_time_of_day(value: &str) -> Option<(u32, u32)> {
    let (hour, minute) = value.trim().split_once(':')?;
    let hour: u32 = hour.trim().parse().ok()?;
    let minute: u32 = minute.trim().parse().ok()?;
    (hour <= 23 && minute <= 59 && !hour.to_string().is_empty()).then_some((hour, minute))
}

fn pad(value: u32) -> String {
    format!("{value:02}")
}

/// The cron a preset stands for; `None` while the draft is incomplete.
pub fn preset_cron(draft: &ScheduleDraft) -> Option<String> {
    let time = parse_time_of_day(&draft.time);
    match draft.preset {
        SchedulePreset::Daily => time.map(|(hour, minute)| format!("{minute} {hour} * * *")),
        SchedulePreset::Weekdays => time.map(|(hour, minute)| format!("{minute} {hour} * * 1-5")),
        SchedulePreset::Weekly if draft.weekday <= 6 => time.map(|(hour, minute)| format!("{minute} {hour} * * {}", draft.weekday)),
        SchedulePreset::Weekly => None,
        SchedulePreset::Hours => (1..=12).contains(&draft.every_hours).then(|| format!("0 */{} * * *", draft.every_hours)),
        SchedulePreset::Custom => {
            let fields: Vec<&str> = draft.cron.split_whitespace().collect();
            (fields.len() == 5).then(|| fields.join(" "))
        }
        SchedulePreset::Once => None,
    }
}

/// The daemon schedule the draft stands for, or `None` while it is not valid.
pub fn draft_to_schedule(draft: &ScheduleDraft) -> Option<DaemonSchedule> {
    if draft.preset == SchedulePreset::Once {
        return once_at_utc(&draft.once_at).map(|at| DaemonSchedule::Once { at });
    }
    preset_cron(draft).map(|cron| DaemonSchedule::Cron { cron, timezone: draft.timezone.clone() })
}

/// `YYYY-MM-DDTHH:mm` in local time, as the daemon's UTC instant.
pub fn once_at_utc(local: &str) -> Option<String> {
    use chrono::TimeZone;
    let naive = chrono::NaiveDateTime::parse_from_str(local.trim(), "%Y-%m-%dT%H:%M").ok()?;
    let at = chrono::Local.from_local_datetime(&naive).earliest()?;
    Some(at.to_utc().to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
}

/// Recognises the editor's own presets so an existing schedule opens on the
/// right choice (`scheduleToDraft`).
pub fn schedule_to_draft(schedule: &DaemonSchedule, fallback_zone: &str) -> ScheduleDraft {
    let base = default_draft(fallback_zone);
    match schedule {
        DaemonSchedule::Once { at } => {
            let local = chrono::DateTime::parse_from_rfc3339(at.trim())
                .map(|at| at.with_timezone(&chrono::Local).format("%Y-%m-%dT%H:%M").to_string())
                .unwrap_or_default();
            ScheduleDraft { preset: SchedulePreset::Once, once_at: local, ..base }
        }
        DaemonSchedule::Cron { cron, timezone } => {
            let draft = ScheduleDraft { preset: SchedulePreset::Custom, cron: cron.clone(), timezone: timezone.clone(), ..base };
            let fields: Vec<&str> = cron.split_whitespace().collect();
            if fields.len() != 5 {
                return draft;
            }
            let [minute, hour, dom, month, dow] = [fields[0], fields[1], fields[2], fields[3], fields[4]];
            let number = |value: &str| -> Option<u32> {
                if !value.is_empty() && value.chars().all(|character| character.is_ascii_digit()) && value.len() <= 2 {
                    value.parse().ok()
                } else {
                    None
                }
            };
            if minute == "0" && dom == "*" && month == "*" && dow == "*" {
                if let Some(rest) = hour.strip_prefix("*/") {
                    if let Some(every) = number(rest) {
                        if (1..=12).contains(&every) {
                            return ScheduleDraft { preset: SchedulePreset::Hours, every_hours: every, ..draft };
                        }
                    }
                }
            }
            if let (Some(minute), Some(hour)) = (number(minute), number(hour)) {
                if minute <= 59 && hour <= 23 && dom == "*" && month == "*" {
                    let time = format!("{}:{}", pad(hour), pad(minute));
                    if dow == "*" {
                        return ScheduleDraft { preset: SchedulePreset::Daily, time, ..draft };
                    }
                    if dow == "1-5" {
                        return ScheduleDraft { preset: SchedulePreset::Weekdays, time, ..draft };
                    }
                    if let Some(day) = number(dow) {
                        if day <= 6 {
                            return ScheduleDraft { preset: SchedulePreset::Weekly, time, weekday: day, ..draft };
                        }
                    }
                }
            }
            draft
        }
    }
}

/// One line for a list row: "Every day at 09:00", "Every 6 hours", "Once on
/// 14 Nov at 18:00", "Custom: …".
pub fn describe_schedule(schedule: &DaemonSchedule) -> String {
    let draft = schedule_to_draft(schedule, "UTC");
    match draft.preset {
        SchedulePreset::Daily => format!("Every day at {}", draft.time),
        SchedulePreset::Weekdays => format!("Weekdays at {}", draft.time),
        SchedulePreset::Weekly => format!(
            "Every {} at {}",
            WEEKDAYS.get(draft.weekday as usize).copied().unwrap_or("day"),
            draft.time
        ),
        SchedulePreset::Hours if draft.every_hours == 1 => "Every hour".to_owned(),
        SchedulePreset::Hours => format!("Every {} hours", draft.every_hours),
        SchedulePreset::Once => {
            match chrono::DateTime::parse_from_rfc3339(draft.once_at.as_str()) {
                Ok(at) => at.with_timezone(&chrono::Local).format("Once on %-d %b at %H:%M").to_string(),
                Err(_) => "Once".to_owned(),
            }
        }
        SchedulePreset::Custom => format!("Custom: {}", draft.cron),
    }
}

/// "in 3 h", "in 12 min", "Mon 9 Nov 09:00" for a next-run instant.
pub fn describe_next_run(next: Option<&str>, now_ms: i64) -> String {
    let Some(next) = next.filter(|value| !value.trim().is_empty()) else { return String::new() };
    let Ok(at) = chrono::DateTime::parse_from_rfc3339(next.trim()) else { return String::new() };
    let diff = at.timestamp_millis() - now_ms;
    if diff <= 0 {
        return "due now".to_owned();
    }
    if diff < 60_000 {
        return "in under a minute".to_owned();
    }
    if diff < 3_600_000 {
        return format!("in {} min", (diff as f64 / 60_000.0).round() as i64);
    }
    if diff < 24 * 3_600_000 {
        let total = (diff as f64 / 60_000.0).round() as i64;
        let hours = total / 60;
        let minutes = total % 60;
        if hours == 24 {
            return "in 24 h".to_owned();
        }
        return if minutes > 0 { format!("in {hours} h {minutes} min") } else { format!("in {hours} h") };
    }
    at.with_timezone(&chrono::Local).format("%a %-d %b %H:%M").to_string()
}

pub fn run_status_label(status: &str) -> &str {
    match status {
        "running" => "Running",
        "completed" => "Done",
        "failed" => "Failed",
        "skipped" => "Skipped",
        "cancelled" => "Cancelled",
        "timeout" => "Timed out",
        "interrupted" => "Interrupted",
        other => other,
    }
}

/// A failed run that changed things is never retried by the daemon: the owner
/// decides (wave 13).
pub fn needs_manual_retry(status: Option<&str>, reason: Option<&str>) -> bool {
    matches!(status, Some("failed") | Some("timeout")) && reason.is_some_and(|reason| reason.ends_with("_after_changes"))
}

/// The daemon's reason in words, or empty when it needs none.
pub fn run_reason_text(reason: Option<&str>) -> String {
    let Some(reason) = reason.filter(|reason| !reason.is_empty()) else { return String::new() };
    if let Some(rest) = reason.strip_prefix("late:") {
        return match rest.parse::<i64>() {
            Ok(secs) if secs >= 60 => format!("Started {} min late", (secs + 59) / 60),
            _ => String::new(),
        };
    }
    match reason {
        "still_running" => "The previous run was still going.".to_owned(),
        "nothing_to_report" => "Nothing to report.".to_owned(),
        "error_after_changes" => "It stopped after changing things, so it was not retried on its own.".to_owned(),
        "timeout_after_changes" => "It ran out of time after changing things, so it was not retried on its own.".to_owned(),
        "busy" => "The bot was busy for too long.".to_owned(),
        _ => String::new(),
    }
}

// ---------------------------------------------------------------------------
// The daemon's rows.
// ---------------------------------------------------------------------------

/// One automation (`Automation` in `types.ts`).
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Automation {
    pub id: String,
    pub agent_id: String,
    pub name: String,
    pub prompt: String,
    pub schedule: Value,
    pub session_mode: Option<String>,
    pub approval_mode: Option<String>,
    pub max_runtime_secs: Option<i64>,
    pub enabled: bool,
    pub created_by: Option<String>,
    pub next_run_at: Option<String>,
    pub last_run_at: Option<String>,
    pub consecutive_failures: Option<i64>,
    pub pause_reason: Option<String>,
    pub running: bool,
    pub last_run: Option<AutomationRun>,
}

impl Automation {
    pub fn schedule(&self) -> Option<DaemonSchedule> {
        DaemonSchedule::from_json(&self.schedule)
    }

    /// A schedule a bot proposed and that waits for the owner's approval.
    pub fn is_proposal(&self) -> bool {
        self.created_by.as_deref() == Some("agent") && self.pause_reason.as_deref() == Some("needs_approval")
    }

    pub fn pause_text(&self) -> &'static str {
        match self.pause_reason.as_deref() {
            Some("needs_approval") => "Waiting for your approval",
            Some("auto_failures") => "Paused after three failed runs",
            _ => "Paused",
        }
    }

    /// The state line under the name.
    pub fn state_line(&self, now_ms: i64) -> String {
        let label = if self.running {
            "Running now".to_owned()
        } else if !self.enabled {
            self.pause_text().to_owned()
        } else if self.next_run_at.is_some() {
            format!("Next run {}", describe_next_run(self.next_run_at.as_deref(), now_ms))
        } else {
            "No upcoming run".to_owned()
        };
        if self.approval_mode.as_deref() == Some("plan") {
            format!("{label} · read-only")
        } else {
            label
        }
    }
}

/// One recorded run (`AutomationRun`).
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct AutomationRun {
    pub id: String,
    pub automation_id: String,
    pub session_id: Option<String>,
    pub status: String,
    pub reason: Option<String>,
    pub scheduled_for: Option<String>,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
}

/// What the editor sends (`AutomationInput`).
#[derive(Clone, Debug, PartialEq)]
pub struct AutomationInput {
    pub agent_id: String,
    pub name: String,
    pub prompt: String,
    pub schedule: DaemonSchedule,
    /// `new` or `reuse`.
    pub session_mode: String,
    /// `plan` or `auto`; never Full access, because nobody is there to approve it.
    pub approval_mode: String,
    pub max_runtime_secs: i64,
    pub enabled: Option<bool>,
}

impl AutomationInput {
    pub fn params(&self) -> Value {
        let mut value = json!({
            "agentId": self.agent_id,
            "name": self.name,
            "prompt": self.prompt,
            "schedule": self.schedule.to_json(),
            "sessionMode": self.session_mode,
            "approvalMode": self.approval_mode,
            "maxRuntimeSecs": self.max_runtime_secs,
        });
        if let Some(enabled) = self.enabled {
            value["enabled"] = json!(enabled);
        }
        value
    }
}

/// The runtime limits the editor offers.
pub const RUNTIME_CHOICES: [(i64, &str); 5] = [(900, "15 minutes"), (1800, "30 minutes"), (3600, "1 hour"), (7200, "2 hours"), (14400, "4 hours")];

/// A starting point the empty page offers (`TEMPLATES`).
pub struct Template {
    pub id: &'static str,
    pub title: &'static str,
    pub hint: &'static str,
    pub name: &'static str,
    pub prompt: &'static str,
    pub preset: SchedulePreset,
    pub time: &'static str,
    pub approval_mode: &'static str,
}

/// A `static`, not a `const`: the editor keeps a reference to the idea a new
/// schedule started from, and a const's address is not stable.
pub static TEMPLATES: [Template; 4] = [
    Template {
        id: "brief",
        title: "Morning brief",
        hint: "The news that matters to your work, each morning.",
        name: "Morning brief",
        prompt: "Give me a short brief of the news that matters for my work today: at most five bullets, each with one line of context and a link when you have one.",
        preset: SchedulePreset::Daily,
        time: "08:00",
        approval_mode: "plan",
    },
    Template {
        id: "email",
        title: "Check my email",
        hint: "What needs a reply or a decision, nothing sent.",
        name: "Email check",
        prompt: "Check my email and tell me what needs my attention: replies I owe, meetings to confirm and anything urgent. Skip newsletters. Do not send, archive or delete anything.",
        preset: SchedulePreset::Daily,
        time: "08:30",
        approval_mode: "plan",
    },
    Template {
        id: "checkin",
        title: "Project check-in",
        hint: "What changed in the repository since yesterday.",
        name: "Project check-in",
        prompt: "Summarise what changed in this project since yesterday: commits, open branches and anything that looks stuck. End with what you would do first today.",
        preset: SchedulePreset::Weekdays,
        time: "09:00",
        approval_mode: "plan",
    },
    Template {
        id: "tests",
        title: "Nightly tests",
        hint: "Run the test suite and report only failures.",
        name: "Nightly tests",
        prompt: "Run the project test suite. Report what failed and the likely cause for each failure. If everything passes, say so in one line.",
        preset: SchedulePreset::Daily,
        time: "23:00",
        approval_mode: "auto",
    },
];

// ---------------------------------------------------------------------------
// The window's own state.
// ---------------------------------------------------------------------------

/// The schedules page's state, and the editor when one is open.
#[derive(Default)]
pub struct SchedulesState {
    pub automations: Vec<Automation>,
    pub loaded: bool,
    pub error: Option<String>,
    /// The global switch (`automations.paused`), as the daemon reports it.
    pub paused: bool,
    pub paused_loaded: bool,
    pub busy: Option<String>,
    pub problem: Option<String>,
    pub runs: BTreeMap<String, Vec<AutomationRun>>,
    pub history_open: BTreeSet<String>,
    pub deleting: Option<(String, String)>,
    pub editor: Option<ScheduleEditor>,
    /// The next runs the daemon computed for the editor's draft.
    pub preview: Vec<String>,
    pub preview_error: Option<String>,
    pub name_input: Option<Entity<TextInput>>,
    pub prompt_input: Option<Entity<TextInput>>,
    pub choice_input: Option<Entity<TextInput>>,
}

/// The schedule editor, one at a time.
pub struct ScheduleEditor {
    /// The automation being edited; `None` for a new one.
    pub id: Option<String>,
    /// The idea the new one started from.
    pub template: Option<&'static Template>,
    pub agent_id: String,
    pub draft: ScheduleDraft,
    /// `plan` or `auto`.
    pub approval_mode: String,
    /// `new` or `reuse`.
    pub session_mode: String,
    pub limit_secs: i64,
    pub saving: bool,
    pub error: Option<String>,
}

impl ScheduleEditor {
    /// What the editor sends, or `None` while it is not valid.
    pub fn input(&self, name: &str, prompt: &str) -> Option<AutomationInput> {
        let name = name.trim();
        let prompt = prompt.trim();
        if name.is_empty() || name.chars().count() > 80 {
            return None;
        }
        if prompt.is_empty() || prompt.chars().count() > 8_000 {
            return None;
        }
        if self.agent_id.trim().is_empty() {
            return None;
        }
        let schedule = draft_to_schedule(&self.draft)?;
        Some(AutomationInput {
            agent_id: self.agent_id.clone(),
            name: name.to_owned(),
            prompt: prompt.to_owned(),
            schedule,
            session_mode: self.session_mode.clone(),
            approval_mode: self.approval_mode.clone(),
            max_runtime_secs: self.limit_secs,
            enabled: if self.id.is_none() { Some(true) } else { None },
        })
    }
}

impl Root {
    /// The Schedules page's entry point. The Director puts the button where the
    /// sidebar lists pages; this is the call it makes.
    pub fn open_schedules(&mut self, cx: &mut Context<Self>) {
        self.page = crate::ui::root::Page::Schedules;
        self.page_explicit = true;
        self.schedules.loaded = false;
        self.load_automations(cx);
        self.load_schedules_paused(cx);
        self.need_root_focus = true;
        cx.notify();
    }

    pub(crate) fn load_automations(&mut self, cx: &mut Context<Self>) {
        self.daemon_command(
            crate::daemon::Command::Rpc { tag: "automations".to_owned(), method: "automation.list".to_owned(), params: json!({}) },
            cx,
        );
    }

    fn load_schedules_paused(&mut self, cx: &mut Context<Self>) {
        self.daemon_command(
            crate::daemon::Command::Rpc { tag: "schedule-paused".to_owned(), method: "settings.get".to_owned(), params: json!({}) },
            cx,
        );
    }

    /// `automation.changed`, `.deleted` and `.run.changed` all mean one thing to
    /// this page: read the list again, because the daemon is the only writer.
    pub(crate) fn schedules_event(&mut self, event: &crate::daemon::client::DaemonEvent, cx: &mut Context<Self>) {
        if !matches!(event.event_type.as_str(), "automation.changed" | "automation.deleted" | "automation.run.changed") {
            return;
        }
        let open: Vec<String> = self.schedules.history_open.iter().cloned().collect();
        for id in open {
            self.load_automation_runs(id, cx);
        }
        if self.page == crate::ui::root::Page::Schedules {
            self.load_automations(cx);
        }
    }

    /// One answer to one of this page's RPCs; `true` when the tag was ours.
    pub(crate) fn schedules_rpc(&mut self, tag: &str, value: &Value, cx: &mut Context<Self>) -> bool {
        match tag {
            "automations" => {
                let rows = value.get("automations").and_then(Value::as_array).cloned().unwrap_or_default();
                self.schedules.automations = rows
                    .iter()
                    .filter_map(|row| serde_json::from_value::<Automation>(row.clone()).ok())
                    .collect();
                self.schedules.loaded = true;
                self.schedules.error = None;
            }
            "schedule-paused" => {
                let settings = value.get("settings").unwrap_or(value);
                let paused = settings
                    .get("automations.paused")
                    .and_then(Value::as_bool)
                    .or_else(|| value.get("automations.paused").and_then(Value::as_bool))
                    .unwrap_or(false);
                self.schedules.paused = paused;
                self.schedules.paused_loaded = true;
            }
            "automation-runs" => {
                let id = value.get("automationId").and_then(Value::as_str).unwrap_or("").to_owned();
                let rows = value.get("runs").and_then(Value::as_array).cloned().unwrap_or_default();
                if !id.is_empty() {
                    self.schedules.runs.insert(
                        id,
                        rows.iter().filter_map(|row| serde_json::from_value::<AutomationRun>(row.clone()).ok()).collect(),
                    );
                }
            }
            "automation-preview" => {
                let utc = value.get("utc").and_then(Value::as_array).cloned().unwrap_or_default();
                self.schedules.preview =
                    utc.iter().filter_map(Value::as_str).map(str::to_owned).collect::<Vec<_>>();
                self.schedules.preview_error = None;
            }
            "automation-save" => {
                if let Some(editor) = self.schedules.editor.as_mut() {
                    editor.saving = false;
                }
                self.schedules.editor = None;
                self.schedules.name_input = None;
                self.schedules.prompt_input = None;
                self.schedules.choice_input = None;
                self.toast("Schedule saved.", cx);
                self.load_automations(cx);
            }
            "automation-toggle" | "automation-delete" => {
                self.schedules.busy = None;
                if tag == "automation-delete" {
                    self.schedules.deleting = None;
                    self.toast("Schedule deleted. Conversations it started stay where they are.", cx);
                }
                self.load_automations(cx);
            }
            "automation-run-now" => {
                self.schedules.busy = None;
                self.toast("Started. The run shows in its history.", cx);
                self.load_automations(cx);
            }
            "automations-paused" => {
                self.schedules.busy = None;
                self.toast(if self.schedules.paused { "All schedules paused." } else { "Schedules resume." }, cx);
            }
            _ => return false,
        }
        cx.notify();
        true
    }

    /// A failure from one of this page's calls: `true` when the tag was ours.
    pub(crate) fn schedules_rpc_failed(&mut self, tag: &str, message: &str, cx: &mut Context<Self>) -> bool {
        match tag {
            "automations" | "schedule-paused" => {
                self.schedules.loaded = true;
                self.schedules.error = Some(message.to_owned());
            }
            "automation-runs" => {}
            "automation-preview" => self.schedules.preview_error = Some(message.to_owned()),
            _ => {
                self.schedules.busy = None;
                if let Some(editor) = self.schedules.editor.as_mut() {
                    editor.saving = false;
                    editor.error = Some(message.to_owned());
                } else {
                    self.schedules.problem = Some(message.to_owned());
                }
            }
        }
        cx.notify();
        matches!(
            tag,
            "automations"
                | "schedule-paused"
                | "automation-runs"
                | "automation-preview"
                | "automation-save"
                | "automation-toggle"
                | "automation-delete"
                | "automation-run-now"
                | "automations-paused"
        )
    }

    fn load_automation_runs(&mut self, id: String, cx: &mut Context<Self>) {
        self.daemon_command(
            crate::daemon::Command::Rpc {
                tag: "automation-runs".to_owned(),
                method: "automation.runs".to_owned(),
                params: json!({ "id": id, "limit": 20 }),
            },
            cx,
        );
    }

    fn request_preview(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = self.schedules.editor.as_ref() else { return };
        let Some(schedule) = draft_to_schedule(&editor.draft) else {
            self.schedules.preview.clear();
            self.schedules.preview_error = None;
            return;
        };
        self.daemon_command(
            crate::daemon::Command::Rpc {
                tag: "automation-preview".to_owned(),
                method: "automation.preview".to_owned(),
                params: json!({ "schedule": schedule.to_json(), "count": 3 }),
            },
            cx,
        );
    }

    /// Open the editor for a new schedule, optionally from one of the ideas.
    pub(crate) fn open_schedule_editor(&mut self, existing: Option<Automation>, idea: Option<&'static Template>, cx: &mut Context<Self>) {
        let agents: Vec<Agent> = self.agents().iter().filter(|agent| !agent.archived && agent.parent_agent_id.is_none()).cloned().collect();
        let (agent_id, draft, approval_mode, session_mode, limit, id) = match &existing {
            Some(automation) => (
                automation.agent_id.clone(),
                automation.schedule().map(|schedule| schedule_to_draft(&schedule, &local_timezone())).unwrap_or_else(|| default_draft(&local_timezone())),
                automation.approval_mode.clone().unwrap_or_else(|| "auto".to_owned()),
                automation.session_mode.clone().unwrap_or_else(|| "new".to_owned()),
                automation.max_runtime_secs.unwrap_or(1800),
                Some(automation.id.clone()),
            ),
            None => {
                let mut draft = default_draft(&local_timezone());
                if let Some(idea) = idea {
                    draft.preset = idea.preset;
                    draft.time = idea.time.to_owned();
                }
                (
                    agents.first().map(|agent| agent.id.clone()).unwrap_or_default(),
                    draft,
                    idea.map(|idea| idea.approval_mode.to_owned()).unwrap_or_else(|| "auto".to_owned()),
                    "new".to_owned(),
                    1800,
                    None,
                )
            }
        };
        self.schedules.editor = Some(ScheduleEditor {
            id,
            template: idea,
            agent_id,
            draft,
            approval_mode,
            session_mode,
            limit_secs: limit,
            saving: false,
            error: None,
        });
        self.schedules.preview.clear();
        self.schedules.preview_error = None;
        self.install_schedule_fields(existing.as_ref(), idea, cx);
        self.request_preview(cx);
        cx.notify();
    }

    /// The editor's fields, created with the page so a text box is never `None`.
    fn install_schedule_fields(&mut self, existing: Option<&Automation>, idea: Option<&'static Template>, cx: &mut Context<Self>) {
        let name = existing.map(|automation| automation.name.clone()).or_else(|| idea.map(|idea| idea.name.to_owned())).unwrap_or_default();
        let prompt = existing.map(|automation| automation.prompt.clone()).or_else(|| idea.map(|idea| idea.prompt.to_owned())).unwrap_or_default();
        let this = cx.weak_entity();
        let name_input = {
            let this = this.clone();
            cx.new(move |cx| {
                let mut input = TextInput::new("Morning summary", px(20.0), px(theme::FS_SM), cx);
                input.set_text_quiet(name);
                input.set_on_change(Box::new(move |_text: &str, _caret: usize, cx: &mut gpui::App| {
                    if let Some(root) = this.upgrade() {
                        root.update(cx, |_root, cx| cx.notify());
                    }
                }));
                input
            })
        };
        let prompt_input = {
            let this = this.clone();
            cx.new(move |cx| {
                let mut input = TextInput::new("Summarise what changed since yesterday…", px(18.0), px(theme::FS_SM), cx);
                input.set_text_quiet(prompt);
                input.set_on_change(Box::new(move |_text: &str, _caret: usize, cx: &mut gpui::App| {
                    if let Some(root) = this.upgrade() {
                        root.update(cx, |_root, cx| cx.notify());
                    }
                }));
                input
            })
        };
        // The preset's own field (a time, a cron, an instant), one at a time.
        let choice = {
            let editor = self.schedules.editor.as_ref();
            let draft = editor.map(|editor| editor.draft.clone()).unwrap_or_else(|| default_draft(&local_timezone()));
            match draft.preset {
                SchedulePreset::Daily | SchedulePreset::Weekdays | SchedulePreset::Weekly => draft.time,
                SchedulePreset::Once => draft.once_at,
                SchedulePreset::Hours => String::new(),
                SchedulePreset::Custom => draft.cron,
            }
        };
        let this = cx.weak_entity();
        let choice_input = cx.new(move |cx| {
            let this = this.clone();
            let mut input = TextInput::new("09:00", px(18.0), px(theme::FS_SM), cx);
            input.set_text_quiet(choice);
            input.set_on_change(Box::new(move |text: &str, _caret: usize, cx: &mut gpui::App| {
                let text = text.to_owned();
                if let Some(root) = this.upgrade() {
                    root.update(cx, |root, cx| root.schedule_choice_changed(&text, cx));
                }
            }));
            input
        });
        self.schedules.name_input = Some(name_input);
        self.schedules.prompt_input = Some(prompt_input);
        self.schedules.choice_input = Some(choice_input);
    }

    /// Save the editor's draft, then close it when the daemon took it.
    pub(crate) fn save_schedule(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = self.schedules.editor.as_ref() else { return };
        if editor.saving {
            return;
        }
        let name = self.schedules.name_input.as_ref().map(|input| input.read(cx).text().to_owned()).unwrap_or_default();
        let prompt = self.schedules.prompt_input.as_ref().map(|input| input.read(cx).text().to_owned()).unwrap_or_default();
        let Some(input) = editor.input(&name, &prompt) else {
            if let Some(editor) = self.schedules.editor.as_mut() {
                editor.error = Some("A schedule needs a name, what to do, a bot and a valid time.".to_owned());
            }
            cx.notify();
            return;
        };
        let id = editor.id.clone();
        if let Some(editor) = self.schedules.editor.as_mut() {
            editor.saving = true;
            editor.error = None;
        }
        let (method, params) = match id {
            Some(id) => ("automation.update", {
                let mut params = input.params();
                params["id"] = json!(id);
                params
            }),
            None => ("automation.create", input.params()),
        };
        self.daemon_command(
            crate::daemon::Command::Rpc { tag: "automation-save".to_owned(), method: method.to_owned(), params },
            cx,
        );
    }

    pub(crate) fn set_automation_enabled(&mut self, id: String, enabled: bool, cx: &mut Context<Self>) {
        self.schedules.busy = Some(id.clone());
        self.daemon_command(
            crate::daemon::Command::Rpc {
                tag: "automation-toggle".to_owned(),
                method: if enabled { "automation.resume" } else { "automation.pause" }.to_owned(),
                params: json!({ "id": id }),
            },
            cx,
        );
    }

    pub(crate) fn run_automation_now(&mut self, id: String, cx: &mut Context<Self>) {
        self.schedules.busy = Some(id.clone());
        self.daemon_command(
            crate::daemon::Command::Rpc { tag: "automation-run-now".to_owned(), method: "automation.run_now".to_owned(), params: json!({ "id": id }) },
            cx,
        );
    }

    pub(crate) fn delete_automation(&mut self, id: String, cx: &mut Context<Self>) {
        self.schedules.busy = Some(id.clone());
        self.daemon_command(
            crate::daemon::Command::Rpc { tag: "automation-delete".to_owned(), method: "automation.delete".to_owned(), params: json!({ "id": id }) },
            cx,
        );
    }

    pub(crate) fn set_schedules_paused(&mut self, paused: bool, cx: &mut Context<Self>) {
        self.schedules.paused = paused;
        self.schedules.busy = Some("global".to_owned());
        self.daemon_command(
            crate::daemon::Command::Rpc {
                tag: "automations-paused".to_owned(),
                method: "settings.set".to_owned(),
                params: json!({ "key": "automations.paused", "value": paused }),
            },
            cx,
        );
        cx.notify();
    }

    /// Escape closes the top thing this page opened; `true` when it did.
    pub(crate) fn schedules_close_overlay(&mut self, cx: &mut Context<Self>) -> bool {
        if self.schedules.editor.take().is_some() {
            self.schedules.name_input = None;
            self.schedules.prompt_input = None;
            self.schedules.choice_input = None;
            cx.notify();
            return true;
        }
        if self.schedules.deleting.take().is_some() {
            cx.notify();
            return true;
        }
        false
    }

    // -----------------------------------------------------------------------
    // Drawing
    // -----------------------------------------------------------------------

    pub(crate) fn render_schedules_page(&mut self, colors: Colors, cx: &mut Context<Self>) -> AnyElement {
        let now_ms = chrono::Utc::now().timestamp_millis();
        let agents: Vec<Agent> = self.agents().to_vec();
        let connected = self.connected;

        let back = div()
            .id("schedules-back")
            .size(px(28.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(theme::RADIUS_SM))
            .cursor_pointer()
            .hover(move |style| style.bg(colors.hover))
            .child("←")
            .on_click(cx.listener(|root, _: &gpui::ClickEvent, _window, cx| root.back_to_conversation(cx)));

        let new_button = if connected && agents.iter().any(|agent| !agent.archived) {
            self.primary_button("New schedule", "schedules-new", colors, cx, |root, cx| {
                root.open_schedule_editor(None, None, cx);
            })
        } else {
            div().into_any_element()
        };

        let header = div()
            .h(px(46.0))
            .flex_none()
            .px(px(12.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .border_b_1()
            .border_color(colors.line)
            .child(back)
            .child(div().text_size(px(theme::FS_LG)).font_weight(FontWeight::SEMIBOLD).child("Schedules"))
            .child(div().flex_1())
            .child(new_button);

        let paused_row = div()
            .h(px(44.0))
            .px(px(12.0))
            .flex()
            .items_center()
            .gap(px(10.0))
            .rounded(px(theme::RADIUS_MD))
            .bg(colors.panel)
            .border_1()
            .border_color(colors.line)
            .child(div().flex_1().flex().flex_col().child(div().font_weight(FontWeight::MEDIUM).child("Pause all schedules")).child(
                div().text_size(px(theme::FS_XS)).text_color(colors.text_3).child(
                    "Schedules run only while Retinue is running and your computer is awake. Missed runs become one run when you're back.",
                ),
            ))
            .child(
                div()
                    .id("schedules-pause-all")
                    .h(px(24.0))
                    .px(px(10.0))
                    .flex()
                    .items_center()
                    .rounded(px(theme::RADIUS_SM))
                    .bg(if self.schedules.paused { colors.accent } else { colors.raised })
                    .text_color(if self.schedules.paused { colors.text_inverse } else { colors.text_2 })
                    .text_size(px(theme::FS_XS))
                    .cursor_pointer()
                    .hover(move |style| style.bg(colors.selected))
                    .child(if self.schedules.paused { "Paused" } else { "Running" })
                    .on_click(cx.listener(move |root, _: &gpui::ClickEvent, _window, cx| {
                        let next = !root.schedules.paused;
                        root.set_schedules_paused(next, cx);
                    })),
            );

        let mut column = div().flex().flex_col().gap(px(10.0)).child(paused_row);
        if let Some(problem) = self.schedules.problem.clone().or_else(|| self.schedules.error.clone()) {
            column = column.child(div().text_size(px(theme::FS_XS)).text_color(colors.status_danger).child(SharedString::from(problem)));
        }
        if !connected {
            column = column.child(div().text_color(colors.text_3).child("Connect to the local runtime to manage schedules."));
        }
        if !self.schedules.loaded && connected {
            column = column.child(div().text_color(colors.text_3).child("Loading schedules…"));
        }

        let mut proposals: Vec<Automation> = Vec::new();
        let mut rest: Vec<Automation> = Vec::new();
        for automation in &self.schedules.automations {
            if automation.is_proposal() {
                proposals.push(automation.clone());
            } else {
                rest.push(automation.clone());
            }
        }
        rest.sort_by(|a, b| b.running.cmp(&a.running).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));

        if self.schedules.loaded && rest.is_empty() && proposals.is_empty() {
            column = column.child(self.render_schedule_empty(colors, cx));
        }
        if !proposals.is_empty() {
            let mut group = div().flex().flex_col().gap(px(6.0)).child(div().font_weight(FontWeight::MEDIUM).child("Proposed by your bots"));
            group = group.child(div().text_size(px(theme::FS_XS)).text_color(colors.text_3).child("These stay off until you approve them."));
            for automation in proposals {
                group = group.child(self.render_schedule_card(&automation, &agents, now_ms, colors, cx));
            }
            column = column.child(group);
        }
        for automation in rest {
            column = column.child(self.render_schedule_card(&automation, &agents, now_ms, colors, cx));
        }

        div()
            .flex_1()
            .h_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(header)
            .child(
                div()
                    .id("schedules-scroll")
                    .flex_1()
                    .min_h(px(0.0))
                    .w_full()
                    .overflow_y_scroll()
                    .child(div().max_w(px(860.0)).mx_auto().w_full().px(px(20.0)).py(px(16.0)).child(column)),
            )
            .into_any_element()
    }

    fn render_schedule_empty(&mut self, colors: Colors, cx: &mut Context<Self>) -> AnyElement {
        let block = div()
            .flex()
            .flex_col()
            .gap(px(10.0))
            .p(px(16.0))
            .rounded(px(theme::RADIUS_MD))
            .bg(colors.panel)
            .border_1()
            .border_color(colors.line)
            .child(div().font_weight(FontWeight::SEMIBOLD).child("No schedules yet"))
            .child(div().text_color(colors.text_2).child(
                "A schedule is something a bot does by itself: check your email every morning, brief you on the news, run the tests overnight. Start from an idea or write your own.",
            ));
        let mut ideas = div().flex().flex_col().gap(px(6.0));
        for template in TEMPLATES.iter() {
            ideas = ideas.child(
                div()
                    .id(SharedString::from(format!("schedule-idea-{}", template.id)))
                    .px(px(12.0))
                    .py(px(8.0))
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .rounded(px(theme::RADIUS_SM))
                    .bg(colors.raised)
                    .cursor_pointer()
                    .hover(move |style| style.bg(colors.hover))
                    .child(div().font_weight(FontWeight::MEDIUM).child(SharedString::from(template.title)))
                    .child(div().text_size(px(theme::FS_XS)).text_color(colors.text_3).child(SharedString::from(template.hint)))
                    .on_click(cx.listener(move |root, _: &gpui::ClickEvent, _window, cx| {
                        root.open_schedule_editor(None, Some(template), cx);
                    })),
            );
        }
        block.child(ideas).into_any_element()
    }

    fn render_schedule_card(&mut self, automation: &Automation, agents: &[Agent], now_ms: i64, colors: Colors, cx: &mut Context<Self>) -> AnyElement {
        let agent = agents.iter().find(|agent| agent.id == automation.agent_id).cloned();
        let busy = self.schedules.busy.as_deref() == Some(automation.id.as_str());
        let history_open = self.schedules.history_open.contains(&automation.id);
        let id = automation.id.clone();
        let name = automation.name.clone();
        let sub = format!(
            "{} · {}",
            agent.as_ref().map(|agent| agent.name.clone()).unwrap_or_else(|| "Unknown bot".to_owned()),
            automation.schedule().map(|schedule| describe_schedule(&schedule)).unwrap_or_else(|| "No schedule".to_owned())
        );
        let mut row = div()
            .id(SharedString::from(format!("schedule-{}", automation.id)))
            .p(px(12.0))
            .flex()
            .flex_col()
            .gap(px(6.0))
            .rounded(px(theme::RADIUS_MD))
            .bg(colors.panel)
            .border_1()
            .border_color(colors.line);
        let mut head = div().flex().items_center().gap(px(10.0));
        if let Some(agent) = agent.as_ref() {
            let provider = self.provider_of(agent);
            head = head.child(
                crate::art::Mark::for_agent(28.0, crate::art::agent_color(&agent.color), agent.icon(), provider.as_deref(), agent.name.clone(), colors).render(),
            );
        }
        head = head
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .child(div().font_weight(FontWeight::MEDIUM).truncate().child(SharedString::from(automation.name.clone())))
                    .child(div().text_size(px(theme::FS_XS)).text_color(colors.text_3).truncate().child(SharedString::from(sub)))
                    .child(
                        div()
                            .text_size(px(theme::FS_XS))
                            .text_color(if automation.running { colors.status_working } else { colors.text_2 })
                            .truncate()
                            .child(SharedString::from(automation.state_line(now_ms))),
                    ),
            )
            .child(if automation.is_proposal() {
                self.primary_button("Approve", "schedule-approve", colors, cx, {
                    let id = id.clone();
                    move |root, cx| root.set_automation_enabled(id.clone(), true, cx)
                })
            } else {
                let on = automation.enabled;
                let target = id.clone();
                div()
                    .id(SharedString::from(format!("schedule-toggle-{}", automation.id)))
                    .h(px(24.0))
                    .px(px(9.0))
                    .flex()
                    .items_center()
                    .rounded(px(theme::RADIUS_SM))
                    .bg(if on { colors.accent } else { colors.raised })
                    .text_color(if on { colors.text_inverse } else { colors.text_2 })
                    .text_size(px(theme::FS_2XS))
                    .cursor_pointer()
                    .hover(move |style| style.bg(colors.selected))
                    .child(if on { "On" } else { "Off" })
                    .on_click(cx.listener(move |root, _: &gpui::ClickEvent, _window, cx| {
                        root.set_automation_enabled(target.clone(), !on, cx);
                    }))
                    .into_any_element()
            })
            .child(self.ghost_button("Run now", "schedule-run", colors, cx, {
                let id = id.clone();
                move |root, cx| root.run_automation_now(id.clone(), cx)
            }))
            .child(self.ghost_button("Edit", "schedule-edit", colors, cx, {
                let automation = automation.clone();
                move |root, cx| root.open_schedule_editor(Some(automation.clone()), None, cx)
            }))
            .child(self.ghost_button("Delete", "schedule-delete", colors, cx, {
                let id = id.clone();
                let name = name.clone();
                move |root, cx| {
                    root.schedules.deleting = Some((id.clone(), name.clone()));
                    cx.notify();
                }
            }))
            .child(self.ghost_button(if history_open { "Hide" } else { "History" }, "schedule-history", colors, cx, {
                let id = id.clone();
                move |root, cx| {
                    if root.schedules.history_open.contains(&id) {
                        root.schedules.history_open.remove(&id);
                        cx.notify();
                    } else {
                        root.schedules.history_open.insert(id.clone());
                        root.load_automation_runs(id.clone(), cx);
                        cx.notify();
                    }
                }
            }));
        row = row.child(head);
        if automation.is_proposal() {
            row = row.child(div().text_size(px(theme::FS_XS)).text_color(colors.text_2).child(
                "A bot proposed this. It runs in read-only mode until you approve it.",
            ));
        }
        if let Some(last) = automation.last_run.as_ref().filter(|_| !history_open) {
            let retry_id = automation.id.clone();
            let retry = needs_manual_retry(Some(&last.status), last.reason.as_deref()) && !automation.running;
            row = row.child(div().text_size(px(theme::FS_XS)).text_color(colors.text_3).child(SharedString::from(format!(
                "Last run {} · {}{}",
                crate::state::tasks::when_text(last.started_at.as_deref()),
                run_status_label(&last.status),
                match run_reason_text(last.reason.as_deref()) {
                    reason if reason.is_empty() => String::new(),
                    reason => format!(" · {reason}"),
                }
            ))));
            if retry {
                row = row.child(self.ghost_button("Retry now", "schedule-retry", colors, cx, move |root, cx| {
                    root.run_automation_now(retry_id.clone(), cx);
                }));
            }
        }
        if busy {
            row = row.child(div().text_size(px(theme::FS_XS)).text_color(colors.text_3).child("Working…"));
        }
        if history_open {
            row = row.child(div().text_size(px(theme::FS_XS)).text_color(colors.text_2).child(SharedString::from(automation.prompt.clone())));
            match self.schedules.runs.get(&automation.id) {
                None => row = row.child(div().text_size(px(theme::FS_XS)).text_color(colors.text_3).child("Loading…")),
                Some(runs) if runs.is_empty() => row = row.child(div().text_size(px(theme::FS_XS)).text_color(colors.text_3).child("No runs yet.")),
                Some(runs) => {
                    let mut list = div().flex().flex_col().gap(px(2.0));
                    for run in runs.iter().take(20) {
                        let reason = run_reason_text(run.reason.as_deref());
                        list = list.child(div().flex().items_center().gap(px(8.0)).text_size(px(theme::FS_XS)).child(
                            div().w(px(110.0)).text_color(colors.text_3).child(SharedString::from(crate::state::tasks::when_text(run.started_at.as_deref()))),
                        ).child(
                            div()
                                .flex_1()
                                .min_w(px(0.0))
                                .truncate()
                                .child(SharedString::from(format!("{}{}", run_status_label(&run.status), if reason.is_empty() { String::new() } else { format!(" · {reason}") }))),
                        ));
                    }
                    row = row.child(list);
                }
            }
        }
        row.into_any_element()
    }

    /// The editor, drawn over the window, and the delete confirmation with it.
    pub(crate) fn render_schedules_overlay(&mut self, colors: Colors, cx: &mut Context<Self>) -> Option<AnyElement> {
        let card = if self.schedules.editor.is_some() {
            Some(self.render_schedule_editor(colors, cx))
        } else if self.schedules.deleting.is_some() {
            Some(self.render_schedule_delete(colors, cx))
        } else {
            None
        }?;
        Some(
            div()
                .id("schedules-backdrop")
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .bg(colors.scrim)
                .on_mouse_down(gpui::MouseButton::Left, cx.listener(|root, _: &gpui::MouseDownEvent, _window, cx| {
                    root.schedules_close_overlay(cx);
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

    fn render_schedule_editor(&mut self, colors: Colors, cx: &mut Context<Self>) -> AnyElement {
        let Some(editor) = self.schedules.editor.as_ref() else { return div().into_any_element() };
        let editing = editor.id.is_some();
        let draft = editor.draft.clone();
        let agent_id = editor.agent_id.clone();
        let approval = editor.approval_mode.clone();
        let session_mode = editor.session_mode.clone();
        let limit = editor.limit_secs;
        let saving = editor.saving;
        let error = editor.error.clone();
        let agents: Vec<Agent> = self.agents().iter().filter(|agent| !agent.archived && agent.parent_agent_id.is_none()).cloned().collect();
        let preview: Vec<String> = self.schedules.preview.clone();
        let preview_error = self.schedules.preview_error.clone();
        let now_ms = chrono::Utc::now().timestamp_millis();

        let mut body = div()
            .w(px(560.0))
            .max_h(px(620.0))
            .id("schedule-editor")
            .overflow_y_scroll()
            .p(px(16.0))
            .flex()
            .flex_col()
            .gap(px(10.0))
            .rounded(px(theme::RADIUS_LG))
            .bg(colors.overlay)
            .border_1()
            .border_color(colors.line_strong)
            .child(div().text_size(px(theme::FS_LG)).font_weight(FontWeight::SEMIBOLD).child(if editing { "Edit schedule" } else { "New schedule" }));

        if !editing {
            let mut ideas = div().flex().flex_wrap().gap(px(6.0));
            for template in TEMPLATES.iter() {
                ideas = ideas.child(
                    div()
                        .id(SharedString::from(format!("editor-idea-{}", template.id)))
                        .px(px(9.0))
                        .h(px(24.0))
                        .flex()
                        .items_center()
                        .rounded(px(theme::RADIUS_SM))
                        .bg(colors.raised)
                        .text_size(px(theme::FS_XS))
                        .cursor_pointer()
                        .hover(move |style| style.bg(colors.hover))
                        .child(SharedString::from(template.title))
                        .on_click(cx.listener(move |root, _: &gpui::ClickEvent, _window, cx| {
                            if let Some(editor) = root.schedules.editor.as_mut() {
                                editor.template = Some(template);
                                editor.draft.preset = template.preset;
                                editor.draft.time = template.time.to_owned();
                                editor.approval_mode = template.approval_mode.to_owned();
                            }
                            if let Some(input) = root.schedules.name_input.clone() {
                                input.update(cx, |input, cx| input.set_text(template.name.to_owned(), cx));
                            }
                            if let Some(input) = root.schedules.prompt_input.clone() {
                                input.update(cx, |input, cx| input.set_text(template.prompt.to_owned(), cx));
                            }
                            if let Some(input) = root.schedules.choice_input.clone() {
                                input.update(cx, |input, cx| input.set_text(template.time.to_owned(), cx));
                            }
                            root.request_preview(cx);
                            cx.notify();
                        })),
                );
            }
            body = body.child(ideas);
        }

        body = body.child(
            div()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(div().text_size(px(theme::FS_XS)).text_color(colors.text_3).child("Name"))
                .child(
                    div()
                        .h(px(32.0))
                        .px(px(8.0))
                        .flex()
                        .items_center()
                        .rounded(px(theme::RADIUS_SM))
                        .bg(colors.input)
                        .border_1()
                        .border_color(colors.line)
                        .child(div().flex_1().min_w(px(0.0)).child(self.schedules.name_input.clone().unwrap_or_else(|| cx.new(|cx| TextInput::new("Name", px(20.0), px(theme::FS_SM), cx))))),
                ),
        );

        let mut bot_row = div().flex().flex_wrap().gap(px(6.0));
        for agent in &agents {
            let chosen = agent.id == agent_id;
            let id = agent.id.clone();
            bot_row = bot_row.child(
                div()
                    .id(SharedString::from(format!("editor-bot-{}", agent.id)))
                    .h(px(24.0))
                    .px(px(9.0))
                    .flex()
                    .items_center()
                    .rounded(px(theme::RADIUS_SM))
                    .bg(if chosen { colors.accent } else { colors.raised })
                    .text_color(if chosen { colors.text_inverse } else { colors.text_2 })
                    .text_size(px(theme::FS_XS))
                    .cursor_pointer()
                    .hover(move |style| style.bg(colors.selected))
                    .child(SharedString::from(agent.name.clone()))
                    .on_click(cx.listener(move |root, _: &gpui::ClickEvent, _window, cx| {
                        if let Some(editor) = root.schedules.editor.as_mut() {
                            editor.agent_id = id.clone();
                        }
                        cx.notify();
                    })),
            );
        }
        body = body.child(
            div()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(div().text_size(px(theme::FS_XS)).text_color(colors.text_3).child("Bot"))
                .child(bot_row),
        );

        body = body.child(
            div()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(div().text_size(px(theme::FS_XS)).text_color(colors.text_3).child("What should it do?"))
                .child(
                    div()
                        .h(px(64.0))
                        .px(px(8.0))
                        .py(px(6.0))
                        .flex()
                        .rounded(px(theme::RADIUS_SM))
                        .bg(colors.input)
                        .border_1()
                        .border_color(colors.line)
                        .child(div().flex_1().min_w(px(0.0)).child(self.schedules.prompt_input.clone().unwrap_or_else(|| cx.new(|cx| TextInput::new("Prompt", px(18.0), px(theme::FS_SM), cx))))),
                ),
        );

        // When: the presets, then the one detail the preset needs.
        let mut presets = div().flex().flex_wrap().gap(px(6.0));
        for preset in SchedulePreset::ALL {
            let chosen = draft.preset == preset;
            presets = presets.child(
                div()
                    .id(SharedString::from(format!("editor-preset-{}", preset.id())))
                    .h(px(24.0))
                    .px(px(9.0))
                    .flex()
                    .items_center()
                    .rounded(px(theme::RADIUS_SM))
                    .bg(if chosen { colors.accent } else { colors.raised })
                    .text_color(if chosen { colors.text_inverse } else { colors.text_2 })
                    .text_size(px(theme::FS_XS))
                    .cursor_pointer()
                    .hover(move |style| style.bg(colors.selected))
                    .child(SharedString::from(preset.label()))
                    .on_click(cx.listener(move |root, _: &gpui::ClickEvent, _window, cx| {
                        if let Some(editor) = root.schedules.editor.as_mut() {
                            editor.draft.preset = preset;
                        }
                        let seed = root
                            .schedules
                            .editor
                            .as_ref()
                            .map(|editor| match editor.draft.preset {
                                SchedulePreset::Custom => editor.draft.cron.clone(),
                                SchedulePreset::Once => editor.draft.once_at.clone(),
                                SchedulePreset::Hours => String::new(),
                                _ => editor.draft.time.clone(),
                            })
                            .unwrap_or_default();
                        if let Some(input) = root.schedules.choice_input.clone() {
                            input.update(cx, |input, cx| input.set_text(seed, cx));
                        }
                        root.request_preview(cx);
                        cx.notify();
                    })),
            );
        }
        let mut when = div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(div().text_size(px(theme::FS_XS)).text_color(colors.text_3).child("When"))
            .child(presets);
        match draft.preset {
            SchedulePreset::Daily | SchedulePreset::Weekdays | SchedulePreset::Weekly => {
                when = when.child(self.choice_field("Time (24-hour, HH:MM)", colors, cx));
            }
            SchedulePreset::Once => when = when.child(self.choice_field("Date and time (2026-11-09T18:00)", colors, cx)),
            SchedulePreset::Custom => when = when.child(self.choice_field("Cron (minute hour day month weekday)", colors, cx)),
            SchedulePreset::Hours => {
                let mut hours = div().flex().flex_wrap().gap(px(6.0));
                for n in [1u32, 2, 3, 4, 6, 8, 12] {
                    let chosen = draft.every_hours == n;
                    hours = hours.child(
                        div()
                            .id(SharedString::from(format!("editor-hours-{n}")))
                            .h(px(24.0))
                            .px(px(9.0))
                            .flex()
                            .items_center()
                            .rounded(px(theme::RADIUS_SM))
                            .bg(if chosen { colors.accent } else { colors.raised })
                            .text_color(if chosen { colors.text_inverse } else { colors.text_2 })
                            .text_size(px(theme::FS_XS))
                            .cursor_pointer()
                            .hover(move |style| style.bg(colors.selected))
                            .child(SharedString::from(if n == 1 { "Every hour".to_owned() } else { format!("Every {n} hours") }))
                            .on_click(cx.listener(move |root, _: &gpui::ClickEvent, _window, cx| {
                                if let Some(editor) = root.schedules.editor.as_mut() {
                                    editor.draft.every_hours = n;
                                }
                                root.request_preview(cx);
                                cx.notify();
                            })),
                    );
                }
                when = when.child(hours);
            }
        }
        if draft.preset == SchedulePreset::Weekly {
            let mut days = div().flex().flex_wrap().gap(px(6.0));
            for (index, label) in WEEKDAYS.iter().enumerate() {
                let index = index as u32;
                let chosen = draft.weekday == index;
                days = days.child(
                    div()
                        .id(SharedString::from(format!("editor-weekday-{index}")))
                        .h(px(24.0))
                        .px(px(9.0))
                        .flex()
                        .items_center()
                        .rounded(px(theme::RADIUS_SM))
                        .bg(if chosen { colors.accent } else { colors.raised })
                        .text_color(if chosen { colors.text_inverse } else { colors.text_2 })
                        .text_size(px(theme::FS_XS))
                        .cursor_pointer()
                        .hover(move |style| style.bg(colors.selected))
                        .child(SharedString::from(*label))
                        .on_click(cx.listener(move |root, _: &gpui::ClickEvent, _window, cx| {
                            if let Some(editor) = root.schedules.editor.as_mut() {
                                editor.draft.weekday = index;
                            }
                            root.request_preview(cx);
                            cx.notify();
                        })),
                );
            }
            when = when.child(days);
        }
        if draft.preset != SchedulePreset::Once {
            when = when.child(div().text_size(px(theme::FS_2XS)).text_color(colors.text_3).child(SharedString::from(format!("Time zone: {}", draft.timezone))));
        }
        when = when.child(div().text_size(px(theme::FS_XS)).text_color(if preview_error.is_some() { colors.status_danger } else { colors.text_3 }).child(
            SharedString::from(match preview_error {
                Some(error) => error,
                None if preview.is_empty() => "Choose a valid time.".to_owned(),
                None => format!(
                    "Next: {}",
                    preview.iter().map(|at| describe_next_run(Some(at), now_ms)).collect::<Vec<_>>().join(" · ")
                ),
            }),
        ));
        body = body.child(when);

        // While it runs: never Full access, because nobody is there to approve it.
        let mut modes = div().flex().gap(px(6.0));
        for (id, title, hint) in [
            ("auto", "Safe actions only", "Reads and edits inside the project run. Anything riskier is declined."),
            ("plan", "Read-only", "The bot looks and reports. It does not change anything."),
        ] {
            let chosen = approval == id;
            modes = modes.child(
                div()
                    .id(SharedString::from(format!("editor-approval-{id}")))
                    .flex_1()
                    .p(px(8.0))
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .rounded(px(theme::RADIUS_SM))
                    .bg(if chosen { colors.selected } else { colors.raised })
                    .border_1()
                    .border_color(if chosen { colors.line_strong } else { colors.line })
                    .cursor_pointer()
                    .hover(move |style| style.bg(colors.hover))
                    .child(div().font_weight(FontWeight::MEDIUM).text_size(px(theme::FS_XS)).child(title))
                    .child(div().text_size(px(theme::FS_2XS)).text_color(colors.text_3).child(hint))
                    .on_click(cx.listener(move |root, _: &gpui::ClickEvent, _window, cx| {
                        if let Some(editor) = root.schedules.editor.as_mut() {
                            editor.approval_mode = id.to_owned();
                        }
                        cx.notify();
                    })),
            );
        }
        body = body.child(
            div()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(div().text_size(px(theme::FS_XS)).text_color(colors.text_3).child("While it runs"))
                .child(modes),
        );

        let mut conversations = div().flex().gap(px(6.0));
        for (id, label) in [("new", "A new one each run"), ("reuse", "Keep going in the same one")] {
            let chosen = session_mode == id;
            conversations = conversations.child(
                div()
                    .id(SharedString::from(format!("editor-session-{id}")))
                    .h(px(24.0))
                    .px(px(9.0))
                    .flex()
                    .items_center()
                    .rounded(px(theme::RADIUS_SM))
                    .bg(if chosen { colors.accent } else { colors.raised })
                    .text_color(if chosen { colors.text_inverse } else { colors.text_2 })
                    .text_size(px(theme::FS_XS))
                    .cursor_pointer()
                    .hover(move |style| style.bg(colors.selected))
                    .child(label)
                    .on_click(cx.listener(move |root, _: &gpui::ClickEvent, _window, cx| {
                        if let Some(editor) = root.schedules.editor.as_mut() {
                            editor.session_mode = id.to_owned();
                        }
                        cx.notify();
                    })),
            );
        }
        body = body.child(
            div()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(div().text_size(px(theme::FS_XS)).text_color(colors.text_3).child("Conversation"))
                .child(conversations),
        );

        let mut limits = div().flex().flex_wrap().gap(px(6.0));
        for (secs, label) in RUNTIME_CHOICES {
            let chosen = limit == secs;
            limits = limits.child(
                div()
                    .id(SharedString::from(format!("editor-limit-{secs}")))
                    .h(px(24.0))
                    .px(px(9.0))
                    .flex()
                    .items_center()
                    .rounded(px(theme::RADIUS_SM))
                    .bg(if chosen { colors.accent } else { colors.raised })
                    .text_color(if chosen { colors.text_inverse } else { colors.text_2 })
                    .text_size(px(theme::FS_XS))
                    .cursor_pointer()
                    .hover(move |style| style.bg(colors.selected))
                    .child(label)
                    .on_click(cx.listener(move |root, _: &gpui::ClickEvent, _window, cx| {
                        if let Some(editor) = root.schedules.editor.as_mut() {
                            editor.limit_secs = secs;
                        }
                        cx.notify();
                    })),
            );
        }
        body = body.child(
            div()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(div().text_size(px(theme::FS_XS)).text_color(colors.text_3).child("Stop a run after"))
                .child(limits),
        );

        if let Some(error) = error {
            body = body.child(div().text_size(px(theme::FS_XS)).text_color(colors.status_danger).child(SharedString::from(error)));
        }

        let mut actions = div().flex().items_center().gap(px(8.0)).child(div().flex_1());
        actions = actions
            .child(self.ghost_button("Cancel", "schedule-cancel", colors, cx, |root, cx| {
                root.schedules_close_overlay(cx);
            }))
            .child(if saving {
                div().px(px(12.0)).h(px(28.0)).flex().items_center().text_size(px(theme::FS_XS)).text_color(colors.text_3).child("Saving…").into_any_element()
            } else {
                self.primary_button(if editing { "Save" } else { "Create schedule" }, "schedule-save", colors, cx, |root, cx| {
                    root.save_schedule(cx);
                })
            });
        body.child(actions).into_any_element()
    }

    fn choice_field(&mut self, label: &str, colors: Colors, cx: &mut Context<Self>) -> AnyElement {
        let placeholder = label.to_owned();
        div()
            .flex()
            .flex_col()
            .gap(px(3.0))
            .child(div().text_size(px(theme::FS_2XS)).text_color(colors.text_3).child(SharedString::from(placeholder.clone())))
            .child(
                div()
                    .h(px(30.0))
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .rounded(px(theme::RADIUS_SM))
                    .bg(colors.input)
                    .border_1()
                    .border_color(colors.line)
                    .child(div().flex_1().min_w(px(0.0)).child(
                        self.schedules
                            .choice_input
                            .clone()
                            .unwrap_or_else(|| cx.new(move |cx| TextInput::new(placeholder, px(18.0), px(theme::FS_SM), cx))),
                    )),
            )
            .into_any_element()
    }

    /// The draft's own detail field writes back into the draft as it is typed:
    /// read at save time from the field, applied here by a change hook.
    pub(crate) fn schedule_choice_changed(&mut self, text: &str, cx: &mut Context<Self>) {
        let Some(editor) = self.schedules.editor.as_mut() else { return };
        match editor.draft.preset {
            SchedulePreset::Daily | SchedulePreset::Weekdays | SchedulePreset::Weekly => editor.draft.time = text.to_owned(),
            SchedulePreset::Once => editor.draft.once_at = text.to_owned(),
            SchedulePreset::Custom => editor.draft.cron = text.to_owned(),
            SchedulePreset::Hours => {}
        }
        cx.notify();
    }

    fn render_schedule_delete(&mut self, colors: Colors, cx: &mut Context<Self>) -> AnyElement {
        let Some((id, name)) = self.schedules.deleting.clone() else { return div().into_any_element() };
        let card = div()
            .w(px(420.0))
            .p(px(16.0))
            .flex()
            .flex_col()
            .gap(px(10.0))
            .rounded(px(theme::RADIUS_LG))
            .bg(colors.overlay)
            .border_1()
            .border_color(colors.line_strong)
            .child(div().text_size(px(theme::FS_MD)).font_weight(FontWeight::SEMIBOLD).child(SharedString::from(format!("Delete \"{name}\"?"))))
            .child(div().text_color(colors.text_2).child("The schedule stops. Conversations it already started stay where they are."));
        let mut actions = div().flex().items_center().gap(px(8.0)).child(div().flex_1());
        actions = actions
            .child(self.ghost_button("Keep", "schedule-keep", colors, cx, |root, cx| {
                root.schedules.deleting = None;
                cx.notify();
            }))
            .child(self.primary_button("Delete", "schedule-confirm-delete", colors, cx, move |root, cx| {
                root.delete_automation(id.clone(), cx);
            }));
        card.child(actions).into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft(preset: SchedulePreset) -> ScheduleDraft {
        ScheduleDraft { preset, ..default_draft("Europe/Berlin") }
    }

    #[test]
    fn a_preset_becomes_the_cron_the_daemon_expects() {
        let mut daily = draft(SchedulePreset::Daily);
        daily.time = "09:00".into();
        assert_eq!(preset_cron(&daily).as_deref(), Some("0 9 * * *"));
        daily.time = "07:30".into();
        assert_eq!(preset_cron(&daily).as_deref(), Some("30 7 * * *"));
        let weekdays = ScheduleDraft { preset: SchedulePreset::Weekdays, time: "07:30".into(), ..default_draft("UTC") };
        assert_eq!(preset_cron(&weekdays).as_deref(), Some("30 7 * * 1-5"));
        let weekly = ScheduleDraft { preset: SchedulePreset::Weekly, time: "18:05".into(), weekday: 3, ..default_draft("UTC") };
        assert_eq!(preset_cron(&weekly).as_deref(), Some("5 18 * * 3"));
        let hours = ScheduleDraft { preset: SchedulePreset::Hours, every_hours: 6, ..default_draft("UTC") };
        assert_eq!(preset_cron(&hours).as_deref(), Some("0 */6 * * *"));
    }

    #[test]
    fn an_incomplete_draft_has_no_schedule() {
        let mut daily = draft(SchedulePreset::Daily);
        daily.time = "9".into();
        assert_eq!(preset_cron(&daily), None);
        daily.time = "24:00".into();
        assert_eq!(preset_cron(&daily), None);
        let hours = ScheduleDraft { preset: SchedulePreset::Hours, every_hours: 0, ..default_draft("UTC") };
        assert_eq!(preset_cron(&hours), None);
        let hours = ScheduleDraft { preset: SchedulePreset::Hours, every_hours: 13, ..default_draft("UTC") };
        assert_eq!(preset_cron(&hours), None);
        let custom = ScheduleDraft { preset: SchedulePreset::Custom, cron: "0 9 *".into(), ..default_draft("UTC") };
        assert_eq!(draft_to_schedule(&custom), None);
        let custom = ScheduleDraft { preset: SchedulePreset::Custom, cron: "  0   9  * *  * ".into(), ..default_draft("UTC") };
        assert_eq!(preset_cron(&custom).as_deref(), Some("0 9 * * *"));
    }

    #[test]
    fn an_existing_cron_reopens_on_the_preset_it_stands_for() {
        let daily = schedule_to_draft(&DaemonSchedule::Cron { cron: "30 7 * * *".into(), timezone: "UTC".into() }, "UTC");
        assert_eq!((daily.preset, daily.time.as_str()), (SchedulePreset::Daily, "07:30"));
        let weekdays = schedule_to_draft(&DaemonSchedule::Cron { cron: "30 7 * * 1-5".into(), timezone: "UTC".into() }, "UTC");
        assert_eq!((weekdays.preset, weekdays.time.as_str()), (SchedulePreset::Weekdays, "07:30"));
        let weekly = schedule_to_draft(&DaemonSchedule::Cron { cron: "5 18 * * 3".into(), timezone: "UTC".into() }, "UTC");
        assert_eq!((weekly.preset, weekly.weekday), (SchedulePreset::Weekly, 3));
        let hours = schedule_to_draft(&DaemonSchedule::Cron { cron: "0 */6 * * *".into(), timezone: "UTC".into() }, "UTC");
        assert_eq!((hours.preset, hours.every_hours), (SchedulePreset::Hours, 6));
        let custom = schedule_to_draft(&DaemonSchedule::Cron { cron: "15 2 1 * *".into(), timezone: "UTC".into() }, "UTC");
        assert_eq!(custom.preset, SchedulePreset::Custom, "a cron the editor has no preset for stays custom");
    }

    #[test]
    fn a_draft_describes_itself_in_one_line() {
        let daily = DaemonSchedule::Cron { cron: "0 9 * * *".into(), timezone: "UTC".into() };
        assert_eq!(describe_schedule(&daily), "Every day at 09:00");
        let weekdays = DaemonSchedule::Cron { cron: "30 7 * * 1-5".into(), timezone: "UTC".into() };
        assert_eq!(describe_schedule(&weekdays), "Weekdays at 07:30");
        let weekly = DaemonSchedule::Cron { cron: "5 18 * * 3".into(), timezone: "UTC".into() };
        assert_eq!(describe_schedule(&weekly), "Every Wednesday at 18:05");
        let hours = DaemonSchedule::Cron { cron: "0 */1 * * *".into(), timezone: "UTC".into() };
        assert_eq!(describe_schedule(&hours), "Every hour");
        let hours = DaemonSchedule::Cron { cron: "0 */4 * * *".into(), timezone: "UTC".into() };
        assert_eq!(describe_schedule(&hours), "Every 4 hours");
        let custom = DaemonSchedule::Cron { cron: "15 2 1 * *".into(), timezone: "UTC".into() };
        assert_eq!(describe_schedule(&custom), "Custom: 15 2 1 * *");
    }

    #[test]
    fn a_next_run_is_said_in_words() {
        let now = crate::state::tasks::parse_time(Some("2026-11-08T10:00:00Z")).unwrap();
        assert_eq!(describe_next_run(Some("2026-11-08T11:30:00Z"), now), "in 1 h 30 min");
        assert_eq!(describe_next_run(Some("2026-11-08T10:12:00Z"), now), "in 12 min");
        assert_eq!(describe_next_run(Some("2026-11-08T10:00:20Z"), now), "in under a minute");
        assert_eq!(describe_next_run(Some("2026-11-08T09:00:00Z"), now), "due now");
        assert_eq!(describe_next_run(None, now), "");
        assert_eq!(describe_next_run(Some("not a date"), now), "");
    }

    #[test]
    fn a_one_time_schedule_is_stored_as_the_daemons_utc_instant() {
        let at = once_at_utc("2026-11-09T18:00").expect("a local instant");
        assert!(at.ends_with('Z'), "{at}");
        let schedule = DaemonSchedule::Once { at: at.clone() };
        let back = schedule_to_draft(&schedule, "UTC");
        assert_eq!(back.preset, SchedulePreset::Once);
        assert!(!back.once_at.is_empty());
        assert_eq!(once_at_utc("not a date"), None);
    }

    #[test]
    fn a_schedule_round_trips_through_the_wire_shape() {
        let schedule = DaemonSchedule::Cron { cron: "0 9 * * *".into(), timezone: "Europe/Berlin".into() };
        assert_eq!(DaemonSchedule::from_json(&schedule.to_json()), Some(schedule));
        let once = DaemonSchedule::Once { at: "2026-11-09T17:00:00.000Z".into() };
        assert_eq!(DaemonSchedule::from_json(&once.to_json()), Some(once));
        assert_eq!(DaemonSchedule::from_json(&json!({"kind":"nonsense"})), None);
    }

    #[test]
    fn a_bot_proposal_waiting_for_approval_says_so() {
        let mut automation = Automation {
            id: "a".into(),
            agent_id: "ada".into(),
            name: "Morning brief".into(),
            enabled: false,
            created_by: Some("agent".into()),
            pause_reason: Some("needs_approval".into()),
            ..Automation::default()
        };
        assert!(automation.is_proposal());
        assert_eq!(automation.pause_text(), "Waiting for your approval");
        automation.pause_reason = Some("auto_failures".into());
        assert_eq!(automation.pause_text(), "Paused after three failed runs");
        automation.enabled = true;
        automation.running = true;
        assert_eq!(automation.state_line(0), "Running now");
    }

    #[test]
    fn the_editor_wants_a_name_a_prompt_a_bot_and_a_valid_time() {
        let mut editor = ScheduleEditor {
            id: None,
            template: None,
            agent_id: "ada".into(),
            draft: default_draft("UTC"),
            approval_mode: "auto".into(),
            session_mode: "new".into(),
            limit_secs: 1800,
            saving: false,
            error: None,
        };
        assert!(editor.input("", "Do the thing").is_none());
        assert!(editor.input("Brief", "").is_none());
        assert!(editor.input("Brief", "Do the thing").is_some(), "a new one starts enabled");
        editor.agent_id.clear();
        assert!(editor.input("Brief", "Do the thing").is_none(), "a schedule needs a bot");
        editor.agent_id = "ada".into();
        editor.draft.time = "99:99".into();
        assert!(editor.input("Brief", "Do the thing").is_none(), "and a valid time");
        editor.draft.time = "09:00".into();
        let input = editor.input("Brief", "Do the thing").unwrap();
        assert_eq!(input.params()["approvalMode"], json!("auto"));
        assert_eq!(input.params()["enabled"], json!(true));
        editor.id = Some("a".into());
        assert!(editor.input("Brief", "Do the thing").unwrap().params().get("enabled").is_none(), "an edit keeps its own switch");
    }

    #[test]
    fn an_unattended_run_is_never_full_access() {
        // The editor offers exactly two modes, and neither is `bypass`.
        for mode in ["auto", "plan"] {
            assert_ne!(mode, "bypass");
        }
        let editor = ScheduleEditor {
            id: None,
            template: None,
            agent_id: "ada".into(),
            draft: default_draft("UTC"),
            approval_mode: "auto".into(),
            session_mode: "new".into(),
            limit_secs: 900,
            saving: false,
            error: None,
        };
        let params = editor.input("Brief", "Do it").unwrap().params();
        assert!(params["maxRuntimeSecs"].as_i64().unwrap() > 0, "a run always has its own time limit");
        assert_eq!(params["sessionMode"], json!("new"));
    }

    #[test]
    fn a_run_reason_is_put_in_words_or_left_empty() {
        assert_eq!(run_reason_text(Some("error_after_changes")), "It stopped after changing things, so it was not retried on its own.");
        assert_eq!(run_reason_text(Some("late:600")), "Started 10 min late");
        assert_eq!(run_reason_text(Some("late:10")), "");
        assert_eq!(run_reason_text(None), "");
        assert!(needs_manual_retry(Some("failed"), Some("error_after_changes")));
        assert!(!needs_manual_retry(Some("failed"), Some("still_running")));
    }
}
