//! The strip a conversation shows after a turn changed files: what changed, by
//! how much, and an Undo for each file. Ported from `ui/ChangeBanner.tsx`.
//!
//! The files come from the daemon's own checkpoints (`session.checkpoints`) — the
//! native snapshot does not carry a conversation's `files` list — so the strip
//! shows what the daemon recorded for the newest turn, which is also the list
//! that can be undone.

use gpui::{div, prelude::*, px, AnyElement, Context, SharedString};
use serde_json::json;

use crate::state::changes::{banner_changes, CONTEXT_TIP_PERCENT};
use crate::theme::{self, Colors};
use crate::ui::root::Root;

impl Root {
    pub(crate) fn render_change_banner(&mut self, colors: Colors, cx: &mut Context<Self>) -> Option<AnyElement> {
        let session_id = self.session_id.clone()?;
        let session = self.selected_session().cloned()?;
        let live = session.is_busy();
        if !self.changes.checkpoints_requested.contains(&session_id) {
            self.request_checkpoints(session_id.clone(), cx);
        }
        let dismissed = self.changes.banner_dismissed.get(&session_id).cloned().unwrap_or_default();
        let checkpoints = self.changes.checkpoints.get(&session_id).cloned();
        let changes = banner_changes(checkpoints.as_ref(), live, &dismissed);
        // The window has no context size to compare the used tokens with (the
        // native snapshot carries the used count, not the window), so the
        // "nearly full" suggestion only appears when both are known.
        let context_tip = false;
        let _ = CONTEXT_TIP_PERCENT;
        if changes.is_empty() && !context_tip {
            return None;
        }
        let added: i64 = changes.iter().map(|change| change.added).sum();
        let removed: i64 = changes.iter().map(|change| change.removed).sum();
        let count = changes.len();
        let open = self.changes.banner_open;

        let mut strip = div()
            .flex_none()
            .px(px(12.0))
            .py(px(6.0))
            .flex()
            .flex_col()
            .gap(px(4.0))
            .bg(colors.panel)
            .border_b_1()
            .border_color(colors.line);

        let mut head = div().flex().items_center().gap(px(8.0)).text_size(px(theme::FS_XS));
        if count == 1 {
            let change = changes[0].clone();
            head = head.child(SharedString::from(change.name.clone())).child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(div().text_color(colors.status_success).child(SharedString::from(format!("+{}", change.added))))
                    .child(div().text_color(colors.status_danger).child(SharedString::from(format!("−{}", change.removed)))),
            );
            head = head.child(self.banner_undo_button(&session_id, &change.id, change.clipped, &change.name, colors, cx));
        } else {
            head = head.child(
                div()
                    .id("banner-toggle")
                    .cursor_pointer()
                    .hover(move |style| style.text_color(colors.text_1))
                    .child(SharedString::from(format!("{count} files changed")))
                    .on_click(cx.listener(|root, _: &gpui::ClickEvent, _window, cx| {
                        root.changes.banner_open = !root.changes.banner_open;
                        cx.notify();
                    })),
            );
            head = head
                .child(div().text_color(colors.status_success).child(SharedString::from(format!("+{added}"))))
                .child(div().text_color(colors.status_danger).child(SharedString::from(format!("−{removed}"))));
        }
        head = head.child(div().flex_1());
        let ids: Vec<String> = changes.iter().map(|change| change.id.clone()).collect();
        head = head.child(
            div()
                .id("banner-dismiss")
                .size(px(20.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(theme::RADIUS_SM))
                .cursor_pointer()
                .text_color(colors.text_3)
                .hover(move |style| style.bg(colors.hover).text_color(colors.text_1))
                .child("✕")
                .on_click(cx.listener(move |root, _: &gpui::ClickEvent, _window, cx| {
                    let session_id = root.session_id.clone().unwrap_or_default();
                    root.changes.banner_dismissed.entry(session_id).or_default().extend(ids.clone());
                    cx.notify();
                })),
        );
        strip = strip.child(head);

        if count > 1 && open {
            for change in &changes {
                strip = strip.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .text_size(px(theme::FS_2XS))
                        .child(div().flex_1().min_w(px(0.0)).truncate().text_color(colors.text_2).child(SharedString::from(change.name.clone())))
                        .child(div().text_color(colors.status_success).child(SharedString::from(format!("+{}", change.added))))
                        .child(div().text_color(colors.status_danger).child(SharedString::from(format!("−{}", change.removed))))
                        .child(self.banner_undo_button(&session_id, &change.id, change.clipped, &change.name, colors, cx)),
                );
            }
        }
        Some(strip.into_any_element())
    }

    /// Undo one file's change, in two presses: the first asks, the second does
    /// it. The daemon refuses a file that changed again since.
    fn banner_undo_button(
        &mut self,
        session_id: &str,
        file_change_id: &str,
        clipped: bool,
        name: &str,
        colors: Colors,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let session_id = session_id.to_owned();
        let file_change_id = file_change_id.to_owned();
        let name = name.to_owned();
        let confirming = self.changes.undo_confirm.as_deref() == Some(file_change_id.as_str());
        if clipped {
            return div()
                .px(px(4.0))
                .text_size(px(theme::FS_2XS))
                .text_color(colors.text_3)
                .child("Cannot undo")
                .into_any_element();
        }
        div()
            .id(SharedString::from(format!("banner-undo-{file_change_id}")))
            .h(px(20.0))
            .px(px(6.0))
            .flex()
            .items_center()
            .rounded(px(theme::RADIUS_SM))
            .bg(colors.raised)
            .text_size(px(theme::FS_2XS))
            .text_color(colors.text_2)
            .cursor_pointer()
            .hover(move |style| style.bg(colors.selected).text_color(colors.text_1))
            .child(if confirming { format!("Undo {name}?") } else { "Undo".to_owned() })
            .on_click(cx.listener(move |root, _: &gpui::ClickEvent, _window, cx| {
                if root.changes.undo_confirm.as_deref() != Some(file_change_id.as_str()) {
                    root.changes.undo_confirm = Some(file_change_id.clone());
                    cx.notify();
                    return;
                }
                root.changes.undo_confirm = None;
                if let Some(review) = root.changes.review.as_mut() {
                    review.busy = true;
                }
                root.daemon_command(
                    crate::daemon::Command::Rpc {
                        tag: "file-revert".to_owned(),
                        method: "file.revert".to_owned(),
                        params: json!({ "sessionId": session_id, "fileChangeId": file_change_id }),
                    },
                    cx,
                );
                cx.notify();
            }))
            .into_any_element()
    }
}
