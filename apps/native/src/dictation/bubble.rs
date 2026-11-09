//! The dictation bubble: a small window above the companion that shows the
//! session while the companion is hidden (`DictationBubble.tsx`).
//!
//! It is a recorder only: the timer, the microphone's level bars and the one
//! message it ever says — what went wrong, in the React app's own words. The
//! live words ride above those bars (the brief asks for them; the React island
//! deliberately shows none). A tap opens the main window; a drag moves it.

#![allow(dead_code)]  // A ported surface: the helpers are complete and
// slice 6's settings page and slice 7 wire the rest.

use gpui::{
    div, prelude::*, px, AnyWindowHandle, App, Context, Entity, MouseButton, SharedString, Window,
    WindowAppearance, WindowBackgroundAppearance, WindowBounds, WindowHandle, WindowKind,
    WindowOptions,
};

use crate::companion::window as floating;
use crate::companion::view::{companion_colors, format_seconds, level_bars};
use crate::dictation::copy::IslandPhase;
use crate::dictation::{BUBBLE_COMPACT, BUBBLE_GAP, Dictation};
use crate::theme::{self, Colors};

/// A move in progress, and whether it is a tap rather than a drag.
struct Press {
    cursor: (i32, i32),
    rect: (i32, i32, i32, i32),
    moved: i32,
}

pub struct DictationBubble {
    dictation: Entity<Dictation>,
    window: Option<AnyWindowHandle>,
    hwnd: Option<isize>,
    system_is_light: bool,
    press: Option<Press>,
    /// Kept alive so the bubble repaints when the session changes.
    _subscription: gpui::Subscription,
}

impl DictationBubble {
    pub fn new(dictation: Entity<Dictation>, system_is_light: bool, cx: &mut Context<Self>) -> Self {
        let _subscription = cx.observe(&dictation, |bubble, _dictation, cx| bubble.on_dictation(cx));
        Self { dictation, window: None, hwnd: None, system_is_light, press: None, _subscription }
    }

    fn colors(&self) -> Colors {
        companion_colors(!self.system_is_light)
    }

    fn on_dictation(&mut self, cx: &mut Context<Self>) {
        cx.notify();
    }

    fn adopt_window(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.hwnd.is_some() {
            return;
        }
        let Some(hwnd) = floating::hwnd_of(window) else { return };
        self.hwnd = Some(hwnd);
        self.window = Some(window.window_handle());
        let handle = window.window_handle();
        self.dictation.update(cx, |dictation, _| dictation.attach_bubble(handle, hwnd));
    }

    fn begin_press(&mut self, cx: &mut Context<Self>) {
        let Some(hwnd) = self.hwnd else { return };
        let (Some(cursor), Some(rect)) = (floating::cursor(), floating::window_rect(hwnd)) else { return };
        self.press = Some(Press { cursor, rect, moved: 0 });
        cx.notify();
    }

    fn drag(&mut self, cx: &mut Context<Self>) {
        let Some(press) = self.press.as_ref() else { return };
        let Some(hwnd) = self.hwnd else { return };
        let Some(cursor) = floating::cursor() else { return };
        let (dx, dy) = (cursor.0 - press.cursor.0, cursor.1 - press.cursor.1);
        if dx.abs() + dy.abs() < 4 {
            return;
        }
        let moved = (press.rect.0 + dx, press.rect.1 + dy, press.rect.2, press.rect.3);
        floating::place(cx.background_executor(), hwnd, moved.0, moved.1, moved.2, moved.3);
        if let Some(press) = self.press.as_mut() {
            press.moved = dx.abs() + dy.abs();
        }
        cx.notify();
    }

    /// A tap, rather than a drag, opens the main window; a drag is clamped back
    /// into the display it ended on.
    fn end_press(&mut self, cx: &mut Context<Self>) {
        let Some(press) = self.press.take() else { return };
        let Some(hwnd) = self.hwnd else { return };
        if press.moved < 4 {
            if let Some(handle) = self.dictation.read(cx).main_handle() {
                let _ = handle.update(cx, |_, window, _| window.activate_window());
            }
            return;
        }
        if let Some(rect) = floating::window_rect(hwnd) {
            let clamped = crate::companion::positions::clamp_into(
                &floating::work_areas(),
                rect,
                (rect.0 + rect.2 / 2, rect.1 + rect.3 / 2),
            );
            floating::place(cx.background_executor(), hwnd, clamped.0, clamped.1, clamped.2, clamped.3);
        }
        cx.notify();
    }
}

impl Render for DictationBubble {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.adopt_window(window, cx);
        let colors = self.colors();
        let dictation = self.dictation.read(cx);
        let phase = dictation.phase;
        let (level, warming, error) = (dictation.level, dictation.warming, dictation.error.clone());
        let seconds = dictation.display_seconds();
        let words = dictation.live_words().to_owned();
        let note = dictation.clipboard_note.clone();
        if phase == crate::dictation::Phase::Idle {
            return div().size_full().into_any_element();
        }
        let island = phase.island().unwrap_or(IslandPhase::Listening);
        let bars = level_bars(level, island);
        let shown = if island == IslandPhase::Error { crate::dictation::copy::friendly_error(&error) } else { String::new() };

        let mut pill = div()
            .id("dictation-bubble")
            .size_full()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .p(px(10.0))
            .rounded(px(16.0))
            .bg(colors.panel)
            .border_1()
            .border_color(colors.line)
            .overflow_hidden()
            .on_mouse_down(MouseButton::Left, cx.listener(|bubble, _: &gpui::MouseDownEvent, _window, cx| bubble.begin_press(cx)))
            .on_mouse_move(cx.listener(|bubble, _: &gpui::MouseMoveEvent, _window, cx| bubble.drag(cx)))
            .on_mouse_up(MouseButton::Left, cx.listener(|bubble, _: &gpui::MouseUpEvent, _window, cx| bubble.end_press(cx)));

        let mut row = div().flex().items_center().gap(px(10.0));
        row = row.child(div().flex_none().size(px(8.0)).rounded(px(4.0)).bg(colors.status_danger));
        row = row.child(div().text_size(px(theme::FS_XS)).text_color(colors.text_1).child(SharedString::from(format_seconds(seconds))));
        if let Some(label) = island.note(warming) {
            row = row.child(div().text_size(px(theme::FS_2XS)).text_color(colors.text_3).child(SharedString::from(label)));
        }
        row = row.child(
            div()
                .flex()
                .flex_1()
                .justify_end()
                .items_end()
                .gap(px(2.0))
                .children(bars.into_iter().map(|height| div().flex_none().w(px(3.0)).h(px(height)).rounded(px(2.0)).bg(colors.text_3))),
        );
        pill = pill.child(row);
        if island == IslandPhase::Listening && !words.is_empty() {
            pill = pill.child(
                div()
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_hidden()
                    .text_size(px(theme::FS_XS))
                    .text_color(colors.text_2)
                    .child(SharedString::from(words)),
            );
        }
        if !shown.is_empty() {
            pill = pill.child(div().flex_1().text_size(px(theme::FS_2XS)).text_color(colors.status_danger).child(SharedString::from(shown)));
        }
        if let Some(note) = note {
            pill = pill.child(div().text_size(px(theme::FS_2XS)).text_color(colors.text_3).child(SharedString::from(note)));
        }
        pill.into_any_element()
    }
}

/// Open the bubble window. It sits above the companion island, transparent and
/// frameless like the companion, and starts hidden.
pub fn open(dictation: Entity<Dictation>, cx: &mut App) -> gpui::Result<WindowHandle<DictationBubble>> {
    let bounds = bubble_bounds(cx);
    cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: None,
            focus: false,
            show: false,
            kind: WindowKind::PopUp,
            is_movable: true,
            is_resizable: false,
            is_minimizable: false,
            display_id: None,
            window_background: WindowBackgroundAppearance::Transparent,
            app_id: None,
            window_min_size: None,
            window_decorations: None,
            tabbing_identifier: None,
        },
        move |window, cx| {
            let appearance = window.appearance();
            let system_is_light = appearance != WindowAppearance::Dark && appearance != WindowAppearance::VibrantDark;
            cx.new(|cx| DictationBubble::new(dictation, system_is_light, cx))
        },
    )
}

fn bubble_bounds(cx: &mut App) -> gpui::Bounds<gpui::Pixels> {
    let areas = floating::work_areas();
    let area = areas
        .first()
        .cloned()
        .unwrap_or(crate::companion::positions::WorkArea { x: 0, y: 0, width: 1920, height: 1080, name: String::new() });
    let scale = cx.primary_display().map(|_| 1.0_f32).unwrap_or(1.0);
    let width = BUBBLE_COMPACT.0 as i32;
    let height = BUBBLE_COMPACT.1 as i32;
    let gap = (BUBBLE_GAP * scale).round() as i32;
    let x = area.x + (area.width - width) / 2;
    let y = (area.y + area.height - height - gap).max(area.y);
    gpui::Bounds::new(
        gpui::point(px(x as f32), px(y as f32)),
        gpui::Size { width: px(BUBBLE_COMPACT.0), height: px(BUBBLE_COMPACT.1) },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bubble_and_the_island_agree_on_their_sizes() {
        assert!(crate::dictation::BUBBLE_EXPANDED.0 > BUBBLE_COMPACT.0, "an error grows the bubble");
        assert!(crate::dictation::BUBBLE_EXPANDED.1 > BUBBLE_COMPACT.1);
        assert!(BUBBLE_GAP > BUBBLE_COMPACT.1, "the bubble sits above the companion");
    }
}
