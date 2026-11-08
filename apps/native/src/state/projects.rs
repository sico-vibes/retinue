//! The Projects tree, ported from `apps/desktop/src/ui/ProjectsSidebar.tsx`
//! (`projectBlocks`, `conversationStatus`) with the same grouping and ordering.
//!
//! Projects newest first; under each, its employees newest first; under a hired
//! employee, its conversations newest first. Membership comes from the daemon's
//! hiring table (`project.members`) when the window has it, and from
//! conversations and default projects when it does not.

use std::collections::BTreeMap;

use super::{Agent, Project, Session};

pub const RECENT_PER_BOT: usize = 4;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BotInProject {
    pub agent_id: String,
    pub session_ids: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct ProjectBlock {
    pub key: String,
    pub project: Option<Project>,
    pub bots: Vec<BotInProject>,
    pub latest: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Working,
    Needs,
    Limited,
    Bad,
    Idle,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConversationStatus {
    pub label: &'static str,
    pub tone: Tone,
}

/// What a conversation is doing now, in words for the card's second line.
pub fn conversation_status(session: &Session, needs_you: bool) -> ConversationStatus {
    let state = session.state_word().to_lowercase();
    if needs_you || state == "waiting_permission" {
        return ConversationStatus { label: "Needs you", tone: Tone::Needs };
    }
    if session.is_busy() {
        return ConversationStatus { label: "Working", tone: Tone::Working };
    }
    if state == "limited" || state == "rate_limited" {
        return ConversationStatus { label: "Limited", tone: Tone::Limited };
    }
    if state == "error" || state == "failed" {
        return ConversationStatus { label: "Stopped with an error", tone: Tone::Bad };
    }
    ConversationStatus { label: "Idle", tone: Tone::Idle }
}

/// The project a conversation works in: its stamped project, then its folder.
pub fn project_of_session<'a>(session: &Session, projects: &'a [Project]) -> Option<&'a Project> {
    if let Some(project_id) = session.project_id.as_deref() {
        return projects.iter().find(|project| project.id == project_id);
    }
    let path = session.project_path.as_deref()?.trim_end_matches(['\\', '/']).to_lowercase();
    if path.is_empty() {
        return None;
    }
    projects.iter().find(|project| {
        project
            .path
            .as_deref()
            .map(|project_path| project_path.trim_end_matches(['\\', '/']).to_lowercase() == path)
            .unwrap_or(false)
    })
}

/// Project id → the bots hired into it. The window keeps this from
/// `project.members`; `None` with an older daemon falls back to conversations.
pub type MembersByProject = BTreeMap<String, Vec<String>>;

pub fn project_blocks(
    agents: &[Agent],
    projects: &[Project],
    sessions: &[Session],
    members: Option<&MembersByProject>,
) -> Vec<ProjectBlock> {
    let known: BTreeMap<&str, &Agent> = agents.iter().map(|agent| (agent.id.as_str(), agent)).collect();
    let mut cells: BTreeMap<String, BTreeMap<String, Vec<String>>> = BTreeMap::new();
    let mut stamps: BTreeMap<String, String> = BTreeMap::new();

    let mut put = |key: &str, agent_id: &str, session: Option<&Session>| {
        let row = cells.entry(key.to_owned()).or_default();
        let list = row.entry(agent_id.to_owned()).or_default();
        if let Some(session) = session {
            list.push(session.id.clone());
            let stamp = session.stamp().to_owned();
            stamps.insert(session.id.clone(), stamp);
        }
    };

    for session in sessions {
        if session.archived || session.is_side() {
            continue;
        }
        let Some(agent_id) = session.agent_id.as_deref() else { continue };
        if !known.contains_key(agent_id) {
            continue;
        }
        let key = project_of_session(session, projects).map(|project| project.id.clone()).unwrap_or_else(|| "none".to_owned());
        put(&key, agent_id, Some(session));
    }
    for agent in agents {
        if let Some(project_id) = agent.project_id.as_deref() {
            if projects.iter().any(|project| project.id == project_id) {
                put(project_id, &agent.id, None);
            }
        }
    }
    if let Some(members) = members {
        for (project_id, list) in members {
            if !projects.iter().any(|project| &project.id == project_id) {
                continue;
            }
            for agent_id in list {
                if known.contains_key(agent_id.as_str()) {
                    put(project_id, agent_id, None);
                }
            }
        }
    }

    let stamp_of = |session_id: &String| stamps.get(session_id).cloned().unwrap_or_default();
    let mut blocks: Vec<ProjectBlock> = projects
        .iter()
        .map(|project| (Some(project.clone()), project.id.clone()))
        .chain(std::iter::once((None, "none".to_owned())))
        .map(|(project, key)| {
            let mut bots: Vec<BotInProject> = cells
                .get(&key)
                .map(|row| {
                    row.iter()
                        .map(|(agent_id, list)| {
                            let mut session_ids = list.clone();
                            session_ids.sort_by_key(|session_id| std::cmp::Reverse(stamp_of(session_id)));
                            BotInProject { agent_id: agent_id.clone(), session_ids }
                        })
                        .collect()
                })
                .unwrap_or_default();
            bots.sort_by(|a, b| {
                let (a_stamp, b_stamp) = (
                    a.session_ids.first().map(stamp_of).unwrap_or_default(),
                    b.session_ids.first().map(stamp_of).unwrap_or_default(),
                );
                let (a_name, b_name) = (
                    known.get(a.agent_id.as_str()).map(|agent| agent.name.clone()).unwrap_or_default(),
                    known.get(b.agent_id.as_str()).map(|agent| agent.name.clone()).unwrap_or_default(),
                );
                b_stamp.cmp(&a_stamp).then_with(|| a_name.cmp(&b_name))
            });
            let latest = bots
                .iter()
                .filter_map(|bot| bot.session_ids.first())
                .map(stamp_of)
                .max()
                .unwrap_or_default();
            ProjectBlock { key, project, bots, latest }
        })
        .collect();
    // The "No project" block only shows when something is in it.
    blocks.retain(|block| block.project.is_some() || !block.bots.is_empty());
    blocks.sort_by(|a, b| {
        let (a_has, b_has) = (a.project.is_some(), b.project.is_some());
        let by_kind = (if a_has { 0 } else { 1 }).cmp(&(if b_has { 0 } else { 1 }));
        let by_latest = b.latest.cmp(&a.latest);
        let by_name = a
            .project
            .as_ref()
            .map(|project| project.name.clone())
            .unwrap_or_default()
            .cmp(&b.project.as_ref().map(|project| project.name.clone()).unwrap_or_default());
        by_kind.then(by_latest).then(by_name)
    });
    blocks
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Agent;

    fn agent(id: &str) -> Agent {
        Agent { id: id.into(), name: id.into(), ..Agent::default() }
    }

    fn project(id: &str) -> Project {
        Project { id: id.into(), name: id.into(), path: Some(format!("C:/{id}")), sort_order: 0 }
    }

    fn session(id: &str, agent_id: &str, project_id: &str, updated_at: &str) -> Session {
        Session {
            id: id.into(),
            agent_id: Some(agent_id.into()),
            project_id: Some(project_id.into()),
            project_path: Some(format!("C:/{project_id}")),
            updated_at: Some(updated_at.into()),
            state: Some("idle".into()),
            ..Session::default()
        }
    }

    fn members(rows: &[(&str, &[&str])]) -> MembersByProject {
        rows.iter()
            .map(|(project_id, agent_ids)| ((*project_id).to_owned(), agent_ids.iter().map(|id| (*id).to_owned()).collect()))
            .collect()
    }

    #[test]
    fn a_hired_employee_shows_before_its_first_conversation_there() {
        let agents = [agent("ada"), agent("wu")];
        let projects = [project("shop")];
        let without = project_blocks(&agents, &projects, &[], None);
        let block = without.iter().find(|block| block.key == "shop").expect("the project is listed");
        assert!(block.bots.is_empty(), "no conversation and no hiring yet");

        let with = project_blocks(&agents, &projects, &[], Some(&members(&[("shop", &["ada"])])));
        let block = with.iter().find(|block| block.key == "shop").unwrap();
        assert_eq!(block.bots.iter().map(|bot| bot.agent_id.as_str()).collect::<Vec<_>>(), vec!["ada"]);
    }

    #[test]
    fn members_of_a_gone_project_or_an_unlisted_agent_are_ignored() {
        let agents = [agent("ada")];
        let projects = [project("shop")];
        let rows = members(&[("gone", &["ada"]), ("shop", &["ghost"])]);
        let blocks = project_blocks(&agents, &projects, &[], Some(&rows));
        assert!(blocks.iter().flat_map(|block| block.bots.iter()).next().is_none());
    }

    #[test]
    fn an_employees_conversations_are_newest_first() {
        let agents = [agent("ada")];
        let projects = [project("shop")];
        let sessions = [
            session("a", "ada", "shop", "2026-11-01T10:00:00Z"),
            session("b", "ada", "shop", "2026-11-02T10:00:00Z"),
        ];
        let blocks = project_blocks(&agents, &projects, &sessions, None);
        assert_eq!(blocks[0].bots[0].session_ids, vec!["b".to_owned(), "a".to_owned()]);
    }

    #[test]
    fn a_conversation_says_what_it_is_doing_in_words() {
        let base = session("a", "ada", "shop", "2026-11-01T10:00:00Z");
        assert_eq!(conversation_status(&base, false), ConversationStatus { label: "Idle", tone: Tone::Idle });
        let working = Session { state: Some("working".into()), ..base.clone() };
        assert_eq!(conversation_status(&working, false).label, "Working");
        assert_eq!(conversation_status(&working, true).label, "Needs you");
        let limited = Session { state: Some("limited".into()), ..base.clone() };
        assert_eq!(conversation_status(&limited, false).label, "Limited");
        let failed = Session { state: Some("error".into()), ..base.clone() };
        assert_eq!(conversation_status(&failed, false).tone, Tone::Bad);
    }

    #[test]
    fn the_no_project_block_only_appears_when_something_is_in_it() {
        let agents = [agent("ada")];
        let projects = [project("shop")];
        let loose = Session {
            id: "x".into(),
            agent_id: Some("ada".into()),
            project_id: None,
            project_path: None,
            updated_at: Some("2026-11-03T10:00:00Z".into()),
            ..Session::default()
        };
        let blocks = project_blocks(&agents, &projects, &[loose], None);
        assert_eq!(blocks.len(), 2, "the project with no one in it still lists");
        // A project's block always sorts before "No project", as in the app, and
        // "No project" only exists because a conversation really is in it.
        assert!(blocks[0].project.is_some());
        assert_eq!(blocks[1].project, None);
        assert_eq!(blocks[1].bots.len(), 1);
    }
}
