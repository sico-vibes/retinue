//! The task form: every field the daemon lets the owner set on a draft. One form
//! serves "New task" and editing a draft, so the two cannot drift apart.
//! Ported from `ui/TaskForm.tsx`.

use gpui::{div, prelude::*, px, AnyElement, Context, FontWeight, SharedString};

use crate::state::tasks::{estimate_problem, form_problem, TIME_LIMIT_CHOICES};
use crate::theme::{self, Colors};
use crate::ui::root::Root;
use crate::ui::schedules::{SchedulePreset, WEEKDAYS};

impl Root {
    pub(crate) fn render_task_form(&mut self, colors: Colors, cx: &mut Context<Self>) -> AnyElement {
        let Some(draft) = self.tasks.draft.clone() else { return div().into_any_element() };
        let editing = self.tasks.editing.clone();
        let task = editing.as_deref().and_then(|id| self.tasks.task(id)).cloned();
        let problem = form_problem(&draft);
        let bots = self.assignable();
        let projects: Vec<(String, String)> = self
            .snapshot()
            .map(|snapshot| snapshot.projects.iter().map(|project| (project.id.clone(), project.name.clone())).collect())
            .unwrap_or_default();
        let can_start = draft.when == "now" && !draft.bot_id.trim().is_empty();
        let submitting = self.tasks.form_error.is_none() && problem.is_none();

        let field = |label: &str, help: Option<String>, input: gpui::Entity<crate::ui::text_input::TextInput>, height: f32| -> gpui::Div {
            div()
                .flex()
                .flex_col()
                .gap(px(3.0))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .child(div().text_size(px(theme::FS_XS)).text_color(colors.text_3).child(SharedString::from(label.to_owned())))
                        .when_some(help, |this, help| {
                            this.child(div().text_size(px(theme::FS_2XS)).text_color(colors.text_3).child(SharedString::from(help)))
                        }),
                )
                .child(
                    div()
                        .h(px(height))
                        .px(px(8.0))
                        .py(px(4.0))
                        .flex()
                        .rounded(px(theme::RADIUS_SM))
                        .bg(colors.input)
                        .border_1()
                        .border_color(colors.line)
                        .child(div().flex_1().min_w(px(0.0)).child(input)),
                )
        };

        let mut body = div()
            .w(px(600.0))
            .max_h(px(640.0))
            .id("task-form")
            .overflow_y_scroll()
            .p(px(16.0))
            .flex()
            .flex_col()
            .gap(px(9.0))
            .rounded(px(theme::RADIUS_LG))
            .bg(colors.overlay)
            .border_1()
            .border_color(colors.line_strong)
            .child(
                div()
                    .text_size(px(theme::FS_LG))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(if editing.is_some() { "Edit task" } else { "New task" }),
            );

        if let Some(fields) = self.tasks.fields.as_ref() {
            let title = fields.title.clone();
            let brief = fields.brief.clone();
            let done_goal = fields.done_goal.clone();
            let report_back = fields.report_back.clone();
            let estimate = fields.estimate.clone();
            let when_detail = fields.when_detail.clone();
            let brief_help = format!("{}/12000", draft.brief.chars().count());
            body = body
                .child(field("Title", None, title, 32.0))
                .child(field("What needs doing", Some(brief_help), brief, 64.0))
                .child(field("Done when", None, done_goal, 30.0))
                .child(field("Report back", None, report_back, 30.0));

            // Images: the strip, an add button, and one remove per image.
            body = body.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(div().text_size(px(theme::FS_XS)).text_color(colors.text_3).child("Images"))
                    .child(self.render_draft_images(task.as_ref(), colors, cx))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .child(self.ghost_button("Add image…", "task-add-image", colors, cx, |root, cx| {
                                root.choose_task_images(cx);
                            }))
                            .when(!draft.images.is_empty(), |this| {
                                this.child(self.ghost_button("Remove the last", "task-remove-image", colors, cx, |root, cx| {
                                    if let Some(draft) = root.tasks.draft.as_mut() {
                                        draft.images.pop();
                                    }
                                    cx.notify();
                                }))
                            })
                            .child(
                                div()
                                    .text_size(px(theme::FS_2XS))
                                    .text_color(colors.text_3)
                                    .child("Choose, paste or drop up to 8 images."),
                            ),
                    ),
            );

            // Project and bot.
            let mut project_chips = div().flex().flex_wrap().gap(px(4.0));
            project_chips = project_chips.child(self.task_chip("task-project-none", "No project".to_owned(), draft.project_id.is_empty(), colors, cx, {
                move |root: &mut Root, cx: &mut Context<Root>| {
                    if let Some(draft) = root.tasks.draft.as_mut() {
                        draft.project_id.clear();
                    }
                    cx.notify();
                }
            }));
            for (id, name) in &projects {
                let chosen = draft.project_id == *id;
                let project_id = id.clone();
                project_chips = project_chips.child(self.task_chip(
                    &format!("task-project-{id}"),
                    name.clone(),
                    chosen,
                    colors,
                    cx,
                    move |root: &mut Root, cx: &mut Context<Root>| {
                        if let Some(draft) = root.tasks.draft.as_mut() {
                            draft.project_id = project_id.clone();
                        }
                        cx.notify();
                    },
                ));
            }
            body = body.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(3.0))
                    .child(div().text_size(px(theme::FS_XS)).text_color(colors.text_3).child("Project"))
                    .child(project_chips),
            );

            let timed = draft.when != "now";
            let mut bot_chips = div().flex().flex_wrap().gap(px(4.0));
            if !timed {
                bot_chips = bot_chips.child(self.task_chip("task-bot-none", "Choose later".to_owned(), draft.bot_id.trim().is_empty(), colors, cx, {
                    move |root: &mut Root, cx: &mut Context<Root>| {
                        if let Some(draft) = root.tasks.draft.as_mut() {
                            draft.bot_id.clear();
                        }
                        cx.notify();
                    }
                }));
            }
            for agent in &bots {
                let chosen = draft.bot_id == agent.id;
                let id = agent.id.clone();
                let name = agent.name.clone();
                bot_chips = bot_chips.child(self.task_chip(&format!("task-bot-{id}"), name, chosen, colors, cx, move |root: &mut Root, cx: &mut Context<Root>| {
                    if let Some(draft) = root.tasks.draft.as_mut() {
                        draft.bot_id = id.clone();
                    }
                    cx.notify();
                }));
            }
            body = body.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(3.0))
                    .child(div().text_size(px(theme::FS_XS)).text_color(colors.text_3).child("Bot"))
                    .child(bot_chips),
            );

            // When: now, once, repeat.
            let mut when_chips = div().flex().gap(px(4.0));
            for (id, label) in [("now", "Now"), ("once", "Once"), ("repeat", "Repeat")] {
                let chosen = draft.when == id;
                let value = id.to_owned();
                when_chips = when_chips.child(self.task_chip(&format!("task-when-{id}"), label.to_owned(), chosen, colors, cx, move |root: &mut Root, cx: &mut Context<Root>| {
                    if let Some(draft) = root.tasks.draft.as_mut() {
                        draft.when = value.clone();
                        if value == "once" {
                            draft.schedule.preset = SchedulePreset::Once;
                        } else if value == "repeat" && draft.schedule.preset == SchedulePreset::Once {
                            draft.schedule.preset = SchedulePreset::Daily;
                        }
                    }
                    if let (Some(input), Some(draft)) = (root.tasks.fields.as_ref().map(|fields| fields.when_detail.clone()), root.tasks.draft.as_ref()) {
                        let text = match draft.schedule.preset {
                            SchedulePreset::Once => draft.schedule.once_at.clone(),
                            _ => draft.schedule.time.clone(),
                        };
                        input.update(cx, |input, cx| input.set_text(text, cx));
                    }
                    cx.notify();
                }));
            }
            body = body.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(3.0))
                    .child(div().text_size(px(theme::FS_XS)).text_color(colors.text_3).child("When"))
                    .child(when_chips),
            );

            if timed {
                let mut presets = div().flex().flex_wrap().gap(px(4.0));
                for preset in SchedulePreset::REPEAT {
                    if draft.when == "once" && preset != SchedulePreset::Once {
                        continue;
                    }
                    let chosen = draft.schedule.preset == preset;
                    presets = presets.child(self.task_chip(
                        &format!("task-preset-{}", preset.id()),
                        preset.label().to_owned(),
                        chosen,
                        colors,
                        cx,
                        {
                            move |root: &mut Root, cx: &mut Context<Root>| {
                                if let Some(draft) = root.tasks.draft.as_mut() {
                                    draft.schedule.preset = preset;
                                }
                                if let (Some(input), Some(draft)) = (root.tasks.fields.as_ref().map(|fields| fields.when_detail.clone()), root.tasks.draft.as_ref()) {
                                    let text = match draft.schedule.preset {
                                        SchedulePreset::Once => draft.schedule.once_at.clone(),
                                        _ => draft.schedule.time.clone(),
                                    };
                                    input.update(cx, |input, cx| input.set_text(text, cx));
                                }
                                cx.notify();
                            }
                        },
                    ));
                }
                body = body.child(presets);
                if draft.schedule.preset == SchedulePreset::Weekly {
                    let mut days = div().flex().flex_wrap().gap(px(4.0));
                    for (index, label) in WEEKDAYS.iter().enumerate() {
                        let index = index as u32;
                        let chosen = draft.schedule.weekday == index;
                        days = days.child(self.task_chip(&format!("task-weekday-{index}"), (*label).to_owned(), chosen, colors, cx, move |root: &mut Root, cx: &mut Context<Root>| {
                            if let Some(draft) = root.tasks.draft.as_mut() {
                                draft.schedule.weekday = index;
                            }
                            cx.notify();
                        }));
                    }
                    body = body.child(days);
                }
                let help = if draft.schedule.preset == SchedulePreset::Once {
                    "Date and time (2026-11-09T18:00)".to_owned()
                } else if draft.schedule.preset == SchedulePreset::Hours {
                    format!("Every {} hours, from the daemon's own timer", draft.schedule.every_hours)
                } else {
                    "Time (24-hour, HH:MM)".to_owned()
                };
                if draft.schedule.preset != SchedulePreset::Hours {
                    body = body.child(field(&help, None, when_detail, 30.0));
                } else {
                    body = body.child(div().text_size(px(theme::FS_2XS)).text_color(colors.text_3).child(SharedString::from(help)));
                }
                body = body.child(
                    div()
                        .text_size(px(theme::FS_2XS))
                        .text_color(colors.text_3)
                        .child(crate::state::tasks::SCHEDULE_DISCLAIMER),
                );
            } else {
                body = body.child(div().text_size(px(theme::FS_2XS)).text_color(colors.text_3).child(
                    "Runs when you give it to a bot, which is right away for the bot you chose.",
                ));
            }

            // The time limit and the estimate.
            let mut limits = div().flex().flex_wrap().gap(px(4.0));
            let mut choices: Vec<(i64, String)> = TIME_LIMIT_CHOICES.iter().map(|(value, label)| (*value, (*label).to_owned())).collect();
            if !choices.iter().any(|(value, _)| *value == draft.limit_secs) {
                choices.push((draft.limit_secs, format!("{} minutes", (draft.limit_secs / 60).max(1))));
            }
            for (value, label) in choices {
                let chosen = draft.limit_secs == value;
                limits = limits.child(self.task_chip(&format!("task-limit-{value}"), label, chosen, colors, cx, move |root: &mut Root, cx: &mut Context<Root>| {
                    if let Some(draft) = root.tasks.draft.as_mut() {
                        draft.limit_secs = value;
                    }
                    cx.notify();
                }));
            }
            body = body.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(3.0))
                    .child(div().text_size(px(theme::FS_XS)).text_color(colors.text_3).child("Stop a run after"))
                    .child(limits),
            );
            let estimate_help = estimate_problem(&draft.estimate).map(|problem| problem.text().to_owned());
            body = body.child(field("Estimate (minutes)", estimate_help, estimate, 30.0));
        }

        if let Some(problem) = problem.filter(|_| self.tasks.form_error.is_none()) {
            body = body.child(div().text_size(px(theme::FS_XS)).text_color(colors.status_danger).child(problem.text()));
        }
        if let Some(error) = self.tasks.form_error.clone() {
            body = body.child(div().text_size(px(theme::FS_XS)).text_color(colors.status_danger).child(SharedString::from(error)));
        }

        let mut actions = div().flex().items_center().gap(px(6.0)).child(div().flex_1());
        if editing.is_some() {
            actions = actions.child(self.ghost_button("Discard task", "task-discard", colors, cx, |root, cx| {
                if let Some(id) = root.tasks.editing.clone() {
                    root.task_act(id, "task.cancel", cx);
                    root.tasks.creating = false;
                    root.tasks.editing = None;
                    root.tasks.draft = None;
                    root.tasks.fields = None;
                }
                cx.notify();
            }));
        }
        actions = actions
            .child(self.ghost_button("Cancel", "task-cancel", colors, cx, |root, cx| {
                root.tasks_close_overlay(cx);
            }))
            .child(if submitting {
                self.primary_button(
                    if !timed_label(&draft.when) && can_start { if editing.is_some() { "Save and start" } else { "Add and start" } } else if editing.is_some() { "Save" } else { "Add task" },
                    "task-save",
                    colors,
                    cx,
                    |root, cx| root.save_task(cx),
                )
            } else {
                div()
                    .h(px(28.0))
                    .px(px(12.0))
                    .flex()
                    .items_center()
                    .rounded(px(theme::RADIUS_SM))
                    .bg(colors.raised)
                    .text_color(colors.text_3)
                    .text_size(px(theme::FS_XS))
                    .child(if editing.is_some() { "Save" } else { "Add task" })
                    .into_any_element()
            });
        body.child(actions).into_any_element()
    }

    /// One choice chip: the board's own select, flattened to a row.
    fn task_chip(
        &self,
        id: &str,
        label: String,
        active: bool,
        colors: Colors,
        cx: &mut Context<Self>,
        action: impl Fn(&mut Root, &mut Context<Root>) + 'static,
    ) -> AnyElement {
        div()
            .id(SharedString::from(id.to_owned()))
            .h(px(24.0))
            .px(px(9.0))
            .flex()
            .items_center()
            .rounded(px(theme::RADIUS_SM))
            .bg(if active { colors.accent } else { colors.raised })
            .text_color(if active { colors.text_inverse } else { colors.text_2 })
            .text_size(px(theme::FS_XS))
            .cursor_pointer()
            .hover(move |style| style.bg(colors.selected))
            .child(SharedString::from(label))
            .on_click(cx.listener(move |root, _: &gpui::ClickEvent, _window, cx| action(root, cx)))
            .into_any_element()
    }
}

fn timed_label(when: &str) -> bool {
    when != "now"
}
