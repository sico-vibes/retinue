//! Motion, ported from `ui/Marquee.tsx`, `ui/marquee.css`, `ui/Collapse.tsx`,
//! `ui/collapse.css` and the working shimmer in `ui/projectsSidebar.css:50-52`.
//!
//! Two things differ from the browser. GPUI cannot clip a gradient to text, so
//! the shimmer is a soft band of the text colour travelling over the word rather
//! than a gradient inside its glyphs. And a tree's height is known only after
//! layout, so a folding list animates the height it is told (a row count times a
//! row height), which is every list this window folds.
//!
//! With Windows' "Show animations" off (`SPI_GETCLIENTAREAANIMATION`), every one
//! of these is instant and the shimmer is the plain word.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use gpui::{
    div, prelude::*, px, Animation, AnimationExt, Font, Hsla, Pixels, SharedString, TextRun,
    Window,
};
use crate::theme::{self, Colors};

/// `ps-shimmer` runs for 1.9 s a sweep (`projectsSidebar.css:51`).
pub const SHIMMER_MS: u64 = 1900;
/// `collapse.css:2`: 180 ms for the height, 160 ms for the chevron.
pub const COLLAPSE_MS: u64 = 180;
/// `Marquee.tsx`: pixels per second the title travels.
pub const MARQUEE_SPEED: f32 = 28.0;
/// `Marquee.tsx`: seconds of pause at each end of one cycle.
pub const MARQUEE_PAUSES: f32 = 3.2;

static ANIMATIONS: AtomicBool = AtomicBool::new(true);

/// Whether Windows shows animations. Asked once at startup and again whenever
/// the window appearance changes; a syscall per frame would be silly.
pub fn animations_enabled() -> bool {
    ANIMATIONS.load(Ordering::Relaxed)
}

/// Re-read the Windows setting. Returns the new value.
pub fn refresh_animations() -> bool {
    let enabled = system_animations();
    ANIMATIONS.store(enabled, Ordering::Relaxed);
    enabled
}

#[cfg(windows)]
fn system_animations() -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SystemParametersInfoW, SPI_GETCLIENTAREAANIMATION,
    };
    let mut enabled: windows_sys::Win32::Foundation::BOOL = 1;
    // `SPI_GETCLIENTAREAANIMATION` fills a BOOL; zero means "show animations" is off.
    let ok = unsafe {
        SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            &mut enabled as *mut _ as *mut core::ffi::c_void,
            0,
        )
    };
    ok != 0 && enabled != 0
}

#[cfg(not(windows))]
fn system_animations() -> bool {
    true
}

/// `cubic-bezier(.2, .7, .2, 1)`, close enough with a standard ease-out.
pub fn ease_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t) * (1.0 - t) * (1.0 - t)
}

/// `cubic-bezier(.45, 0, .55, 1)` from the marquee's keyframes.
pub fn ease_in_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
    }
}

/// One marquee cycle, in seconds: `marqueeTiming` in `Marquee.tsx`.
pub fn marquee_seconds(shift: f32) -> f32 {
    round_to_tenth(MARQUEE_PAUSES + (2.0 * shift) / MARQUEE_SPEED)
}

fn round_to_tenth(value: f32) -> f32 {
    (value * 10.0).round() / 10.0
}

/// The horizontal offset at `progress` (0..1) of a marquee cycle: still for the
/// first 18%, travel to the end by 50%, hold until 68%, then return.
pub fn marquee_offset(progress: f32, shift: f32) -> f32 {
    let progress = progress.rem_euclid(1.0);
    let (out, back) = (0.18, 0.50);
    if progress < out {
        0.0
    } else if progress < back {
        -shift * ease_in_out((progress - out) / (back - out))
    } else if progress < 0.68 {
        -shift
    } else {
        -shift * (1.0 - ease_in_out((progress - 0.68) / 0.32))
    }
}

/// The width of one line of text, shaped by the window's cache: the browser reads
/// `scrollWidth` for the same reason.
pub fn text_width(window: &mut Window, text: &str, size: Pixels) -> f32 {
    if text.is_empty() {
        return 0.0;
    }
    let run = TextRun {
        len: text.len(),
        font: Font { family: theme::UI_FONT.into(), ..Default::default() },
        color: Hsla::default(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let shaped = window
        .text_system()
        .shape_line(SharedString::from(text.to_owned()), size, &[run], None);
    f32::from(shaped.width)
}

/// A single line that scrolls when it overflows, and stays put otherwise.
///
/// `available` is the width the row gives it; `active` is a selected row, and
/// `hovered` is the row under the pointer. `id` must be stable per row.
#[allow(clippy::too_many_arguments)]
pub fn marquee(
    id: SharedString,
    text: SharedString,
    size: Pixels,
    available: f32,
    active: bool,
    hovered: bool,
    colors: Colors,
    window: &mut Window,
) -> gpui::AnyElement {
    let width = text_width(window, &text, size);
    let shift = (width - available).max(0.0);
    let moving = shift > 1.0 && animations_enabled() && (active || hovered);
    // At rest a long line ends in an ellipsis, as `marquee.css` has it; only the
    // moving copy drops it, because the text slides into view instead.
    let base = div()
        .flex_1()
        .min_w(px(0.0))
        .text_size(size)
        .text_color(colors.text_1);
    if !moving {
        return base.truncate().child(text).into_any_element();
    }
    let base = base.overflow_hidden().whitespace_nowrap();
    let seconds = marquee_seconds(shift);
    let inner = div()
        .w(px(after_width(width)))
        .whitespace_nowrap()
        .text_size(size)
        .text_color(colors.text_1)
        .child(text);
    base.child(
        inner.with_animation(
            id,
            Animation::new(Duration::from_secs_f32(seconds))
                .repeat()
                .with_easing(|delta| delta),
            move |element, delta| element.left(px(marquee_offset(delta, shift))),
        ),
    )
    .into_any_element()
}

fn after_width(width: f32) -> f32 {
    // A pixel of slack keeps the last glyph off the clip edge.
    width + 1.0
}

/// "Working", with a soft band of the text colour sweeping across it.
pub fn working_word(id: SharedString, colors: Colors) -> gpui::AnyElement {
    let word = div()
        .flex_none()
        .text_size(px(theme::FS_2XS))
        .font_weight(gpui::FontWeight::MEDIUM)
        .text_color(colors.status_working)
        .child("Working");
    if !animations_enabled() {
        return word.into_any_element();
    }
    // The band travels over a fixed window the width of the word's row. GPUI has
    // no text clipping, so the band is drawn over the glyphs: a light sheen in
    // both looks, fading in and out at its edges.
    let peak: Hsla = colors.text_1.into();
    let sheen = |alpha: f32, left_side: bool| {
        let clear = gpui::linear_color_stop(peak.opacity(0.0), 0.0);
        let hot = gpui::linear_color_stop(peak.opacity(alpha), 1.0);
        let background = if left_side {
            gpui::linear_gradient(90.0, clear, hot)
        } else {
            gpui::linear_gradient(90.0, hot, clear)
        };
        div().h_full().w(px(14.0)).bg(background)
    };
    let travel = 96.0;
    let band = div()
        .absolute()
        .top_0()
        .h_full()
        .flex()
        .child(sheen(0.30, true))
        .child(sheen(0.30, false))
        .with_animation(
            id,
            Animation::new(Duration::from_millis(SHIMMER_MS))
                .repeat()
                .with_easing(|delta| delta),
            move |element, delta| element.left(px(-28.0 + delta * travel)),
        );
    div()
        .relative()
        .flex_none()
        .h(px(14.0))
        .overflow_hidden()
        .child(word)
        .child(band)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_marquee_cycle_pauses_at_both_ends() {
        assert_eq!(marquee_offset(0.0, 40.0), 0.0);
        assert_eq!(marquee_offset(0.17, 40.0), 0.0, "still for the first 18%");
        assert!((marquee_offset(0.5, 40.0) + 40.0).abs() < 0.01, "fully out at 50%");
        assert!((marquee_offset(0.6, 40.0) + 40.0).abs() < 0.01, "held until 68%");
        assert!(marquee_offset(0.9, 40.0) > -40.0, "on its way back");
        assert_eq!(marquee_offset(1.0, 40.0), 0.0);
    }

    #[test]
    fn a_marquee_cycle_is_the_pauses_plus_the_travel() {
        // (3.2 s of pauses + 2 × 28 px / 28 px per second), rounded to a tenth.
        assert_eq!(marquee_seconds(28.0), 5.2);
        assert_eq!(marquee_seconds(0.0), 3.2);
    }

    #[test]
    fn the_easings_start_at_nothing_and_finish_at_one() {
        assert_eq!(ease_out(0.0), 0.0);
        assert_eq!(ease_out(1.0), 1.0);
        assert_eq!(ease_in_out(0.0), 0.0);
        assert_eq!(ease_in_out(1.0), 1.0);
        assert!(ease_out(0.5) > 0.5, "ease-out is ahead in the middle");
        assert!((ease_in_out(0.5) - 0.5).abs() < 0.001);
    }
}
