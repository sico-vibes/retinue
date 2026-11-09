//! One task's details: the facts the daemon recorded, what the task asked for,
//! its attempts, and the actions that move it on. Ported from the detail half of
//! `ui/TasksView.tsx`.

use gpui::{div, prelude::*, px, AnyElement, Context, FontWeight, SharedString};

use crate::state::tasks::{elapsed_label, task_state_label, when_text};
use crate::theme::{self, Colors};
use crate::ui::root::Root;

impl Root {
    pub(crate) fn render_task_detail(&mut self, colors: Colors, cx: &mut Context<Self>) -> AnyElement {
        let Some(open_id) = self.tasks.open.clone() else { return div().into_any_element() };
        let Some(task) = self.tasks.task(&open_id).cloned() else {
            return div().text_color(colors.text_3).child("This task is no longer here.").into_any_element();
        };
        let now_ms = chrono::Utc::now().timestamp_millis();
        let busy = self.tasks.busy.contains(&task.id);
        let runs = self.tasks.runs_for(&task.id).to_vec();
        let latest = self.tasks.latest_run(&task.id).cloned();
        let owner = task
            .assignee_agent_id
            .as_deref()
            .and_then(|agent_id| self.snapshot().and_then(|snapshot| snapshot.agent(agent_id)))
            .map(|agent| agent.name.clone());
        let project = task
            .project_id
            .as_deref()
            .and_then(|project_id| self.snapshot().and_then(|snapshot| snapshot.project_name(Some(project_id)).map(str::to_owned)));
        let took = elapsed_label(task.started_at.as_deref(), task.completed_at.as_deref(), now_ms);
        let schedule = self.task_schedule_line(&task);
        let outcome = task.schedule_last_outcome.as_deref().map(crate::state::tasks::schedule_outcome_label);

        let mut card = div()
            .w(px(620.0))
            .max_h(px(640.0))
            .id("task-detail")
            .overflow_y_scroll()
            .p(px(16.0))
            .flex()
            .flex_col()
            .gap(px(10.0))
            .rounded(px(theme::RADIUS_LG))
            .bg(colors.overlay)
            .border_1()
            .border_color(colors.line_strong)
            .child(
                div()
                    .flex()
                    .items_start()
                    .gap(px(8.0))
                    .child(div().flex_1().min_w(px(0.0)).text_size(px(theme::FS_LG)).font_weight(FontWeight::SEMIBOLD).child(SharedString::from(task.title.clone())))
                    .child(
                        div()
                            .h(px(22.0))
                            .px(px(8.0))
                            .flex()
                            .items_center()
                            .rounded(px(theme::RADIUS_SM))
                            .bg(colors.raised)
                            .text_size(px(theme::FS_2XS))
                            .text_color(colors.text_2)
                            .child(SharedString::from(task.state_label().to_owned())),
                    ),
            );

        // The facts, in the order the React dialog lists them.
        let mut facts: Vec<(String, String)> = Vec::new();
        if let Some(owner) = owner {
            facts.push(("Bot".to_owned(), owner));
        }
        if let Some(project) = project {
            facts.push(("Project".to_owned(), project));
        }
        if task.started_at.is_some() {
            facts.push(("Started".to_owned(), when_text(task.started_at.as_deref())));
        }
        if let Some(took) = took {
            facts.push((if task.completed_at.is_some() { "Took" } else { "Running for" }.to_owned(), took));
        }
        if let Some(minutes) = task.estimate_minutes {
            facts.push(("Estimate".to_owned(), format!("about {minutes} min")));
        }
        if let Some(secs) = task.max_runtime_secs {
            facts.push(("Time limit".to_owned(), format!("{} min", (secs as f64 / 60.0).round() as i64)));
        }
        if task.unattended {
            facts.push(("Started by".to_owned(), "A schedule".to_owned()));
        }
        if let Some(schedule) = schedule {
            let line = match outcome {
                Some(outcome) => format!("{schedule} · {outcome}"),
                None => schedule,
            };
            facts.push(("Schedule".to_owned(), line));
        }
        let mut facts_block = div().flex().flex_col().gap(px(2.0)).text_size(px(theme::FS_XS));
        for (label, value) in facts {
            facts_block = facts_block.child(
                div()
                    .flex()
                    .gap(px(8.0))
                    .child(div().w(px(96.0)).text_color(colors.text_3).child(SharedString::from(label)))
                    .child(div().flex_1().min_w(px(0.0)).text_color(colors.text_2).child(SharedString::from(value))),
            );
        }
        card = card.child(facts_block);

        // What the task asked for.
        for (title, text) in [
            ("What needs doing", task.brief.clone()),
            ("Done when", task.done_goal.clone()),
            ("Report back", task.report_back.clone()),
        ] {
            if text.trim().is_empty() {
                continue;
            }
            card = card.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(div().text_size(px(theme::FS_XS)).font_weight(FontWeight::MEDIUM).child(SharedString::from(title.to_owned())))
                    .child(div().text_size(px(theme::FS_SM)).text_color(colors.text_2).child(SharedString::from(text))),
            );
        }

        if !task.images.is_empty() {
            card = card.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(div().text_size(px(theme::FS_XS)).font_weight(FontWeight::MEDIUM).child("Images"))
                    .child(self.render_task_images(&task, colors, cx)),
            );
        }

        if task.state == "awaiting_review" {
            card = card.child(div().text_size(px(theme::FS_XS)).text_color(colors.text_2).child(
                "The bot finished its turn. Check the result, then mark the task done or not done. Finishing a turn does not count as done.",
            ));
        }

        if runs.len() > 1 {
            let mut ordered = runs.clone();
            ordered.sort_by(|a, b| b.attempt.cmp(&a.attempt));
            let mut list = div().flex().flex_col().gap(px(2.0)).child(div().text_size(px(theme::FS_XS)).text_color(colors.text_3).child("Attempts"));
            for run in ordered {
                list = list.child(
                    div()
                        .flex()
                        .gap(px(8.0))
                        .text_size(px(theme::FS_XS))
                        .text_color(colors.text_2)
                        .child(div().w(px(70.0)).child(SharedString::from(format!("Attempt {}", run.attempt))))
                        .child(
                            div()
                                .w(px(110.0))
                                .text_color(colors.text_3)
                                .child(SharedString::from(run.assignee_agent_name.clone().unwrap_or_else(|| "Bot".to_owned()))),
                        )
                        .child(div().text_color(colors.text_3).child(SharedString::from(task_state_label(&run.state).to_owned()))),
                );
            }
            card = card.child(list);
        }

        // Try again with…: one press dispatches the card to that bot again.
        if matches!(task.state.as_str(), "failed" | "awaiting_review") {
            let mut bots = div().flex().flex_wrap().gap(px(4.0));
            for agent in self.assignable() {
                let agent_id = agent.id.clone();
                let task_id = task.id.clone();
                let disabled = busy;
                bots = bots.child(
                    div()
                        .id(SharedString::from(format!("retry-{}", agent.id)))
                        .h(px(24.0))
                        .px(px(9.0))
                        .flex()
                        .items_center()
                        .rounded(px(theme::RADIUS_SM))
                        .bg(colors.raised)
                        .text_color(colors.text_2)
                        .text_size(px(theme::FS_XS))
                        .cursor_pointer()
                        .hover(move |style| style.bg(colors.selected).text_color(colors.text_1))
                        .child(SharedString::from(format!("Try again with {}", agent.name)))
                        .on_click(cx.listener(move |root, _: &gpui::ClickEvent, _window, cx| {
                            if disabled {
                                return;
                            }
                            root.dispatch_task(task_id.clone(), agent_id.clone(), cx);
                        })),
                );
            }
            card = card.child(bots);
        }

        let mut actions = div().flex().items_center().gap(px(6.0)).child(div().flex_1());
        actions = actions.child(self.ghost_button("Close", "task-detail-close", colors, cx, |root, cx| {
            root.tasks.open = None;
            cx.notify();
        }));
        if task.state == "unassigned" {
            let edit_id = task.id.clone();
            actions = actions.child(self.ghost_button("Edit", "task-detail-edit", colors, cx, move |root, cx| {
                root.edit_task(edit_id.clone(), cx);
            }));
        }
        if let Some(session_id) = latest.as_ref().and_then(|run| run.session_id.clone()) {
            actions = actions.child(self.ghost_button("Open conversation", "task-detail-session", colors, cx, move |root, cx| {
                if let Some(session) = root.snapshot().and_then(|snapshot| snapshot.session(&session_id)).cloned() {
                    root.tasks.open = None;
                    if let Some(agent_id) = session.agent_id.clone() {
                        root.open_conversation(agent_id, session.id.clone(), cx);
                    }
                }
            }));
        }
        match task.state.as_str() {
            "queued" => {
                let unassign = task.id.clone();
                actions = actions.child(self.ghost_button("Unassign", "task-detail-unassign", colors, cx, move |root, cx| {
                    root.task_act(unassign.clone(), "task.unassign", cx);
                }));
                let cancel = task.id.clone();
                actions = actions.child(self.ghost_button("Cancel task", "task-detail-cancel", colors, cx, move |root, cx| {
                    root.task_act(cancel.clone(), "task.cancel", cx);
                    root.tasks.open = None;
                    cx.notify();
                }));
            }
            "running" | "needs_you" => {
                let stop = task.id.clone();
                actions = actions.child(self.ghost_button("Stop", "task-detail-stop", colors, cx, move |root, cx| {
                    root.task_act(stop.clone(), "task.stop", cx);
                }));
            }
            _ => {}
        }
        if matches!(task.state.as_str(), "awaiting_review" | "needs_you") {
            let not_done = task.id.clone();
            let done = task.id.clone();
            actions = actions
                .child(self.ghost_button("Not done", "task-detail-not-done", colors, cx, move |root, cx| {
                    root.review_task(not_done.clone(), "not_done", cx);
                }))
                .child(self.primary_button("Mark done", "task-detail-done", colors, cx, move |root, cx| {
                    root.review_task(done.clone(), "done", cx);
                }));
        }
        card = card.child(actions);
        card.into_any_element()
    }

    /// One card, opened from anywhere: the board, the tree or the timeline.
    pub(crate) fn open_task_detail(&mut self, task_id: String, cx: &mut Context<Self>) {
        self.tasks.open = Some(task_id.clone());
        self.load_task_runs(task_id, cx);
        cx.notify();
    }


}
