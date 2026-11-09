//! The preset employees: `apps/desktop/src/team/presets.json`.
//!
//! Retinue ships 28 preset employees. The file is the catalogue the UI shows,
//! so it lives in the desktop app and is embedded here with `include_str!`: one
//! table, two readers, no second copy to drift. It is parsed once per process.
//!
//! Hiring is in `team.rs` (`project.hire`) and the CEO's two tools are in
//! `proposals.rs` (`list_presets`, `propose_hires`).

use crate::AppState;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::OnceLock;

const PRESETS_JSON: &str = include_str!("../../../apps/desktop/src/team/presets.json");

#[derive(Deserialize)]
pub struct Catalogue {
    pub version: u32,
    pub departments: Vec<Department>,
    pub presets: Vec<Preset>,
}

#[derive(Deserialize, Clone)]
pub struct Department {
    pub id: String,
    pub name: String,
}

#[derive(Deserialize, Clone)]
pub struct Preset {
    pub id: String,
    pub nickname: String,
    pub portrait: String,
    pub department: String,
    pub role: String,
    pub color: String,
    pub tags: Vec<String>,
    pub description: String,
    /// One line each; joined with a newline when the bot is made.
    pub instructions: Vec<String>,
}

impl Preset {
    pub fn instructions(&self) -> String {
        self.instructions.join("\n")
    }

    /// The look a Haze bot wears: a round silhouette with the portrait as its
    /// mark. `style` is a look-theme concept the daemon never stores, so the
    /// shape is written explicitly (`valid_look` requires it).
    pub fn look(&self) -> Value {
        json!({"shape":"circle","icon":self.portrait})
    }
}

fn catalogue() -> &'static Catalogue {
    static CATALOGUE: OnceLock<Catalogue> = OnceLock::new();
    CATALOGUE.get_or_init(|| {
        let catalogue: Catalogue = serde_json::from_str(PRESETS_JSON).expect("apps/desktop/src/team/presets.json is valid at build time");
        // Version 1 is the only shape this build understands, and the catalogue
        // test pins it; a future file must bump this check with its reader.
        debug_assert_eq!(catalogue.version, 1, "apps/desktop/src/team/presets.json has an unknown version");
        catalogue
    })
}

pub fn presets() -> &'static [Preset] {
    &catalogue().presets
}

pub fn departments() -> &'static [Department] {
    &catalogue().departments
}

pub fn find(id: &str) -> Option<&'static Preset> {
    presets().iter().find(|preset| preset.id == id)
}

/// A preset's department name, for proposal cards.
pub fn department_name(id: &str) -> String {
    departments().iter().find(|department| department.id == id).map(|department| department.name.clone()).unwrap_or_else(|| id.to_owned())
}

/// `preset.list`: the departments and every preset, each with the bot it made
/// (or null). The UI joins `hiredAgentId` with its own agent list.
pub fn list(st: &AppState) -> Result<Value, String> {
    let hired = st.db.preset_agents().map_err(|error| error.to_string())?;
    let agent_of = |preset_id: &str| hired.iter().find(|row| row["presetId"].as_str() == Some(preset_id)).map(|row| row["agentId"].clone()).unwrap_or(Value::Null);
    Ok(json!({
        "departments": departments().iter().map(|department| json!({"id":department.id,"name":department.name})).collect::<Vec<_>>(),
        "presets": presets().iter().map(|preset| json!({
            "id": preset.id,
            "nickname": preset.nickname,
            "portrait": preset.portrait,
            "department": preset.department,
            "role": preset.role,
            "color": preset.color,
            "tags": preset.tags,
            "description": preset.description,
            "hiredAgentId": agent_of(&preset.id),
        })).collect::<Vec<_>>(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The catalogue has to be publishable: every field is one the daemon can
    /// store. `valid_look` is the portrait allowlist, so a typo in the JSON
    /// fails here rather than when someone hires that employee.
    #[test]
    fn every_preset_is_valid_for_storage() {
        let catalogue = catalogue();
        assert_eq!(catalogue.version, 1);
        assert_eq!(catalogue.presets.len(), 28, "28 preset employees ship");
        assert!(!catalogue.departments.is_empty());
        let mut ids = std::collections::HashSet::new();
        let mut nicknames = std::collections::HashSet::new();
        for preset in presets() {
            assert!(ids.insert(preset.id.clone()), "duplicate preset id {}", preset.id);
            assert!(nicknames.insert(preset.nickname.to_lowercase()), "duplicate nickname {}", preset.nickname);
            assert!(departments().iter().any(|department| department.id == preset.department), "{} has no department {}", preset.id, preset.department);
            assert!(preset.role.chars().count() <= 40, "{}'s role is longer than 40 characters", preset.id);
            assert!(preset.description.chars().count() <= 255, "{}'s description is longer than 255 characters", preset.id);
            assert!(!preset.nickname.trim().is_empty() && preset.nickname.chars().count() <= 60);
            assert!(!preset.instructions.is_empty(), "{} has no instructions", preset.id);
            assert!(retinue_storage::agent_accepts_preset(&json!({
                "name": preset.nickname,
                "description": preset.description,
                "color": preset.color,
                "role": preset.role,
                "tags": preset.tags,
                "look": preset.look(),
                "instructions": preset.instructions(),
            })), "{} is not a valid bot", preset.id);
        }
    }

    #[test]
    fn hiring_a_preset_writes_a_haze_look() {
        let preset = find("ada").expect("Ada ships");
        assert_eq!(preset.look(), json!({"shape":"circle","icon":"char-ada-lovelace"}));
        assert!(preset.instructions().contains('\n'), "the lines are joined with newlines");
        assert_eq!(department_name(&preset.department), "Engineering");
        assert!(find("nobody").is_none());
    }
}
