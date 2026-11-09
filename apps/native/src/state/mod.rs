//! The window's model: the daemon's snapshot plus the events that change it.
//!
//! Same idea as `apps/desktop/src/types.ts` `applyEvent`: the daemon is
//! authoritative, the window applies ordered events on top of one snapshot, and
//! unknown fields and event types are ignored rather than refused.

pub mod bot;
pub mod changes;
pub mod projects;
pub mod quota;
pub mod slash;
pub mod tasks;
pub mod team;

use serde::Deserialize;
use serde_json::Value;

use crate::daemon::client::DaemonEvent;

pub use projects::{conversation_status, project_blocks, project_of_session, ConversationStatus, Tone};

pub mod serde_defaults {
    //! The daemon writes `null` where a list is empty in more than one place (a
    //! message with no attachments, a snapshot with no quota rows yet), and a
    //! list that refuses `null` throws the whole snapshot away. These turn `null`
    //! into the empty value instead, so an old daemon shape still reads.

    use serde::{Deserialize, Deserializer};

    pub fn null_as_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
    where
        D: Deserializer<'de>,
        T: Default + Deserialize<'de>,
    {
        Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
    }
}


/// The bot's stored look, as far as the mark is concerned (`blob/haze/icons.ts`).
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Look {
    pub icon: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[allow(dead_code)]
pub struct Agent {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub color: String,
    #[serde(default)]
    pub runtime_id: String,
    #[serde(default)]
    pub archived: bool,
    #[serde(default)]
    pub hidden: bool,
    #[serde(default)]
    pub parent_agent_id: Option<String>,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub last_project_id: Option<String>,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub system_role: Option<String>,
    #[serde(default)]
    pub sort_order: i64,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
    /// The Haze mark this bot shows: a portrait id or a logo choice.
    #[serde(default)]
    pub look: Option<Look>,
    /// The preset this employee was hired from, when it was hired from one.
    #[serde(default)]
    pub preset_id: Option<String>,
    /// The bot's own model, effort, speed and permission mode: what a new
    /// conversation starts with (`agent.update`).
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub thinking: Option<String>,
    #[serde(default)]
    pub service_tier: Option<String>,
    #[serde(default)]
    pub approval_mode: Option<String>,
    #[serde(default)]
    pub effective_approval_mode: Option<String>,
    #[serde(default)]
    pub instructions: Option<String>,
    #[serde(default, deserialize_with = "serde_defaults::null_as_default")]
    pub tags: Vec<String>,
    /// `all`, `input` (approvals and questions only) or `muted`.
    #[serde(default)]
    pub notification_level: Option<String>,
}

impl Agent {
    /// The permission mode this bot starts conversations on, as the composer
    /// shows it (`sessionApprovalMode` in `ui/ComposerControls.tsx`).
    pub fn permission_mode(&self) -> String {
        self.effective_approval_mode
            .as_deref()
            .or(self.approval_mode.as_deref())
            .map(str::trim)
            .filter(|mode| APPROVAL_MODES.contains(mode))
            .unwrap_or("ask")
            .to_owned()
    }
}

/// Every permission mode the app offers, in the order its menu lists them.
pub const APPROVAL_MODES: [&str; 4] = ["ask", "accept_edits", "auto", "bypass"];

/// A permission mode's two labels: the pill's word and the menu's sentence
/// (`APPROVAL_MODE_COPY` in `ui/ComposerControls.tsx`).
pub fn approval_mode_copy(mode: &str) -> (String, &'static str) {
    match mode {
        "accept_edits" => ("Accept edits".to_owned(), "Reading and editing files inside the project run; every command asks."),
        "auto" => ("Auto".to_owned(), "Low-risk actions inside the project run; anything else asks."),
        "bypass" => ("Full access".to_owned(), "The agent acts without asking. Use only in projects you trust."),
        _ => ("Ask first".to_owned(), "You approve each command and file change."),
    }
}

impl Agent {
    pub fn is_manager(&self) -> bool {
        self.system_role.as_deref() == Some("manager")
    }

    /// A bot the sidebar lists: not archived, not a sub-bot, not the CEO.
    pub fn is_employee(&self) -> bool {
        !self.archived && !self.hidden && self.parent_agent_id.is_none() && !self.is_manager()
    }

    pub fn icon(&self) -> Option<&str> {
        self.look.as_ref().and_then(|look| look.icon.as_deref())
    }

    pub fn role_or_default(&self) -> &str {
        self.role
            .as_deref()
            .map(str::trim)
            .filter(|role| !role.is_empty())
            .or_else(|| Some(self.description.trim()).filter(|text| !text.is_empty()))
            .unwrap_or("Employee")
    }
}

/// One coding agent the daemon found (`runtime.list`, and `runtimes` in the
/// snapshot). The window only needs enough of it to label the agent and to see
/// whether it can be used.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Runtime {
    pub id: String,
    pub provider: String,
    pub display_name: Option<String>,
    pub status: Option<String>,
    pub auth_state: Option<String>,
}

impl Runtime {
    /// `runtimeUsable`: an agent that is installed and signed in.
    pub fn is_usable(&self) -> bool {
        let status = self.status.as_deref().unwrap_or("");
        let auth = self.auth_state.as_deref().unwrap_or("");
        status != "missing" && status != "unavailable" && auth != "missing" && !self.id.is_empty()
    }

    pub fn label(&self) -> String {
        crate::art::provider_name(&self.provider)
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub name: String,
    pub path: Option<String>,
    pub sort_order: i64,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Message {
    pub id: Option<String>,
    pub turn_id: Option<String>,
    pub sequence: i64,
    pub role: Option<String>,
    pub content: Option<String>,
    pub created_at: Option<String>,
    pub status: Option<String>,
    pub meta: Option<Value>,
    /// The images a message carried, as the daemon stored them.
    #[serde(default, deserialize_with = "serde_defaults::null_as_default")]
    pub attachments: Vec<MessageAttachment>,
}

/// One image on a message: its name, the staged path it was sent from and,
/// when the daemon copied it, the file it keeps.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct MessageAttachment {
    pub name: String,
    pub path: String,
    pub file: Option<String>,
    pub media_type: Option<String>,
}

impl Message {
    pub fn role(&self) -> &str {
        self.role.as_deref().unwrap_or("")
    }

    pub fn text(&self) -> &str {
        self.content.as_deref().unwrap_or("")
    }


    /// Thinking is hidden: it is the agent's working notes, not the conversation.
    pub fn is_thinking(&self) -> bool {
        matches!(self.role(), "thinking" | "reasoning" | "analysis")
            || self.meta.as_ref().and_then(|meta| meta.get("kind")).and_then(Value::as_str) == Some("thinking")
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Tool {
    pub id: Option<String>,
    pub turn_id: Option<String>,
    pub kind: Option<String>,
    pub title: Option<String>,
    pub state: Option<String>,
    pub started_at: Option<String>,
}

impl Tool {
    /// One muted line for a tool step, the way `read_conversation` describes it.
    pub fn summary(&self) -> String {
        if let Some(title) = self.title.as_deref().map(str::trim).filter(|title| !title.is_empty()) {
            return title.to_owned();
        }
        match self.kind.as_deref().unwrap_or("") {
            "shell" | "command" | "terminal" => "Ran a command".to_owned(),
            "read" | "read_file" | "view" | "glob" | "list" => "Read a file".to_owned(),
            "edit" | "write" | "create" | "multi_edit" => "Edited a file".to_owned(),
            "search" | "grep" | "find" => "Searched the project".to_owned(),
            "fetch" | "web" | "http" => "Looked something up".to_owned(),
            "" => "Used a tool".to_owned(),
            other => format!("Used {}", other.replace('_', " ")),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Turn {
    pub id: String,
    pub state: Option<String>,
    pub created_at: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SessionLink {
    pub kind: Option<String>,
    pub peer_agent_id: Option<String>,
    pub peer_name: Option<String>,
    pub topic: Option<String>,
    /// The conversation this side conversation was started from, so the column
    /// can draw it under its own branch.
    pub origin_session_id: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Session {
    pub id: String,
    pub runtime_id: String,
    pub agent_id: Option<String>,
    pub provider: Option<String>,
    pub title: Option<String>,
    pub state: Option<String>,
    pub archived: bool,
    pub project_id: Option<String>,
    pub project_name: Option<String>,
    pub project_path: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    #[serde(default, deserialize_with = "serde_defaults::null_as_default")]
    pub messages: Vec<Message>,
    #[serde(default, deserialize_with = "serde_defaults::null_as_default")]
    pub tools: Vec<Tool>,
    #[serde(default, deserialize_with = "serde_defaults::null_as_default")]
    pub turns: Vec<Turn>,
    pub link: Option<SessionLink>,
    pub partial: bool,
    pub limited_until: Option<String>,
    pub workspace_label: Option<String>,
    /// What this conversation pinned for itself: the model, effort, speed and
    /// permission mode that differ from the bot's own defaults.
    pub model_lock: Option<ModelLock>,
    /// The project's own permission mode, when it sets one.
    pub project_approval_mode: Option<String>,
    /// How much of the provider's context the conversation has used, and which
    /// model that was: switching model with context costs the working memory.
    pub context_used: Option<f64>,
    pub context_model: Option<String>,
}

/// `SessionModelLock` in `types.ts`: a field that is absent follows the bot.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ModelLock {
    pub model: Option<String>,
    pub thinking: Option<String>,
    pub approval_mode: Option<String>,
    pub approval_mode_source: Option<String>,
    pub service_tier: Option<String>,
    pub plan_mode: Option<bool>,
}

impl Session {
    pub fn stamp(&self) -> &str {
        self.updated_at.as_deref().or(self.created_at.as_deref()).unwrap_or("")
    }

    /// The model this conversation runs on: its own pin, else the bot's.
    pub fn model(&self, agent: Option<&Agent>) -> Option<String> {
        self.model_lock
            .as_ref()
            .and_then(|lock| lock.model.clone())
            .or_else(|| agent.and_then(|agent| agent.model.clone()))
    }

    /// The permission mode in force here (`sessionApprovalMode`): the
    /// conversation's own pin, then its project's, then the bot's.
    pub fn permission_mode(&self, agent: Option<&Agent>) -> (String, bool, bool) {
        let pinned = self
            .model_lock
            .as_ref()
            .and_then(|lock| lock.approval_mode.as_deref())
            .map(str::trim)
            .filter(|mode| APPROVAL_MODES.contains(mode));
        if let Some(mode) = pinned {
            return (mode.to_owned(), true, false);
        }
        let project = self
            .project_approval_mode
            .as_deref()
            .map(str::trim)
            .filter(|mode| APPROVAL_MODES.contains(mode));
        if let Some(mode) = project {
            return (mode.to_owned(), false, true);
        }
        (agent.map(Agent::permission_mode).unwrap_or_else(|| "ask".to_owned()), false, false)
    }

    pub fn title_or_default(&self) -> &str {
        self.title.as_deref().map(str::trim).filter(|title| !title.is_empty()).unwrap_or("New chat")
    }

    pub fn state_word(&self) -> &str {
        self.state.as_deref().unwrap_or("")
    }

    /// The states the app counts as busy (`BUSY` in `ui/teamSelectors.ts`).
    pub fn is_busy(&self) -> bool {
        matches!(self.state_word().to_lowercase().as_str(), "starting" | "working" | "cancelling" | "waiting_permission")
    }

    pub fn is_side(&self) -> bool {
        self.link.as_ref().and_then(|link| link.kind.as_deref()) == Some("side")
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Permission {
    pub id: String,
    pub session_id: Option<String>,
    pub title: Option<String>,
    pub detail: Option<String>,
    pub description: Option<String>,
    pub tool: Option<String>,
    pub command: Option<String>,
    #[serde(default, deserialize_with = "serde_defaults::null_as_default")]
    pub choices: Vec<String>,
    pub kind: Option<String>,
    pub status: Option<String>,
    pub created_at: Option<String>,
    pub expires_at: Option<String>,
    /// The project a "always allow" rule could cover (`projectName`).
    pub project_name: Option<String>,
    /// Whether the daemon offered to remember this request, and its sentence.
    pub rememberable: bool,
    pub remember_label: Option<String>,
    /// The bot's own questions, for a `kind: "question"` card.
    #[serde(default, deserialize_with = "serde_defaults::null_as_default")]
    pub questions: Vec<UserQuestion>,
    /// What the CEO proposes, carried by a `kind: "proposal"` card.
    pub proposal: Option<Proposal>,
}

/// One structured question a bot asks (`UserQuestion` in `types.ts`).
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct UserQuestion {
    pub id: String,
    pub header: String,
    pub question: String,
    #[serde(default, deserialize_with = "serde_defaults::null_as_default")]
    pub options: Vec<QuestionOption>,
    pub multi_select: bool,
    pub allow_other: bool,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct QuestionOption {
    pub label: String,
    pub description: Option<String>,
}

/// `ProposalCard` in `types.ts`.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Proposal {
    pub id: String,
    pub kind: String,
    pub payload: Option<Value>,
}

impl Proposal {
    pub fn text(&self, key: &str) -> &str {
        self.payload
            .as_ref()
            .and_then(|payload| payload.get(key))
            .and_then(Value::as_str)
            .unwrap_or("")
    }
}

impl Permission {
    /// A question or a proposal is answered with its own card, not Allow/Deny.
    pub fn is_question(&self) -> bool {
        matches!(self.kind.as_deref(), Some("question"))
    }

    /// A proposal needs `proposal.decide`, not `permission.reply`.
    pub fn is_proposal(&self) -> bool {
        self.proposal.is_some() || matches!(self.kind.as_deref(), Some("proposal"))
    }

    pub fn proposal_kind(&self) -> Option<&str> {
        let proposal = self.proposal.as_ref()?;
        Some(proposal.kind.as_str())
    }

    /// The sentence under an approval card's title: the daemon's own detail, the
    /// command's first line, or a plain "the agent asked to use X".
    pub fn body(&self) -> String {
        if let Some(detail) = self.detail.as_deref().map(str::trim).filter(|text| !text.is_empty()) {
            return detail.to_owned();
        }
        if let Some(description) = self.description.as_deref().map(str::trim).filter(|text| !text.is_empty()) {
            return description.to_owned();
        }
        if self.command.is_some() {
            return String::new();
        }
        match self.tool.as_deref().map(str::trim).filter(|text| !text.is_empty()) {
            Some(tool) => format!("The agent requested permission to use {tool}."),
            None => "The agent requested permission.".to_owned(),
        }
    }
}

/// The word a choice shows (`permissionChoiceLabel` in `ui/permissionChoices.ts`).
/// A provider's own id is never renamed; only the shared spellings are.
pub fn permission_choice_label(choice: &str) -> String {
    match choice {
        "allow_once" | "allow" | "approve" | "accept" => "Allow once".to_owned(),
        "allow_session" => "Allow this session".to_owned(),
        "deny" | "reject" | "decline" => "Deny".to_owned(),
        "reject_once" => "Reject once".to_owned(),
        "reject_always" => "Reject always".to_owned(),
        other => {
            let mut chars = other.replace(['_', '-'], " ").to_lowercase().chars().collect::<Vec<_>>();
            if let Some(first) = chars.first_mut() {
                *first = first.to_uppercase().next().unwrap_or(*first);
            }
            chars.into_iter().collect()
        }
    }
}

/// The allow and deny choices, found by their first word
/// (`choicesForShortcuts` in `ui/ApprovalCard.tsx`), so Enter and Escape pick
/// the same buttons the React card does.
pub fn approval_shortcuts(choices: &[String]) -> (Option<String>, Option<String>) {
    let first_word = |choice: &str| {
        choice
            .replace(['_', '/', '.', '-'], " ")
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_lowercase()
    };
    let allow = choices
        .iter()
        .find(|choice| matches!(first_word(choice).as_str(), "allow" | "approve" | "accept"))
        .cloned();
    let deny = choices
        .iter()
        .find(|choice| matches!(first_word(choice).as_str(), "deny" | "reject" | "decline"))
        .cloned();
    (allow, deny)
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Snapshot {
    pub sequence: u64,
    #[serde(default, deserialize_with = "serde_defaults::null_as_default")]
    pub agents: Vec<Agent>,
    #[serde(default, deserialize_with = "serde_defaults::null_as_default")]
    pub sessions: Vec<Session>,
    #[serde(default, deserialize_with = "serde_defaults::null_as_default")]
    pub projects: Vec<Project>,
    #[serde(default, deserialize_with = "serde_defaults::null_as_default")]
    pub permissions: Vec<Permission>,
    /// The coding agents the daemon knows. The mark needs each one's provider.
    #[serde(default, deserialize_with = "serde_defaults::null_as_default")]
    pub runtimes: Vec<Runtime>,
    /// The plan limits each provider reported (`quota.list`, `quota.updated`).
    #[serde(default, deserialize_with = "serde_defaults::null_as_default")]
    pub quotas: Vec<Value>,
    /// Conversations this window saw deleted. A late event must not revive one.
    #[serde(skip)]
    pub deleted_sessions: Vec<String>,
    /// Projects whose hiring changed since the last snapshot, as the event names
    /// them. The window reloads each one's `project.members`.
    #[serde(skip)]
    pub dirty_members: Vec<String>,
    /// Agents only arrive in a snapshot, so a hiring event asks for a fresh one.
    #[serde(skip)]
    pub refresh_wanted: bool,
    /// The queued messages of each conversation, from `session.queue.list`.
    #[serde(skip)]
    pub queues: std::collections::BTreeMap<String, QueueState>,
    /// Conversations whose queue changed, so the window reloads it.
    #[serde(skip)]
    pub dirty_queues: Vec<String>,
}

/// A conversation's queue: what waits to be sent, and whether it is paused.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct QueueState {
    pub items: Vec<QueuedPrompt>,
    pub paused: bool,
}

/// One queued message (`QueuedPrompt` in `types.ts`).
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct QueuedPrompt {
    pub id: String,
    pub session_id: String,
    pub text: String,
    pub attachments: Vec<Value>,
    /// `user`, `send_later`, `retry` or a daemon kind.
    #[serde(rename = "kind")]
    pub kind_name: String,
    pub due_at: Option<String>,
    pub status: String,
    pub error: Option<String>,
    pub position: i64,
    pub created_at: Option<String>,
    pub when_limit_resets: bool,
    pub ambiguous: bool,
}

impl Snapshot {
    pub fn agent(&self, id: &str) -> Option<&Agent> {
        self.agents.iter().find(|agent| agent.id == id)
    }

    pub fn session(&self, id: &str) -> Option<&Session> {
        self.sessions.iter().find(|session| session.id == id)
    }

    pub fn manager(&self) -> Option<&Agent> {
        self.agents.iter().find(|agent| agent.is_manager() && !agent.archived)
    }

    pub fn project_name(&self, id: Option<&str>) -> Option<&str> {
        let id = id?;
        self.projects.iter().find(|project| project.id == id).map(|project| project.name.as_str())
    }

    /// A bot's provider, through the runtime it runs on.
    pub fn provider_of(&self, agent: &Agent) -> Option<String> {
        self.runtimes
            .iter()
            .find(|runtime| runtime.id == agent.runtime_id)
            .map(|runtime| runtime.provider.clone())
            .filter(|provider| !provider.is_empty())
    }

    /// The runtime table's own order, usable agents first.
    pub fn usable_runtimes(&self) -> Vec<&Runtime> {
        self.runtimes.iter().filter(|runtime| runtime.is_usable()).collect()
    }

    /// The conversations a bot has, newest first, without side conversations.
    pub fn conversations_for(&self, agent_id: &str) -> Vec<Session> {
        let mut rows: Vec<Session> = self
            .sessions
            .iter()
            .filter(|session| session.agent_id.as_deref() == Some(agent_id) && !session.is_side() && !session.archived)
            .cloned()
            .collect();
        rows.sort_by(|a, b| b.stamp().cmp(a.stamp()).then_with(|| b.id.cmp(&a.id)));
        rows
    }

    /// The conversations of one bot that were archived, newest first.
    pub fn archived_conversations_for(&self, agent_id: &str) -> Vec<Session> {
        let mut rows: Vec<Session> = self
            .sessions
            .iter()
            .filter(|session| session.agent_id.as_deref() == Some(agent_id) && !session.is_side() && session.archived)
            .cloned()
            .collect();
        rows.sort_by(|a, b| b.stamp().cmp(a.stamp()).then_with(|| b.id.cmp(&a.id)));
        rows
    }

    /// The queued messages of one conversation.
    pub fn queue(&self, session_id: &str) -> QueueState {
        self.queues.get(session_id).cloned().unwrap_or_default()
    }

    /// The waiting approval for one conversation, if there is one.
    pub fn pending_permission(&self, session_id: &str) -> Option<&Permission> {
        self.permissions.iter().find(|permission| permission.session_id.as_deref() == Some(session_id))
    }
}

/// Apply one ordered event. Events at or behind the snapshot are dropped.
pub fn apply_event(snapshot: &mut Snapshot, event: &DaemonEvent) {
    if event.sequence != 0 && event.sequence <= snapshot.sequence {
        return;
    }
    snapshot.sequence = snapshot.sequence.max(event.sequence);
    let payload = &event.payload;
    match event.event_type.as_str() {
        "session.changed" => apply_session_changed(snapshot, payload),
        "session.deleted" => {
            let id = payload.get("sessionId").or_else(|| payload.get("id")).and_then(Value::as_str);
            if let Some(id) = id {
                snapshot.sessions.retain(|session| session.id != id);
                if !snapshot.deleted_sessions.iter().any(|seen| seen == id) {
                    snapshot.deleted_sessions.push(id.to_owned());
                }
            }
        }
        "project.changed" => {
            apply_project_changed(snapshot, payload);
            snapshot.refresh_wanted = true;
        }
        "proposal.changed" => {
            snapshot.refresh_wanted = true;
        }
        "permission.requested" => {
            let row = payload.get("permission").cloned().unwrap_or_else(|| payload.clone());
            if let Ok(permission) = serde_json::from_value::<Permission>(row) {
                snapshot.permissions.retain(|existing| existing.id != permission.id);
                snapshot.permissions.push(permission);
            }
        }
        "permission.resolved" => {
            let id = payload
                .get("permissionId")
                .or_else(|| payload.get("permission").and_then(|permission| permission.get("id")))
                .or_else(|| payload.get("id"))
                .and_then(Value::as_str);
            if let Some(id) = id {
                snapshot.permissions.retain(|permission| permission.id != id);
            }
        }
        "message.delta" | "message.completed" => apply_message(snapshot, &event.event_type, payload),
        "tool.changed" | "command.changed" => apply_tool(snapshot, payload),
        "turn.completed" => set_turn_state(snapshot, payload, Some("completed")),
        "turn.error" => set_turn_state(snapshot, payload, None),
        "runtime.changed" => {
            if let Some(rows) = payload.get("runtimes").and_then(Value::as_array) {
                if let Ok(runtimes) = serde_json::from_value::<Vec<Runtime>>(Value::Array(rows.clone())) {
                    snapshot.runtimes = runtimes;
                }
            }
        }
        // One provider's plan limits changed: keep every other row as it was.
        "quota.updated" => {
            let provider = payload.get("provider").and_then(Value::as_str).unwrap_or("");
            let row = payload.get("quota").filter(|row| row.is_object()).cloned().unwrap_or_else(|| payload.clone());
            if !provider.is_empty() {
                snapshot.quotas.retain(|quota| quota.get("provider").and_then(Value::as_str) != Some(provider));
                let mut row = row;
                if row.get("provider").is_none() {
                    row["provider"] = Value::String(provider.to_owned());
                }
                snapshot.quotas.push(row);
            }
        }
        // The queue is read back on demand; this only says which conversation's.
        "queue.changed" => {
            if let Some(session_id) = payload.get("sessionId").and_then(Value::as_str) {
                if !snapshot.dirty_queues.iter().any(|seen| seen == session_id) {
                    snapshot.dirty_queues.push(session_id.to_owned());
                }
            }
        }
        // Hiring changes who is in a project and can create an agent, and agents
        // only arrive in a full snapshot: reload the members, and mark the
        // snapshot stale so the window takes a fresh one.
        "project.members.changed" => {
            if let Some(project_id) = payload.get("projectId").and_then(Value::as_str) {
                if !snapshot.dirty_members.iter().any(|seen| seen == project_id) {
                    snapshot.dirty_members.push(project_id.to_owned());
                }
            }
            snapshot.refresh_wanted = true;
        }
        _ => {}
    }
}

fn payload_session_id(payload: &Value) -> Option<&str> {
    payload.get("sessionId").and_then(Value::as_str)
}

fn apply_session_changed(snapshot: &mut Snapshot, payload: &Value) {
    let row = payload.get("session").cloned().unwrap_or_else(|| payload.clone());
    let Some(id) = row.get("id").and_then(Value::as_str) else { return };
    if snapshot.deleted_sessions.iter().any(|deleted| deleted == id) {
        return;
    }
    if let Some(index) = snapshot.sessions.iter().position(|session| session.id == id) {
        patch_session(&mut snapshot.sessions[index], &row);
        return;
    }
    if let Ok(session) = serde_json::from_value::<Session>(row) {
        snapshot.sessions.push(session);
    }
}

/// Copy the fields a `session.changed` row really carries. The streamed fields
/// (messages, tools, turns) are kept unless the row brings its own, exactly as
/// the React window merges them.
fn patch_session(session: &mut Session, row: &Value) {
    if let Some(title) = row.get("title").and_then(Value::as_str) {
        session.title = Some(title.to_owned());
    }
    if let Some(state) = row.get("state").and_then(Value::as_str) {
        session.state = Some(state.to_owned());
    }
    if let Some(archived) = row.get("archived").and_then(Value::as_bool) {
        session.archived = archived;
    }
    if let Some(updated) = row.get("updatedAt").and_then(Value::as_str) {
        session.updated_at = Some(updated.to_owned());
    }
    if let Some(project_id) = row.get("projectId") {
        session.project_id = project_id.as_str().map(str::to_owned);
    }
    if let Some(project_name) = row.get("projectName") {
        session.project_name = project_name.as_str().map(str::to_owned);
    }
    if row.get("modelLock").is_some_and(|value| value.is_object()) {
        session.model_lock = serde_json::from_value(row["modelLock"].clone()).ok();
    }
    if let Some(mode) = row.get("projectApprovalMode") {
        session.project_approval_mode = mode.as_str().map(str::to_owned);
    }
    if let Some(used) = row.get("contextUsed").and_then(Value::as_f64) {
        session.context_used = Some(used);
    }
    if let Some(model) = row.get("contextModel") {
        session.context_model = model.as_str().map(str::to_owned);
    }
    if let Some(rows) = row.get("messages").filter(|rows| rows.as_array().is_some_and(|rows| !rows.is_empty())) {
        if let Ok(messages) = serde_json::from_value::<Vec<Message>>(rows.clone()) {
            session.messages = messages;
        }
    }
    if let Some(rows) = row.get("tools").filter(|rows| rows.as_array().is_some_and(|rows| !rows.is_empty())) {
        if let Ok(tools) = serde_json::from_value::<Vec<Tool>>(rows.clone()) {
            session.tools = tools;
        }
    }
    if let Some(rows) = row.get("turns").filter(|rows| rows.as_array().is_some_and(|rows| !rows.is_empty())) {
        if let Ok(turns) = serde_json::from_value::<Vec<Turn>>(rows.clone()) {
            session.turns = turns;
        }
    }
}

fn apply_project_changed(snapshot: &mut Snapshot, payload: &Value) {
    let action = payload.get("action").and_then(Value::as_str).unwrap_or("");
    if action == "deleted" {
        let id = payload.get("projectId").and_then(Value::as_str);
        if let Some(id) = id {
            snapshot.projects.retain(|project| project.id != id);
        }
        return;
    }
    let Some(row) = payload.get("project") else { return };
    if let Ok(project) = serde_json::from_value::<Project>(row.clone()) {
        if project.id.is_empty() {
            return;
        }
        snapshot.projects.retain(|existing| existing.id != project.id);
        snapshot.projects.push(project);
    }
}

fn apply_message(snapshot: &mut Snapshot, event_type: &str, payload: &Value) {
    let Some(session_id) = payload_session_id(payload) else { return };
    let Some(session) = snapshot.sessions.iter_mut().find(|session| session.id == session_id) else { return };
    let row = payload.get("message").cloned().unwrap_or_else(|| payload.clone());
    let turn_id = payload
        .get("turnId")
        .and_then(Value::as_str)
        .or_else(|| row.get("turnId").and_then(Value::as_str))
        .map(str::to_owned);
    let role = payload
        .get("role")
        .and_then(Value::as_str)
        .or_else(|| row.get("role").and_then(Value::as_str))
        .map(str::to_owned);
    let message_id = payload
        .get("messageId")
        .and_then(Value::as_str)
        .or_else(|| row.get("id").and_then(Value::as_str))
        .map(str::to_owned);
    let delta = payload.get("delta").and_then(Value::as_str).unwrap_or("");

    let mut index = message_id
        .as_deref()
        .and_then(|id| session.messages.iter().position(|message| message.id.as_deref() == Some(id)));
    if index.is_none() {
        // Streamed text finds its row by turn and role. A turn can hold many
        // notices, so a notice only ever matches its own id.
        if let (Some(turn_id), Some(role)) = (turn_id.as_deref(), role.as_deref()) {
            if role != "notice" {
                index = session.messages.iter().position(|message| {
                    message.turn_id.as_deref() == Some(turn_id) && message.role.as_deref() == Some(role)
                });
            }
        }
    }

    match index {
        Some(index) => {
            let message = &mut session.messages[index];
            if event_type == "message.delta" {
                let mut content = message.content.clone().unwrap_or_default();
                content.push_str(delta);
                message.content = Some(content);
            } else if let Some(content) = row.get("content").and_then(Value::as_str) {
                message.content = Some(content.to_owned());
            }
            if let Some(status) = row.get("status").and_then(Value::as_str) {
                message.status = Some(status.to_owned());
            }
        }
        None => {
            let mut message: Message = serde_json::from_value(row).unwrap_or_default();
            if message.id.is_none() {
                message.id = message_id;
            }
            if message.turn_id.is_none() {
                message.turn_id = turn_id;
            }
            if message.role.is_none() {
                message.role = role;
            }
            if event_type == "message.delta" {
                message.content = Some(delta.to_owned());
            }
            session.messages.push(message);
        }
    }
    bump_session(session);
}

fn apply_tool(snapshot: &mut Snapshot, payload: &Value) {
    let Some(session_id) = payload_session_id(payload) else { return };
    let Some(session) = snapshot.sessions.iter_mut().find(|session| session.id == session_id) else { return };
    let row = payload.get("tool").or_else(|| payload.get("command")).cloned().unwrap_or_else(|| payload.clone());
    let Ok(tool) = serde_json::from_value::<Tool>(row) else { return };
    match tool.id.as_deref().and_then(|id| session.tools.iter().position(|existing| existing.id.as_deref() == Some(id))) {
        Some(index) => {
            let existing = &mut session.tools[index];
            if tool.title.is_some() {
                existing.title = tool.title;
            }
            if tool.kind.is_some() {
                existing.kind = tool.kind;
            }
            if tool.state.is_some() {
                existing.state = tool.state;
            }
        }
        None => session.tools.push(tool),
    }
    if session.state.as_deref() != Some("waiting_permission") {
        session.state = Some("working".to_owned());
    }
}

fn set_turn_state(snapshot: &mut Snapshot, payload: &Value, state: Option<&str>) {
    let Some(session_id) = payload_session_id(payload) else { return };
    let Some(session) = snapshot.sessions.iter_mut().find(|session| session.id == session_id) else { return };
    let turn_id = payload.get("turnId").and_then(Value::as_str);
    let next = state
        .map(str::to_owned)
        .or_else(|| payload.get("state").and_then(Value::as_str).map(str::to_owned))
        .unwrap_or_else(|| "error".to_owned());
    if let Some(turn_id) = turn_id {
        if let Some(turn) = session.turns.iter_mut().find(|turn| turn.id == turn_id) {
            turn.state = Some(next.clone());
        }
    }
    session.state = Some(next);
    bump_session(session);
}

fn bump_session(session: &mut Session) {
    session.partial = false;
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn event(sequence: u64, event_type: &str, payload: Value) -> DaemonEvent {
        DaemonEvent { v: 1, sequence, event_type: event_type.to_owned(), payload }
    }

    fn snapshot_with_one_session() -> Snapshot {
        let mut snapshot = Snapshot { sequence: 10, ..Snapshot::default() };
        snapshot.sessions.push(Session {
            id: "s1".into(),
            agent_id: Some("ada".into()),
            state: Some("working".into()),
            ..Session::default()
        });
        snapshot
    }

    #[test]
    fn streamed_deltas_accumulate_on_one_row() {
        let mut snapshot = snapshot_with_one_session();
        apply_event(&mut snapshot, &event(11, "message.delta", json!({"sessionId":"s1","turnId":"t1","messageId":"m1","role":"assistant","delta":"Check"})));
        apply_event(&mut snapshot, &event(12, "message.delta", json!({"sessionId":"s1","turnId":"t1","messageId":"m1","role":"assistant","delta":"ing the tests"})));
        let messages = &snapshot.sessions[0].messages;
        assert_eq!(messages.len(), 1, "one streamed row, not one per delta");
        assert_eq!(messages[0].content.as_deref(), Some("Checking the tests"));
    }

    #[test]
    fn a_completed_message_replaces_the_streamed_text() {
        let mut snapshot = snapshot_with_one_session();
        apply_event(&mut snapshot, &event(11, "message.delta", json!({"sessionId":"s1","turnId":"t1","messageId":"m1","role":"assistant","delta":"partial"})));
        apply_event(&mut snapshot, &event(12, "message.completed", json!({"sessionId":"s1","message":{"id":"m1","turnId":"t1","role":"assistant","content":"the whole reply"}})));
        assert_eq!(snapshot.sessions[0].messages[0].content.as_deref(), Some("the whole reply"));
    }

    #[test]
    fn an_event_behind_the_snapshot_is_ignored() {
        let mut snapshot = snapshot_with_one_session();
        apply_event(&mut snapshot, &event(9, "message.delta", json!({"sessionId":"s1","delta":"stale"})));
        assert!(snapshot.sessions[0].messages.is_empty());
        assert_eq!(snapshot.sequence, 10);
    }

    #[test]
    fn session_changed_keeps_the_messages_it_does_not_carry() {
        let mut snapshot = snapshot_with_one_session();
        snapshot.sessions[0].messages.push(Message { id: Some("m1".into()), content: Some("kept".into()), ..Message::default() });
        apply_event(&mut snapshot, &event(11, "session.changed", json!({"session":{"id":"s1","title":"Fix the tests","state":"idle"}})));
        assert_eq!(snapshot.sessions[0].title_or_default(), "Fix the tests");
        assert_eq!(snapshot.sessions[0].messages.len(), 1);
        assert_eq!(snapshot.sessions[0].state_word(), "idle");
    }

    #[test]
    fn a_waiting_approval_arrives_and_is_cleared() {
        let mut snapshot = snapshot_with_one_session();
        apply_event(&mut snapshot, &event(11, "permission.requested", json!({"permission":{"id":"p1","sessionId":"s1","title":"Run tests","choices":["allow_once","deny"]}})));
        assert!(snapshot.pending_permission("s1").is_some());
        apply_event(&mut snapshot, &event(12, "permission.resolved", json!({"permissionId":"p1"})));
        assert!(snapshot.pending_permission("s1").is_none());
    }

    #[test]
    fn a_deleted_conversation_is_not_revived_by_a_late_event() {
        let mut snapshot = snapshot_with_one_session();
        apply_event(&mut snapshot, &event(11, "session.deleted", json!({"sessionId":"s1"})));
        apply_event(&mut snapshot, &event(12, "session.changed", json!({"session":{"id":"s1","state":"idle"}})));
        assert!(snapshot.sessions.is_empty());
    }

    #[test]
    fn the_tool_summary_is_one_plain_line() {
        let tool = Tool { kind: Some("shell".into()), ..Tool::default() };
        assert_eq!(tool.summary(), "Ran a command");
        let titled = Tool { kind: Some("shell".into()), title: Some("Run cargo test".into()), ..Tool::default() };
        assert_eq!(titled.summary(), "Run cargo test");
    }
}
