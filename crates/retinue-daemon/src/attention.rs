//! Lane 6 (11 November 2026), task A: what the owner is looking at, and when
//! they last typed there.
//!
//! The rule the owner asked for is "one surface speaks" (CNVS's Distraction
//! Guard, `docs/research/ade-ux-and-delegation-2026-11.md` §4.4). The window
//! decides its own approval card, bell row and desktop notification, so a
//! daemon-side alert (the phone push today) must stay silent in two cases:
//!
//! * the desktop reports that conversation as open **and** visible, and
//! * for routine events only, the owner typed into that conversation in the
//!   last minute. An approval, a question or a failure is never held back by
//!   typing, because the owner needs it wherever they are.
//!
//! [`should_alert`] is that one decision, and every daemon-side alert asks it.
//! Nothing here is stored or logged: the desktop reports focus on every change
//! and as a heartbeat, and a report older than [`FOCUS_TTL`] is ignored, so a
//! window that closed or crashed cannot keep a phone silent for ever. With no
//! report at all the daemon behaves exactly as it did before this module.
//!
//! Task B lives here as well: [`wake_if_snoozed`] and [`owner_sent`] clear the
//! conversation's own snooze (the `snoozed_until_needed` column, which is *not*
//! the usage-limit `limit_snoozed`), and [`should_alert`] keeps a snoozed
//! conversation's progress quiet.

use crate::AppState;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// How long one focus report stands without a heartbeat from the window.
pub const FOCUS_TTL: Duration = Duration::from_secs(90);
/// How long routine events stay quiet after the owner typed in a conversation.
pub const TYPED_QUIET: Duration = Duration::from_secs(60);

/// What one event means for the owner, if anything.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Urgency {
    /// An approval, a question or a failed turn: the owner has to act.
    NeedsOwner,
    /// A turn finished or a reply arrived: progress.
    Routine,
}

/// How urgent one event is, or `None` for the many that are neither. The same
/// two kinds the phone push carries are the ones that need the owner.
pub fn urgency(kind: &str, payload: &Value) -> Option<Urgency> {
    match kind {
        "permission.requested" | "turn.error" => Some(Urgency::NeedsOwner),
        "turn.completed" | "turn.cancelled" => Some(Urgency::Routine),
        "message.completed" if payload["role"].as_str() == Some("assistant") => Some(Urgency::Routine),
        _ => None,
    }
}

/// What the desktop last said. One record: the install has one desktop window.
struct Report {
    session_id: Option<String>,
    visible: bool,
    at: Instant,
}

/// The desktop's focus report and when the owner last typed. Memory only, so a
/// restart forgets both and behaves as it did before this module existed.
#[derive(Default)]
pub struct Attention {
    report: Mutex<Option<Report>>,
    typed: Mutex<HashMap<String, Instant>>,
}

impl Attention {
    /// The window's latest report: the conversation it shows (if any) and
    /// whether it has keyboard focus. `at` is the report's own clock, so a test
    /// can age it.
    pub fn report_focus(&self, session_id: Option<String>, visible: bool, at: Instant) {
        if let Ok(mut slot) = self.report.lock() {
            *slot = Some(Report { session_id, visible, at });
        }
    }

    /// The owner sent a message or a steer into this conversation.
    pub fn note_typed(&self, session_id: &str, at: Instant) {
        let Ok(mut typed) = self.typed.lock() else { return };
        typed.retain(|_, when| at.saturating_duration_since(*when) < TYPED_QUIET);
        typed.insert(session_id.to_string(), at);
    }

    /// Whether the desktop is showing this conversation with focus right now.
    pub fn focused(&self, session_id: &str, now: Instant) -> bool {
        let Ok(slot) = self.report.lock() else { return false };
        slot.as_ref().is_some_and(|report| {
            report.visible
                && report.session_id.as_deref() == Some(session_id)
                && now.saturating_duration_since(report.at) < FOCUS_TTL
        })
    }

    /// Whether the owner typed in this conversation a moment ago.
    pub fn typed_recently(&self, session_id: &str, now: Instant) -> bool {
        self.typed
            .lock()
            .ok()
            .and_then(|typed| typed.get(session_id).copied())
            .is_some_and(|when| now.saturating_duration_since(when) < TYPED_QUIET)
    }
}

/// Whether the daemon should speak about this event in this conversation.
///
/// Focus silences everything, because the window's own card is the alert.
/// Typing silences only routine events. A snoozed conversation's progress is
/// silent as well; what needs the owner always gets through, and
/// [`wake_if_snoozed`] has already cleared the flag by then.
pub fn should_alert(st: &AppState, kind: &str, payload: &Value, session_id: &str) -> bool {
    let Some(urgency) = urgency(kind, payload) else { return false };
    let now = Instant::now();
    if st.attention.focused(session_id, now) { return false; }
    if urgency == Urgency::Routine {
        if st.attention.typed_recently(session_id, now) { return false; }
        if st.db.session_snoozed(session_id).unwrap_or(false) { return false; }
    }
    true
}

/// Task B: the first approval, a question or a failure in a snoozed conversation
/// wakes it. Returns the summary to announce, which `emit` sends after the event
/// itself (a wake must not call `emit`, or the two would recurse).
pub fn wake_if_snoozed(st: &AppState, kind: &str, payload: &Value) -> Option<Value> {
    if urgency(kind, payload) != Some(Urgency::NeedsOwner) { return None; }
    let session_id = payload["sessionId"].as_str().filter(|id| !id.is_empty())?;
    if !st.db.session_snoozed(session_id).unwrap_or(false) { return None; }
    st.db.session_set_snoozed(session_id, false).ok()?;
    st.db.session_event_summary(session_id).ok()
}

/// The owner sent this conversation a message or a steer: its routine events
/// stay quiet for a minute, and a snooze is over.
pub async fn owner_sent(st: &AppState, session_id: &str) {
    st.attention.note_typed(session_id, Instant::now());
    if !st.db.session_snoozed(session_id).unwrap_or(false) { return; }
    if st.db.session_set_snoozed(session_id, false).is_ok() {
        if let Ok(summary) = st.db.session_event_summary(session_id) {
            st.emit("session.changed", summary).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn focus_stands_for_ninety_seconds_and_only_while_visible() {
        let attention = Attention::default();
        let now = Instant::now();
        assert!(!attention.focused("s", now), "nothing reported: not focused");
        attention.report_focus(Some("s".into()), true, now - Duration::from_secs(89));
        assert!(attention.focused("s", now), "a fresh report with visibility counts");
        assert!(!attention.focused("other", now), "another conversation is not focused");
        // The same report a second past the deadline is forgotten.
        attention.report_focus(Some("s".into()), true, now - FOCUS_TTL - Duration::from_secs(1));
        assert!(!attention.focused("s", now));
        // A window in the background reports the conversation but not visibility.
        attention.report_focus(Some("s".into()), false, now);
        assert!(!attention.focused("s", now));
        // Nothing open at all.
        attention.report_focus(None, true, now);
        assert!(!attention.focused("s", now));
    }

    #[test]
    fn typing_quiet_is_sixty_seconds_and_is_forgotten_soon_after() {
        let attention = Attention::default();
        let now = Instant::now();
        assert!(!attention.typed_recently("s", now));
        attention.note_typed("s", now - Duration::from_secs(59));
        assert!(attention.typed_recently("s", now));
        assert!(!attention.typed_recently("other", now));
        attention.note_typed("s", now - TYPED_QUIET - Duration::from_secs(1));
        assert!(!attention.typed_recently("s", now));
    }

    #[test]
    fn only_the_events_the_owner_acted_on_or_waited_for_carry_an_urgency() {
        assert_eq!(urgency("permission.requested", &json!({"sessionId": "s"})), Some(Urgency::NeedsOwner));
        assert_eq!(urgency("turn.error", &json!({"sessionId": "s"})), Some(Urgency::NeedsOwner));
        assert_eq!(urgency("turn.completed", &json!({"sessionId": "s"})), Some(Urgency::Routine));
        assert_eq!(urgency("message.completed", &json!({"role": "assistant"})), Some(Urgency::Routine));
        assert_eq!(urgency("message.completed", &json!({"role": "user"})), None, "the owner's own message is not progress");
        assert_eq!(urgency("message.delta", &json!({"role": "assistant"})), None);
        assert_eq!(urgency("session.changed", &json!({"id": "s"})), None);
    }

    #[tokio::test]
    async fn a_focused_conversation_silences_everything_and_typing_only_the_routine() {
        let (st, _events) = crate::phase2a_tests::state();
        st.db.create_session("s", "rt-test", "codex", ".", "Chat").unwrap();
        let approval = json!({"sessionId": "s"});
        let progress = json!({"sessionId": "s"});
        // No report and no typing: exactly as before.
        assert!(should_alert(&st, "permission.requested", &approval, "s"));
        assert!(should_alert(&st, "turn.completed", &progress, "s"));
        // Focused and visible: the card is the alert.
        st.attention.report_focus(Some("s".into()), true, Instant::now());
        assert!(!should_alert(&st, "permission.requested", &approval, "s"));
        assert!(!should_alert(&st, "turn.completed", &progress, "s"));
        // A hidden window is not the card: the approval speaks again.
        st.attention.report_focus(Some("s".into()), false, Instant::now());
        assert!(should_alert(&st, "permission.requested", &approval, "s"));
        // The owner typed a moment ago: progress stays quiet, the approval does not.
        st.attention.note_typed("s", Instant::now());
        assert!(!should_alert(&st, "turn.completed", &progress, "s"));
        assert!(should_alert(&st, "permission.requested", &approval, "s"));
        assert!(should_alert(&st, "turn.error", &approval, "s"));
        // Once the minute is over, progress speaks again.
        st.attention.note_typed("s", Instant::now() - TYPED_QUIET - Duration::from_secs(1));
        assert!(should_alert(&st, "turn.completed", &progress, "s"));
    }

    #[tokio::test]
    async fn snooze_holds_progress_and_a_needs_owner_event_wakes_the_conversation() {
        let (st, _events) = crate::phase2a_tests::state();
        st.db.create_session("s", "rt-test", "codex", ".", "Chat").unwrap();
        assert!(st.db.session_set_snoozed("s", true).unwrap());
        let progress = json!({"sessionId": "s"});
        assert!(!should_alert(&st, "turn.completed", &progress, "s"), "a snoozed conversation's completion is quiet");
        assert!(!should_alert(&st, "message.completed", &json!({"sessionId": "s", "role": "assistant"}), "s"));
        // An approval wakes it: `emit` clears the flag first, and it alerts.
        let approval = json!({"sessionId": "s"});
        let summary = wake_if_snoozed(&st, "permission.requested", &approval).expect("the wake is announced");
        assert_eq!(summary["id"], json!("s"));
        assert!(summary.get("snoozed").is_none(), "the summary no longer carries the flag");
        assert!(!st.db.session_snoozed("s").unwrap());
        assert!(should_alert(&st, "permission.requested", &approval, "s"));
        // A routine event never wakes it.
        assert!(st.db.session_set_snoozed("s", true).unwrap());
        assert!(wake_if_snoozed(&st, "turn.completed", &progress).is_none());
        assert!(st.db.session_snoozed("s").unwrap());
        // The owner's own message wakes it too.
        owner_sent(&st, "s").await;
        assert!(!st.db.session_snoozed("s").unwrap());
    }
}
