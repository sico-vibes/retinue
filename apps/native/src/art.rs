//! A bot's mark, ported from `apps/desktop/src/blob/haze/HazeMark.tsx`.
//!
//! `agent.look.icon` is `logo`, `logo-tint`, `logo-mono` or a `char-*` portrait
//! id (`ui/LookEditor`, `blob/haze/icons.ts`). A logo is the bot's coding agent's
//! mark: `logo` is the agent's own colours, `logo-tint` is the bot's colour,
//! `logo-mono` is the text colour. A bot whose agent has no logo shows its
//! initial, exactly as the React app does.
//!
//! The portraits are full-colour vector art, so `build.rs` rasterises them once
//! and every view shares the decoded image (`Arc`, loaded on first use, never
//! per frame).

use std::collections::BTreeMap;
use std::sync::{Arc, LazyLock};

use gpui::{
    div, img, prelude::*, px, AnyElement, App, Bounds, Element, GlobalElementId, Hsla, Image,
    ImageFormat, InspectorElementId, LayoutId, Pixels, Rgba, SharedString, Style,
    TransformationMatrix, Window,
};

use crate::theme::Colors;

/// The generated table: a `char-*` id or a provider logo file name to PNG bytes.
mod generated {
    include!(concat!(env!("OUT_DIR"), "/art_assets.rs"));
}

/// The coding agent a bot runs on, as far as the mark is concerned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    Claude,
    Codex,
    Opencode,
}

impl Family {
    /// `logoFamily` in `HazeMark.tsx`.
    pub fn of(provider: &str) -> Option<Family> {
        let key = provider.trim().to_lowercase();
        match key.as_str() {
            "claude" | "anthropic" => Some(Family::Claude),
            "codex" | "openai" => Some(Family::Codex),
            other if other.starts_with("opencode") => Some(Family::Opencode),
            _ => None,
        }
    }

    fn logo_file(self, light: bool) -> &'static str {
        match (self, light) {
            (Family::Claude, _) => "claude",
            (Family::Codex, true) => "codex-light",
            (Family::Codex, false) => "codex-dark",
            (Family::Opencode, true) => "opencode-light",
            (Family::Opencode, false) => "opencode-dark",
        }
    }

    /// The one-colour drawing, as the React app draws it with `currentColor`.
    fn tint_bytes(self) -> (&'static str, &'static [u8]) {
        match self {
            Family::Claude => ("tint-claude", include_bytes!("../assets/logos/tint-claude.svg")),
            Family::Codex => ("tint-codex", include_bytes!("../assets/logos/tint-codex.svg")),
            Family::Opencode => ("tint-opencode", include_bytes!("../assets/logos/tint-opencode.svg")),
        }
    }
}

/// The name the app gives a coding agent (`providerBrand` in `ui/providerBrand.tsx`).
pub fn provider_name(provider: &str) -> String {
    match provider.trim().to_lowercase().as_str() {
        "claude" | "anthropic" => "Claude Code".to_owned(),
        "codex" | "openai" => "Codex".to_owned(),
        "opencode" | "opencode-go" | "opencode-zen" => "OpenCode".to_owned(),
        "cursor" => "Cursor CLI".to_owned(),
        "grok" => "Grok CLI".to_owned(),
        "qwen" => "Qwen Code".to_owned(),
        "" => "Coding agent".to_owned(),
        _ => {
            let trimmed = provider.trim();
            let mut chars = trimmed.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => "Coding agent".to_owned(),
            }
        }
    }
}

/// What the bot shows (`icons.ts`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Icon {
    Logo,
    LogoTint,
    LogoMono,
    Character(String),
}

impl Icon {
    /// `resolveHazeIcon`: the stored choice, else the logo; a retired pixel icon
    /// becomes the logo in the bot's colour.
    pub fn resolve(stored: Option<&str>) -> Icon {
        let Some(value) = stored.map(str::trim).filter(|value| !value.is_empty()) else {
            return Icon::Logo;
        };
        if value.starts_with("px-") {
            return Icon::LogoTint;
        }
        match value {
            "logo" => Icon::Logo,
            "logo-tint" => Icon::LogoTint,
            "logo-mono" => Icon::LogoMono,
            other if generated::portrait(other).is_some() => Icon::Character(other.to_owned()),
            _ => Icon::Logo,
        }
    }

    pub fn is_character(&self) -> bool {
        matches!(self, Icon::Character(_))
    }
}

/// The images, decoded once and shared. `img()` wants an `Arc<Image>`; making one
/// per frame would decode 30 portraits again every 16 ms.
static IMAGES: LazyLock<BTreeMap<&'static str, Arc<Image>>> = LazyLock::new(|| {
    let mut images: BTreeMap<&'static str, Arc<Image>> = BTreeMap::new();
    for id in generated::portrait_ids() {
        if let Some(bytes) = generated::portrait(id) {
            images.insert(id, Arc::new(Image::from_bytes(ImageFormat::Png, bytes.to_vec())));
        }
    }
    for name in ["claude", "codex-dark", "codex-light", "opencode-dark", "opencode-light"] {
        if let Some(bytes) = generated::logo(name) {
            images.insert(name, Arc::new(Image::from_bytes(ImageFormat::Png, bytes.to_vec())));
        }
    }
    images
});

fn image(key: &str) -> Option<Arc<Image>> {
    IMAGES.get(key).cloned()
}

/// Every portrait the build embedded: the look picker draws them and the tests
/// check them.
pub fn portrait_ids() -> &'static [&'static str] {
    generated::portrait_ids()
}

/// The first letter or digit of the name (`initialOf` in `HazeMark.tsx`).
pub fn initial_of(name: &str) -> String {
    name.chars()
        .find(|ch| ch.is_alphanumeric())
        .map(|ch| ch.to_uppercase().to_string())
        .unwrap_or_else(|| "·".to_owned())
}

/// One bot's mark at `size` square pixels.
///
/// `provider` is the coding agent's provider (from `runtime.list`), `color` the
/// bot's own swatch colour and `light` the current theme (the agent logos come in
/// a dark and a light file).
pub struct Mark {
    pub size: f32,
    pub color: Rgba,
    pub icon: Icon,
    pub provider: Option<Family>,
    pub label: String,
    pub light: bool,
    pub colors: Colors,
}

impl Mark {
    pub fn new(size: f32, color: Rgba, icon: Icon, label: impl Into<String>, colors: Colors) -> Self {
        Self { size, color, icon, provider: None, label: label.into(), light: !colors.dark, colors }
    }

    /// A bot's mark from its stored look, its runtime's provider and its colour.
    pub fn for_agent(
        size: f32,
        color: Rgba,
        icon: Option<&str>,
        provider: Option<&str>,
        label: impl Into<String>,
        colors: Colors,
    ) -> Self {
        Self {
            size,
            color,
            icon: Icon::resolve(icon),
            provider: provider.and_then(Family::of),
            label: label.into(),
            light: !colors.dark,
            colors,
        }
    }

    pub fn render(self) -> AnyElement {
        let size = px(self.size);
        let characters = self.icon.is_character();
        // Haze never draws a portrait smaller than 26 px (`HazeMark.tsx`).
        let box_size = if characters { self.size.max(26.0) } else { self.size };
        let mut outer = div()
            .flex_none()
            .size(px(box_size))
            .flex()
            .items_center()
            .justify_center()
            .overflow_hidden();
        if !characters {
            outer = outer.rounded(px(box_size * 0.28));
        }

        if let Icon::Character(id) = &self.icon {
            if let Some(image) = image(id) {
                return outer.child(img(image).size(px(box_size))).into_any_element();
            }
        }
        match self.provider {
            Some(family) if self.icon == Icon::Logo => {
                let drawn = box_size * 0.8;
                match image(family.logo_file(self.light)) {
                    Some(image) => outer.child(img(image).size(px(drawn))).into_any_element(),
                    None => initial_mark(size, box_size, self.color, &self.label),
                }
            }
            Some(family) => {
                let drawn = box_size * 0.8;
                let color = if self.icon == Icon::LogoTint { self.color.into() } else { self.colors.text_1.into() };
                let (name, bytes) = family.tint_bytes();
                outer
                    .child(TintSvg {
                        name,
                        bytes,
                        color,
                        size: drawn,
                        style: gpui::StyleRefinement::default(),
                    })
                    .into_any_element()
            }
            None => initial_mark(size, box_size, self.color, &self.label),
        }
    }
}

/// The initial on the bot's own colour (`haze.css:10`: 28% radius, white text).
fn initial_mark(_size: Pixels, box_size: f32, color: Rgba, label: &str) -> AnyElement {
    let drawn = box_size * 0.8;
    div()
        .flex()
        .items_center()
        .justify_center()
        .size(px(drawn))
        .rounded(px(drawn * 0.28))
        .bg(color)
        .text_color(gpui::rgb(0xffffff))
        .text_size(px(drawn * 0.42))
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .child(SharedString::from(initial_of(label)))
        .into_any_element()
}

/// One SVG drawn in a single colour, from bytes in the binary.
///
/// `gpui::svg()` loads its file through the app's asset source, which this
/// window does not install, so the mask is painted directly.
pub struct TintSvg {
    name: &'static str,
    bytes: &'static [u8],
    color: Hsla,
    size: f32,
    style: gpui::StyleRefinement,
}

impl Element for TintSvg {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<gpui::ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = px(self.size).into();
        style.size.height = px(self.size).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Self::PrepaintState {
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let _ = window.paint_svg(
            bounds,
            SharedString::from(self.name.to_owned()),
            Some(self.bytes),
            TransformationMatrix::default(),
            self.color,
            cx,
        );
    }
}

impl IntoElement for TintSvg {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Styled for TintSvg {
    fn style(&mut self) -> &mut gpui::StyleRefinement {
        &mut self.style
    }
}

/// One Lucide icon, drawn in the text colour like `lucide-react` does.
///
/// The files are Lucide 0.468.0 (ISC, see `assets/icons/LICENSE.txt`), the same
/// version `lucide-react` is pinned to in `apps/desktop/package.json`; only the
/// icons this window uses are shipped.
#[derive(Clone, Copy)]
pub struct Glyph {
    pub name: &'static str,
    pub bytes: &'static [u8],
}

macro_rules! glyphs {
    ($($constant:ident => $file:literal),* $(,)?) => {
        impl Glyph {
            $(
                #[allow(non_upper_case_globals)]
                pub const $constant: Glyph = Glyph {
                    name: $file,
                    bytes: include_bytes!(concat!("../assets/icons/", $file, ".svg")),
                };
            )*
        }
    };
}

glyphs! {
    BELL => "bell",
    ALERT => "circle-alert",
    SEARCH => "search",
    PLUS => "plus",
    X => "x",
    SETTINGS => "settings",
    USER_PLUS => "user-plus",
    MESSAGES => "messages-square",
    CHEVRON => "chevron-right",
    CHEVRON_DOWN => "chevron-down",
    ARROW_LEFT => "arrow-left",
    ARROW_RIGHT => "arrow-right",
    ARROW_UP => "arrow-up",
    CHECK => "check",
    PIN => "pin",
    ARCHIVE => "archive",
    PENCIL => "pencil",
    MORE => "more",
    CLOCK => "clock",
    PAPERCLIP => "paperclip",
    COPY => "copy",
    ROTATE => "rotate-ccw",
    IMAGE => "image",
    FOLDER => "folder",
    SHIELD => "shield-alert",
}

/// Draw one icon at `size`, in `color`.
pub fn glyph(glyph: Glyph, size: f32, color: Rgba) -> AnyElement {
    TintSvg {
        name: glyph.name,
        bytes: glyph.bytes,
        color: color.into(),
        size,
        style: gpui::StyleRefinement::default(),
    }
    .into_any_element()
}

/// The bot's own swatch, from `ui/agentColor.ts` (the same list as the sidebar).
pub fn agent_color(value: &str) -> Rgba {
    let key = value.trim().to_lowercase();
    let hex = match key.as_str() {
        "coral" => 0xF38C6F,
        "orange" => 0xF0A05A,
        "amber" => 0xE8C15A,
        "lemon" => 0xE4D56A,
        "lime" => 0xA8D45C,
        "mint" => 0x89D6B3,
        "teal" => 0x5EBEB0,
        "cyan" => 0x6EC8E0,
        "sky" => 0x7EB6F0,
        "blue" => 0x82AAFF,
        "violet" => 0xBF9CFF,
        "pink" => 0xF0A0C4,
        other => {
            let custom = other.strip_prefix('#').unwrap_or(other);
            u32::from_str_radix(custom, 16).unwrap_or(0x89D6B3) & 0xFFFFFF
        }
    };
    gpui::rgb(hex)
}

/// A bot's provider for its mark, from the runtime table the window loaded.
#[cfg(test)]
pub fn provider_of(runtimes: &BTreeMap<String, String>, runtime_id: &str) -> Option<String> {
    runtimes.get(runtime_id).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stored_icon_resolves_the_way_the_app_resolves_it() {
        assert_eq!(Icon::resolve(None), Icon::Logo);
        assert_eq!(Icon::resolve(Some("")), Icon::Logo);
        assert_eq!(Icon::resolve(Some("logo")), Icon::Logo);
        assert_eq!(Icon::resolve(Some("logo-tint")), Icon::LogoTint);
        assert_eq!(Icon::resolve(Some("logo-mono")), Icon::LogoMono);
        // A retired pixel icon becomes the logo in the bot's colour.
        assert_eq!(Icon::resolve(Some("px-cat")), Icon::LogoTint);
        // An unknown value is the logo, not a crash.
        assert_eq!(Icon::resolve(Some("nonsense")), Icon::Logo);
        assert_eq!(Icon::resolve(Some("char-ada-lovelace")), Icon::Character("char-ada-lovelace".into()));
    }

    #[test]
    fn a_provider_names_its_family() {
        assert_eq!(Family::of("claude"), Some(Family::Claude));
        assert_eq!(Family::of("Anthropic"), Some(Family::Claude));
        assert_eq!(Family::of("codex"), Some(Family::Codex));
        assert_eq!(Family::of("openai"), Some(Family::Codex));
        assert_eq!(Family::of("opencode-go"), Some(Family::Opencode));
        assert_eq!(Family::of("opencode"), Some(Family::Opencode));
        assert_eq!(Family::of("cursor"), None, "no logo, so the bot shows its initial");
        assert_eq!(Family::of(""), None);
    }

    #[test]
    fn every_portrait_id_has_an_image_and_every_logo_file_exists() {
        let ids = portrait_ids();
        assert!(ids.len() >= 29, "29 portraits ship with the app, found {}", ids.len());
        for id in ids {
            assert!(id.starts_with("char-"), "{id} is a char-* id");
            assert!(image(id).is_some(), "{id} has no embedded image");
            assert_eq!(Icon::resolve(Some(id)), Icon::Character((*id).to_owned()));
        }
        for family in [Family::Claude, Family::Codex, Family::Opencode] {
            for light in [true, false] {
                let file = family.logo_file(light);
                assert!(image(file).is_some(), "the logo {file} is missing");
            }
        }
    }

    #[test]
    fn an_initial_is_the_first_letter_or_digit() {
        assert_eq!(initial_of("ada"), "A");
        assert_eq!(initial_of("  wilson"), "W");
        assert_eq!(initial_of("42"), "4");
        assert_eq!(initial_of("—"), "·");
    }

    #[test]
    fn the_swatches_match_the_apps_list() {
        assert_eq!(agent_color("violet"), gpui::rgb(0xBF9CFF));
        assert_eq!(agent_color("coral"), gpui::rgb(0xF38C6F));
        assert_eq!(agent_color("#123456"), gpui::rgb(0x123456));
        // An unknown word falls back to mint, as `agentColor.ts` does.
        assert_eq!(agent_color("chartreuse"), gpui::rgb(0x89D6B3));
    }

    #[test]
    fn a_provider_is_looked_up_by_its_runtime() {
        let mut runtimes = BTreeMap::new();
        runtimes.insert("rt-1".to_owned(), "claude".to_owned());
        assert_eq!(provider_of(&runtimes, "rt-1"), Some("claude".to_owned()));
        assert_eq!(provider_of(&runtimes, "rt-2"), None);
    }
}
