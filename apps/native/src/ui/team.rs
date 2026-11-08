//! The Team page, ported from `ui/TeamPage.tsx` and `ui/teamPage.css`.
//!
//! "Your team" lists every employee with the projects they are hired into;
//! "Hire" is the preset catalogue by department, with a search. Hiring goes
//! through the daemon's `project.hire`, releasing through `project.release`, and
//! the page follows `project.members.changed`.

use gpui::{div, prelude::*, px, AnyElement, Context, FontWeight, SharedString, Window};

use crate::art::{self, Glyph, Mark};
use crate::state::team;
use crate::state::Agent;
use crate::theme::{self, Colors};
use crate::ui::root::{Root, TeamTab};

impl Root {
    pub(crate) fn render_team_page(&mut self, colors: Colors, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let employees = self.employees().into_iter().cloned().collect::<Vec<Agent>>();
        let tab = self.team_tab;
        let query = self.team_search.read(cx).text().to_owned();

        let header = {
            let back = div()
                .id("team-back")
                .size(px(28.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(theme::RADIUS_SM))
                .cursor_pointer()
                .hover(move |style| style.bg(colors.hover))
                .child(art::glyph(Glyph::ARROW_LEFT, 15.0, colors.text_2))
                .on_click(cx.listener(|root, _: &gpui::ClickEvent, _window, cx| root.back_to_conversation(cx)));
            let team_tab = self.team_tab_button(TeamTab::Team, employees.len(), colors, cx);
            let hire_tab = self.team_tab_button(TeamTab::Hire, 0, colors, cx);
            div()
                .h(px(46.0))
                .flex_none()
                .px(px(12.0))
                .flex()
                .items_center()
                .gap(px(8.0))
                .border_b_1()
                .border_color(colors.line)
                .child(back)
                .child(div().text_size(px(theme::FS_LG)).font_weight(FontWeight::SEMIBOLD).child("Team"))
                .child(div().flex_1())
                .child(
                    div()
                        .mr(px(6.0))
                        .p(px(2.0))
                        .flex()
                        .gap(px(2.0))
                        .rounded(px(theme::RADIUS_MD))
                        .bg(colors.hover)
                        .child(team_tab)
                        .child(hire_tab),
                )
        };

        let search = div()
            .h(px(36.0))
            .px(px(10.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .rounded(px(theme::RADIUS_MD))
            .bg(colors.input)
            .border_1()
            .border_color(colors.line)
            .child(art::glyph(Glyph::SEARCH, 14.0, colors.text_3))
            .child(div().flex_1().min_w(px(0.0)).child(self.team_search.clone()));

        let mut column = div()
            .max_w(px(960.0))
            .mx_auto()
            .flex()
            .flex_col()
            .gap(px(18.0))
            .px(px(24.0))
            .py(px(18.0))
            .child(search);
        column = column.child(match tab {
            TeamTab::Team => self.render_roster(&employees, &query, colors, cx),
            TeamTab::Hire => self.render_catalogue(&query, colors, window, cx),
        });

        div()
            .flex_1()
            .h_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(header)
            .child(
                div()
                    .id("team-scroll")
                    .flex_1()
                    .w_full()
                    .overflow_y_scroll()
                    .child(div().id("team-scroll").flex_1().w_full().overflow_y_scroll().child(column)),
            )
            .into_any_element()
    }

    fn team_tab_button(&mut self, tab: TeamTab, count: usize, colors: Colors, cx: &mut Context<Self>) -> AnyElement {
        let active = self.team_tab == tab;
        div()
            .id(SharedString::from(format!("team-tab-{}", tab.label())))
            .h(px(28.0))
            .px(px(12.0))
            .flex()
            .items_center()
            .gap(px(6.0))
            .rounded(px(theme::RADIUS_SM))
            .bg(if active { colors.selected } else { gpui::rgba(0x00000000) })
            .text_color(if active { colors.text_1 } else { colors.text_2 })
            .text_size(px(theme::FS_SM))
            .cursor_pointer()
            .hover(move |style| style.text_color(colors.text_1))
            .child(SharedString::from(tab.label()))
            .when(tab == TeamTab::Team, |this| {
                this.child(
                    div()
                        .text_size(px(theme::FS_2XS))
                        .text_color(colors.text_3)
                        .child(SharedString::from(count.to_string())),
                )
            })
            .on_click(cx.listener(move |root, _: &gpui::ClickEvent, _window, cx| {
                root.team_tab = tab;
                cx.notify();
            }))
            .into_any_element()
    }

    fn render_roster(&mut self, employees: &[Agent], query: &str, colors: Colors, cx: &mut Context<Self>) -> AnyElement {
        let needle = query.trim().to_lowercase();
        let shown: Vec<&Agent> = employees
            .iter()
            .filter(|agent| {
                needle.is_empty()
                    || [agent.name.as_str(), agent.role_or_default(), agent.description.as_str()]
                        .iter()
                        .any(|value| value.to_lowercase().contains(&needle))
            })
            .collect();
        let mut list = div().flex().flex_col().gap(px(8.0));
        if shown.is_empty() {
            list = list.child(
                div()
                    .py(px(24.0))
                    .text_color(colors.text_2)
                    .child(if employees.is_empty() { "No one on your team yet." } else { "No one matches that search." }),
            );
            if employees.is_empty() {
                list = list.child(self.primary_button("Hire your first employee", "hire-first", colors, cx, |root, cx| {
                    root.team_tab = TeamTab::Hire;
                    cx.notify();
                }));
            }
            return list.into_any_element();
        }
        let approvals = self.approval_session_ids();
        for agent in shown {
            let provider = self.provider_of(agent);
            let mark = Mark::for_agent(44.0, art::agent_color(&agent.color), agent.icon(), provider.as_deref(), agent.name.clone(), colors).render();
            let working = self
                .sessions()
                .iter()
                .any(|session| session.agent_id.as_deref() == Some(agent.id.as_str()) && session.is_busy());
            let needs_you = self.employee_needs_you(&agent.id, &approvals);
            let projects = self.projects_for(&agent.id);
            let agent_id = agent.id.clone();
            let agent_name = agent.name.clone();
            let state = if needs_you {
                ("Needs you", colors.status_working)
            } else if working {
                ("Working", colors.status_working)
            } else {
                ("Idle", colors.text_3)
            };

            let mut chips = div().flex().flex_wrap().items_center().gap(px(6.0)).pl(px(56.0));
            for project_id in &projects {
                let name = self
                    .snapshot()
                    .and_then(|snapshot| snapshot.project_name(Some(project_id)))
                    .unwrap_or("Project")
                    .to_owned();
                let release_project = project_id.clone();
                let release_agent = agent.id.clone();
                chips = chips.child(
                    div()
                        .h(px(26.0))
                        .pl(px(8.0))
                        .pr(px(4.0))
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .rounded(px(theme::RADIUS_SM))
                        .border_1()
                        .border_color(colors.line)
                        .text_size(px(theme::FS_XS))
                        .text_color(colors.text_2)
                        .child(SharedString::from(name))
                        .child(
                            div()
                                .id(SharedString::from(format!("release-{}-{}", agent.id, project_id)))
                                .size(px(18.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(theme::RADIUS_SM))
                                .cursor_pointer()
                                .hover(move |style| style.bg(colors.hover))
                                .child(art::glyph(Glyph::X, 10.0, colors.text_3))
                                .on_click(cx.listener(move |root, _: &gpui::ClickEvent, _window, cx| {
                                    root.release(release_project.clone(), release_agent.clone(), cx);
                                })),
                        ),
                );
            }
            let hire_agent = agent.id.clone();
            chips = chips.child(
                div()
                    .id(SharedString::from(format!("hire-into-{}", agent.id)))
                    .h(px(26.0))
                    .px(px(9.0))
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .rounded(px(theme::RADIUS_SM))
                    .border_dashed()
                    .border_1()
                    .border_color(colors.line)
                    .text_size(px(theme::FS_XS))
                    .text_color(colors.text_2)
                    .cursor_pointer()
                    .hover(move |style| style.bg(colors.hover).text_color(colors.text_1))
                    .child(art::glyph(Glyph::PLUS, 10.0, colors.text_2))
                    .child("Hire into a project")
                    .on_click(cx.listener(move |root, _: &gpui::ClickEvent, _window, cx| {
                        root.open_hire(None, Some(hire_agent.clone()), None, cx);
                    })),
            );

            list = list.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .p(px(12.0))
                    .rounded(px(theme::RADIUS_MD))
                    .border_1()
                    .border_color(colors.line)
                    .child(
                        div()
                            .id(SharedString::from(format!("team-person-{}", agent.id)))
                            .flex()
                            .items_center()
                            .gap(px(12.0))
                            .cursor_pointer()
                            .child(mark)
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .flex()
                                    .flex_col()
                                    .gap(px(2.0))
                                    .child(
                                        div()
                                            .truncate()
                                            .text_size(px(theme::FS_MD))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child(SharedString::from(agent_name.clone())),
                                    )
                                    .child(
                                        div()
                                            .truncate()
                                            .text_size(px(theme::FS_SM))
                                            .text_color(colors.text_2)
                                            .child(SharedString::from(agent.role_or_default().to_owned())),
                                    ),
                            )
                            .child(
                                div()
                                    .text_size(px(theme::FS_XS))
                                    .text_color(state.1)
                                    .child(SharedString::from(state.0)),
                            )
                            .on_click(cx.listener(move |root, _: &gpui::ClickEvent, _window, cx| {
                                root.select_agent(agent_id.clone(), cx);
                            })),
                    )
                    .child(chips),
            );
        }
        list.into_any_element()
    }

    fn render_catalogue(&mut self, query: &str, colors: Colors, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let matching = team::search(query);
        let owned: Vec<team::Preset> = matching.iter().map(|preset| (*preset).clone()).collect();
        let groups = team::by_department(&owned);
        let projects_exist = self.snapshot().map(|snapshot| !snapshot.projects.is_empty()).unwrap_or(false);
        let mut list = div().flex().flex_col().gap(px(22.0));
        if groups.is_empty() {
            list = list.child(div().py(px(20.0)).text_color(colors.text_2).child("No one matches that search."));
        }
        for (department, presets) in groups {
            let mut cards = div().flex().flex_wrap().items_stretch().gap(px(10.0));
            for preset in presets {
                cards = cards.child(self.render_preset_card(preset, projects_exist, colors, window, cx));
            }
            list = list.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(10.0))
                    .child(
                        div()
                            .text_size(px(theme::FS_XS))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(colors.text_2)
                            .child(SharedString::from(department.name.clone())),
                    )
                    .child(cards),
            );
        }
        list.child(
            div()
                .flex()
                .flex_col()
                .gap(px(10.0))
                .child(
                    div()
                        .text_size(px(theme::FS_XS))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(colors.text_2)
                        .child("Someone else"),
                )
                .child(
                    div()
                        .id("create-own")
                        .w(px(208.0))
                        .h(px(120.0))
                        .flex()
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .gap(px(6.0))
                        .rounded(px(theme::RADIUS_MD))
                        .border_1()
                        .border_dashed()
                        .border_color(colors.line)
                        .text_color(colors.text_2)
                        .cursor_pointer()
                        .hover(move |style| style.bg(colors.hover))
                        .child(art::glyph(Glyph::PLUS, 22.0, colors.text_2))
                        .child(div().font_weight(FontWeight::SEMIBOLD).child("Create your own"))
                        .child(
                            div()
                                .max_w(px(170.0))
                                .text_size(px(theme::FS_XS))
                                .text_color(colors.text_3)
                                .child("Give a new employee a name, a portrait, a role and instructions."),
                        )
                        .on_click(cx.listener(|root, _: &gpui::ClickEvent, _window, cx| {
                            root.notice = Some("Creating your own employee comes in a later slice. Hire a preset for now.".to_owned());
                            cx.notify();
                        })),
                ),
        )
        .into_any_element()
    }

    fn render_preset_card(
        &mut self,
        preset: &team::Preset,
        projects_exist: bool,
        colors: Colors,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let hired = self
            .agents()
            .iter()
            .find(|agent| !agent.archived && agent.preset_id.as_deref() == Some(preset.id.as_str()))
            .map(|agent| agent.id.clone());
        let mark = Mark::new(
            56.0,
            art::agent_color(&preset.color),
            art::Icon::resolve(Some(&preset.portrait)),
            preset.nickname.clone(),
            colors,
        )
        .render();
        let preset_id = preset.id.clone();
        let hired_for_click = hired.clone();
        let enabled = projects_exist;
        let mut tags = div().flex().flex_wrap().gap(px(4.0));
        for tag in preset.tags.iter().take(3) {
            tags = tags.child(
                div()
                    .px(px(6.0))
                    .py(px(1.0))
                    .rounded(px(5.0))
                    .bg(colors.hover)
                    .text_size(px(theme::FS_2XS))
                    .text_color(colors.text_3)
                    .child(SharedString::from(tag.clone())),
            );
        }
        let label = if hired.is_some() { "Hire into…" } else { "Hire" };
        let description = clamp_two_lines(window, &preset.description, theme::FS_XS, CARD_TEXT_WIDTH);
        let copy = div()
            .flex()
            .flex_col()
            .gap(px(2.0))
            .child(
                div()
                    .truncate()
                    .text_size(px(theme::FS_MD))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(SharedString::from(preset.nickname.clone())),
            )
            .child(
                div()
                    .truncate()
                    .text_size(px(theme::FS_SM))
                    .text_color(colors.text_2)
                    .child(SharedString::from(preset.role.clone())),
            )
            .child(
                div()
                    // Two lines, as `-webkit-line-clamp: 2` in `teamPage.css:45`.
                    .mt(px(4.0))
                    .h(px(CARD_DESCRIPTION_HEIGHT))
                    .line_height(px(CARD_DESCRIPTION_HEIGHT / 2.0))
                    .overflow_hidden()
                    .text_size(px(theme::FS_XS))
                    .text_color(colors.text_3)
                    .child(SharedString::from(description)),
            );
        div()
            .w(px(208.0))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .p(px(14.0))
            .rounded(px(theme::RADIUS_MD))
            .border_1()
            .border_color(colors.line)
            .hover(move |style| style.border_color(colors.line_strong).bg(colors.hover))
            .child(div().flex().child(mark))
            // The copy and the tags take the room, so the button sits on the card's
            // bottom edge in every card of a row.
            .child(div().flex_1().flex().flex_col().gap(px(8.0)).child(copy).child(tags))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(8.0))
                    .when(hired.is_some(), |this| {
                        this.child(
                            div()
                                .flex_1()
                                .text_size(px(theme::FS_2XS))
                                .text_color(colors.text_3)
                                .child("On your team"),
                        )
                    })
                    .child(
                        div()
                            .id(SharedString::from(format!("preset-hire-{}", preset.id)))
                            .h(px(26.0))
                            .px(px(10.0))
                            .flex()
                            .items_center()
                            .rounded(px(theme::RADIUS_SM))
                            .bg(if hired.is_some() { colors.raised } else { colors.accent })
                            .text_color(if hired.is_some() { colors.text_1 } else { colors.text_inverse })
                            .text_size(px(theme::FS_XS))
                            .when(!enabled, |this| this.opacity(0.5))
                            .cursor_pointer()
                            .hover(move |style| style.bg(colors.selected_strong))
                            .child(label)
                            .on_click(cx.listener(move |root, _: &gpui::ClickEvent, _window, cx| {
                                if !projects_exist {
                                    root.error = Some("Create a project first; an employee works inside one.".to_owned());
                                    cx.notify();
                                    return;
                                }
                                match &hired_for_click {
                                    Some(agent_id) => root.open_hire(None, Some(agent_id.clone()), root.team_project.clone(), cx),
                                    None => root.open_hire(Some(preset_id.clone()), None, root.team_project.clone(), cx),
                                }
                            })),
                    ),
            )
            .into_any_element()
    }

    pub(crate) fn primary_button(
        &self,
        label: &str,
        id: &str,
        colors: Colors,
        cx: &mut Context<Self>,
        action: impl Fn(&mut Root, &mut Context<Root>) + 'static,
    ) -> AnyElement {
        div()
            .id(SharedString::from(format!("button-{id}")))
            .h(px(28.0))
            .px(px(12.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(theme::RADIUS_SM))
            .bg(colors.accent)
            .text_color(colors.text_inverse)
            .text_size(px(theme::FS_XS))
            .font_weight(FontWeight::MEDIUM)
            .cursor_pointer()
            .hover(move |style| style.bg(colors.selected_strong))
            .child(SharedString::from(label.to_owned()))
            .on_click(cx.listener(move |root, _: &gpui::ClickEvent, _window, cx| action(root, cx)))
            .into_any_element()
    }

    pub(crate) fn ghost_button(
        &self,
        label: &str,
        id: &str,
        colors: Colors,
        cx: &mut Context<Self>,
        action: impl Fn(&mut Root, &mut Context<Root>) + 'static,
    ) -> AnyElement {
        div()
            .id(SharedString::from(format!("ghost-{id}")))
            .h(px(28.0))
            .px(px(12.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(theme::RADIUS_SM))
            .text_color(colors.text_2)
            .text_size(px(theme::FS_XS))
            .cursor_pointer()
            .hover(move |style| style.text_color(colors.text_1).bg(colors.hover))
            .child(SharedString::from(label.to_owned()))
            .on_click(cx.listener(move |root, _: &gpui::ClickEvent, _window, cx| action(root, cx)))
            .into_any_element()
    }
}

/// The Team page's search placeholder, for `Root::new` and the page's header.
pub const SEARCH_PLACEHOLDER: &str = "Search roles, skills or names";

/// A catalogue card is 208 px wide with 14 px of padding either side.
const CARD_TEXT_WIDTH: f32 = 178.0;
/// Two lines of `--fs-xs` text.
const CARD_DESCRIPTION_HEIGHT: f32 = 36.0;

/// Two lines of a description, ending in an ellipsis when there is more.
///
/// GPUI has no `-webkit-line-clamp`, so the text is measured once and cut at a
/// word boundary near the end of the second line.
fn clamp_two_lines(window: &mut Window, text: &str, size: f32, available: f32) -> String {
    let width = crate::ui::motion::text_width(window, text, px(size));
    cut_two_lines(text, width, available)
}

/// The cut itself, from the width the whole line would take.
fn cut_two_lines(text: &str, measured: f32, available: f32) -> String {
    if measured <= available * 2.0 {
        return text.to_owned();
    }
    let counted = text.chars().count();
    let keep = (((available * 2.0) / measured) * counted as f32).floor() as usize;
    let keep = keep.clamp(16, counted.saturating_sub(1));
    let cut: String = text.chars().take(keep).collect();
    let cut = match cut.rfind(' ') {
        Some(index) => cut[..index].to_owned(),
        None => cut,
    };
    format!("{}…", cut.trim_end())
}

#[cfg(test)]
mod tests {
    use super::*;

    const ADA: &str = "Builds and changes server code, APIs and business logic, with tests, and checks every edge case it can find.";

    #[test]
    fn a_long_description_is_cut_at_a_word_boundary_with_an_ellipsis() {
        // Four lines' worth of text in a two-line box.
        let measured = ADA.chars().count() as f32 * 5.5;
        let line = cut_two_lines(ADA, measured, 178.0);
        assert!(line.ends_with('…'), "{line}");
        assert!(line.chars().count() < ADA.chars().count());
        assert!(!line.trim_end_matches('…').ends_with(' '), "no space before the ellipsis");
        // It ends on a whole word: the character before the ellipsis is a letter.
        assert!(line.trim_end_matches('…').chars().last().unwrap().is_alphanumeric(), "{line}");
        assert!(ADA.starts_with(line.trim_end_matches('…')));
    }

    #[test]
    fn a_description_that_fits_is_left_alone() {
        assert_eq!(cut_two_lines("Short and sweet.", 90.0, 178.0), "Short and sweet.");
        // Exactly two lines: no cut.
        assert_eq!(cut_two_lines(ADA, 356.0, 178.0), ADA);
    }
}
