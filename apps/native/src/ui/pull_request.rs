//! The pull request dialog: the last look before a pull request. When this
//! computer's GitHub CLI can open it, the owner reads the drafted title and body
//! and confirms, and the daemon does it. Otherwise, or if they prefer, confirming
//! sends the conversation's coding agent an ordinary message.
//! Ported from `ui/PullRequestDialog.tsx`.

use gpui::{div, prelude::*, px, AnyElement, Context, FontWeight, SharedString};
use serde_json::json;

use crate::state::changes::{pull_request_plan, pull_request_prompt, pull_request_step_label};
use crate::theme::{self, Colors};
use crate::ui::root::Root;

impl Root {
    pub(crate) fn render_pull_request_dialog(&mut self, colors: Colors, cx: &mut Context<Self>) -> Option<AnyElement> {
        let session_id = self.changes.pull_request.as_ref()?.session_id.clone();
        let repository = self.changes.pull_request.as_ref()?.repository.clone();
        let draft = self.changes.pull_request.as_ref().and_then(|dialog| dialog.draft.clone());
        let created = self.changes.pull_request.as_ref().and_then(|dialog| dialog.created.clone());
        let ask_instead = self.changes.pull_request.as_ref().is_some_and(|dialog| dialog.ask_instead);
        let as_draft = self.changes.pull_request.as_ref().is_some_and(|dialog| dialog.as_draft);
        let commit_first = self.changes.pull_request.as_ref().is_some_and(|dialog| dialog.commit_first);
        let pending = self.changes.pull_request.as_ref().is_some_and(|dialog| dialog.pending);
        let failure = self.changes.pull_request.as_ref().and_then(|dialog| dialog.failure.clone());
        let title = self
            .changes
            .pull_request
            .as_ref()
            .and_then(|dialog| dialog.title_input.clone())
            .map(|input| input.read(cx).text().to_owned())
            .unwrap_or_default();
        let message = self
            .changes
            .pull_request
            .as_ref()
            .and_then(|dialog| dialog.message_input.clone())
            .map(|input| input.read(cx).text().to_owned())
            .unwrap_or_default();
        let body_input = self.changes.pull_request.as_ref().and_then(|dialog| dialog.body_input.clone());
        let title_input = self.changes.pull_request.as_ref().and_then(|dialog| dialog.title_input.clone());
        let message_input = self.changes.pull_request.as_ref().and_then(|dialog| dialog.message_input.clone());
        let bot = self.selected_agent().map(|agent| agent.name.clone()).unwrap_or_else(|| "The bot".to_owned());
        let root_path = repository.repo_root.clone().or_else(|| repository.project_path.clone()).unwrap_or_default();
        let folder = root_path.replace('\\', "/").trim_end_matches('/').rsplit('/').next().unwrap_or("this repository").to_owned();

        let mut card = div()
            .w(px(520.0))
            .max_h(px(620.0))
            .id("pr-dialog")
            .overflow_y_scroll()
            .p(px(16.0))
            .flex()
            .flex_col()
            .gap(px(10.0))
            .rounded(px(theme::RADIUS_LG))
            .bg(colors.overlay)
            .border_1()
            .border_color(colors.line_strong)
            ;

        let native = draft.as_ref().is_some_and(|draft| draft.ready) && draft.as_ref().is_some_and(|draft| draft.existing_url.is_none()) && !ask_instead;

        if let Some(created) = created {
            let failed = created.steps.iter().any(|step| step.status == "failed");
            card = card.child(
                div()
                    .text_size(px(theme::FS_LG))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(if failed { "Not everything worked" } else { "Pull request opened" }),
            );
            for step in &created.steps {
                let (label, tone) = match step.status.as_str() {
                    "done" => ("✓", colors.status_success),
                    "failed" => ("!", colors.status_danger),
                    _ => ("–", colors.text_3),
                };
                card = card.child(
                    div()
                        .flex()
                        .items_start()
                        .gap(px(8.0))
                        .text_size(px(theme::FS_XS))
                        .child(div().text_color(tone).child(SharedString::from(label.to_owned())))
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.0))
                                .flex()
                                .flex_col()
                                .child(div().text_color(colors.text_2).child(SharedString::from(pull_request_step_label(&step.id))))
                                .when_some(step.detail.clone(), |this, detail| {
                                    this.child(div().text_size(px(theme::FS_2XS)).text_color(colors.text_3).child(SharedString::from(detail)))
                                }),
                        ),
                );
            }
            let url = created.url.clone();
            let mut actions = div().flex().items_center().gap(px(6.0)).child(div().flex_1());
            if let Some(url) = url {
                actions = actions.child(self.ghost_button("Open on GitHub", "pr-open-url", colors, cx, move |root, cx| {
                    root.open_link(url.clone(), cx);
                }));
            }
            actions = actions.child(self.primary_button("Done", "pr-done", colors, cx, |root, cx| {
                root.changes.pull_request = None;
                cx.notify();
            }));
            card = card.child(actions);
        } else if native {
            let draft = draft.clone().unwrap_or_default();
            card = card
                .child(div().text_size(px(theme::FS_LG)).font_weight(FontWeight::SEMIBOLD).child("Create a pull request"))
                .child(self.pr_field("Title", title_input, 30.0, colors))
                .child(self.pr_field("Description", body_input, 90.0, colors));
            if draft.uncommitted > 0 {
                let checked = commit_first;
                card = card.child(
                    div()
                        .id("pr-commit-first")
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .text_size(px(theme::FS_XS))
                        .text_color(colors.text_2)
                        .cursor_pointer()
                        .child(SharedString::from(format!(
                            "{} Commit {} uncommitted {} first",
                            if checked { "[x]" } else { "[ ]" },
                            draft.uncommitted,
                            if draft.uncommitted == 1 { "file" } else { "files" }
                        )))
                        .on_click(cx.listener(|root, _: &gpui::ClickEvent, _window, cx| {
                            if let Some(dialog) = root.changes.pull_request.as_mut() {
                                dialog.commit_first = !dialog.commit_first;
                            }
                            cx.notify();
                        })),
                );
                if commit_first {
                    card = card.child(self.pr_field("Commit message", message_input, 30.0, colors));
                }
            }
            let checked = as_draft;
            card = card
                .child(
                    div()
                        .id("pr-as-draft")
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .text_size(px(theme::FS_XS))
                        .text_color(colors.text_2)
                        .cursor_pointer()
                        .child(SharedString::from(format!("{} Open it as a draft", if checked { "[x]" } else { "[ ]" })))
                        .on_click(cx.listener(|root, _: &gpui::ClickEvent, _window, cx| {
                            if let Some(dialog) = root.changes.pull_request.as_mut() {
                                dialog.as_draft = !dialog.as_draft;
                            }
                            cx.notify();
                        })),
                )
                .child(
                    div()
                        .p(px(8.0))
                        .flex()
                        .flex_col()
                        .gap(px(2.0))
                        .rounded(px(theme::RADIUS_SM))
                        .bg(colors.panel)
                        .text_size(px(theme::FS_XS))
                        .text_color(colors.text_2)
                        .child(div().font_weight(FontWeight::MEDIUM).child("This will:"))
                        .children(pull_request_plan(&draft, commit_first, as_draft).into_iter().enumerate().map(|(index, line)| {
                            div().child(SharedString::from(format!("{}. {line}", index + 1)))
                        })),
                );
            if draft.uncommitted > 0 && !commit_first {
                card = card.child(note(colors, "The uncommitted files stay on your computer and aren't part of this."));
            }
            let needs_message = draft.uncommitted > 0 && commit_first;
            let can_create = !pending && !title.trim().is_empty() && (!needs_message || !message.trim().is_empty());
            let mut actions = div().flex().items_center().gap(px(6.0)).child(div().flex_1());
            actions = actions
                .child(self.ghost_button("Ask the bot instead", "pr-ask", colors, cx, |root, cx| {
                    if let Some(dialog) = root.changes.pull_request.as_mut() {
                        dialog.ask_instead = true;
                    }
                    cx.notify();
                }))
                .child(self.ghost_button("Cancel", "pr-cancel", colors, cx, |root, cx| {
                    root.changes.pull_request = None;
                    cx.notify();
                }));
            if can_create {
                let base = draft.base.clone();
                let session = session_id.clone();
                let commit_message = needs_message.then(|| message.trim().to_owned());
                actions = actions.child(self.primary_button(
                    if pending { "Creating…" } else { "Create pull request" },
                    "pr-create",
                    colors,
                    cx,
                    move |root, cx| {
                        let title = root
                            .changes
                            .pull_request
                            .as_ref()
                            .and_then(|dialog| dialog.title_input.clone())
                            .map(|input| input.read(cx).text().trim().to_owned())
                            .unwrap_or_default();
                        let body = root
                            .changes
                            .pull_request
                            .as_ref()
                            .and_then(|dialog| dialog.body_input.clone())
                            .map(|input| input.read(cx).text().to_owned())
                            .unwrap_or_default();
                        if let Some(dialog) = root.changes.pull_request.as_mut() {
                            dialog.pending = true;
                            dialog.failure = None;
                        }
                        root.daemon_command(
                            crate::daemon::Command::Rpc {
                                tag: "pr-create".to_owned(),
                                method: "session.pr.create".to_owned(),
                                params: json!({
                                    "sessionId": session,
                                    "title": title,
                                    "body": body,
                                    "base": base,
                                    "draft": as_draft,
                                    "commitMessage": commit_message,
                                    "confirm": true,
                                }),
                            },
                            cx,
                        );
                        cx.notify();
                    },
                ));
            } else {
                actions = actions.child(
                    div()
                        .h(px(28.0))
                        .px(px(12.0))
                        .flex()
                        .items_center()
                        .rounded(px(theme::RADIUS_SM))
                        .bg(colors.raised)
                        .text_color(colors.text_3)
                        .text_size(px(theme::FS_XS))
                        .child("Create pull request"),
                );
            }
            card = card.child(actions);
        } else {
            let reason = draft.as_ref().filter(|draft| !draft.ready).and_then(|draft| draft.reason.clone());
            let existing = draft.as_ref().and_then(|draft| draft.existing_url.clone()).filter(|_| !ask_instead);
            card = card
                .child(div().text_size(px(theme::FS_LG)).font_weight(FontWeight::SEMIBOLD).child("Create a pull request?"))
                .child(div().text_color(colors.text_2).child(SharedString::from(format!(
                    "{bot} will look at these changes, commit the ones that belong together, push the branch and open a pull request with the tools installed on this computer. It sends you the link, or says what stopped it."
                ))));
            if let Some(url) = existing {
                let mut row = div().flex().items_center().gap(px(6.0)).text_size(px(theme::FS_XS)).text_color(colors.text_2);
                row = row.child("This branch already has an open pull request.");
                row = row.child(self.ghost_button("Open it on GitHub", "pr-existing", colors, cx, move |root, cx| {
                    root.open_link(url.clone(), cx);
                }));
                card = card.child(row);
            }
            if let Some(reason) = reason {
                card = card.child(note(colors, format!("Bitbots can't open this one itself: {reason}")));
            }
            // The facts the owner is agreeing to.
            let mut facts: Vec<(String, String)> = Vec::new();
            facts.push(("Repository".to_owned(), folder.clone()));
            facts.push((
                "Branch".to_owned(),
                if repository.detached {
                    "Detached HEAD".to_owned()
                } else {
                    repository.branch.clone().unwrap_or_else(|| "Unknown".to_owned())
                },
            ));
            facts.push((
                "Changes".to_owned(),
                format!(
                    "{} {} · +{} −{}",
                    repository.files.len(),
                    if repository.files.len() == 1 { "file" } else { "files" },
                    repository.added_lines,
                    repository.removed_lines
                ),
            ));
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
            card = card.child(facts_block).child(note(
                colors,
                format!("These are all the uncommitted changes in the repository, not only this conversation's. {bot} commits only what it judges relevant and tells you what it left out."),
            ));
            let prompt_repository = repository.clone();
            let mut actions = div().flex().items_center().gap(px(6.0)).child(div().flex_1());
            actions = actions
                .child(self.ghost_button("Cancel", "pr-cancel-ask", colors, cx, |root, cx| {
                    root.changes.pull_request = None;
                    cx.notify();
                }))
                .child(self.primary_button(if pending { "Sending…" } else { "Ask the bot to create it" }, "pr-send", colors, cx, move |root, cx| {
                    if let Some(dialog) = root.changes.pull_request.as_mut() {
                        dialog.pending = true;
                        dialog.failure = None;
                    }
                    let prompt = pull_request_prompt(&prompt_repository);
                    let sent = root.dispatch_send(prompt, cx);
                    if sent {
                        root.changes.pull_request = None;
                        root.toast("Asked the bot for a pull request.", cx);
                    } else if let Some(dialog) = root.changes.pull_request.as_mut() {
                        dialog.pending = false;
                        dialog.failure = Some("The request could not be sent. Open a conversation first.".to_owned());
                    }
                    cx.notify();
                }));
            card = card.child(actions);
        }

        if let Some(failure) = failure {
            card = card.child(div().text_size(px(theme::FS_XS)).text_color(colors.status_danger).child(SharedString::from(failure)));
        }

        Some(
            div()
                .id("pr-backdrop")
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .bg(colors.scrim)
                .on_mouse_down(gpui::MouseButton::Left, cx.listener(|root, _: &gpui::MouseDownEvent, _window, cx| {
                    root.changes.pull_request = None;
                    cx.notify();
                }))
                .child(
                    div()
                        .on_mouse_down(gpui::MouseButton::Left, cx.listener(|_, _: &gpui::MouseDownEvent, _window, cx| {
                            cx.stop_propagation();
                        }))
                        .child(card),
                )
                .into_any_element(),
        )
    }

    fn pr_field(
        &self,
        label: &str,
        input: Option<gpui::Entity<crate::ui::text_input::TextInput>>,
        height: f32,
        colors: Colors,
    ) -> AnyElement {
        let mut column = div()
            .flex()
            .flex_col()
            .gap(px(3.0))
            .child(div().text_size(px(theme::FS_XS)).text_color(colors.text_3).child(SharedString::from(label.to_owned())));
        if let Some(input) = input {
            column = column.child(
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
            );
        }
        column.into_any_element()
    }
}

fn note(colors: Colors, text: impl Into<String>) -> AnyElement {
    div().text_size(px(theme::FS_2XS)).text_color(colors.text_3).child(SharedString::from(text.into())).into_any_element()
}

