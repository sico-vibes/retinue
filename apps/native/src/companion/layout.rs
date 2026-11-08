//! The companion window's sizes, presets and limits, in logical pixels.
//!
//! Ported from `src/blob/companionLayout.ts` and the constants in
//! `src-tauri/src/lib.rs` (`COMPANION_MIN_SIZE`, `COMPANION_DEFAULT_SIZE`,
//! `COMPANION_LARGE_SIZE`, `clamp_companion_size`). The Rust shell wins over the
//! TypeScript copy where they disagree, exactly as the React app notes.

#![allow(dead_code)]  // A ported surface: the helpers are complete and
// slice 6's settings page and slice 7 wire the rest.

use crate::companion::fsm::Mode;

/// The compact bar (`set_companion_mode`'s `"petit" | "hidden"`).
pub const COMPACT: (f32, f32) = (344.0, 62.0);
/// The welcome greeting: `COMPANION_WINDOW.greeting` (`EXPANDED_W` × `OVERVIEW_H`).
pub const GREETING: (f32, f32) = (640.0, 150.0);
/// `COMPANION_MIN_SIZE`.
pub const EXPANDED_MIN: (f32, f32) = (520.0, 340.0);
/// `COMPANION_DEFAULT_SIZE`.
pub const EXPANDED_DEFAULT: (f32, f32) = (640.0, 420.0);
/// `COMPANION_LARGE_SIZE`.
pub const EXPANDED_LARGE: (f32, f32) = (960.0, 720.0);
/// `companion_position`'s inset from the work-area edges.
pub const WORK_AREA_INSET: f32 = 12.0;
/// Every launch sits this fraction of the work area's height above its bottom.
pub const LAUNCH_BOTTOM_GAP: f32 = 0.18;

/// The three size presets the React app offers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Preset {
    /// Back to the size the owner dragged to.
    Normal,
    /// Big enough to work beside another app.
    Large,
    /// Forget the dragged size.
    Default,
}

impl Preset {
    pub fn key(self) -> &'static str {
        match self {
            Preset::Normal => "normal",
            Preset::Large => "large",
            Preset::Default => "default",
        }
    }

    pub fn from_key(value: &str) -> Option<Self> {
        match value {
            "normal" => Some(Preset::Normal),
            "large" => Some(Preset::Large),
            "default" => Some(Preset::Default),
            _ => None,
        }
    }

    /// The size a preset asks for before clamping, and whether it is the large one.
    pub fn target(self, saved: Option<(f32, f32)>) -> ((f32, f32), bool) {
        match self {
            Preset::Large => (EXPANDED_LARGE, true),
            Preset::Normal => (saved.unwrap_or(EXPANDED_DEFAULT), false),
            Preset::Default => (EXPANDED_DEFAULT, false),
        }
    }
}

/// At least the minimum, at most the work area (the area wins on a screen
/// smaller than the minimum). `clamp_companion_size`.
pub fn clamp_size(width: f32, height: f32, max_width: f32, max_height: f32) -> (f32, f32) {
    let width = if width.is_finite() { width } else { EXPANDED_DEFAULT.0 };
    let height = if height.is_finite() { height } else { EXPANDED_DEFAULT.1 };
    (
        width.max(EXPANDED_MIN.0).min(max_width.max(1.0)),
        height.max(EXPANDED_MIN.1).min(max_height.max(1.0)),
    )
}

/// The window's size for a mode. The compact bar is fixed; the greeting has its
/// own size; the expanded companion keeps the size the owner dragged it to,
/// clamped to the display.
pub fn size_for(mode: Mode, saved: Option<(f32, f32)>, area: (f32, f32)) -> (f32, f32) {
    match mode {
        Mode::Petit | Mode::Hidden => COMPACT,
        Mode::Welcome => GREETING,
        Mode::Home => clamp_size(
            saved.map(|size| size.0).unwrap_or(EXPANDED_DEFAULT.0),
            saved.map(|size| size.1).unwrap_or(EXPANDED_DEFAULT.1),
            area.0,
            area.1,
        ),
    }
}

/// Where a launch lands: horizontally centred, a little above the bottom edge
/// (`companion_launch_spot`). The clamp is the caller's, because it needs the
/// physical work area.
pub fn launch_offset(area_width: i32, area_height: i32, width: i32, height: i32) -> (i32, i32) {
    let gap = (area_height as f64 * LAUNCH_BOTTOM_GAP as f64).round() as i32;
    ((area_width - width) / 2, area_height - height - gap)
}

/// An expand's growth: a damped spring (`spring_progress`).
pub fn spring_progress(seconds: f64) -> f64 {
    let damping: f64 = 0.72;
    let omega = std::f64::consts::TAU / 0.5;
    let damped = omega * (1.0 - damping * damping).sqrt();
    1.0 - (-damping * omega * seconds).exp()
        * ((damped * seconds).cos() + damping / (1.0 - damping * damping).sqrt() * (damped * seconds).sin())
}

/// A collapse's curve: `cubic-bezier(0.45, 0, 0.2, 1)` (`cubic_bezier`).
pub fn cubic_bezier(x1: f64, y1: f64, x2: f64, y2: f64, input: f64) -> f64 {
    let input = input.clamp(0.0, 1.0);
    let curve = |a: f64, b: f64, t: f64| {
        3.0 * (1.0 - t).powi(2) * t * a + 3.0 * (1.0 - t) * t.powi(2) * b + t.powi(3)
    };
    let (mut low, mut high) = (0.0, 1.0);
    for _ in 0..16 {
        let middle = (low + high) / 2.0;
        if curve(x1, x2, middle) < input {
            low = middle;
        } else {
            high = middle;
        }
    }
    curve(y1, y2, (low + high) / 2.0)
}

/// How far into a grow (1.2 s) or a shrink (0.34 s) `elapsed` is, 0 to 1.
pub fn transition_progress(expand: bool, elapsed_seconds: f64) -> f64 {
    if expand {
        spring_progress(elapsed_seconds.max(0.0))
    } else {
        cubic_bezier(0.45, 0.0, 0.2, 1.0, elapsed_seconds / 0.34)
    }
}

/// How long a grow or a shrink runs.
pub fn transition_seconds(expand: bool) -> f64 {
    if expand { 1.2 } else { 0.34 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_size_is_at_least_the_minimum_and_at_most_the_work_area() {
        assert_eq!(clamp_size(100.0, 100.0, 1900.0, 1000.0), EXPANDED_MIN);
        assert_eq!(clamp_size(5000.0, 5000.0, 1900.0, 1000.0), (1900.0, 1000.0));
        assert_eq!(clamp_size(700.0, 500.0, 1900.0, 1000.0), (700.0, 500.0));
        // A screen smaller than the minimum wins over the minimum.
        assert_eq!(clamp_size(640.0, 420.0, 480.0, 300.0), (480.0, 300.0));
        assert_eq!(clamp_size(f32::NAN, f32::INFINITY, 1900.0, 1000.0), EXPANDED_DEFAULT);
    }

    #[test]
    fn each_mode_has_its_own_size_and_the_expanded_one_remembers() {
        let area = (1920.0, 1040.0);
        assert_eq!(size_for(Mode::Petit, Some((700.0, 500.0)), area), COMPACT);
        assert_eq!(size_for(Mode::Hidden, None, area), COMPACT);
        assert_eq!(size_for(Mode::Welcome, Some((700.0, 500.0)), area), GREETING);
        assert_eq!(size_for(Mode::Home, Some((700.0, 500.0)), area), (700.0, 500.0));
        assert_eq!(size_for(Mode::Home, None, area), EXPANDED_DEFAULT);
        // A saved size from a larger display is clamped to this display's work area.
        assert_eq!(size_for(Mode::Home, Some((1900.0, 1300.0)), area), (1900.0, 1040.0));
    }

    #[test]
    fn the_presets_ask_for_their_own_sizes() {
        assert_eq!(Preset::from_key("large"), Some(Preset::Large));
        assert_eq!(Preset::from_key("nope"), None);
        assert_eq!(Preset::Large.target(None), (EXPANDED_LARGE, true));
        assert_eq!(Preset::Normal.target(None), (EXPANDED_DEFAULT, false));
        assert_eq!(Preset::Normal.target(Some((700.0, 500.0))).0, (700.0, 500.0));
        assert_eq!(Preset::Default.target(Some((700.0, 500.0))).0, EXPANDED_DEFAULT);
    }

    #[test]
    fn a_launch_is_centred_and_above_the_bottom_edge() {
        // 1920x1040 work area, 344x62 compact bar.
        let (x, y) = launch_offset(1920, 1040, 344, 62);
        assert_eq!(x, 788);
        assert_eq!(y, 1040 - 62 - 187);
        // A narrow work area keeps it inside.
        let (x, _) = launch_offset(300, 200, 344, 62);
        assert_eq!(x, -22, "the clamp that follows pulls it to the edge");
    }

    #[test]
    fn the_spring_grows_to_its_end_and_the_curve_settles_at_one() {
        assert!(spring_progress(0.0).abs() < 1e-9);
        assert!((spring_progress(5.0) - 1.0).abs() < 1e-6);
        // A damped spring overshoots a little on the way.
        assert!(spring_progress(0.5) > 0.9);
        assert!((cubic_bezier(0.45, 0.0, 0.2, 1.0, 0.0)).abs() < 1e-6);
        assert!((cubic_bezier(0.45, 0.0, 0.2, 1.0, 1.0) - 1.0).abs() < 1e-6);
        assert!((transition_progress(false, 0.34) - 1.0).abs() < 1e-6);
        assert_eq!(transition_seconds(true), 1.2);
        assert_eq!(transition_seconds(false), 0.34);
    }
}
