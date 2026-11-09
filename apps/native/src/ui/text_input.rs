//! A small multi-line text input with a real cursor and a real IME.
//!
//! GPUI has no published text field, and IME support is the one thing a native
//! port can quietly lose, so this follows Zed's own `crates/gpui/examples/input.rs`
//! at the pinned revision: an `EntityInputHandler` plus an `ElementInputHandler`
//! registered in `paint`. Windows sends its composition here, so Japanese and
//! Chinese input arrive as marked text rather than as loose keys.

use std::ops::Range;

use gpui::{
    actions, div, fill, point, prelude::*, px, relative, size, App, Bounds, Context,
    CursorStyle, Element, ElementId, ElementInputHandler, Entity, EntityInputHandler, FocusHandle,
    Focusable, GlobalElementId, InspectorElementId, KeyBinding, LayoutId, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, PaintQuad, Pixels, Point, ShapedLine, SharedString,
    Style, TextAlign, TextRun, UTF16Selection, UnderlineStyle, Window,
};

actions!(
    text_input,
    [Backspace, Delete, Left, Right, SelectLeft, SelectRight, SelectAll, Home, End, Paste, Cut, Copy, InsertNewline]
);

/// Key bindings for every composer in the app.
pub fn key_bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("backspace", Backspace, Some("Composer")),
        KeyBinding::new("delete", Delete, Some("Composer")),
        KeyBinding::new("left", Left, Some("Composer")),
        KeyBinding::new("right", Right, Some("Composer")),
        KeyBinding::new("shift-left", SelectLeft, Some("Composer")),
        KeyBinding::new("shift-right", SelectRight, Some("Composer")),
        KeyBinding::new("ctrl-a", SelectAll, Some("Composer")),
        KeyBinding::new("ctrl-v", Paste, Some("Composer")),
        KeyBinding::new("ctrl-c", Copy, Some("Composer")),
        KeyBinding::new("ctrl-x", Cut, Some("Composer")),
        KeyBinding::new("home", Home, Some("Composer")),
        KeyBinding::new("end", End, Some("Composer")),
        KeyBinding::new("shift-enter", InsertNewline, Some("Composer")),
    ]
}

/// The most lines the composer grows to before it stops growing.
pub const MAX_LINES: usize = 8;

// The hooks take `&mut TextInput` **and** `&mut App`, and that shape is the
// point. A hook runs while this box is leased (an input listener holds it), so
// it must not go back through `entity.update(..)` on the same box: GPUI takes the
// entity out of its map while it is leased, and the second call panics with
// "cannot update ... while it is already being updated", which aborts the
// window. Passing the box itself lets a hook read and edit it directly; the
// owner is notified through `Root`, which is a different entity.

/// What Enter does with no shift: the window sends the message.
pub type SubmitHandler = Box<dyn Fn(&mut TextInput, &mut Window, &mut App) + 'static>;

/// A key the window wants to see before the box acts on it: the `/command` menu,
/// the history recall and the quick switcher's arrow keys. Returning `true` means
/// the key was used and the box must not also act on it.
pub type KeyHook = Box<dyn Fn(&mut TextInput, &gpui::KeyDownEvent, &mut Window, &mut App) -> bool + 'static>;

/// Called after the owner typed, with the new text and the caret, so a field that
/// filters a list can redraw. It never receives the box, because the box is
/// leased while this runs.
pub type ChangeHook = Box<dyn Fn(&str, usize, &mut App) + 'static>;

pub struct TextInput {
    focus_handle: FocusHandle,
    content: String,
    cursor: usize,
    anchor: usize,
    marked_range: Option<Range<usize>>,
    placeholder: SharedString,
    line_height: Pixels,
    font_size: Pixels,
    /// Enter with no shift: the window sends the message.
    on_submit: Option<SubmitHandler>,
    on_key: Option<KeyHook>,
    on_change: Option<ChangeHook>,
    lines: Vec<ShapedLine>,
    bounds: Option<Bounds<Pixels>>,
    is_selecting: bool,
}

impl TextInput {
    pub fn new(placeholder: impl Into<SharedString>, line_height: Pixels, font_size: Pixels, cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            content: String::new(),
            cursor: 0,
            anchor: 0,
            marked_range: None,
            placeholder: placeholder.into(),
            line_height,
            font_size,
            on_submit: None,
            on_key: None,
            on_change: None,
            lines: Vec::new(),
            bounds: None,
            is_selecting: false,
        }
    }

    pub fn set_on_submit(&mut self, submit: SubmitHandler) {
        self.on_submit = Some(submit);
    }

    pub fn set_on_key(&mut self, hook: KeyHook) {
        self.on_key = Some(hook);
    }

    pub fn set_on_change(&mut self, hook: ChangeHook) {
        self.on_change = Some(hook);
    }

    /// Where the caret is, in bytes, which is what the `/command` menu and the
    /// history recall read.
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn text(&self) -> &str {
        &self.content
    }

    /// The composer's focus handle, for the window to focus on open and after a send.
    pub fn handle(&self) -> FocusHandle {
        self.focus_handle.clone()
    }

    pub fn is_empty(&self) -> bool {
        self.content.trim().is_empty()
    }

    /// Replace the whole text, as the window does (a recalled prompt, a rename
    /// field, the quick switcher's empty box). This is not the owner typing, so
    /// it does not call the change hook: the caller is already in control and may
    /// be inside this box's own update.
    pub fn set_text(&mut self, text: String, cx: &mut Context<Self>) {
        self.set_text_quiet(text);
        cx.notify();
    }

    /// The same, for a caller that already holds the box and will notify itself.
    pub fn set_text_quiet(&mut self, text: String) {
        self.content = text;
        self.cursor = self.content.len();
        self.anchor = self.cursor;
        self.marked_range = None;
    }

    /// Remove one byte range, for a `/command` chip that is taken back out.
    pub fn delete_range_quiet(&mut self, start: usize, end: usize) {
        let start = start.min(self.content.len());
        let end = end.clamp(start, self.content.len());
        let mut content = String::with_capacity(self.content.len());
        content.push_str(&self.content[..start]);
        content.push_str(&self.content[end..]);
        self.content = content;
        self.cursor = start;
        self.anchor = start;
        self.marked_range = None;
    }

    /// Tell the owner of this field that the owner typed. Never through the field
    /// itself: the field is leased by the caller while this runs.
    fn changed(&self, cx: &mut App) {
        if let Some(change) = self.on_change.as_ref() {
            change(&self.content, self.cursor, cx);
        }
    }

    pub fn clear(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.clear_quiet();
        let _ = window;
        cx.notify();
    }

    /// Empty the box without notifying, for a caller that holds it.
    pub fn clear_quiet(&mut self) {
        self.content.clear();
        self.cursor = 0;
        self.anchor = 0;
        self.marked_range = None;
        self.lines.clear();
    }

    /// The number of explicit lines, which is how tall the box grows.
    pub fn line_count(&self) -> usize {
        self.content.split('\n').count().clamp(1, MAX_LINES)
    }

    fn selection(&self) -> Range<usize> {
        self.cursor.min(self.anchor)..self.cursor.max(self.anchor)
    }

    fn selection_reversed(&self) -> bool {
        self.cursor < self.anchor
    }

    fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        let offset = offset.min(self.content.len());
        self.cursor = offset;
        self.anchor = offset;
        cx.notify();
    }

    fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.cursor = offset.min(self.content.len());
        cx.notify();
    }

    fn previous_boundary(&self, offset: usize) -> usize {
        let mut index = offset.min(self.content.len());
        while index > 0 && !self.content.is_char_boundary(index - 1) {
            index -= 1;
        }
        index.saturating_sub(1)
    }

    fn next_boundary(&self, offset: usize) -> usize {
        let mut index = offset.min(self.content.len());
        if index < self.content.len() {
            index += 1;
            while index < self.content.len() && !self.content.is_char_boundary(index) {
                index += 1;
            }
        }
        index
    }

    fn offset_to_utf16(&self, offset: usize) -> usize {
        self.content[..offset.min(self.content.len())].chars().map(char::len_utf16).sum()
    }

    fn offset_from_utf16(&self, offset: usize) -> usize {
        let mut utf16 = 0;
        for (index, ch) in self.content.char_indices() {
            if utf16 >= offset {
                return index;
            }
            utf16 += ch.len_utf16();
        }
        self.content.len()
    }

    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(range.start)..self.offset_to_utf16(range.end)
    }

    fn range_from_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(range.start)..self.offset_from_utf16(range.end)
    }

    fn backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut Context<Self>) {
        if self.selection().is_empty() {
            let previous = self.previous_boundary(self.cursor);
            self.cursor = previous;
            self.anchor = previous;
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn delete(&mut self, _: &Delete, window: &mut Window, cx: &mut Context<Self>) {
        if self.selection().is_empty() {
            let next = self.next_boundary(self.cursor);
            self.cursor = next;
            self.anchor = next;
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn left(&mut self, _: &Left, _: &mut Window, cx: &mut Context<Self>) {
        let offset = self.previous_boundary(self.selection().start);
        self.move_to(offset, cx);
    }

    fn right(&mut self, _: &Right, _: &mut Window, cx: &mut Context<Self>) {
        let offset = self.next_boundary(self.selection().end);
        self.move_to(offset, cx);
    }

    fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.previous_boundary(self.cursor), cx);
    }

    fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.next_boundary(self.cursor), cx);
    }

    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.anchor = 0;
        self.cursor = self.content.len();
        cx.notify();
    }

    fn home(&mut self, _: &Home, _: &mut Window, cx: &mut Context<Self>) {
        let before = &self.content[..self.cursor.min(self.content.len())];
        let start = before.rfind('\n').map(|index| index + 1).unwrap_or(0);
        self.move_to(start, cx);
    }

    fn end(&mut self, _: &End, _: &mut Window, cx: &mut Context<Self>) {
        let after = &self.content[self.cursor.min(self.content.len())..];
        let end = after.find('\n').map(|index| self.cursor + index).unwrap_or(self.content.len());
        self.move_to(end, cx);
    }

    fn insert_newline(&mut self, _: &InsertNewline, window: &mut Window, cx: &mut Context<Self>) {
        self.replace_text_in_range(None, "\n", window, cx);
    }

    fn paste(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.replace_text_in_range(None, &text, window, cx);
        }
    }

    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        let selection = self.selection();
        if !selection.is_empty() {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string(self.content[selection].to_owned()));
        }
    }

    fn cut(&mut self, _: &Cut, window: &mut Window, cx: &mut Context<Self>) {
        self.copy(&Copy, window, cx);
        self.replace_text_in_range(None, "", window, cx);
    }

    fn on_mouse_down(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.focus_handle.focus(window, cx);
        self.is_selecting = true;
        let offset = self.index_for_position(event.position);
        if event.modifiers.shift {
            self.select_to(offset, cx);
        } else {
            self.move_to(offset, cx);
        }
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.is_selecting = false;
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.is_selecting {
            let offset = self.index_for_position(event.position);
            self.select_to(offset, cx);
        }
    }

    /// Which byte offset a point inside the box lands on.
    fn index_for_position(&self, position: Point<Pixels>) -> usize {
        let Some(bounds) = self.bounds else { return self.content.len() };
        let row = (((position.y - bounds.top()) / self.line_height).floor() as usize).min(self.lines.len().saturating_sub(1));
        let mut offset = 0;
        for line in self.lines.iter().take(row) {
            offset += line.text.len() + 1;
        }
        let Some(line) = self.lines.get(row) else { return self.content.len() };
        let local_x = position.x - bounds.left();
        offset + line.closest_index_for_x(local_x)
    }

}

impl EntityInputHandler for TextInput {
    fn text_for_range(&mut self, range_utf16: Range<usize>, actual_range: &mut Option<Range<usize>>, _window: &mut Window, _cx: &mut Context<Self>) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16);
        actual_range.replace(self.range_to_utf16(&range));
        Some(self.content[range].to_string())
    }

    fn selected_text_range(&mut self, _ignore_disabled_input: bool, _window: &mut Window, _cx: &mut Context<Self>) -> Option<UTF16Selection> {
        Some(UTF16Selection { range: self.range_to_utf16(&self.selection()), reversed: self.selection_reversed() })
    }

    fn marked_text_range(&self, _window: &mut Window, _cx: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked_range.as_ref().map(|range| self.range_to_utf16(range))
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        self.marked_range = None;
    }

    fn replace_text_in_range(&mut self, range_utf16: Option<Range<usize>>, new_text: &str, _window: &mut Window, cx: &mut Context<Self>) {
        let range = range_utf16
            .as_ref()
            .map(|range| self.range_from_utf16(range))
            .or_else(|| self.marked_range.clone())
            .unwrap_or_else(|| self.selection());
        let mut content = String::with_capacity(self.content.len() + new_text.len());
        content.push_str(&self.content[..range.start.min(self.content.len())]);
        content.push_str(new_text);
        content.push_str(&self.content[range.end.min(self.content.len())..]);
        self.content = content;
        let cursor = range.start + new_text.len();
        self.cursor = cursor;
        self.anchor = cursor;
        self.marked_range = None;
        self.changed(cx);
        cx.notify();
    }

    fn replace_and_mark_text_in_range(&mut self, range_utf16: Option<Range<usize>>, new_text: &str, new_selected_range_utf16: Option<Range<usize>>, window: &mut Window, cx: &mut Context<Self>) {
        // The platform sends the whole composition again on each key, so the
        // previous marked text is replaced, never appended to.
        let previous = range_utf16
            .as_ref()
            .map(|range| self.range_from_utf16(range))
            .or_else(|| self.marked_range.clone());
        let start = previous.as_ref().map(|range| range.start).unwrap_or_else(|| self.selection().start);
        self.marked_range = None;
        self.cursor = start;
        self.anchor = start;
        let replace = previous.as_ref().map(|range| self.range_to_utf16(range));
        self.replace_text_in_range(replace, new_text, window, cx);
        if !new_text.is_empty() {
            self.marked_range = Some(start..start + new_text.len());
        }
        if let Some(range) = new_selected_range_utf16 {
            let range = self.range_from_utf16(&range);
            self.cursor = start + range.end;
            self.anchor = start + range.start;
        }
        cx.notify();
    }

    fn bounds_for_range(&mut self, range_utf16: Range<usize>, bounds: Bounds<Pixels>, _window: &mut Window, _cx: &mut Context<Self>) -> Option<Bounds<Pixels>> {
        let range = self.range_from_utf16(&range_utf16);
        let (row, column) = self.row_and_column(range.start);
        let line = self.lines.get(row)?;
        let x = line.x_for_index(column);
        Some(Bounds::from_corners(
            point(bounds.left() + x, bounds.top() + self.line_height * row as f32),
            point(bounds.left() + x, bounds.top() + self.line_height * (row + 1) as f32),
        ))
    }

    fn character_index_for_point(&mut self, point: gpui::Point<Pixels>, _window: &mut Window, _cx: &mut Context<Self>) -> Option<usize> {
        Some(self.offset_to_utf16(self.index_for_position(point)))
    }
}

impl TextInput {
    fn row_and_column(&self, offset: usize) -> (usize, usize) {
        let mut start = 0;
        for (index, line) in self.lines.iter().enumerate() {
            let end = start + line.text.len();
            if offset <= end {
                return (index, offset - start);
            }
            start = end + 1;
        }
        (self.lines.len().saturating_sub(1), 0)
    }
}

struct PrepaintState {
    lines: Vec<ShapedLine>,
    cursor: Option<PaintQuad>,
    selections: Vec<PaintQuad>,
}

struct TextElement {
    input: Entity<TextInput>,
    placeholder_color: gpui::Rgba,
    cursor_color: gpui::Rgba,
    selection_color: gpui::Rgba,
}

impl IntoElement for TextElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for TextElement {
    type RequestLayoutState = ();
    type PrepaintState = PrepaintState;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(&mut self, _id: Option<&GlobalElementId>, _inspector_id: Option<&InspectorElementId>, window: &mut Window, cx: &mut App) -> (LayoutId, Self::RequestLayoutState) {
        let (line_height, lines) = {
            let input = self.input.read(cx);
            (input.line_height, input.line_count())
        };
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = (line_height * lines as f32).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(&mut self, _id: Option<&GlobalElementId>, _inspector_id: Option<&InspectorElementId>, bounds: Bounds<Pixels>, _request_layout: &mut Self::RequestLayoutState, window: &mut Window, cx: &mut App) -> Self::PrepaintState {
        let style = window.text_style();
        let (content, cursor, selection, marked_range, placeholder, font_size, line_height) = {
            let input = self.input.read(cx);
            (
                input.content.clone(),
                input.cursor,
                input.selection(),
                input.marked_range.clone(),
                input.placeholder.clone(),
                input.font_size,
                input.line_height,
            )
        };
        let font = style.font();
        let showing_placeholder = content.is_empty();
        let text_color = if showing_placeholder { self.placeholder_color.into() } else { style.color };
        let text: SharedString = if showing_placeholder { placeholder } else { content.clone().into() };

        let mut lines = Vec::new();
        let mut selections = Vec::new();
        let mut cursor_quad = None;
        let mut start = 0;
        let slices: Vec<&str> = if showing_placeholder { vec![text.as_ref()] } else { text.split('\n').collect() };
        for (row, slice) in slices.into_iter().enumerate() {
            let mut runs = vec![TextRun {
                len: slice.len(),
                font: font.clone(),
                color: text_color,
                background_color: None,
                underline: None,
                strikethrough: None,
            }];
            if let Some(marked) = marked_range.as_ref() {
                let local_start = marked.start.saturating_sub(start).min(slice.len());
                let local_end = marked.end.saturating_sub(start).min(slice.len());
                if local_end > local_start {
                    runs = vec![
                        TextRun { len: local_start, ..runs[0].clone() },
                        TextRun {
                            len: local_end - local_start,
                            underline: Some(UnderlineStyle { color: Some(text_color), thickness: px(1.0), wavy: false }),
                            ..runs[0].clone()
                        },
                        TextRun { len: slice.len() - local_end, ..runs[0].clone() },
                    ]
                    .into_iter()
                    .filter(|run| run.len > 0)
                    .collect();
                }
            }
            let line = window.text_system().shape_line(SharedString::from(slice.to_owned()), font_size, &runs, None);
            let top = bounds.top() + line_height * row as f32;
            if !showing_placeholder {
                let line_start = start;
                let line_end = start + slice.len();
                let selected_start = selection.start.max(line_start).min(line_end) - line_start;
                let selected_end = selection.end.max(line_start).min(line_end) - line_start;
                if selected_end > selected_start {
                    selections.push(fill(
                        Bounds::from_corners(
                            point(bounds.left() + line.x_for_index(selected_start), top),
                            point(bounds.left() + line.x_for_index(selected_end), top + line_height),
                        ),
                        self.selection_color,
                    ));
                }
                if cursor >= line_start && cursor <= line_end && selection.is_empty() {
                    let x = bounds.left() + line.x_for_index(cursor - line_start);
                    cursor_quad = Some(fill(Bounds::new(point(x, top + px(2.)), size(px(2.), line_height - px(4.))), self.cursor_color));
                }
            }
            lines.push(line);
            start += slice.len() + 1;
        }

        PrepaintState { lines, cursor: cursor_quad, selections }
    }

    fn paint(&mut self, _id: Option<&GlobalElementId>, _inspector_id: Option<&InspectorElementId>, bounds: Bounds<Pixels>, _request_layout: &mut Self::RequestLayoutState, prepaint: &mut Self::PrepaintState, window: &mut Window, cx: &mut App) {
        let focus_handle = self.input.read(cx).focus_handle.clone();
        window.handle_input(&focus_handle, ElementInputHandler::new(bounds, self.input.clone()), cx);
        for selection in prepaint.selections.drain(..) {
            window.paint_quad(selection);
        }
        let line_height = self.input.read(cx).line_height;
        for (row, line) in prepaint.lines.iter().enumerate() {
            let origin = point(bounds.left(), bounds.top() + line_height * row as f32);
            let _ = line.paint(origin, line_height, TextAlign::Left, None, window, cx);
        }
        if focus_handle.is_focused(window) {
            if let Some(cursor) = prepaint.cursor.take() {
                window.paint_quad(cursor);
            }
        }
        self.input.update(cx, |input, _cx| {
            input.lines = prepaint.lines.clone();
            input.bounds = Some(bounds);
        });
    }
}

impl Render for TextInput {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let submit = self.on_submit.is_some() || self.on_key.is_some();
        div()
            .flex()
            .flex_col()
            .w_full()
            .key_context("Composer")
            .track_focus(&self.focus_handle(cx))
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::home))
            .on_action(cx.listener(Self::end))
            .on_action(cx.listener(Self::insert_newline))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::copy))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .when(submit, |this| {
                this.on_key_down(cx.listener(|input, event: &gpui::KeyDownEvent, window, cx| {
                    // A hook is taken out of the box so it can be handed the box
                    // itself (it is leased right now, so it must not go back
                    // through the entity); it goes straight back afterwards.
                    if let Some(hook) = input.on_key.take() {
                        let used = hook(input, event, window, cx);
                        input.on_key = Some(hook);
                        if used {
                            return;
                        }
                    }
                    if event.keystroke.key == "enter" && !event.keystroke.modifiers.shift {
                        if let Some(submit) = input.on_submit.take() {
                            submit(input, window, cx);
                            input.on_submit = Some(submit);
                        }
                    }
                }))
            })
            .child({
                // The window's own colours, not a fixed dark set: the placeholder
                // was nearly invisible on a light background. It is `text_3` at
                // full strength, so it reads as a hint rather than as text.
                let current = crate::theme::current_colors();
                TextElement {
                    input: cx.entity(),
                    placeholder_color: current.placeholder,
                    cursor_color: current.text_1,
                    selection_color: current.selected_strong,
                }
            })
    }
}

impl Focusable for TextInput {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}
