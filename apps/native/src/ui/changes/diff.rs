//! One file's diff: unified, or side by side. Ported from `ui/DiffBody.tsx`.
//!
//! A diff can be thousands of lines, so the unified view is GPUI's `list` with
//! one row per line: it draws only what is on screen and keeps its own scroll.
//! The side-by-side view is a plain column, capped, because pairing two files'
//! lines into rows is the point of it and a short diff is the common case.

use std::rc::Rc;

use gpui::{div, prelude::*, px, AnyElement, FontWeight, ListAlignment, ListState, SharedString};

use crate::state::changes::{parse_unified_diff, side_by_side_rows, DiffLine, LineKind, ParsedDiff, SideRow};
use crate::theme::{self, Colors};
use crate::ui::changes::DiffMode;

/// How many side-by-side rows are drawn before the view says it is cut short.
pub const SPLIT_ROW_LIMIT: usize = 1_200;

/// One row of the unified diff.
pub fn render_line(index: usize, lines: &[DiffLine], colors: Colors) -> AnyElement {
    let Some(line) = lines.get(index) else { return div().into_any_element() };
    let (background, text) = match line.kind {
        LineKind::Added => (gpui::Rgba::from(gpui::Hsla::from(colors.status_success).opacity(0.14)), colors.status_success),
        LineKind::Removed => (gpui::Rgba::from(gpui::Hsla::from(colors.status_danger).opacity(0.14)), colors.status_danger),
        LineKind::Hunk => (colors.selected, colors.status_info),
        LineKind::Meta => (gpui::rgba(0x00000000), colors.text_3),
        LineKind::Context => (gpui::rgba(0x00000000), colors.text_2),
    };
    let number = |value: Option<u32>| match value {
        Some(value) => value.to_string(),
        None => String::new(),
    };
    div()
        .w_full()
        .min_w(px(0.0))
        .flex()
        .items_start()
        .bg(background)
        .font_family(theme::MONO_FONT)
        .text_size(px(theme::FS_XS))
        .when(line.is_code(), |this| {
            this.child(
                div()
                    .w(px(44.0))
                    .flex_none()
                    .text_color(colors.text_3)
                    .child(SharedString::from(number(line.old_number))),
            )
            .child(
                div()
                    .w(px(44.0))
                    .flex_none()
                    .text_color(colors.text_3)
                    .child(SharedString::from(number(line.new_number))),
            )
        })
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .text_color(text)
                .when(line.kind == LineKind::Added || line.kind == LineKind::Removed, |this| this.font_weight(FontWeight::MEDIUM))
                .child(SharedString::from(line.text.clone())),
        )
        .into_any_element()
}

/// What the body draws for one file, or `None` when there is nothing to show.
pub enum DiffContent {
    Empty,
    Binary,
    Unified(Rc<Vec<DiffLine>>),
    Split(Rc<Vec<SideRow>>, bool),
}

/// Parse a diff and put it in the shape the chosen mode draws.
pub fn prepare(content: &str, mode: DiffMode) -> (ParsedDiff, DiffContent) {
    let parsed = parse_unified_diff(content);
    if parsed.empty {
        return (parsed, DiffContent::Empty);
    }
    if parsed.binary {
        return (parsed, DiffContent::Binary);
    }
    match mode {
        DiffMode::Unified => {
            let lines = Rc::new(parsed.lines.clone());
            (parsed, DiffContent::Unified(lines))
        }
        DiffMode::Split => {
            let rows = side_by_side_rows(&parsed.lines);
            let capped = rows.len() > SPLIT_ROW_LIMIT;
            let rows = Rc::new(rows.into_iter().take(SPLIT_ROW_LIMIT).collect::<Vec<_>>());
            (parsed, DiffContent::Split(rows, capped))
        }
    }
}

/// One side-by-side cell.
pub fn split_cell(line: Option<&DiffLine>, side: bool, colors: Colors) -> AnyElement {
    let (background, text) = match line.map(|line| line.kind) {
        Some(LineKind::Added) => (gpui::Rgba::from(gpui::Hsla::from(colors.status_success).opacity(0.14)), colors.status_success),
        Some(LineKind::Removed) => (gpui::Rgba::from(gpui::Hsla::from(colors.status_danger).opacity(0.14)), colors.status_danger),
        Some(LineKind::Context) => (gpui::rgba(0x00000000), colors.text_2),
        _ => (gpui::rgba(0x00000000), colors.text_3),
    };
    let number = line
        .and_then(|line| if side { line.new_number } else { line.old_number })
        .map(|value| value.to_string())
        .unwrap_or_default();
    div()
        .flex_1()
        .min_w(px(0.0))
        .flex()
        .items_start()
        .bg(background)
        .font_family(theme::MONO_FONT)
        .text_size(px(theme::FS_XS))
        .child(div().w(px(40.0)).flex_none().text_color(colors.text_3).child(SharedString::from(number)))
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .text_color(text)
                .child(SharedString::from(line.map(|line| line.code().to_owned()).unwrap_or_default())),
        )
        .into_any_element()
}

/// The list a diff body scrolls with: one row per diff line.
pub fn diff_list(row_count: usize) -> ListState {
    ListState::new(row_count, ListAlignment::Top, px(600.0))
}

/// A "no text here" sentence, in the diff body's own words.
pub fn empty_note(text: &str, colors: Colors) -> AnyElement {
    div().p(px(12.0)).text_size(px(theme::FS_XS)).text_color(colors.text_3).child(SharedString::from(text.to_owned())).into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIFF: &str = "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -1,2 +1,2 @@\n keep\n-old\n+new\n";

    #[test]
    fn the_unified_mode_prepares_one_row_per_line() {
        let (parsed, content) = prepare(DIFF, DiffMode::Unified);
        assert_eq!(parsed.additions, 1);
        match content {
            DiffContent::Unified(lines) => assert_eq!(lines.len(), parsed.lines.len()),
            _ => panic!("expected the unified rows"),
        }
    }

    #[test]
    fn the_split_mode_pairs_the_change_into_one_row() {
        let (_, content) = prepare(DIFF, DiffMode::Split);
        match content {
            DiffContent::Split(rows, capped) => {
                assert!(!capped);
                assert_eq!(rows.len(), 6, "four header rows, the context pair and the changed pair");
                assert!(matches!(rows[4], SideRow::Pair { .. }));
                assert!(matches!(rows[5], SideRow::Pair { .. }));
            }
            _ => panic!("expected split rows"),
        }
    }

    #[test]
    fn an_empty_or_binary_diff_says_so_instead_of_drawing_nothing() {
        assert!(matches!(prepare("", DiffMode::Unified).1, DiffContent::Empty));
        assert!(matches!(
            prepare("diff --git a/x b/x\nBinary files a/x and b/x differ\n", DiffMode::Unified).1,
            DiffContent::Binary
        ));
    }

    #[test]
    fn a_huge_split_diff_is_cut_short_and_says_so() {
        let mut content = String::from("@@ -1,2000 +1,2000 @@\n");
        for index in 0..1_400 {
            content.push_str(&format!("+line {index}\n"));
        }
        match prepare(&content, DiffMode::Split).1 {
            DiffContent::Split(rows, capped) => {
                assert!(capped);
                assert_eq!(rows.len(), SPLIT_ROW_LIMIT);
            }
            _ => panic!("expected split rows"),
        }
    }

    #[test]
    fn a_line_keeps_its_numbers_and_its_colour_side() {
        let parsed = parse_unified_diff(DIFF);
        let added = parsed.lines.iter().find(|line| line.kind == LineKind::Added).unwrap();
        assert_eq!(added.new_number, Some(2));
        assert_eq!(added.old_number, None);
        assert_eq!(added.code(), "new");
    }
}
