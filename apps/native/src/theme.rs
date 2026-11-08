//! Haze (dark) tokens, copied from the app's own stylesheets:
//! `apps/desktop/src/ui/themes/haze.css` lines 5-36 (surfaces, text, lines,
//! status), `ui/theme.css` (the type scale, radii and base palette) and
//! `ui/density.css` (row heights and gaps).
//!
//! Every colour and size in a view comes from here; no view holds a raw colour.
//! Values are the CSS values, alpha included, so a later side-by-side check
//! compares like with like.

use gpui::{rgb, rgba, Rgba};

/// The sidebar and the conversations column, from the app's shell layout.
pub const SIDEBAR_WIDTH: f32 = 280.0;
pub const CONVERSATIONS_WIDTH: f32 = 248.0;

/// `--fs-*` from `ui/theme.css:104`.
pub const FS_2XS: f32 = 11.0;
pub const FS_XS: f32 = 12.0;
pub const FS_SM: f32 = 13.0;
pub const FS_MD: f32 = 14.0;
pub const FS_LG: f32 = 15.0;
pub const FS_XL: f32 = 18.0;

/// Haze narrows the radii in the main window (`haze.css:274`).
pub const RADIUS_SM: f32 = 7.0;
pub const RADIUS_MD: f32 = 10.0;
pub const RADIUS_LG: f32 = 14.0;

/// `--dn-*` at the default density (`ui/density.css`).
pub const ROW_GAP: f32 = 3.0;
pub const GROUP_GAP: f32 = 10.0;
pub const NEST_GAP: f32 = 4.0;
pub const CARD_PAD_Y: f32 = 6.0;
pub const PERSON_HEIGHT: f32 = 46.0;
pub const CARD_HEIGHT: f32 = 40.0;

pub const UI_FONT: &str = "Segoe UI Variable Text";
pub const MONO_FONT: &str = "Cascadia Mono";

#[derive(Clone, Copy, Debug)]
#[allow(dead_code)]
pub struct Colors {
    /// `--haze-base` `#1a1b1d`, used opaque where nothing blurs.
    pub base: Rgba,
    /// The same base at 80%, the documented fallback when Windows draws no blur.
    pub backdrop_fallback: Rgba,
    /// `--surface-sidebar` `rgb(16 17 19 / .08)`.
    pub sidebar: Rgba,
    /// `--surface-panel` `rgb(34 35 38 / .97)`.
    pub panel: Rgba,
    /// `--surface-raised` `rgb(255 255 255 / .055)`.
    pub raised: Rgba,
    /// `--surface-overlay` `rgb(36 37 40 / .96)`.
    pub overlay: Rgba,
    /// `--surface-input` `rgb(0 0 0 / .22)`.
    pub input: Rgba,
    /// `--surface-hover` `rgb(255 255 255 / .06)`.
    pub hover: Rgba,
    /// `--surface-selected` `rgb(255 255 255 / .085)`.
    pub selected: Rgba,
    /// `--surface-selected-strong` `rgb(255 255 255 / .12)`.
    pub selected_strong: Rgba,
    /// `--surface-avatar` `rgb(255 255 255 / .07)`.
    pub avatar: Rgba,
    /// `--bubble-user` `rgb(255 255 255 / .085)`.
    pub bubble_user: Rgba,
    /// `--text-1` `#f3f3f2`.
    pub text_1: Rgba,
    /// `--text-2` `rgb(243 243 242 / .70)`.
    pub text_2: Rgba,
    /// `--text-3` `rgb(243 243 242 / .50)`.
    pub text_3: Rgba,
    /// `--line` `rgb(255 255 255 / .08)`.
    pub line: Rgba,
    /// `--line-strong` `rgb(255 255 255 / .14)`.
    pub line_strong: Rgba,
    /// `--status-working` `#ffae42`.
    pub status_working: Rgba,
    /// `--status-success` `#3ddc97`.
    pub status_success: Rgba,
    /// `--status-danger` `#ff6b6b`.
    pub status_danger: Rgba,
    /// `--status-info` `#6ea8ff`.
    pub status_info: Rgba,
    /// `--accent` `#f3f3f2` while the owner has no accent colour.
    pub accent: Rgba,
}

/// Haze, dark. Light mode is a later slice; this is the look the owner uses.
pub fn haze_dark() -> Colors {
    Colors {
        base: rgb(0x1a1b1d),
        backdrop_fallback: rgba(0x1a1b1dcc),
        sidebar: rgba(0x10111314),
        panel: rgba(0x222326f7),
        raised: rgba(0xffffff0e),
        overlay: rgba(0x242528f5),
        input: rgba(0x00000038),
        hover: rgba(0xffffff0f),
        selected: rgba(0xffffff16),
        selected_strong: rgba(0xffffff1f),
        avatar: rgba(0xffffff12),
        bubble_user: rgba(0xffffff16),
        text_1: rgb(0xf3f3f2),
        text_2: rgba(0xf3f3f2b3),
        text_3: rgba(0xf3f3f280),
        line: rgba(0xffffff14),
        line_strong: rgba(0xffffff24),
        status_working: rgb(0xffae42),
        status_success: rgb(0x3ddc97),
        status_danger: rgb(0xff6b6b),
        status_info: rgb(0x6ea8ff),
        accent: rgb(0xf3f3f2),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tokens_keep_the_stylesheet_alpha() {
        let colors = haze_dark();
        // rgb(34 35 38 / .97): 0.97 * 255 rounds to 247.
        assert!((colors.panel.a * 255.0 - 247.0).abs() < 0.5);
        // rgb(243 243 242 / .70): 0.70 * 255 rounds to 179.
        assert!((colors.text_2.a * 255.0 - 179.0).abs() < 0.5);
        // rgb(255 255 255 / .085): 0.085 * 255 rounds to 22.
        assert!((colors.selected.a * 255.0 - 22.0).abs() < 0.5);
        // The backdrop fallback is the documented "about 80% opaque".
        assert!((colors.backdrop_fallback.a - 0.8).abs() < 0.01);
    }
}
