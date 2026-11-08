//! The dialogs: a new project, who should staff it, the hire dialog and the one
//! confirmation the Settings page asks for.
//!
//! They are drawn as one overlay over the window (`team-dialog-backdrop` in
//! `teamPage.css`), so the sidebar and the conversation stay where they are.

use gpui::{div, prelude::*, px, AnyElement, Context, FontWeight, SharedString};

use crate::art::{self, Glyph, Mark};
use crate::theme::{self, Colors};
use crate::ui::root::Root;

impl Root {
    pub(crate) fn render_dialogs(&mut self, colors: Colors, cx: &mut Context<Self>) -> Option<AnyElement> {
        let card = if self.hire.is_some() {
            Some(self.render_hire_dialog(colors, cx))
        } else if self.new_project.is_some() {
            Some(self.render_new_project_dialog(colors, cx))
        } else if self.staff.is_some() {
            Some(self.render_staff_dialog(colors, cx))
        } else if self.pending_model.is_some() {
            Some(self.render_model_change_dialog(colors, cx))
        } else if self.confirm_bypass.is_some() {
            Some(self.render_bypass_dialog(colors, cx))
        } else if self.confirm.is_some() {
            Some(self.render_confirm(colors, cx))
        } else {
            None
        }?;
        // A click on the backdrop closes the dialog; the card stops its own.
        Some(
            div()
                .id("dialog-backdrop")
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .bg(colors.scrim)
                .on_mouse_down(gpui::MouseButton::Left, cx.listener(|root, _: &gpui::MouseDownEvent, _window, cx| {
                    root.close_top_dialog(cx);
                }))
                .child(
                    div()
                        .on_mouse_down(gpui::MouseButton::Left, cx.listener(|_, _: &gpui::MouseDownEvent, _window, cx| {
                            // A click inside the card is not a click on the backdrop.
                            cx.stop_propagation();
                        }))
                        .child(card),
                )
                .into_any_element(),
        )
    }

    fn dialog_shell(&self, width: f32, colors: Colors, cx: &mut Context<Self>, body: AnyElement) -> AnyElement {
        div()
            .id("dialog-card")
            .w(px(width))
            .max_h(px(560.0))
            .overflow_y_scroll()
            .p(px(16.0))
            .flex()
            .flex_col()
            .gap(px(14.0))
            .rounded(px(theme::RADIUS_LG))
            .bg(colors.overlay)
            .border_1()
            .border_color(colors.line_strong)
            // Clicking the card itself must not close it, as the backdrop does.
            .on_click(cx.listener(|_root, _: &gpui::ClickEvent, _window, _cx| {}))
            .child(body)
            .into_any_element()
    }

    fn dialog_head(&self, title: &str, subtitle: &str, colors: Colors, cx: &mut Context<Self>, close: CloseKind) -> AnyElement {
        let title = title.to_owned();
        let subtitle = subtitle.to_owned();
        div()
            .flex()
            .items_center()
            .gap(px(12.0))
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(div().text_size(px(theme::FS_MD)).font_weight(FontWeight::SEMIBOLD).child(SharedString::from(title)))
                    .when(!subtitle.is_empty(), |this| {
                        this.child(div().text_size(px(theme::FS_SM)).text_color(colors.text_2).child(SharedString::from(subtitle)))
                    }),
            )
            .child(
                div()
                    .id("dialog-close")
                    .size(px(24.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(theme::RADIUS_SM))
                    .cursor_pointer()
                    .hover(move |style| style.bg(colors.hover))
                    .child(art::glyph(Glyph::X, 14.0, colors.text_3))
                    .on_click(cx.listener(move |root, _: &gpui::ClickEvent, _window, cx| match close {
                        CloseKind::Hire => root.close_hire(cx),
                        CloseKind::NewProject => {
                            root.new_project = None;
                            cx.notify();
                        }
                        CloseKind::Staff => {
                            root.staff = None;
                            cx.notify();
                        }
                        CloseKind::Confirm => root.close_confirm(cx),
                        CloseKind::ModelChange => {
                            root.pending_model = None;
                            cx.notify();
                        }
                        CloseKind::Bypass => {
                            root.confirm_bypass = None;
                            cx.notify();
                        }
                    })),
            )
            .into_any_element()
    }

    // ---- new project ---------------------------------------------------------

    fn render_new_project_dialog(&mut self, colors: Colors, cx: &mut Context<Self>) -> AnyElement {
        let (folder, error, busy) = {
            let Some(dialog) = self.new_project.as_ref() else { return div().into_any_element() };
            (dialog.folder.clone(), dialog.error.clone(), dialog.busy)
        };
        let folder_line = folder.clone().unwrap_or_else(|| "A new folder — choose where it goes".to_owned());
        let choose_label = if folder.is_some() { "Choose another folder" } else { "Choose a folder" };
        let head = self.dialog_head("New project", "A name, and the folder its conversations work in.", colors, cx, CloseKind::NewProject);
        let choose = div()
            .id("choose-folder")
            .h(px(28.0))
            .px(px(10.0))
            .flex()
            .items_center()
            .gap(px(6.0))
            .rounded(px(theme::RADIUS_SM))
            .bg(colors.raised)
            .text_color(colors.text_1)
            .text_size(px(theme::FS_XS))
            .cursor_pointer()
            .hover(move |style| style.bg(colors.selected))
            .child(art::glyph(Glyph::PLUS, 12.0, colors.text_2))
            .child(SharedString::from(choose_label))
            .on_click(cx.listener(|root, _: &gpui::ClickEvent, _window, cx| root.choose_folder(cx)));
        let create_label = if busy { "Creating…" } else { "Create project" };
        let body = div()
            .flex()
            .flex_col()
            .gap(px(10.0))
            .child(head)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(div().text_size(px(theme::FS_XS)).text_color(colors.text_2).child("Name"))
                    .child(
                        div()
                            .h(px(34.0))
                            .px(px(10.0))
                            .flex()
                            .items_center()
                            .rounded(px(theme::RADIUS_SM))
                            .bg(colors.input)
                            .border_1()
                            .border_color(colors.line)
                            .child(self.project_name.clone()),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(div().text_size(px(theme::FS_XS)).text_color(colors.text_2).child("Folder"))
                    .child(
                        div()
                            .text_size(px(theme::FS_XS))
                            .font_family(theme::MONO_FONT)
                            .text_color(colors.text_2)
                            .child(SharedString::from(folder_line)),
                    )
                    .child(choose),
            )
            .when_some(error, |this, error| {
                this.child(div().text_size(px(theme::FS_XS)).text_color(colors.status_danger).child(SharedString::from(error)))
            })
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.0))
                    .child(self.ghost_button("Cancel", "new-project-cancel", colors, cx, |root, cx| {
                        root.new_project = None;
                        cx.notify();
                    }))
                    .child(self.small_primary(create_label, "new-project-create", colors, cx, |root, cx| {
                        root.create_project(cx)
                    })),
            );
        self.dialog_shell(430.0, colors, cx, body.into_any_element())
    }

    // ---- who should work on it? ------------------------------------------------

    fn render_staff_dialog(&mut self, colors: Colors, cx: &mut Context<Self>) -> AnyElement {
        let Some(staff) = self.staff.as_ref() else { return div().into_any_element() };
        let name = staff.project_name.clone();
        let project_id = staff.project_id.clone();
        let ceo = self
            .snapshot()
            .and_then(|snapshot| snapshot.manager())
            .map(|agent| agent.name.clone())
            .unwrap_or_else(|| "the CEO".to_owned());
        let can_ask = self.snapshot().and_then(|snapshot| snapshot.manager()).is_some();
        let head = self.dialog_head(
            &format!("Who should work on {name}?"),
            "You can change the team any time.",
            colors,
            cx,
            CloseKind::Staff,
        );
        let ask_id = project_id.clone();
        let hire_id = project_id.clone();
        let body = div()
            .flex()
            .flex_col()
            .gap(px(10.0))
            .child(head)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(self.staff_option(
                        Glyph::MESSAGES,
                        &format!("Interview with {ceo}"),
                        &format!("Explain the project; {ceo} suggests who to hire."),
                        can_ask,
                        colors,
                        cx,
                        move |root, cx| root.ask_ceo_to_staff(ask_id.clone(), cx),
                    ))
                    .child(self.staff_option(
                        Glyph::USER_PLUS,
                        "I'll hire myself",
                        "Pick employees from the list of specialists.",
                        true,
                        colors,
                        cx,
                        move |root, cx| root.open_hire(None, None, Some(hire_id.clone()), cx),
                    )),
            )
            .child(
                div()
                    .flex()
                    .justify_end()
                    .child(self.ghost_button("Not now", "staff-skip", colors, cx, |root, cx| {
                        root.staff = None;
                        cx.notify();
                    })),
            );
        self.dialog_shell(430.0, colors, cx, body.into_any_element())
    }

    #[allow(clippy::too_many_arguments)]
    fn staff_option(
        &self,
        glyph: Glyph,
        title: &str,
        hint: &str,
        enabled: bool,
        colors: Colors,
        cx: &mut Context<Self>,
        action: impl Fn(&mut Root, &mut Context<Root>) + 'static,
    ) -> AnyElement {
        let title = title.to_owned();
        let hint = hint.to_owned();
        div()
            .id(SharedString::from(format!("staff-{title}")))
            .p(px(12.0))
            .flex()
            .items_start()
            .gap(px(12.0))
            .rounded(px(theme::RADIUS_MD))
            .border_1()
            .border_color(colors.line)
            .when(!enabled, |this| this.opacity(0.55))
            .when(enabled, |this| {
                this.cursor_pointer().hover(move |style| style.bg(colors.hover).border_color(colors.line_strong))
            })
            .child(art::glyph(glyph, 18.0, colors.text_2))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(div().text_size(px(theme::FS_SM)).font_weight(FontWeight::MEDIUM).child(SharedString::from(title.clone())))
                    .child(div().text_size(px(theme::FS_XS)).text_color(colors.text_3).child(SharedString::from(hint.clone()))),
            )
            .when(enabled, |this| this.on_click(cx.listener(move |root, _: &gpui::ClickEvent, _window, cx| action(root, cx))))
            .into_any_element()
    }

    // ---- hire ------------------------------------------------------------------

    fn render_hire_dialog(&mut self, colors: Colors, cx: &mut Context<Self>) -> AnyElement {
        let Some(dialog) = self.hire.as_ref() else { return div().into_any_element() };
        let name = dialog.name.clone();
        let role = dialog.role.clone();
        let portrait = dialog.portrait.clone();
        let color = dialog.color.clone();
        let chosen: Vec<String> = dialog.projects.iter().cloned().collect();
        let runtime_id = dialog.runtime_id.clone();
        let model = dialog.model.clone();
        let busy = dialog.busy;
        let error = dialog.error.clone();
        let is_agent = dialog.agent_id.is_some();

        let projects = self.snapshot().map(|snapshot| snapshot.projects.clone()).unwrap_or_default();
        let already: Vec<String> = dialog
            .agent_id
            .as_ref()
            .map(|agent_id| self.projects_for(agent_id))
            .unwrap_or_default();
        let runtimes: Vec<(String, String)> = self
            .snapshot()
            .map(|snapshot| {
                snapshot
                    .usable_runtimes()
                    .into_iter()
                    .map(|runtime| {
                        let label = runtime
                            .display_name
                            .clone()
                            .filter(|label| !label.is_empty())
                            .unwrap_or_else(|| runtime.label());
                        (runtime.id.clone(), label)
                    })
                    .collect()
            })
            .unwrap_or_default();
        let models = self.models.get(&runtime_id).cloned().unwrap_or_default();

        let mark = Mark::new(40.0, art::agent_color(&color), art::Icon::resolve(Some(&portrait)), name.clone(), colors).render();
        let head = self.dialog_head(&format!("Hire {name}"), &role, colors, cx, CloseKind::Hire);

        let mut project_rows = div().flex().flex_col().gap(px(2.0));
        for project in &projects {
            let inside = already.iter().any(|id| id == &project.id);
            let selected = inside || chosen.iter().any(|id| id == &project.id);
            let project_id = project.id.clone();
            project_rows = project_rows.child(
                div()
                    .id(SharedString::from(format!("hire-project-{}", project.id)))
                    .h(px(32.0))
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .rounded(px(theme::RADIUS_SM))
                    .when(!inside, |this| this.cursor_pointer().hover(move |style| style.bg(colors.hover)))
                    .child(checkbox(selected, colors))
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(theme::FS_SM))
                            .text_color(if inside { colors.text_3 } else { colors.text_1 })
                            .child(SharedString::from(project.name.clone())),
                    )
                    .when(inside, |this| {
                        this.child(div().text_size(px(theme::FS_2XS)).text_color(colors.text_3).child("Already here"))
                    })
                    .when(!inside, |this| {
                        this.on_click(cx.listener(move |root, _: &gpui::ClickEvent, _window, cx| {
                            root.hire_toggle_project(project_id.clone(), cx)
                        }))
                    }),
            );
        }
        if projects.is_empty() {
            project_rows = project_rows.child(
                div()
                    .text_size(px(theme::FS_XS))
                    .text_color(colors.text_3)
                    .child("Create a project first."),
            );
        }

        let mut runtime_rows = div().flex().flex_wrap().gap(px(6.0));
        if runtimes.len() > 1 {
            for (id, label) in &runtimes {
                let id = id.clone();
                runtime_rows = runtime_rows.child(self.chip(
                    label,
                    runtime_id == *id || (runtime_id.is_empty() && runtimes.first().map(|(first, _)| first) == Some(&id)),
                    &format!("hire-runtime-{id}"),
                    colors,
                    cx,
                    move |root, cx| root.hire_set_runtime(id.clone(), cx),
                ));
            }
        }
        let mut model_rows = div().flex().flex_wrap().gap(px(6.0));
        model_rows = model_rows.child(self.chip("Agent default", model.is_none(), "hire-model-default", colors, cx, |root, cx| {
            root.hire_set_model(None, cx)
        }));
        for choice in models.iter().take(6) {
            let id = choice.id.clone();
            model_rows = model_rows.child(self.chip(
                &choice.label,
                model.as_deref() == Some(id.as_str()),
                &format!("hire-model-{id}"),
                colors,
                cx,
                move |root, cx| root.hire_set_model(Some(id.clone()), cx),
            ));
        }

        let count = chosen.len();
        let confirm_label = if busy {
            "Hiring…".to_owned()
        } else if count > 1 {
            format!("Hire into {count} projects")
        } else {
            "Hire".to_owned()
        };
        let body = div()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(head)
            .child(
                div()
                    .flex()
                    .gap(px(10.0))
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
                                    .text_size(px(theme::FS_SM))
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(SharedString::from(name.clone())),
                            )
                            .child(div().text_size(px(theme::FS_XS)).text_color(colors.text_2).child(SharedString::from(role.clone()))),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.0))
                    .child(div().text_size(px(theme::FS_XS)).text_color(colors.text_2).child("Into which projects?"))
                    .child(project_rows),
            )
            .when(!is_agent && runtimes.len() > 1, |this| {
                this.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(6.0))
                        .child(div().text_size(px(theme::FS_XS)).text_color(colors.text_2).child("Coding agent"))
                        .child(runtime_rows),
                )
            })
            .when(!is_agent, |this| {
                this.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(6.0))
                        .child(div().text_size(px(theme::FS_XS)).text_color(colors.text_2).child("Model"))
                        .child(model_rows),
                )
            })
            .when(!is_agent, |this| {
                this.child(
                    div()
                        .text_size(px(theme::FS_XS))
                        .text_color(colors.text_3)
                        .child(SharedString::from(format!(
                            "{name} starts with Ask before acting. You can change the model, access and instructions in their settings."
                        ))),
                )
            })
            .when_some(error, |this, error| {
                this.child(div().text_size(px(theme::FS_XS)).text_color(colors.status_danger).child(SharedString::from(error)))
            })
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.0))
                    .child(self.ghost_button("Cancel", "hire-cancel", colors, cx, |root, cx| root.close_hire(cx)))
                    .child(self.small_primary(&confirm_label, "hire-confirm", colors, cx, |root, cx| root.hire_confirm(cx))),
            );
        self.dialog_shell(420.0, colors, cx, body.into_any_element())
    }

    fn chip(
        &self,
        label: &str,
        active: bool,
        id: &str,
        colors: Colors,
        cx: &mut Context<Self>,
        action: impl Fn(&mut Root, &mut Context<Root>) + 'static,
    ) -> AnyElement {
        div()
            .id(SharedString::from(format!("chip-{id}")))
            .h(px(24.0))
            .px(px(9.0))
            .flex()
            .items_center()
            .rounded(px(theme::RADIUS_SM))
            .bg(if active { colors.selected } else { colors.raised })
            .text_color(if active { colors.text_1 } else { colors.text_2 })
            .text_size(px(theme::FS_XS))
            .cursor_pointer()
            .hover(move |style| style.bg(colors.selected))
            .child(SharedString::from(label.to_owned()))
            .on_click(cx.listener(move |root, _: &gpui::ClickEvent, _window, cx| action(root, cx)))
            .into_any_element()
    }

    // ---- one confirmation ------------------------------------------------------

    /// "Switch model and start fresh?": the daemon's own reason, then what stays.
    fn render_model_change_dialog(&mut self, colors: Colors, cx: &mut Context<Self>) -> AnyElement {
        let Some(pending) = self.pending_model.as_ref() else { return div().into_any_element() };
        let body_text = if pending.context_at_risk {
            crate::ui::composer::model_change_body(Some(&pending.reason))
        } else {
            "This conversation has no working memory to lose yet, so the new model starts here cleanly.".to_owned()
        };
        let model = pending.model.clone();
        let head = self.dialog_head("Switch model and start fresh?", "", colors, cx, CloseKind::ModelChange);
        let body = div()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(head)
            .child(
                div()
                    .text_size(px(theme::FS_XS))
                    .text_color(colors.text_2)
                    .child(SharedString::from(format!("New model: {model}"))),
            )
            .child(div().text_size(px(theme::FS_SM)).text_color(colors.text_2).child(SharedString::from(body_text)))
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.0))
                    .child(self.ghost_button("Keep current model", "model-keep", colors, cx, |root, cx| {
                        root.pending_model = None;
                        cx.notify();
                    }))
                    .child(self.small_primary("Switch model", "model-switch", colors, cx, |root, cx| {
                        let model = root.pending_model.as_ref().map(|pending| pending.model.clone());
                        root.write_model(model, cx);
                    })),
            );
        self.dialog_shell(460.0, colors, cx, body.into_any_element())
    }

    /// "Give this bot full access?": the app's own warning before `bypass`.
    fn render_bypass_dialog(&mut self, colors: Colors, cx: &mut Context<Self>) -> AnyElement {
        let Some(session_id) = self.confirm_bypass.clone() else { return div().into_any_element() };
        let name = self.selected_agent().map(|agent| agent.name.clone()).unwrap_or_else(|| "this bot".to_owned());
        let head = self.dialog_head("Give this bot full access?", "", colors, cx, CloseKind::Bypass);
        let body = div()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(head)
            .child(
                div()
                    .text_size(px(theme::FS_SM))
                    .text_color(colors.text_2)
                    .child(SharedString::from(format!(
                        "The agent will run commands and change files without asking you. Cancel and Stop still stop it. This is {name}'s own setting, so every conversation is affected."
                    ))),
            )
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.0))
                    .child(self.ghost_button("Keep asking", "bypass-cancel", colors, cx, |root, cx| {
                        root.confirm_bypass = None;
                        cx.notify();
                    }))
                    .child(self.small_primary("Allow full access", "bypass-allow", colors, cx, move |root, cx| {
                        root.confirm_bypass = None;
                        let _ = &session_id;
                        root.write_permission("bypass".to_owned(), cx);
                    })),
            );
        self.dialog_shell(460.0, colors, cx, body.into_any_element())
    }


    fn render_confirm(&mut self, colors: Colors, cx: &mut Context<Self>) -> AnyElement {
        let Some(confirm) = self.confirm.as_ref() else { return div().into_any_element() };
        let title = confirm.title.clone();
        let body_text = confirm.body.clone();
        let confirm_label = confirm.confirm_label.clone();
        let head = self.dialog_head(&title, "", colors, cx, CloseKind::Confirm);
        let body = div()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(head)
            .child(div().text_size(px(theme::FS_SM)).text_color(colors.text_2).child(SharedString::from(body_text)))
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.0))
                    .child(self.ghost_button("Cancel", "confirm-cancel", colors, cx, |root, cx| root.close_confirm(cx)))
                    .child(self.small_primary(&confirm_label, "confirm-ok", colors, cx, |root, cx| root.run_confirm(cx))),
            );
        self.dialog_shell(440.0, colors, cx, body.into_any_element())
    }
}

#[derive(Clone, Copy)]
pub enum CloseKind {
    Hire,
    NewProject,
    Staff,
    Confirm,
    ModelChange,
    Bypass,
}

fn checkbox(checked: bool, colors: Colors) -> AnyElement {
    div()
        .size(px(15.0))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(4.0))
        .border_1()
        .border_color(if checked { colors.text_1 } else { colors.line_strong })
        .when(checked, |this| this.bg(colors.text_1))
        .when(checked, |this| this.child(art::glyph(Glyph::CHECK, 10.0, colors.base)))
        .into_any_element()
}
