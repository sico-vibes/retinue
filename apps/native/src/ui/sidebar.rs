//! The sidebar, ported from `apps/desktop/src/ui/TeamSidebar.tsx` (its search
//! box and CEO row) and `ui/ProjectsSidebar.tsx` (the Projects tree, the
//! Projects | Team switch and the roster).
//!
//! Room and rules follow the design language of 8 November 2026: project headers
//! 32px with a fold chevron, employee rows two lines with a 28px portrait, the
//! employee holding the selection shows its newest four conversations as
//! two-line cards, and a conversation's state is a word at the end of its line.

use std::collections::BTreeSet;

use gpui::{div, prelude::*, px, AnyElement, Context, SharedString};

use crate::state::{conversation_status, project_blocks, Agent, ConversationStatus, Session, Tone};
use crate::theme::{self, Colors};
use crate::ui::root::{Root, SidebarView};

impl Root {
    pub(crate) fn render_sidebar(&mut self, colors: Colors, cx: &mut Context<Self>) -> AnyElement {
        div()
            .w(px(theme::SIDEBAR_WIDTH))
            .h_full()
            .flex_none()
            .flex()
            .flex_col()
            .bg(colors.sidebar)
            .border_r_1()
            .border_color(colors.line)
            .child(self.render_search(colors))
            .child(self.render_ceo_row(colors))
            .child(self.render_view_switch(colors, cx))
            .child(
                div()
                    .id("sidebar-scroll")
                    .flex_1()
                    .w_full()
                    .overflow_y_scroll()
                    .child(match self.view {
                        SidebarView::Projects => self.render_projects_tree(colors, cx).into_any_element(),
                        SidebarView::Team => self.render_team_list(colors, cx).into_any_element(),
                    }),
            )
            .child(self.render_new_project(colors, cx))
            .into_any_element()
    }

    /// The search box: a placeholder in this slice, exactly as the brief allows.
    fn render_search(&self, colors: Colors) -> AnyElement {
        div()
            .mx(px(8.0))
            .mt(px(8.0))
            .h(px(30.0))
            .px(px(9.0))
            .flex()
            .items_center()
            .rounded(px(theme::RADIUS_SM))
            .bg(colors.input)
            .border_1()
            .border_color(colors.line)
            .text_color(colors.text_3)
            .text_size(px(theme::FS_XS))
            .child("Search")
            .into_any_element()
    }

    fn render_ceo_row(&self, colors: Colors) -> AnyElement {
        let Some(manager) = self.snapshot().and_then(|snapshot| snapshot.manager()) else {
            return div().into_any_element();
        };
        let initial = initial_of(&manager.name);
        let tint = agent_color(&manager.color, colors);
        div()
            .mx(px(8.0))
            .mt(px(8.0))
            .px(px(8.0))
            .h(px(38.0))
            .flex()
            .items_center()
            .gap(px(9.0))
            .rounded(px(theme::RADIUS_MD))
            .child(
                div()
                    .size(px(26.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(theme::RADIUS_SM))
                    .bg(colors.avatar)
                    .text_color(tint)
                    .text_size(px(theme::FS_XS))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(initial),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(div().text_size(px(theme::FS_SM)).font_weight(gpui::FontWeight::MEDIUM).child(SharedString::from(manager.name.clone())))
                    .child(div().text_size(px(theme::FS_2XS)).text_color(colors.text_3).child("CEO")),
            )
            .into_any_element()
    }

    fn render_view_switch(&mut self, colors: Colors, cx: &mut Context<Self>) -> AnyElement {
        let projects = self.switch_button("Projects", SidebarView::Projects, colors, cx);
        let team = self.switch_button("Team", SidebarView::Team, colors, cx);
        div()
            .mx(px(8.0))
            .my(px(8.0))
            .p(px(2.0))
            .flex()
            .gap(px(2.0))
            .rounded(px(theme::RADIUS_MD))
            .bg(colors.hover)
            .child(projects)
            .child(team)
            .into_any_element()
    }

    fn switch_button(&self, label: &str, view: SidebarView, colors: Colors, cx: &mut Context<Self>) -> AnyElement {
        let active = self.view == view;
        div()
            .id(SharedString::from(format!("view-{label}")))
            .flex_1()
            .h(px(26.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(theme::RADIUS_SM))
            .bg(if active { colors.selected } else { gpui::rgba(0x00000000) })
            .text_color(if active { colors.text_1 } else { colors.text_2 })
            .text_size(px(theme::FS_XS))
            .cursor_pointer()
            .hover(|style| style.text_color(colors.text_1))
            .child(SharedString::from(label.to_owned()))
            .on_click(cx.listener(move |root, _: &gpui::ClickEvent, _window, cx| root.set_view(view, cx)))
            .into_any_element()
    }

    fn render_new_project(&mut self, colors: Colors, cx: &mut Context<Self>) -> AnyElement {
        div()
            .id("new-project")
            .mx(px(8.0))
            .mb(px(8.0))
            .h(px(28.0))
            .px(px(8.0))
            .flex()
            .items_center()
            .gap(px(6.0))
            .rounded(px(theme::RADIUS_SM))
            .text_color(colors.text_2)
            .text_size(px(theme::FS_XS))
            .cursor_pointer()
            .hover(|style| style.text_color(colors.text_1))
            .child("New project")
            .on_click(cx.listener(|root, _: &gpui::ClickEvent, _window, cx| {
                root.notice = Some("New project comes in a later slice.".to_owned());
                cx.notify();
            }))
            .into_any_element()
    }

    fn render_projects_tree(&mut self, colors: Colors, cx: &mut Context<Self>) -> AnyElement {
        let employees: Vec<Agent> = self.employees().into_iter().cloned().collect();
        let projects = self.snapshot().map(|snapshot| snapshot.projects.clone()).unwrap_or_default();
        let sessions = self.sessions().to_vec();
        let members = self.members.clone();
        let blocks = project_blocks(&employees, &projects, &sessions, members.as_ref());
        let approvals = self.approval_session_ids();
        let selected_session = self.session_id.clone();
        let folded = self.folded_projects.clone();

        let mut list = div().flex().flex_col().gap(px(theme::GROUP_GAP)).px(px(6.0)).py(px(4.0));
        if blocks.is_empty() {
            list = list.child(div().px(px(6.0)).py(px(6.0)).text_size(px(theme::FS_XS)).text_color(colors.text_3).child("No projects yet."));
        }
        for block in &blocks {
            let name = block.project.as_ref().map(|project| project.name.clone()).unwrap_or_else(|| "No project".to_owned());
            let is_folded = folded.contains(&block.key);
            let key = block.key.clone();
            let fold_icon = if is_folded { "›" } else { "⌄" };
            let mut section = div()
                .flex()
                .flex_col()
                .gap(px(2.0))
                .child(
                    div()
                        .id(SharedString::from(format!("project-{}", block.key)))
                        .h(px(32.0))
                        .px(px(8.0))
                        .flex()
                        .items_center()
                        .gap(px(7.0))
                        .rounded(px(theme::RADIUS_SM))
                        .text_color(colors.text_2)
                        .text_size(px(theme::FS_SM))
                        .cursor_pointer()
                        .hover(|style| style.text_color(colors.text_1).bg(colors.hover))
                        .child(div().w(px(10.0)).text_size(px(theme::FS_XS)).child(fold_icon))
                        .child(div().flex_1().truncate().child(SharedString::from(name.clone())))
                        .when(is_folded && !block.bots.is_empty(), |this| {
                            this.child(
                                div()
                                    .text_size(px(theme::FS_2XS))
                                    .text_color(colors.text_3)
                                    .child(SharedString::from(block.bots.len().to_string())),
                            )
                        })
                        .on_click(cx.listener(move |root, _: &gpui::ClickEvent, _window, cx| {
                            root.toggle_project_fold(key.clone(), cx);
                        })),
                );

            if !is_folded {
                let mut rows = div().flex().flex_col().gap(px(theme::ROW_GAP)).pl(px(8.0)).py(px(2.0));
                if block.bots.is_empty() {
                    rows = rows.child(
                        div()
                            .px(px(10.0))
                            .py(px(4.0))
                            .text_size(px(theme::FS_XS))
                            .text_color(colors.text_3)
                            .child("No one here yet."),
                    );
                }
                for bot in &block.bots {
                    let Some(agent) = employees.iter().find(|agent| agent.id == bot.agent_id).cloned() else { continue };
                    let row_key = format!("{}:{}", block.key, agent.id);
                    let recent: Vec<&Session> = bot
                        .session_ids
                        .iter()
                        .take(crate::state::projects::RECENT_PER_BOT)
                        .filter_map(|id| sessions.iter().find(|session| &session.id == id))
                        .collect();
                    let holds_selection = recent.iter().any(|session| Some(&session.id) == selected_session.as_ref());
                    let expanded = !recent.is_empty() && self.employee_folds.get(&row_key).copied().unwrap_or(holds_selection);
                    let needs_you = self.employee_needs_you(&agent.id, &approvals);
                    let working = recent.iter().any(|session| session.is_busy());
                    rows = rows.child(self.render_employee(colors, cx, &agent, &recent, holds_selection, expanded, needs_you, working, &row_key));
                }
                section = section.child(rows);
            }
            list = list.child(section);
        }
        list.into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn render_employee(
        &self,
        colors: Colors,
        cx: &mut Context<Self>,
        agent: &Agent,
        recent: &[&Session],
        holds_selection: bool,
        expanded: bool,
        needs_you: bool,
        working: bool,
        row_key: &str,
    ) -> AnyElement {
        let latest = recent.first().copied();
        let tint = agent_color(&agent.color, colors);
        let initial = initial_of(&agent.name);
        let status: Option<ConversationStatus> = if needs_you {
            Some(ConversationStatus { label: "Needs you", tone: Tone::Needs })
        } else if working {
            Some(ConversationStatus { label: "Working", tone: Tone::Working })
        } else {
            None
        };
        let time = latest.map(|session| short_time(session.stamp())).unwrap_or_default();
        let subtitle = agent
            .role
            .as_deref()
            .map(str::trim)
            .filter(|role| !role.is_empty())
            .map(str::to_owned)
            .or_else(|| latest.map(|session| session.title_or_default().to_owned()))
            .unwrap_or_else(|| "No conversations yet".to_owned());

        let agent_id = agent.id.clone();
        let latest_id = latest.map(|session| session.id.clone());
        let row_key_owned = row_key.to_owned();
        let more_open = !recent.is_empty();
        let more_agent = agent.id.clone();

        let mut row = div()
            .flex()
            .flex_col()
            .rounded(px(theme::RADIUS_MD))
            .when(holds_selection, |this| this.bg(colors.selected))
            .child(
                div()
                    .id(SharedString::from(format!("employee-{row_key}")))
                    .h(px(theme::PERSON_HEIGHT))
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .rounded(px(theme::RADIUS_MD))
                    .cursor_pointer()
                    .when(!holds_selection, |this| this.hover(|style| style.bg(colors.hover)))
                    .child(
                        div()
                            .size(px(28.0))
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(theme::RADIUS_SM))
                            .bg(colors.avatar)
                            .text_color(tint)
                            .text_size(px(theme::FS_XS))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(initial),
                    )
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap(px(1.0))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(8.0))
                                    .child(
                                        div()
                                            .flex_1()
                                            .truncate()
                                            .text_size(px(theme::FS_SM))
                                            .font_weight(gpui::FontWeight::MEDIUM)
                                            .text_color(colors.text_1)
                                            .child(SharedString::from(agent.name.clone())),
                                    )
                                    .child(match status {
                                        Some(status) => status_element(status, colors),
                                        None if !time.is_empty() => div()
                                            .flex_none()
                                            .text_size(px(theme::FS_2XS))
                                            .text_color(colors.text_3)
                                            .child(SharedString::from(time.clone()))
                                            .into_any_element(),
                                        None => div().into_any_element(),
                                    }),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(theme::FS_XS))
                                    .text_color(colors.text_3)
                                    .child(SharedString::from(subtitle)),
                            ),
                    )
                    .on_click(cx.listener(move |root, _: &gpui::ClickEvent, _window, cx| match &latest_id {
                        Some(session_id) => {
                            if !expanded {
                                root.set_employee_fold(row_key_owned.clone(), true, cx);
                            }
                            root.open_conversation(agent_id.clone(), session_id.clone(), cx);
                        }
                        None => root.new_conversation(agent_id.clone(), None, cx),
                    })),
            );

        if more_open && recent.len() > 1 {
            let fold_key = row_key.to_owned();
            row = row.child(
                div()
                    .id(SharedString::from(format!("employee-more-{row_key}")))
                    .ml(px(38.0))
                    .h(px(16.0))
                    .text_size(px(theme::FS_2XS))
                    .text_color(colors.text_3)
                    .cursor_pointer()
                    .hover(|style| style.text_color(colors.text_1))
                    .child(if expanded { "Hide conversations" } else { "Show conversations" })
                    .on_click(cx.listener(move |root, _: &gpui::ClickEvent, _window, cx| {
                        // The owner's choice wins from now on, so it is written
                        // even when it agrees with what the selection implied.
                        root.set_employee_fold(fold_key.clone(), !expanded, cx);
                    })),
            );
            let _ = more_agent;
        }

        if expanded {
            let mut cards = div().flex().flex_col().gap(px(theme::ROW_GAP)).pl(px(26.0)).py(px(theme::NEST_GAP));
            for session in recent {
                let selected = Some(&session.id) == self.session_id.as_ref();
                let status = conversation_status(session, self.approval_session_ids().contains(&session.id));
                let agent_id = agent.id.clone();
                let session_id = session.id.clone();
                cards = cards.child(
                    div()
                        .id(SharedString::from(format!("conversation-{}", session.id)))
                        .w_full()
                        .flex()
                        .flex_col()
                        .gap(px(2.0))
                        .px(px(10.0))
                        .py(px(theme::CARD_PAD_Y))
                        .rounded(px(theme::RADIUS_MD))
                        .text_color(if selected { colors.text_1 } else { colors.text_2 })
                        .when(selected, |this| this.bg(colors.selected))
                        .when(!selected, |this| this.hover(|style| style.bg(colors.hover).text_color(colors.text_1)))
                        .cursor_pointer()
                        .child(div().truncate().text_size(px(theme::FS_SM)).child(SharedString::from(session.title_or_default().to_owned())))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(5.0))
                                .text_size(px(theme::FS_2XS))
                                .text_color(colors.text_3)
                                .child(SharedString::from(short_time(session.stamp())))
                                .child(div().child("·"))
                                .child(status_element(status, colors)),
                        )
                        .on_click(cx.listener(move |root, _: &gpui::ClickEvent, _window, cx| {
                            root.open_conversation(agent_id.clone(), session_id.clone(), cx);
                        })),
                );
            }
            row = row.child(cards);
        }
        row.into_any_element()
    }

    fn render_team_list(&mut self, colors: Colors, cx: &mut Context<Self>) -> AnyElement {
        let employees: Vec<Agent> = self.employees().into_iter().cloned().collect();
        let approvals = self.approval_session_ids();
        let mut list = div().flex().flex_col().gap(px(theme::ROW_GAP)).p(px(6.0));
        if employees.is_empty() {
            list = list.child(div().px(px(6.0)).py(px(6.0)).text_size(px(theme::FS_XS)).text_color(colors.text_3).child("No employees yet"));
        }
        for agent in &employees {
            let tint = agent_color(&agent.color, colors);
            let initial = initial_of(&agent.name);
            let needs_you = self.employee_needs_you(&agent.id, &approvals);
            let working = self
                .sessions()
                .iter()
                .any(|session| session.agent_id.as_deref() == Some(agent.id.as_str()) && session.is_busy());
            let status = if needs_you {
                Some(ConversationStatus { label: "Needs you", tone: Tone::Needs })
            } else if working {
                Some(ConversationStatus { label: "Working", tone: Tone::Working })
            } else {
                None
            };
            let agent_id = agent.id.clone();
            list = list.child(
                div()
                    .id(SharedString::from(format!("team-{}", agent.id)))
                    .h(px(theme::CARD_HEIGHT))
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .rounded(px(theme::RADIUS_MD))
                    .cursor_pointer()
                    .hover(|style| style.bg(colors.hover))
                    .child(
                        div()
                            .size(px(24.0))
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(theme::RADIUS_SM))
                            .bg(colors.avatar)
                            .text_color(tint)
                            .text_size(px(theme::FS_2XS))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(initial),
                    )
                    .child(div().flex_1().truncate().text_size(px(theme::FS_SM)).child(SharedString::from(agent.name.clone())))
                    .child(match status {
                        Some(status) => status_element(status, colors),
                        None => div()
                            .text_size(px(theme::FS_2XS))
                            .text_color(colors.text_3)
                            .child("Idle")
                            .into_any_element(),
                    })
                    .on_click(cx.listener(move |root, _: &gpui::ClickEvent, _window, cx| root.select_agent(agent_id.clone(), cx))),
            );
        }
        list.into_any_element()
    }
}

/// A status in words, with a bell for *Needs you* and the working colour while
/// something is happening. The shimmer is a later slice.
pub(crate) fn status_element(status: ConversationStatus, colors: Colors) -> AnyElement {
    let color = match status.tone {
        Tone::Needs => colors.status_working,
        Tone::Working => colors.status_working,
        Tone::Limited => colors.text_2,
        Tone::Bad => colors.status_danger,
        Tone::Idle => colors.text_3,
    };
    let icon = match status.tone {
        Tone::Needs => Some("◔"),
        Tone::Bad => Some("!"),
        _ => None,
    };
    div()
        .flex()
        .items_center()
        .gap(px(4.0))
        .text_size(px(theme::FS_2XS))
        .text_color(color)
        .when(status.tone == Tone::Needs, |this| this.font_weight(gpui::FontWeight::MEDIUM))
        .when_some(icon, |this, icon| this.child(div().text_size(px(theme::FS_2XS)).child(icon)))
        .child(SharedString::from(status.label))
        .into_any_element()
}

pub(crate) fn initial_of(name: &str) -> SharedString {
    SharedString::from(name.chars().next().map(|ch| ch.to_uppercase().to_string()).unwrap_or_else(|| "?".to_owned()))
}

/// The bot's own colour, from the app's swatch list (`ui/agentColor.ts`).
pub(crate) fn agent_color(value: &str, _colors: Colors) -> gpui::Rgba {
    let key = value.trim().to_lowercase();
    let hex = match key.as_str() {
        "coral" => 0xF38C6F,
        "orange" => 0xF0A05A,
        "amber" => 0xE8C15A,
        "lemon" => 0xE4D56A,
        "lime" => 0xA8D45C,
        "mint" => 0x89D6B3,
        "teal" => 0x5EBEB0,
        "cyan" => 0x6EC8E0,
        "sky" => 0x7EB6F0,
        "blue" => 0x82AAFF,
        "violet" => 0xBF9CFF,
        "pink" => 0xF0A0C4,
        other => {
            let custom = other.strip_prefix('#').unwrap_or(other);
            u32::from_str_radix(custom, 16).unwrap_or(0x89D6B3) & 0xFFFFFF
        }
    };
    gpui::rgb(hex)
}

/// "now", "3m", "4h", "2d", then a short date: `relativeTime` in
/// `ui/ConversationsPanel.tsx`.
pub(crate) fn short_time(stamp: &str) -> String {
    let Ok(time) = chrono::DateTime::parse_from_rfc3339(stamp) else { return String::new() };
    let now = chrono::Utc::now();
    let minutes = (now - time.with_timezone(&chrono::Utc)).num_minutes().max(0);
    if minutes < 1 {
        return "now".to_owned();
    }
    if minutes < 60 {
        return format!("{minutes}m");
    }
    let hours = (minutes as f64 / 60.0).round() as i64;
    if hours < 24 {
        return format!("{hours}h");
    }
    let days = (hours as f64 / 24.0).round() as i64;
    if days < 7 {
        return format!("{days}d");
    }
    time.format("%b %-d").to_string()
}

#[allow(dead_code)]
fn unused(_: &BTreeSet<String>) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_time_reads_as_a_short_word_or_number() {
        let now = chrono::Utc::now();
        assert_eq!(short_time(&now.to_rfc3339()), "now");
        let two_hours = now - chrono::Duration::minutes(125);
        assert_eq!(short_time(&two_hours.to_rfc3339()), "2h");
        assert_eq!(short_time("not a date"), "");
    }
}
