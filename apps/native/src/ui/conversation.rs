//! The conversation pane: the header, the message list, and the composer. The
//! reading and sending path is ported from `apps/desktop/src/ui/App.tsx`
//! (`MessageItem`, `sendPrompt`); one row's own drawing lives in `message`.
//!
//! The list is GPUI's `list` with bottom alignment, so a long conversation stays
//! smooth and the view sticks to the bottom while text streams.

use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;
use std::sync::Arc;

use gpui::{div, prelude::*, px, AnyElement, Context, FontWeight, Image, SharedString, Window};

use crate::state::{Permission, Session};
use crate::theme::{self, Colors};
use crate::ui::message::{self, FileRef, ImageRef, Row, RowContext};
use crate::ui::root::Root;

/// One entry before runs of tool steps are folded together.
enum Piece {
    User { id: String, text: String, images: Vec<ImageRef>, files: Vec<FileRef>, at: String },
    Assistant { id: String, text: String, at: String },
    Notice(String),
    Tool { kind: String, line: String },
}

/// The rows the list shows: messages and tool steps in time order, thinking left
/// out, with each run of tool steps folded into one row.
pub fn build_rows(session: &Session) -> Vec<Row> {
    let mut entries: Vec<(String, usize, Piece)> = Vec::new();
    let mut order = 0;
    for message in &session.messages {
        order += 1;
        if message.is_thinking() {
            continue;
        }
        let text = message.text().to_owned();
        let id = message
            .id
            .clone()
            .unwrap_or_else(|| format!("{}-{}", message.turn_id.clone().unwrap_or_default(), message.sequence));
        let at = message.created_at.clone().unwrap_or_default();
        let stamp = if at.is_empty() { "9999".to_owned() } else { at.clone() };
        let piece = match message.role() {
            "user" => {
                let images: Vec<ImageRef> = message
                    .attachments
                    .iter()
                    .enumerate()
                    .filter(|(_, attachment)| attachment_is_image(attachment))
                    .map(|(index, attachment)| ImageRef {
                        session_id: session.id.clone(),
                        message_id: id.clone(),
                        index,
                        name: attachment.name.clone(),
                    })
                    .collect();
                let files: Vec<FileRef> = message
                    .attachments
                    .iter()
                    .filter(|attachment| !attachment_is_image(attachment))
                    .map(|attachment| FileRef { name: attachment.name.clone(), path: attachment.path.clone(), size: String::new() })
                    .collect();
                Piece::User { id, text, images, files, at }
            }
            "notice" => Piece::Notice(text),
            "assistant" | "" => {
                if text.trim().is_empty() {
                    continue;
                }
                Piece::Assistant { id, text, at }
            }
            _ => Piece::Notice(text),
        };
        entries.push((stamp, order, piece));
    }
    for (index, tool) in session.tools.iter().enumerate() {
        order += 1;
        entries.push((
            tool.started_at.clone().unwrap_or_else(|| "9999".to_owned()),
            order,
            Piece::Tool {
                kind: tool.kind.clone().unwrap_or_else(|| format!("tool-{index}")),
                line: tool.summary(),
            },
        ));
    }
    entries.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));

    let mut rows: Vec<Row> = Vec::new();
    let mut run: Vec<(String, String)> = Vec::new();
    let flush = |run: &mut Vec<(String, String)>, rows: &mut Vec<Row>| {
        match run.len() {
            0 => {}
            1 => rows.push(Row::Tool(run[0].1.clone())),
            _ => {
                let kinds: Vec<String> = run.iter().map(|(kind, _)| kind.clone()).collect();
                rows.push(Row::Steps {
                    key: format!("steps:{}:{}", run[0].1, run.len()),
                    summary: steps_summary(&kinds),
                    lines: run.iter().map(|(_, line)| line.clone()).collect(),
                });
            }
        }
        run.clear();
    };
    for (_, _, piece) in entries {
        match piece {
            Piece::Tool { kind, line } => run.push((kind, line)),
            Piece::User { id, text, images, files, at } => {
                flush(&mut run, &mut rows);
                rows.push(Row::User { id, text, images, files, at });
            }
            Piece::Assistant { id, text, at } => {
                flush(&mut run, &mut rows);
                rows.push(Row::Assistant { id, text, at });
            }
            Piece::Notice(text) => {
                flush(&mut run, &mut rows);
                rows.push(Row::Notice(text));
            }
        }
    }
    flush(&mut run, &mut rows);
    rows
}

/// Whether an attachment travelled as an image the daemon can serve.
fn attachment_is_image(attachment: &crate::state::MessageAttachment) -> bool {
    if attachment.media_type.as_deref().is_some_and(|media_type| media_type.starts_with("image/")) {
        return true;
    }
    let name = attachment.path.to_lowercase();
    [".png", ".jpg", ".jpeg", ".gif", ".webp"].iter().any(|extension| name.ends_with(extension))
}

/// "Ran 3 commands · edited 1 file", from `activitySummary` in
/// `ui/conversation.ts`. One step is shown as itself, so this only counts runs.
pub fn steps_summary(kinds: &[String]) -> String {
    let mut commands = 0;
    let mut tools = 0;
    let mut searches = 0;
    let mut files = 0;
    let mut reads = 0;
    for kind in kinds {
        match kind.as_str() {
            "shell" | "command" | "terminal" => commands += 1,
            "edit" | "write" | "create" | "multi_edit" => files += 1,
            "read" | "read_file" | "view" => reads += 1,
            "search" | "grep" | "find" | "glob" | "list" => searches += 1,
            _ => tools += 1,
        }
    }
    let plural = |count: usize, one: &str, many: &str| format!("{count} {}", if count == 1 { one } else { many });
    let parts: Vec<String> = [
        (commands > 0).then(|| format!("ran {}", plural(commands, "command", "commands"))),
        (tools > 0).then(|| format!("used {}", plural(tools, "tool", "tools"))),
        (searches > 0).then(|| format!("searched {}", if searches == 1 { "once".to_owned() } else { format!("{searches} times") })),
        (files > 0).then(|| format!("edited {}", plural(files, "file", "files"))),
        (reads > 0).then(|| format!("read {}", plural(reads, "file", "files"))),
    ]
    .into_iter()
    .flatten()
    .collect();
    let line = parts.join(" · ");
    let line = if line.is_empty() { plural(kinds.len(), "step", "steps") } else { line };
    let mut chars = line.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => line,
    }
}

pub fn row_count(session: &Session) -> usize {
    build_rows(session).len()
}

impl Root {
    pub(crate) fn render_conversation_pane(&mut self, colors: Colors, _window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let agent = self.selected_agent().cloned();
        let agent_name = agent.as_ref().map(|agent| agent.name.clone());
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
            let title = session.as_ref().map(|session| session.title_or_default().to_owned());
            let mut row = div()
                .h(px(44.0))
                .flex_none()
                .px(px(14.0))
                .flex()
                .items_center()
                .gap(px(8.0))
                .border_b_1()
                .border_color(colors.line);
            if let Some(agent) = agent.as_ref() {
                let provider = self.provider_of(agent);
                row = row.child(
                    crate::art::Mark::for_agent(
                        32.0,
                        crate::art::agent_color(&agent.color),
                        agent.icon(),
                        provider.as_deref(),
                        agent.name.clone(),
                        colors,
                    )
                    .render(),
                );
            }
            row = row.child(
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
            if let Some(title) = title {
                row = row
                    .child(div().text_color(colors.text_3).child("·"))
                    .child(div().flex_1().min_w(px(0.0)).truncate().text_size(px(theme::FS_XS)).text_color(colors.text_3).child(SharedString::from(title)));
            } else {
                row = row.child(div().flex_1());
            }
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
                // A row's height is the height its text takes at the pane's
                // width, so a window resize has to measure every row again.
                let width = f32::from(self.list.viewport_bounds().size.width);
                if width > 0.0 && (width - self.list_width).abs() > 0.5 {
                    self.list_width = width;
                    if self.list_count > 0 {
                        self.list.remeasure();
                    }
                }
                // `.chat-column`: min(760px, the pane less its 20px sides).
                let column_width = if self.list_width > 0.0 {
                    (self.list_width - 40.0).clamp(0.0, message::CHAT_COLUMN_WIDTH)
                } else {
                    message::CHAT_COLUMN_WIDTH
                };
                let rows = Rc::new(build_rows(session));
                let light = !colors.dark;
                let _ = light;
                // The row closure cannot borrow the window's state, so the few
                // things it draws from are copied into it once per frame.
                let images: Rc<BTreeMap<String, Arc<Image>>> = Rc::new(self.message_images.clone());
                let expanded: Rc<BTreeSet<String>> = Rc::new(self.expanded_steps.clone());
                let hovered = self.hovered_message.clone();
                let project_path = session.project_path.clone();
                let session_id = session.id.clone();
                let this = cx.weak_entity();
                // Ask for the pictures this conversation shows, once each.
                let wanted: Vec<ImageRef> = rows
                    .iter()
                    .filter_map(|row| match row {
                        Row::User { images, .. } => Some(images.clone()),
                        _ => None,
                    })
                    .flatten()
                    .collect();
                for reference in wanted {
                    self.request_message_image(&session_id, &reference.message_id, reference.index, cx);
                }
                div()
                    .flex_1()
                    .w_full()
                    .flex()
                    .overflow_hidden()
                    .child(
                        gpui::list(self.list.clone(), move |index, _window, _cx| {
                            let row = rows.get(index).cloned().unwrap_or(Row::Notice(String::new()));
                            let context = RowContext {
                                column_width,
                                colors,
                                project_path: project_path.clone(),
                                images: (*images).clone(),
                                expanded: (*expanded).clone(),
                                hovered: hovered.clone(),
                                root: this.clone(),
                            };
                            // Every row sits in the centred chat column, 30px of
                            // air above the first and 24px below the last.
                            let last = index + 1 == rows.len();
                            div()
                                .w_full()
                                .px(px(20.0))
                                .when(index == 0, |this| this.pt(px(30.0)))
                                .when(last, |this| this.pb(px(24.0)))
                                .flex()
                                .justify_center()
                                .child(
                                    div()
                                        .w(px(column_width))
                                        .min_w(px(0.0))
                                        .child(message::render_row(&row, &context)),
                                )
                                .into_any_element()
                        })
                        .flex_1()
                        .h_full(),
                    )
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
            pane = pane.child(self.render_pending_card(&permission, colors, cx));
        }
        pane.child(self.render_composer(colors, session.as_ref(), approvals_waiting, cx)).into_any_element()
    }

    /// The card waiting in this conversation: an approval, a question or a
    /// proposal, each with its own way of being answered.
    fn render_pending_card(&mut self, permission: &Permission, colors: Colors, cx: &mut Context<Self>) -> AnyElement {
        if permission.is_proposal() {
            self.render_proposal(permission, colors, cx)
        } else if permission.is_question() {
            self.render_question(permission, colors, cx)
        } else {
            self.render_approval(permission, colors, cx)
        }
    }
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
        assert_eq!(rows.len(), 3);
        assert!(matches!(rows[0], Row::User { .. }));
        assert_eq!(rows[1], Row::Tool("Ran a command".into()));
        assert!(matches!(&rows[2], Row::Assistant { text, .. } if text == "Done"));
        assert_eq!(row_count(&session), rows.len());
    }

    #[test]
    fn an_empty_reply_does_not_take_a_row() {
        let session = Session { messages: vec![message("assistant", "   ", "2026-11-08T10:00:00Z")], ..Session::default() };
        assert!(build_rows(&session).is_empty());
    }

    fn tool(id: &str, kind: &str, at: &str) -> Tool {
        Tool { id: Some(id.into()), kind: Some(kind.into()), started_at: Some(at.into()), ..Tool::default() }
    }

    #[test]
    fn a_run_of_tool_steps_is_one_row_that_expands() {
        // The live report: thirty commands before the final reply.
        let tools: Vec<Tool> = (0..30)
            .map(|index| tool(&format!("t{index}"), "shell", &format!("2026-11-08T10:00:{:02}Z", index % 60)))
            .collect();
        let session = Session { tools, ..Session::default() };
        let rows = build_rows(&session);
        assert_eq!(rows.len(), 1, "one muted row for the whole run");
        match &rows[0] {
            Row::Steps { summary, lines, .. } => {
                assert_eq!(summary, "Ran 30 commands");
                assert_eq!(lines.len(), 30, "every step is still there, behind the row");
            }
            other => panic!("expected one steps row, got {other:?}"),
        }
        assert_eq!(row_count(&session), 1);
    }

    #[test]
    fn the_summary_counts_every_kind_of_step() {
        let tools = vec![
            tool("a", "shell", "2026-11-08T10:00:01Z"),
            tool("b", "edit", "2026-11-08T10:00:02Z"),
            tool("c", "read", "2026-11-08T10:00:03Z"),
            tool("d", "grep", "2026-11-08T10:00:04Z"),
        ];
        let session = Session { tools, ..Session::default() };
        let rows = build_rows(&session);
        match &rows[0] {
            Row::Steps { summary, .. } => assert_eq!(summary, "Ran 1 command · searched once · edited 1 file · read 1 file"),
            other => panic!("expected a steps row, got {other:?}"),
        }
    }

    #[test]
    fn a_message_ends_the_run() {
        let session = Session {
            messages: vec![message("assistant", "Between", "2026-11-08T10:00:02Z")],
            tools: vec![
                tool("a", "shell", "2026-11-08T10:00:01Z"),
                tool("b", "shell", "2026-11-08T10:00:03Z"),
                tool("c", "shell", "2026-11-08T10:00:04Z"),
            ],
            ..Session::default()
        };
        let rows = build_rows(&session);
        // One step, then the reply, then a run of two.
        assert_eq!(rows.len(), 3, "{rows:?}");
        assert!(matches!(rows[0], Row::Tool(_)));
        assert!(matches!(rows[1], Row::Assistant { .. }));
        assert!(matches!(rows[2], Row::Steps { .. }));
    }

    #[test]
    fn a_sent_image_belongs_to_its_message() {
        let mut message = message("user", "Look", "2026-11-08T10:00:00Z");
        message.attachments.push(crate::state::MessageAttachment {
            name: "shot.png".into(),
            path: "C:/tmp/shot.png".into(),
            file: None,
            media_type: Some("image/png".into()),
        });
        let session = Session { id: "s1".into(), messages: vec![message], ..Session::default() };
        let rows = build_rows(&session);
        match &rows[0] {
            Row::User { images, .. } => {
                assert_eq!(images.len(), 1);
                assert_eq!(images[0].key(), format!("s1:user-2026-11-08T10:00:00Z:0"));
            }
            other => panic!("expected a user row, got {other:?}"),
        }
    }
}
