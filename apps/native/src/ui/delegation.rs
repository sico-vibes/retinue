//! The manager's delegations: the ledger the daemon keeps for every assignment a
//! bot sends to a teammate. Ported from `ui/delegations.ts`, `ui/DelegationGroup.tsx`
//! and `ui/SubbotViews.tsx`, as far as one rail card can show them.
//!
//! React draws these inline in the conversation, which needs a hook in
//! `ui/conversation.rs` (the Director's file), so this window lists the same rows
//! in the session rail: the daemon's own status, the teammate, the elapsed time
//! and a Stop. Nothing is inferred: an assignment the ledger did not record is
//! simply not shown here.

use gpui::Context;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::state::Session;

/// One assignment (`DelegationRecord` in `delegations.ts`).
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct DelegationRecord {
    pub id: String,
    pub origin_session_id: String,
    pub origin_turn_id: Option<String>,
    pub side_session_id: String,
    pub side_turn_id: Option<String>,
    pub from_agent_id: Option<String>,
    pub target_agent_id: Option<String>,
    pub topic: String,
    pub brief_preview: String,
    /// `queued`, `running`, `completed`, `failed`, `cancelled` or `interrupted`.
    pub status: String,
    pub error: Option<String>,
    pub workspace_path: Option<String>,
    pub created_at: Option<String>,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub updated_at: Option<String>,
    /// `busy` or `slots`: why a queued assignment has not started.
    pub queue_reason: Option<String>,
    /// The teammate replied `(pass)`.
    pub silent: bool,
    pub done_goal: Option<String>,
}

impl DelegationRecord {
    pub fn is_active(&self) -> bool {
        matches!(self.status.as_str(), "queued" | "running")
    }

    /// The row's status, in the daemon's own words, upgraded to "Needs approval"
    /// only while the teammate's conversation is waiting for one.
    pub fn label(&self, side: Option<&Session>, asking: bool) -> String {
        match self.status.as_str() {
            "queued" => match self.queue_reason.as_deref() {
                Some("busy") => "Queued · bot is busy".to_owned(),
                Some("slots") => "Queued · no free slot".to_owned(),
                _ => "Queued".to_owned(),
            },
            "running" => {
                if asking {
                    "Has a question".to_owned()
                } else if side.is_some_and(Session::is_busy) {
                    "Needs approval".to_owned()
                } else {
                    "Working".to_owned()
                }
            }
            "completed" if self.silent => "Done, nothing to report".to_owned(),
            "completed" => "Done".to_owned(),
            "failed" => "Failed".to_owned(),
            "cancelled" => "Cancelled".to_owned(),
            "interrupted" => "Interrupted".to_owned(),
            other => other.to_owned(),
        }
    }

    /// Waiting or working time while it is open; frozen at the daemon's finish
    /// time once it is not.
    pub fn elapsed(&self, now_ms: i64) -> String {
        let Some(started) = self.started_at.as_deref().and_then(|value| crate::state::tasks::parse_time(Some(value))) else {
            return "—".to_owned();
        };
        let end = if self.is_active() {
            now_ms
        } else {
            self.finished_at.as_deref().and_then(|value| crate::state::tasks::parse_time(Some(value))).unwrap_or(now_ms)
        };
        duration_text((end - started).max(0))
    }
}

fn duration_text(ms: i64) -> String {
    let seconds = (ms / 1000).max(0);
    if seconds < 60 {
        format!("{seconds}s")
    } else if seconds < 3_600 {
        format!("{}m {:02}s", seconds / 60, seconds % 60)
    } else {
        format!("{}h {}m", seconds / 3_600, (seconds % 3_600) / 60)
    }
}

/// Accepts only a well-formed record; anything else is dropped rather than
/// guessed at.
pub fn normalize_delegation(raw: &Value) -> Option<DelegationRecord> {
    if !raw.is_object() {
        return None;
    }
    let record: DelegationRecord = serde_json::from_value(raw.clone()).ok()?;
    if record.id.is_empty() || record.origin_session_id.is_empty() || record.side_session_id.is_empty() {
        return None;
    }
    if !matches!(record.status.as_str(), "queued" | "running" | "completed" | "failed" | "cancelled" | "interrupted") {
        return None;
    }
    Some(record)
}

/// One line for the header: "2 working · 1 done".
pub fn summarize(records: &[DelegationRecord]) -> String {
    let count = |status: &str| records.iter().filter(|record| record.status == status).count();
    [
        (count("running"), "working"),
        (count("queued"), "queued"),
        (count("failed"), "failed"),
        (count("interrupted"), "interrupted"),
        (count("cancelled"), "cancelled"),
        (count("completed"), "done"),
    ]
    .into_iter()
    .filter(|(count, _)| *count > 0)
    .map(|(count, label)| format!("{count} {label}"))
    .collect::<Vec<_>>()
    .join(" · ")
}

/// The window's own state for the ledger.
#[derive(Default)]
pub struct DelegationsState {
    pub records: Vec<DelegationRecord>,
    pub loaded: bool,
    pub available: bool,
    pub origin: Option<String>,
}

impl DelegationsState {
    pub fn for_session(&self, session_id: &str) -> Vec<DelegationRecord> {
        let mut rows: Vec<DelegationRecord> =
            self.records.iter().filter(|record| record.origin_session_id == session_id).cloned().collect();
        rows.sort_by(|a, b| b.created_at.clone().unwrap_or_default().cmp(&a.created_at.clone().unwrap_or_default()));
        rows
    }
}

impl crate::ui::root::Root {
    /// Read the ledger for the open conversation, once per conversation.
    pub(crate) fn request_delegations(&mut self, cx: &mut Context<Self>) {
        let Some(session_id) = self.session_id.clone() else { return };
        if self.delegations.origin.as_deref() == Some(session_id.as_str()) && self.delegations.loaded {
            return;
        }
        self.delegations.origin = Some(session_id.clone());
        self.delegations.loaded = true;
        self.daemon_command(
            crate::daemon::Command::Rpc {
                tag: "delegations".to_owned(),
                method: "delegation.list".to_owned(),
                params: json!({ "originSessionId": session_id }),
            },
            cx,
        );
    }

    /// `delegation.updated`: one row changed.
    pub(crate) fn delegation_event(&mut self, event: &crate::daemon::client::DaemonEvent, cx: &mut Context<Self>) {
        if event.event_type != "delegation.updated" {
            return;
        }
        let Some(raw) = event.payload.get("delegation") else { return };
        let Some(record) = normalize_delegation(raw) else { return };
        if Some(&record.origin_session_id) != self.delegations.origin.as_ref() {
            return;
        }
        match self.delegations.records.iter_mut().find(|existing| existing.id == record.id) {
            Some(existing) => {
                if record.updated_at >= existing.updated_at {
                    *existing = record;
                }
            }
            None => self.delegations.records.push(record),
        }
        cx.notify();
    }

    pub(crate) fn delegations_rpc(&mut self, tag: &str, value: &Value, cx: &mut Context<Self>) -> bool {
        match tag {
            "delegations" => {
                let rows = value.get("delegations").and_then(Value::as_array).cloned().unwrap_or_default();
                self.delegations.records = rows.iter().filter_map(normalize_delegation).collect();
                self.delegations.available = true;
            }
            "delegation-cancel" => {}
            _ => return false,
        }
        cx.notify();
        true
    }

    pub(crate) fn delegations_rpc_failed(&mut self, tag: &str, _message: &str, cx: &mut Context<Self>) -> bool {
        match tag {
            // An older daemon without the ledger answers `not_found`: the card
            // simply does not appear, as React's `available` does.
            "delegations" => self.delegations.available = false,
            "delegation-cancel" => {}
            _ => return false,
        }
        cx.notify();
        true
    }

    /// `delegation.cancel`: stop one assignment.
    pub(crate) fn cancel_delegation(&mut self, id: String, cx: &mut Context<Self>) {
        self.daemon_command(
            crate::daemon::Command::Rpc {
                tag: "delegation-cancel".to_owned(),
                method: "delegation.cancel".to_owned(),
                params: json!({ "delegationId": id }),
            },
            cx,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(status: &str) -> DelegationRecord {
        DelegationRecord {
            id: "d1".into(),
            origin_session_id: "s1".into(),
            side_session_id: "side".into(),
            status: status.into(),
            created_at: Some("2026-11-08T10:00:00Z".into()),
            started_at: Some("2026-11-08T10:00:00Z".into()),
            ..DelegationRecord::default()
        }
    }

    #[test]
    fn a_record_without_its_ids_is_dropped() {
        assert!(normalize_delegation(&json!({ "id": "d1", "status": "running" })).is_none());
        assert!(normalize_delegation(&json!({ "id": "d1", "originSessionId": "s1", "sideSessionId": "x", "status": "nonsense" })).is_none());
        let record = normalize_delegation(&json!({
            "id": "d1", "originSessionId": "s1", "sideSessionId": "x", "status": "running", "topic": "Fix the tests"
        }))
        .expect("a record");
        assert_eq!(record.topic, "Fix the tests");
    }

    #[test]
    fn a_queued_row_says_what_it_waits_for() {
        let mut queued = record("queued");
        assert_eq!(queued.label(None, false), "Queued");
        queued.queue_reason = Some("busy".into());
        assert_eq!(queued.label(None, false), "Queued · bot is busy");
        queued.queue_reason = Some("slots".into());
        assert_eq!(queued.label(None, false), "Queued · no free slot");
    }

    #[test]
    fn a_running_row_is_working_unless_the_teammate_is_waiting() {
        let running = record("running");
        assert_eq!(running.label(None, false), "Working");
        assert_eq!(running.label(None, true), "Has a question");
        let waiting = Session { state: Some("waiting_permission".into()), ..Session::default() };
        assert_eq!(running.label(Some(&waiting), false), "Needs approval");
    }

    #[test]
    fn a_silent_finish_is_not_the_same_as_a_report() {
        let mut done = record("completed");
        assert_eq!(done.label(None, false), "Done");
        done.silent = true;
        assert_eq!(done.label(None, false), "Done, nothing to report");
    }

    #[test]
    fn elapsed_time_is_frozen_once_the_assignment_ends() {
        let mut running = record("running");
        let start = crate::state::tasks::parse_time(Some("2026-11-08T10:00:00Z")).unwrap();
        assert_eq!(running.elapsed(start + 65_000), "1m 05s");
        running.status = "completed".into();
        running.finished_at = Some("2026-11-08T10:00:30Z".into());
        assert_eq!(running.elapsed(start + 600_000), "30s", "the finish time wins");
        running.started_at = None;
        assert_eq!(running.elapsed(start), "—");
    }

    #[test]
    fn the_summary_counts_the_buckets_it_has() {
        let rows = vec![record("running"), record("completed"), record("running")];
        assert_eq!(summarize(&rows), "2 working · 1 done");
        assert_eq!(summarize(&[]), "");
    }
}
