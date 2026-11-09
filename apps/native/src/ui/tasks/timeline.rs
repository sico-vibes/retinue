//! The timeline: each attempt at a task, in the daemon's own order.
//! Ported from `ui/TaskTimeline.tsx`.

use gpui::{div, prelude::*, px, AnyElement, Context, FontWeight, SharedString};

use crate::state::tasks::{elapsed_label, BoardTask};
use crate::state::Agent;
use crate::theme::{self, Colors};
use crate::ui::root::Root;

impl Root {
    pub(crate) fn render_task_timeline(&mut self, tasks: &[BoardTask], agents: &[Agent], colors: Colors, cx: &mut Context<Self>) -> AnyElement {
        let now_ms = chrono::Utc::now().timestamp_millis();
        let mut assigned: Vec<&BoardTask> = tasks.iter().filter(|task| task.assignee_agent_id.is_some()).collect();
        assigned.sort_by(|a, b| b.updated_at.cmp(&a.updated_at).then_with(|| a.id.cmp(&b.id)));
        // The attempts of every assigned task, once each.
        for task in &assigned {
            if !self.tasks.runs_requested.contains(&task.id) {
                self.load_task_runs(task.id.clone(), cx);
            }
        }
        if assigned.is_empty() {
            return div().text_color(colors.text_3).child("No task has been given to a bot yet.").into_any_element();
        }
        let mut list = div().flex().flex_col().gap(px(8.0));
        for task in assigned {
            let id = task.id.clone();
            let owner = task
                .assignee_agent_id
                .as_deref()
                .and_then(|agent_id| agents.iter().find(|agent| agent.id == agent_id))
                .map(|agent| agent.name.clone())
                .unwrap_or_else(|| "A bot".to_owned());
            let mut card = div()
                .p(px(10.0))
                .flex()
                .flex_col()
                .gap(px(4.0))
                .rounded(px(theme::RADIUS_MD))
                .bg(colors.panel)
                .border_1()
                .border_color(colors.line)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(div().flex_1().min_w(px(0.0)).truncate().font_weight(FontWeight::MEDIUM).child(SharedString::from(task.title.clone())))
                        .child(div().text_size(px(theme::FS_XS)).text_color(colors.text_3).child(SharedString::from(owner)))
                        .child(div().text_size(px(theme::FS_XS)).text_color(colors.text_3).child(SharedString::from(task.state_label().to_owned()))),
                );
            let runs = self.tasks.runs_for(&task.id).to_vec();
            if runs.is_empty() {
                card = card.child(div().text_size(px(theme::FS_XS)).text_color(colors.text_3).child("No attempts recorded yet."));
            } else {
                let mut rows = div().flex().flex_col().gap(px(2.0));
                for run in runs.iter() {
                    let took = elapsed_label(run.started_at.as_deref(), run.ended_at.as_deref(), now_ms);
                    rows = rows.child(
                        div()
                            .h(px(22.0))
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .text_size(px(theme::FS_XS))
                            .child(div().w(px(64.0)).text_color(colors.text_3).child(SharedString::from(format!("Attempt {}", run.attempt))))
                            .child(div().w(px(110.0)).text_color(colors.text_2).child(SharedString::from(run.state.clone())))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .truncate()
                                    .text_color(colors.text_3)
                                    .child(SharedString::from(crate::state::tasks::when_text(run.started_at.as_deref()))),
                            )
                            .when_some(took, |this, took| {
                                this.child(div().text_color(colors.text_3).child(SharedString::from(took)))
                            }),
                    );
                }
                card = card.child(rows);
            }
            let card = card.id(SharedString::from(format!("timeline-{id}")));
            list = list.child(card.on_click(cx.listener(move |root, _: &gpui::ClickEvent, _window, cx| {
                root.tasks.open = Some(id.clone());
                root.load_task_runs(id.clone(), cx);
                cx.notify();
            })));
        }
        div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(div().font_weight(FontWeight::SEMIBOLD).child("Timeline"))
            .child(list)
            .into_any_element()
    }
}
