//! The one line under the focused bot's name (`ui/companionStatus.ts`).
//!
//! Reduced to what the native preview has: no usage-limit label, no file
//! activity (there are no per-file rows in this slice), and no "heard nothing"
//! registry — the turn's own `updated_at` stands in for the last activity.

#![allow(dead_code)]  // A ported surface: the helpers are complete and
// slice 6's settings page and slice 7 wire the rest.

use crate::state::Session;

/// A working turn that has heard nothing for this long says it is waiting on
/// the coding agent (`SILENT_AFTER_MS`).
pub const SILENT_AFTER_MS: i64 = 60_000;
/// A finished turn shows the happy state this long, then the bot relaxes.
pub const SUCCESS_HOLD_MS: i64 = 6_000;
/// A bot reads as online for this long after its last activity.
pub const ONLINE_HOLD_MS: i64 = 2 * 60_000;
/// After this long without activity the bot falls asleep.
pub const SLEEP_AFTER_MS: i64 = 10 * 60_000;

/// A short label for the compact bar, one per resting/attention mood
/// (`COMPANION_COMPACT_MOOD_LABELS`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mood {
    Idle,
    Online,
    Working,
    Thinking,
    Success,
    Error,
    Offline,
    Permission,
    Sleeping,
    Listening,
    RateLimited,
    ToolActivity,
}

impl Mood {
    /// The word in the status dot's tooltip and the log.
    pub fn key(self) -> &'static str {
        match self {
            Mood::Idle => "idle",
            Mood::Online => "online",
            Mood::Working => "working",
            Mood::Thinking => "thinking",
            Mood::Success => "success",
            Mood::Error => "error",
            Mood::Offline => "offline",
            Mood::Permission => "permission",
            Mood::Sleeping => "sleeping",
            Mood::Listening => "listening",
            Mood::RateLimited => "rate_limited",
            Mood::ToolActivity => "tool_activity",
        }
    }

    /// The compact bar's short label, or `None` when the status line should speak.
    pub fn compact_label(self) -> Option<&'static str> {
        match self {
            Mood::Sleeping => Some("Sleeping"),
            Mood::Error => Some("Something failed"),
            Mood::Offline => Some("Offline"),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Status {
    pub mood: Mood,
    pub label: String,
}

/// What the status line decides from.
#[derive(Clone, Debug)]
pub struct StatusInput<'a> {
    pub connected: bool,
    /// The focused bot's coding agent status (`runtime.status`).
    pub runtime_status: Option<&'a str>,
    /// Its provider, for "Waiting for Claude Code…".
    pub provider: Option<&'a str>,
    pub session: Option<&'a Session>,
    /// An approval is open for this conversation.
    pub permission_pending: bool,
    /// The composer has text, or the owner is typing.
    pub composing: bool,
    /// Milliseconds since the Unix epoch, so tests can name a moment.
    pub now_ms: i64,
}

/// `deriveCompanionStatus`, in the same order of precedence: connection, coding
/// agent, approval, limit, failure, composing, a fresh finish, an active turn,
/// then rest.
pub fn derive(input: &StatusInput<'_>) -> Status {
    let idle_ms = session_idle_ms(input.session, input.now_ms);
    if !input.connected {
        return status(Mood::Offline, "Daemon disconnected");
    }
    let runtime = input.runtime_status.unwrap_or("").to_lowercase();
    if runtime.is_empty() || matches!(runtime.as_str(), "offline" | "disconnected") {
        return status(Mood::Offline, if input.runtime_status.is_some() { "Runtime offline" } else { "No runtime selected" });
    }
    if runtime == "error" {
        return status(Mood::Error, "Coding agent check failed");
    }
    let Some(session) = input.session else {
        return if input.composing { status(Mood::Listening, "Ready for your message") } else { status(Mood::Idle, "Ready") };
    };
    if input.permission_pending || session.state_word() == "waiting_permission" {
        return status(Mood::Permission, "Approval needed");
    }
    if session.state_word() == "limited" {
        return status(Mood::RateLimited, "Usage limit reached");
    }
    if session.state_word() == "retrying" {
        return status(Mood::Working, "Retrying…");
    }
    if session.state_word() == "rate_limited" {
        return status(Mood::RateLimited, "Rate limited");
    }
    if session.state_word() == "sleeping" {
        return status(Mood::Sleeping, "Sleeping");
    }
    if matches!(session.state_word(), "error" | "failed") {
        return status(Mood::Error, "Needs attention");
    }
    if input.composing {
        return status(Mood::Listening, "Ready for your message");
    }
    if session.state_word() == "completed" {
        return match idle_ms {
            Some(idle) if idle < SUCCESS_HOLD_MS => status(Mood::Success, "Done"),
            _ => resting(idle_ms, "Done"),
        };
    }
    if matches!(session.state_word(), "cancelled" | "canceled") {
        return match idle_ms {
            None => status(Mood::Idle, "Cancelled"),
            Some(idle) => resting(Some(idle), "Cancelled"),
        };
    }
    // A tool step that is still running says what it does.
    if let Some(tool) = session.tools.iter().rev().find(|tool| {
        matches!(tool.state.as_deref().unwrap_or("").to_lowercase().as_str(), "running" | "pending" | "started")
    }) {
        return status(Mood::ToolActivity, tool.summary());
    }
    if matches!(session.state_word(), "working" | "starting") {
        // Nothing has arrived for a while: say who it is waiting on.
        if idle_ms.map(|idle| idle >= SILENT_AFTER_MS).unwrap_or(false) {
            let brand = crate::art::provider_name(input.provider.unwrap_or(""));
            return status(Mood::Working, format!("Waiting for {brand}…"));
        }
        if session.messages.last().map(|message| message.is_thinking()).unwrap_or(false) {
            return status(Mood::Thinking, "Thinking");
        }
        return status(Mood::Working, "Typing…");
    }
    if matches!(session.state_word(), "cancelling") {
        return status(Mood::Working, "Typing…");
    }
    let rest = if session.state_word().is_empty() {
        "Idle".to_owned()
    } else {
        session.state_word().replace('_', " ")
    };
    if matches!(runtime.as_str(), "online" | "ready" | "available" | "connected") {
        return match idle_ms {
            Some(idle) if idle >= ONLINE_HOLD_MS => resting(Some(idle), "Idle"),
            _ => status(Mood::Online, "Online"),
        };
    }
    resting(idle_ms, &rest)
}

fn status(mood: Mood, label: impl Into<String>) -> Status {
    Status { mood, label: label.into() }
}

fn resting(idle_ms: Option<i64>, label: &str) -> Status {
    match idle_ms {
        Some(idle) if idle >= SLEEP_AFTER_MS => status(Mood::Sleeping, "Sleeping"),
        _ => status(Mood::Idle, label.to_owned()),
    }
}

/// Milliseconds since the conversation last changed, or `None` when unknown.
pub fn session_idle_ms(session: Option<&Session>, now_ms: i64) -> Option<i64> {
    let stamp = session?.updated_at.as_deref()?;
    let parsed = chrono::DateTime::parse_from_rfc3339(stamp).ok()?.timestamp_millis();
    Some((now_ms - parsed).max(0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{Message, Tool};

    fn now() -> i64 {
        chrono::DateTime::parse_from_rfc3339("2026-11-08T12:00:00Z").unwrap().timestamp_millis()
    }

    fn session(state: &str, updated_at: &str) -> Session {
        Session {
            id: "s1".into(),
            agent_id: Some("ada".into()),
            state: Some(state.to_owned()),
            updated_at: Some(updated_at.to_owned()),
            ..Session::default()
        }
    }

    fn input<'a>(session: Option<&'a Session>, runtime: &'a str) -> StatusInput<'a> {
        StatusInput {
            connected: true,
            runtime_status: Some(runtime),
            provider: Some("claude"),
            session,
            permission_pending: false,
            composing: false,
            now_ms: now(),
        }
    }

    #[test]
    fn a_disconnected_daemon_or_a_missing_coding_agent_reads_offline() {
        let session = session("idle", "2026-11-08T11:59:00Z");
        let mut status = input(Some(&session), "ready");
        status.connected = false;
        assert_eq!(derive(&status).mood, Mood::Offline);
        let mut status = input(Some(&session), "ready");
        status.runtime_status = None;
        assert_eq!(derive(&status).label, "No runtime selected");
    }

    #[test]
    fn an_open_approval_outranks_a_busy_turn() {
        let session = session("waiting_permission", "2026-11-08T11:59:59Z");
        let status = derive(&input(Some(&session), "ready"));
        assert_eq!(status.mood, Mood::Permission);
        assert_eq!(status.label, "Approval needed");
    }

    #[test]
    fn a_working_turn_types_then_admits_it_is_waiting() {
        let fresh = session("working", "2026-11-08T11:59:59Z");
        assert_eq!(derive(&input(Some(&fresh), "ready")).label, "Typing…");
        let quiet = session("working", "2026-11-08T11:58:00Z");
        assert_eq!(derive(&input(Some(&quiet), "ready")).label, "Waiting for Claude Code…");
        // A running tool step says what it does.
        let mut busy = session("working", "2026-11-08T11:59:59Z");
        busy.tools.push(Tool { id: Some("t1".into()), kind: Some("shell".into()), title: Some("Run cargo test".into()), state: Some("running".into()), ..Tool::default() });
        assert_eq!(derive(&input(Some(&busy), "ready")).label, "Run cargo test");
        // Thinking is only shown when the newest message is the working note.
        let mut thinking = session("working", "2026-11-08T11:59:59Z");
        thinking.messages.push(Message { role: Some("thinking".into()), ..Message::default() });
        assert_eq!(derive(&input(Some(&thinking), "ready")).label, "Thinking");
    }

    #[test]
    fn a_finish_is_happy_for_six_seconds_and_then_the_bot_relaxes() {
        let just_done = session("completed", "2026-11-08T11:59:57Z");
        let status = derive(&input(Some(&just_done), "ready"));
        assert_eq!((status.mood, status.label.as_str()), (Mood::Success, "Done"));
        let long_ago = session("completed", "2026-11-08T11:50:00Z");
        let relaxed = derive(&input(Some(&long_ago), "ready"));
        assert_eq!((relaxed.mood, relaxed.label.as_str()), (Mood::Sleeping, "Sleeping"));
    }

    #[test]
    fn typing_in_the_composer_says_it_is_ready_for_the_message() {
        let idle = session("idle", "2026-11-08T11:59:59Z");
        let mut status = input(Some(&idle), "ready");
        status.composing = true;
        assert_eq!(derive(&status).label, "Ready for your message");
        assert_eq!(Mood::Sleeping.compact_label(), Some("Sleeping"));
        assert_eq!(Mood::Working.compact_label(), None);
    }

    #[test]
    fn an_unknown_timestamp_still_reads_as_online_rather_than_sleeping() {
        let no_stamp = Session { id: "s1".into(), state: Some("idle".into()), ..Session::default() };
        let status = derive(&input(Some(&no_stamp), "ready"));
        assert_eq!((status.mood, status.label.as_str()), (Mood::Online, "Online"));
        assert_eq!(session_idle_ms(Some(&no_stamp), now()), None);
    }
}
