//! The conversations column: the selected bot's conversations, grouped by the
//! project each one works in, as two-line cards (the title, then `time · status`),
//! plus one "New conversation" button. Ported from
//! `apps/desktop/src/ui/ConversationsPanel.tsx` without the branch tree, rename
//! and archive, which are later slices.

use gpui::{div, prelude::*, px, AnyElement, Context, SharedString};

use crate::state::{conversation_status, project_of_session, Project, Session};
use crate::theme::{self, Colors};
use crate::ui::root::Root;
use crate::ui::sidebar::{short_time, status_element};

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
        let sessions = self.snapshot().map(|snapshot| snapshot.conversations_for(&agent.id)).unwrap_or_default();
        let projects = self.snapshot().map(|snapshot| snapshot.projects.clone()).unwrap_or_default();
        let approvals = self.approval_session_ids();
        let selected = self.session_id.clone();
        let groups = group_by_project(&sessions, &projects, selected.as_deref());

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

        let mut body = div().id("conversations-scroll").flex_1().w_full().overflow_y_scroll().px(px(8.0)).py(px(6.0));
        if sessions.is_empty() {
            body = body.child(
                div()
                    .px(px(4.0))
                    .py(px(6.0))
                    .text_size(px(theme::FS_XS))
                    .text_color(colors.text_3)
                    .child(SharedString::from(format!("No conversations with {} yet.", agent.name))),
            );
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
                let is_selected = selected.as_deref() == Some(session.id.as_str());
                let status = conversation_status(session, approvals.contains(&session.id));
                let agent_id = agent.id.clone();
                let session_id = session.id.clone();
                let title = session.title_or_default().to_owned();
                section = section.child(
                    div()
                        .id(SharedString::from(format!("convo-card-{}", session.id)))
                        .w_full()
                        .flex()
                        .flex_col()
                        .gap(px(2.0))
                        .px(px(10.0))
                        .py(px(tokens.card_pad_y))
                        .rounded(px(theme::RADIUS_MD))
                        .text_color(if is_selected { colors.text_1 } else { colors.text_2 })
                        .when(is_selected, |this| this.bg(colors.selected))
                        .when(!is_selected, |this| this.hover(|style| style.bg(colors.hover).text_color(colors.text_1)))
                        .cursor_pointer()
                        .child(div().truncate().text_size(px(theme::FS_SM)).child(SharedString::from(title)))
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
                        .on_click(cx.listener(move |root, _: &gpui::ClickEvent, _window, cx| {
                            root.open_conversation(agent_id.clone(), session_id.clone(), cx);
                        })),
                );
            }
            body = body.child(section);
        }
        column = column.child(body);
        column.into_any_element()
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
            .child("New conversation")
            .on_click(cx.listener(move |root, _: &gpui::ClickEvent, _window, cx| {
                root.new_conversation(agent_id.clone(), None, cx);
            }))
            .into_any_element()
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Project;

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
}
