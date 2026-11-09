//! Answering what a bot asks: the approval card with every choice it offers
//! (`ui/ApprovalCard.tsx`, `ui/PermissionChoiceButton.tsx`,
//! `ui/permissionChoices.ts`) and the bot's own questions with their options,
//! multi-select and free text (`QuestionCard` in `ui/TeamMessages.tsx`).
//!
//! The card shows the tool, its command and — for an edit — the change the tool
//! proposed, because the daemon puts that in the request's own detail. "Always
//! allow" is offered only when the daemon marked the request rememberable, and it
//! can be remembered for this project alone or for every project.

use std::collections::BTreeMap;

use gpui::{div, prelude::*, px, AnyElement, Context, FontWeight, SharedString, Window};

use crate::art::{self, Glyph};
use crate::state::{approval_shortcuts, permission_choice_label, Permission, UserQuestion};
use crate::theme::{self, Colors};
use crate::ui::composer::question_answers;
use crate::ui::root::Root;

/// "in 4 min", "in 30 s", "now" for an absolute deadline (`expiresIn`).
pub fn expires_in(deadline_ms: i64, now_ms: i64) -> String {
    let seconds = (deadline_ms - now_ms) / 1000;
    if seconds <= 0 {
        return "now".to_owned();
    }
    if seconds < 90 {
        return format!("in {seconds} s");
    }
    format!("in {} min", (seconds + 30) / 60)
}

impl Root {
    /// The approval card: what was asked, the command with a Copy button, and one
    /// button per choice the daemon offered.
    pub(crate) fn render_approval(&mut self, permission: &Permission, colors: Colors, cx: &mut Context<Self>) -> AnyElement {
        let title = permission.title.clone().unwrap_or_else(|| "Approval requested".to_owned());
        let body = permission.body();
        let command = permission.command.clone().filter(|command| !command.trim().is_empty());
        let tool = permission.tool.clone().filter(|tool| !tool.trim().is_empty());
        let (allow, _deny) = approval_shortcuts(&permission.choices);
        let rememberable = permission.rememberable && allow.is_some();
        let remember_label = permission.remember_label.clone().unwrap_or_default();
        let project = permission.project_name.clone().unwrap_or_default();
        let in_project = !project.is_empty();
        let deadline = permission
            .expires_at
            .as_deref()
            .and_then(|stamp| chrono::DateTime::parse_from_rfc3339(stamp).ok())
            .map(|time| time.timestamp_millis());

        let mut head = div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .child(art::glyph(Glyph::SHIELD, 14.0, colors.status_working))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .truncate()
                    .text_size(px(theme::FS_SM))
                    .font_weight(FontWeight::MEDIUM)
                    .child(SharedString::from(title.clone())),
            );
        if let Some(tool) = tool.clone() {
            head = head.child(
                div()
                    .px(px(6.0))
                    .rounded(px(4.0))
                    .bg(colors.raised)
                    .text_size(px(theme::FS_2XS))
                    .text_color(colors.text_3)
                    .child(SharedString::from(tool)),
            );
        }
        if let Some(deadline) = deadline {
            head = head.child(
                div()
                    .text_size(px(theme::FS_2XS))
                    .text_color(colors.text_3)
                    .child(SharedString::from(format!("Expires {}", expires_in(deadline, crate::state::quota::now_ms())))),
            );
        }

        let diff = self.approval_diff(permission);
        let mut card = div()
            .mx(px(12.0))
            .mb(px(8.0))
            .p(px(10.0))
            .flex()
            .flex_col()
            .gap(px(6.0))
            .rounded(px(theme::RADIUS_MD))
            .bg(colors.raised)
            .border_1()
            .border_color(colors.line_strong)
            .child(head);
        if !body.is_empty() {
            card = card.child(div().text_size(px(theme::FS_XS)).text_color(colors.text_2).whitespace_normal().child(SharedString::from(body)));
        }
        if let Some(command) = command {
            let copy = command.clone();
            card = card.child(
                div()
                    .flex()
                    .items_start()
                    .gap(px(6.0))
                    .rounded(px(theme::RADIUS_SM))
                    .bg(colors.panel)
                    .p(px(7.0))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .font_family(theme::MONO_FONT)
                            .text_size(px(theme::FS_XS))
                            .text_color(colors.text_2)
                            .child(SharedString::from(command)),
                    )
                    .child(
                        div()
                            .id(SharedString::from(format!("copy-command-{}", permission.id)))
                            .size(px(20.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(4.0))
                            .cursor_pointer()
                            .hover(move |style| style.bg(colors.hover))
                            .child(art::glyph(Glyph::COPY, 11.0, colors.text_3))
                            .on_click(cx.listener(move |root, _: &gpui::ClickEvent, _window, cx| {
                                root.copy_to_clipboard(copy.clone(), cx);
                            })),
                    ),
            );
        }

        // An edit the daemon described with its own diff is shown as one, so the
        // owner sees what changes before allowing it.
        if let Some(diff) = diff {
            let mut block = div()
                .id(SharedString::from(format!("diff-{}", permission.id)))
                .max_h(px(220.0))
                .overflow_y_scroll()
                .rounded(px(theme::RADIUS_SM))
                .bg(colors.panel)
                .p(px(7.0))
                .font_family(theme::MONO_FONT)
                .text_size(px(theme::FS_2XS));
            for line in diff.lines().take(200) {
                let color = if line.starts_with('+') {
                    colors.status_success
                } else if line.starts_with('-') {
                    colors.status_danger
                } else {
                    colors.text_2
                };
                block = block.child(div().text_color(color).child(SharedString::from(line.to_owned())));
            }
            card = card.child(block);
        }

        let mut buttons = div().flex().items_center().gap(px(6.0)).flex_wrap();
        for choice in &permission.choices {
            let is_allow = allow.as_deref() == Some(choice.as_str());
            let permission_id = permission.id.clone();
            let value = choice.clone();
            buttons = buttons.child(
                div()
                    .id(SharedString::from(format!("choice-{}-{choice}", permission.id)))
                    .h(px(26.0))
                    .px(px(10.0))
                    .flex()
                    .items_center()
                    .gap(px(5.0))
                    .rounded(px(theme::RADIUS_SM))
                    .bg(if is_allow { colors.accent } else { colors.panel })
                    .border_1()
                    .border_color(colors.line)
                    .text_color(if is_allow { colors.text_inverse } else { colors.text_1 })
                    .text_size(px(theme::FS_XS))
                    .cursor_pointer()
                    .hover(move |style| style.bg(if is_allow { colors.selected_strong } else { colors.selected }))
                    .child(SharedString::from(permission_choice_label(choice)))
                    .on_click(cx.listener(move |root, _: &gpui::ClickEvent, _window, cx| {
                        root.reply_permission(permission_id.clone(), value.clone(), cx);
                    })),
            );
            // "Always allow" is offered only for an allow choice the daemon said
            // it can remember.
            if is_allow && rememberable {
                let label = if !in_project {
                    "Always allow".to_owned()
                } else {
                    format!("Always in {project}")
                };
                let id = permission.id.clone();
                let value = choice.clone();
                buttons = buttons.child(
                    div()
                        .id(SharedString::from(format!("always-{}", permission.id)))
                        .h(px(26.0))
                        .px(px(10.0))
                        .flex()
                        .items_center()
                        .rounded(px(theme::RADIUS_SM))
                        .border_1()
                        .border_color(colors.line)
                        .text_size(px(theme::FS_XS))
                        .text_color(colors.text_2)
                        .cursor_pointer()
                        .hover(move |style| style.bg(colors.selected))
                        .child(SharedString::from(label.clone()))
                        .on_click(cx.listener(move |root, _: &gpui::ClickEvent, _window, cx| {
                            root.reply_permission_remembered(id.clone(), value.clone(), in_project, cx);
                        })),
                );
                if in_project {
                    let id = permission.id.clone();
                    let value = choice.clone();
                    buttons = buttons.child(
                        div()
                            .id(SharedString::from(format!("always-all-{}", permission.id)))
                            .h(px(26.0))
                            .px(px(10.0))
                            .flex()
                            .items_center()
                            .rounded(px(theme::RADIUS_SM))
                            .border_1()
                            .border_color(colors.line)
                            .text_size(px(theme::FS_XS))
                            .text_color(colors.text_2)
                            .cursor_pointer()
                            .hover(move |style| style.bg(colors.selected))
                            .child("Always, every project")
                            .on_click(cx.listener(move |root, _: &gpui::ClickEvent, _window, cx| {
                                root.reply_permission_remembered(id.clone(), value.clone(), false, cx);
                            })),
                    );
                }
                if !remember_label.is_empty() {
                    card = card.child(
                        div()
                            .text_size(px(theme::FS_2XS))
                            .text_color(colors.text_3)
                            .child(SharedString::from(format!("Remembering: {remember_label}"))),
                    );
                }
            }
        }
        card.child(buttons).into_any_element()
    }

    /// A bot's own question card: each question with its lettered options, a box
    /// for the owner's own answer, and one Send for all of them.
    pub(crate) fn render_question(&mut self, permission: &Permission, colors: Colors, cx: &mut Context<Self>) -> AnyElement {
        let questions = permission.questions.clone();
        let single = questions.len() == 1 && !questions[0].multi_select;
        let chosen = self.question_choices.clone();
        let mut card = div()
            .mx(px(12.0))
            .mb(px(8.0))
            .p(px(10.0))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .rounded(px(theme::RADIUS_MD))
            .bg(colors.raised)
            .border_1()
            .border_color(colors.line_strong)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .flex_1()
                            .truncate()
                            .text_size(px(theme::FS_SM))
                            .font_weight(FontWeight::MEDIUM)
                            .child(SharedString::from(permission.title.clone().unwrap_or_else(|| "A question".to_owned()))),
                    )
                    .child(
                        div()
                            .id(SharedString::from(format!("dismiss-question-{}", permission.id)))
                            .size(px(20.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(4.0))
                            .cursor_pointer()
                            .hover(move |style| style.bg(colors.hover))
                            .child(art::glyph(Glyph::X, 11.0, colors.text_3))
                            .on_click(cx.listener({
                                let id = permission.id.clone();
                                move |root, _: &gpui::ClickEvent, _window, cx| {
                                    root.question_choices.clear();
                                    root.question_other.clear();
                                    root.reply_question(id.clone(), None, cx);
                                }
                            })),
                    ),
            );
        for question in &questions {
            let mut block = div()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(div().text_size(px(theme::FS_SM)).child(SharedString::from(question.question.clone())));
            if !question.header.is_empty() && question.header != question.question {
                block = block.child(div().text_size(px(theme::FS_2XS)).text_color(colors.text_3).child(SharedString::from(question.header.clone())));
            }
            let mut options = div().flex().flex_wrap().gap(px(4.0));
            for (index, option) in question.options.iter().enumerate() {
                let selected = chosen.get(&question.id).map(|values| values.iter().any(|value| value == &option.label)).unwrap_or(false);
                let id = permission.id.clone();
                let question_id = question.id.clone();
                let label = option.label.clone();
                let multi = question.multi_select;
                let mut button = div()
                    .id(SharedString::from(format!("option-{}-{}-{index}", permission.id, question.id)))
                    .px(px(8.0))
                    .py(px(4.0))
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .rounded(px(theme::RADIUS_SM))
                    .bg(if selected { colors.selected } else { colors.panel })
                    .border_1()
                    .border_color(if selected { colors.line_strong } else { colors.line })
                    .text_size(px(theme::FS_XS))
                    .cursor_pointer()
                    .hover(move |style| style.bg(colors.hover))
                    .child(
                        div()
                            .w(px(14.0))
                            .text_size(px(theme::FS_2XS))
                            .text_color(colors.text_3)
                            .child(SharedString::from(((b'A' + index as u8) as char).to_string())),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .child(div().font_weight(FontWeight::MEDIUM).child(SharedString::from(option.label.clone())))
                            .when_some(option.description.clone(), |this, description| {
                                this.child(div().text_size(px(theme::FS_2XS)).text_color(colors.text_3).child(SharedString::from(description)))
                            }),
                    );
                if selected {
                    button = button.child(art::glyph(Glyph::CHECK, 11.0, colors.text_1));
                }
                let answer = question.clone();
                options = options.child(button.on_click(cx.listener(move |root, _: &gpui::ClickEvent, _window, cx| {
                    let values = root.question_choices.entry(question_id.clone()).or_default();
                    if multi {
                        if let Some(index) = values.iter().position(|value| value == &label) {
                            values.remove(index);
                        } else {
                            values.push(label.clone());
                        }
                    } else {
                        *values = vec![label.clone()];
                    }
                    let answers = question_answers(std::slice::from_ref(&answer), &root.question_choices, &root.question_other);
                    if single {
                        root.question_choices.clear();
                        root.question_other.clear();
                        root.reply_question(id.clone(), answers, cx);
                    } else {
                        cx.notify();
                    }
                })));
            }
            block = block.child(options);
            if question.allow_other {
                let response = self.question_other.get(&question.id).cloned().unwrap_or_default();
                let question_id = question.id.clone();
                let id = permission.id.clone();
                let answer = question.clone();
                block = block.child(
                    div()
                        .id(SharedString::from(format!("other-{}", question.id)))
                        .h(px(26.0))
                        .px(px(8.0))
                        .flex()
                        .items_center()
                        .rounded(px(theme::RADIUS_SM))
                        .bg(colors.input)
                        .border_1()
                        .border_color(colors.line)
                        .cursor_pointer()
                        .text_size(px(theme::FS_XS))
                        .text_color(if response.is_empty() { colors.text_3 } else { colors.text_1 })
                        .child(SharedString::from(if response.is_empty() {
                            "Type your own answer".to_owned()
                        } else {
                            response.clone()
                        }))
                        .on_click(cx.listener(move |root, _: &gpui::ClickEvent, window: &mut Window, cx| {
                            // The box is the composer: an answer typed there is
                            // sent as this question's own words.
                            root.start_question_answer(id.clone(), question_id.clone());
                            let _ = &answer;
                            root.focus_composer(window, cx);
                            cx.notify();
                        })),
                );
            }
            card = card.child(block);
        }
        if !single {
            let answers = question_answers(&questions, &self.question_choices, &self.question_other);
            let id = permission.id.clone();
            card = card.child(
                div().flex().justify_end().child(
                    div()
                        .id(SharedString::from(format!("send-answers-{}", permission.id)))
                        .h(px(26.0))
                        .px(px(10.0))
                        .flex()
                        .items_center()
                        .rounded(px(theme::RADIUS_SM))
                        .bg(if answers.is_some() { colors.accent } else { colors.panel })
                        .text_color(if answers.is_some() { colors.text_inverse } else { colors.text_3 })
                        .text_size(px(theme::FS_XS))
                        .font_weight(FontWeight::MEDIUM)
                        .when(answers.is_some(), |this| this.cursor_pointer().hover(move |style| style.bg(colors.selected_strong)))
                        .child("Send answers")
                        .when_some(answers, |this, answers| {
                            this.on_click(cx.listener(move |root, _: &gpui::ClickEvent, _window, cx| {
                                root.question_choices.clear();
                                root.question_other.clear();
                                root.reply_question(id.clone(), Some(answers.clone()), cx);
                            }))
                        }),
                ),
            );
        }
        card.into_any_element()
    }

    /// The bot's questions, for the answer the composer is holding.
    pub(crate) fn question_list(&self) -> Vec<UserQuestion> {
        self.question_target
            .as_ref()
            .and_then(|(permission_id, _)| {
                self.snapshot()
                    .and_then(|snapshot| snapshot.permissions.iter().find(|permission| &permission.id == permission_id))
                    .map(|permission| permission.questions.clone())
            })
            .unwrap_or_default()
    }

    /// The owner chose to type an answer: the composer's next send is that answer.
    pub(crate) fn start_question_answer(&mut self, permission_id: String, question_id: String) {
        self.question_target = Some((permission_id, question_id));
    }

    /// If a question's answer is being typed, `text` is that answer rather than a
    /// message. The box is emptied by whoever holds it.
    pub(crate) fn send_question_answer(&mut self, text: &str, cx: &mut Context<Self>) -> bool {
        let Some((permission_id, question_id)) = self.question_target.clone() else { return false };
        let text = text.trim().to_owned();
        if text.is_empty() {
            return false;
        }
        let questions = self.question_list();
        self.question_target = None;
        let mut chosen: BTreeMap<String, Vec<String>> = BTreeMap::new();
        chosen.insert(question_id, vec![text]);
        let answers = question_answers(&questions, &chosen, &BTreeMap::new());
        self.reply_question(permission_id, answers, cx);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_deadline_reads_as_the_time_left() {
        let now = 1_000_000_000_000;
        assert_eq!(expires_in(now + 30_000, now), "in 30 s");
        assert_eq!(expires_in(now + 4 * 60_000, now), "in 4 min");
        assert_eq!(expires_in(now - 1_000, now), "now");
    }
}
