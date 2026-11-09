//! The tree view: every task with the sub-tasks the daemon recorded under it.
//! Ported from `ui/TaskTree.tsx`.

use gpui::{div, prelude::*, px, AnyElement, Context, FontWeight, SharedString};

use crate::state::tasks::BoardTask;
use crate::state::Agent;
use crate::theme::{self, Colors};
use crate::ui::root::Root;

impl Root {
    pub(crate) fn render_task_tree(&mut self, tasks: &[BoardTask], agents: &[Agent], colors: Colors, cx: &mut Context<Self>) -> AnyElement {
        let mut rows: Vec<(BoardTask, usize)> = Vec::new();
        let mut roots: Vec<&BoardTask> = tasks.iter().filter(|task| task.parent_task_id.is_none()).collect();
        roots.sort_by(|a, b| a.created_at.cmp(&b.created_at).then_with(|| a.id.cmp(&b.id)));
        for root in roots {
            rows.push((root.clone(), 0));
            let mut children: Vec<&BoardTask> = tasks.iter().filter(|task| task.parent_task_id.as_deref() == Some(root.id.as_str())).collect();
            children.sort_by(|a, b| a.created_at.cmp(&b.created_at).then_with(|| a.id.cmp(&b.id)));
            for child in children {
                rows.push((child.clone(), 1));
                // One more level, which is what the daemon's own sub-bot tree
                // records in practice.
                for grandchild in tasks.iter().filter(|task| task.parent_task_id.as_deref() == Some(child.id.as_str())) {
                    rows.push((grandchild.clone(), 2));
                }
            }
        }
        if rows.is_empty() {
            return div().text_color(colors.text_3).child("No tasks yet. Add one to see it here.").into_any_element();
        }
        let mut list = div().flex().flex_col().gap(px(2.0));
        for (task, depth) in rows {
            let id = task.id.clone();
            let owner = task
                .assignee_agent_id
                .as_deref()
                .and_then(|agent_id| agents.iter().find(|agent| agent.id == agent_id))
                .map(|agent| agent.name.clone());
            list = list.child(
                div()
                    .id(SharedString::from(format!("tree-{}", task.id)))
                    .h(px(28.0))
                    .pl(px(6.0 + depth as f32 * 18.0))
                    .pr(px(6.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .rounded(px(theme::RADIUS_SM))
                    .cursor_pointer()
                    .hover(move |style| style.bg(colors.hover))
                    .child(div().size(px(6.0)).rounded(px(3.0)).bg(match task.state.as_str() {
                        "done" => colors.status_success,
                        "failed" => colors.status_danger,
                        "running" => colors.status_working,
                        "needs_you" | "awaiting_review" => colors.status_info,
                        _ => colors.text_3,
                    }))
                    .child(div().flex_1().min_w(px(0.0)).truncate().text_size(px(theme::FS_SM)).child(SharedString::from(task.title.clone())))
                    .when_some(owner, |this, owner| {
                        this.child(div().text_size(px(theme::FS_XS)).text_color(colors.text_3).child(SharedString::from(owner)))
                    })
                    .child(div().text_size(px(theme::FS_XS)).text_color(colors.text_3).child(SharedString::from(task.state_label().to_owned())))
                    .on_click(cx.listener(move |root, _: &gpui::ClickEvent, _window, cx| {
                        root.open_task_detail(id.clone(), cx);
                    })),
            );
        }
        div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(div().font_weight(FontWeight::SEMIBOLD).child("Task tree"))
            .child(list)
            .into_any_element()
    }
}
