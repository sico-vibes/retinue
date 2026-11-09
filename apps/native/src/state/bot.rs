//! The bot form behind the "Create your own employee" wizard and a bot's own
//! settings page (`ui/BotWizard.tsx`, `ui/BlobSettings.tsx`).
//!
//! The field list, its rules and the body `agent.create` / `agent.update` take
//! live here as plain data, so the wizard and the settings page share one form
//! and the rules can be tested without a window.

use std::collections::BTreeMap;

use serde_json::{json, Value};

/// The wizard's steps, in order.
pub const STEPS: [&str; 5] = ["Start from", "Name it", "Choose its brain", "Access and look", "Review"];

/// One bot being created or edited. `agent_id` is `None` while creating.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BotForm {
    pub agent_id: Option<String>,
    pub name: String,
    pub role: String,
    pub description: String,
    pub instructions: String,
    /// Comma-separated in the field.
    pub tags: String,
    pub runtime_id: String,
    pub model: Option<String>,
    pub thinking: Option<String>,
    pub approval_mode: String,
    pub project_id: Option<String>,
    /// A `char-*` portrait id, or `logo`, `logo-tint` or `logo-mono`.
    pub icon: String,
    /// A swatch name or a `#rrggbb` colour.
    pub color: String,
    /// Which wizard step is showing, 0-based.
    pub step: usize,
}

impl BotForm {
    pub fn editing(agent_id: String) -> Self {
        Self { agent_id: Some(agent_id), approval_mode: "ask".to_owned(), icon: "logo".to_owned(), color: "violet".to_owned(), ..Self::default() }
    }

    pub fn creating() -> Self {
        Self { approval_mode: "ask".to_owned(), icon: "logo".to_owned(), color: "violet".to_owned(), ..Self::default() }
    }

    pub fn is_edit(&self) -> bool {
        self.agent_id.is_some()
    }

    /// The tags as the form holds them, split and de-duplicated.
    pub fn tag_list(&self) -> Vec<String> {
        let mut tags: Vec<String> = Vec::new();
        for tag in self.tags.split(',') {
            let tag = tag.trim().to_lowercase();
            if !tag.is_empty() && !tags.contains(&tag) {
                tags.push(tag);
            }
        }
        tags
    }
}

/// A field's problem, by field name. The wizard will not leave a step while one
/// of that step's fields is here, and the settings page will not save.
pub fn validate(form: &BotForm) -> BTreeMap<&'static str, String> {
    let mut errors = BTreeMap::new();
    let name = form.name.trim();
    if name.is_empty() {
        errors.insert("name", "Give this bot a name.".to_owned());
    } else if name.chars().count() > 40 {
        errors.insert("name", "Keep the name to 40 characters.".to_owned());
    }
    if form.role.trim().chars().count() > 60 {
        errors.insert("role", "Keep the role to 60 characters.".to_owned());
    }
    if form.description.trim().chars().count() > 255 {
        errors.insert("description", "Keep the description to 255 characters.".to_owned());
    }
    if form.instructions.trim().chars().count() > 20_000 {
        errors.insert("instructions", "Keep the instructions under 20,000 characters.".to_owned());
    }
    if form.runtime_id.trim().is_empty() {
        errors.insert("runtime", "Choose the coding agent this bot runs on.".to_owned());
    }
    if !crate::state::APPROVAL_MODES.contains(&form.approval_mode.as_str()) {
        errors.insert("approvalMode", "Choose a permission mode.".to_owned());
    }
    if form.icon.trim().is_empty() {
        errors.insert("icon", "Choose a portrait or a logo.".to_owned());
    }
    if form.color.trim().is_empty() {
        errors.insert("color", "Choose the bot's colour.".to_owned());
    }
    errors
}

/// The first problem of a wizard step, which is what stops "Next".
pub fn step_blocker(form: &BotForm) -> Option<String> {
    let errors = validate(form);
    let keys: &[&str] = match form.step {
        1 => &["name", "role", "description"],
        2 => &["runtime", "instructions"],
        3 => &["approvalMode", "icon", "color"],
        _ => &[],
    };
    keys.iter().find_map(|key| errors.get(key).cloned())
}

/// The body `agent.create` takes, or the fields `agent.update` changes. Only what
/// the form owns is written, so a save never clears a setting it does not show.
pub fn params(form: &BotForm) -> Value {
    let model = form.model.clone().filter(|model| !model.trim().is_empty());
    let thinking = form.thinking.clone().filter(|level| !level.trim().is_empty());
    json!({
        "name": form.name.trim(),
        "role": form.role.trim(),
        "description": form.description.trim(),
        "instructions": form.instructions.trim(),
        "tags": form.tag_list(),
        "runtimeId": form.runtime_id,
        "model": model,
        "thinking": thinking,
        "approvalMode": form.approval_mode,
        "projectId": form.project_id,
        "look": { "shape": "circle", "icon": form.icon },
        "color": form.color,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ready() -> BotForm {
        BotForm {
            name: "Scout".into(),
            role: "Research".into(),
            runtime_id: "rt-claude".into(),
            approval_mode: "ask".into(),
            icon: "char-ada".into(),
            color: "mint".into(),
            ..BotForm::creating()
        }
    }

    #[test]
    fn a_ready_form_has_no_problems() {
        assert!(validate(&ready()).is_empty());
        assert_eq!(step_blocker(&ready()), None);
    }

    #[test]
    fn a_bot_needs_a_name_and_a_coding_agent() {
        let mut form = ready();
        form.name = "   ".into();
        form.runtime_id = String::new();
        let errors = validate(&form);
        assert_eq!(errors.get("name").map(String::as_str), Some("Give this bot a name."));
        assert!(errors.contains_key("runtime"));
    }

    #[test]
    fn a_name_is_capped_at_forty_characters() {
        let mut form = ready();
        form.name = "x".repeat(41);
        assert_eq!(validate(&form).get("name").map(String::as_str), Some("Keep the name to 40 characters."));
        form.name = "x".repeat(40);
        assert!(!validate(&form).contains_key("name"));
    }

    #[test]
    fn a_step_only_stops_on_its_own_fields() {
        let mut form = ready();
        form.step = 1;
        form.name = String::new();
        assert_eq!(step_blocker(&form).as_deref(), Some("Give this bot a name."));
        // An empty coding agent stops step 2, not step 1.
        form.name = "Scout".into();
        form.runtime_id = String::new();
        assert_eq!(step_blocker(&form), None);
        form.step = 2;
        assert_eq!(step_blocker(&form).as_deref(), Some("Choose the coding agent this bot runs on."));
    }

    #[test]
    fn tags_are_split_trimmed_lowercased_and_deduplicated() {
        let mut form = ready();
        form.tags = " Research , backend ,, backend, ui ".into();
        assert_eq!(form.tag_list(), vec!["research".to_owned(), "backend".to_owned(), "ui".to_owned()]);
    }

    #[test]
    fn the_body_keeps_the_apps_own_field_names() {
        let mut form = ready();
        form.tags = "ops".into();
        form.model = Some("claude-sonnet-5-5".into());
        let body = params(&form);
        assert_eq!(body["name"], "Scout");
        assert_eq!(body["runtimeId"], "rt-claude");
        assert_eq!(body["approvalMode"], "ask");
        assert_eq!(body["tags"], json!(["ops"]));
        assert_eq!(body["model"], "claude-sonnet-5-5");
        // An empty model means the agent's default, not an empty string.
        form.model = Some("  ".into());
        assert!(params(&form)["model"].is_null());
        // The look is the Haze shape the daemon validates.
        assert_eq!(body["look"]["icon"], "char-ada");
        assert_eq!(body["look"]["shape"], "circle");
    }
}
