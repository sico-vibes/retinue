//! Who needs you: the companion's attention state
//! (`CompanionAttention.tsx`, and `blobState` in `CompanionTeam.tsx`).
//!
//! The window shows one pill — "2 need you" — and, when an approval is waiting,
//! the request itself with Allow and Deny, through the same `permission.reply`
//! RPC the main window uses. "Later" sets one request aside for five minutes.

#![allow(dead_code)]  // A ported surface: the helpers are complete and
// slice 6's settings page and slice 7 wire the rest.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use crate::state::{Agent, Session, Snapshot};

/// Five minutes, the companion's "Later".
pub const LATER_HOLD: Duration = Duration::from_secs(5 * 60);

/// What a bot's status dot says (`BlobState`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlobState {
    Waiting,
    Working,
    Idle,
    Offline,
}

impl BlobState {
    pub fn key(self) -> &'static str {
        match self {
            BlobState::Waiting => "waiting",
            BlobState::Working => "working",
            BlobState::Idle => "idle",
            BlobState::Offline => "offline",
        }
    }

    /// The one line a teammate shows: what it needs, or what it is doing.
    pub fn line(self) -> &'static str {
        match self {
            BlobState::Waiting => "Waiting for you",
            BlobState::Working => "Working…",
            BlobState::Offline => "Offline",
            BlobState::Idle => "Ready",
        }
    }
}

/// The bot's own conversations, newest first, without side conversations or
/// archived ones (`mainSessionFor` plus its side sessions).
fn own_sessions<'a>(snapshot: &'a Snapshot, agent_id: &str) -> Vec<&'a Session> {
    let mut rows: Vec<&Session> = snapshot
        .sessions
        .iter()
        .filter(|session| session.agent_id.as_deref() == Some(agent_id) && !session.archived)
        .collect();
    rows.sort_by(|a, b| b.stamp().cmp(a.stamp()));
    rows
}

/// `blobState`: offline when the coding agent cannot run, waiting while an
/// approval is open, working while any of its turns is busy, else idle.
pub fn blob_state(snapshot: &Snapshot, agent: &Agent, connected: bool) -> BlobState {
    let runtime = snapshot.runtimes.iter().find(|runtime| runtime.id == agent.runtime_id);
    // A coding agent that is not installed or not signed in is offline, the same
    // four words `Runtime::is_usable` and the React `blobState` refuse.
    let runtime_ok = runtime
        .map(|runtime| {
            !matches!(
                runtime.status.as_deref().unwrap_or("").to_lowercase().as_str(),
                "offline" | "error" | "disconnected" | "missing" | "unavailable"
            )
        })
        .unwrap_or(false);
    if !connected || !runtime_ok {
        return BlobState::Offline;
    }
    let own = own_sessions(snapshot, &agent.id);
    if own.iter().any(|session| session.state_word() == "waiting_permission") {
        return BlobState::Waiting;
    }
    if own.iter().any(|session| session.is_busy()) {
        return BlobState::Working;
    }
    BlobState::Idle
}

/// Every bot waiting for the owner, in the sidebar's own order (by name).
pub fn waiting_agents(snapshot: &Snapshot, connected: bool) -> Vec<&Agent> {
    let mut waiting: Vec<&Agent> = snapshot
        .agents
        .iter()
        .filter(|agent| agent.is_employee() && blob_state(snapshot, agent, connected) == BlobState::Waiting)
        .collect();
    waiting.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    waiting
}

/// The next waiting bot after `focused`, wrapping around; the first one when the
/// focused bot is not waiting.
#[allow(clippy::needless_lifetimes)] // the slice's lifetime is the answer's
pub fn next_waiting<'a>(waiting: &[&'a Agent], focused: Option<&str>) -> Option<&'a Agent> {
    if waiting.is_empty() {
        return None;
    }
    let at = focused.and_then(|id| waiting.iter().position(|agent| agent.id == id));
    match at {
        Some(at) => waiting.get((at + 1) % waiting.len()).copied(),
        None => waiting.first().copied(),
    }
}

/// "1 needs you" / "2 need you", or nothing at zero so a quiet companion stays
/// quiet.
pub fn needs_you_text(count: usize) -> Option<String> {
    match count {
        0 => None,
        1 => Some("1 needs you".to_owned()),
        count => Some(format!("{count} need you")),
    }
}

/// One arrow step through the queue, wrapping at both ends.
pub fn step_index(index: usize, total: usize, forward: bool) -> usize {
    if total == 0 {
        return 0;
    }
    if forward {
        (index + 1) % total
    } else {
        (index + total - 1) % total
    }
}

/// The request the companion offers for an answer: only a plain approval can be
/// answered with Allow and Deny.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Approval {
    pub id: String,
    pub session_id: Option<String>,
    pub agent_id: Option<String>,
    pub title: String,
    pub detail: String,
    /// The choice a click on Allow sends, from the request's own list.
    pub allow: String,
    /// The choice a click on Deny sends.
    pub deny: String,
}

/// Which choice is "Allow": the request's own wording decides (`allow_once`,
/// `approve`, `accept`, `yes`), and anything else falls back to its first
/// choice, exactly as the main window's cards do.
pub fn allow_choice(choices: &[String]) -> String {
    choices
        .iter()
        .find(|choice| {
            let word = choice.to_lowercase();
            word.contains("allow") || word.contains("approve") || word.contains("accept") || word == "yes" || word == "once"
        })
        .cloned()
        .or_else(|| choices.first().cloned())
        .unwrap_or_else(|| "allow_once".to_owned())
}

/// Which choice is "Deny", the other half of the same rule.
pub fn deny_choice(choices: &[String]) -> String {
    choices
        .iter()
        .find(|choice| {
            let word = choice.to_lowercase();
            word.contains("deny") || word.contains("reject") || word.contains("decline") || word == "no" || word.contains("cancel")
        })
        .cloned()
        .unwrap_or_else(|| "deny".to_owned())
}

/// The approvals the companion can answer, oldest request first, without the
/// ones the owner set aside with "Later".
pub fn approvals(snapshot: &Snapshot, held: &BTreeMap<String, Instant>, now: Instant) -> Vec<Approval> {
    snapshot
        .permissions
        .iter()
        .filter(|permission| !permission.is_question() && !permission.is_proposal() && !is_held(held, &permission.id, now))
        .map(|permission| {
            let session = permission
                .session_id
                .as_deref()
                .and_then(|id| snapshot.session(id));
            Approval {
                id: permission.id.clone(),
                session_id: permission.session_id.clone(),
                agent_id: session.and_then(|session| session.agent_id.clone()),
                title: permission.title.clone().unwrap_or_else(|| "Approval needed".to_owned()),
                detail: permission.detail.clone().unwrap_or_default(),
                allow: allow_choice(&permission.choices),
                deny: deny_choice(&permission.choices),
            }
        })
        .collect()
}

/// Whether a request is still set aside.
pub fn is_held(held: &BTreeMap<String, Instant>, permission_id: &str, now: Instant) -> bool {
    held.get(permission_id).map(|until| *until > now).unwrap_or(false)
}

/// Set one request aside for five minutes, and drop the holds that have run out.
pub fn hold(held: &mut BTreeMap<String, Instant>, permission_id: &str, now: Instant) {
    held.retain(|_, until| *until > now);
    held.insert(permission_id.to_owned(), now + LATER_HOLD);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{Look, Permission};
    use serde_json::json;

    fn agent(id: &str, name: &str) -> Agent {
        Agent {
            id: id.into(),
            name: name.into(),
            runtime_id: "claude".into(),
            look: Some(Look { icon: None }),
            system_role: Some("employee".into()),
            ..Agent::default()
        }
    }

    fn snapshot_with(states: &[(&str, &str)]) -> Snapshot {
        let mut snapshot = Snapshot::default();
        snapshot.runtimes.push(crate::state::Runtime {
            id: "claude".into(),
            provider: "claude".into(),
            status: Some("ready".into()),
            auth_state: Some("signed_in".into()),
            ..crate::state::Runtime::default()
        });
        for (agent_id, state) in states {
            snapshot.agents.push(agent(agent_id, agent_id));
            let mut session = Session { id: format!("{agent_id}-1"), agent_id: Some((*agent_id).into()), ..Session::default() };
            session.state = Some((*state).to_owned());
            session.updated_at = Some("2026-11-08T10:00:00Z".into());
            snapshot.sessions.push(session);
        }
        snapshot
    }

    #[test]
    fn a_waiting_approval_puts_its_bot_in_the_attention_list() {
        let snapshot = snapshot_with(&[("ada", "waiting_permission"), ("bo", "working"), ("cy", "idle")]);
        let waiting: Vec<&str> = waiting_agents(&snapshot, true).iter().map(|agent| agent.id.as_str()).collect();
        assert_eq!(waiting, vec!["ada"]);
        assert_eq!(blob_state(&snapshot, &agent("bo", "bo"), true), BlobState::Working);
        assert_eq!(blob_state(&snapshot, &agent("cy", "cy"), true), BlobState::Idle);
    }

    #[test]
    fn a_disconnected_daemon_makes_every_bot_offline() {
        let snapshot = snapshot_with(&[("ada", "waiting_permission")]);
        assert_eq!(blob_state(&snapshot, &agent("ada", "ada"), false), BlobState::Offline);
        assert!(waiting_agents(&snapshot, false).is_empty());
        // A coding agent that cannot run is offline even while connected.
        let mut broken = snapshot.clone();
        broken.runtimes[0].status = Some("missing".into());
        assert_eq!(blob_state(&broken, &agent("ada", "ada"), true), BlobState::Offline);
    }

    #[test]
    fn the_badge_counts_and_the_pill_jumps_to_the_next_waiting_bot() {
        assert_eq!(needs_you_text(0), None);
        assert_eq!(needs_you_text(1).unwrap(), "1 needs you");
        assert_eq!(needs_you_text(3).unwrap(), "3 need you");
        let snapshot = snapshot_with(&[("ada", "waiting_permission"), ("bo", "waiting_permission")]);
        let waiting = waiting_agents(&snapshot, true);
        assert_eq!(next_waiting(&waiting, Some("ada")).unwrap().id, "bo");
        assert_eq!(next_waiting(&waiting, Some("bo")).unwrap().id, "ada", "wraps around");
        assert_eq!(next_waiting(&waiting, Some("cy")).unwrap().id, "ada");
        assert!(next_waiting(&[], None).is_none());
    }

    #[test]
    fn the_queue_steps_both_ways_and_wraps() {
        assert_eq!(step_index(0, 3, true), 1);
        assert_eq!(step_index(2, 3, true), 0);
        assert_eq!(step_index(0, 3, false), 2);
        assert_eq!(step_index(0, 0, true), 0);
    }

    #[test]
    fn allow_and_deny_come_from_the_requests_own_choices() {
        let choices = vec!["allow_once".to_string(), "allow_always".to_string(), "deny".to_string()];
        assert_eq!(allow_choice(&choices), "allow_once");
        assert_eq!(deny_choice(&choices), "deny");
        let unusual = vec!["yes".to_string(), "no".to_string()];
        assert_eq!(allow_choice(&unusual), "yes");
        assert_eq!(deny_choice(&unusual), "no");
        // A request with no wording still sends something the daemon knows.
        assert_eq!(allow_choice(&[]), "allow_once");
        assert_eq!(deny_choice(&[]), "deny");
    }

    #[test]
    fn a_question_or_a_proposal_is_not_an_allow_deny_card() {
        let mut snapshot = snapshot_with(&[("ada", "waiting_permission")]);
        snapshot.permissions.push(Permission {
            id: "p1".into(),
            session_id: Some("ada-1".into()),
            title: Some("Run the tests".into()),
            choices: vec!["allow_once".into(), "deny".into()],
            ..Permission::default()
        });
        snapshot.permissions.push(Permission {
            id: "p2".into(),
            session_id: Some("ada-1".into()),
            title: Some("Which folder?".into()),
            kind: Some("question".into()),
            choices: vec![],
            ..Permission::default()
        });
        snapshot.permissions.push(Permission {
            id: "p3".into(),
            session_id: Some("ada-1".into()),
            title: Some("Hire?".into()),
            kind: Some("proposal".into()),
            proposal: Some(crate::state::Proposal { id: "x".into(), kind: "hire".into(), payload: Some(json!({})) }),
            ..Permission::default()
        });
        let rows = approvals(&snapshot, &BTreeMap::new(), Instant::now());
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "p1");
        assert_eq!(rows[0].agent_id.as_deref(), Some("ada"));
        assert_eq!(rows[0].allow, "allow_once");
    }

    #[test]
    fn later_sets_a_request_aside_for_five_minutes() {
        let now = Instant::now();
        let mut snapshot = snapshot_with(&[("ada", "waiting_permission")]);
        snapshot.permissions.push(Permission {
            id: "p1".into(),
            session_id: Some("ada-1".into()),
            choices: vec!["allow_once".into(), "deny".into()],
            ..Permission::default()
        });
        let mut held = BTreeMap::new();
        hold(&mut held, "p1", now);
        assert!(is_held(&held, "p1", now + Duration::from_secs(60)));
        assert!(approvals(&snapshot, &held, now + Duration::from_secs(60)).is_empty(), "hidden for five minutes");
        assert_eq!(approvals(&snapshot, &held, now + LATER_HOLD).len(), 1, "then it comes back");
    }
}
