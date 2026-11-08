//! One conversation row, and everything a message can hold: markdown with
//! tables, links, quotes and code (`ui/SafeMarkdown.tsx`, `ui/RichTable.tsx`), a
//! file chip (`ui/FileReferenceChip.tsx`), the images a message carried
//! (`ui/GeneratedImage.tsx`, `ui/MediaPreview.tsx`), its time on hover, and the
//! hover actions (`ui/MessageActions.tsx`): Copy, then Retry or Edit.
//!
//! The app's wording and its checks are the reference: a link opens only when
//! `safe_http_url` accepts it, and a file opens only when
//! `canonical_project_file` finds it inside this conversation's project.

use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::Arc;

use gpui::{div, prelude::*, px, AnyElement, FontStyle, FontWeight, HighlightStyle, Image, Rgba, SharedString, StyledText};

use crate::art::{self, Glyph};
use crate::theme::{self, Colors};
use crate::ui::markdown::{parse_blocks, parse_inline, Block, Inline};
use crate::ui::root::Root;

/// One image a message carried, addressed the way `message.image` wants it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImageRef {
    pub session_id: String,
    pub message_id: String,
    pub index: usize,
    pub name: String,
}

impl ImageRef {
    /// The key the window caches the decoded picture under.
    pub fn key(&self) -> String {
        format!("{}:{}:{}", self.session_id, self.message_id, self.index)
    }
}

/// A file named in a message by path (`fileReference.ts`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileRef {
    pub name: String,
    pub path: String,
    pub size: String,
}

const FILE_SENTENCE: &str = "Local file reference selected by the user: ";

/// The one sentence a "file by path" message carries, and the chip it becomes.
/// A message with no sentence comes back unchanged (`splitFileReference`).
pub fn split_file_reference(text: &str) -> (String, Option<FileRef>) {
    let Some(start) = text.find(FILE_SENTENCE) else { return (text.to_owned(), None) };
    let rest_start = start + FILE_SENTENCE.len();
    // `<name> (<path>, <size>)`, and the sentence ends at that bracket.
    let Some(open) = text[rest_start..].find(" (").map(|index| rest_start + index) else {
        return (text.to_owned(), None);
    };
    let Some(close) = text[open..].find(')').map(|index| open + index) else {
        return (text.to_owned(), None);
    };
    let name = text[rest_start..open].trim().to_owned();
    let inside = &text[open + 2..close];
    let Some((path, size)) = inside.rsplit_once(", ") else { return (text.to_owned(), None) };
    if name.is_empty() || path.trim().is_empty() || size.trim().is_empty() {
        return (text.to_owned(), None);
    }
    // The app's sentence carries its own standard tail; the body is everything
    // before it and anything the owner wrote after it.
    const TAIL: &str = "The provider may only access this path if its own process permissions and working directory allow it.";
    let tail = match text[start..].find(TAIL) {
        Some(index) => {
            let mut end = start + index + TAIL.len();
            if text[end..].starts_with(". ") {
                end += 1;
            }
            end
        }
        None => close + 1,
    };
    let body = format!("{}{}", &text[..start], text[tail..].trim_start());
    (body.trim().to_owned(), Some(FileRef { name, path: path.trim().to_owned(), size: size.trim().to_owned() }))
}

/// A message's time, as a tooltip and beside the actions: the local clock.
pub fn message_time(stamp: &str) -> String {
    match chrono::DateTime::parse_from_rfc3339(stamp) {
        Ok(time) => time.with_timezone(&chrono::Local).format("%H:%M").to_string(),
        Err(_) => String::new(),
    }
}

/// What a row is, once runs of tool steps are folded (`Row` in
/// `ui/conversation.ts`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Row {
    User { id: String, text: String, images: Vec<ImageRef>, files: Vec<FileRef>, at: String },
    Assistant { id: String, text: String, at: String },
    Notice(String),
    Tool(String),
    Steps { key: String, summary: String, lines: Vec<String> },
}

impl Row {
    /// The key the hover state is kept under, when the row has one.
    pub fn id(&self) -> Option<&str> {
        match self {
            Row::User { id, .. } | Row::Assistant { id, .. } => Some(id),
            _ => None,
        }
    }

    pub fn text(&self) -> &str {
        match self {
            Row::User { text, .. } | Row::Assistant { text, .. } => text,
            Row::Notice(text) | Row::Tool(text) => text,
            Row::Steps { summary, .. } => summary,
        }
    }
}

/// Everything one row's drawing needs, gathered once per frame so the list
/// closure only clones cheap handles.
#[derive(Clone)]
pub struct RowContext {
    pub colors: Colors,
    /// The project folder this conversation works in, for the file check.
    pub project_path: Option<String>,
    /// The pictures already decoded, by `ImageRef::key`.
    pub images: BTreeMap<String, Arc<Image>>,
    pub expanded: std::collections::BTreeSet<String>,
    /// The row whose actions are showing.
    pub hovered: Option<String>,
    pub root: gpui::WeakEntity<Root>,
}

/// An image's thumbnail, or the placeholder that says it is loading.
fn thumbnail(key: &str, name: &str, ctx: &RowContext) -> AnyElement {
    match ctx.images.get(key).cloned() {
        Some(image) => div()
            .id(SharedString::from(format!("image-{key}")))
            .w(px(150.0))
            .h(px(110.0))
            .flex_none()
            .rounded(px(theme::RADIUS_SM))
            .border_1()
            .border_color(ctx.colors.line)
            .overflow_hidden()
            .cursor_pointer()
            .child(gpui::img(image).size_full())
            .on_click({
                let root = ctx.root.clone();
                let key = key.to_owned();
                move |_event: &gpui::ClickEvent, _window: &mut gpui::Window, cx: &mut gpui::App| {
                    if let Some(root) = root.upgrade() {
                        let key = key.clone();
                        root.update(cx, |root, cx| {
                            root.viewer = Some(key);
                            cx.notify();
                        });
                    }
                }
            })
            .into_any_element(),
        None => div()
            .w(px(150.0))
            .h(px(110.0))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .gap(px(6.0))
            .rounded(px(theme::RADIUS_SM))
            .bg(ctx.colors.raised)
            .border_1()
            .border_color(ctx.colors.line)
            .child(art::glyph(Glyph::IMAGE, 14.0, ctx.colors.text_3))
            .child(div().truncate().max_w(px(96.0)).text_size(px(theme::FS_2XS)).text_color(ctx.colors.text_3).child(SharedString::from(name.to_owned())))
            .into_any_element(),
    }
}

/// The hover row's actions: Copy, and Retry and Edit on a message the bot can be
/// asked again (`MessageActions`, `MessageContextMenu`).
fn actions(row: &Row, ctx: &RowContext) -> AnyElement {
    let text = row.text().to_owned();
    let (id, at, can_retry) = match row {
        Row::User { id, at, .. } => (id.clone(), at.clone(), false),
        Row::Assistant { id, at, .. } => (id.clone(), at.clone(), true),
        _ => (row.id().unwrap_or_default().to_owned(), String::new(), false),
    };
    let copy_text = text.clone();
    let copy = div()
        .id(SharedString::from(format!("copy-{id}")))
        .size(px(22.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(5.0))
        .cursor_pointer()
        .hover(move |style| style.bg(ctx.colors.hover))
        .child(art::glyph(Glyph::COPY, 12.0, ctx.colors.text_3))
        .on_click(cx_listener(ctx, move |root, cx| {
            root.copy_to_clipboard(copy_text.clone(), cx);
        }));
    let mut row_actions = div().flex().items_center().gap(px(2.0)).child(copy);
    if can_retry {
        let retry_text = text.clone();
        row_actions = row_actions.child(
            div()
                .id(SharedString::from(format!("retry-{id}")))
                .size(px(22.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(5.0))
                .cursor_pointer()
                .hover(move |style| style.bg(ctx.colors.hover))
                .child(art::glyph(Glyph::ROTATE, 12.0, ctx.colors.text_3))
                .on_click(cx_listener(ctx, move |root, cx| {
                    root.retry_prompt(retry_text.clone(), cx);
                })),
        );
        let edit_text = text;
        row_actions = row_actions.child(
            div()
                .id(SharedString::from(format!("edit-{id}")))
                .size(px(22.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(5.0))
                .cursor_pointer()
                .hover(move |style| style.bg(ctx.colors.hover))
                .child(art::glyph(Glyph::PENCIL, 12.0, ctx.colors.text_3))
                .on_click(cx_listener(ctx, move |root, cx| {
                    root.edit_prompt(edit_text.clone(), cx);
                })),
        );
    }
    let time = message_time(&at);
    if !time.is_empty() {
        row_actions = row_actions.child(
            div()
                .ml(px(2.0))
                .text_size(px(theme::FS_2XS))
                .text_color(ctx.colors.text_3)
                .child(SharedString::from(time)),
        );
    }
    row_actions.into_any_element()
}

/// A listener on the root entity, from a closure that does not need the window.
fn cx_listener<F>(ctx: &RowContext, action: F) -> impl Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static
where
    F: Fn(&mut Root, &mut gpui::Context<Root>) + 'static,
{
    let root = ctx.root.clone();
    let action = Rc::new(action);
    move |_event: &gpui::ClickEvent, _window: &mut gpui::Window, cx: &mut gpui::App| {
        if let Some(root) = root.upgrade() {
            let action = action.clone();
            root.update(cx, |root, cx| action(root, cx));
        }
    }
}

/// Draw one row.
pub fn render_row(row: &Row, ctx: &RowContext) -> AnyElement {
    let colors = ctx.colors;
    match row {
        Row::Steps { key, summary, lines } => steps_row(key, summary, lines, ctx),
        Row::Notice(text) => div()
            .w_full()
            .min_w(px(0.0))
            .px(px(16.0))
            .py(px(3.0))
            .text_size(px(theme::FS_XS))
            .text_color(colors.text_3)
            .whitespace_normal()
            .child(SharedString::from(text.clone()))
            .into_any_element(),
        Row::Tool(text) => div()
            .w_full()
            .min_w(px(0.0))
            .px(px(16.0))
            .py(px(3.0))
            .flex()
            .items_center()
            .gap(px(6.0))
            .child(div().size(px(4.0)).rounded_full().bg(colors.text_3))
            .child(div().text_size(px(theme::FS_XS)).text_color(colors.text_3).child(SharedString::from(text.clone())))
            .into_any_element(),
        Row::User { text, images, files, id, .. } => {
            let hovered = ctx.hovered.as_deref() == Some(id.as_str());
            let mut bubble = div()
                .max_w(px(560.0))
                .min_w(px(0.0))
                .px(px(11.0))
                .py(px(7.0))
                .flex()
                .flex_col()
                .gap(px(6.0))
                .rounded(px(theme::RADIUS_MD))
                .bg(colors.bubble_user)
                .text_size(px(theme::FS_MD))
                .text_color(colors.text_1)
                .whitespace_normal();
            if !images.is_empty() {
                let mut strip = div().flex().flex_wrap().gap(px(6.0));
                for image in images {
                    strip = strip.child(thumbnail(&image.key(), &image.name, ctx));
                }
                bubble = bubble.child(strip);
            }
            if !text.trim().is_empty() {
                // The shaped paragraph itself, not a block around it: this bubble
                // is as wide as its text (up to its cap), so a wrapper has no
                // width to wrap against and the text becomes one character a line.
                bubble = bubble.child(styled_inline(text, colors));
            }
            for file in files {
                bubble = bubble.child(file_chip(file, ctx));
            }
            div()
                .id(SharedString::from(format!("row-{id}")))
                .w_full()
                .min_w(px(0.0))
                .px(px(16.0))
                .py(px(4.0))
                .flex()
                .flex_col()
                .items_end()
                .gap(px(2.0))
                .when(hovered, |this| this)
                .on_hover(cx_hover(ctx, id.clone()))
                .child(div().flex().items_center().gap(px(8.0)).when(hovered, |this| this.child(actions(row, ctx))).child(bubble))
                .into_any_element()
        }
        Row::Assistant { text, id, .. } => {
            let hovered = ctx.hovered.as_deref() == Some(id.as_str());
            let (body, files) = split_file_reference(text);
            let mut column = div()
                .w_full()
                .min_w(px(0.0))
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(render_markdown(&body, ctx));
            if let Some(file) = files {
                column = column.child(div().px(px(2.0)).child(file_chip(&file, ctx)));
            }
            div()
                .id(SharedString::from(format!("row-{id}")))
                .w_full()
                .min_w(px(0.0))
                .px(px(16.0))
                .py(px(5.0))
                .flex()
                .flex_col()
                .gap(px(2.0))
                .on_hover(cx_hover(ctx, id.clone()))
                .when(hovered, |this| this.child(div().flex().items_center().gap(px(8.0)).child(actions(row, ctx))))
                .child(column)
                .into_any_element()
        }
    }
}

fn cx_hover(ctx: &RowContext, id: String) -> impl Fn(&bool, &mut gpui::Window, &mut gpui::App) + 'static {
    let root = ctx.root.clone();
    move |is_hovered: &bool, _window: &mut gpui::Window, cx: &mut gpui::App| {
        if let Some(root) = root.upgrade() {
            let key = is_hovered.then(|| id.clone());
            root.update(cx, |root, cx| {
                if root.hovered_message != key {
                    root.hovered_message = key;
                    cx.notify();
                }
            });
        }
    }
}

/// The path relative to the project folder, when it is inside it
/// (`projectRelative` in `ui/fileReference.ts`).
pub fn project_relative(path: &str, project_path: Option<&str>) -> Option<String> {
    let project = project_path?;
    let norm = |value: &str| value.replace('\\', "/").trim_end_matches('/').to_lowercase();
    let file = path.replace('\\', "/");
    let root = norm(project);
    let lowered = file.to_lowercase();
    if lowered.starts_with(&format!("{root}/")) {
        Some(file[root.len() + 1..].to_owned())
    } else {
        None
    }
}

/// A file named by path: its name, its size and the fact that it travelled as a
/// path. **It is never started**: a bot wrote the path, so its bytes are shown
/// inside the window when they are a picture or text, and Explorer can highlight
/// it in its folder. That is the whole of what a chip does
/// (`ui/FileReferenceChip.tsx`).
fn file_chip(file: &FileRef, ctx: &RowContext) -> AnyElement {
    let colors = ctx.colors;
    let relative = project_relative(&file.path, ctx.project_path.as_deref());
    let inside = relative.is_some();
    let can_preview = inside && crate::files::previewable_name(&file.name);
    let preview_path = file.path.clone();
    let folder_path = file.path.clone();
    let project = ctx.project_path.clone();
    let folder_project = ctx.project_path.clone();

    let mut head = div()
        .w_full()
        .min_w(px(0.0))
        .p(px(7.0))
        .flex()
        .items_center()
        .gap(px(6.0))
        .rounded(px(theme::RADIUS_SM))
        .bg(colors.raised)
        .border_1()
        .border_color(colors.line)
        .child(art::glyph(Glyph::PAPERCLIP, 11.0, colors.text_3))
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .truncate()
                .text_size(px(theme::FS_XS))
                .child(SharedString::from(file.name.clone())),
        )
        .child(
            div()
                .flex_none()
                .text_size(px(theme::FS_2XS))
                .text_color(colors.text_3)
                .child(SharedString::from(if file.size.is_empty() {
                    "sent as a path".to_owned()
                } else {
                    format!("{} · sent as a path", file.size)
                })),
        );
    if can_preview {
        head = head.child(
            div()
                .id(SharedString::from(format!("file-preview-{}", file.name)))
                .h(px(20.0))
                .px(px(6.0))
                .flex_none()
                .flex()
                .items_center()
                .rounded(px(4.0))
                .bg(colors.panel)
                .border_1()
                .border_color(colors.line)
                .text_size(px(theme::FS_2XS))
                .text_color(colors.text_1)
                .cursor_pointer()
                .hover(move |style| style.bg(colors.selected))
                .child("Preview")
                .on_click(cx_listener(ctx, move |root, cx| {
                    root.preview_file(preview_path.clone(), project.clone(), cx)
                })),
        );
    }
    if inside {
        head = head.child(
            div()
                .id(SharedString::from(format!("file-folder-{}", file.name)))
                .h(px(20.0))
                .px(px(6.0))
                .flex_none()
                .flex()
                .items_center()
                .rounded(px(4.0))
                .border_1()
                .border_color(colors.line)
                .text_size(px(theme::FS_2XS))
                .text_color(colors.text_2)
                .cursor_pointer()
                .hover(move |style| style.bg(colors.selected))
                .child("Show in folder")
                .on_click(cx_listener(ctx, move |root, cx| {
                    root.show_in_folder(folder_path.clone(), folder_project.clone(), cx)
                })),
        );
    }

    let note = if !inside {
        Some("This file is outside the project, so Bitbots does not show it. The agent can open it only if its permissions allow.")
    } else if !can_preview {
        Some("Bitbots cannot show this kind of file. The agent can open it by its path.")
    } else {
        None
    };

    div()
        .w_full()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .gap(px(3.0))
        .child(head)
        .when_some(note, |this, note| {
            this.child(
                div()
                    .text_size(px(theme::FS_2XS))
                    .text_color(colors.text_3)
                    .whitespace_normal()
                    .child(SharedString::from(note.to_owned())),
            )
        })
        .into_any_element()
}

/// A run of tool steps as one muted line, which expands into the steps on click
/// (`ActivitySummaryLine` in `ui/ActivityDetails.tsx`).
fn steps_row(key: &str, summary: &str, lines: &[String], ctx: &RowContext) -> AnyElement {
    let open = ctx.expanded.contains(key);
    let toggle_key = key.to_owned();
    let mut column = div()
        .id(SharedString::from(format!("steps-{key}")))
        .w_full()
        .px(px(16.0))
        .py(px(3.0))
        .flex()
        .flex_col()
        .gap(px(2.0))
        .child(
            div()
                .id(SharedString::from(format!("steps-toggle-{key}")))
                .flex()
                .items_center()
                .gap(px(6.0))
                .cursor_pointer()
                .hover(move |style| style.text_color(ctx.colors.text_2))
                .child(art::glyph(if open { Glyph::CHEVRON_DOWN } else { Glyph::CHEVRON }, 10.0, ctx.colors.text_3))
                .child(div().size(px(4.0)).rounded_full().bg(ctx.colors.text_3))
                .child(div().text_size(px(theme::FS_XS)).text_color(ctx.colors.text_3).child(SharedString::from(summary.to_owned())))
                .on_click(cx_listener(ctx, move |root, cx| root.toggle_steps(toggle_key.clone(), cx))),
        );
    if open {
        for line in lines {
            column = column.child(
                div()
                    .pl(px(26.0))
                    .text_size(px(theme::FS_XS))
                    .text_color(ctx.colors.text_3)
                    .child(SharedString::from(line.clone())),
            );
        }
    }
    column.into_any_element()
}

/// Markdown, block by block: paragraphs with inline runs, headings, code with a
/// language label and a Copy button, lists, quotes and tables.
pub fn render_markdown(text: &str, ctx: &RowContext) -> AnyElement {
    let colors = ctx.colors;
    // The whole chain takes the pane's width and may shrink below its content,
    // or a long token in inline code pushes the column past the pane and the
    // list clips it.
    let mut column = div()
        .w_full()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .gap(px(6.0))
        .text_size(px(theme::FS_MD))
        .text_color(colors.text_1);
    for block in parse_blocks(text) {
        // A link the app would open gets a chip of its own under the paragraph:
        // one shaped line cannot hold a click in GPUI, and a link nobody can
        // press is worse than one beside the text.
        let links: Vec<(String, String)> = match &block {
            Block::Paragraph(text) | Block::Heading { text, .. } => parse_inline(text)
                .into_iter()
                .filter_map(|token| match token {
                    Inline::Link { text, url } => Some((text, url)),
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        };
        column = column.child(match block {
            Block::Paragraph(text) => inline_text(&text, colors, theme::FS_MD, colors.text_1).into_any_element(),
            Block::Heading { level, text } => {
                let size = if level == 1 { theme::FS_XL } else if level == 2 { theme::FS_LG } else { theme::FS_MD };
                inline_text(&text, colors, size, colors.text_1).font_weight(FontWeight::SEMIBOLD).into_any_element()
            }
            Block::Code { language, text } => code_block(&language, &text, ctx),
            Block::Quote(text) => {
                let mut quote = div()
                    .w_full()
                    .min_w(px(0.0))
                    .pl(px(10.0))
                    .border_l_2()
                    .border_color(colors.line_strong)
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .text_color(colors.text_2);
                for line in text.split('\n') {
                    quote = quote.child(inline_text(line, colors, theme::FS_MD, colors.text_2));
                }
                quote.into_any_element()
            }
            Block::Table { rows } => table(&rows, ctx),
            Block::List { ordered, items } => {
                let mut list = div().w_full().min_w(px(0.0)).flex().flex_col().gap(px(2.0));
                for (index, item) in items.iter().enumerate() {
                    let marker = if ordered { format!("{}.", index + 1) } else { "•".to_owned() };
                    list = list.child(
                        div()
                            .w_full()
                            .min_w(px(0.0))
                            .flex()
                            .gap(px(6.0))
                            .pl(px(if item.depth > 0 { 14.0 } else { 2.0 }))
                            .child(div().w(px(16.0)).flex_none().text_color(colors.text_3).child(SharedString::from(marker)))
                            // The text takes the rest of the row and may shrink.
                            .child(div().flex_1().min_w(px(0.0)).child(inline_text(&item.text, colors, theme::FS_MD, colors.text_1))),
                    );
                }
                list.into_any_element()
            }
        });
        if !links.is_empty() {
            let mut row = div().flex().flex_wrap().gap(px(4.0));
            for (label, url) in links {
                let shown = if label == url { url.clone() } else { label };
                row = row.child(link_chip(&shown, &url, ctx));
            }
            column = column.child(row);
        }
    }
    column.into_any_element()
}

/// One link, as a chip that opens it in the browser after `safe_http_url`.
fn link_chip(label: &str, url: &str, ctx: &RowContext) -> AnyElement {
    let target = url.to_owned();
    div()
        .id(SharedString::from(format!("link-{url}")))
        .h(px(22.0))
        .px(px(7.0))
        .flex()
        .items_center()
        .gap(px(4.0))
        .rounded(px(theme::RADIUS_SM))
        .bg(ctx.colors.raised)
        .border_1()
        .border_color(ctx.colors.line)
        .text_size(px(theme::FS_2XS))
        .text_color(ctx.colors.status_info)
        .cursor_pointer()
        .hover(move |style| style.bg(ctx.colors.selected))
        .child(art::glyph(Glyph::ARROW_RIGHT, 10.0, ctx.colors.status_info))
        .child(div().max_w(px(320.0)).truncate().child(SharedString::from(label.to_owned())))
        .on_click(cx_listener(ctx, move |root, cx| root.open_link(target.clone(), cx)))
        .into_any_element()
}

/// A fenced block with its language on the left and Copy on the right.
fn code_block(language: &str, text: &str, ctx: &RowContext) -> AnyElement {
    let colors = ctx.colors;
    let label = if language.trim().is_empty() { "Code".to_owned() } else { language.trim().to_uppercase() };
    let copy_text = text.to_owned();
    div()
        .w_full()
        .rounded(px(theme::RADIUS_SM))
        .bg(colors.raised)
        .border_1()
        .border_color(colors.line)
        .flex()
        .flex_col()
        .child(
            div()
                .h(px(24.0))
                .px(px(8.0))
                .flex()
                .items_center()
                .gap(px(8.0))
                .border_b_1()
                .border_color(colors.line)
                .child(div().flex_1().text_size(px(theme::FS_2XS)).text_color(colors.text_3).child(SharedString::from(label)))
                .child(
                    div()
                        .id(SharedString::from(format!("copy-code-{}", text.len())))
                        .px(px(6.0))
                        .flex()
                        .items_center()
                        .gap(px(4.0))
                        .rounded(px(4.0))
                        .cursor_pointer()
                        .hover(move |style| style.bg(colors.hover))
                        .child(art::glyph(Glyph::COPY, 10.0, colors.text_3))
                        .child(div().text_size(px(theme::FS_2XS)).text_color(colors.text_3).child("Copy"))
                        .on_click(cx_listener(ctx, move |root, cx| root.copy_to_clipboard(copy_text.clone(), cx))),
                ),
        )
        .child(
            // Code keeps its own lines: it scrolls sideways rather than wrapping
            // (as `<pre>` does), and never pushes the row wider than the pane.
            div()
                .id(SharedString::from(format!("code-{}", language.len() + text.len())))
                .w_full()
                .min_w(px(0.0))
                .max_h(px(420.0))
                .overflow_x_scroll()
                .p(px(10.0))
                .font_family(theme::MONO_FONT)
                .text_size(px(theme::FS_XS))
                .text_color(colors.text_2)
                .whitespace_nowrap()
                .child(SharedString::from(text.to_owned())),
        )
        .into_any_element()
}

/// A table: the header row, then one line per row. The author's own text is kept
/// cell by cell, so "12%" stays "12%" (`ui/RichTable.tsx`).
fn table(rows: &[Vec<String>], ctx: &RowContext) -> AnyElement {
    let colors = ctx.colors;
    let mut body = div()
        .w_full()
        .flex()
        .flex_col()
        .rounded(px(theme::RADIUS_SM))
        .border_1()
        .border_color(colors.line)
        .overflow_hidden();
    for (index, row) in rows.iter().enumerate() {
        let header = index == 0;
        let mut line = div()
            .flex()
            .w_full()
            .min_w(px(0.0))
            .when(header, |this| this.bg(colors.raised))
            .when(index > 0, |this| this.border_t_1().border_color(colors.line));
        for cell in row {
            line = line.child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .px(px(8.0))
                    .py(px(4.0))
                    .flex()
                    .items_center()
                    .text_size(px(theme::FS_XS))
                    .text_color(if header { colors.text_1 } else { colors.text_2 })
                    .when(header, |this| this.font_weight(FontWeight::SEMIBOLD))
                    .child(SharedString::from(cell.clone())),
            );
        }
        body = body.child(line);
    }
    body.into_any_element()
}

/// One paragraph of inline runs. Bold, italic, code and links are highlight
/// ranges on one shaped text, so wrapping and selection behave like plain text.
fn inline_text(text: &str, colors: Colors, size: f32, color: Rgba) -> gpui::Div {
    // A block with the pane's line height and text style, wrapping the shaped
    // paragraph. Only for a parent whose width is settled (a column, a row with
    // `flex_1`); a content-sized bubble takes `styled_inline` itself.
    div()
        .line_height(px(size * 1.65))
        .text_size(px(size))
        .text_color(color)
        .child(styled_inline(text, colors))
}

/// One shaped paragraph with the inline styles (bold, italic, code, links) as
/// highlight ranges on it, so wrapping and selection behave like plain text.
fn styled_inline(text: &str, colors: Colors) -> StyledText {
    let mut joined = String::new();
    let mut highlights: Vec<(std::ops::Range<usize>, HighlightStyle)> = Vec::new();
    for token in parse_inline(text) {
        let start = joined.len();
        match token {
            Inline::Text(text) => joined.push_str(&text),
            Inline::Code(code) => {
                joined.push_str(&code);
                highlights.push((start..joined.len(), HighlightStyle { background_color: Some(colors.raised.into()), ..Default::default() }));
            }
            Inline::Strong(strong) => {
                joined.push_str(&strong);
                highlights.push((start..joined.len(), HighlightStyle { font_weight: Some(FontWeight::SEMIBOLD), ..Default::default() }));
            }
            Inline::Em(emphasis) => {
                joined.push_str(&emphasis);
                highlights.push((start..joined.len(), HighlightStyle { font_style: Some(FontStyle::Italic), ..Default::default() }));
            }
            Inline::Link { text, .. } => {
                joined.push_str(&text);
                highlights.push((
                    start..joined.len(),
                    HighlightStyle { underline: Some(gpui::UnderlineStyle { color: Some(colors.status_info.into()), thickness: px(1.0), wavy: false }), ..Default::default() },
                ));
            }
            Inline::LinkText { text, .. } => joined.push_str(&text),
        }
    }
    StyledText::new(SharedString::from(joined)).with_highlights(highlights)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_by_path_becomes_a_chip_and_the_sentence_leaves_the_body() {
        let text = "Please look at this.\n\nLocal file reference selected by the user: report.pdf (C:/shop/report.pdf, 12.4 KB). File contents were not read by Bitbots. The provider may only access this path if its own process permissions and working directory allow it.";
        let (body, file) = split_file_reference(text);
        assert_eq!(body, "Please look at this.");
        let file = file.expect("the chip");
        assert_eq!(file.name, "report.pdf");
        assert_eq!(file.path, "C:/shop/report.pdf");
        assert_eq!(file.size, "12.4 KB");
    }

    #[test]
    fn a_file_is_said_to_be_inside_the_project_or_outside_it() {
        assert_eq!(project_relative("C:/shop/report.pdf", Some("C:/shop")), Some("report.pdf".to_owned()));
        // A windows path with backslashes, and any case, is the same folder.
        assert_eq!(project_relative("c:\\Shop\\docs\\a.md", Some("C:/shop/")), Some("docs/a.md".to_owned()));
        // Outside it, or with no project, there is no relative path.
        assert_eq!(project_relative("C:/elsewhere/a.md", Some("C:/shop")), None);
        assert_eq!(project_relative("C:/shop/a.md", None), None);
        // A sibling whose name only starts the same is not inside.
        assert_eq!(project_relative("C:/shopper/a.md", Some("C:/shop")), None);
    }

    #[test]
    fn a_message_with_no_sentence_is_left_alone() {
        let (body, file) = split_file_reference("Just a message.");
        assert_eq!(body, "Just a message.");
        assert!(file.is_none());
        // A half sentence is not a file either.
        let (body, file) = split_file_reference("Local file reference selected by the user: broken");
        assert!(file.is_none());
        assert!(body.starts_with("Local file reference"));
    }

    #[test]
    fn an_image_is_addressed_by_its_own_message() {
        let reference = ImageRef { session_id: "s".into(), message_id: "m".into(), index: 2, name: "shot.png".into() };
        assert_eq!(reference.key(), "s:m:2");
    }

    #[test]
    fn a_time_is_the_local_clock() {
        let at = "2026-11-08T10:30:00Z";
        let local = chrono::DateTime::parse_from_rfc3339(at).unwrap().with_timezone(&chrono::Local);
        assert_eq!(message_time(at), local.format("%H:%M").to_string());
        assert_eq!(message_time("not a date"), "");
    }

    #[test]
    fn a_row_knows_its_own_text() {
        let row = Row::User { id: "m1".into(), text: "hello".into(), images: Vec::new(), files: Vec::new(), at: "2026-11-08T10:00:00Z".into() };
        assert_eq!(row.id(), Some("m1"));
        assert_eq!(row.text(), "hello");
    }
}
