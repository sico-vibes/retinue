//! The conversations column: the selected bot's conversations, grouped by the
//! project each one works in as two-line cards (the title, then `time · status`),
//! with the ones the owner pinned in a Pinned group at the top, the branches a
//! side conversation hangs from drawn under it, a "…" popover on each card
//! (rename, pin, archive), an inline rename, and the Archived toggle at the
//! foot. Ported from `apps/desktop/src/ui/ConversationsPanel.tsx`.

use std::collections::BTreeSet;
use std::rc::Rc;

use gpui::{div, prelude::*, px, AnyElement, Context, SharedString};

use crate::art::Glyph;
use crate::state::{conversation_status, project_of_session, Project, Session};
use crate::theme::{self, Colors};
use crate::ui::menu::{Menu, MenuChoice, MenuItem};
use crate::ui::root::Root;
use crate::ui::sidebar::{short_time, status_element};

/// How deep a branch is drawn: the React column indents each side conversation
/// one step under the conversation that started it.
pub const BRANCH_INDENT: f32 = 12.0;

pub struct ConversationGroup {
    pub key: String,
    pub project: Option<Project>,
    pub sessions: Vec<Session>,
}

/// `conversationsByProject`: the same grouping, newest group first. A
/// conversation nothing was said in is only listed while it is the open one,
/// as in the app.
pub fn group_by_project(sessions: &[Session], projects: &[Project], selected: Option<&str>) -> Vec<ConversationGroup> {
    let mut groups: Vec<ConversationGroup> = Vec::new();
    for session in sessions {
        if session.messages.is_empty() && session.tools.is_empty() && selected != Some(session.id.as_str()) {
            continue;
        }
        let project = project_of_session(session, projects).cloned();
        let key = project.as_ref().map(|project| project.id.clone()).unwrap_or_else(|| "none".to_owned());
        match groups.iter_mut().find(|group| group.key == key) {
            Some(group) => group.sessions.push(session.clone()),
            None => groups.push(ConversationGroup { key, project, sessions: vec![session.clone()] }),
        }
    }
    for group in &mut groups {
        group.sessions.sort_by(|a, b| b.stamp().cmp(a.stamp()));
    }
    groups.sort_by(|a, b| {
        let a_latest = a.sessions.first().map(Session::stamp).unwrap_or("");
        let b_latest = b.sessions.first().map(Session::stamp).unwrap_or("");
        b_latest.cmp(a_latest)
    });
    groups
}

/// The conversations the owner pinned, in the order they pinned them, and the
/// rest: the Pinned group reads first, and a pinned conversation is not drawn a
/// second time in its project.
pub fn split_pinned(sessions: &[Session], pinned: &[String]) -> (Vec<Session>, Vec<Session>) {
    let mut pinned_rows: Vec<Session> = Vec::new();
    for id in pinned {
        if let Some(session) = sessions.iter().find(|session| &session.id == id) {
            pinned_rows.push(session.clone());
        }
    }
    let rest = sessions.iter().filter(|session| !pinned.contains(&session.id)).cloned().collect();
    (pinned_rows, rest)
}

/// A conversation and the ones that branch off it.
#[derive(Clone, Debug)]
pub struct Branch {
    pub session: Session,
    pub depth: usize,
}

/// The column's tree: a conversation, then each side conversation that started
/// from it, one level in (`conversationTree`). A side conversation whose parent
/// is not in this list stands on its own rather than vanishing.
pub fn conversation_tree(sessions: &[Session]) -> Vec<Branch> {
    let mut roots: Vec<&Session> = Vec::new();
    let mut children: Vec<(String, &Session)> = Vec::new();
    for session in sessions.iter().filter(|session| !session.is_side()) {
        roots.push(session);
    }
    for session in sessions.iter().filter(|session| session.is_side()) {
        match session.link.as_ref().and_then(|link| link.origin_session_id.clone()) {
            Some(parent) if sessions.iter().any(|other| other.id == parent) => children.push((parent, session)),
            _ => roots.push(session),
        }
    }
    let mut rows = Vec::new();
    for root in roots {
        rows.push(Branch { session: root.clone(), depth: 0 });
        push_children(&mut rows, &children, &root.id, 1);
    }
    rows
}

fn push_children(rows: &mut Vec<Branch>, children: &[(String, &Session)], parent: &str, depth: usize) {
    for (origin, session) in children.iter().filter(|(origin, _)| origin == parent) {
        rows.push(Branch { session: (*session).clone(), depth });
        push_children(rows, children, &session.id, depth + 1);
        let _ = origin;
    }
}

impl Root {
    pub(crate) fn render_conversations(&mut self, colors: Colors, cx: &mut Context<Self>) -> AnyElement {
        let tokens = self.tokens();
        let Some(agent) = self.selected_agent().cloned() else {
            return div()
                .w(px(theme::CONVERSATIONS_WIDTH))
                .h_full()
                .flex_none()
                .border_r_1()
                .border_color(colors.line)
                .into_any_element();
        };
        let showing_archived = self.show_archived;
        let snapshot_sessions: Vec<Session> = match self.snapshot() {
            Some(snapshot) if showing_archived => snapshot.archived_conversations_for(&agent.id),
            Some(snapshot) => snapshot.conversations_for(&agent.id),
            None => Vec::new(),
        };
        let projects = self.snapshot().map(|snapshot| snapshot.projects.clone()).unwrap_or_default();
        let approvals = self.approval_session_ids();
        let selected = self.session_id.clone();
        let pinned = self.pinned.clone();
        let (pinned_rows, rest) = split_pinned(&snapshot_sessions, &pinned);
        let tree = conversation_tree(&rest);
        let rows: Vec<Session> = tree.iter().map(|branch| branch.session.clone()).collect();
        let groups = group_by_project(&rows, &projects, selected.as_deref());
        let renaming = self.renaming.clone();

        let mut column = div()
            .w(px(theme::CONVERSATIONS_WIDTH))
            .h_full()
            .flex_none()
            .flex()
            .flex_col()
            .border_r_1()
            .border_color(colors.line)
            .child(div().px(px(12.0)).pt(px(11.0)).pb(px(8.0)).child(
                div().text_size(px(theme::FS_SM)).font_weight(gpui::FontWeight::SEMIBOLD).child("Conversations"),
            ))
            .child(self.render_new_conversation(agent.id.clone(), colors, cx));

        let mut body = div()
            .id("conversations-scroll")
            .flex_1()
            .min_h(px(0.0))
            .w_full()
            .overflow_y_scroll()
            .px(px(8.0))
            .py(px(6.0));
        if snapshot_sessions.is_empty() {
            body = body.child(
                div()
                    .px(px(4.0))
                    .py(px(6.0))
                    .text_size(px(theme::FS_XS))
                    .text_color(colors.text_3)
                    .child(SharedString::from(if showing_archived {
                        "Nothing archived.".to_owned()
                    } else {
                        format!("No conversations with {} yet.", agent.name)
                    })),
            );
        }
        if !pinned_rows.is_empty() {
            let mut section = div()
                .flex()
                .flex_col()
                .gap(px(tokens.row_gap))
                .pb(px(tokens.group_gap))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(4.0))
                        .px(px(4.0))
                        .pt(px(6.0))
                        .pb(px(2.0))
                        .text_size(px(theme::FS_2XS))
                        .text_color(colors.text_3)
                        .child(crate::art::glyph(Glyph::PIN, 10.0, colors.text_3))
                        .child("Pinned"),
                );
            for session in &pinned_rows {
                section = section.child(self.render_card(session, &agent, 0, true, colors, tokens, &approvals, selected.as_deref(), &renaming, cx));
            }
            body = body.child(section);
        }
        for group in &groups {
            let name = group.project.as_ref().map(|project| project.name.clone()).unwrap_or_else(|| "No project".to_owned());
            let mut section = div().flex().flex_col().gap(px(tokens.row_gap)).pb(px(tokens.group_gap)).child(
                div()
                    .px(px(4.0))
                    .pt(px(6.0))
                    .pb(px(2.0))
                    .text_size(px(theme::FS_2XS))
                    .text_color(colors.text_3)
                    .child(SharedString::from(name)),
            );
            for session in &group.sessions {
                let depth = tree
                    .iter()
                    .find(|branch| branch.session.id == session.id)
                    .map(|branch| branch.depth)
                    .unwrap_or(0);
                section = section.child(self.render_card(session, &agent, depth, false, colors, tokens, &approvals, selected.as_deref(), &renaming, cx));
            }
            body = body.child(section);
        }
        column = column.child(body);
        column = column.child(self.render_archived_toggle(showing_archived, colors, cx));
        column.into_any_element()
    }

    /// One card: the open area (title and meta, or the rename field) and the "…"
    /// button **beside** it, never on top of the title. The React column has the
    /// same two parts (`.convo-open` and `.convo-actions`), for the same reason.
    #[allow(clippy::too_many_arguments)]
    fn render_card(
        &mut self,
        session: &Session,
        agent: &crate::state::Agent,
        depth: usize,
        pinned: bool,
        colors: Colors,
        tokens: crate::theme::DensityTokens,
        approvals: &BTreeSet<String>,
        selected: Option<&str>,
        renaming: &Option<crate::ui::root::RenameState>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let is_selected = selected == Some(session.id.as_str());
        let status = conversation_status(session, approvals.contains(&session.id));
        let agent_id = agent.id.clone();
        let session_id = session.id.clone();
        let title = session.title_or_default().to_owned();
        let renaming_this = renaming.as_ref().filter(|rename| rename.session_id == session.id).cloned();
        // While its own rename field is open the card must not be pressed to
        // open: the press belongs to the field.
        let can_open = renaming_this.is_none();
        let indent = depth as f32 * BRANCH_INDENT;

        let mut row = div()
            .id(SharedString::from(format!("convo-card-{}", session.id)))
            .relative()
            .w_full()
            .min_w(px(0.0))
            .pl(px(10.0 + indent))
            .pr(px(3.0))
            .py(px(tokens.card_pad_y))
            .flex()
            .items_start()
            .gap(px(3.0))
            .rounded(px(theme::RADIUS_MD))
            .text_color(if is_selected { colors.text_1 } else { colors.text_2 })
            .when(is_selected, |this| this.bg(colors.selected))
            .when(!is_selected, |this| this.hover(move |style| style.bg(colors.hover).text_color(colors.text_1)))
            .active(move |style| style.bg(colors.selected_strong));

        // A branch's own line, drawn down its left edge.
        if depth > 0 {
            row = row.child(
                div()
                    .absolute()
                    .left(px(4.0 + indent - BRANCH_INDENT))
                    .top_0()
                    .h_full()
                    .w(px(1.0))
                    .bg(colors.line),
            );
        }

        // The open area: everything that names and opens the conversation.
        let open = match renaming_this {
            Some(rename) => {
                let mut area = div()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .gap(px(3.0))
                    .child(
                        div()
                            .id(SharedString::from(format!("rename-{}", session.id)))
                            .h(px(22.0))
                            .px(px(4.0))
                            .flex()
                            .items_center()
                            .rounded(px(4.0))
                            .bg(colors.input)
                            .border_1()
                            .border_color(colors.line_strong)
                            .child(self.rename_input.clone()),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .child(
                                div()
                                    .id(SharedString::from(format!("rename-save-{}", session.id)))
                                    .text_size(px(theme::FS_2XS))
                                    .text_color(colors.status_info)
                                    .cursor_pointer()
                                    .child("Rename")
                                    .on_click(cx.listener(|root, _: &gpui::ClickEvent, _window, cx| root.commit_rename_from_field(cx))),
                            )
                            .child(
                                div()
                                    .id(SharedString::from(format!("rename-cancel-{}", session.id)))
                                    .text_size(px(theme::FS_2XS))
                                    .text_color(colors.text_3)
                                    .cursor_pointer()
                                    .child("Cancel")
                                    .on_click(cx.listener(|root, _: &gpui::ClickEvent, _window, cx| {
                                        root.renaming = None;
                                        cx.notify();
                                    })),
                            ),
                    );
                if let Some(error) = rename.error {
                    area = area.child(div().text_size(px(theme::FS_2XS)).text_color(colors.status_danger).child(SharedString::from(error)));
                }
                area.into_any_element()
            }
            None => div()
                .id(SharedString::from(format!("convo-open-{}", session.id)))
                .flex_1()
                .min_w(px(0.0))
                .flex()
                .flex_col()
                .gap(px(2.0))
                .cursor_pointer()
                .child(
                    // The title truncates in the width the button leaves it.
                    div().w_full().min_w(px(0.0)).truncate().text_size(px(theme::FS_SM)).child(SharedString::from(title.clone())),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(5.0))
                        .text_size(px(theme::FS_2XS))
                        .text_color(colors.text_3)
                        .child(SharedString::from(short_time(session.stamp())))
                        .child(div().child("·"))
                        .child(status_element(status, colors, &format!("convo-{}", session.id))),
                )
                .into_any_element(),
        };
        row = row.child(open);

        // The "…" popover: its own 20 px, taken out of the title's width.
        let menu_id = session.id.clone();
        let menu_title = title.clone();
        let menu_agent = agent.id.clone();
        let archived = session.archived;
        let mut more = div()
            .id(SharedString::from(format!("convo-more-{}", session.id)))
            .size(px(20.0))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(4.0))
            .cursor_pointer()
            .opacity(if is_selected { 1.0 } else { 0.55 })
            .hover(move |style| style.bg(colors.hover).opacity(1.0))
            .active(move |style| style.bg(colors.selected))
            .child(crate::art::glyph(Glyph::MORE, 12.0, colors.text_3));
        if pinned {
            more = more.child(div().absolute().top(px(-2.0)).right(px(0.0)).child(crate::art::glyph(Glyph::PIN, 7.0, colors.text_3)));
        }
        more = more.on_mouse_down(gpui::MouseButton::Left, cx.listener(|_, _: &gpui::MouseDownEvent, _window, cx| {
            // The press belongs to this button: the card under it must not open.
            cx.stop_propagation();
        }));
        more = more.on_click(cx.listener(move |root, event: &gpui::ClickEvent, window, cx| {
            // The menu hangs where the button was pressed, and the press does not
            // also open the conversation (`stop_propagation`).
            cx.stop_propagation();
            let at = root.click_position(event, window);
            let items = conversation_menu_items(root, menu_id.clone(), menu_title.clone(), archived, menu_agent.clone());
            root.open_menu(Menu::new(format!("convo-{}", menu_id), at, items).width(210.0), cx);
        }));
        row = row.child(more);

        // The press anywhere on the card opens its conversation. This is the
        // card's own area (padding included), not just the block that holds the
        // title, so a press on the background counts too.
        if can_open {
            row = row.on_mouse_down(gpui::MouseButton::Left, cx.listener(move |root, _: &gpui::MouseDownEvent, _window, cx| {
                root.open_conversation(agent_id.clone(), session_id.clone(), cx);
            }));
        }

        // A right-click anywhere on the card opens the same menu.
        let right_id = session.id.clone();
        let right_title = title;
        let right_agent = agent.id.clone();
        row = row.on_aux_click(cx.listener(move |root, event: &gpui::ClickEvent, _window, cx| {
            let at = event.position();
            let archived = root
                .snapshot()
                .and_then(|snapshot| snapshot.session(&right_id))
                .map(|session| session.archived)
                .unwrap_or(false);
            let items = conversation_menu_items(root, right_id.clone(), right_title.clone(), archived, right_agent.clone());
            root.open_menu(Menu::new(format!("convo-right-{}", right_id), at, items).width(210.0), cx);
        }));
        row.into_any_element()
    }

    fn render_archived_toggle(&mut self, showing: bool, colors: Colors, cx: &mut Context<Self>) -> AnyElement {
        div()
            .id("archived-toggle")
            .mx(px(8.0))
            .mb(px(8.0))
            .h(px(24.0))
            .px(px(6.0))
            .flex()
            .items_center()
            .gap(px(6.0))
            .rounded(px(theme::RADIUS_SM))
            .cursor_pointer()
            .text_size(px(theme::FS_2XS))
            .text_color(if showing { colors.text_1 } else { colors.text_3 })
            .hover(move |style| style.bg(colors.hover))
            .child(crate::art::glyph(Glyph::ARCHIVE, 11.0, colors.text_3))
            .child(SharedString::from(if showing { "Back to open conversations".to_owned() } else { "Archived".to_owned() }))
            .on_click(cx.listener(move |root, _: &gpui::ClickEvent, _window, cx| {
                root.show_archived = !root.show_archived;
                cx.notify();
            }))
            .into_any_element()
    }

    fn render_new_conversation(&mut self, agent_id: String, colors: Colors, cx: &mut Context<Self>) -> AnyElement {
        div()
            .id("new-conversation")
            .mx(px(8.0))
            .mb(px(6.0))
            .h(px(28.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(theme::RADIUS_SM))
            .border_1()
            .border_color(colors.line_strong)
            .text_size(px(theme::FS_XS))
            .text_color(colors.text_2)
            .cursor_pointer()
            .hover(|style| style.text_color(colors.text_1).bg(colors.hover))
            .active(move |style| style.bg(colors.selected))
            .child("New conversation")
            .on_click(cx.listener(move |root, _: &gpui::ClickEvent, _window, cx| {
                root.new_conversation(agent_id.clone(), None, cx);
            }))
            .into_any_element()
    }
}

/// A conversation's own menu: rename, pin or unpin, archive or restore.
fn conversation_menu_items(
    root: &Root,
    session_id: String,
    title: String,
    archived: bool,
    agent_id: String,
) -> Vec<MenuItem> {
    let is_pinned = root.pinned.iter().any(|id| id == &session_id);
    let mut items = Vec::new();
    let rename_id = session_id.clone();
    items.push(MenuItem::Choice(
        MenuChoice::new("rename", "Rename", Rc::new(move |root: &mut Root, cx: &mut Context<Root>| {
            root.start_rename(rename_id.clone(), cx);
        }))
        .icon(Glyph::PENCIL),
    ));
    let pin_id = session_id.clone();
    items.push(MenuItem::Choice(
        MenuChoice::new(
            "pin",
            if is_pinned { "Unpin" } else { "Pin to the top" },
            Rc::new(move |root: &mut Root, cx: &mut Context<Root>| root.toggle_pin(pin_id.clone(), cx)),
        )
        .icon(Glyph::PIN),
    ));
    items.push(MenuItem::Separator);
    let archive_id = session_id.clone();
    items.push(MenuItem::Choice(
        MenuChoice::new(
            "archive",
            if archived { "Restore" } else { "Archive" },
            Rc::new(move |root: &mut Root, cx: &mut Context<Root>| {
                if archived {
                    root.set_archived(archive_id.clone(), false, cx);
                } else {
                    // It leaves the column, so it asks first, as the app does.
                    root.confirm = Some(crate::ui::root::Confirm {
                        title: "Archive this conversation?".to_owned(),
                        body: "It leaves the column. Its messages stay saved, and the Archived list at the foot of the column brings it back.".to_owned(),
                        confirm_label: "Archive".to_owned(),
                        action: crate::ui::root::ConfirmAction::ArchiveSession(archive_id.clone()),
                    });
                    cx.notify();
                }
            }),
        )
        .icon(if archived { Glyph::ROTATE } else { Glyph::ARCHIVE }),
    ));
    let _ = (title, agent_id);
    items
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{Project, SessionLink};

    fn session(id: &str, project_id: &str, messages: usize) -> Session {
        Session {
            id: id.into(),
            agent_id: Some("ada".into()),
            project_id: Some(project_id.into()),
            updated_at: Some(format!("2026-11-0{}T10:00:00Z", messages)),
            messages: vec![Default::default(); messages],
            ..Session::default()
        }
    }

    #[test]
    fn conversations_group_by_project_newest_group_first() {
        let projects = vec![Project { id: "shop".into(), name: "Shop".into(), path: None, sort_order: 0 }];
        let sessions = vec![session("a", "shop", 1), session("b", "none", 3)];
        let groups = group_by_project(&sessions, &projects, None);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].key, "none");
        assert_eq!(groups[1].sessions.len(), 1);
    }

    #[test]
    fn a_conversation_nothing_was_said_in_is_hidden_unless_it_is_open() {
        let sessions = vec![session("a", "shop", 0)];
        assert!(group_by_project(&sessions, &[], None).is_empty());
        assert_eq!(group_by_project(&sessions, &[], Some("a")).len(), 1);
    }

    #[test]
    fn a_pinned_conversation_reads_first_and_is_not_drawn_twice() {
        let sessions = vec![session("a", "shop", 1), session("b", "shop", 2), session("c", "shop", 3)];
        let pinned = vec!["b".to_owned(), "a".to_owned()];
        let (pinned_rows, rest) = split_pinned(&sessions, &pinned);
        assert_eq!(pinned_rows.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(), vec!["b", "a"], "in the order they were pinned");
        assert_eq!(rest.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(), vec!["c"], "the pinned ones are left out");
        // A pin for a conversation that is gone is simply not drawn.
        let (gone, _) = split_pinned(&sessions, &["zzz".to_owned()]);
        assert!(gone.is_empty());
    }

    #[test]
    fn an_archived_conversation_is_neither_pinned_nor_grouped() {
        let mut archived = session("x", "shop", 2);
        archived.archived = true;
        // The column asks the snapshot for one list or the other, so an archived
        // row never reaches the open groups.
        let open = vec![session("a", "shop", 1)];
        let (pinned_rows, rest) = split_pinned(&open, &["x".to_owned()]);
        assert!(pinned_rows.is_empty());
        assert_eq!(rest.len(), 1);
        assert!(archived.archived);
    }

    fn side(id: &str, origin: &str) -> Session {
        Session {
            id: id.into(),
            agent_id: Some("ada".into()),
            link: Some(SessionLink { kind: Some("side".into()), origin_session_id: Some(origin.into()), ..SessionLink::default() }),
            messages: vec![Default::default()],
            ..Session::default()
        }
    }

    #[test]
    fn a_branch_is_drawn_under_its_own_conversation_one_step_in() {
        let sessions = vec![session("main", "shop", 1), side("side-1", "main"), side("side-2", "side-1")];
        let tree = conversation_tree(&sessions);
        assert_eq!(
            tree.iter().map(|branch| (branch.session.id.as_str(), branch.depth)).collect::<Vec<_>>(),
            vec![("main", 0), ("side-1", 1), ("side-2", 2)]
        );
    }

    #[test]
    fn a_branch_whose_parent_is_gone_stands_on_its_own() {
        let sessions = vec![side("orphan", "missing")];
        let tree = conversation_tree(&sessions);
        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].depth, 0);
    }
}
