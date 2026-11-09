//! The companion's position and size geometry, in physical pixels.
//!
//! Ported from `src-tauri/src/lib.rs`: `clamp_physical_position`,
//! `companion_work_areas`, `clamp_companion_position`, `companion_launch_spot`,
//! `companion_saved_compact_spot`, `resized_rect`, `parse_resize_edge`, and the
//! per-display memory that `companionPositions.ts` tidies up (`at most 8
//! entries`, rounded to whole pixels, a display that is gone drops out).
//!
//! Everything here is pure. The Win32 calls that read the work areas and move
//! the window live in [`crate::companion::window`], so these functions can be
//! tested across two displays and across a display being unplugged.

#![allow(dead_code)]  // A ported surface: the helpers are complete and
// slice 6's settings page and slice 7 wire the rest.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// One display's work area, excluding the taskbar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkArea {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    /// `MONITORINFOEXW.szDevice`, for example `\\.\DISPLAY1`.
    pub name: String,
}

impl WorkArea {
    pub fn right(&self) -> i32 {
        self.x + self.width
    }

    pub fn bottom(&self) -> i32 {
        self.y + self.height
    }

    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x && x < self.right() && y >= self.y && y < self.bottom()
    }

    /// The work area in logical pixels, for the size clamps.
    pub fn logical(&self, scale: f32) -> (f32, f32) {
        let scale = if scale > 0.0 { scale } else { 1.0 };
        (self.width as f32 / scale, self.height as f32 / scale)
    }
}

/// A rectangle in physical pixels: `(x, y, width, height)`.
pub type Rect = (i32, i32, i32, i32);

/// Where the companion was left on one display, as `Point` in `lib.rs` is
/// stored: `y` is the bottom edge, so a taller window still hangs from the same
/// spot, and the anchor keeps it centred on the same place when the display's
/// resolution changes.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct StoredPoint {
    pub x: i32,
    pub y: i32,
    pub anchor_x: Option<f64>,
    pub bottom_gap: Option<f64>,
    pub monitor: Option<String>,
}

/// How many displays are remembered (`MAX_COMPANION_POSITIONS`).
pub const MAX_MONITOR_POSITIONS: usize = 8;
/// The longest display name worth keeping (`MAX_MONITOR_NAME`).
const MAX_MONITOR_NAME: usize = 200;

/// The remembered positions, per display name.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CompanionPositions {
    pub entries: BTreeMap<String, StoredPoint>,
    /// Display names, oldest first, so the cap drops the oldest one.
    pub order: Vec<String>,
}

impl CompanionPositions {
    pub fn get(&self, monitor: &str) -> Option<&StoredPoint> {
        self.entries.get(monitor)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Remember one display's spot, dropping the oldest entry once the cap is
    /// reached. A nameless or absurdly named display is not remembered.
    pub fn remember(&mut self, monitor: &str, point: StoredPoint) {
        if !valid_monitor(monitor) {
            return;
        }
        self.entries.insert(monitor.to_owned(), point);
        self.order.retain(|name| name != monitor);
        self.order.push(monitor.to_owned());
        while self.order.len() > MAX_MONITOR_POSITIONS {
            let oldest = self.order.remove(0);
            self.entries.remove(&oldest);
        }
    }

    pub fn forget(&mut self, monitor: &str) {
        if self.entries.remove(monitor).is_some() {
            self.order.retain(|name| name != monitor);
        }
    }
}

fn valid_monitor(name: &str) -> bool {
    !name.is_empty() && name.chars().count() <= MAX_MONITOR_NAME
}

/// One axis of the clamp: a window narrower than the area can sit anywhere
/// inside it; a window wider than the area is pinned to its origin.
pub fn clamp_axis(position: i32, size: i32, origin: i32, area_size: i32) -> i32 {
    let (position, size, origin, area_size) = (position as i64, size.max(0) as i64, origin as i64, area_size.max(0) as i64);
    if size > area_size {
        origin as i32
    } else {
        position.clamp(origin, origin + area_size - size) as i32
    }
}

/// `clamp_physical_position`.
pub fn clamp_position(x: i32, y: i32, width: i32, height: i32, area: &WorkArea) -> (i32, i32) {
    (
        clamp_axis(x, width, area.x, area.width),
        clamp_axis(y, height, area.y, area.height),
    )
}

/// The display whose work area holds the point, if any.
pub fn area_at(areas: &[WorkArea], x: i32, y: i32) -> Option<&WorkArea> {
    areas.iter().find(|area| area.contains(x, y))
}

/// The display a window mostly sits on, falling back to the display its centre
/// came from and then to the first one (`clamp_companion_position`).
pub fn nearest_area(areas: &[WorkArea], centre: (i32, i32), fallback_centre: (i32, i32)) -> Option<&WorkArea> {
    area_at(areas, centre.0, centre.1)
        .or_else(|| area_at(areas, fallback_centre.0, fallback_centre.1))
        .or_else(|| areas.first())
}

/// Clamp a rectangle into the display it mostly sits on.
pub fn clamp_into(areas: &[WorkArea], rect: Rect, fallback_centre: (i32, i32)) -> Rect {
    let (x, y, width, height) = rect;
    let centre = (x + width / 2, y + height / 2);
    let Some(area) = nearest_area(areas, centre, fallback_centre) else { return rect };
    let (x, y) = clamp_position(x, y, width, height, area);
    (x, y, width, height)
}

/// The display the companion last used, by name; a display that is gone is
/// skipped so a removed monitor cannot leave the window off-screen.
pub fn area_for_monitor<'a>(areas: &'a [WorkArea], monitor: Option<&str>) -> Option<&'a WorkArea> {
    let name = monitor?;
    areas.iter().find(|area| area.name == name)
}

/// `companion_launch_spot`: centred, a little above the bottom edge.
pub fn launch_spot(area: &WorkArea, width: i32, height: i32) -> (i32, i32) {
    let (dx, dy) = crate::companion::layout::launch_offset(area.width, area.height, width, height);
    let (x, y) = (area.x + dx, area.y + dy);
    clamp_position(x, y, width, height, area)
}

/// `companion_last_launch_spot`: keep the welcome island's bottom centre on the
/// spot the compact bar was left at.
pub fn last_launch_spot(compact: Rect, greeting_width: i32, greeting_height: i32, area: &WorkArea) -> (i32, i32) {
    let (x, y, width, height) = compact;
    let x = x + width / 2 - greeting_width / 2;
    let y = y + height - greeting_height;
    clamp_position(x, y, greeting_width, greeting_height, area)
}

/// `companion_saved_compact_spot`: the stored point as a compact-bar rectangle.
pub fn saved_spot(point: &StoredPoint, width: i32, height: i32, scale: f64, area: &WorkArea) -> (i32, i32) {
    let (x, y) = match (point.anchor_x, point.bottom_gap) {
        (Some(anchor_x), Some(bottom_gap)) => {
            let centre_x = area.x + (area.width as f64 * anchor_x.clamp(0.0, 1.0)).round() as i32;
            let bottom = area.bottom() - (bottom_gap * scale).round() as i32;
            (centre_x - width / 2, bottom - height)
        }
        _ => (point.x, point.y - height),
    };
    clamp_position(x, y, width, height, area)
}

/// The stored point for a window left at `rect` on `area`, at `scale`.
pub fn stored_point(rect: Rect, area: &WorkArea, scale: f64) -> StoredPoint {
    let (x, y, width, height) = rect;
    let centre_x = x + width / 2;
    let scale = if scale > 0.0 { scale } else { 1.0 };
    StoredPoint {
        x,
        y: y + height,
        anchor_x: Some((centre_x - area.x) as f64 / area.width.max(1) as f64),
        bottom_gap: Some((area.bottom() - y - height) as f64 / scale),
        monitor: Some(area.name.clone()),
    }
}

/// Which sides a resize drag moves: `n`, `s`, `e`, `w` or a corner such as `se`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResizeEdge {
    pub left: bool,
    pub right: bool,
    pub top: bool,
    pub bottom: bool,
}

pub fn parse_resize_edge(edge: &str) -> Option<ResizeEdge> {
    let flags = ResizeEdge { left: edge.contains('w'), right: edge.contains('e'), top: edge.contains('n'), bottom: edge.contains('s') };
    matches!(edge, "n" | "s" | "e" | "w" | "ne" | "nw" | "se" | "sw").then_some(flags)
}

/// The rectangle after dragging `edge` by `delta`. The opposite side stays put,
/// also when the size hits the minimum or the work area's size (`resized_rect`).
pub fn resized_rect(start: Rect, delta: (i32, i32), edge: ResizeEdge, min: (i32, i32), max: (i32, i32)) -> Rect {
    let (x, y, width, height) = start;
    let fit = |value: i32, low: i32, high: i32| value.max(low).min(high.max(1));
    let (mut new_x, mut new_width) = (x, width);
    if edge.right {
        new_width = fit(width + delta.0, min.0, max.0);
    }
    if edge.left {
        new_width = fit(width - delta.0, min.0, max.0);
        new_x = x + width - new_width;
    }
    let (mut new_y, mut new_height) = (y, height);
    if edge.bottom {
        new_height = fit(height + delta.1, min.1, max.1);
    }
    if edge.top {
        new_height = fit(height - delta.1, min.1, max.1);
        new_y = y + height - new_height;
    }
    (new_x, new_y, new_width, new_height)
}

/// A dragged window: the same size, moved by the cursor's delta.
pub fn moved_rect(start: Rect, delta: (i32, i32)) -> Rect {
    (start.0 + delta.0, start.1 + delta.1, start.2, start.3)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn primary() -> WorkArea {
        WorkArea { x: 0, y: 0, width: 1920, height: 1040, name: r"\\.\DISPLAY1".into() }
    }

    fn secondary() -> WorkArea {
        WorkArea { x: 1920, y: 0, width: 1280, height: 680, name: r"\\.\DISPLAY2".into() }
    }

    #[test]
    fn inside_is_unchanged_and_every_edge_clamps() {
        assert_eq!(clamp_position(120, 130, 200, 100, &primary()), (120, 130));
        assert_eq!(clamp_position(450, 80, 100, 100, &WorkArea { x: 0, y: 0, width: 500, height: 400, ..primary() }), (400, 80));
        assert_eq!(clamp_position(-50, 80, 100, 100, &WorkArea { x: 0, y: 0, width: 500, height: 400, ..primary() }), (0, 80));
        assert_eq!(clamp_position(80, 350, 100, 100, &WorkArea { x: 0, y: 0, width: 500, height: 400, ..primary() }), (80, 300));
    }

    #[test]
    fn an_oversized_window_pins_to_the_work_area_origin() {
        let area = WorkArea { x: 10, y: 20, width: 500, height: 400, ..primary() };
        assert_eq!(clamp_position(80, 80, 600, 450, &area), (10, 20));
    }

    #[test]
    fn a_negative_origin_display_clamps_its_own_way() {
        let area = WorkArea { x: -1080, y: 0, width: 1080, height: 800, ..primary() };
        assert_eq!(clamp_position(-20, 60, 100, 100, &area), (-100, 60));
    }

    #[test]
    fn two_displays_drag_the_companion_between_them() {
        let areas = [primary(), secondary()];
        let rect = clamped_into(&areas, (2600, 100, 344, 62), (0, 0));
        assert_eq!(rect, (2600, 100, 344, 62), "on the second display, untouched");
        // Past the second display's right edge it is pulled back, and the
        // fallback centre (where the window already was) keeps it there.
        assert_eq!(clamped_into(&areas, (3100, 100, 344, 62), (2600, 100)), (1920 + 1280 - 344, 100, 344, 62));
        // A bar straddling the seam is mostly on the second display (its centre
        // is at 2072), so that display's work area decides: it is pushed clear
        // of the first one instead of being cut in half by the seam.
        assert_eq!(clamped_into(&areas, (1900, 100, 344, 62), (1900, 100)), (1920, 100, 344, 62));
        // Its centre still on the first display keeps it there.
        assert_eq!(clamped_into(&areas, (1540, 100, 344, 62), (1540, 100)), (1540, 100, 344, 62));
    }

    fn clamped_into(areas: &[WorkArea], rect: Rect, fallback: (i32, i32)) -> Rect {
        clamp_into(areas, rect, fallback)
    }

    #[test]
    fn a_removed_display_is_skipped_and_the_window_lands_on_the_primary() {
        let areas = [primary()];
        let mut positions = CompanionPositions::default();
        positions.remember(&primary().name, stored_point((900, 500, 344, 62), &primary(), 1.0));
        positions.remember(&secondary().name, stored_point((2100, 300, 344, 62), &secondary(), 1.0));
        // The second display is unplugged: its entry no longer names a display,
        // so the launch asks for the primary and clamps a stale entry into it.
        assert!(area_for_monitor(&areas, Some(&secondary().name)).is_none());
        let stale = positions.get(&secondary().name).expect("kept for a return");
        // The anchor is relative to the display, so the spot lands at the same
        // proportion of the primary: 27.5% across, 318 px above its bottom.
        let (x, y) = saved_spot(stale, 344, 62, 1.0, &primary());
        assert_eq!((x, y), (356, 660));
    }

    #[test]
    fn at_most_eight_displays_are_remembered_oldest_first() {
        let mut positions = CompanionPositions::default();
        for index in 0..10 {
            let name = format!("display{index}");
            positions.remember(&name, stored_point((index * 10, index * 10, 344, 62), &primary(), 1.0));
        }
        assert_eq!(positions.len(), MAX_MONITOR_POSITIONS);
        assert!(positions.get("display0").is_none() && positions.get("display1").is_none());
        assert!(positions.get("display9").is_some());
        // Remembering one again moves it to the front of the queue, so it is the
        // last to be dropped.
        positions.remember("display9", stored_point((1, 2, 344, 62), &primary(), 1.0));
        assert_eq!(positions.order.last().map(String::as_str), Some("display9"));
        positions.forget("display9");
        assert!(positions.get("display9").is_none());
    }

    #[test]
    fn a_nameless_display_is_not_remembered() {
        let mut positions = CompanionPositions::default();
        positions.remember("", stored_point((10, 10, 344, 62), &primary(), 1.0));
        positions.remember(&"x".repeat(201), stored_point((10, 10, 344, 62), &primary(), 1.0));
        assert!(positions.is_empty());
    }

    #[test]
    fn the_launch_is_centred_and_above_the_bottom_edge() {
        // 1920x1040 work area, 344x62 compact bar.
        assert_eq!(launch_spot(&primary(), 344, 62), (788, 1040 - 62 - 187));
                // A work area narrower than the bar pins it to the edge; one too short
        // pins its top, but its height still fits, so only the x is pinned.
        let narrow = WorkArea { x: 0, y: 0, width: 300, height: 200, ..primary() };
        assert_eq!(launch_spot(&narrow, 344, 62), (0, 102));
    }

    #[test]
    fn the_welcome_keeps_the_compact_bars_bottom_centre() {
        let (x, y) = last_launch_spot((900, 500, 344, 62), 640, 150, &primary());
        assert_eq!(x + 320, 900 + 172);
        assert_eq!(y + 150, 500 + 62);
        // The whole greeting is clamped into the display.
        assert_eq!(last_launch_spot((1700, 900, 344, 62), 640, 150, &primary()), (1280, 812));
    }

    #[test]
    fn a_stored_anchor_survives_a_resolution_change() {
        let point = StoredPoint { anchor_x: Some(0.5), bottom_gap: Some(24.0), monitor: Some(primary().name), ..Default::default() };
        // Centred on a 1280-wide display, 24 px above its bottom edge.
        let wide = WorkArea { x: 0, y: 0, width: 1280, height: 720, name: primary().name };
        let (x, y) = saved_spot(&point, 344, 62, 1.0, &wide);
        assert_eq!((x, y), (640 - 172, 720 - 24 - 62));
    }

    #[test]
    fn the_resize_edges_parse_and_anything_else_is_refused() {
        assert_eq!(parse_resize_edge("e"), Some(ResizeEdge { left: false, right: true, top: false, bottom: false }));
        assert_eq!(parse_resize_edge("nw"), Some(ResizeEdge { left: true, right: false, top: true, bottom: false }));
        for bad in ["", "x", "ee", "ns", "up"] {
            assert!(parse_resize_edge(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn dragging_the_right_or_bottom_edge_keeps_the_top_left_corner() {
        let start = (100, 200, 800, 600);
        assert_eq!(resized_rect(start, (50, 30), parse_resize_edge("se").unwrap(), (500, 300), (1900, 1000)), (100, 200, 850, 630));
        assert_eq!(resized_rect(start, (-100, 0), parse_resize_edge("e").unwrap(), (500, 300), (1900, 1000)), (100, 200, 700, 600));
    }

    #[test]
    fn dragging_the_left_or_top_edge_keeps_the_far_side_still() {
        let start = (100, 200, 800, 600);
        assert_eq!(resized_rect(start, (-40, 0), parse_resize_edge("w").unwrap(), (500, 300), (1900, 1000)), (60, 200, 840, 600));
        assert_eq!(resized_rect(start, (0, -50), parse_resize_edge("n").unwrap(), (500, 300), (1900, 1000)), (100, 150, 800, 650));
        let corner = resized_rect(start, (60, 80), parse_resize_edge("nw").unwrap(), (500, 300), (1900, 1000));
        assert_eq!((corner.0 + corner.2, corner.1 + corner.3), (900, 800));
    }

    #[test]
    fn the_minimum_and_the_work_area_stop_a_drag_without_moving_the_far_side() {
        let start = (100, 200, 800, 600);
        let (x, y, width, height) = resized_rect(start, (900, 900), parse_resize_edge("nw").unwrap(), (500, 300), (1900, 1000));
        assert_eq!((width, height), (500, 300));
        assert_eq!((x + width, y + height), (900, 800), "the bottom right corner stays put");
        let (_, _, width, height) = resized_rect(start, (5000, 5000), parse_resize_edge("se").unwrap(), (500, 300), (1900, 1000));
        assert_eq!((width, height), (1900, 1000));
    }

    #[test]
    fn a_drag_moves_without_resizing() {
        assert_eq!(moved_rect((100, 200, 344, 62), (40, -30)), (140, 170, 344, 62));
    }

    #[test]
    fn the_positions_survive_the_settings_file() {
        let mut positions = CompanionPositions::default();
        positions.remember(&primary().name, stored_point((900, 500, 344, 62), &primary(), 1.0));
        let text = serde_json::to_string(&positions).unwrap();
        let back: CompanionPositions = serde_json::from_str(&text).unwrap();
        assert_eq!(back, positions);
        // An anchor_x of 900/1920 and the point's own bottom.
        let point = back.get(&primary().name).unwrap();
        assert_eq!(point.y, 562);
        assert!((point.anchor_x.unwrap() - 1072.0 / 1920.0).abs() < 1e-9);
    }
}
