//! The terminal dock: real shells in the selected project's folder, through
//! Windows ConPTY. Ported from `ui/TerminalDock.tsx` and
//! `apps/desktop/src-tauri/src/pty.rs`.
//!
//! The dock stays mounted while hidden, so hiding it does not end a running
//! command; closing a tab does. Output is only ever painted: it is never logged.
//!
//! Three things are simpler than the installed app, and on purpose:
//! - the dock is resized with the two buttons on its edge (GPUI at this revision
//!   has no pointer-drag tracking for a splitter);
//! - the grid is drawn as text runs with the sixteen ANSI colours, and a cell
//!   cursor, rather than a full VT emulator;
//! - pasting writes the clipboard's text straight to the shell.

pub mod ansi;
pub mod pty;

use gpui::{div, prelude::*, px, AnyElement, App, Context, Rgba, SharedString, Window};

use crate::theme::{self, Colors};
use crate::ui::root::Root;
use crate::ui::terminal::ansi::{grid_for, Colour, Screen, Style as CellStyle};
use crate::ui::terminal::pty::{PtyEvent, ShellKind};

/// The size one character takes, in pixels: the cursor and the pty's own size
/// are worked out from these two numbers.
pub const CHAR_WIDTH: f32 = 7.7;
pub const LINE_HEIGHT: f32 = 17.0;
const FONT_SIZE: f32 = 13.0;
pub const MIN_HEIGHT: f32 = 140.0;
pub const DEFAULT_HEIGHT: f32 = 280.0;

/// One open terminal.
pub struct TerminalTab {
    pub id: String,
    pub shell: ShellKind,
    pub cwd: String,
    pub screen: Screen,
    pub scroll_back: usize,
    pub exit: Option<Option<i32>>,
    /// The size last sent to the pty, so a repaint does not send it again.
    pub size: (u16, u16),
}

/// The dock's own state.
pub struct TerminalState {
    pub open: bool,
    pub tabs: Vec<TerminalTab>,
    pub active: Option<String>,
    pub height: f32,
    pub error: Option<String>,
    /// Whether the shell has the caret: keys go to it and not to the composer.
    pub focused: bool,
    /// The channel the reading threads write to.
    pub events: Option<async_channel::Receiver<PtyEvent>>,
    pub sender: Option<async_channel::Sender<PtyEvent>>,
    pub started: bool,
    /// The last size measured for the dock, and the size the tabs were told.
    pub measured: (f32, f32),
}

impl Default for TerminalState {
    fn default() -> Self {
        TerminalState {
            open: false,
            tabs: Vec::new(),
            active: None,
            height: DEFAULT_HEIGHT,
            error: None,
            focused: false,
            events: None,
            sender: None,
            started: false,
            measured: (0.0, 0.0),
        }
    }
}

impl TerminalState {
    pub fn active(&self) -> Option<&TerminalTab> {
        let id = self.active.as_deref()?;
        self.tabs.iter().find(|tab| tab.id == id)
    }

    pub fn active_mut(&mut self) -> Option<&mut TerminalTab> {
        let id = self.active.clone()?;
        self.tabs.iter_mut().find(|tab| tab.id == id)
    }
}

/// The sixteen colours the terminal draws with, folded from the look's own text
/// colours so a bright red reads as red in both looks.
pub fn palette(colors: Colors) -> [Rgba; 16] {
    if colors.dark {
        [
            gpui::rgb(0x2b2d30),
            gpui::rgb(0xff6b6b),
            gpui::rgb(0x3ddc97),
            gpui::rgb(0xffae42),
            gpui::rgb(0x6ea8ff),
            gpui::rgb(0xd68cf0),
            gpui::rgb(0x5fd1d9),
            gpui::rgb(0xd7d7d4),
            gpui::rgb(0x6b6e73),
            gpui::rgb(0xff8f8f),
            gpui::rgb(0x6ee7b0),
            gpui::rgb(0xffc470),
            gpui::rgb(0x93bdff),
            gpui::rgb(0xe4aef5),
            gpui::rgb(0x8be0e6),
            gpui::rgb(0xf3f3f2),
        ]
    } else {
        [
            gpui::rgb(0x2b2d30),
            gpui::rgb(0xc9304a),
            gpui::rgb(0x117a55),
            gpui::rgb(0x8a5a00),
            gpui::rgb(0x2556b8),
            gpui::rgb(0x8a3fb0),
            gpui::rgb(0x0e7a86),
            gpui::rgb(0x70726f),
            gpui::rgb(0x55585c),
            gpui::rgb(0xd23a52),
            gpui::rgb(0x14875f),
            gpui::rgb(0xa86a00),
            gpui::rgb(0x2f6bd6),
            gpui::rgb(0xa04bc8),
            gpui::rgb(0x12909e),
            gpui::rgb(0x1b1b1a),
        ]
    }
}

fn colour_of(colour: Colour, colors: Colors, palette: &[Rgba; 16]) -> Rgba {
    match colour {
        Colour::Default => colors.text_1,
        Colour::Indexed(index) if (index as usize) < 16 => palette.get(index as usize).copied().unwrap_or(colors.text_1),
        Colour::Indexed(index) => xterm_rgb(index),
        Colour::Rgb(r, g, b) => gpui::rgb(((r as u32) << 16) | ((g as u32) << 8) | b as u32),
    }
}

/// The standard xterm colour for an index from 16 to 255: a 6×6×6 cube from 16
/// to 231, then a grey ramp.
pub fn xterm_rgb(index: u8) -> Rgba {
    const STEPS: [u8; 6] = [0, 95, 135, 175, 215, 255];
    if index >= 232 {
        let level = 8 + (index - 232) * 10;
        return gpui::rgb(((level as u32) << 16) | ((level as u32) << 8) | level as u32);
    }
    let index = index.saturating_sub(16) as usize;
    let r = STEPS[index / 36 % 6];
    let g = STEPS[index / 6 % 6];
    let b = STEPS[index % 6];
    gpui::rgb(((r as u32) << 16) | ((g as u32) << 8) | b as u32)
}

/// One line of the grid, as runs of one style each.
fn render_line(runs: &[(String, CellStyle)], colors: Colors, palette: &[Rgba; 16]) -> AnyElement {
    let mut row = div()
        .w_full()
        .min_w(px(0.0))
        .h(px(LINE_HEIGHT))
        .flex()
        .font_family(theme::MONO_FONT)
        .text_size(px(FONT_SIZE))
        .line_height(px(LINE_HEIGHT))
        .whitespace_nowrap();
    for (text, style) in runs {
        let mut fg = colour_of(style.fg, colors, palette);
        let mut bg = if style.bg == Colour::Default { gpui::rgba(0x00000000) } else { colour_of(style.bg, colors, palette) };
        if style.inverse {
            let text = fg;
            fg = if bg.a > 0.0 { bg } else { colors.panel };
            bg = text;
        }
        if style.dim {
            fg = gpui::Hsla::from(fg).opacity(0.6).into();
        }
        let mut run = div().text_color(fg).bg(bg).child(SharedString::from(text.clone()));
        if style.bold {
            run = run.font_weight(gpui::FontWeight::BOLD);
        }
        row = row.child(run);
    }
    row.into_any_element()
}

impl Root {
    /// Show or hide the dock, starting a shell the first time it is shown.
    pub fn toggle_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.terminal.open = !self.terminal.open;
        if !self.terminal.open {
            self.terminal.focused = false;
            cx.notify();
            return;
        }
        self.ensure_terminal_hub(cx);
        if self.terminal.tabs.is_empty() {
            self.open_terminal(ShellKind::PowerShell, cx);
        }
        // The shell takes the caret, so the next keystroke is typed into it.
        self.terminal.focused = true;
        let handle = self.root_focus.clone();
        window.focus(&handle, cx);
        cx.notify();
    }

    /// The channel from the shells to the window, once.
    fn ensure_terminal_hub(&mut self, cx: &mut Context<Self>) {
        if self.terminal.started {
            return;
        }
        self.terminal.started = true;
        let (sender, receiver) = async_channel::bounded(256);
        self.terminal.sender = Some(sender);
        self.terminal.events = Some(receiver.clone());
        let this = cx.weak_entity();
        // One reader for every terminal: the window only reads a channel.
        cx.spawn(async move |_this, cx: &mut gpui::AsyncApp| {
            while let Ok(event) = receiver.recv().await {
                if this.update(cx, |root, cx| root.terminal_event(event, cx)).is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    fn terminal_event(&mut self, event: PtyEvent, cx: &mut Context<Self>) {
        match event {
            PtyEvent::Output { id, data } => {
                if let Some(tab) = self.terminal.tabs.iter_mut().find(|tab| tab.id == id) {
                    tab.screen.feed(&data);
                    // A scrolled-back view stays where the owner put it.
                    if tab.scroll_back > 0 {
                        tab.scroll_back = tab.scroll_back.min(tab.screen.scrollback_len());
                    }
                }
            }
            PtyEvent::Exit { id, code } => {
                if let Some(tab) = self.terminal.tabs.iter_mut().find(|tab| tab.id == id) {
                    tab.exit = Some(code);
                }
            }
        }
        cx.notify();
    }

    /// The folder a shell starts in: the conversation's project.
    fn terminal_folder(&self) -> Option<String> {
        self.selected_session().and_then(|session| session.project_path.clone()).filter(|path| !path.is_empty())
    }

    /// Start one shell. The process is spawned off the UI thread.
    pub fn open_terminal(&mut self, shell: ShellKind, cx: &mut Context<Self>) {
        self.ensure_terminal_hub(cx);
        let Some(folder) = self.terminal_folder() else {
            self.terminal.error = Some("Open a conversation in a project to start a terminal in its folder.".to_owned());
            cx.notify();
            return;
        };
        let Some(sender) = self.terminal.sender.clone() else { return };
        let (cols, rows) = self.terminal_grid();
        let task = cx.background_executor().spawn(async move { pty::open(&folder, shell, cols, rows, sender) });
        cx.spawn(async move |this, cx: &mut gpui::AsyncApp| {
            let result = task.await;
            cx.update(|app| {
                if let Some(root) = this.upgrade() {
                    root.update(app, |root, cx| {
                        match result {
                            Ok(opened) => {
                                root.terminal.tabs.push(TerminalTab {
                                    id: opened.id.clone(),
                                    shell: opened.shell,
                                    cwd: opened.cwd,
                                    screen: Screen::new(cols as usize, rows as usize),
                                    scroll_back: 0,
                                    exit: None,
                                    size: (cols, rows),
                                });
                                root.terminal.active = Some(opened.id);
                                root.terminal.focused = true;
                                root.terminal.error = None;
                            }
                            Err(error) => root.terminal.error = Some(error),
                        }
                        cx.notify();
                    });
                }
            });
        })
        .detach();
        cx.notify();
    }

    pub fn close_terminal(&mut self, id: String, cx: &mut Context<Self>) {
        pty::close(&id);
        self.terminal.tabs.retain(|tab| tab.id != id);
        if self.terminal.active.as_deref() == Some(id.as_str()) {
            self.terminal.active = self.terminal.tabs.last().map(|tab| tab.id.clone());
        }
        cx.notify();
    }

    /// The grid the dock currently holds, from the last measured size.
    fn terminal_grid(&self) -> (u16, u16) {
        let (width, height) = self.terminal.measured;
        if width <= 0.0 || height <= 0.0 {
            return (80, 20);
        }
        grid_for(width, height, CHAR_WIDTH, LINE_HEIGHT)
    }

    /// Whether keys belong to the shell: the dock is open, a tab is there, and
    /// the window's own focus is on the root (not in a text field).
    pub(crate) fn terminal_captures_keys(&self, window: &Window, cx: &App) -> bool {
        if !self.terminal.open || !self.terminal.focused || self.terminal.active().is_none() {
            return false;
        }
        window.focused(cx).map(|handle| handle == self.root_focus).unwrap_or(false)
    }

    /// One keystroke for the shell; `true` when it was used.
    pub(crate) fn terminal_key(&mut self, event: &gpui::KeyDownEvent, cx: &mut Context<Self>) -> bool {
        let Some(tab_id) = self.terminal.active.clone() else { return false };
        let keystroke = &event.keystroke;
        let ctrl = keystroke.modifiers.control;
        let key = keystroke.key.to_lowercase();
        if key == "escape" && keystroke.modifiers.shift {
            self.terminal.focused = false;
            cx.notify();
            return true;
        }
        let data: Option<String> = match key.as_str() {
            "enter" => Some("\r".to_owned()),
            "backspace" => Some("\u{7f}".to_owned()),
            "tab" => Some("\t".to_owned()),
            "escape" => Some("\u{1b}".to_owned()),
            "up" => Some("\u{1b}[A".to_owned()),
            "down" => Some("\u{1b}[B".to_owned()),
            "right" => Some("\u{1b}[C".to_owned()),
            "left" => Some("\u{1b}[D".to_owned()),
            "home" => Some("\u{1b}[H".to_owned()),
            "end" => Some("\u{1b}[F".to_owned()),
            "delete" => Some("\u{1b}[3~".to_owned()),
            "space" => Some(" ".to_owned()),
            "pageup" => {
                if let Some(tab) = self.terminal.active_mut() {
                    tab.scroll_back = (tab.scroll_back + 10).min(tab.screen.scrollback_len());
                }
                cx.notify();
                return true;
            }
            "pagedown" => {
                if let Some(tab) = self.terminal.active_mut() {
                    tab.scroll_back = tab.scroll_back.saturating_sub(10);
                }
                cx.notify();
                return true;
            }
            "v" if ctrl => {
                if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                    let _ = pty::write(&tab_id, &text);
                }
                return true;
            }
            "c" if ctrl => {
                let _ = pty::write(&tab_id, "\u{3}");
                return true;
            }
            _ => None,
        };
        let data = data.or_else(|| {
            if keystroke.modifiers.control || keystroke.modifiers.alt || keystroke.modifiers.platform {
                None
            } else {
                keystroke.key_char.clone()
            }
        });
        let Some(data) = data else { return false };
        if let Err(error) = pty::write(&tab_id, &data) {
            self.terminal.error = Some(error);
        }
        // Typing at the bottom returns the view to the bottom.
        if let Some(tab) = self.terminal.active_mut() {
            if tab.scroll_back > 0 {
                tab.scroll_back = 0;
            }
        }
        cx.notify();
        true
    }

    /// The dock, at the bottom of the conversation view.
    pub(crate) fn render_terminal_dock(&mut self, colors: Colors, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let height = self.terminal.height;
        let palette = palette(colors);
        let active_id = self.terminal.active.clone();
        let focused = self.terminal.focused;
        let tabs: Vec<(String, ShellKind, String, Option<Option<i32>>)> = self
            .terminal
            .tabs
            .iter()
            .map(|tab| (tab.id.clone(), tab.shell, tab.cwd.clone(), tab.exit))
            .collect();

        let mut header = div()
            .h(px(30.0))
            .flex_none()
            .px(px(8.0))
            .flex()
            .items_center()
            .gap(px(6.0))
            .border_b_1()
            .border_color(colors.line);
        let mut tab_row = div().flex().items_center().gap(px(4.0));
        for (id, shell, cwd, exit) in &tabs {
            let active = active_id.as_deref() == Some(id.as_str());
            let tab_id = id.clone();
            let folder = cwd.replace('\\', "/").trim_end_matches('/').rsplit('/').next().unwrap_or(cwd).to_owned();
            let mut tab = div()
                .id(SharedString::from(format!("terminal-tab-{id}")))
                .h(px(22.0))
                .px(px(7.0))
                .flex()
                .items_center()
                .gap(px(5.0))
                .rounded(px(theme::RADIUS_SM))
                .bg(if active { colors.selected } else { gpui::rgba(0x00000000) })
                .text_size(px(theme::FS_2XS))
                .text_color(if active { colors.text_1 } else { colors.text_2 })
                .cursor_pointer()
                .hover(move |style| style.bg(colors.hover))
                .child(SharedString::from(shell.label()))
                .child(div().text_color(colors.text_3).child(SharedString::from(folder)))
                .when(exit.is_some(), |this| this.child(div().text_color(colors.text_3).child("(ended)")))
                .on_click(cx.listener(move |root, _: &gpui::ClickEvent, window, cx| {
                    root.terminal.active = Some(tab_id.clone());
                    root.terminal.focused = true;
                    let handle = root.root_focus.clone();
                    window.focus(&handle, cx);
                    cx.notify();
                }));
            let close_id = id.clone();
            tab = tab.child(
                div()
                    .id(SharedString::from(format!("terminal-close-{id}")))
                    .w(px(14.0))
                    .h(px(14.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(3.0))
                    .cursor_pointer()
                    .hover(move |style| style.bg(colors.selected_strong))
                    .child("✕")
                    .on_click(cx.listener(move |root, _: &gpui::ClickEvent, _window, cx| {
                        cx.stop_propagation();
                        root.close_terminal(close_id.clone(), cx);
                    })),
            );
            tab_row = tab_row.child(tab);
        }
        header = header.child(tab_row);
        for shell in [ShellKind::PowerShell, ShellKind::PowerShell7, ShellKind::CommandPrompt] {
            header = header.child(
                div()
                    .id(SharedString::from(format!("terminal-new-{}", shell.id())))
                    .h(px(22.0))
                    .px(px(7.0))
                    .flex()
                    .items_center()
                    .rounded(px(theme::RADIUS_SM))
                    .text_size(px(theme::FS_2XS))
                    .text_color(colors.text_3)
                    .cursor_pointer()
                    .hover(move |style| style.bg(colors.hover).text_color(colors.text_1))
                    .child(SharedString::from(format!("+ {}", shell.label())))
                    .on_click(cx.listener(move |root, _: &gpui::ClickEvent, _window, cx| {
                        root.open_terminal(shell, cx);
                    })),
            );
        }
        header = header
            .child(div().flex_1())
            .child(
                div()
                    .id("terminal-smaller")
                    .h(px(20.0))
                    .px(px(6.0))
                    .flex()
                    .items_center()
                    .rounded(px(theme::RADIUS_SM))
                    .text_size(px(theme::FS_2XS))
                    .text_color(colors.text_3)
                    .cursor_pointer()
                    .hover(move |style| style.bg(colors.hover))
                    .child("−")
                    .on_click(cx.listener(|root, _: &gpui::ClickEvent, _window, cx| {
                        root.terminal.height = (root.terminal.height - 40.0).max(MIN_HEIGHT);
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .id("terminal-bigger")
                    .h(px(20.0))
                    .px(px(6.0))
                    .flex()
                    .items_center()
                    .rounded(px(theme::RADIUS_SM))
                    .text_size(px(theme::FS_2XS))
                    .text_color(colors.text_3)
                    .cursor_pointer()
                    .hover(move |style| style.bg(colors.hover))
                    .child("+")
                    .on_click(cx.listener(|root, _: &gpui::ClickEvent, _window, cx| {
                        root.terminal.height = (root.terminal.height + 40.0).min(560.0);
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .id("terminal-hide")
                    .h(px(20.0))
                    .px(px(6.0))
                    .flex()
                    .items_center()
                    .rounded(px(theme::RADIUS_SM))
                    .text_size(px(theme::FS_2XS))
                    .text_color(colors.text_3)
                    .cursor_pointer()
                    .hover(move |style| style.bg(colors.hover))
                    .child("Hide")
                    .on_click(cx.listener(|root, _: &gpui::ClickEvent, _window, cx| {
                        root.hide_terminal(cx);
                    })),
            );

        // The grid of the active tab.
        let mut body = div().flex_1().min_h(px(0.0)).w_full().overflow_hidden().relative();
        if let Some(tab) = self.terminal.tabs.iter().find(|tab| active_id.as_deref() == Some(tab.id.as_str())) {
            let scroll = tab.scroll_back;
            let lines = tab.screen.visible(scroll);
            let mut grid = div().w_full().flex().flex_col();
            for runs in &lines {
                grid = grid.child(render_line(runs, colors, &palette));
            }
            body = body.child(grid);
            // The cursor, at the cell the screen says.
            if scroll == 0 && tab.screen.cursor_visible() {
                let (row, col) = tab.screen.cursor();
                body = body.child(
                    div()
                        .absolute()
                        .top(px(row as f32 * LINE_HEIGHT))
                        .left(px(col as f32 * CHAR_WIDTH))
                        .w(px(if focused { CHAR_WIDTH } else { 2.0 }))
                        .h(px(LINE_HEIGHT))
                        .bg(if focused { gpui::Hsla::from(colors.text_1).opacity(0.6) } else { gpui::Hsla::from(colors.text_1).opacity(0.5) }),
                );
            }
            if tab.exit.is_some() {
                body = body.child(
                    div()
                        .absolute()
                        .bottom_0()
                        .left_0()
                        .px(px(8.0))
                        .py(px(2.0))
                        .text_size(px(theme::FS_2XS))
                        .text_color(colors.text_3)
                        .child(SharedString::from(match tab.exit {
                            Some(Some(code)) => format!("The shell ended with code {code}."),
                            _ => "The shell ended.".to_owned(),
                        })),
                );
            }
        } else {
            body = body.child(
                div()
                    .p(px(10.0))
                    .text_size(px(theme::FS_XS))
                    .text_color(colors.text_3)
                    .child(if self.terminal_folder().is_some() {
                        "No terminal is open. Choose a shell above."
                    } else {
                        "Open a conversation in a project to start a terminal in its folder."
                    }),
            );
        }

        let body_wrapped = div()
            .flex_1()
            .min_h(px(0.0))
            .w_full()
            .relative()
            .on_mouse_down(gpui::MouseButton::Left, cx.listener(|root, _: &gpui::MouseDownEvent, window, cx| {
                root.terminal.focused = true;
                let handle = root.root_focus.clone();
                window.focus(&handle, cx);
                cx.notify();
            }))
            .child(body);

        // The pane's own width, from the window's: the sidebar and the
        // conversations column are fixed, and the rail is a known width.
        let window_width: f32 = window.bounds().size.width.into();
        let columns = theme::SIDEBAR_WIDTH + theme::CONVERSATIONS_WIDTH + if self.changes.rail_open { 288.0 } else { 0.0 };
        let grid_width = (window_width - columns).max(0.0);
        let grid_height = (height - 30.0).max(0.0);
        self.apply_terminal_size(grid_width, grid_height, cx);

        div()
            .id("terminal-dock")
            .h(px(height.max(MIN_HEIGHT)))
            .flex_none()
            .w_full()
            .flex()
            .flex_col()
            .bg(colors.panel)
            .border_t_1()
            .border_color(colors.line)
            .when_some(self.terminal.error.clone(), |this, error| {
                this.child(div().px(px(8.0)).py(px(3.0)).text_size(px(theme::FS_2XS)).text_color(colors.status_danger).child(SharedString::from(error)))
            })
            .child(header)
            .child(body_wrapped)
            .into_any_element()
    }

    /// One measured size per frame decides the pty's grid, and a new size is
    /// sent only when the grid really changed.
    fn apply_terminal_size(&mut self, width: f32, height: f32, cx: &mut Context<Self>) {
        if width <= 0.0 || height <= 0.0 {
            return;
        }
        let previous = self.terminal.measured;
        self.terminal.measured = (width, height);
        if (previous.0 - width).abs() < 1.0 && (previous.1 - height).abs() < 1.0 {
            return;
        }
        let (cols, rows) = grid_for(width, height, CHAR_WIDTH, LINE_HEIGHT);
        let ids: Vec<(String, (u16, u16))> = self.terminal.tabs.iter().map(|tab| (tab.id.clone(), tab.size)).collect();
        for (id, size) in ids {
            if size == (cols, rows) {
                continue;
            }
            if let Some(tab) = self.terminal.tabs.iter_mut().find(|tab| tab.id == id) {
                tab.size = (cols, rows);
                tab.screen.resize(cols as usize, rows as usize);
            }
            let _ = pty::resize(&id, cols, rows);
        }
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::terminal::ansi::Screen;

    #[test]
    fn the_dock_starts_closed_and_empty() {
        let state = TerminalState::default();
        assert!(!state.open);
        assert!(state.tabs.is_empty());
        assert!(state.active().is_none());
        assert_eq!(state.height, DEFAULT_HEIGHT);
    }

    #[test]
    fn the_grid_comes_from_the_measured_size_and_the_character_metrics() {
        // 800 / 7.7 = 103 columns; 340 / 17 = 20 rows.
        assert_eq!(grid_for(800.0, 340.0, CHAR_WIDTH, LINE_HEIGHT), (103, 20));
    }

    #[test]
    fn a_tab_keeps_its_own_screen_and_scroll() {
        let mut state = TerminalState::default();
        state.tabs.push(TerminalTab {
            id: "t1".into(),
            shell: ShellKind::PowerShell,
            cwd: "D:/work".into(),
            screen: Screen::new(80, 24),
            scroll_back: 0,
            exit: None,
            size: (80, 24),
        });
        state.active = Some("t1".into());
        state.active_mut().unwrap().screen.feed("hello");
        assert_eq!(state.active().unwrap().screen.cursor(), (0, 5));
        assert_eq!(state.active().unwrap().cwd, "D:/work");
    }

    #[test]
    fn the_palette_has_sixteen_colours_in_both_looks() {
        let dark = palette(theme::haze_dark());
        let light = palette(theme::haze_light());
        assert_eq!(dark.len(), 16);
        assert_eq!(light.len(), 16);
        assert_ne!(dark[1], light[1], "a red reads differently on each background");
    }
}
