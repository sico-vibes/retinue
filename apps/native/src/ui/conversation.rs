//! The conversation pane: the header, the message list, the approval card and
//! the composer. The reading and sending path is ported from
//! `apps/desktop/src/ui/App.tsx` (`MessageItem`, `sendPrompt`) and the markdown
//! subset from `ui/SafeMarkdown.tsx`.
//!
//! The list is GPUI's `list` with bottom alignment, so a long conversation stays
//! smooth and the view sticks to the bottom while text streams.

use std::rc::Rc;

use gpui::{
    div, prelude::*, px, AnyElement, Context, FontStyle, FontWeight, HighlightStyle, Rgba,
    SharedString, StyledText,
};

use crate::state::{Permission, Session};
use crate::theme::{self, Colors};
use crate::ui::markdown::{parse_blocks, parse_inline, Block, Inline};
use crate::ui::root::Root;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Row {
    /// The owner's own message: a bubble on the right.
    User(String),
    /// The bot's reply: markdown, plain.
    Assistant(String),
    /// A notice from the daemon, in one muted line.
    Notice(String),
    /// One tool step, in one muted line.
    Tool(String),
}

/// The rows the list shows: messages and tool steps in time order, thinking left
/// out, exactly the order a reload gives.
pub fn build_rows(session: &Session) -> Vec<Row> {
    let mut entries: Vec<(String, usize, Row)> = Vec::new();
    let mut order = 0;
    for message in &session.messages {
        order += 1;
        if message.is_thinking() {
            continue;
        }
        let text = message.text().to_owned();
        let row = match message.role() {
            "user" => Row::User(text),
            "notice" => Row::Notice(text),
            "assistant" | "" => {
                if text.trim().is_empty() {
                    continue;
                }
                Row::Assistant(text)
            }
            _ => Row::Notice(text),
        };
        entries.push((message.created_at.clone().unwrap_or_else(|| "9999".to_owned()), order, row));
    }
    for tool in &session.tools {
        order += 1;
        entries.push((tool.started_at.clone().unwrap_or_else(|| "9999".to_owned()), order, Row::Tool(tool.summary())));
    }
    entries.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    entries.into_iter().map(|(_, _, row)| row).collect()
}

pub fn row_count(session: &Session) -> usize {
    build_rows(session).len()
}

impl Root {
    pub(crate) fn render_conversation_pane(&mut self, colors: Colors, cx: &mut Context<Self>) -> AnyElement {
        let agent_name = self.selected_agent().map(|agent| agent.name.clone());
        let session = self.selected_session().cloned();
        let permission = session
            .as_ref()
            .and_then(|session| self.snapshot().and_then(|snapshot| snapshot.pending_permission(&session.id)).cloned());
        let approvals_waiting = permission.is_some();

        let header = {
            let project = session
                .as_ref()
                .and_then(|session| session.project_name.clone())
                .or_else(|| self.selected_session().and_then(|session| self.snapshot().and_then(|snapshot| snapshot.project_name(session.project_id.as_deref()).map(str::to_owned))));
            let busy = session.as_ref().map(Session::is_busy).unwrap_or(false);
            let mut row = div()
                .h(px(44.0))
                .flex_none()
                .px(px(14.0))
                .flex()
                .items_center()
                .gap(px(6.0))
                .border_b_1()
                .border_color(colors.line)
                .child(
                    div()
                        .text_size(px(theme::FS_SM))
                        .font_weight(FontWeight::MEDIUM)
                        .child(SharedString::from(agent_name.clone().unwrap_or_else(|| "No employee".to_owned()))),
                );
            if let Some(project) = project {
                row = row
                    .child(div().text_color(colors.text_3).child(" / "))
                    .child(div().text_size(px(theme::FS_SM)).text_color(colors.text_2).child(SharedString::from(project)));
            }
            row = row.child(div().flex_1());
            if busy {
                row = row.child(
                    div()
                        .id("stop-turn")
                        .h(px(26.0))
                        .px(px(10.0))
                        .flex()
                        .items_center()
                        .rounded(px(theme::RADIUS_SM))
                        .border_1()
                        .border_color(colors.line_strong)
                        .text_size(px(theme::FS_XS))
                        .text_color(colors.text_2)
                        .cursor_pointer()
                        .hover(|style| style.text_color(colors.text_1).bg(colors.hover))
                        .child("Stop")
                        .on_click(cx.listener(|root, _: &gpui::ClickEvent, _window, cx| root.stop_turn(cx))),
                );
            }
            row
        };

        let body: AnyElement = match (&session, agent_name.as_ref()) {
            (Some(session), _) => {
                let rows = Rc::new(build_rows(session));
                let columns = colors;
                div()
                    .flex_1()
                    .w_full()
                    .flex()
                    .overflow_hidden()
                    .child(gpui::list(self.list.clone(), move |index, _window, _cx| {
                        let row = rows.get(index).cloned().unwrap_or(Row::Notice(String::new()));
                        render_row(&row, columns)
                    }).flex_1().h_full())
                    .into_any_element()
            }
            (None, Some(_)) => div()
                .flex_1()
                .w_full()
                .flex()
                .items_center()
                .justify_center()
                .child(div().text_color(colors.text_3).text_size(px(theme::FS_MD)).child("Start a conversation"))
                .into_any_element(),
            (None, None) => div()
                .flex_1()
                .w_full()
                .flex()
                .items_center()
                .justify_center()
                .child(div().text_color(colors.text_3).text_size(px(theme::FS_MD)).child("No employees yet"))
                .into_any_element(),
        };

        let mut pane = div()
            .flex_1()
            .h_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(header)
            .child(body);

        if let Some(permission) = permission {
            pane = pane.child(self.render_approval(&permission, colors, cx));
        }
        pane.child(self.render_composer(colors, session.as_ref(), approvals_waiting, cx)).into_any_element()
    }

    fn render_approval(&mut self, permission: &Permission, colors: Colors, cx: &mut Context<Self>) -> AnyElement {
        let title = permission.title.clone().unwrap_or_else(|| "Approval needed".to_owned());
        let detail = permission.detail.clone().unwrap_or_default();
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
            .child(div().text_size(px(theme::FS_SM)).font_weight(FontWeight::MEDIUM).child(SharedString::from(title)));
        if !detail.trim().is_empty() {
            card = card.child(div().text_size(px(theme::FS_XS)).text_color(colors.text_2).child(SharedString::from(detail)));
        }
        if permission.is_simple_approval() && !permission.choices.is_empty() {
            let mut buttons = div().flex().items_center().gap(px(8.0));
            for choice in permission.choices.iter().take(3) {
                let (label, primary) = match choice.as_str() {
                    "allow_once" | "allow" | "approve" => ("Allow", true),
                    "deny" | "reject" => ("Deny", false),
                    other => (other, false),
                };
                let permission_id = permission.id.clone();
                let choice = choice.clone();
                buttons = buttons.child(
                    div()
                        .id(SharedString::from(format!("permission-{permission_id}-{choice}")))
                        .h(px(26.0))
                        .px(px(10.0))
                        .flex()
                        .items_center()
                        .rounded(px(theme::RADIUS_SM))
                        .bg(if primary { colors.accent } else { colors.raised })
                        .text_color(if primary { colors.base } else { colors.text_1 })
                        .text_size(px(theme::FS_XS))
                        .cursor_pointer()
                        .hover(move |style| style.bg(if primary { colors.selected_strong } else { colors.selected }))
                        .child(SharedString::from(label.to_owned()))
                        .on_click(cx.listener(move |root, _: &gpui::ClickEvent, _window, cx| {
                            root.reply_permission(permission_id.clone(), choice.clone(), cx);
                        })),
                );
            }
            card = card.child(buttons);
        } else {
            card = card.child(
                div()
                    .text_size(px(theme::FS_XS))
                    .text_color(colors.text_3)
                    .child("Open this in Bitbots to answer."),
            );
        }
        card.into_any_element()
    }

    fn render_composer(&mut self, colors: Colors, session: Option<&Session>, approvals_waiting: bool, cx: &mut Context<Self>) -> AnyElement {
        let placeholder_hint = if approvals_waiting {
            "Answer the request above first"
        } else if session.map(Session::is_busy).unwrap_or(false) {
            "Your message queues for after this turn"
        } else {
            "Enter sends · Shift+Enter adds a line"
        };
        let send_label = if session.map(Session::is_busy).unwrap_or(false) { "Send later" } else { "Send" };
        div()
            .flex_none()
            .px(px(12.0))
            .pb(px(12.0))
            .pt(px(6.0))
            .child(
                div()
                    .p(px(8.0))
                    .flex()
                    .flex_col()
                    .gap(px(6.0))
                    .rounded(px(theme::RADIUS_LG))
                    .bg(colors.panel)
                    .border_1()
                    .border_color(colors.line_strong)
                    .child(self.composer.clone())
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .flex_1()
                                    .truncate()
                                    .text_size(px(theme::FS_2XS))
                                    .text_color(colors.text_3)
                                    .child(SharedString::from(placeholder_hint.to_owned())),
                            )
                            .child(
                                div()
                                    .id("send-message")
                                    .h(px(26.0))
                                    .px(px(10.0))
                                    .flex()
                                    .items_center()
                                    .rounded(px(theme::RADIUS_SM))
                                    .bg(colors.accent)
                                    .text_color(colors.base)
                                    .text_size(px(theme::FS_XS))
                                    .font_weight(FontWeight::MEDIUM)
                                    .cursor_pointer()
                                    .hover(|style| style.bg(colors.selected_strong))
                                    .child(send_label)
                                    .on_click(cx.listener(|root, _: &gpui::ClickEvent, window, cx| root.send(window, cx))),
                            ),
                    ),
            )
            .into_any_element()
    }
}

fn render_row(row: &Row, colors: Colors) -> AnyElement {
    match row {
        Row::User(text) => div()
            .w_full()
            .px(px(16.0))
            .py(px(4.0))
            .flex()
            .justify_end()
            .child(
                div()
                    .max_w(px(560.0))
                    .px(px(11.0))
                    .py(px(7.0))
                    .rounded(px(theme::RADIUS_MD))
                    .bg(colors.bubble_user)
                    .text_size(px(theme::FS_MD))
                    .text_color(colors.text_1)
                    .whitespace_normal()
                    .child(SharedString::from(text.clone())),
            )
            .into_any_element(),
        Row::Assistant(text) => div()
            .w_full()
            .px(px(16.0))
            .py(px(5.0))
            .child(render_markdown(text, colors))
            .into_any_element(),
        Row::Notice(text) => div()
            .w_full()
            .px(px(16.0))
            .py(px(3.0))
            .text_size(px(theme::FS_XS))
            .text_color(colors.text_3)
            .child(SharedString::from(text.clone()))
            .into_any_element(),
        Row::Tool(text) => div()
            .w_full()
            .px(px(16.0))
            .py(px(3.0))
            .flex()
            .items_center()
            .gap(px(6.0))
            .child(div().size(px(4.0)).rounded_full().bg(colors.text_3))
            .child(
                div()
                    .text_size(px(theme::FS_XS))
                    .text_color(colors.text_3)
                    .child(SharedString::from(text.clone())),
            )
            .into_any_element(),
    }
}

/// Basic markdown: paragraphs, headings, fenced code, bullet and numbered lists,
/// with inline code, bold and italic. Tables, charts and images are later slices.
pub fn render_markdown(text: &str, colors: Colors) -> AnyElement {
    let mut column = div().flex().flex_col().gap(px(6.0)).text_size(px(theme::FS_MD)).text_color(colors.text_1);
    for block in parse_blocks(text) {
        column = column.child(match block {
            Block::Paragraph(text) => inline_text(&text, colors, theme::FS_MD, colors.text_1),
            Block::Heading { level, text } => {
                let size = if level == 1 { theme::FS_XL } else if level == 2 { theme::FS_LG } else { theme::FS_MD };
                inline_text(&text, colors, size, colors.text_1).font_weight(FontWeight::SEMIBOLD)
            }
            Block::Code { text, .. } => div()
                .w_full()
                .p(px(10.0))
                .rounded(px(theme::RADIUS_SM))
                .bg(colors.raised)
                .font_family(theme::MONO_FONT)
                .text_size(px(theme::FS_XS))
                .text_color(colors.text_2)
                .child(SharedString::from(text)),
            Block::List { ordered, items } => {
                let mut list = div().flex().flex_col().gap(px(2.0));
                for (index, item) in items.iter().enumerate() {
                    let marker = if ordered { format!("{}.", index + 1) } else { "•".to_owned() };
                    list = list.child(
                        div()
                            .flex()
                            .gap(px(6.0))
                            .pl(px(if item.depth > 0 { 14.0 } else { 2.0 }))
                            .child(div().w(px(14.0)).text_color(colors.text_3).child(SharedString::from(marker)))
                            .child(inline_text(&item.text, colors, theme::FS_MD, colors.text_1)),
                    );
                }
                list
            }
        });
    }
    column.into_any_element()
}

/// One paragraph of inline runs. Bold, italic and code are highlight ranges on
/// one shaped text, so wrapping and selection behave like plain text.
fn inline_text(text: &str, colors: Colors, size: f32, color: Rgba) -> gpui::Div {
    let mut joined = String::new();
    let mut highlights: Vec<(std::ops::Range<usize>, HighlightStyle)> = Vec::new();
    for token in parse_inline(text) {
        let start = joined.len();
        match token {
            Inline::Text(text) => joined.push_str(&text),
            Inline::Code(code) => {
                joined.push_str(&code);
                highlights.push((
                    start..joined.len(),
                    HighlightStyle { background_color: Some(colors.raised.into()), ..Default::default() },
                ));
            }
            Inline::Strong(strong) => {
                joined.push_str(&strong);
                highlights.push((
                    start..joined.len(),
                    HighlightStyle { font_weight: Some(FontWeight::SEMIBOLD), ..Default::default() },
                ));
            }
            Inline::Em(emphasis) => {
                joined.push_str(&emphasis);
                highlights.push((
                    start..joined.len(),
                    HighlightStyle { font_style: Some(FontStyle::Italic), ..Default::default() },
                ));
            }
        }
    }
    div()
        .text_size(px(size))
        .text_color(color)
        .child(StyledText::new(SharedString::from(joined)).with_highlights(highlights))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{Message, Tool};

    fn message(role: &str, text: &str, at: &str) -> Message {
        Message {
            id: Some(format!("{role}-{at}")),
            role: Some(role.into()),
            content: Some(text.into()),
            created_at: Some(at.into()),
            ..Message::default()
        }
    }

    #[test]
    fn thinking_is_hidden_and_tool_steps_become_one_line() {
        let session = Session {
            messages: vec![
                message("user", "Fix the tests", "2026-11-08T10:00:00Z"),
                message("thinking", "let me think", "2026-11-08T10:00:01Z"),
                message("assistant", "Done", "2026-11-08T10:00:05Z"),
            ],
            tools: vec![Tool { id: Some("t1".into()), kind: Some("shell".into()), started_at: Some("2026-11-08T10:00:03Z".into()), ..Tool::default() }],
            ..Session::default()
        };
        let rows = build_rows(&session);
        assert_eq!(
            rows,
            vec![Row::User("Fix the tests".into()), Row::Tool("Ran a command".into()), Row::Assistant("Done".into())]
        );
        assert_eq!(row_count(&session), rows.len());
    }

    #[test]
    fn an_empty_reply_does_not_take_a_row() {
        let session = Session { messages: vec![message("assistant", "   ", "2026-11-08T10:00:00Z")], ..Session::default() };
        assert!(build_rows(&session).is_empty());
    }
}
