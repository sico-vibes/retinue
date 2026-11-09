//! The Haze misty backdrop, computed the way `ui/themes/haze.css` paints
//! `.haze-backdrop`: a neutral base under four broad, blended colour blobs, the
//! whole composition at 80% so the window's own Windows backdrop shows through.
//!
//! GPUI at the pinned revision draws no radial gradients, so the composition is
//! rendered once per theme into a small image (the blobs are soft enough that a
//! stretched 480×300 image is indistinguishable from the CSS) and drawn behind
//! every column.

use std::sync::{Arc, LazyLock};

use gpui::{Image, ImageFormat};

/// The image size; stretched to the window. Wide, like the window.
const WIDTH: u32 = 480;
const HEIGHT: u32 = 300;

/// `opacity: .8` on `.haze-backdrop`: about 20% of the desktop shows through.
const BACKDROP_OPACITY: f32 = 0.8;

#[derive(Clone, Copy)]
struct Rgb {
    r: f32,
    g: f32,
    b: f32,
}

const fn hex(value: u32) -> Rgb {
    Rgb {
        r: ((value >> 16) & 0xff) as f32 / 255.0,
        g: ((value >> 8) & 0xff) as f32 / 255.0,
        b: (value & 0xff) as f32 / 255.0,
    }
}

/// One `radial-gradient(<rx>vw <ry>vh at <cx>% <cy>%, colour stops…)` layer.
struct Blob {
    color: Rgb,
    /// Ellipse radii as fractions of the window's width and height.
    rx: f32,
    ry: f32,
    /// Centre as fractions of the window.
    cx: f32,
    cy: f32,
    /// (position 0..1, alpha) stops; transparent at 100%.
    stops: &'static [(f32, f32)],
}

const FOUR_STOPS: &[(f32, f32)] = &[(0.0, 0.46), (0.38, 0.30), (0.66, 0.12), (1.0, 0.0)];
const THREE_STOPS: &[(f32, f32)] = &[(0.0, 0.36), (0.45, 0.18), (1.0, 0.0)];

struct Palette {
    base: Rgb,
    d1: Rgb,
    d2: Rgb,
    d3: Rgb,
    d4: Rgb,
}

/// `haze.css:6-9` (dark) and `:39-41` (light).
const DARK: Palette = Palette { base: hex(0x1a1b1d), d1: hex(0x8794a3), d2: hex(0x8d939b), d3: hex(0x6b7884), d4: hex(0xb4b8bd) };
const LIGHT: Palette = Palette { base: hex(0xf1efeb), d1: hex(0xa9b4c0), d2: hex(0xc3c8ce), d3: hex(0xaab6c0), d4: hex(0xdcdee1) };

/// The layers bottom to top: CSS lists the top layer first, so this is the
/// reverse of `haze.css:91-94`.
fn blobs(palette: &Palette) -> [Blob; 4] {
    [
        Blob { color: palette.d4, rx: 0.80, ry: 0.75, cx: 0.54, cy: 0.32, stops: THREE_STOPS },
        Blob { color: palette.d3, rx: 1.25, ry: 0.90, cx: 0.38, cy: 1.14, stops: FOUR_STOPS },
        Blob { color: palette.d2, rx: 1.00, ry: 1.10, cx: 1.00, cy: 0.62, stops: FOUR_STOPS },
        Blob { color: palette.d1, rx: 1.10, ry: 1.05, cx: 0.10, cy: 0.06, stops: FOUR_STOPS },
    ]
}

/// The alpha of a gradient's stops at `t` (0 at the centre, 1 on the ellipse).
fn stop_alpha(stops: &[(f32, f32)], t: f32) -> f32 {
    if t <= stops[0].0 {
        return stops[0].1;
    }
    for pair in stops.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        if t <= b.0 {
            let k = (t - a.0) / (b.0 - a.0);
            return a.1 + (b.1 - a.1) * k;
        }
    }
    0.0
}

/// One pixel of the composition, as straight (not premultiplied) RGBA 0..1.
fn pixel(palette: &Palette, layers: &[Blob; 4], x: f32, y: f32) -> [f32; 4] {
    let mut color = palette.base;
    for blob in layers {
        let dx = (x - blob.cx) / blob.rx;
        let dy = (y - blob.cy) / blob.ry;
        let alpha = stop_alpha(blob.stops, (dx * dx + dy * dy).sqrt());
        color.r += (blob.color.r - color.r) * alpha;
        color.g += (blob.color.g - color.g) * alpha;
        color.b += (blob.color.b - color.b) * alpha;
    }
    [color.r, color.g, color.b, BACKDROP_OPACITY]
}

/// The composition as PNG bytes.
fn render(palette: &Palette) -> Vec<u8> {
    let layers = blobs(palette);
    let mut pixels = image::RgbaImage::new(WIDTH, HEIGHT);
    for (px, py, out) in pixels.enumerate_pixels_mut() {
        let x = (px as f32 + 0.5) / WIDTH as f32;
        let y = (py as f32 + 0.5) / HEIGHT as f32;
        let [r, g, b, a] = pixel(palette, &layers, x, y);
        // A touch of ordered dither keeps the soft ramps free of banding once
        // the image is stretched to the window.
        let dither = (((px * 7 + py * 13) % 8) as f32 / 8.0 - 0.5) / 255.0;
        let channel = |value: f32| ((value + dither).clamp(0.0, 1.0) * 255.0).round() as u8;
        *out = image::Rgba([channel(r), channel(g), channel(b), (a * 255.0).round() as u8]);
    }
    let mut bytes = Vec::new();
    pixels
        .write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Png)
        .expect("encoding an in-memory PNG cannot fail");
    bytes
}

static DARK_IMAGE: LazyLock<Arc<Image>> = LazyLock::new(|| Arc::new(Image::from_bytes(ImageFormat::Png, render(&DARK))));
static LIGHT_IMAGE: LazyLock<Arc<Image>> = LazyLock::new(|| Arc::new(Image::from_bytes(ImageFormat::Png, render(&LIGHT))));

/// The backdrop for the theme being drawn, built once and shared.
pub fn haze(dark: bool) -> Arc<Image> {
    if dark { DARK_IMAGE.clone() } else { LIGHT_IMAGE.clone() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stop_list_interpolates_and_ends_transparent() {
        assert!((stop_alpha(FOUR_STOPS, 0.0) - 0.46).abs() < 1e-6);
        assert!((stop_alpha(FOUR_STOPS, 0.19) - 0.38).abs() < 1e-3, "halfway between 46% and 30%");
        assert_eq!(stop_alpha(FOUR_STOPS, 1.0), 0.0);
        assert_eq!(stop_alpha(FOUR_STOPS, 1.7), 0.0, "outside the ellipse nothing is painted");
    }

    #[test]
    fn the_corner_under_the_first_blob_is_lighter_than_the_base() {
        let layers = blobs(&DARK);
        let [r, _, _, a] = pixel(&DARK, &layers, 0.10, 0.06);
        assert!(r > DARK.base.r + 0.1, "the d1 blob lifts its corner");
        assert!((a - 0.8).abs() < 1e-6, "the whole composition sits at 80%");
    }

    #[test]
    fn both_themes_encode() {
        assert!(!render(&DARK).is_empty());
        assert!(!render(&LIGHT).is_empty());
    }
}
