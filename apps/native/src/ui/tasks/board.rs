//! The board: unassigned drafts on the left, one lane per bot on the right.
//! Ported from `ui/DelegateBoard.tsx`.
//!
//! React lets the owner drag a draft onto a bot. GPUI at the pinned revision has
//! an internal drag-and-drop (`on_drag` needs a `DragSource` entity registered
//! per card), which is a large amount of machinery for one gesture, so this board
//! gives every draft the same single dispatch action as a row of "Assign to"
//! buttons: one press, one `task.dispatch`, exactly the call a drop makes.

use gpui::{div, prelude::*, px, AnyElement, Context, FontWeight, SharedString};

use crate::art::{self, Mark};
use crate::state::tasks::{draft_tasks, finished_tasks, lane_availability, lane_tasks, queue_label, scheduled_tasks, state_columns, task_state_label, BoardTask};
use crate::state::Agent;
use crate::theme::{self, Colors};
use crate::ui::root::Root;

impl Root {
    pub(crate) fn render_task_board(&mut self, tasks: &[BoardTask], agents: &[Agent], colors: Colors, cx: &mut Context<Self>) -> AnyElement {
        let drafts = draft_tasks(tasks);
        let scheduled = scheduled_tasks(tasks);
        let finished = finished_tasks(tasks);
        let mut bots: Vec<Agent> = agents.iter().filter(|agent| !agent.archived && !agent.hidden).cloned().collect();
        bots.sort_by_key(|agent| agent.sort_order);

        // The states at a glance, in the daemon's own order: one chip per state
        // that has cards, with the count. This is the "columns by state" the
        // brief asks for; the lanes below are how React lays the work out.
        let mut columns = div().flex().flex_wrap().gap(px(6.0));
        for (state, rows) in state_columns(tasks) {
            columns = columns.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(5.0))
                    .h(px(22.0))
                    .px(px(8.0))
                    .rounded(px(theme::RADIUS_SM))
                    .bg(colors.panel)
                    .border_1()
                    .border_color(colors.line)
                    .text_size(px(theme::FS_2XS))
                    .text_color(colors.text_2)
                    .child(SharedString::from(task_state_label(state).to_owned()))
                    .child(div().text_color(colors.text_3).child(SharedString::from(rows.len().to_string()))),
            );
        }

        let mut backlog = div()
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(div().font_weight(FontWeight::SEMIBOLD).child("Unassigned"))
                    .child(div().text_size(px(theme::FS_XS)).text_color(colors.text_3).child(SharedString::from(drafts.len().to_string()))),
            );

        if drafts.is_empty() {
            backlog = backlog.child(
                div()
                    .p(px(12.0))
                    .rounded(px(theme::RADIUS_MD))
                    .bg(colors.panel)
                    .border_1()
                    .border_color(colors.line)
                    .text_color(colors.text_3)
                    .child("No drafts. Ask your CEO to plan some work, or add a task."),
            );
        }
        for task in drafts {
            backlog = backlog.child(self.render_draft_card(&task, &bots, colors, cx));
        }

        if !scheduled.is_empty() {
            let now_ms = chrono::Utc::now().timestamp_millis();
            let mut block = div()
                .mt(px(6.0))
                .flex()
                .flex_col()
                .gap(px(6.0))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(div().font_weight(FontWeight::SEMIBOLD).child("Scheduled"))
                        .child(div().text_size(px(theme::FS_XS)).text_color(colors.text_3).child(SharedString::from(scheduled.len().to_string()))),
                );
            for task in scheduled {
                let id = task.id.clone();
                let next = crate::ui::schedules::describe_next_run(task.schedule_next_at.as_deref(), now_ms);
                let last = task
                    .schedule_last_outcome
                    .as_deref()
                    .map(crate::state::tasks::schedule_outcome_label)
                    .map(|label| format!("Last: {label}"))
                    .unwrap_or_default();
                let line = match (next.is_empty(), last.is_empty()) {
                    (false, false) => format!("Next run {next} · {last}"),
                    (false, true) => format!("Next run {next}"),
                    (true, false) => last,
                    (true, true) => String::new(),
                };
                block = block.child(
                    div()
                        .id(SharedString::from(format!("scheduled-{}", task.id)))
                        .p(px(10.0))
                        .flex()
                        .flex_col()
                        .gap(px(2.0))
                        .rounded(px(theme::RADIUS_MD))
                        .bg(colors.panel)
                        .border_1()
                        .border_color(colors.line)
                        .cursor_pointer()
                        .hover(move |style| style.bg(colors.hover))
                        .child(div().font_weight(FontWeight::MEDIUM).truncate().child(SharedString::from(task.title.clone())))
                        .child(
                            div()
                                .text_size(px(theme::FS_XS))
                                .text_color(colors.text_3)
                                .truncate()
                                .child(SharedString::from(self.task_schedule_line(&task).unwrap_or_default())),
                        )
                        .child(div().text_size(px(theme::FS_XS)).text_color(colors.text_3).truncate().child(SharedString::from(line)))
                        .on_click(cx.listener(move |root, _: &gpui::ClickEvent, _window, cx| {
                            root.open_task_detail(id.clone(), cx);
                        })),
                );
            }
            backlog = backlog.child(block);
        }

        if !finished.is_empty() {
            let open = self.tasks.show_finished;
            let mut block = div().mt(px(6.0)).flex().flex_col().gap(px(4.0)).child(
                div()
                    .id("tasks-finished-toggle")
                    .h(px(26.0))
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .rounded(px(theme::RADIUS_SM))
                    .text_size(px(theme::FS_XS))
                    .text_color(colors.text_2)
                    .cursor_pointer()
                    .hover(move |style| style.text_color(colors.text_1))
                    .child("Finished")
                    .child(div().text_color(colors.text_3).child(SharedString::from(finished.len().to_string())))
                    .on_click(cx.listener(|root, _: &gpui::ClickEvent, _window, cx| {
                        root.tasks.show_finished = !root.tasks.show_finished;
                        cx.notify();
                    })),
            );
            if open {
                for task in finished {
                    let id = task.id.clone();
                    let tone = match task.state.as_str() {
                        "done" => colors.status_success,
                        "failed" => colors.status_danger,
                        _ => colors.text_3,
                    };
                    block = block.child(
                        div()
                            .id(SharedString::from(format!("finished-{}", task.id)))
                            .h(px(26.0))
                            .px(px(6.0))
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .rounded(px(theme::RADIUS_SM))
                            .cursor_pointer()
                            .hover(move |style| style.bg(colors.hover))
                            .child(div().size(px(6.0)).rounded(px(3.0)).bg(tone))
                            .child(div().flex_1().min_w(px(0.0)).truncate().text_size(px(theme::FS_XS)).child(SharedString::from(task.title.clone())))
                            .child(div().text_size(px(theme::FS_2XS)).text_color(colors.text_3).child(SharedString::from(task.state_label().to_owned())))
                            .on_click(cx.listener(move |root, _: &gpui::ClickEvent, _window, cx| {
                                root.open_task_detail(id.clone(), cx);
                            })),
                    );
                }
            }
            backlog = backlog.child(block);
        }

        let mut lanes = div().flex_1().min_w(px(0.0)).flex().flex_col().gap(px(8.0)).child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(div().font_weight(FontWeight::SEMIBOLD).child("Bots"))
                .child(
                    div()
                        .text_size(px(theme::FS_XS))
                        .text_color(colors.text_3)
                        .child("Assign a draft to start the task or queue it"),
                ),
        );
        if bots.is_empty() {
            lanes = lanes.child(div().text_color(colors.text_3).child("Create a bot first, then give it work here."));
        }
        let now_ms = chrono::Utc::now().timestamp_millis();
        for agent in &bots {
            lanes = lanes.child(self.render_lane(agent, tasks, now_ms, colors, cx));
        }

        div()
            .flex()
            .flex_col()
            .gap(px(10.0))
            .child(columns)
            .child(div().flex().gap(px(16.0)).items_start().child(backlog).child(lanes))
            .into_any_element()
    }

    fn render_draft_card(&mut self, task: &BoardTask, bots: &[Agent], colors: Colors, cx: &mut Context<Self>) -> AnyElement {
        let id = task.id.clone();
        let busy = self.tasks.busy.contains(&task.id);
        let meta = self.task_meta(task);
        let mut card = div()
            .id(SharedString::from(format!("draft-{}", task.id)))
            .p(px(10.0))
            .flex()
            .flex_col()
            .gap(px(6.0))
            .rounded(px(theme::RADIUS_MD))
            .bg(colors.panel)
            .border_1()
            .border_color(colors.line)
            .child(
                div()
                    .id(SharedString::from(format!("draft-open-{}", task.id)))
                    .flex()
                    .items_start()
                    .gap(px(6.0))
                    .cursor_pointer()
                    .child(div().mt(px(3.0)).size(px(6.0)).rounded(px(3.0)).bg(colors.line_strong))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .flex()
                            .flex_col()
                            .child(div().font_weight(FontWeight::MEDIUM).truncate().child(SharedString::from(task.title.clone())))
                            .when(!meta.is_empty(), |this| {
                                this.child(div().text_size(px(theme::FS_XS)).text_color(colors.text_3).truncate().child(SharedString::from(meta)))
                            }),
                    )
                    .on_click(cx.listener(move |root, _: &gpui::ClickEvent, _window, cx| {
                        root.open_task_detail(id.clone(), cx);
                    })),
            );
        let mut assign = div().flex().flex_wrap().gap(px(4.0));
        if bots.is_empty() {
            assign = assign.child(div().text_size(px(theme::FS_2XS)).text_color(colors.text_3).child("No bot to give this to yet."));
        }
        for agent in bots {
            let agent_id = agent.id.clone();
            let task_id = task.id.clone();
            let disabled = busy;
            assign = assign.child(
                div()
                    .id(SharedString::from(format!("assign-{}-{}", task.id, agent.id)))
                    .h(px(22.0))
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .rounded(px(theme::RADIUS_SM))
                    .bg(colors.raised)
                    .text_color(if disabled { colors.text_3 } else { colors.text_2 })
                    .text_size(px(theme::FS_2XS))
                    .cursor_pointer()
                    .hover(move |style| style.bg(colors.selected).text_color(colors.text_1))
                    .child(SharedString::from(format!("→ {}", agent.name)))
                    .on_click(cx.listener(move |root, _: &gpui::ClickEvent, _window, cx| {
                        if disabled {
                            return;
                        }
                        root.dispatch_task(task_id.clone(), agent_id.clone(), cx);
                    })),
            );
        }
        card = card.child(assign);
        if busy {
            card = card.child(div().text_size(px(theme::FS_2XS)).text_color(colors.text_3).child("Starting…"));
        }
        card.into_any_element()
    }

    fn render_lane(&mut self, agent: &Agent, tasks: &[BoardTask], now_ms: i64, colors: Colors, cx: &mut Context<Self>) -> AnyElement {
        let lane = lane_tasks(tasks, &agent.id);
        let availability = lane_availability(&lane, now_ms);
        let tone = match availability.tone {
            "working" => colors.status_working,
            "attention" => colors.status_info,
            _ => colors.status_success,
        };
        let provider = self.provider_of(agent);
        let mark = Mark::for_agent(28.0, art::agent_color(&agent.color), agent.icon(), provider.as_deref(), agent.name.clone(), colors).render();
        let mut section = div()
            .id(SharedString::from(format!("lane-{}", agent.id)))
            .p(px(10.0))
            .flex()
            .flex_col()
            .gap(px(6.0))
            .rounded(px(theme::RADIUS_MD))
            .bg(colors.panel)
            .border_1()
            .border_color(colors.line)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(mark)
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .flex()
                            .flex_col()
                            .child(div().font_weight(FontWeight::MEDIUM).truncate().child(SharedString::from(agent.name.clone())))
                            .child(
                                div()
                                    .text_size(px(theme::FS_2XS))
                                    .text_color(colors.text_3)
                                    .truncate()
                                    .child(SharedString::from(agent.model.clone().unwrap_or_default())),
                            ),
                    )
                    .child(
                        div()
                            .h(px(20.0))
                            .px(px(7.0))
                            .flex()
                            .items_center()
                            .gap(px(4.0))
                            .rounded(px(theme::RADIUS_SM))
                            .bg(colors.raised)
                            .child(div().size(px(5.0)).rounded(px(3.0)).bg(tone))
                            .child(div().text_size(px(theme::FS_2XS)).text_color(colors.text_2).child(SharedString::from(availability.label.clone()))),
                    )
                    .when(availability.queued > 0, |this| {
                        this.child(
                            div()
                                .text_size(px(theme::FS_2XS))
                                .text_color(colors.text_3)
                                .child(SharedString::from(format!("{} queued", availability.queued))),
                        )
                    }),
            );
        if lane.is_empty() {
            section = section.child(div().py(px(4.0)).text_size(px(theme::FS_XS)).text_color(colors.text_3).child("Free"));
        }
        for task in lane {
            let id = task.id.clone();
            let meta = match (task.state.as_str(), task.queue_position) {
                ("queued", Some(position)) => format!("Queued {}", queue_label(Some(position)).unwrap_or(1)),
                _ => task.state_label().to_owned(),
            };
            let tone = match task.state.as_str() {
                "running" => colors.status_working,
                "needs_you" | "awaiting_review" => colors.status_info,
                "failed" => colors.status_danger,
                _ => colors.text_3,
            };
            section = section.child(
                div()
                    .id(SharedString::from(format!("lane-task-{}", task.id)))
                    .h(px(26.0))
                    .px(px(6.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .rounded(px(theme::RADIUS_SM))
                    .cursor_pointer()
                    .hover(move |style| style.bg(colors.hover))
                    .child(div().size(px(6.0)).rounded(px(3.0)).bg(tone))
                    .child(div().flex_1().min_w(px(0.0)).truncate().text_size(px(theme::FS_XS)).child(SharedString::from(task.title.clone())))
                    .child(div().text_size(px(theme::FS_2XS)).text_color(colors.text_3).child(SharedString::from(meta)))
                    .on_click(cx.listener(move |root, _: &gpui::ClickEvent, _window, cx| {
                        root.open_task_detail(id.clone(), cx);
                    })),
            );
        }
        section.into_any_element()
    }
}
