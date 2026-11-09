//! The composer, complete: the model, effort and permission controls of this
//! conversation (`ui/ComposerControls.tsx`, `ui/SessionExecutionControls.tsx`),
//! the `/command` menu (`ui/SlashMenu.tsx`), the queue above the box
//! (`ui/PromptQueue.tsx`) and the images a message carries.
//!
//! The rules are the React app's own: a choice writes the bot (so it holds for
//! later conversations) and, for a running conversation, pins the conversation
//! with `session.model.update` / `session.permissions.set`. A model change with
//! context behind it asks first; a turn that is running takes the prompt into
//! the queue instead of failing.

use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use gpui::{div, prelude::*, px, AnyElement, Context, FontWeight, Image, SharedString, Window};

use crate::art::{self, Glyph};
use crate::daemon::{Command, PromptMethod};
use crate::paths;
use crate::state::slash::{slash_items, slash_query, SlashItem, SlashKind};
use crate::state::{approval_mode_copy, QueuedPrompt, Session, UserQuestion};
use crate::state::APPROVAL_MODES;
use crate::theme::{self, Colors};
use crate::ui::menu::{Menu, MenuChoice, MenuItem};
use crate::ui::root::{Catalog, ComposerImage, ModelChoice, PendingModel, Root, SlashState};
use crate::ui::text_input::TextInput;

/// The text change a picked `/command` makes.
enum SlashEdit {
    /// An action: its own `/word` comes back out of the message.
    Delete { start: usize, end: usize },
    /// A skill or a CEO command: the whole message, command first.
    Replace(String),
}

/// A model change starts a new provider session, which only costs the owner
/// something when the conversation already has user turns or a context
/// (`modelChangeNeedsWarning` in `ui/SessionExecutionControls.tsx`).
pub fn model_change_needs_warning(session: &Session, next_model: Option<&str>, current_model: Option<&str>) -> bool {
    let Some(next) = next_model else { return false };
    if Some(next) == current_model {
        return false;
    }
    if session.messages.iter().any(|message| message.role() == "user") {
        return true;
    }
    if session.context_used.is_some_and(|used| used > 0.0) {
        return true;
    }
    session.context_model.as_deref().is_some_and(|model| !model.is_empty())
}

/// The sentence under "Switch model and start fresh?"
/// (`modelChangeBody`).
pub fn model_change_body(plan_reason: Option<&str>) -> String {
    let what = plan_reason
        .map(str::trim)
        .filter(|reason| !reason.is_empty())
        .unwrap_or("The new model starts a new provider session without the working memory of this chat, so its replies can drift until it catches up.");
    format!("{what} Your saved messages, drafts, attachments and project stay as they are.")
}

/// A session that is mid-turn: the app's `BUSY` list.
pub fn session_busy(session: Option<&Session>) -> bool {
    session.map(Session::is_busy).unwrap_or(false)
}

/// The queue as the daemon lists it: scheduled and retry items first, then the
/// plain ones in their own order (`ORDER BY` in `queue_list`).
pub fn queue_order(items: &[QueuedPrompt]) -> Vec<QueuedPrompt> {
    let mut rows = items.to_vec();
    rows.sort_by(|a, b| {
        let rank = |item: &QueuedPrompt| if matches!(item.kind_name.as_str(), "send_later" | "retry") { 0 } else { 1 };
        rank(a)
            .cmp(&rank(b))
            .then_with(|| a.position.cmp(&b.position))
            .then_with(|| a.created_at.cmp(&b.created_at))
            .then_with(|| a.id.cmp(&b.id))
    });
    rows
}

/// One queued row's words: an automatic retry says so, a message with pictures
/// counts them (`queueRowText`).
pub fn queue_row_text(item: &QueuedPrompt) -> String {
    if item.kind_name == "retry" {
        return "Continue automatically".to_owned();
    }
    let flat = item.text.split_whitespace().collect::<Vec<_>>().join(" ");
    let images = item.attachments.len();
    let mut text = if flat.chars().count() > 120 {
        format!("{}…", flat.chars().take(119).collect::<String>().trim_end())
    } else {
        flat
    };
    if images > 0 {
        text = if text.is_empty() {
            format!("{images} image{}", if images == 1 { "" } else { "s" })
        } else {
            format!("{text} · {images} image{}", if images == 1 { "" } else { "s" })
        };
    }
    if text.is_empty() {
        "Empty message".to_owned()
    } else {
        text
    }
}

/// An effort level's own words: the provider's ids turned into names.
pub fn effort_label(level: &str) -> String {
    match level.trim().to_lowercase().as_str() {
        "" => "Default".to_owned(),
        "low" => "Low".to_owned(),
        "medium" => "Medium".to_owned(),
        "high" => "High".to_owned(),
        "max" => "Max".to_owned(),
        "ultra" | "ultracode" => "Ultra".to_owned(),
        "none" => "None".to_owned(),
        other => {
            let mut chars = other.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => "Default".to_owned(),
            }
        }
    }
}

/// What a send does when the window says the turn is busy: the app queues the
/// prompt rather than refusing it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Delivery {
    Send,
    Queue,
}

pub fn delivery_for(busy: bool, approvals_waiting: bool) -> Delivery {
    if busy || approvals_waiting {
        Delivery::Queue
    } else {
        Delivery::Send
    }
}

impl Root {
    /// The box's text changed (the owner typed): keep the `/command` menu in
    /// step with it. The text and the caret are handed in, because the box is
    /// leased by the caller.
    pub(crate) fn slash_from_text(&mut self, text: &str, cursor: usize, cx: &mut Context<Self>) {
        match slash_query(text, cursor) {
            Some((start, query)) => {
                let index = self
                    .slash
                    .as_ref()
                    .filter(|slash| slash.query == query && slash.start == start)
                    .map(|slash| slash.index)
                    .unwrap_or(0);
                self.slash = Some(SlashState { start, query, index });
            }
            None => self.slash = None,
        }
        cx.notify();
    }

    /// The rows the `/command` menu shows now.
    pub(crate) fn slash_rows(&self) -> Vec<SlashItem> {
        let Some(slash) = self.slash.as_ref() else { return Vec::new() };
        let is_manager = self.selected_agent().map(|agent| agent.is_manager()).unwrap_or(false);
        slash_items(&slash.query, &[], is_manager)
    }

    /// The model's name in the pill: the catalog's own label, else the id.
    pub(crate) fn model_label(&self, session: Option<&Session>, agent: Option<&crate::state::Agent>) -> Option<(String, String)> {
        let catalog = session.and_then(|session| self.catalogs.get(&session.runtime_id));
        let model = session.and_then(|session| session.model(agent)).or_else(|| agent.and_then(|agent| agent.model.clone()))?;
        if model.trim().is_empty() {
            return None;
        }
        let label = catalog
            .and_then(|catalog| catalog.model(Some(&model)))
            .map(|choice| choice.label.clone())
            .unwrap_or_else(|| model.clone());
        Some((model, label))
    }

    /// Ask for a runtime's model list once, for this conversation's catalog.
    pub(crate) fn request_catalog(&mut self, runtime_id: String, cx: &mut Context<Self>) {
        if runtime_id.is_empty() || !self.catalog_requested.insert(runtime_id.clone()) {
            return;
        }
        self.daemon_command(
            Command::Rpc {
                tag: "catalog".to_owned(),
                method: "runtime.models".to_owned(),
                params: serde_json::json!({ "runtimeId": runtime_id }),
            },
            cx,
        );
    }

    // ---- the composer -------------------------------------------------------

    /// The Send button: read the box, send, then empty and focus it. The button's
    /// listener holds `Root`, not the box, so touching the box here is safe.
    pub(crate) fn send(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.composer.read(cx).text().to_owned();
        if self.dispatch_send(text, cx) {
            self.composer.update(cx, |input, cx| input.clear(window, cx));
            self.focus_composer(window, cx);
        }
    }

    /// Send `text` to this conversation, or into its queue while a turn runs.
    /// Answers `true` when something went out. It never touches the box: Enter
    /// calls this while the box is leased, and empties it through the box it was
    /// handed.
    pub(crate) fn dispatch_send(&mut self, text: String, cx: &mut Context<Self>) -> bool {
        self.error = None;
        // A question's own answer goes back as that answer, not as a message.
        if self.send_question_answer(&text, cx) {
            return true;
        }
        let text = text.trim().to_owned();
        let images: Vec<crate::daemon::PromptAttachment> = self
            .images
            .iter()
            .filter(|image| image.ready())
            .filter_map(|image| image.path.clone().map(|path| crate::daemon::PromptAttachment { path, name: image.name.clone() }))
            .collect();
        if text.is_empty() && images.is_empty() {
            return false;
        }
        let Some(agent_id) = self.agent_id.clone() else {
            self.error = Some("Choose an employee first.".to_owned());
            cx.notify();
            return false;
        };
        let session = self.selected_session().cloned();
        let busy = session_busy(session.as_ref());
        let approvals_waiting = session
            .as_ref()
            .and_then(|session| self.snapshot().and_then(|snapshot| snapshot.pending_permission(&session.id)))
            .is_some();
        let method = match delivery_for(busy, approvals_waiting) {
            Delivery::Send => crate::daemon::PromptMethod::Prompt,
            Delivery::Queue => crate::daemon::PromptMethod::Queue,
        };
        // A message that starts with one of this bot's commands travels with it,
        // stored exactly as typed (`leading_command`).
        let is_manager = self.selected_agent().map(|agent| agent.is_manager()).unwrap_or(false);
        let command = crate::state::slash::leading_command(&text, &[], is_manager);
        match session.as_ref().map(|session| session.id.clone()) {
            Some(session_id) => {
                self.sent.push(text.clone());
                if self.sent.len() > 20 {
                    self.sent.remove(0);
                }
                let client_message_id = uuid::Uuid::new_v4().to_string();
                match command {
                    Some(command) => {
                        let mut params = serde_json::json!({
                            "sessionId": session_id,
                            "text": text,
                            "clientMessageId": client_message_id,
                            "command": { "kind": if command.kind == SlashKind::Manager { "manager" } else { "skill" }, "name": command.name },
                        });
                        if !images.is_empty() {
                            params["attachments"] = serde_json::json!(images
                                .iter()
                                .map(|attachment| serde_json::json!({ "path": attachment.path, "name": attachment.name }))
                                .collect::<Vec<_>>());
                        }
                        let method_name = match method {
                            PromptMethod::Prompt => "session.prompt",
                            PromptMethod::Queue => "session.queue.add",
                        };
                        self.daemon_command(Command::Rpc { tag: "sent".to_owned(), method: method_name.to_owned(), params }, cx);
                    }
                    None => {
                        self.daemon_command(
                            Command::SendPrompt { session_id, text, attachments: images, method, client_message_id },
                            cx,
                        );
                    }
                }
            }
            None => {
                // A first message creates the conversation; the prompt follows it.
                self.pending_prompt = Some(text);
                self.pending_attachments = images;
                self.new_conversation(agent_id, None, cx);
            }
        }
        self.images.clear();
        self.slash = None;
        self.queue_open = false;
        cx.notify();
        true
    }

    /// Up in an empty box recalls what was last sent here. The box is handed in,
    /// so this edits it directly and notifies the window as a whole.
    pub(crate) fn recall_last_prompt(&mut self, input: &mut TextInput, cx: &mut Context<Self>) -> bool {
        if !input.is_empty() {
            return false;
        }
        let Some(last) = self.sent.last().cloned() else { return false };
        input.set_text_quiet(last);
        cx.notify();
        true
    }

    /// The keys the composer wants before Enter: the `/command` menu's arrows,
    /// Enter and Escape, then the history. `input` is the box these keys belong
    /// to (leased by the caller), so it is edited directly.
    pub(crate) fn composer_key(
        &mut self,
        input: &mut TextInput,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let key = event.keystroke.key.as_str();
        if self.slash.is_some() {
            let rows = self.slash_rows();
            match key {
                "down" => {
                    let len = rows.len().max(1);
                    if let Some(slash) = self.slash.as_mut() {
                        slash.index = (slash.index + 1) % len;
                    }
                    cx.notify();
                    return true;
                }
                "up" => {
                    let len = rows.len().max(1);
                    if let Some(slash) = self.slash.as_mut() {
                        slash.index = (slash.index + len - 1) % len;
                    }
                    cx.notify();
                    return true;
                }
                "escape" => {
                    self.slash = None;
                    cx.notify();
                    return true;
                }
                "enter" => {
                    if let Some(row) = rows.get(self.slash.as_ref().map(|slash| slash.index).unwrap_or(0)).cloned() {
                        self.choose_slash(row, input, cx);
                        return true;
                    }
                }
                _ => {}
            }
        }
        if key == "up" && self.recall_last_prompt(input, cx) {
            return true;
        }
        if key == "v" && (event.keystroke.modifiers.control || event.keystroke.modifiers.platform) && self.clipboard_has_image(cx) {
            // A copied screenshot pastes as an attachment; anything else is the
            // text paste the box already handles.
            self.paste_image(cx);
            return true;
        }
        let _ = window;
        false
    }

    /// Whether the clipboard holds a picture rather than words.
    pub(crate) fn clipboard_has_image(&self, cx: &mut Context<Self>) -> bool {
        cx.read_from_clipboard()
            .map(|item| item.entries().iter().any(|entry| matches!(entry, gpui::ClipboardEntry::Image(_))))
            .unwrap_or(false)
    }

    /// A row of the `/command` menu was clicked. The box is not leased here, so
    /// its text edit goes through its entity and the action runs on the window.
    pub(crate) fn choose_slash_from_menu(&mut self, row: SlashItem, cx: &mut Context<Self>) {
        let (text, cursor) = {
            let input = self.composer.read(cx);
            (input.text().to_owned(), input.cursor())
        };
        let edit = self.slash_edit(&row, &text, cursor);
        let composer = self.composer.clone();
        composer.update(cx, |input, cx| {
            match &edit {
                SlashEdit::Delete { start, end } => input.delete_range_quiet(*start, *end),
                SlashEdit::Replace(text) => input.set_text_quiet(text.clone()),
            }
            cx.notify();
        });
        self.slash = None;
        self.run_slash_action(&row, cx);
    }

    /// What picking a `/command` does to the box's text.
    ///
    /// A skill or a CEO command stays in the message exactly as typed (with the
    /// words that follow it); an action takes its `/word` back out.
    fn slash_edit(&self, row: &SlashItem, text: &str, cursor: usize) -> SlashEdit {
        let Some(start) = self.slash.as_ref().map(|slash| slash.start) else {
            return SlashEdit::Replace(text.to_owned());
        };
        let end = start + 1 + self.slash.as_ref().map(|slash| slash.query.len()).unwrap_or(0);
        match row.kind {
            SlashKind::Skill | SlashKind::Manager => {
                let command = crate::state::slash::SlashCommand { kind: row.kind, name: row.name.clone() };
                let message = crate::state::slash::command_message(&command, "");
                let cursor = cursor.min(text.len());
                let tail = text[cursor..].trim_start().to_owned();
                SlashEdit::Replace(format!("{message} {tail}").trim_end().to_owned())
            }
            SlashKind::Action => SlashEdit::Delete { start, end },
        }
    }

    /// Run what a `/command` action asks for. It touches only `Root`.
    fn run_slash_action(&mut self, row: &SlashItem, cx: &mut Context<Self>) {
        if row.kind != SlashKind::Action {
            cx.notify();
            return;
        }
        match row.name.as_str() {
            "new" => {
                if let Some(agent_id) = self.agent_id.clone() {
                    self.new_conversation(agent_id, None, cx);
                }
            }
            "model" => self.open_model_menu_at(Self::no_pointer(), cx),
            "effort" => self.open_effort_menu_at(Self::no_pointer(), cx),
            "rename" => self.begin_rename(cx),
            "help" => self.shortcuts_open = true,
            "plan" => self.toggle_plan_mode(cx),
            _ => {}
        }
        cx.notify();
    }

    /// The keyboard path: the box is handed in (it is leased), so the same edit
    /// is applied to it directly and the action runs on the window.
    pub(crate) fn choose_slash(&mut self, row: SlashItem, input: &mut TextInput, cx: &mut Context<Self>) {
        let edit = self.slash_edit(&row, input.text(), input.cursor());
        match edit {
            SlashEdit::Delete { start, end } => input.delete_range_quiet(start, end),
            SlashEdit::Replace(text) => input.set_text_quiet(text),
        }
        self.slash = None;
        self.run_slash_action(&row, cx);
    }

    /// The keys a `/command` action opens its menu at, when no pointer is known.
    fn no_pointer() -> gpui::Point<gpui::Pixels> {
        gpui::point(gpui::px(220.0), gpui::px(180.0))
    }

    /// Plan mode is part of the model lock (`session.model.update` `planMode`).
    fn toggle_plan_mode(&mut self, cx: &mut Context<Self>) {
        let Some(session) = self.selected_session().cloned() else { return };
        if session.id.starts_with("draft:") {
            return;
        }
        let on = session.model_lock.as_ref().and_then(|lock| lock.plan_mode).unwrap_or(false);
        self.daemon_command(
            Command::Rpc {
                tag: "session".to_owned(),
                method: "session.model.update".to_owned(),
                params: serde_json::json!({ "sessionId": session.id, "planMode": !on }),
            },
            cx,
        );
    }

    // ---- the controls -------------------------------------------------------

    pub(crate) fn open_model_menu_at(&mut self, at: gpui::Point<gpui::Pixels>, cx: &mut Context<Self>) {
        let Some(session) = self.selected_session().cloned() else { return };
        if session.is_busy() {
            self.toast("The model can change once this turn ends. Your message and the queue are not affected.", cx);
            return;
        }
        let agent = self.selected_agent().cloned();
        let (current, _) = match self.model_label(Some(&session), agent.as_ref()) {
            Some((id, label)) => (Some(id), label),
            None => (None, String::new()),
        };
        let catalog = self.catalogs.get(&session.runtime_id).cloned().unwrap_or_default();
        let bot_default = agent.as_ref().and_then(|agent| agent.model.clone());
        let mut items = Vec::new();
        if catalog.models.is_empty() {
            // The runtime has not reported its models yet (or cannot): the row
            // says so and asks again, as the app's unavailable pill does.
            let runtime_id = session.runtime_id.clone();
            items.push(
                MenuItem::Choice(
                    MenuChoice::new("retry", "Ask for the model list again", Rc::new(move |root: &mut Root, cx: &mut Context<Root>| {
                        root.catalog_requested.remove(&runtime_id);
                        root.request_catalog(runtime_id.clone(), cx);
                        root.toast("Asking the coding agent for its models…", cx);
                    }))
                    .hint("This coding agent has not reported a model list."),
                ),
            );
        } else {
            // The pill shows the bot's own model until the owner picks one here;
            // a conversation's model cannot be unset again (`session.model.update`
            // refuses it), so the menu only offers models.
            let _ = &bot_default;
            for choice in &catalog.models {
                let id = choice.id.clone();
                let label = choice.label.clone();
                let provider = crate::art::provider_name(&choice.provider);
                let selected = current.as_deref() == Some(id.as_str());
                items.push(MenuItem::Choice(
                    MenuChoice::new(
                        format!("model-{id}"),
                        label,
                        Rc::new(move |root: &mut Root, cx: &mut Context<Root>| {
                            root.apply_model_choice(Some(id.clone()), cx);
                        }),
                    )
                    .hint(provider)
                    .checked(selected),
                ));
            }
        }
        let position = at;
        let provider = if catalog.provider.is_empty() {
            "this coding agent".to_owned()
        } else {
            crate::art::provider_name(&catalog.provider)
        };
        self.open_menu(
            Menu::new("model", position, items)
                .title("Model for this conversation")
                .note(format!("From {provider}. Applies to this conversation and the bot's new ones from the next message.")),
            cx,
        );
    }

    pub(crate) fn open_effort_menu_at(&mut self, at: gpui::Point<gpui::Pixels>, cx: &mut Context<Self>) {
        let Some(session) = self.selected_session().cloned() else { return };
        if session.is_busy() {
            self.toast("Effort can change once this turn ends.", cx);
            return;
        }
        let agent = self.selected_agent().cloned();
        let model = session.model(agent.as_ref());
        let catalog = self.catalogs.get(&session.runtime_id).cloned().unwrap_or_default();
        let levels = catalog
            .model(model.as_deref())
            .map(|choice| choice.thinking.clone())
            .unwrap_or_default();
        let current = session
            .model_lock
            .as_ref()
            .and_then(|lock| lock.thinking.clone())
            .or_else(|| agent.as_ref().and_then(|agent| agent.thinking.clone()));
        let mut items = Vec::new();
        if levels.is_empty() {
            items.push(MenuItem::Choice(
                MenuChoice::new("default", "The model's own default", Rc::new(move |root: &mut Root, cx: &mut Context<Root>| {
                    root.apply_effort(None, cx);
                }))
                .checked(current.is_none())
                .hint("This model does not publish effort levels."),
            ));
        } else {
            items.push(MenuItem::Choice(
                MenuChoice::new("default", "The model's own default", Rc::new(move |root: &mut Root, cx: &mut Context<Root>| {
                    root.apply_effort(None, cx);
                }))
                .checked(current.is_none()),
            ));
            for level in levels {
                let value = level.clone();
                items.push(MenuItem::Choice(
                    MenuChoice::new(
                        format!("effort-{level}"),
                        effort_label(&level),
                        Rc::new(move |root: &mut Root, cx: &mut Context<Root>| root.apply_effort(Some(value.clone()), cx)),
                    )
                    .checked(current.as_deref() == Some(level.as_str())),
                ));
            }
        }
        let position = at;
        self.open_menu(Menu::new("effort", position, items).title("Effort").note("Applies from the next message."), cx);
    }

    pub(crate) fn open_permission_menu_at(&mut self, at: gpui::Point<gpui::Pixels>, cx: &mut Context<Self>) {
        let Some(session) = self.selected_session().cloned() else { return };
        let agent = self.selected_agent().cloned();
        let (mode, pinned, from_project) = session.permission_mode(agent.as_ref());
        let bot_mode = agent.as_ref().map(crate::state::Agent::permission_mode).unwrap_or_else(|| "ask".to_owned());
        let mut items = Vec::new();
        for choice in APPROVAL_MODES {
            let (label, detail) = approval_mode_copy(choice);
            let value = choice.to_owned();
            items.push(MenuItem::Choice(
                MenuChoice::new(
                    format!("mode-{choice}"),
                    label,
                    Rc::new(move |root: &mut Root, cx: &mut Context<Root>| root.apply_permission(value.clone(), cx)),
                )
                .hint(detail)
                .tag((choice == bot_mode).then(|| "The bot's default".to_owned()))
                .checked(choice == mode),
            ));
        }
        let note = if from_project {
            "This project sets its own approvals, so this changes only this conversation."
        } else if pinned {
            "Set for this conversation. Choosing the bot's default follows the bot again."
        } else {
            "Applies to this bot from the next message."
        };
        let position = at;
        self.open_menu(Menu::new("permission", position, items).title("Mode").note(note), cx);
    }

    /// A model choice: the bot's default with `None`, else this conversation's.
    pub(crate) fn apply_model_choice(&mut self, model: Option<String>, cx: &mut Context<Self>) {
        let Some(session) = self.selected_session().cloned() else { return };
        let agent = self.selected_agent().cloned();
        let current = session.model(agent.as_ref());
        if model.is_some() && model == current {
            return;
        }
        if model.is_some() && model_change_needs_warning(&session, model.as_deref(), current.as_deref()) {
            self.pending_model = Some(PendingModel {
                session_id: session.id.clone(),
                model: model.clone().unwrap_or_default(),
                reason: String::new(),
                context_at_risk: true,
            });
            crate::trace::log(format!(
                "model change needs a confirmation: {} -> {}",
                current.as_deref().unwrap_or("the bot's default"),
                model.as_deref().unwrap_or("the bot's default")
            ));
            cx.notify();
            return;
        }
        self.write_model(model, cx);
    }

    /// Write the conversation's model, then the bot's default for new ones. A
    /// refused conversation write leaves both untouched (`save` in
    /// `ui/SessionExecutionControls.tsx`).
    pub(crate) fn write_model(&mut self, model: Option<String>, cx: &mut Context<Self>) {
        let Some(session) = self.selected_session().cloned() else { return };
        let Some(agent) = self.selected_agent().cloned() else { return };
        // A confirmation belongs to the conversation that asked; if the owner
        // moved on, it is dropped rather than applied somewhere else.
        if let Some(pending) = self.pending_model.as_ref() {
            if pending.session_id != session.id {
                self.pending_model = None;
                return;
            }
        }
        let current = session.model(Some(&agent));
        let changing = model != current;
        let mut params = serde_json::json!({ "sessionId": session.id, "model": model });
        if changing {
            params["expectedModel"] = serde_json::json!(current);
            params["confirmModelChange"] = serde_json::json!(true);
        }
        self.daemon_command(
            Command::Rpc { tag: "session-model".to_owned(), method: "session.model.update".to_owned(), params },
            cx,
        );
        self.daemon_command(
            Command::Rpc {
                tag: "agent".to_owned(),
                method: "agent.update".to_owned(),
                params: serde_json::json!({ "agentId": agent.id, "model": model }),
            },
            cx,
        );
        self.pending_model = None;
    }

    pub(crate) fn apply_effort(&mut self, level: Option<String>, cx: &mut Context<Self>) {
        let Some(session) = self.selected_session().cloned() else { return };
        let Some(agent) = self.selected_agent().cloned() else { return };
        let model = session.model(Some(&agent));
        self.daemon_command(
            Command::Rpc {
                tag: "session-model".to_owned(),
                method: "session.model.update".to_owned(),
                params: serde_json::json!({ "sessionId": session.id, "model": model, "thinking": level }),
            },
            cx,
        );
        self.daemon_command(
            Command::Rpc {
                tag: "agent".to_owned(),
                method: "agent.update".to_owned(),
                params: serde_json::json!({ "agentId": agent.id, "thinking": level }),
            },
            cx,
        );
    }

    /// A permission choice: the bot's own setting, and this conversation's pin
    /// cleared, so every later conversation agrees (`apply` in
    /// `ui/ComposerControls.tsx`). A project with its own mode keeps the choice
    /// on this conversation alone.
    pub(crate) fn apply_permission(&mut self, mode: String, cx: &mut Context<Self>) {
        let Some(session) = self.selected_session().cloned() else { return };
        let Some(agent) = self.selected_agent().cloned() else { return };
        if mode == "bypass" && session.permission_mode(Some(&agent)).0 != "bypass" {
            self.confirm_bypass = Some(session.id.clone());
            cx.notify();
            return;
        }
        self.write_permission(mode, cx);
    }

    pub(crate) fn write_permission(&mut self, mode: String, cx: &mut Context<Self>) {
        let Some(session) = self.selected_session().cloned() else { return };
        let Some(agent) = self.selected_agent().cloned() else { return };
        let bot_mode = agent.permission_mode();
        // Full access always persists: writing null there would clear the bot's
        // setting and drop the conversation back to the global default.
        let write = if mode == "bypass" {
            Some(mode.clone())
        } else if mode == bot_mode {
            None
        } else {
            Some(mode.clone())
        };
        if session.project_approval_mode.is_some() {
            self.daemon_command(
                Command::Rpc {
                    tag: "session".to_owned(),
                    method: "session.permissions.set".to_owned(),
                    params: serde_json::json!({ "sessionId": session.id, "approvalMode": mode }),
                },
                cx,
            );
            return;
        }
        self.daemon_command(
            Command::Rpc {
                tag: "agent".to_owned(),
                method: "agent.update".to_owned(),
                params: serde_json::json!({ "agentId": agent.id, "approvalMode": write }),
            },
            cx,
        );
        if !session.id.starts_with("draft:") {
            self.daemon_command(
                Command::Rpc {
                    tag: "session".to_owned(),
                    method: "session.permissions.set".to_owned(),
                    params: serde_json::json!({ "sessionId": session.id, "approvalMode": null }),
                },
                cx,
            );
        }
    }

    // ---- the box and its rows ----------------------------------------------

    pub(crate) fn render_composer(&mut self, colors: Colors, session: Option<&Session>, approvals_waiting: bool, cx: &mut Context<Self>) -> AnyElement {
        let busy = session_busy(session);
        let queue = session.and_then(|session| self.snapshot().map(|snapshot| snapshot.queue(&session.id)));
        // `.composer-wrap`: min(800px, 100% - 40px), centred, 6px above and 14px below.
        let mut column = div().w_full().max_w(px(COMPOSER_WIDTH)).flex().flex_col().gap(px(6.0));

        if let Some(queue) = queue.as_ref() {
            if !queue.items.is_empty() {
                column = column.child(self.render_queue(queue.items.clone(), queue.paused, busy, colors, cx));
            }
        }
        if !self.images.is_empty() {
            column = column.child(self.render_attachment_tray(colors, cx));
        }
        if self.slash.is_some() {
            let rows = self.slash_rows();
            if !rows.is_empty() {
                column = column.child(self.render_slash_menu(rows, colors, cx));
            }
        }

        let hint = if approvals_waiting {
            "Answer the request above first"
        } else if busy {
            "Your message queues for after this turn"
        } else {
            "Enter sends · Shift+Enter adds a line · paste an image"
        };

        let mut controls = div().flex_none().flex().items_center().gap(px(4.0));
        if let Some(session) = session {
            controls = controls
                .child(self.model_pill(session, colors, cx))
                .child(self.effort_pill(session, colors, cx))
                .child(self.permission_pill(session, colors, cx));
        }
        controls = controls.child(
            // The app's round send button: an arrow, or a clock while the
            // message would wait for the running turn.
            div()
                .id("send-message")
                .size(px(30.0))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(15.0))
                .bg(colors.accent)
                .cursor_pointer()
                .hover(move |style| style.opacity(0.88))
                .active(move |style| style.opacity(0.76))
                .child(art::glyph(if busy { Glyph::CLOCK } else { Glyph::ARROW_UP }, 15.0, colors.text_inverse))
                .on_click(cx.listener(|root, _: &gpui::ClickEvent, window, cx| root.send(window, cx))),
        );

        let column = column.child(
                // `.composer-box` (composer.css:9, Haze's input surface): 22px
                // corners, a hairline border and the 96px resting height.
                div()
                    .min_h(px(96.0))
                    .pt(px(10.0))
                    .pr(px(10.0))
                    .pb(px(8.0))
                    .pl(px(14.0))
                    .flex()
                    .flex_col()
                    .justify_between()
                    .gap(px(6.0))
                    .rounded(px(22.0))
                    .bg(colors.input)
                    .border_1()
                    .border_color(colors.line)
                    .child(div().min_h(px(36.0)).text_size(px(theme::FS_MD)).child(self.composer.clone()))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                // The hint gives way first: it shrinks to nothing
                                // before the controls wrap.
                                div()
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .truncate()
                                    .text_size(px(theme::FS_2XS))
                                    .text_color(colors.text_3)
                                    .child(SharedString::from(hint.to_owned())),
                            )
                            .child(controls),
                    ),
            );
        div()
            .flex_none()
            .w_full()
            .px(px(20.0))
            .pt(px(6.0))
            .pb(px(14.0))
            .flex()
            .justify_center()
            .child(column)
            .into_any_element()
    }

    fn model_pill(&mut self, session: &Session, colors: Colors, cx: &mut Context<Self>) -> AnyElement {
        let agent = self.selected_agent().cloned();
        let label = self.model_label(Some(session), agent.as_ref()).map(|(_, label)| label);
        let text = label.unwrap_or_else(|| "Pick a model".to_owned());
        self.pill("model-pill", &text, None, colors, cx, |root, at, cx| root.open_model_menu_at(at, cx))
    }

    fn effort_pill(&mut self, session: &Session, colors: Colors, cx: &mut Context<Self>) -> AnyElement {
        let agent = self.selected_agent().cloned();
        let level = session
            .model_lock
            .as_ref()
            .and_then(|lock| lock.thinking.clone())
            .or_else(|| agent.as_ref().and_then(|agent| agent.thinking.clone()));
        let text = level.map(|level| effort_label(&level)).unwrap_or_else(|| "Effort".to_owned());
        self.pill("effort-pill", &text, None, colors, cx, |root, at, cx| root.open_effort_menu_at(at, cx))
    }

    fn permission_pill(&mut self, session: &Session, colors: Colors, cx: &mut Context<Self>) -> AnyElement {
        let agent = self.selected_agent().cloned();
        let (mode, pinned, _) = session.permission_mode(agent.as_ref());
        let (label, detail) = approval_mode_copy(&mode);
        let text = if pinned { format!("{label} · here") } else { label };
        let glyph = match mode.as_str() {
            "bypass" => Glyph::ALERT,
            "auto" | "accept_edits" => Glyph::SHIELD,
            _ => Glyph::CHECK,
        };
        self.pill("permission-pill", &text, Some((glyph, detail)), colors, cx, |root, at, cx| {
            root.open_permission_menu_at(at, cx)
        })
    }

    /// One small pill in the composer's row: a word, a mark, and the menu it
    /// opens.
    fn pill(
        &mut self,
        id: &str,
        label: &str,
        detail: Option<(Glyph, &'static str)>,
        colors: Colors,
        cx: &mut Context<Self>,
        action: impl Fn(&mut Root, gpui::Point<gpui::Pixels>, &mut Context<Root>) + 'static,
    ) -> AnyElement {
        let title = detail.map(|(_, detail)| detail.to_owned());
        div()
            .id(SharedString::from(format!("pill-{id}")))
            // `.composer-execution-pill`: no fill until hovered, 30px tall.
            .h(px(30.0))
            .px(px(8.0))
            .flex()
            .items_center()
            .gap(px(6.0))
            .rounded(px(15.0))
            .text_color(colors.text_2)
            .text_size(px(theme::FS_XS))
            .cursor_pointer()
            .hover(move |style| style.bg(colors.hover).text_color(colors.text_1))
            .active(move |style| style.bg(colors.selected))
            .when_some(detail, |this, (glyph, _)| this.child(art::glyph(glyph, 11.0, colors.text_3)))
            .child(div().max_w(px(150.0)).truncate().child(SharedString::from(label.to_owned())))
            .child(art::glyph(Glyph::CHEVRON_DOWN, 10.0, colors.text_3))
            .when_some(title, |this, _| this)
            .on_click(cx.listener(move |root, event: &gpui::ClickEvent, _window, cx| {
                let at = event.position();
                action(root, at, cx);
            }))
            .into_any_element()
    }

    fn render_slash_menu(&mut self, rows: Vec<SlashItem>, colors: Colors, cx: &mut Context<Self>) -> AnyElement {
        let active = self.slash.as_ref().map(|slash| slash.index).unwrap_or(0);
        let mut list = div()
            .id("slash-menu")
            .max_h(px(260.0))
            .overflow_y_scroll()
            .p(px(3.0))
            .flex()
            .flex_col()
            .gap(px(1.0))
            .rounded(px(theme::RADIUS_MD))
            .bg(colors.overlay)
            .border_1()
            .border_color(colors.line_strong);
        let mut last_group = None;
        for (index, row) in rows.iter().enumerate() {
            if last_group != Some(row.group) {
                list = list.child(
                    div()
                        .px(px(8.0))
                        .pt(px(4.0))
                        .text_size(px(theme::FS_2XS))
                        .text_color(colors.text_3)
                        .child(SharedString::from(row.group.label())),
                );
                last_group = Some(row.group);
            }
            let selected = index == active;
            let pick = row.clone();
            list = list.child(
                div()
                    .id(SharedString::from(format!("slash-{}-{}", row.name, index)))
                    .h(px(34.0))
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .rounded(px(theme::RADIUS_SM))
                    .when(selected, |this| this.bg(colors.selected))
                    .cursor_pointer()
                    .hover(move |style| style.bg(colors.hover))
                    .child(
                        div()
                            .text_size(px(theme::FS_XS))
                            .font_weight(FontWeight::MEDIUM)
                            .child(SharedString::from(format!("/{}", row.name))),
                    )
                    .child(
                        div()
                            .flex_1()
                            .truncate()
                            .text_size(px(theme::FS_2XS))
                            .text_color(colors.text_3)
                            .child(SharedString::from(row.description.clone())),
                    )
                    .on_click(cx.listener(move |root, _: &gpui::ClickEvent, _window, cx| {
                        root.choose_slash_from_menu(pick.clone(), cx);
                    })),
            );
        }
        list.into_any_element()
    }

    fn render_queue(&mut self, items: Vec<QueuedPrompt>, paused: bool, busy: bool, colors: Colors, cx: &mut Context<Self>) -> AnyElement {
        let rows = queue_order(&items);
        let mut list = div().flex().flex_col().gap(px(2.0));
        let shown = if self.queue_open { rows.len() } else { rows.len().min(1) };
        for item in rows.iter().take(shown) {
            let failed = item.status == "failed";
            let id = item.id.clone();
            let delete_id = id.clone();
            let session_id = item.session_id.clone();
            let delete_session = session_id.clone();
            list = list.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .px(px(8.0))
                    .h(px(24.0))
                    .rounded(px(theme::RADIUS_SM))
                    .bg(colors.raised)
                    .text_size(px(theme::FS_2XS))
                    .text_color(if failed { colors.status_danger } else { colors.text_2 })
                    .child(art::glyph(if failed { Glyph::ALERT } else { Glyph::CLOCK }, 10.0, colors.text_3))
                    .child(div().flex_1().truncate().child(SharedString::from(queue_row_text(item))))
                    .when(!failed, |this| {
                        this.child(
                            div()
                                .id(SharedString::from(format!("queue-now-{id}")))
                                .px(px(4.0))
                                .rounded(px(4.0))
                                .cursor_pointer()
                                .text_size(px(theme::FS_2XS))
                                .hover(move |style| style.bg(colors.hover).text_color(colors.text_1))
                                .child(if busy { "Next" } else { "Send now" })
                                .on_click(cx.listener(move |root, _: &gpui::ClickEvent, _window, cx| {
                                    root.queue_send_now(session_id_of(root, &id), id.clone(), cx);
                                })),
                        )
                    })
                    .child(
                        div()
                            .id(SharedString::from(format!("queue-del-{delete_id}")))
                            .size(px(16.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(4.0))
                            .cursor_pointer()
                            .hover(move |style| style.bg(colors.hover))
                            .child(art::glyph(Glyph::X, 9.0, colors.text_3))
                            .on_click(cx.listener(move |root, _: &gpui::ClickEvent, _window, cx| {
                                root.queue_delete(delete_session.clone(), delete_id.clone(), cx);
                            })),
                    ),
            );
        }
        if rows.len() > 1 {
            let more = rows.len().saturating_sub(shown);
            list = list.child(
                div()
                    .id("queue-more")
                    .ml(px(8.0))
                    .text_size(px(theme::FS_2XS))
                    .text_color(colors.text_3)
                    .cursor_pointer()
                    .hover(move |style| style.text_color(colors.text_1))
                    .child(SharedString::from(if self.queue_open {
                        "Show fewer".to_owned()
                    } else {
                        format!("{more} more queued")
                    }))
                    .on_click(cx.listener(|root, _: &gpui::ClickEvent, _window, cx| {
                        root.queue_open = !root.queue_open;
                        cx.notify();
                    })),
            );
        }
        if paused {
            list = list.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .px(px(8.0))
                    .text_size(px(theme::FS_2XS))
                    .text_color(colors.status_working)
                    .child("Paused until you resume")
                    .child(
                        div()
                            .id("queue-resume")
                            .cursor_pointer()
                            .hover(move |style| style.text_color(colors.text_1))
                            .child("Resume")
                            .on_click(cx.listener(|root, _: &gpui::ClickEvent, _window, cx| root.queue_resume(cx))),
                    ),
            );
        }
        list.into_any_element()
    }

    fn render_attachment_tray(&mut self, colors: Colors, cx: &mut Context<Self>) -> AnyElement {
        let mut row = div().flex().items_center().gap(px(6.0)).flex_wrap();
        for image in self.images.clone() {
            let id = image.id.clone();
            let mut chip = div()
                .h(px(44.0))
                .px(px(6.0))
                .flex()
                .items_center()
                .gap(px(6.0))
                .rounded(px(theme::RADIUS_SM))
                .bg(colors.raised)
                .border_1()
                .border_color(if image.error.is_some() { colors.status_danger } else { colors.line });
            match image.preview.clone() {
                Some(preview) => {
                    chip = chip.child(gpui::img(preview).size(px(32.0)).rounded(px(4.0)));
                }
                None => {
                    chip = chip.child(art::glyph(Glyph::IMAGE, 14.0, colors.text_3));
                }
            }
            chip = chip.child(
                div()
                    .max_w(px(150.0))
                    .truncate()
                    .text_size(px(theme::FS_2XS))
                    .text_color(if image.error.is_some() { colors.status_danger } else { colors.text_2 })
                    .child(SharedString::from(image.name.clone())),
            );
            chip = chip.child(
                div()
                    .id(SharedString::from(format!("image-remove-{id}")))
                    .size(px(16.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(4.0))
                    .cursor_pointer()
                    .hover(move |style| style.bg(colors.hover))
                    .child(art::glyph(Glyph::X, 9.0, colors.text_3))
                    .on_click(cx.listener(move |root, _: &gpui::ClickEvent, _window, cx| {
                        root.images.retain(|image| image.id != id);
                        cx.notify();
                    })),
            );
            row = row.child(chip);
        }
        row.child(
            div()
                .id("image-paste-hint")
                .text_size(px(theme::FS_2XS))
                .text_color(colors.text_3)
                .child("Paste or drop another image"),
        )
        .into_any_element()
    }

    // ---- images -------------------------------------------------------------

    /// Paste an image from the clipboard, on the owner's own gesture only.
    pub(crate) fn paste_image(&mut self, cx: &mut Context<Self>) {
        let item = cx.read_from_clipboard();
        let has_image = item
            .as_ref()
            .map(|item| item.entries().iter().any(|entry| matches!(entry, gpui::ClipboardEntry::Image(_) | gpui::ClipboardEntry::ExternalPaths(_))))
            .unwrap_or(false);
        let this = cx.weak_entity();
        let task = cx.background_executor().spawn(async move { crate::files::clipboard_png(item) });
        cx.spawn(async move |_, cx: &mut gpui::AsyncApp| {
            let result = task.await;
            cx.update(|app| {
                if let Some(root) = this.upgrade() {
                    root.update(app, |root, cx| {
                        root.add_image_bytes(result, "Pasted image", cx);
                        let _ = has_image;
                    });
                }
            });
        })
        .detach();
    }

    /// A file dropped on the window: an image is read and staged, anything else
    /// is refused with the app's own sentence.
    pub(crate) fn drop_files(&mut self, paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        for path in paths {
            let name = path.file_name().map(|name| name.to_string_lossy().to_string()).unwrap_or_else(|| "image".to_owned());
            let this = cx.weak_entity();
            let read = path.clone();
            let task = cx.background_executor().spawn(async move { crate::files::read_image_file(&read) });
            cx.spawn(async move |_, cx: &mut gpui::AsyncApp| {
                let result = task.await;
                cx.update(|app| {
                    if let Some(root) = this.upgrade() {
                        root.update(app, |root, cx| root.add_image_bytes(result, &name, cx));
                    }
                });
            })
            .detach();
        }
    }

    /// Stage one image and show its thumbnail.
    pub(crate) fn add_image_bytes(&mut self, result: Result<Vec<u8>, String>, name: &str, cx: &mut Context<Self>) {
        let id = format!("image-{}", uuid::Uuid::new_v4());
        match result {
            Ok(bytes) => {
                let folder = paths::attachments_dir();
                let staged = crate::files::stage_attachment(&folder, &bytes, name);
                let preview = crate::files::image_format(&bytes).map(|format| Arc::new(Image::from_bytes(format, bytes.clone())));
                match staged {
                    Ok(path) => self.images.push(ComposerImage {
                        id,
                        name: name.to_owned(),
                        path: Some(path.to_string_lossy().to_string()),
                        preview,
                        error: None,
                    }),
                    Err(error) => {
                        self.error = Some(error);
                    }
                }
            }
            Err(error) => {
                // "The clipboard does not contain a readable image" is not an
                // error worth a red bar; a drop of a wrong file is.
                if !error.contains("does not contain a readable image") {
                    self.error = Some(error);
                }
            }
        }
        self.images.truncate(8);
        cx.notify();
    }
}

/// The open conversation's id, for a queue row that only names its item.
fn session_id_of(root: &Root, _item: &str) -> String {
    root.session_id.clone().unwrap_or_default()
}

impl Root {
    pub(crate) fn queue_send_now(&mut self, session_id: String, id: String, cx: &mut Context<Self>) {
        let session_id = if session_id.is_empty() { self.session_id.clone().unwrap_or_default() } else { session_id };
        self.daemon_command(
            Command::Rpc {
                tag: "queue".to_owned(),
                method: "session.queue.send_now".to_owned(),
                params: serde_json::json!({ "sessionId": session_id, "id": id }),
            },
            cx,
        );
    }

    pub(crate) fn queue_delete(&mut self, session_id: String, id: String, cx: &mut Context<Self>) {
        self.daemon_command(
            Command::Rpc {
                tag: "queue".to_owned(),
                method: "session.queue.delete".to_owned(),
                params: serde_json::json!({ "sessionId": session_id, "id": id }),
            },
            cx,
        );
    }

    pub(crate) fn queue_resume(&mut self, cx: &mut Context<Self>) {
        let Some(session_id) = self.session_id.clone() else { return };
        self.daemon_command(
            Command::Rpc {
                tag: "queue".to_owned(),
                method: "session.queue.resume".to_owned(),
                params: serde_json::json!({ "sessionId": session_id }),
            },
            cx,
        );
    }

    /// Read the open conversation's queue back.
    pub(crate) fn load_queue(&mut self, cx: &mut Context<Self>) {
        let Some(session_id) = self.session_id.clone() else { return };
        self.daemon_command(
            Command::Rpc {
                tag: "queue-list".to_owned(),
                method: "session.queue.list".to_owned(),
                params: serde_json::json!({ "sessionId": session_id }),
            },
            cx,
        );
    }
}

/// The catalog answer turned into the window's own rows (`parseModelCatalog`).
pub fn parse_catalog(runtime_id: &str, value: &serde_json::Value) -> Catalog {
    let catalog = value.get("catalog").unwrap_or(value);
    let provider = catalog.get("provider").and_then(|provider| provider.as_str()).unwrap_or("").to_owned();
    let models = catalog
        .get("models")
        .and_then(|models| models.as_array())
        .map(|rows| {
            rows.iter()
                .filter_map(|row| {
                    let id = row.get("id").and_then(|id| id.as_str())?;
                    let label = row
                        .get("displayName")
                        .and_then(|label| label.as_str())
                        .filter(|label| !label.is_empty())
                        .unwrap_or(id)
                        .to_owned();
                    let thinking = row
                        .get("supportedThinking")
                        .and_then(|levels| levels.as_array())
                        .map(|levels| levels.iter().filter_map(|level| level.as_str().map(str::to_owned)).collect::<Vec<_>>())
                        .unwrap_or_default();
                    let default_thinking = row.get("defaultThinking").and_then(|level| level.as_str()).map(str::to_owned);
                    Some(ModelChoice {
                        id: id.to_owned(),
                        label,
                        provider: row
                            .get("providerId")
                            .and_then(|provider| provider.as_str())
                            .unwrap_or(&provider)
                            .to_owned(),
                        thinking,
                        default_thinking,
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let runtime = catalog.get("runtimeId").and_then(|id| id.as_str()).unwrap_or(runtime_id).to_owned();
    let _ = runtime;
    Catalog { provider, models }
}

/// The questions of one card, with each question's chosen options and its own
/// typed answer: the payload `permission.reply` takes as `answers`.
pub fn question_answers(
    questions: &[UserQuestion],
    chosen: &std::collections::BTreeMap<String, Vec<String>>,
    other: &std::collections::BTreeMap<String, String>,
) -> Option<serde_json::Value> {
    let mut answers = serde_json::Map::new();
    for question in questions {
        let mut values = chosen.get(&question.id).cloned().unwrap_or_default();
        if let Some(text) = other.get(&question.id).map(|text| text.trim()).filter(|text| !text.is_empty()) {
            values.push(text.to_owned());
        }
        if values.is_empty() {
            return None;
        }
        answers.insert(question.id.clone(), serde_json::json!(values));
    }
    Some(serde_json::Value::Object(answers))
}


/// `.composer-wrap` (`shell.css:260`): the composer never grows past 800px.
const COMPOSER_WIDTH: f32 = 800.0;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{Permission, QuestionOption};

    fn session_with(user_turns: usize, context_used: Option<f64>) -> Session {
        let mut session = Session { id: "s1".into(), ..Session::default() };
        for index in 0..user_turns {
            session.messages.push(crate::state::Message {
                id: Some(format!("m{index}")),
                role: Some("user".into()),
                content: Some("hello".into()),
                ..crate::state::Message::default()
            });
        }
        session.context_used = context_used;
        session
    }

    #[test]
    fn a_model_change_only_warns_when_the_conversation_has_memory() {
        let quiet = session_with(0, None);
        assert!(!model_change_needs_warning(&quiet, Some("beta"), None), "a first choice is free");
        let talked = session_with(1, None);
        assert!(model_change_needs_warning(&talked, Some("beta"), None), "a user turn is context");
        let active = session_with(0, Some(12.0));
        assert!(model_change_needs_warning(&active, Some("beta"), None), "a used context counts");
        let with_context_model = Session { context_model: Some("alpha".into()), ..Session::default() };
        assert!(model_change_needs_warning(&with_context_model, Some("beta"), None));
        // Picking the model already in force, or none, never warns.
        assert!(!model_change_needs_warning(&talked, Some("alpha"), Some("alpha")));
        assert!(!model_change_needs_warning(&talked, None, Some("alpha")));
    }

    #[test]
    fn the_model_change_sentence_says_what_stays() {
        let body = model_change_body(None);
        assert!(body.starts_with("The new model starts a new provider session"));
        assert!(body.ends_with("Your saved messages, drafts, attachments and project stay as they are."));
        let with_reason = model_change_body(Some("This provider carries the chat over."));
        assert!(with_reason.starts_with("This provider carries the chat over."));
    }

    fn queued(id: &str, kind: &str, position: i64, text: &str, attachments: usize) -> QueuedPrompt {
        QueuedPrompt {
            id: id.into(),
            session_id: "s1".into(),
            text: text.into(),
            kind_name: kind.into(),
            position,
            status: "queued".into(),
            attachments: vec![serde_json::json!({}); attachments],
            ..QueuedPrompt::default()
        }
    }

    #[test]
    fn the_queue_lists_scheduled_and_retry_items_first_then_the_rest_in_order() {
        let items = vec![
            queued("a", "user", 0, "first", 0),
            queued("b", "send_later", 2, "later", 0),
            queued("c", "user", 1, "second", 0),
            queued("d", "retry", 3, "retry", 0),
        ];
        let ordered = queue_order(&items);
        let order: Vec<&str> = ordered.iter().map(|item| item.id.as_str()).collect();
        assert_eq!(order, vec!["b", "d", "a", "c"], "the daemon's own ORDER BY");
    }

    #[test]
    fn a_queued_row_says_what_it_is() {
        assert_eq!(queue_row_text(&queued("a", "retry", 0, "Continue", 0)), "Continue automatically");
        assert_eq!(queue_row_text(&queued("b", "user", 0, "  hello   there ", 0)), "hello there");
        assert_eq!(queue_row_text(&queued("c", "user", 0, "", 2)), "2 images");
        assert_eq!(queue_row_text(&queued("d", "user", 0, "Look", 1)), "Look · 1 image");
        assert_eq!(queue_row_text(&queued("e", "user", 0, "", 0)), "Empty message");
        let long = "word ".repeat(40);
        assert!(queue_row_text(&queued("f", "user", 0, &long, 0)).ends_with('…'));
    }

    #[test]
    fn a_busy_turn_takes_the_prompt_into_the_queue() {
        assert_eq!(delivery_for(false, false), Delivery::Send);
        assert_eq!(delivery_for(true, false), Delivery::Queue);
        assert_eq!(delivery_for(false, true), Delivery::Queue, "an unanswered card holds the turn");
    }

    fn question(id: &str, multi: bool) -> UserQuestion {
        UserQuestion {
            id: id.into(),
            header: "Header".into(),
            question: "Which?".into(),
            options: vec![QuestionOption { label: "Blue".into(), description: None }],
            multi_select: multi,
            allow_other: true,
        }
    }

    #[test]
    fn the_answers_a_question_carries_are_the_chosen_options_and_the_typed_words() {
        let questions = vec![question("q1", false), question("q2", true)];
        let mut chosen = std::collections::BTreeMap::new();
        chosen.insert("q1".to_owned(), vec!["Blue".to_owned()]);
        chosen.insert("q2".to_owned(), vec!["Blue".to_owned()]);
        let mut other = std::collections::BTreeMap::new();
        other.insert("q2".to_owned(), "  and green  ".to_owned());
        let answers = question_answers(&questions, &chosen, &other).expect("both are answered");
        assert_eq!(answers["q1"], serde_json::json!(["Blue"]));
        assert_eq!(answers["q2"], serde_json::json!(["Blue", "and green"]), "the typed words join the options");
    }

    #[test]
    fn a_question_with_nothing_chosen_has_no_answer_to_send() {
        let questions = vec![question("q1", false), question("q2", true)];
        let mut chosen = std::collections::BTreeMap::new();
        chosen.insert("q1".to_owned(), vec!["Blue".to_owned()]);
        assert!(question_answers(&questions, &chosen, &std::collections::BTreeMap::new()).is_none());
        // Whitespace alone is not an answer either.
        let mut other = std::collections::BTreeMap::new();
        other.insert("q2".to_owned(), "   ".to_owned());
        assert!(question_answers(&questions, &chosen, &other).is_none());
    }

    #[test]
    fn a_permission_card_is_read_the_way_the_app_reads_it() {
        let card = Permission { id: "p".into(), ..Permission::default() };
        assert!(!card.is_question() && !card.is_proposal());
        let question = Permission { id: "q".into(), kind: Some("question".into()), ..Permission::default() };
        assert!(question.is_question() && !question.is_proposal());
        let proposal = Permission { id: "r".into(), kind: Some("proposal".into()), ..Permission::default() };
        assert!(proposal.is_proposal() && !proposal.is_question());
    }
}
