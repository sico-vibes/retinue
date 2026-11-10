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
const PERSONALITIES_JSON: &str = include_str!("../../../apps/desktop/src/team/personalities.json");

/// Longest personality prompt, in characters (the storage rule).
pub const MAX_PERSONALITY_CHARS: usize = 2000;

/// The skills a preset wears: on by default, and offered but off by default.
/// Ids name library skills (`skill_library.rs`).
#[derive(Deserialize, Clone, Default)]
pub struct PresetSkills {
    #[serde(default)]
    pub default: Vec<String>,
    #[serde(default)]
    pub optional: Vec<String>,
}

#[derive(Deserialize)]
struct PersonalityFile {
    version: u32,
    personalities: Vec<Personality>,
}

/// One personality the owner can pick: a short card and the prompt it adds.
#[derive(Deserialize, Clone)]
pub struct Personality {
    pub id: String,
    pub name: String,
    pub summary: String,
    pub prompt: String,
}

fn personality_file() -> &'static PersonalityFile {
    static FILE: OnceLock<PersonalityFile> = OnceLock::new();
    FILE.get_or_init(|| {
        let file: PersonalityFile = serde_json::from_str(PERSONALITIES_JSON).expect("apps/desktop/src/team/personalities.json is valid at build time");
        debug_assert_eq!(file.version, 1, "apps/desktop/src/team/personalities.json has an unknown version");
        file
    })
}

pub fn personalities() -> &'static [Personality] {
    &personality_file().personalities
}

pub fn personality(id: &str) -> Option<&'static Personality> {
    personalities().iter().find(|personality| personality.id == id)
}

/// A personality from an RPC body: null stays null; an object `{id, prompt?}`
/// needs an id that is one of the shipped personalities or `custom`. The
/// prompt defaults to the shipped one for a known id, is required for
/// `custom`, and is at most 2,000 characters either way. Returns the stored
/// shape `{id, prompt}`.
pub fn personality_value(input: &Value) -> Result<Value, String> {
    if input.is_null() {
        return Ok(Value::Null);
    }
    let object = input.as_object().ok_or("personality must be an object with an id and a prompt.")?;
    if object.keys().any(|key| key != "id" && key != "prompt") {
        return Err("personality takes only an id and a prompt.".into());
    }
    let id = input["id"].as_str().map(str::trim).filter(|id| !id.is_empty()).ok_or("personality needs an id.")?;
    let known = personality(id);
    if known.is_none() && id != "custom" {
        return Err(format!("There is no personality called {id}."));
    }
    let prompt = match input.get("prompt") {
        None | Some(Value::Null) => known.map(|personality| personality.prompt.clone()).ok_or("A custom personality needs a prompt.")?,
        Some(Value::String(text)) => text.trim().to_owned(),
        Some(_) => return Err("The personality prompt must be text.".into()),
    };
    if prompt.is_empty() {
        return Err("The personality prompt must not be empty.".into());
    }
    if prompt.chars().count() > MAX_PERSONALITY_CHARS {
        return Err(format!("The personality prompt must be at most {MAX_PERSONALITY_CHARS} characters."));
    }
    if prompt.contains('\0') {
        return Err("The personality prompt contains an unsupported character.".into());
    }
    Ok(json!({"id": id, "prompt": prompt}))
}

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
    /// The profile photo the employee wears (`photo-NN`, `apps/desktop/src/blob/photos/`).
    pub photo: String,
    pub department: String,
    pub role: String,
    pub color: String,
    pub tags: Vec<String>,
    pub description: String,
    /// One line each; joined with a newline when the bot is made.
    pub instructions: Vec<String>,
    /// The personality (`personalities.json` id) the employee starts with.
    #[serde(default)]
    pub personality: String,
    /// The library skills the employee wears (default on) or may wear (optional).
    #[serde(default)]
    pub skills: PresetSkills,
}

impl Preset {
    pub fn instructions(&self) -> String {
        self.instructions.join("\n")
    }

    /// The look a hired employee wears: its profile photo, with the character
    /// portrait kept as the fallback mark. `style` is a look-theme concept the
    /// daemon never stores, so the shape is written explicitly (`valid_look` requires it).
    pub fn look(&self) -> Value {
        json!({"shape":"circle","icon":self.portrait,"photo":self.photo})
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
/// (or null), its starting personality id and its skills (`{id, name,
/// description}` for the default and the optional ones), plus the shipped
/// `personalities` and the `skillLibrary`, so the hire dialog needs one call.
/// The UI joins `hiredAgentId` with its own agent list.
pub fn list(st: &AppState) -> Result<Value, String> {
    let hired = st.db.preset_agents().map_err(|error| error.to_string())?;
    let agent_of = |preset_id: &str| hired.iter().find(|row| row["presetId"].as_str() == Some(preset_id)).map(|row| row["agentId"].clone()).unwrap_or(Value::Null);
    Ok(json!({
        "departments": departments().iter().map(|department| json!({"id":department.id,"name":department.name})).collect::<Vec<_>>(),
        "presets": presets().iter().map(|preset| json!({
            "id": preset.id,
            "nickname": preset.nickname,
            "portrait": preset.portrait,
            "photo": preset.photo,
            "department": preset.department,
            "role": preset.role,
            "color": preset.color,
            "tags": preset.tags,
            "description": preset.description,
            "personality": preset.personality,
            "skills": {
                "default": preset.skills.default.iter().map(|id| crate::skill_library::card(id)).collect::<Vec<_>>(),
                "optional": preset.skills.optional.iter().map(|id| crate::skill_library::card(id)).collect::<Vec<_>>(),
            },
            "hiredAgentId": agent_of(&preset.id),
        })).collect::<Vec<_>>(),
        "personalities": personalities().iter().map(|personality| json!({"id":personality.id,"name":personality.name,"summary":personality.summary,"prompt":personality.prompt})).collect::<Vec<_>>(),
        "skillLibrary": crate::skill_library::list(),
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

    /// Every preset names a shipped personality and library skills, within the
    /// owner's limits: up to 10 on by default, up to 5 optional, none twice.
    #[test]
    fn every_preset_has_a_personality_and_skills_from_the_library() {
        assert_eq!(personalities().len(), 10, "ten personalities ship");
        for personality in personalities() {
            assert!(personality.prompt.chars().count() <= MAX_PERSONALITY_CHARS, "{}", personality.id);
            assert!(personality_value(&json!({"id": personality.id})).is_ok(), "{}", personality.id);
        }
        for preset in presets() {
            assert!(personality(&preset.personality).is_some(), "{} has no personality {}", preset.id, preset.personality);
            assert!(preset.skills.default.len() <= 10 && preset.skills.optional.len() <= 5, "{} has too many skills", preset.id);
            assert!(!preset.skills.default.is_empty(), "{} has no default skills", preset.id);
            let mut seen = std::collections::HashSet::new();
            for id in preset.skills.default.iter().chain(preset.skills.optional.iter()) {
                assert!(crate::skill_library::get(id).is_some(), "{} names {id}, which the library does not ship", preset.id);
                assert!(seen.insert(id.clone()), "{} lists {id} twice", preset.id);
            }
        }
    }

    #[test]
    fn personality_values_are_checked() {
        assert_eq!(personality_value(&Value::Null).unwrap(), Value::Null);
        assert_eq!(personality_value(&json!({"id":"warm"})).unwrap()["prompt"], personality("warm").unwrap().prompt.as_str());
        assert_eq!(personality_value(&json!({"id":"custom","prompt":"  Short.  "})).unwrap(), json!({"id":"custom","prompt":"Short."}));
        assert!(personality_value(&json!({"id":"custom"})).is_err());
        assert!(personality_value(&json!({"id":"custom","prompt":"  "})).is_err());
        assert!(personality_value(&json!({"id":"nope"})).is_err());
        assert!(personality_value(&json!({"id":"warm","prompt":"x".repeat(2001)})).is_err());
        assert!(personality_value(&json!({"id":"warm","prompt":"x".repeat(2000)})).is_ok());
        assert!(personality_value(&json!({"id":"warm","other":1})).is_err());
        assert!(personality_value(&json!("warm")).is_err());
    }

    #[test]
    fn hiring_a_preset_writes_a_haze_look() {
        let preset = find("ada").expect("Ada ships");
        assert_eq!(preset.look(), json!({"shape":"circle","icon":"char-ada-lovelace","photo":"photo-17"}));
        assert!(preset.instructions().contains('\n'), "the lines are joined with newlines");
        assert_eq!(department_name(&preset.department), "Engineering");
        assert!(find("nobody").is_none());
    }
}
