//! Finding things and moving between them: the quick switcher (Ctrl+K,
//! `ui/QuickSwitcher.tsx` and `quickSwitcherModel.ts`), the sidebar's own search
//! (`ui/ProjectSearch.tsx` behaviour), the keyboard shortcuts (`ui/Keys.tsx`,
//! `shortcuts.ts`) and the sheet that lists them.

use gpui::{actions, div, prelude::*, px, AnyElement, Context, FontWeight, KeyBinding, SharedString, Window};

// The window's own shortcuts, as actions rather than raw key handling: a bound
// action is dispatched to the focused element's context chain, so it arrives
// whether the caret is in the message box, on a page with no field, or nowhere
// at all (`key_context("Workspace")` on the root view).
actions!(
    workspace,
    [
        QuickSwitcher,
        NewConversation,
        OpenSettings,
        FocusComposer,
        ShortcutSheet,
        NextConversation,
        PreviousConversation
    ]
);

/// The shortcut one row of the sheet stands for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shortcut {
    QuickSwitcher,
    NewConversation,
    OpenSettings,
    FocusComposer,
    Keys,
    NextConversation,
    PreviousConversation,
}

/// Every shortcut, in one table: its key as the sheet writes it, its key as GPUI
/// binds it, and what it does. The bindings and the sheet are built from this,
/// so a key cannot be offered without being bound.
pub const SHORTCUTS: [(Shortcut, &str, &str, &str); 7] = [
    (Shortcut::QuickSwitcher, "Ctrl+K", "ctrl-k", "Open the quick switcher"),
    (Shortcut::NewConversation, "Ctrl+N", "ctrl-n", "New conversation"),
    (Shortcut::OpenSettings, "Ctrl+,", "ctrl-,", "Open Settings"),
    (Shortcut::Keys, "Ctrl+/", "ctrl-/", "Show keyboard shortcuts"),
    (Shortcut::FocusComposer, "Ctrl+L", "ctrl-l", "Put the caret in the message box"),
    (Shortcut::NextConversation, "Alt+↓", "alt-down", "Next conversation"),
    (Shortcut::PreviousConversation, "Alt+↑", "alt-up", "Previous conversation"),
];

/// The keys of section 5, bound to the root view's own context. `KeyBinding::new`
/// parses the keystroke and panics on a bad one, so this is also the check that
/// every string above is real.
pub fn key_bindings() -> Vec<KeyBinding> {
    SHORTCUTS
        .iter()
        .map(|(action, _, keys, _)| match action {
            Shortcut::QuickSwitcher => KeyBinding::new(keys, QuickSwitcher, Some("Workspace")),
            Shortcut::NewConversation => KeyBinding::new(keys, NewConversation, Some("Workspace")),
            Shortcut::OpenSettings => KeyBinding::new(keys, OpenSettings, Some("Workspace")),
            Shortcut::FocusComposer => KeyBinding::new(keys, FocusComposer, Some("Workspace")),
            Shortcut::Keys => KeyBinding::new(keys, ShortcutSheet, Some("Workspace")),
            Shortcut::NextConversation => KeyBinding::new(keys, NextConversation, Some("Workspace")),
            Shortcut::PreviousConversation => KeyBinding::new(keys, PreviousConversation, Some("Workspace")),
        })
        .collect()
}

use crate::art::{self, Glyph};
use crate::state::{Session, Snapshot};
use crate::theme::{self, Colors};
use crate::state::projects::project_of_session;
use crate::ui::root::{Page, QuickState, Root};

/// The order the palette groups its results in.
pub const CATEGORIES: [&str; 5] = ["Conversations", "Bots", "Projects", "Create", "Go to"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuickKind {
    Conversation,
    Bot,
    Project,
    Action,
}

impl QuickKind {
    pub fn category(self) -> &'static str {
        match self {
            QuickKind::Conversation => "Conversations",
            QuickKind::Bot => "Bots",
            QuickKind::Project => "Projects",
            QuickKind::Action => "Go to",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct QuickItem {
    pub id: String,
    pub kind: QuickKind,
    pub title: String,
    pub subtitle: String,
    /// Everything the query is matched against, the title and its words.
    pub search_text: String,
    /// A bot's colour, for its dot.
    pub color: Option<String>,
    /// Another row the same action needs (a conversation's bot).
    pub agent_id: Option<String>,
}

/// How well one row answers a query: 0 when the words start it, 1 when each word
/// starts a word of it, 2 when they are only found inside it, and `None` when one
/// of them is missing (`rank` in `quickSwitcherModel.ts`).
pub fn rank_of(item: &QuickItem, words: &[String]) -> Option<u8> {
    if words.is_empty() {
        return Some(0);
    }
    let haystack = item.search_text.to_lowercase();
    if !words.iter().all(|word| haystack.contains(word.as_str())) {
        return None;
    }
    if haystack.starts_with(&words.join(" ")) {
        return Some(0);
    }
    let parts: Vec<&str> = haystack.split_whitespace().collect();
    let every_word_starts_one = words.iter().all(|word| parts.iter().any(|part| part.starts_with(word.as_str())));
    Some(if every_word_starts_one { 1 } else { 2 })
}

/// The rows a query keeps, best first. Conversations with the same rank stay
/// newest first; everything else keeps the order it was given in.
pub fn rank_items(items: &[QuickItem], query: &str) -> Vec<QuickItem> {
    let words: Vec<String> = query.trim().to_lowercase().split_whitespace().map(str::to_owned).collect();
    let first = words.first().cloned().unwrap_or_default();
    let title_starts = |item: &QuickItem| {
        if first.is_empty() || item.title.to_lowercase().starts_with(&first) {
            0u8
        } else {
            1u8
        }
    };
    // A shorter title that answers the words exactly is the better answer.
    let mut ranked: Vec<(usize, u8, u8, usize, &QuickItem)> = items
        .iter()
        .enumerate()
        .filter_map(|(index, item)| rank_of(item, &words).map(|rank| (index, rank, title_starts(item), item.title.chars().count(), item)))
        .collect();
    ranked.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.2.cmp(&b.2)).then_with(|| a.3.cmp(&b.3)).then_with(|| a.0.cmp(&b.0)));
    ranked.into_iter().map(|(_, _, _, _, item)| item.clone()).collect()
}

/// Ranked rows kept in group order, so each heading appears once
/// (`groupQuickSwitcherResults`).
pub fn grouped(ranked: Vec<QuickItem>) -> Vec<QuickItem> {
    let order = |item: &QuickItem| CATEGORIES.iter().position(|category| *category == item.kind.category()).unwrap_or(9);
    let mut rows: Vec<(usize, QuickItem)> = ranked.into_iter().enumerate().collect();
    rows.sort_by(|a, b| order(&a.1).cmp(&order(&b.1)).then_with(|| a.0.cmp(&b.0)));
    rows.into_iter().map(|(_, item)| item).collect()
}

/// The rows the palette shows for a query: everything while words are typed, and
/// recent conversations plus the actions when the box is empty.
pub fn results(items: &[QuickItem], query: &str) -> Vec<QuickItem> {
    let searching = !query.trim().is_empty();
    let ranked = rank_items(items, query);
    if searching {
        return grouped(ranked);
    }
    let conversations: Vec<QuickItem> = ranked.iter().filter(|item| item.kind == QuickKind::Conversation).take(6).cloned().collect();
    let actions: Vec<QuickItem> = ranked.iter().filter(|item| item.kind == QuickKind::Action).cloned().collect();
    grouped(conversations.into_iter().chain(actions).collect())
}

/// Every row the palette can offer, from the window's own state.
pub fn items_for(snapshot: Option<&Snapshot>) -> Vec<QuickItem> {
    let Some(snapshot) = snapshot else { return actions() };
    let mut items = Vec::new();
    for agent in snapshot.agents.iter().filter(|agent| !agent.archived && !agent.hidden && agent.parent_agent_id.is_none()) {
        items.push(QuickItem {
            id: agent.id.clone(),
            kind: QuickKind::Bot,
            title: agent.name.clone(),
            subtitle: agent.role_or_default().to_owned(),
            search_text: format!("{} {}", agent.name, agent.role_or_default()),
            color: Some(agent.color.clone()),
            agent_id: Some(agent.id.clone()),
        });
    }
    for session in snapshot.sessions.iter().filter(|session| !session.archived && !session.is_side()) {
        let owner = session.agent_id.as_deref().and_then(|id| snapshot.agent(id));
        let place = project_of_session(session, &snapshot.projects).map(|project| project.name.clone());
        let mut subtitle = Vec::new();
        if let Some(owner) = owner {
            subtitle.push(owner.name.clone());
        }
        if let Some(place) = place {
            subtitle.push(place);
        }
        items.push(QuickItem {
            id: session.id.clone(),
            kind: QuickKind::Conversation,
            title: session.title_or_default().to_owned(),
            subtitle: subtitle.join(" · "),
            search_text: format!("{} {}", session.title_or_default(), subtitle.join(" ")),
            color: owner.map(|owner| owner.color.clone()).filter(|color| !color.is_empty()),
            agent_id: session.agent_id.clone(),
        });
    }
    for project in &snapshot.projects {
        items.push(QuickItem {
            id: project.id.clone(),
            kind: QuickKind::Project,
            title: project.name.clone(),
            subtitle: project.path.clone().unwrap_or_default(),
            search_text: format!("{} {}", project.name, project.path.clone().unwrap_or_default()),
            color: None,
            agent_id: None,
        });
    }
    items.extend(actions());
    items
}

/// The things the palette can do, which need no daemon.
pub fn actions() -> Vec<QuickItem> {
    let action = |id: &str, title: &str, subtitle: &str| QuickItem {
        id: id.to_owned(),
        kind: QuickKind::Action,
        title: title.to_owned(),
        subtitle: subtitle.to_owned(),
        search_text: format!("{title} {subtitle}"),
        color: None,
        agent_id: None,
    };
    vec![
        action("new-conversation", "New conversation", "Start a new conversation with this bot"),
        action("new-bot", "New bot", "Create your own employee"),
        action("settings", "Open Settings", "Appearance, data and about"),
        action("usage", "Usage", "Plan limits for every coding agent"),
        action("needs", "What needs you", "Approvals, questions and proposals waiting"),
        action("shortcuts", "Keyboard shortcuts", "Every key the window understands"),
    ]
}

/// The words the sidebar's search filters on: a project's name and path, an
/// employee's name and role, and a conversation's title.
pub fn matches(words: &[String], haystacks: &[&str]) -> bool {
    if words.is_empty() {
        return true;
    }
    let joined = haystacks.join(" ").to_lowercase();
    words.iter().all(|word| joined.contains(word.as_str()))
}

/// Split a search box into its lowercased words.
pub fn search_words(query: &str) -> Vec<String> {
    query.trim().to_lowercase().split_whitespace().map(str::to_owned).collect()
}

impl Root {
    /// Open or close the quick switcher, and put the caret in its box.
    pub(crate) fn toggle_quick(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.quick.is_some() {
            self.quick = None;
            self.focus_composer(window, cx);
        } else {
            // The draft in the box is untouched: the switcher is a detour, not a
            // new message.
            self.quick = Some(QuickState::default());
            self.quick_search.update(cx, |input, cx| input.set_text(String::new(), cx));
            let handle = self.quick_search.read(cx).handle();
            window.focus(&handle, cx);
        }
        cx.notify();
    }

    pub(crate) fn quick_results(&self, cx: &App) -> Vec<QuickItem> {
        let snapshot = self.snapshot();
        let query = self.quick_search.read(cx).text().to_owned();
        results(&items_for(snapshot), &query)
    }

    /// Run the highlighted row.
    pub(crate) fn choose_quick(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let rows = self.quick_results(cx);
        let Some(item) = rows.get(index).cloned() else { return };
        self.quick = None;
        match item.kind {
            QuickKind::Conversation => {
                if let Some(agent_id) = item.agent_id.clone() {
                    self.open_conversation(agent_id, item.id, cx);
                }
            }
            QuickKind::Bot => {
                self.agent_id = Some(item.id.clone());
                self.session_id = None;
                self.page = Page::Conversation;
                self.sync_list(cx);
            }
            QuickKind::Project => {
                self.open_team(crate::ui::root::TeamTab::Team, Some(item.id), cx);
            }
            QuickKind::Action => match item.id.as_str() {
                "new-conversation" => {
                    if let Some(agent_id) = self.agent_id.clone() {
                        self.new_conversation(agent_id, None, cx);
                    }
                }
                "new-bot" => self.open_bot_wizard(cx),
                "settings" => self.open_settings(cx),
                "usage" => {
                    self.set_usage_bar(true, cx);
                    self.usage_open = true;
                    self.load_quotas(cx);
                }
                "needs" => self.needs_open = true,
                "shortcuts" => self.shortcuts_open = true,
                _ => {}
            },
        }
        self.focus_composer(window, cx);
        cx.notify();
    }

    /// The quick switcher's own keys: arrows, Enter and Escape.
    pub(crate) fn quick_key(&mut self, event: &gpui::KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let rows = self.quick_results(cx).len().max(1);
        match event.keystroke.key.as_str() {
            "down" => {
                if let Some(quick) = self.quick.as_mut() {
                    quick.index = (quick.index + 1) % rows;
                }
                cx.notify();
                true
            }
            "up" => {
                if let Some(quick) = self.quick.as_mut() {
                    quick.index = (quick.index + rows - 1) % rows;
                }
                cx.notify();
                true
            }
            "enter" => {
                let index = self.quick.as_ref().map(|quick| quick.index).unwrap_or(0);
                self.choose_quick(index, window, cx);
                true
            }
            "escape" => {
                self.quick = None;
                self.focus_composer(window, cx);
                cx.notify();
                true
            }
            _ => false,
        }
    }

    /// The switcher: a search box and its grouped rows, over the window.
    pub(crate) fn render_quick_switcher(&mut self, colors: Colors, cx: &mut Context<Self>) -> Option<AnyElement> {
        self.quick.as_ref()?;
        let rows = self.quick_results(cx);
        let active = self.quick.as_ref().map(|quick| quick.index).unwrap_or(0).min(rows.len().saturating_sub(1));
        let mut list = div()
            .id("quick-results")
            .max_h(px(400.0))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(px(1.0));
        let mut last_category = "";
        for (index, item) in rows.iter().enumerate() {
            if item.kind.category() != last_category {
                last_category = item.kind.category();
                list = list.child(
                    div()
                        .px(px(8.0))
                        .pt(px(6.0))
                        .text_size(px(theme::FS_2XS))
                        .text_color(colors.text_3)
                        .child(SharedString::from(last_category)),
                );
            }
            let selected = index == active;
            let pick = index;
            let mut row = div()
                .id(SharedString::from(format!("quick-{}-{index}", item.id)))
                .h(px(38.0))
                .px(px(8.0))
                .flex()
                .items_center()
                .gap(px(8.0))
                .rounded(px(theme::RADIUS_SM))
                .when(selected, |this| this.bg(colors.selected))
                .cursor_pointer()
                .hover(move |style| style.bg(colors.hover));
            row = match item.kind {
                QuickKind::Bot => {
                    let color = item.color.clone().clone().unwrap_or_else(|| "violet".to_owned());
                    row.child(div().size(px(10.0)).rounded_full().bg(art::agent_color(&color)))
                }
                QuickKind::Conversation => row.child(art::glyph(Glyph::MESSAGES, 13.0, colors.text_3)),
                QuickKind::Project => row.child(art::glyph(Glyph::FOLDER, 13.0, colors.text_3)),
                QuickKind::Action => row.child(art::glyph(Glyph::CHEVRON, 13.0, colors.text_3)),
            };
            row = row
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .flex()
                        .flex_col()
                        .child(div().truncate().text_size(px(theme::FS_SM)).child(SharedString::from(item.title.clone())))
                        .when(!item.subtitle.is_empty(), |this| {
                            this.child(
                                div()
                                    .truncate()
                                    .text_size(px(theme::FS_2XS))
                                    .text_color(colors.text_3)
                                    .child(SharedString::from(item.subtitle.clone())),
                            )
                        }),
                )
                .when(selected, |this| this.child(art::glyph(Glyph::ARROW_RIGHT, 12.0, colors.text_3)))
                .on_click(cx.listener(move |root, _: &gpui::ClickEvent, window, cx| root.choose_quick(pick, window, cx)));
            list = list.child(row);
        }
        if rows.is_empty() {
            list = list.child(div().px(px(8.0)).py(px(10.0)).text_size(px(theme::FS_XS)).text_color(colors.text_3).child("No matching results."));
        }

        let card = div()
            .w(px(560.0))
            .max_h(px(520.0))
            .overflow_hidden()
            .p(px(10.0))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .rounded(px(theme::RADIUS_LG))
            .bg(colors.overlay)
            .border_1()
            .border_color(colors.line_strong)
            .child(
                div()
                    .h(px(38.0))
                    .px(px(10.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .rounded(px(theme::RADIUS_MD))
                    .bg(colors.input)
                    .border_1()
                    .border_color(colors.line)
                    .child(art::glyph(Glyph::SEARCH, 14.0, colors.text_3))
                    .child(div().flex_1().min_w(px(0.0)).child(self.quick_search.clone())),
            )
            .child(list)
            .child(
                div()
                    .px(px(6.0))
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .text_size(px(theme::FS_2XS))
                    .text_color(colors.text_3)
                    .child("↑↓ Navigate")
                    .child("↵ Open")
                    .child("Esc Close"),
            );

        Some(
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .flex()
                .justify_center()
                .pt(px(110.0))
                .bg(colors.scrim)
                .on_mouse_down(gpui::MouseButton::Left, cx.listener(|root, _: &gpui::MouseDownEvent, window, cx| {
                    root.quick = None;
                    root.focus_composer(window, cx);
                    cx.notify();
                }))
                .child(
                    // A click on the card must not reach the backdrop.
                    div().on_mouse_down(gpui::MouseButton::Left, cx.listener(|_, _: &gpui::MouseDownEvent, _window, cx| {
                            // A click inside the card is not a click on the backdrop.
                            cx.stop_propagation();
                        })).child(card),
                )
                .into_any_element(),
        )
    }

    /// The shortcut sheet: the keys this window understands.
    pub(crate) fn render_shortcut_sheet(&mut self, colors: Colors, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.shortcuts_open {
            return None;
        }
        let mut rows: Vec<(&str, &str)> = SHORTCUTS.iter().map(|(_, keys, _, what)| (*keys, *what)).collect();
        rows.push(("Esc", "Close what is open"));
        let mut list = div().flex().flex_col().gap(px(4.0));
        for (keys, what) in rows {
            list = list.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .child(
                        div()
                            .w(px(90.0))
                            .flex_none()
                            .text_size(px(theme::FS_XS))
                            .text_color(colors.text_1)
                            .child(SharedString::from(keys)),
                    )
                    .child(div().flex_1().text_size(px(theme::FS_XS)).text_color(colors.text_2).child(SharedString::from(what))),
            );
        }
        let card = div()
            .w(px(420.0))
            .p(px(16.0))
            .flex()
            .flex_col()
            .gap(px(10.0))
            .rounded(px(theme::RADIUS_LG))
            .bg(colors.overlay)
            .border_1()
            .border_color(colors.line_strong)
            .child(div().text_size(px(theme::FS_MD)).font_weight(FontWeight::SEMIBOLD).child("Keyboard shortcuts"))
            .child(list)
            .child(
                div()
                    .flex()
                    .justify_end()
                    .child(self.ghost_button("Close", "shortcuts-close", colors, cx, |root, cx| {
                        root.shortcuts_open = false;
                        cx.notify();
                    })),
            );
        Some(
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .bg(colors.scrim)
                .on_mouse_down(gpui::MouseButton::Left, cx.listener(|root, _: &gpui::MouseDownEvent, _window, cx| {
                    root.shortcuts_open = false;
                    cx.notify();
                }))
                .child(div().on_mouse_down(gpui::MouseButton::Left, cx.listener(|_, _: &gpui::MouseDownEvent, _window, cx| {
                            // A click inside the card is not a click on the backdrop.
                            cx.stop_propagation();
                        })).child(card))
                .into_any_element(),
        )
    }

    /// The next or previous conversation of this bot.
    pub(crate) fn step_conversation(&mut self, forward: bool, cx: &mut Context<Self>) {
        let Some(agent_id) = self.agent_id.clone() else { return };
        let Some(snapshot) = self.snapshot() else { return };
        let rows: Vec<Session> = snapshot.conversations_for(&agent_id);
        if rows.len() < 2 {
            return;
        }
        let current = self
            .session_id
            .as_deref()
            .and_then(|id| rows.iter().position(|session| session.id == id))
            .unwrap_or(0);
        let next = if forward {
            (current + 1) % rows.len()
        } else {
            (current + rows.len() - 1) % rows.len()
        };
        let session = rows[next].clone();
        self.open_conversation(agent_id, session.id, cx);
    }
}

use gpui::App;

#[cfg(test)]
mod tests {
    use super::*;

    fn item(kind: QuickKind, title: &str, subtitle: &str) -> QuickItem {
        QuickItem {
            id: title.to_lowercase(),
            kind,
            title: title.to_owned(),
            subtitle: subtitle.to_owned(),
            search_text: format!("{title} {subtitle}").to_lowercase(),
            color: None,
            agent_id: None,
        }
    }

    fn sample() -> Vec<QuickItem> {
        vec![
            item(QuickKind::Conversation, "Fix the tests", "Ada Shop"),
            item(QuickKind::Conversation, "Design the parser", "Wilson Site"),
            item(QuickKind::Bot, "Ada", "Backend"),
            item(QuickKind::Bot, "Wilson", "Frontend"),
            item(QuickKind::Project, "Shop", "C:/shop"),
            item(QuickKind::Action, "Open Settings", "Appearance"),
        ]
    }

    #[test]
    fn a_query_keeps_the_rows_that_hold_every_word() {
        let rows = rank_items(&sample(), "ada");
        assert_eq!(rows.len(), 2, "{rows:?}");
        assert_eq!(rows[0].title, "Ada", "a name that starts with the word comes first");
        let none = rank_items(&sample(), "gardening");
        assert!(none.is_empty());
    }

    #[test]
    fn rank_prefers_the_start_then_word_starts_then_the_middle() {
        let items = vec![
            item(QuickKind::Bot, "Middleware Ada", "x"),
            item(QuickKind::Bot, "Ada Lovelace", "y"),
            item(QuickKind::Bot, "Ada", "z"),
        ];
        let rows = rank_items(&items, "ada");
        assert_eq!(rows[0].title, "Ada");
        assert_eq!(rows[1].title, "Ada Lovelace");
        assert_eq!(rows[2].title, "Middleware Ada");
    }

    #[test]
    fn an_empty_box_shows_recent_conversations_then_the_actions() {
        let rows = results(&sample(), "");
        assert!(rows.iter().all(|item| item.kind == QuickKind::Conversation || item.kind == QuickKind::Action));
        assert_eq!(rows[0].kind, QuickKind::Conversation);
        assert!(!rows.iter().any(|item| item.kind == QuickKind::Bot), "no bots until something is typed");
    }

    #[test]
    fn the_rows_are_grouped_in_the_palettes_order() {
        let rows = results(&sample(), "a");
        let order: Vec<&str> = rows.iter().map(|item| item.kind.category()).collect();
        let mut sorted = order.clone();
        sorted.sort_by_key(|category| CATEGORIES.iter().position(|known| known == category).unwrap_or(9));
        assert_eq!(order, sorted, "{order:?}");
    }

    #[test]
    fn the_window_binds_every_shortcut_it_offers() {
        // Building a binding parses its keystroke and panics on a bad one, so a
        // mistyped `ctrl-,` fails here rather than binding nothing while the
        // sheet still offers it.
        assert_eq!(key_bindings().len(), SHORTCUTS.len());
        let labels: Vec<&str> = SHORTCUTS.iter().map(|(_, label, _, _)| *label).collect();
        assert_eq!(labels, vec!["Ctrl+K", "Ctrl+N", "Ctrl+,", "Ctrl+/", "Ctrl+L", "Alt+\u{2193}", "Alt+\u{2191}"]);
        for (_, label, keys, what) in SHORTCUTS {
            assert!(!what.is_empty() && !label.is_empty() && keys.contains('-'), "{label} / {keys}");
        }
    }

    #[test]
    fn a_sidebar_search_matches_every_word_anywhere() {
        let words = search_words("ada shop");
        assert!(matches(&words, &["Ada", "Backend", "Shop"]));
        assert!(!matches(&words, &["Ada", "Backend"]), "both words must be found");
        assert!(matches(&[], &[]), "an empty box keeps everything");
    }
}
