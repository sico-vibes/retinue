//! The review screen: every changed file in the conversation's folder against
//! HEAD, with its diff, stage and discard per file, and comments for the bot.
//! Ported from `ui/ChangesView.tsx`.
//!
//! The daemon takes the folder from the conversation, so this screen only ever
//! names a conversation and a path the repository itself listed.

use gpui::{div, list, prelude::*, px, AnyElement, Context, FontWeight, ListState, SharedString};
use serde_json::json;

use crate::state::changes::{same_path, RepositoryFile};
use crate::theme::{self, Colors};
use crate::ui::changes::diff::{self, prepare, DiffContent};
use crate::ui::changes::{DiffMode, Root};

/// The letter a changed file wears.
pub fn change_letter(change: &str) -> &'static str {
    match change {
        "added" => "A",
        "deleted" => "D",
        "renamed" => "R",
        _ => "M",
    }
}

fn name_of(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).to_owned()
}

fn folder_of(path: &str) -> String {
    match path.rfind('/') {
        Some(at) if at > 0 => path[..at].to_owned(),
        _ => String::new(),
    }
}

impl Root {
    pub(crate) fn render_changes_view(&mut self, colors: Colors, cx: &mut Context<Self>) -> AnyElement {
        // The first frame after opening: ask for the repository once.
        if let Some(review) = self.changes.review.as_mut() {
            if !review.asked_repository {
                review.asked_repository = true;
                let session_id = review.session_id.clone();
                self.request_repository(session_id, cx);
            }
        }
        // The selected file's diff, once per selection.
        let wanted = self
            .changes
            .review
            .as_ref()
            .and_then(|review| review.selected.clone())
            .filter(|selected| {
                self.changes.review.as_ref().is_some_and(|review| review.diff_path.as_deref() != Some(selected.as_str()))
            });
        if let Some(selected) = wanted {
            if let Some(review) = self.changes.review.as_mut() {
                review.diff_path = Some(selected.clone());
                review.diff = None;
                review.diff_error = None;
                let session_id = review.session_id.clone();
                self.daemon_command(
                    crate::daemon::Command::Rpc {
                        tag: "review-diff".to_owned(),
                        method: "session.git.diff".to_owned(),
                        params: json!({ "sessionId": session_id, "repositoryPath": selected }),
                    },
                    cx,
                );
            }
        }

        let Some(review) = self.changes.review.as_ref() else { return div().into_any_element() };
        let repository = review.repository.clone();
        let selected = review.selected.clone();
        let mode = review.mode;
        let failure = review.failure.clone();
        let busy = review.busy;
        let diff_text = review.diff.clone();
        let diff_error = review.diff_error.clone();
        let discarding = review.discarding.clone();
        let list_handle = review.list.clone();
        let comments = review.comments.clone();
        let sending = review.sending;
        let session_id = review.session_id.clone();
        let comment_input = review.comment_input.clone();
        let files = repository.as_ref().map(|repository| repository.files.clone()).unwrap_or_default();
        // The owner may have asked for a file by its absolute path (from the
        // rail) or by its repository path (from this screen).
        let root = repository.as_ref().and_then(|repository| repository.repo_root.clone());
        let current: Option<RepositoryFile> = selected
            .as_deref()
            .and_then(|selected| {
                files
                    .iter()
                    .find(|file| file.repository_path == selected || same_path(&file.path, selected, root.as_deref()))
            })
            .cloned();

        let mut header = div()
            .h(px(44.0))
            .flex_none()
            .px(px(14.0))
            .flex()
            .items_center()
            .gap(px(10.0))
            .border_b_1()
            .border_color(colors.line)
            .child(div().text_size(px(theme::FS_MD)).font_weight(FontWeight::SEMIBOLD).child("Changes"));
        if let Some(repository) = repository.as_ref() {
            let summary = format!(
                "{}{} · {} {} · +{} −{}",
                if repository.detached {
                    "Detached HEAD".to_owned()
                } else {
                    repository.branch.clone().unwrap_or_else(|| "Unknown branch".to_owned())
                },
                "",
                files.len(),
                if files.len() == 1 { "file" } else { "files" },
                repository.added_lines,
                repository.removed_lines
            );
            header = header.child(div().text_size(px(theme::FS_XS)).text_color(colors.text_3).child(SharedString::from(summary)));
        }
        header = header.child(div().flex_1());
        for option in [DiffMode::Unified, DiffMode::Split] {
            let active = mode == option;
            header = header.child(
                div()
                    .id(SharedString::from(format!("diff-mode-{:?}", option)))
                    .h(px(24.0))
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .rounded(px(theme::RADIUS_SM))
                    .bg(if active { colors.selected } else { gpui::rgba(0x00000000) })
                    .text_size(px(theme::FS_XS))
                    .text_color(if active { colors.text_1 } else { colors.text_2 })
                    .cursor_pointer()
                    .hover(move |style| style.bg(colors.hover))
                    .child(if option == DiffMode::Unified { "Unified" } else { "Side by side" })
                    .on_click(cx.listener(move |root, _: &gpui::ClickEvent, _window, cx| {
                        if let Some(review) = root.changes.review.as_mut() {
                            review.mode = option;
                            // The rows are different in each mode.
                            review.list = None;
                            review.list_rows = 0;
                        }
                        cx.notify();
                    })),
            );
        }
        header = header.child(
            div()
                .id("changes-close")
                .size(px(26.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(theme::RADIUS_SM))
                .cursor_pointer()
                .text_color(colors.text_2)
                .hover(move |style| style.bg(colors.hover))
                .child("✕")
                .on_click(cx.listener(|root, _: &gpui::ClickEvent, _window, cx| {
                    root.changes.review = None;
                    cx.notify();
                })),
        );

        let mut body: AnyElement = diff::empty_note("Reading the repository…", colors);
        if let Some(repository) = repository.as_ref() {
            if !repository.git_repo || repository.error.is_some() {
                body = diff::empty_note(
                    &repository.error.clone().unwrap_or_else(|| "This project is not a git repository.".to_owned()),
                    colors,
                );
            } else if files.is_empty() {
                body = diff::empty_note("No changes against the last commit.", colors);
            } else {
                // The file list.
                let mut left = div().w(px(280.0)).flex_none().flex().flex_col().gap(px(2.0)).border_r_1().border_color(colors.line).pr(px(8.0));
                let all_staged = files.iter().all(|file| file.staged == "all");
                let staged_count = files.iter().filter(|file| file.staged == "all").count();
                let paths: Vec<String> = files.iter().map(|file| file.repository_path.clone()).collect();
                let session = session_id.clone();
                left = left.child(
                    div()
                        .id("changes-stage-all")
                        .h(px(24.0))
                        .px(px(6.0))
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .rounded(px(theme::RADIUS_SM))
                        .text_size(px(theme::FS_XS))
                        .text_color(colors.text_2)
                        .cursor_pointer()
                        .hover(move |style| style.bg(colors.hover))
                        .child(if staged_count > 0 { format!("{staged_count} staged") } else { "Stage all".to_owned() })
                        .on_click(cx.listener(move |root, _: &gpui::ClickEvent, _window, cx| {
                            root.stage_paths(session.clone(), paths.clone(), !all_staged, cx);
                        })),
                );
                for file in &files {
                    let active = selected.as_deref() == Some(file.repository_path.as_str());
                    let path = file.repository_path.clone();
                    let count = comments.iter().filter(|(comment_path, _)| comment_path == &path).count();
                    left = left.child(
                        div()
                            .id(SharedString::from(format!("changes-file-{}", file.repository_path)))
                            .h(px(28.0))
                            .px(px(6.0))
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .rounded(px(theme::RADIUS_SM))
                            .bg(if active { colors.selected } else { gpui::rgba(0x00000000) })
                            .cursor_pointer()
                            .hover(move |style| style.bg(colors.hover))
                            .child(
                                div()
                                    .w(px(14.0))
                                    .text_size(px(theme::FS_2XS))
                                    .text_color(colors.text_3)
                                    .child(SharedString::from(change_letter(&file.change))),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .flex()
                                    .flex_col()
                                    .child(div().text_size(px(theme::FS_XS)).truncate().child(SharedString::from(name_of(&file.repository_path))))
                                    .when(!folder_of(&file.repository_path).is_empty(), |this| {
                                        this.child(
                                            div()
                                                .text_size(px(theme::FS_2XS))
                                                .text_color(colors.text_3)
                                                .truncate()
                                                .child(SharedString::from(folder_of(&file.repository_path))),
                                        )
                                    }),
                            )
                            .when(count > 0, |this| {
                                this.child(div().text_size(px(theme::FS_2XS)).text_color(colors.text_3).child(SharedString::from(count.to_string())))
                            })
                            .on_click(cx.listener(move |root, _: &gpui::ClickEvent, _window, cx| {
                                if let Some(review) = root.changes.review.as_mut() {
                                    review.selected = Some(path.clone());
                                }
                                cx.notify();
                            })),
                    );
                }

                // The right side: the file's own bar, its diff, and the comments.
                let mut right = div().flex_1().min_w(px(0.0)).flex().flex_col().gap(px(6.0)).pl(px(8.0));
                if let Some(file) = current.as_ref() {
                    let path = file.repository_path.clone();
                    let staged = file.staged == "all";
                    let session = session_id.clone();
                    let mut bar = div()
                        .h(px(28.0))
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(div().flex_1().min_w(px(0.0)).truncate().font_family(theme::MONO_FONT).text_size(px(theme::FS_XS)).child(SharedString::from(path.clone())));
                    if let Some(added) = file.added_lines.filter(|value| *value > 0) {
                        bar = bar.child(div().text_size(px(theme::FS_2XS)).text_color(colors.status_success).child(SharedString::from(format!("+{added}"))));
                    }
                    if let Some(removed) = file.removed_lines.filter(|value| *value > 0) {
                        bar = bar.child(div().text_size(px(theme::FS_2XS)).text_color(colors.status_danger).child(SharedString::from(format!("−{removed}"))));
                    }
                    bar = bar.child(
                        div()
                            .id("changes-stage-one")
                            .h(px(22.0))
                            .px(px(8.0))
                            .flex()
                            .items_center()
                            .rounded(px(theme::RADIUS_SM))
                            .bg(colors.raised)
                            .text_size(px(theme::FS_XS))
                            .text_color(colors.text_2)
                            .cursor_pointer()
                            .hover(move |style| style.bg(colors.selected))
                            .child(if staged { "Unstage" } else { "Stage" })
                            .on_click(cx.listener(move |root, _: &gpui::ClickEvent, _window, cx| {
                                root.stage_paths(session.clone(), vec![path.clone()], !staged, cx);
                            })),
                    );
                    if discarding.as_deref() == Some(file.repository_path.as_str()) {
                        let session = session_id.clone();
                        let target = file.clone();
                        bar = bar
                            .child(div().text_size(px(theme::FS_XS)).text_color(colors.status_danger).child("Discard?"))
                            .child(
                                div()
                                    .id("changes-keep")
                                    .h(px(22.0))
                                    .px(px(8.0))
                                    .flex()
                                    .items_center()
                                    .rounded(px(theme::RADIUS_SM))
                                    .text_size(px(theme::FS_XS))
                                    .text_color(colors.text_2)
                                    .cursor_pointer()
                                    .hover(move |style| style.bg(colors.hover))
                                    .child("Keep")
                                    .on_click(cx.listener(|root, _: &gpui::ClickEvent, _window, cx| {
                                        if let Some(review) = root.changes.review.as_mut() {
                                            review.discarding = None;
                                        }
                                        cx.notify();
                                    })),
                            )
                            .child(
                                div()
                                    .id("changes-discard-confirm")
                                    .h(px(22.0))
                                    .px(px(8.0))
                                    .flex()
                                    .items_center()
                                    .rounded(px(theme::RADIUS_SM))
                                    .bg(colors.status_danger)
                                    .text_color(colors.text_inverse)
                                    .text_size(px(theme::FS_XS))
                                    .cursor_pointer()
                                    .child("Discard")
                                    .on_click(cx.listener(move |root, _: &gpui::ClickEvent, _window, cx| {
                                        if let Some(review) = root.changes.review.as_mut() {
                                            review.busy = true;
                                        }
                                        root.daemon_command(
                                            crate::daemon::Command::Rpc {
                                                tag: "review-revert".to_owned(),
                                                method: "session.git.revert".to_owned(),
                                                params: json!({
                                                    "sessionId": session.clone(),
                                                    "repositoryPath": target.repository_path,
                                                    "confirm": true,
                                                }),
                                            },
                                            cx,
                                        );
                                        cx.notify();
                                    })),
                            );
                    } else {
                        let repository_path = file.repository_path.clone();
                        bar = bar.child(
                            div()
                                .id("changes-discard")
                                .h(px(22.0))
                                .px(px(8.0))
                                .flex()
                                .items_center()
                                .rounded(px(theme::RADIUS_SM))
                                .text_size(px(theme::FS_XS))
                                .text_color(colors.text_2)
                                .cursor_pointer()
                                .hover(move |style| style.bg(colors.hover))
                                .child("Discard")
                                .on_click(cx.listener(move |root, _: &gpui::ClickEvent, _window, cx| {
                                    if let Some(review) = root.changes.review.as_mut() {
                                        review.discarding = Some(repository_path.clone());
                                    }
                                    cx.notify();
                                })),
                        );
                    }
                    right = right.child(bar);
                }

                // The diff itself.
                let diff_body: AnyElement = if let Some(error) = diff_error.clone() {
                    diff::empty_note(&error, colors)
                } else if let Some(content) = diff_text.clone() {
                    let (parsed, prepared) = prepare(&content, mode);
                    let truncated = current.as_ref().is_some_and(|file| file.truncated);
                    match prepared {
                        DiffContent::Empty => diff::empty_note("There is no text diff for this file.", colors),
                        DiffContent::Binary => diff::empty_note("Binary file change. Text lines are unavailable.", colors),
                        DiffContent::Unified(lines) => {
                            let rows = lines.clone();
                            let list_state: ListState = list_handle.clone().unwrap_or_else(|| diff::diff_list(parsed.lines.len()));
                            let mut block = div().flex_1().min_h(px(0.0)).flex().flex_col();
                            if truncated {
                                block = block.child(diff::empty_note("This diff is too long to show in full.", colors));
                            }
                            block
                                .child(
                                    div().flex_1().min_h(px(0.0)).child(
                                        list(list_state, move |index, _window, _cx| diff::render_line(index, &rows, colors))
                                            .flex_1()
                                            .h_full(),
                                    ),
                                )
                                .into_any_element()
                        }
                        DiffContent::Split(rows, capped) => {
                            let mut column = div().flex().flex_col();
                            for row in rows.iter() {
                                column = column.child(match row {
                                    crate::state::changes::SideRow::Hunk(text) => div()
                                        .w_full()
                                        .bg(colors.selected)
                                        .font_family(theme::MONO_FONT)
                                        .text_size(px(theme::FS_XS))
                                        .text_color(colors.status_info)
                                        .child(SharedString::from(text.clone()))
                                        .into_any_element(),
                                    crate::state::changes::SideRow::Meta(text) => div()
                                        .w_full()
                                        .font_family(theme::MONO_FONT)
                                        .text_size(px(theme::FS_XS))
                                        .text_color(colors.text_3)
                                        .child(SharedString::from(text.clone()))
                                        .into_any_element(),
                                    crate::state::changes::SideRow::Pair { left, right } => div()
                                        .w_full()
                                        .flex()
                                        .child(diff::split_cell(left.as_ref(), false, colors))
                                        .child(diff::split_cell(right.as_ref(), true, colors))
                                        .into_any_element(),
                                });
                            }
                            div()
                                .id("changes-diff-split")
                                .flex_1()
                                .min_h(px(0.0))
                                .overflow_y_scroll()
                                .flex()
                                .flex_col()
                                .when(capped, |this| {
                                    this.child(diff::empty_note("Only the first part of this diff is shown.", colors))
                                })
                                .child(column)
                                .into_any_element()
                        }
                    }
                } else {
                    diff::empty_note("Loading the diff…", colors)
                };
                right = right.child(div().flex_1().min_h(px(0.0)).flex().child(diff_body));

                // Comments for the bot: per file, sent as one ordinary message.
                if let Some(input) = comment_input.clone() {
                    let mut bar = div().flex().items_center().gap(px(6.0));
                    bar = bar.child(
                        div()
                            .h(px(32.0))
                            .px(px(8.0))
                            .flex_1()
                            .min_w(px(0.0))
                            .flex()
                            .items_center()
                            .rounded(px(theme::RADIUS_SM))
                            .bg(colors.input)
                            .border_1()
                            .border_color(colors.line)
                            .child(div().flex_1().min_w(px(0.0)).child(input)),
                    );
                    let path = current.as_ref().map(|file| file.repository_path.clone()).unwrap_or_default();
                    bar = bar.child(
                        div()
                            .id("changes-add-comment")
                            .h(px(26.0))
                            .px(px(9.0))
                            .flex()
                            .items_center()
                            .rounded(px(theme::RADIUS_SM))
                            .bg(colors.raised)
                            .text_size(px(theme::FS_XS))
                            .text_color(colors.text_2)
                            .cursor_pointer()
                            .hover(move |style| style.bg(colors.selected))
                            .child("Add comment")
                            .on_click(cx.listener(move |root, _: &gpui::ClickEvent, _window, cx| {
                                root.add_review_comment(path.clone(), cx);
                            })),
                    );
                    if !comments.is_empty() {
                        bar = bar
                            .child(div().text_size(px(theme::FS_XS)).text_color(colors.text_3).child(SharedString::from(format!(
                                "{} {}",
                                comments.len(),
                                if comments.len() == 1 { "comment" } else { "comments" }
                            ))))
                            .child(
                                div()
                                    .id("changes-send-comments")
                                    .h(px(26.0))
                                    .px(px(9.0))
                                    .flex()
                                    .items_center()
                                    .rounded(px(theme::RADIUS_SM))
                                    .bg(colors.accent)
                                    .text_color(colors.text_inverse)
                                    .text_size(px(theme::FS_XS))
                                    .cursor_pointer()
                                    .child(if sending { "Sending…" } else { "Send to the bot" })
                                    .on_click(cx.listener(move |root, _: &gpui::ClickEvent, _window, cx| {
                                        root.send_review_comments(cx);
                                    })),
                            );
                    }
                    right = right.child(bar);
                }

                body = div().flex().flex_1().min_h(px(0.0)).overflow_hidden().child(left).child(right).into_any_element();
            }
        }

        let mut card = div()
            .id("changes-view")
            .w_full()
            .h_full()
            .flex()
            .flex_col()
            .bg(colors.overlay)
            .child(header);
        if let Some(failure) = failure {
            card = card.child(div().px(px(14.0)).py(px(6.0)).text_size(px(theme::FS_XS)).text_color(colors.status_danger).child(SharedString::from(failure)));
        }
        if busy {
            card = card.child(div().px(px(14.0)).text_size(px(theme::FS_2XS)).text_color(colors.text_3).child("Working…"));
        }
        card = card.child(div().flex_1().min_h(px(0.0)).px(px(14.0)).pb(px(12.0)).child(body));
        card.into_any_element()
    }

    /// `session.git.stage`: stage or unstage one or more paths.
    pub(crate) fn stage_paths(&mut self, session_id: String, paths: Vec<String>, staged: bool, cx: &mut Context<Self>) {
        if let Some(review) = self.changes.review.as_mut() {
            review.busy = true;
            review.failure = None;
        }
        self.daemon_command(
            crate::daemon::Command::Rpc {
                tag: "review-stage".to_owned(),
                method: "session.git.stage".to_owned(),
                params: json!({ "sessionId": session_id, "paths": paths, "staged": staged }),
            },
            cx,
        );
        cx.notify();
    }

    /// Add the typed comment to the selected file.
    pub(crate) fn add_review_comment(&mut self, path: String, cx: &mut Context<Self>) {
        let Some(review) = self.changes.review.as_ref() else { return };
        let Some(input) = review.comment_input.clone() else { return };
        let text = input.read(cx).text().trim().to_owned();
        if text.is_empty() || path.is_empty() {
            return;
        }
        if let Some(review) = self.changes.review.as_mut() {
            review.comments.push((path, text));
        }
        input.update(cx, |input, cx| input.set_text(String::new(), cx));
        cx.notify();
    }

    /// Send the comments as an ordinary message into the conversation, the way
    /// React's `sendReviewComments` does.
    pub(crate) fn send_review_comments(&mut self, cx: &mut Context<Self>) {
        let Some(review) = self.changes.review.as_mut() else { return };
        if review.sending || review.comments.is_empty() {
            return;
        }
        let lines: Vec<String> = review
            .comments
            .iter()
            .map(|(path, text)| format!("- `{path}`: {text}"))
            .collect();
        let message = format!("Comments on your changes:\n\n{}", lines.join("\n"));
        review.sending = true;
        review.comments.clear();
        let sent = self.dispatch_send(message, cx);
        if let Some(review) = self.changes.review.as_mut() {
            review.sending = false;
            if sent {
                review.failure = None;
            } else {
                review.failure = Some("The comments could not be sent. Open a conversation first.".to_owned());
            }
        }
        if sent {
            self.changes.review = None;
            self.toast("Comments sent to the bot.", cx);
        }
        cx.notify();
    }
}
