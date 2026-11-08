//! The preset employees Bitbots ships, and the CEO's hire proposals.
//!
//! The catalogue is `apps/desktop/src/team/presets.json`, the same file the
//! daemon reads (`include_str!` there, `include_str!` here), so the window shows
//! exactly what `preset.list` reports. The search and grouping follow
//! `team/presets.ts` (`presetMatches`, `presetsByDepartment`) and
//! `ui/proposalGroups.ts` (`hireSiblings`).

use serde::Deserialize;

use super::Permission;

#[derive(Clone, Debug, Deserialize)]
pub struct Department {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Preset {
    pub id: String,
    pub nickname: String,
    pub portrait: String,
    pub department: String,
    pub role: String,
    #[serde(default)]
    pub color: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub description: String,
    /// From `preset.list`: the employee made from this preset, when hired.
    #[serde(default)]
    #[allow(dead_code)]
    pub hired_agent_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Catalogue {
    /// The catalogue's own version, read by the tests.
    #[serde(default)]
    #[allow(dead_code)]
    pub version: u32,
    pub departments: Vec<Department>,
    pub presets: Vec<Preset>,
}

static CATALOGUE: std::sync::LazyLock<Catalogue> = std::sync::LazyLock::new(|| {
    parse(include_str!("../../../desktop/src/team/presets.json")).expect("presets.json parses")
});

/// The catalogue, parsed once. Public for the tests; the window reads it through
/// `presets()` and `departments()`.
#[cfg(test)]
pub fn catalogue() -> &'static Catalogue {
    &CATALOGUE
}

pub fn departments() -> &'static [Department] {
    &CATALOGUE.departments
}

pub fn presets() -> &'static [Preset] {
    &CATALOGUE.presets
}

pub fn department_name(id: &str) -> &str {
    departments()
        .iter()
        .find(|department| department.id == id)
        .map(|department| department.name.as_str())
        .unwrap_or("")
}

/// Parse the catalogue; `load` is the only fallible step, so a bad file fails a
/// test rather than a running window.
pub fn parse(text: &str) -> Result<Catalogue, serde_json::Error> {
    serde_json::from_str(text)
}

/// Presets grouped by department, in catalogue order, empty groups dropped.
pub fn by_department(list: &[Preset]) -> Vec<(&'static Department, Vec<&Preset>)> {
    departments()
        .iter()
        .map(|department| {
            let group: Vec<&Preset> = list.iter().filter(|preset| preset.department == department.id).collect();
            (department, group)
        })
        .filter(|(_, group)| !group.is_empty())
        .collect()
}

/// A case-insensitive match on nickname, role, department name, tags and
/// description (`presetMatches`).
pub fn preset_matches(preset: &Preset, query: &str) -> bool {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return true;
    }
    let department = department_name(&preset.department);
    std::iter::once(preset.nickname.as_str())
        .chain(std::iter::once(preset.role.as_str()))
        .chain(std::iter::once(department))
        .chain(std::iter::once(preset.description.as_str()))
        .chain(preset.tags.iter().map(String::as_str))
        .any(|value| value.to_lowercase().contains(&needle))
}

/// The presets a search keeps, in catalogue order.
pub fn search(query: &str) -> Vec<&'static Preset> {
    presets().iter().filter(|preset| preset_matches(preset, query)).collect()
}

/// The employee a preset already became, from `preset.list`. The window reads
/// the same fact off `agent.presetId`, so this is for a later catalogue refresh.
#[allow(dead_code)]
pub fn hired_agent_id(preset: &Preset) -> Option<&str> {
    preset.hired_agent_id.as_deref().filter(|id| !id.is_empty())
}

/// `hireSiblings`: the other waiting `hire` proposals from the same
/// `propose_hires` call, so the card can offer "Hire all N".
pub fn hire_siblings<'a>(permissions: &'a [Permission], current: &'a Permission) -> Vec<&'a Permission> {
    let group_of = |permission: &'a Permission| -> Option<&'a str> {
        let proposal = permission.proposal.as_ref()?;
        if proposal.kind != "hire" {
            return None;
        }
        let group = proposal.text("groupId");
        (!group.is_empty()).then_some(group)
    };
    let Some(group) = group_of(current) else { return Vec::new() };
    permissions
        .iter()
        .filter(|permission| permission.id != current.id && group_of(permission) == Some(group))
        .collect()
}

/// The projects a bot is hired into, by agent id: the inverse of
/// `project.members`. Used by the tests and by a later "who is where" view.
#[cfg(test)]
pub fn projects_of(members: &std::collections::BTreeMap<String, Vec<String>>) -> std::collections::BTreeMap<String, Vec<String>> {
    let mut by_agent: std::collections::BTreeMap<String, Vec<String>> = std::collections::BTreeMap::new();
    for (project_id, agent_ids) in members {
        for agent_id in agent_ids {
            by_agent.entry(agent_id.clone()).or_default().push(project_id.clone());
        }
    }
    by_agent
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_catalogue_parses_and_every_portrait_is_art_the_build_embedded() {
        let catalogue = catalogue();
        assert_eq!(catalogue.version, 1);
        assert_eq!(catalogue.departments.len(), 7);
        assert!(catalogue.presets.len() >= 20, "the app ships {} presets", catalogue.presets.len());
        let ids = crate::art::portrait_ids();
        for preset in &catalogue.presets {
            assert!(
                ids.contains(&preset.portrait.as_str()),
                "{}'s portrait {} has no embedded image",
                preset.id,
                preset.portrait
            );
            assert!(!preset.nickname.is_empty(), "{} has a nickname", preset.id);
            assert!(!preset.role.is_empty(), "{} has a role", preset.id);
            assert!(
                catalogue.departments.iter().any(|department| department.id == preset.department),
                "{} is in a known department",
                preset.id
            );
        }
        // Ids and nicknames are unique, as the daemon's own test requires.
        let mut ids_seen = catalogue.presets.iter().map(|preset| preset.id.as_str()).collect::<Vec<_>>();
        ids_seen.sort_unstable();
        ids_seen.dedup();
        assert_eq!(ids_seen.len(), catalogue.presets.len());
    }

    #[test]
    fn a_search_matches_a_role_a_tag_or_a_department() {
        let backend = presets().iter().find(|preset| preset.id == "ada").expect("ada");
        assert!(preset_matches(backend, ""));
        assert!(preset_matches(backend, "backend"));
        assert!(preset_matches(backend, "Ada"));
        assert!(preset_matches(backend, "engineering"), "the department name matches");
        assert!(preset_matches(backend, "database"), "a tag matches");
        assert!(!preset_matches(backend, "gardening"));
        assert_eq!(search("").len(), presets().len());
    }

    #[test]
    fn the_catalogue_groups_in_department_order() {
        let groups = by_department(presets());
        assert_eq!(groups[0].0.id, "engineering");
        assert_eq!(groups.last().unwrap().0.id, "growth");
        assert!(groups.iter().all(|(_, group)| !group.is_empty()));
        assert!(by_department(&[]).is_empty());
    }

    fn hire(id: &str, group: Option<&str>) -> Permission {
        Permission {
            id: id.to_owned(),
            kind: Some("proposal".into()),
            proposal: Some(super::super::Proposal {
                id: format!("prop-{id}"),
                kind: "hire".into(),
                payload: Some(match group {
                    Some(group) => json!({"groupId": group, "nickname": id}),
                    None => json!({"nickname": id}),
                }),
            }),
            ..Permission::default()
        }
    }

    #[test]
    fn hire_siblings_are_the_rest_of_one_propose_hires_call() {
        let list = vec![hire("1", Some("g1")), hire("2", Some("g1")), hire("3", Some("g1")), hire("4", None)];
        assert_eq!(hire_siblings(&list, &list[0]).iter().map(|item| item.id.as_str()).collect::<Vec<_>>(), vec!["2", "3"]);
        assert!(hire_siblings(&list, &list[3]).is_empty(), "a proposal with no group has no siblings");
        // A proposal that is not a hire never groups.
        let mut bot = hire("5", Some("g1"));
        bot.proposal.as_mut().unwrap().kind = "bot".into();
        assert!(hire_siblings(&[bot.clone(), hire("6", Some("g1"))], &bot).is_empty());
    }

    #[test]
    fn a_members_table_inverts_to_projects_per_bot() {
        let mut members = std::collections::BTreeMap::new();
        members.insert("shop".to_owned(), vec!["ada".to_owned(), "wu".to_owned()]);
        members.insert("site".to_owned(), vec!["ada".to_owned()]);
        let by_agent = projects_of(&members);
        assert_eq!(by_agent.get("ada").unwrap(), &vec!["shop".to_owned(), "site".to_owned()]);
        assert_eq!(by_agent.get("wu").unwrap(), &vec!["shop".to_owned()]);
    }
}
