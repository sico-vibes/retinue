//! Slash commands in the composer, ported from `apps/desktop/src/ui/slashCommands.ts`.
//!
//! Typing `/` at the start of the composer, or after a space, opens a menu with
//! three groups: actions the app performs itself, the skills the pool switched
//! on for this bot, and the CEO's own commands when the conversation's bot is
//! the CEO. Picking a skill or a CEO command attaches it to the message; the
//! message is stored and shown exactly as typed.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlashGroup {
    Actions,
    Skills,
    Manager,
}

impl SlashGroup {
    pub fn label(self) -> &'static str {
        match self {
            SlashGroup::Actions => "Actions",
            SlashGroup::Skills => "Skills",
            SlashGroup::Manager => "Manager",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlashKind {
    Action,
    Skill,
    Manager,
}

/// One row of the menu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlashItem {
    pub kind: SlashKind,
    /// The name without its leading slash.
    pub name: String,
    pub group: SlashGroup,
    pub description: String,
}

/// A skill the pool switched on for this bot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlashSkill {
    pub name: String,
    pub description: String,
}

/// A command attached to the message being composed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlashCommand {
    pub kind: SlashKind,
    pub name: String,
}

fn item(kind: SlashKind, name: &str, group: SlashGroup, description: &str) -> SlashItem {
    SlashItem { kind, name: name.to_owned(), group, description: description.to_owned() }
}

/// The actions the app performs itself; no message is sent.
pub fn slash_actions() -> Vec<SlashItem> {
    vec![
        item(SlashKind::Action, "plan", SlashGroup::Actions, "Turn plan mode on or off for this conversation"),
        item(SlashKind::Action, "new", SlashGroup::Actions, "Start a new conversation with this bot"),
        item(SlashKind::Action, "model", SlashGroup::Actions, "Choose the model for this conversation"),
        item(SlashKind::Action, "effort", SlashGroup::Actions, "Choose the effort level for this conversation"),
        item(SlashKind::Action, "rename", SlashGroup::Actions, "Rename this conversation"),
        item(SlashKind::Action, "help", SlashGroup::Actions, "Show every command"),
    ]
}

/// The CEO's commands, offered only in its own conversations.
pub fn manager_slash_commands() -> Vec<SlashItem> {
    vec![
        item(SlashKind::Manager, "interview", SlashGroup::Manager, "Run or continue the personalisation interview"),
        item(SlashKind::Manager, "onboard", SlashGroup::Manager, "Onboard a project and propose a team"),
        item(SlashKind::Manager, "team", SlashGroup::Manager, "Propose a team of bots for a project"),
        item(SlashKind::Manager, "status", SlashGroup::Manager, "Summarise every project's state and open work"),
        item(SlashKind::Manager, "playbook", SlashGroup::Manager, "Review its own playbook and propose improvements"),
        item(SlashKind::Manager, "remember", SlashGroup::Manager, "Save what follows as a fact about you"),
    ]
}

/// A `/query` being typed at the caret, at the start of the message or after a
/// space. `start` is the byte offset of the slash.
pub fn slash_query(text: &str, caret: usize) -> Option<(usize, String)> {
    let mut caret = caret.min(text.len());
    while caret > 0 && !text.is_char_boundary(caret) {
        caret -= 1;
    }
    let before = &text[..caret];
    let start = before.rfind('/')?;
    // Only at the start or after a space, and only a word of letters, digits,
    // dashes and underscores since it.
    if start > 0 {
        let previous = before[..start].chars().next_back();
        if previous != Some(' ') && previous != Some('\n') {
            return None;
        }
    }
    let query = &before[start + 1..];
    if query.chars().count() > 40 {
        return None;
    }
    if !query.chars().all(|ch| ch.is_alphanumeric() || ch == '_' || ch == '-' || ch == '/') {
        return None;
    }
    Some((start, query.to_owned()))
}

/// The filter's own rule, as a pure function: a case-insensitive match on the
/// name or the description, capped at forty rows.
pub fn slash_items(query: &str, skills: &[SlashSkill], is_manager: bool) -> Vec<SlashItem> {
    let needle = query.trim().to_lowercase();
    let mut rows = slash_actions();
    rows.extend(skills.iter().map(|skill| {
        item(SlashKind::Skill, &skill.name, SlashGroup::Skills, &skill.description)
    }));
    if is_manager {
        rows.extend(manager_slash_commands());
    }
    rows.into_iter()
        .filter(|row| needle.is_empty() || format!("{} {}", row.name, row.description).to_lowercase().contains(&needle))
        .take(40)
        .collect()
}

/// A message that begins with a skill or a CEO command, typed by hand (for
/// example `/code-review check the parser`). `None` when the leading word is not
/// one this bot may use, so an ordinary sentence is never read as a command.
pub fn leading_command(text: &str, skills: &[SlashSkill], is_manager: bool) -> Option<SlashCommand> {
    let rest = text.trim_start();
    let word = rest.strip_prefix('/')?;
    let end = word
        .char_indices()
        .find(|(_, ch)| !(ch.is_ascii_alphanumeric() || *ch == '-'))
        .map(|(index, _)| index)
        .unwrap_or(word.len());
    if end == 0 {
        return None;
    }
    let next = word[end..].chars().next();
    if !matches!(next, None | Some(' ') | Some('\n') | Some('\t')) {
        return None;
    }
    let name = word[..end].to_lowercase();
    if skills.iter().any(|skill| skill.name.to_lowercase() == name) {
        return Some(SlashCommand { kind: SlashKind::Skill, name });
    }
    if is_manager && manager_slash_commands().iter().any(|item| item.name == name) {
        return Some(SlashCommand { kind: SlashKind::Manager, name });
    }
    None
}

/// The message shown and stored for a command plus the typed remainder.
pub fn command_message(command: &SlashCommand, remainder: &str) -> String {
    let rest = remainder.trim();
    if rest.is_empty() {
        format!("/{}", command.name)
    } else {
        format!("/{} {rest}", command.name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn skills() -> Vec<SlashSkill> {
        vec![
            SlashSkill { name: "code-review".into(), description: "Review a diff for bugs".into() },
            SlashSkill { name: "write-tests".into(), description: "Add tests for a change".into() },
        ]
    }

    #[test]
    fn a_slash_at_the_start_of_a_word_opens_the_menu() {
        assert_eq!(slash_query("/mo", 3), Some((0, "mo".to_owned())));
        assert_eq!(slash_query("Fix it /te", 10), Some((7, "te".to_owned())));
        // Only at the start or after a space: a path is not a command.
        assert_eq!(slash_query("src/main.rs /x", 14), Some((12, "x".to_owned())));
        assert_eq!(slash_query("src/main.rs /x", 13), Some((12, String::new())), "the bare slash opens the menu");
        assert_eq!(slash_query("a/b", 3), None);
        assert_eq!(slash_query("no slash here", 13), None);
    }

    #[test]
    fn the_filter_matches_a_name_or_a_description_and_keeps_the_group_order() {
        let all = slash_items("", &skills(), false);
        assert_eq!(all[0].group, SlashGroup::Actions);
        assert!(all.iter().any(|row| row.kind == SlashKind::Skill && row.name == "code-review"));
        assert!(!all.iter().any(|row| row.group == SlashGroup::Manager), "no CEO commands for an employee");
        let manager = slash_items("", &skills(), true);
        assert!(manager.iter().any(|row| row.group == SlashGroup::Manager));
        // A description matches too, and the search is case-insensitive.
        let matched = slash_items("REVIEW", &skills(), false);
        assert_eq!(matched.len(), 1);
        assert_eq!(matched[0].name, "code-review");
        // Nothing matches: no rows, so the menu closes.
        assert!(slash_items("zzz", &skills(), true).is_empty());
        // The filter is capped so a huge catalog cannot fill the window.
        assert!(slash_items("", &skills(), true).len() <= 40);
    }

    #[test]
    fn a_typed_command_is_only_a_command_when_the_bot_may_use_it() {
        assert_eq!(
            leading_command("/code-review check the parser", &skills(), false),
            Some(SlashCommand { kind: SlashKind::Skill, name: "code-review".into() })
        );
        // A CEO command is not an employee's command.
        assert_eq!(leading_command("/status now", &skills(), false), None);
        assert_eq!(
            leading_command("/status now", &skills(), true),
            Some(SlashCommand { kind: SlashKind::Manager, name: "status".into() })
        );
        // A path or an unknown word is ordinary text.
        assert_eq!(leading_command("/usr/bin/env", &skills(), true), None);
        assert_eq!(leading_command("/nonsense please", &skills(), true), None);
        assert_eq!(leading_command("plain", &skills(), true), None);
    }

    #[test]
    fn a_command_and_its_remainder_are_stored_as_typed() {
        let command = SlashCommand { kind: SlashKind::Skill, name: "code-review".into() };
        assert_eq!(command_message(&command, "  check it "), "/code-review check it");
        assert_eq!(command_message(&command, "   "), "/code-review");
    }
}
