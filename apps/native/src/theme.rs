//! Haze tokens, copied from the app's own stylesheets: `ui/themes/haze.css`
//! (the dark block at lines 5-36 and the light block at 37-64), `ui/theme.css`
//! (the type scale and the radii) and `ui/density.css` (`--dn-*`).
//!
//! Every colour and size in a view comes from here; no view holds a raw colour.
//! Values are the CSS values, alpha included, so a later side-by-side check
//! compares like with like.

use std::sync::atomic::{AtomicBool, Ordering};

use gpui::{rgb, rgba, Rgba};
use serde::{Deserialize, Serialize};

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

/// `--font-ui`: Inter, bundled (`assets/fonts`), as the app ships it.
pub const UI_FONT: &str = "Inter";

/// Inter's static weights, converted from the app's own `@fontsource-variable/inter`
/// (latin, OFL; `assets/fonts/LICENSE-Inter.txt`).
pub fn bundled_fonts() -> Vec<std::borrow::Cow<'static, [u8]>> {
    vec![
        std::borrow::Cow::Borrowed(include_bytes!("../assets/fonts/Inter-Regular.ttf").as_slice()),
        std::borrow::Cow::Borrowed(include_bytes!("../assets/fonts/Inter-Medium.ttf").as_slice()),
        std::borrow::Cow::Borrowed(include_bytes!("../assets/fonts/Inter-SemiBold.ttf").as_slice()),
        std::borrow::Cow::Borrowed(include_bytes!("../assets/fonts/Inter-Bold.ttf").as_slice()),
    ]
}
pub const MONO_FONT: &str = "Cascadia Mono";

/// The version this build reports on the About page.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone, Copy, Debug)]
#[allow(dead_code)]
pub struct Colors {
    pub dark: bool,
    /// `--haze-base` `#1a1b1d` (dark) / `#f1efeb` (light), opaque.
    pub base: Rgba,
    /// The same base at 80%, the documented fallback when Windows draws no blur.
    pub backdrop_fallback: Rgba,
    /// `--surface-sidebar`.
    pub sidebar: Rgba,
    /// `--surface-panel`.
    pub panel: Rgba,
    /// `--surface-raised`.
    pub raised: Rgba,
    /// `--surface-overlay`.
    pub overlay: Rgba,
    /// `--surface-input`.
    pub input: Rgba,
    /// `--surface-hover`.
    pub hover: Rgba,
    /// `--surface-selected`.
    pub selected: Rgba,
    /// `--surface-selected-strong`.
    pub selected_strong: Rgba,
    /// `--surface-avatar`.
    pub avatar: Rgba,
    /// `--bubble-user`.
    pub bubble_user: Rgba,
    /// `--text-1`.
    pub text_1: Rgba,
    /// `--text-2`.
    pub text_2: Rgba,
    /// `--text-3`.
    pub text_3: Rgba,
    /// `--text-inverse`: the label on a filled button.
    pub text_inverse: Rgba,
    /// `--line`.
    pub line: Rgba,
    /// `--line-strong`.
    pub line_strong: Rgba,
    /// `--status-working`.
    pub status_working: Rgba,
    /// `--status-success`.
    pub status_success: Rgba,
    /// `--status-danger`.
    pub status_danger: Rgba,
    /// `--status-info`.
    pub status_info: Rgba,
    /// `--accent`: the owner's accent, or text-1 while there is none.
    pub accent: Rgba,
    /// `--scrim`: behind a dialog.
    pub scrim: Rgba,
    /// A field's hint: `--text-3` at full strength, so a placeholder reads as a
    /// hint on both backgrounds instead of vanishing into the light one.
    pub placeholder: Rgba,
}

/// The colours the window is drawing with right now, for the two places that
/// cannot be handed them: a text field's own paint and its placeholder.
static CURRENT_IS_LIGHT: AtomicBool = AtomicBool::new(false);

/// The root view says which theme it is drawing, once per frame.
pub fn set_current_light(light: bool) {
    CURRENT_IS_LIGHT.store(light, Ordering::Relaxed);
}

/// The colours of the last frame.
pub fn current_colors() -> Colors {
    if CURRENT_IS_LIGHT.load(Ordering::Relaxed) {
        haze_light()
    } else {
        haze_dark()
    }
}

/// Haze, dark (`haze.css:5-36`).
pub fn haze_dark() -> Colors {
    Colors {
        dark: true,
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
        text_inverse: rgb(0x1a1b1d),
        line: rgba(0xffffff14),
        line_strong: rgba(0xffffff24),
        status_working: rgb(0xffae42),
        status_success: rgb(0x3ddc97),
        status_danger: rgb(0xff6b6b),
        status_info: rgb(0x6ea8ff),
        accent: rgb(0xf3f3f2),
        scrim: rgba(0x00000052),
        placeholder: rgb(0x8f8f8d),
    }
}

/// Haze, light (`haze.css:37-64`). The status colours are darker here so they
/// stay readable on a pale backdrop, exactly as the stylesheet sets them.
pub fn haze_light() -> Colors {
    Colors {
        dark: false,
        base: rgb(0xf1efeb),
        backdrop_fallback: rgba(0xf1efebee),
        sidebar: rgba(0xffffff14),
        panel: rgba(0xfaf9f7f7),
        raised: rgba(0xffffff8c),
        overlay: rgba(0xfcfbfaf7),
        input: rgba(0xffffff99),
        hover: rgba(0x1414140b),
        selected: rgba(0x14141412),
        selected_strong: rgba(0x1414141a),
        avatar: rgba(0x1414140f),
        bubble_user: rgba(0x14141412),
        text_1: rgb(0x1b1b1a),
        text_2: rgba(0x1b1b1aad),
        text_3: rgba(0x1b1b1a80),
        text_inverse: rgb(0xffffff),
        line: rgba(0x14141414),
        line_strong: rgba(0x14141424),
        status_working: rgb(0xa86a00),
        status_success: rgb(0x14875f),
        status_danger: rgb(0xd23a52),
        status_info: rgb(0x2f6bd6),
        accent: rgb(0x1b1b1a),
        scrim: rgba(0x00000052),
        placeholder: rgb(0x6d6d6b),
    }
}

/// `theme` from Settings: follow Windows, or pin light or dark.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeChoice {
    #[default]
    System,
    Light,
    Dark,
}

impl ThemeChoice {
    pub const ALL: [ThemeChoice; 3] = [ThemeChoice::System, ThemeChoice::Light, ThemeChoice::Dark];

    pub fn label(self) -> &'static str {
        match self {
            ThemeChoice::System => "System",
            ThemeChoice::Light => "Light",
            ThemeChoice::Dark => "Dark",
        }
    }

    /// The stored value, which is also the Settings file's word. `from_key` is
    /// its inverse, for a settings file written by hand.
    pub fn key(self) -> &'static str {
        match self {
            ThemeChoice::System => "system",
            ThemeChoice::Light => "light",
            ThemeChoice::Dark => "dark",
        }
    }

    #[allow(dead_code)]
    pub fn from_key(value: &str) -> Self {
        match value.trim().to_lowercase().as_str() {
            "light" => ThemeChoice::Light,
            "dark" => ThemeChoice::Dark,
            _ => ThemeChoice::System,
        }
    }
}

/// Whether the window is light: an explicit choice wins, System follows Windows
/// (GPUI's `window.appearance()`).
pub fn is_light(choice: ThemeChoice, system_is_light: bool) -> bool {
    match choice {
        ThemeChoice::Light => true,
        ThemeChoice::Dark => false,
        ThemeChoice::System => system_is_light,
    }
}

/// The colours for a choice and the current Windows appearance.
pub fn colors(choice: ThemeChoice, system_is_light: bool) -> Colors {
    if is_light(choice, system_is_light) {
        haze_light()
    } else {
        haze_dark()
    }
}

/// `--dn-*` spacing, from `ui/density.css:8-19`. Heights, gaps and padding only:
/// no colour, no type.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DensityTokens {
    pub row_pad_y: f32,
    pub row_gap: f32,
    pub group_gap: f32,
    pub nest_gap: f32,
    pub card_pad_y: f32,
    pub team_row_h: f32,
    pub person_h: f32,
    pub card_h: f32,
}

/// The owner's density choice.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Density {
    Compact,
    #[default]
    Regular,
    Roomy,
}

impl Density {
    pub const ALL: [Density; 3] = [Density::Compact, Density::Regular, Density::Roomy];

    pub fn label(self) -> &'static str {
        match self {
            Density::Compact => "Compact",
            Density::Regular => "Regular",
            Density::Roomy => "Roomy",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Density::Compact => "compact",
            Density::Regular => "regular",
            Density::Roomy => "roomy",
        }
    }

    #[allow(dead_code)]
    pub fn from_key(value: &str) -> Self {
        match value.trim().to_lowercase().as_str() {
            "compact" => Density::Compact,
            "roomy" => Density::Roomy,
            _ => Density::Regular,
        }
    }

    /// The `:root` block, the `compact` block or the `roomy` block.
    pub fn tokens(self) -> DensityTokens {
        match self {
            Density::Regular => DensityTokens {
                row_pad_y: 6.0,
                row_gap: 3.0,
                group_gap: 10.0,
                nest_gap: 4.0,
                card_pad_y: 6.0,
                team_row_h: 36.0,
                person_h: 46.0,
                card_h: 40.0,
            },
            Density::Compact => DensityTokens {
                row_pad_y: 3.0,
                row_gap: 1.0,
                group_gap: 6.0,
                nest_gap: 2.0,
                card_pad_y: 4.0,
                team_row_h: 30.0,
                person_h: 38.0,
                card_h: 34.0,
            },
            Density::Roomy => DensityTokens {
                row_pad_y: 9.0,
                row_gap: 6.0,
                group_gap: 16.0,
                nest_gap: 6.0,
                card_pad_y: 9.0,
                team_row_h: 44.0,
                person_h: 54.0,
                card_h: 48.0,
            },
        }
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

    #[test]
    fn the_light_tokens_are_light_and_keep_their_own_status_colours() {
        let light = haze_light();
        assert!(!light.dark);
        // #f1efeb: a pale backdrop, not the dark one.
        assert!(light.base.r > 0.85 && light.base.g > 0.85 && light.base.b > 0.85);
        // --text-1 #1b1b1a on light.
        assert!(light.text_1.r < 0.2);
        // The status colours are the light block's, not the dark block's.
        assert!((light.status_working.r - 0xa8 as f32 / 255.0).abs() < 0.01);
        assert!((haze_dark().status_working.r - 0xff as f32 / 255.0).abs() < 0.01);
        // #fff on a filled button in light, #1a1b1d in dark.
        assert!(light.text_inverse.r > 0.99);
        assert!(haze_dark().text_inverse.r < 0.2);
    }

    #[test]
    fn the_theme_choice_system_follows_windows_and_an_explicit_choice_wins() {
        assert!(is_light(ThemeChoice::System, true));
        assert!(!is_light(ThemeChoice::System, false));
        assert!(is_light(ThemeChoice::Light, false), "an explicit choice wins over Windows");
        assert!(!is_light(ThemeChoice::Dark, true));
        assert_eq!(ThemeChoice::from_key("Light"), ThemeChoice::Light);
        assert_eq!(ThemeChoice::from_key("nonsense"), ThemeChoice::System);
        assert!(colors(ThemeChoice::Light, false).base.r > 0.85);
        assert!(colors(ThemeChoice::Dark, true).base.r < 0.2);
    }

    #[test]
    fn density_follows_the_stylesheet_tokens() {
        let regular = Density::Regular.tokens();
        assert_eq!((regular.person_h, regular.card_h, regular.row_gap), (46.0, 40.0, 3.0));
        let compact = Density::Compact.tokens();
        assert_eq!((compact.person_h, compact.card_h, compact.row_gap), (38.0, 34.0, 1.0));
        let roomy = Density::Roomy.tokens();
        assert_eq!((roomy.person_h, roomy.card_h, roomy.row_gap), (54.0, 48.0, 6.0));
        // Compact is tighter and roomy is looser than regular on every measure.
        for (tight, loose) in [
            (compact.row_gap, roomy.row_gap),
            (compact.group_gap, roomy.group_gap),
            (compact.person_h, roomy.person_h),
            (compact.card_h, roomy.card_h),
            (compact.team_row_h, roomy.team_row_h),
        ] {
            assert!(tight < regular.row_gap.max(loose) || tight < loose);
            assert!(tight < loose);
        }
        assert_eq!(Density::from_key("Compact"), Density::Compact);
        assert_eq!(Density::from_key("unknown"), Density::Regular);
    }
}
