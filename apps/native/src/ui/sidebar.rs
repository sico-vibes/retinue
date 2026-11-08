//! The sidebar: `ui/TeamSidebar.tsx` (the header, the CEO row and the roster)
//! and `ui/ProjectsSidebar.tsx` (the Projects tree, the Projects | Team switch,
//! the project "+" menu and the roster).
//!
//! Follows the design language of 8 November 2026: project headers 32px with a
//! rotating chevron, employee rows two lines beside a 28px mark, the employee
//! holding the selection shows its newest four conversations as two-line cards,
//! and a conversation's state is a word at the end of its line. Spacing comes
//! from the density tokens, and the working word carries the sweep.

use std::collections::BTreeSet;
use std::time::Duration;

use gpui::{div, prelude::*, px, Animation, AnimationExt, AnyElement, Context, SharedString, Window};

use crate::art::{self, Glyph, Mark};
use crate::state::{conversation_status, project_blocks, Agent, ConversationStatus, Session, Tone};
use crate::theme::{self, Colors, DensityTokens};
use crate::ui::menu::{Menu, MenuChoice, MenuItem};
use crate::ui::motion;
use crate::ui::root::{Root, SidebarView};
use crate::ui::search::{matches, search_words};

/// The width a sidebar row gives its title, for the marquee. Layout, not magic:
/// the sidebar width less the row's portrait, padding and status.
const NAME_WIDTH: f32 = 150.0;
const TITLE_WIDTH: f32 = 200.0;

impl Root {
    pub(crate) fn render_sidebar(&mut self, colors: Colors, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let tokens = self.tokens();
        div()
            .w(px(theme::SIDEBAR_WIDTH))
            .h_full()
            .flex_none()
            .flex()
            .flex_col()
            .bg(colors.sidebar)
            .border_r_1()
            .border_color(colors.line)
            .child(self.render_search(colors, cx))
            .child(self.render_ceo_row(colors, cx))
            .child(self.render_view_switch(colors, cx))
            .child(
                div()
                    .id("sidebar-scroll")
                    .flex_1()
                    // A flex child refuses to shrink below its content without
                    // this, which pushed the tree over the footer.
                    .min_h(px(0.0))
                    .w_full()
                    .overflow_y_scroll()
                    .child(match self.view {
                        SidebarView::Projects => self.render_projects_tree(colors, tokens, window, cx).into_any_element(),
                        SidebarView::Team => self.render_team_list(colors, tokens, cx).into_any_element(),
                    }),
            )
            .child(self.render_sidebar_footer(colors, cx))
            .into_any_element()
    }

    /// The search box: it filters the tree and the roster as it is typed
    /// (`ui/ProjectSearch.tsx` behaviour), and a click puts the caret in it.
    fn render_search(&mut self, colors: Colors, cx: &mut Context<Self>) -> AnyElement {
        let query = self.sidebar_search.read(cx).text().to_owned();
        div()
            .id("sidebar-search")
            .mx(px(8.0))
            .mt(px(8.0))
            .h(px(30.0))
            .px(px(9.0))
            .flex()
            .items_center()
            .gap(px(7.0))
            .rounded(px(theme::RADIUS_SM))
            .bg(colors.input)
            .border_1()
            .border_color(colors.line)
            .hover(move |style| style.border_color(colors.line_strong))
            .child(art::glyph(Glyph::SEARCH, 12.0, colors.text_3))
            .child(div().flex_1().min_w(px(0.0)).child(self.sidebar_search.clone()))
            .when(!query.is_empty(), |this| {
                this.child(
                    div()
                        .id("search-clear")
                        .size(px(16.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(4.0))
                        .cursor_pointer()
                        .hover(move |style| style.bg(colors.hover))
                        .child(art::glyph(Glyph::X, 9.0, colors.text_3))
                        .on_click(cx.listener(|root, _: &gpui::ClickEvent, _window, cx| {
                            root.sidebar_search.update(cx, |input, cx| input.set_text(String::new(), cx));
                        })),
                )
            })
            .into_any_element()
    }

    fn render_ceo_row(&mut self, colors: Colors, cx: &mut Context<Self>) -> AnyElement {
        let Some(manager) = self.snapshot().and_then(|snapshot| snapshot.manager()).cloned() else {
            return div().into_any_element();
        };
        let provider = self.provider_of(&manager);
        let mark = Mark::for_agent(32.0, art::agent_color(&manager.color), manager.icon(), provider.as_deref(), manager.name.clone(), colors).render();
        // A CEO whose role is its own name would say "CEO" twice.
        let role = manager.role.as_deref().map(str::trim).unwrap_or("");
        let show_role = !role.is_empty() && !role.eq_ignore_ascii_case(manager.name.trim());
        div()
            .id("ceo-row")
            .mx(px(8.0))
            .mt(px(8.0))
            .px(px(4.0))
            .py(px(3.0))
            .flex()
            .items_center()
            .gap(px(9.0))
            .rounded(px(theme::RADIUS_MD))
            .cursor_pointer()
            .hover(move |style| style.bg(colors.hover))
            .child(mark)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(div().text_size(px(theme::FS_SM)).font_weight(gpui::FontWeight::MEDIUM).child(SharedString::from(manager.name.clone())))
                    .when(show_role, |this| {
                        this.child(div().text_size(px(theme::FS_2XS)).text_color(colors.text_3).child(SharedString::from(role.to_owned())))
                    }),
            )
            .on_click(cx.listener(move |root, _: &gpui::ClickEvent, _window, cx| root.select_agent(manager.id.clone(), cx)))
            .into_any_element()
    }

    /// Projects | Team, then "Open team page" (the Team page in the main area).
    fn render_view_switch(&mut self, colors: Colors, cx: &mut Context<Self>) -> AnyElement {
        let projects = self.switch_button("Projects", SidebarView::Projects, colors, cx);
        let team = self.switch_button("Team", SidebarView::Team, colors, cx);
        div()
            .flex()
            .flex_col()
            .gap(px(4.0))
            .mx(px(8.0))
            .my(px(8.0))
            .child(
                div()
                    .p(px(2.0))
                    .flex()
                    .gap(px(2.0))
                    .rounded(px(theme::RADIUS_MD))
                    .bg(colors.hover)
                    .child(projects)
                    .child(team),
            )
            .child(
                div()
                    .id("open-team-page")
                    .h(px(24.0))
                    .px(px(6.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(theme::RADIUS_SM))
                    .text_color(colors.text_2)
                    .text_size(px(theme::FS_XS))
                    .cursor_pointer()
                    .hover(move |style| style.text_color(colors.text_1).bg(colors.hover))
                    .child("Open team page")
                    .on_click(cx.listener(|root, _: &gpui::ClickEvent, _window, cx| root.open_team(crate::ui::root::TeamTab::Team, None, cx))),
            )
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
            .hover(move |style| style.text_color(colors.text_1))
            .child(SharedString::from(label.to_owned()))
            .on_click(cx.listener(move |root, _: &gpui::ClickEvent, _window, cx| root.set_view(view, cx)))
            .into_any_element()
    }

    /// New project, the bell, the plan-usage line and the gear that opens Settings.
    fn render_sidebar_footer(&mut self, colors: Colors, cx: &mut Context<Self>) -> AnyElement {
        let bell = self.render_needs_bell(colors, cx);
        let usage = self.render_usage_line(colors, cx);
        let mut footer = div()
            .mx(px(8.0))
            .mb(px(8.0))
            .mt(px(6.0))
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .child(
                        div()
                            .id("new-project")
                            .flex_1()
                            .h(px(28.0))
                            .px(px(8.0))
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .rounded(px(theme::RADIUS_SM))
                            .text_color(colors.text_2)
                            .text_size(px(theme::FS_XS))
                            .cursor_pointer()
                            .hover(move |style| style.text_color(colors.text_1).bg(colors.hover))
                            .active(move |style| style.bg(colors.selected))
                            .child(art::glyph(Glyph::PLUS, 12.0, colors.text_2))
                            .child("New project")
                            .on_click(cx.listener(|root, _: &gpui::ClickEvent, _window, cx| root.open_new_project(cx))),
                    )
                    .child(bell)
                    .child(
                        div()
                            .id("open-settings")
                            .size(px(28.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(theme::RADIUS_SM))
                            .cursor_pointer()
                            .hover(move |style| style.bg(colors.hover))
                            .child(art::glyph(Glyph::SETTINGS, 14.0, colors.text_2))
                            .on_click(cx.listener(|root, _: &gpui::ClickEvent, _window, cx| root.open_settings(cx))),
                    ),
            );
        if let Some(usage) = usage {
            footer = footer.child(usage);
        }
        footer.into_any_element()
    }

    fn render_projects_tree(
        &mut self,
        colors: Colors,
        tokens: DensityTokens,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let employees_all: Vec<Agent> = self.employees().into_iter().cloned().collect();
        let filter = search_words(self.sidebar_search.read(cx).text());
        let employees: Vec<Agent> = employees_all
            .iter()
            .filter(|agent| matches(&filter, &[agent.name.as_str(), agent.role_or_default(), agent.description.as_str()]))
            .cloned()
            .collect();
        let projects_all = self.snapshot().map(|snapshot| snapshot.projects.clone()).unwrap_or_default();
        let projects: Vec<crate::state::Project> = projects_all
            .iter()
            .filter(|project| matches(&filter, &[project.name.as_str(), project.path.clone().unwrap_or_default().as_str()]))
            .cloned()
            .collect();
        // With words in the box, a project stays when it or one of its bots
        // answers them; with an empty box the whole tree is shown.
        let sessions_all = self.sessions().to_vec();
        let sessions: Vec<Session> = sessions_all
            .iter()
            .filter(|session| {
                filter.is_empty()
                    || matches(&filter, &[session.title_or_default()])
                    || session.agent_id.as_deref().is_some_and(|agent_id| employees.iter().any(|agent| agent.id == agent_id))
                    || projects.iter().any(|project| Some(&project.id) == session.project_id.as_ref())
            })
            .cloned()
            .collect();
        let members = self.members.clone();
        let blocks = project_blocks(&employees, &projects, &sessions, members.as_ref());
        let approvals = self.approval_session_ids();
        let selected_session = self.session_id.clone();
        let folded = self.folded_projects.clone();
        let hovered = self.hovered.clone();

        let mut list = div().flex().flex_col().gap(px(tokens.group_gap)).px(px(6.0)).py(px(4.0));
        if blocks.is_empty() {
            list = list.child(
                div()
                    .px(px(6.0))
                    .py(px(6.0))
                    .text_size(px(theme::FS_XS))
                    .text_color(colors.text_3)
                    .child(SharedString::from(if filter.is_empty() { "No projects yet.".to_owned() } else { "Nothing matches that search.".to_owned() })),
            );
        }
        for block in &blocks {
            let name = block.project.as_ref().map(|project| project.name.clone()).unwrap_or_else(|| "No project".to_owned());
            let is_folded = folded.contains(&block.key);
            let closing = self.is_closing(&block.key);
            let key = block.key.clone();
            let project_id = block.project.as_ref().map(|project| project.id.clone());
            let add_key = block.key.clone();
            let add_name = name.clone();
            let add_project = project_id.clone();
            let mut head = div()
                .flex()
                .items_center()
                .gap(px(2.0))
                .child(
                    div()
                        .id(SharedString::from(format!("project-{}", block.key)))
                        .flex_1()
                        .h(px(32.0))
                        .px(px(8.0))
                        .flex()
                        .items_center()
                        .gap(px(7.0))
                        .rounded(px(theme::RADIUS_SM))
                        .text_color(colors.text_2)
                        .text_size(px(theme::FS_SM))
                        .cursor_pointer()
                        .hover(move |style| style.text_color(colors.text_1).bg(colors.hover))
                        .child(fold_chevron(!is_folded, closing, colors))
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
            // The "+" that opens the project menu: a conversation, a hire, or the CEO.
            if project_id.is_some() {
                head = head.child(
                    div()
                        .id(SharedString::from(format!("project-add-{}", add_key)))
                        .size(px(24.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(theme::RADIUS_SM))
                        .opacity(0.55)
                        .cursor_pointer()
                        .hover(move |style| style.bg(colors.hover).opacity(1.0))
                        .active(move |style| style.bg(colors.selected))
                        .child(art::glyph(Glyph::PLUS, 12.0, colors.text_2))
                        .on_click(cx.listener(move |root, event: &gpui::ClickEvent, window, cx| {
                            let at = root.click_position(event, window);
                            let items = project_menu_items(add_project.clone(), add_name.clone());
                            root.open_menu(Menu::new(format!("project-{add_key}"), at, items).width(240.0), cx);
                        })),
                );
            }

            let mut section = div().flex().flex_col().gap(px(2.0)).child(head);

            let mut rows = div().flex().flex_col().gap(px(tokens.row_gap)).pl(px(8.0)).py(px(2.0));
            if block.bots.is_empty() {
                // "No one hired yet. Hire an employee" (`ProjectsSidebar.tsx:147`).
                let hire_project = project_id.clone();
                rows = rows.child(
                    div()
                        .id(SharedString::from(format!("hire-empty-{}", block.key)))
                        .px(px(10.0))
                        .py(px(4.0))
                        .text_size(px(theme::FS_XS))
                        .text_color(colors.text_3)
                        .when(hire_project.is_some(), |this| {
                            this.cursor_pointer()
                                .hover(move |style| style.text_color(colors.text_1))
                        })
                        .child(if hire_project.is_some() { "No one hired yet. Hire an employee" } else { "No one here yet." })
                        .when_some(hire_project, |this, project_id| {
                            this.on_click(cx.listener(move |root, _: &gpui::ClickEvent, _window, cx| {
                                root.open_team(crate::ui::root::TeamTab::Hire, Some(project_id.clone()), cx);
                            }))
                        }),
                );
            }
            let mut content_height = 0.0;
            for bot in &block.bots {
                let Some(agent) = employees.iter().find(|agent| agent.id == bot.agent_id).cloned() else { continue };
                let row_key = format!("{}:{}", block.key, agent.id);
                let recent: Vec<Session> = bot
                    .session_ids
                    .iter()
                    .take(crate::state::projects::RECENT_PER_BOT)
                    .filter_map(|id| sessions.iter().find(|session| &session.id == id).cloned())
                    .collect();
                let holds_selection = recent.iter().any(|session| Some(&session.id) == selected_session.as_ref());
                let expanded = !recent.is_empty()
                    && self.employee_folds.get(&row_key).copied().unwrap_or(holds_selection);
                let card_closing = self.is_closing(&row_key);
                let needs_you = self.employee_needs_you(&agent.id, &approvals);
                let working = recent.iter().any(|session| session.is_busy());
                let cards_height = if recent.len() > 1 {
                    recent.len() as f32 * (tokens.card_h) + tokens.row_gap * (recent.len() - 1) as f32 + tokens.nest_gap
                } else {
                    0.0
                };
                content_height += tokens.person_h + if expanded || card_closing { cards_height } else { 0.0 } + tokens.row_gap;
                rows = rows.child(self.render_employee(
                    colors,
                    tokens,
                    window,
                    cx,
                    &agent,
                    &recent,
                    holds_selection,
                    expanded,
                    card_closing,
                    needs_you,
                    working,
                    &row_key,
                    hovered.as_deref() == Some(row_key.as_str()),
                    cards_height,
                ));
            }
            if !is_folded || closing {
                section = section.child(collapsible(
                    format!("project-rows-{}", block.key),
                    !is_folded,
                    closing,
                    content_height.max(tokens.person_h) + 6.0,
                    rows.into_any_element(),
                ));
            }
            list = list.child(section);
        }
        list.into_any_element()
    }

}

/// The project menu's rows, built where the trigger is so each one owns what it
/// needs.
fn project_menu_items(project_id: Option<String>, project_name: String) -> Vec<MenuItem> {
    let mut items = Vec::new();
    if project_id.is_none() {
        items.push(MenuItem::Choice(
            MenuChoice::new(
                "new-conversation",
                "New conversation",
                std::rc::Rc::new(move |root: &mut Root, cx: &mut Context<Root>| {
                    if let Some(agent_id) = root.agent_id.clone() {
                        root.new_conversation(agent_id, None, cx);
                    } else {
                        root.error = Some("Choose an employee first.".to_owned());
                        cx.notify();
                    }
                }),
            )
            .icon(Glyph::MESSAGES),
        ));
        return items;
    }
    let project = project_id.clone();
    items.push(MenuItem::Choice(
        MenuChoice::new(
            "new-conversation",
            "New conversation",
            std::rc::Rc::new(move |root: &mut Root, cx: &mut Context<Root>| {
                if let Some(agent_id) = root.agent_id.clone() {
                    root.new_conversation(agent_id, project.clone(), cx);
                } else {
                    root.error = Some("Choose an employee first.".to_owned());
                    cx.notify();
                }
            }),
        )
        .icon(Glyph::MESSAGES),
    ));
    let hire = project_id.clone().unwrap_or_default();
    items.push(MenuItem::Choice(
        MenuChoice::new("hire", "Hire an employee…", std::rc::Rc::new(move |root: &mut Root, cx: &mut Context<Root>| {
            root.open_team(crate::ui::root::TeamTab::Hire, Some(hire.clone()), cx);
        }))
        .icon(Glyph::USER_PLUS),
    ));
    let ask = project_id.unwrap_or_default();
    items.push(MenuItem::Choice(
        MenuChoice::new("ask-ceo", "Ask the CEO who to hire", std::rc::Rc::new(move |root: &mut Root, cx: &mut Context<Root>| {
            root.ask_ceo_to_staff(ask.clone(), cx);
            let _ = &project_name;
        }))
        .icon(Glyph::BELL),
    ));
    items
}

impl Root {

    #[allow(clippy::too_many_arguments)]
    fn render_employee(
        &mut self,
        colors: Colors,
        tokens: DensityTokens,
        window: &mut Window,
        cx: &mut Context<Self>,
        agent: &Agent,
        recent: &[Session],
        holds_selection: bool,
        expanded: bool,
        card_closing: bool,
        needs_you: bool,
        working: bool,
        row_key: &str,
        hovered: bool,
        cards_height: f32,
    ) -> AnyElement {
        let latest = recent.first();
        let provider = self.provider_of(agent);
        let mark = Mark::for_agent(
            // 28 px in the tree, whatever the density; the row itself gives the room.
            28.0,
            art::agent_color(&agent.color),
            agent.icon(),
            provider.as_deref(),
            agent.name.clone(),
            colors,
        )
        .render();
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
        let show_more = recent.len() > 1;
        let hover_key = row_key.to_owned();

        let mut row = div()
            .flex()
            .flex_col()
            .rounded(px(theme::RADIUS_MD))
            .when(holds_selection, |this| this.bg(colors.selected))
            .child(
                div()
                    .id(SharedString::from(format!("employee-{row_key}")))
                    .h(px(tokens.person_h))
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .rounded(px(theme::RADIUS_MD))
                    .cursor_pointer()
                    .when(!holds_selection, |this| this.hover(move |style| style.bg(colors.hover)))
                    .active(move |style| style.bg(colors.selected_strong))
                    .on_hover(cx.listener(move |root, is_hovered: &bool, _window, cx| {
                        root.set_hovered(is_hovered.then(|| hover_key.clone()), cx);
                    }))
                    .child(mark)
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .flex()
                            .flex_col()
                            .gap(px(1.0))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(8.0))
                                    .child(
                                        motion::marquee(
                                            SharedString::from(format!("name-{row_key}")),
                                            SharedString::from(agent.name.clone()),
                                            px(theme::FS_SM),
                                            NAME_WIDTH,
                                            holds_selection,
                                            hovered,
                                            colors,
                                            window,
                                        ),
                                    )
                                    .child(match status {
                                        Some(status) => status_element(status, colors, &format!("status-{row_key}")),
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
                    .on_mouse_down(gpui::MouseButton::Left, cx.listener(move |root, _: &gpui::MouseDownEvent, _window, cx| {
                        match &latest_id {
                            Some(session_id) => {
                                if !expanded {
                                    root.set_employee_fold(row_key_owned.clone(), true, cx);
                                }
                                root.open_conversation(agent_id.clone(), session_id.clone(), cx);
                            }
                            None => root.new_conversation(agent_id.clone(), None, cx),
                        }
                    })),
            );

        if show_more {
            let fold_key = row_key.to_owned();
            row = row.child(
                div()
                    .id(SharedString::from(format!("employee-more-{row_key}")))
                    .ml(px(38.0))
                    .h(px(16.0))
                    .text_size(px(theme::FS_2XS))
                    .text_color(colors.text_3)
                    .cursor_pointer()
                    .hover(move |style| style.text_color(colors.text_1))
                    .child(if expanded { "Hide conversations" } else { "Show conversations" })
                    .on_click(cx.listener(move |root, _: &gpui::ClickEvent, _window, cx| {
                        // The owner's choice wins from now on, so it is written
                        // even when it agrees with what the selection implied.
                        root.set_employee_fold(fold_key.clone(), !expanded, cx);
                    })),
            );
        }

        if expanded || card_closing {
            let approvals = self.approval_session_ids();
            let selected = self.session_id.clone();
            let mut cards = div().flex().flex_col().gap(px(tokens.row_gap)).pl(px(26.0)).py(px(tokens.nest_gap));
            for session in recent {
                let is_selected = Some(&session.id) == selected.as_ref();
                let status = conversation_status(session, approvals.contains(&session.id));
                let card_agent = agent.id.clone();
                let card_session = session.id.clone();
                let card_key = format!("card-{}", session.id);
                cards = cards.child(
                    div()
                        .id(SharedString::from(format!("conversation-{}", session.id)))
                        .w_full()
                        .flex()
                        .flex_col()
                        .gap(px(2.0))
                        .px(px(10.0))
                        .py(px(tokens.card_pad_y))
                        .rounded(px(theme::RADIUS_MD))
                        .text_color(if is_selected { colors.text_1 } else { colors.text_2 })
                        .when(is_selected, |this| this.bg(colors.selected))
                        .when(!is_selected, |this| this.hover(move |style| style.bg(colors.hover).text_color(colors.text_1)))
                        .child(motion::marquee(
                            SharedString::from(format!("title-{}", session.id)),
                            SharedString::from(session.title_or_default().to_owned()),
                            px(theme::FS_SM),
                            TITLE_WIDTH,
                            is_selected,
                            hovered,
                            colors,
                            window,
                        ))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(5.0))
                                .text_size(px(theme::FS_2XS))
                                .text_color(colors.text_3)
                                .child(SharedString::from(short_time(session.stamp())))
                                .child(div().child("·"))
                                .child(status_element(status, colors, &card_key)),
                        )
                        .on_mouse_down(gpui::MouseButton::Left, cx.listener(move |root, _: &gpui::MouseDownEvent, _window, cx| {
                            root.open_conversation(card_agent.clone(), card_session.clone(), cx);
                        })),
                );
            }
            row = row.child(collapsible(
                format!("employee-cards-{row_key}"),
                expanded,
                card_closing,
                cards_height.max(1.0),
                cards.into_any_element(),
            ));
        }
        row.into_any_element()
    }

    fn render_team_list(&mut self, colors: Colors, tokens: DensityTokens, cx: &mut Context<Self>) -> AnyElement {
        let employees: Vec<Agent> = self.employees().into_iter().cloned().collect();
        let approvals = self.approval_session_ids();
        let mut list = div().flex().flex_col().gap(px(tokens.row_gap)).p(px(6.0));
        if employees.is_empty() {
            list = list.child(div().px(px(6.0)).py(px(6.0)).text_size(px(theme::FS_XS)).text_color(colors.text_3).child("No employees yet"));
        }
        for agent in &employees {
            let provider = self.provider_of(agent);
            let mark = Mark::for_agent(24.0, art::agent_color(&agent.color), agent.icon(), provider.as_deref(), agent.name.clone(), colors).render();
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
            let status_key = format!("roster-{}", agent.id);
            let menu_agent = agent.id.clone();
            let menu_name = agent.name.clone();
            list = list.child(
                div()
                    .id(SharedString::from(format!("team-{}", agent.id)))
                    .h(px(tokens.team_row_h))
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .rounded(px(theme::RADIUS_MD))
                    .cursor_pointer()
                    .hover(move |style| style.bg(colors.hover))
                    .child(mark)
                    .child(div().flex_1().truncate().text_size(px(theme::FS_SM)).child(SharedString::from(agent.name.clone())))
                    .child(match status {
                        Some(status) => status_element(status, colors, &status_key),
                        None => div()
                            .text_size(px(theme::FS_2XS))
                            .text_color(colors.text_3)
                            .child("Idle")
                            .into_any_element(),
                    })
                    .child(
                        div()
                            .id(SharedString::from(format!("roster-more-{}", agent.id)))
                            .size(px(20.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(4.0))
                            .cursor_pointer()
                            .hover(move |style| style.bg(colors.selected))
                            .child(art::glyph(Glyph::MORE, 11.0, colors.text_3))
                            .on_mouse_down(gpui::MouseButton::Left, cx.listener(|_, _: &gpui::MouseDownEvent, _window, cx| {
                                // The press belongs to the button, not to the row
                                // that would select the bot.
                                cx.stop_propagation();
                            }))
                            .on_click(cx.listener(move |root, event: &gpui::ClickEvent, window, cx| {
                                let at = root.click_position(event, window);
                                let items = crate::ui::team::bot_menu_items(menu_agent.clone(), menu_name.clone());
                                root.open_menu(Menu::new(format!("roster-{menu_agent}"), at, items).width(210.0), cx);
                            })),
                    )
                    .on_mouse_down(gpui::MouseButton::Left, cx.listener(move |root, _: &gpui::MouseDownEvent, _window, cx| {
                        root.select_agent(agent_id.clone(), cx)
                    })),
            );
        }
        list.into_any_element()
    }
}

/// A status in words, with a bell for *Needs you*, an alert for an error and the
/// shimmer on *Working*. Colour only when something is happening.
pub(crate) fn status_element(status: ConversationStatus, colors: Colors, id: &str) -> AnyElement {
    let color = match status.tone {
        Tone::Needs => colors.status_working,
        Tone::Working => colors.status_working,
        Tone::Limited => colors.text_2,
        Tone::Bad => colors.status_danger,
        Tone::Idle => colors.text_3,
    };
    let mut element = div()
        .flex()
        .flex_none()
        .items_center()
        .gap(px(4.0))
        .text_size(px(theme::FS_2XS))
        .text_color(color)
        .when(status.tone == Tone::Needs, |this| this.font_weight(gpui::FontWeight::MEDIUM));
    match status.tone {
        Tone::Needs => element = element.child(art::glyph(Glyph::BELL, 10.0, color)).child(SharedString::from(status.label)),
        Tone::Bad => element = element.child(art::glyph(Glyph::ALERT, 10.0, color)).child(SharedString::from(status.label)),
        Tone::Working => element = element.child(motion::working_word(SharedString::from(format!("shimmer-{id}")), colors)),
        _ => element = element.child(SharedString::from(status.label)),
    }
    element.into_any_element()
}

/// A box that grows or shrinks to `height` over 180 ms, and is instant when
/// Windows is not showing animations. While open it stops clipping as soon as
/// the animation finishes, so a height that is a little short never truncates
/// the rows for good.
fn collapsible(id: String, open: bool, closing: bool, height: f32, content: AnyElement) -> AnyElement {
    if !motion::animations_enabled() {
        return if open { content } else { div().into_any_element() };
    }
    if open {
        let element = div()
            .w_full()
            .overflow_hidden()
            .child(content)
            .with_animation(
                format!("{id}:open"),
                Animation::new(Duration::from_millis(motion::COLLAPSE_MS)),
                move |element, delta| {
                    if delta >= 0.999 {
                        element
                    } else {
                        element.max_h(px(height * motion::ease_out(delta)))
                    }
                },
            );
        return element.into_any_element();
    }
    if closing {
        let element = div()
            .w_full()
            .overflow_hidden()
            .child(content)
            .with_animation(
                format!("{id}:close"),
                Animation::new(Duration::from_millis(motion::COLLAPSE_MS)),
                move |element, delta| element.max_h(px(height * (1.0 - motion::ease_out(delta)))),
            );
        return element.into_any_element();
    }
    div().into_any_element()
}

/// The fold arrow: open points down, closed points right.
pub(crate) fn fold_chevron(open: bool, closing: bool, colors: Colors) -> AnyElement {
    let _ = closing;
    let glyph = if open { Glyph::CHEVRON_DOWN } else { Glyph::CHEVRON };
    div()
        .w(px(12.0))
        .h(px(12.0))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .child(art::glyph(glyph, 12.0, colors.text_3))
        .into_any_element()
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
