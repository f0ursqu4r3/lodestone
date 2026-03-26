//! Text source rendering — rasterizes text to RGBA frames using cosmic-text.
//!
//! Uses CPU-side text layout and rasterization via cosmic-text. A global
//! [`FontSystem`] is lazily initialized once and shared (behind a mutex) across
//! all render calls.

use std::sync::Mutex;

use cosmic_text::{
    Align, Attrs, Buffer, Family, FontSystem, Metrics, Shaping, Style, SwashCache, SwashContent,
    Weight,
};

use crate::gstreamer::types::RgbaFrame;
use crate::scene::{SourceProperties, TextAlignment};

/// Global font system, initialized once via [`init_font_system`].
static FONT_SYSTEM: Mutex<Option<FontSystem>> = Mutex::new(None);

/// Initialize the global font system with system fonts.
///
/// This should be called once at startup. Subsequent calls are no-ops.
///
/// # Panics
///
/// Panics if the mutex is poisoned.
pub fn init_font_system() {
    let mut guard = FONT_SYSTEM.lock().unwrap();
    if guard.is_none() {
        *guard = Some(FontSystem::new());
    }
}

/// Render a text source to an RGBA frame.
///
/// Extracts text properties from [`SourceProperties::Text`] and rasterizes the
/// text using cosmic-text. Returns `None` for non-text variants, empty text, or
/// if the font system is unavailable.
pub fn render_text_source(props: &SourceProperties) -> Option<RgbaFrame> {
    let SourceProperties::Text {
        content,
        font_family,
        font_size,
        font_color,
        background_color,
        bold,
        italic,
        alignment,
        outline,
        padding,
        wrap_width,
    } = props
    else {
        return None;
    };

    if content.is_empty() {
        return None;
    }

    let mut font_system_guard = FONT_SYSTEM.lock().ok()?;
    let font_system = font_system_guard.as_mut()?;

    // Parse font family
    let family = parse_font_family(font_family);

    // Build attributes
    let weight = if *bold { Weight::BOLD } else { Weight::NORMAL };
    let style = if *italic {
        Style::Italic
    } else {
        Style::Normal
    };
    let attrs = Attrs::new().family(family).weight(weight).style(style);

    // Create buffer with metrics
    let line_height = font_size * 1.2;
    let metrics = Metrics::new(*font_size, line_height);
    let mut buffer = Buffer::new(font_system, metrics);

    // Set wrap width if provided
    let width_for_layout = wrap_width.unwrap_or(4096.0);
    buffer.set_size(font_system, Some(width_for_layout), None);

    // Map alignment
    let cosmic_align = match alignment {
        TextAlignment::Left => Some(Align::Left),
        TextAlignment::Center => Some(Align::Center),
        TextAlignment::Right => Some(Align::Right),
    };

    buffer.set_text(
        font_system,
        content,
        &attrs,
        Shaping::Advanced,
        cosmic_align,
    );
    buffer.shape_until_scroll(font_system, false);

    // Compute bounding box from layout runs
    let pad = *padding;
    let outline_width = outline.map(|o| o.width).unwrap_or(0.0);
    let extra = outline_width.ceil() as i32;

    let mut max_w: f32 = 0.0;
    let mut max_h: f32 = 0.0;

    for run in buffer.layout_runs() {
        max_w = max_w.max(run.line_w);
        max_h = max_h.max(run.line_y + line_height * 0.5);
    }

    if max_w <= 0.0 || max_h <= 0.0 {
        return None;
    }

    // Final image dimensions with padding and outline margin
    let img_w = (max_w + pad * 2.0 + extra as f32 * 2.0).ceil() as u32;
    let img_h = (max_h + pad * 2.0 + extra as f32 * 2.0).ceil() as u32;

    if img_w == 0 || img_h == 0 {
        return None;
    }

    // Allocate pixel buffer with background color
    let bg = color_f32_to_u8(background_color);
    let mut pixels = vec![0u8; (img_w * img_h * 4) as usize];
    for chunk in pixels.chunks_exact_mut(4) {
        chunk.copy_from_slice(&bg);
    }

    let fg = color_f32_to_u8(font_color);
    let origin_x = pad + extra as f32;
    let origin_y = pad + extra as f32;

    let mut swash_cache = SwashCache::new();

    // Render outline if configured
    if let Some(outline_cfg) = outline {
        let outline_color = color_f32_to_u8(&outline_cfg.color);
        let offsets = outline_offsets(outline_cfg.width);

        for run in buffer.layout_runs() {
            for glyph in run.glyphs.iter() {
                for &(dx, dy) in &offsets {
                    let physical = glyph.physical((origin_x + dx, origin_y + dy), 1.0);
                    if let Some(image) = swash_cache.get_image(font_system, physical.cache_key) {
                        draw_glyph_image(
                            &mut pixels,
                            img_w,
                            img_h,
                            image,
                            physical.x,
                            physical.y + run.line_y as i32,
                            &outline_color,
                        );
                    }
                }
            }
        }
    }

    // Render foreground glyphs
    for run in buffer.layout_runs() {
        for glyph in run.glyphs.iter() {
            let physical = glyph.physical((origin_x, origin_y), 1.0);
            if let Some(image) = swash_cache.get_image(font_system, physical.cache_key) {
                draw_glyph_image(
                    &mut pixels,
                    img_w,
                    img_h,
                    image,
                    physical.x,
                    physical.y + run.line_y as i32,
                    &fg,
                );
            }
        }
    }

    Some(RgbaFrame {
        data: pixels,
        width: img_w,
        height: img_h,
    })
}

/// Parse a font family string into a cosmic-text [`Family`].
///
/// Recognized prefixes:
/// - `"bundled:sans"` -> [`Family::SansSerif`]
/// - `"bundled:serif"` -> [`Family::Serif`]
/// - `"bundled:mono"` -> [`Family::Monospace`]
/// - `"bundled:display"` -> [`Family::SansSerif`]
/// - `"system:Name"` -> [`Family::Name("Name")`]
/// - plain name -> [`Family::Name(name)`]
fn parse_font_family(family: &str) -> Family<'_> {
    if let Some(bundled) = family.strip_prefix("bundled:") {
        match bundled {
            "sans" => Family::SansSerif,
            "serif" => Family::Serif,
            "mono" => Family::Monospace,
            "display" => Family::SansSerif,
            _ => Family::SansSerif,
        }
    } else if let Some(name) = family.strip_prefix("system:") {
        Family::Name(name)
    } else {
        Family::Name(family)
    }
}

/// Draw a single glyph image onto the pixel buffer with alpha blending.
fn draw_glyph_image(
    pixels: &mut [u8],
    img_w: u32,
    img_h: u32,
    image: &cosmic_text::SwashImage,
    glyph_x: i32,
    glyph_y: i32,
    color: &[u8; 4],
) {
    let px = glyph_x + image.placement.left;
    let py = glyph_y - image.placement.top;

    match image.content {
        SwashContent::Mask => {
            let mut i = 0;
            for off_y in 0..image.placement.height as i32 {
                for off_x in 0..image.placement.width as i32 {
                    let x = px + off_x;
                    let y = py + off_y;
                    if x >= 0 && y >= 0 && (x as u32) < img_w && (y as u32) < img_h {
                        let alpha = image.data[i];
                        if alpha > 0 {
                            let offset = ((y as u32 * img_w + x as u32) * 4) as usize;
                            alpha_blend(pixels, offset, color, alpha);
                        }
                    }
                    i += 1;
                }
            }
        }
        SwashContent::Color => {
            let mut i = 0;
            for off_y in 0..image.placement.height as i32 {
                for off_x in 0..image.placement.width as i32 {
                    let x = px + off_x;
                    let y = py + off_y;
                    if x >= 0 && y >= 0 && (x as u32) < img_w && (y as u32) < img_h {
                        let src = [
                            image.data[i],
                            image.data[i + 1],
                            image.data[i + 2],
                            image.data[i + 3],
                        ];
                        if src[3] > 0 {
                            let offset = ((y as u32 * img_w + x as u32) * 4) as usize;
                            alpha_blend(pixels, offset, &src, src[3]);
                        }
                    }
                    i += 4;
                }
            }
        }
        SwashContent::SubpixelMask => {
            // Subpixel rendering not supported in RGBA output; ignore.
        }
    }
}

/// Alpha-blend a single pixel.
fn alpha_blend(pixels: &mut [u8], offset: usize, color: &[u8; 4], alpha: u8) {
    let a = alpha as u16;
    let inv_a = 255 - a;
    pixels[offset] = ((color[0] as u16 * a + pixels[offset] as u16 * inv_a) / 255) as u8;
    pixels[offset + 1] = ((color[1] as u16 * a + pixels[offset + 1] as u16 * inv_a) / 255) as u8;
    pixels[offset + 2] = ((color[2] as u16 * a + pixels[offset + 2] as u16 * inv_a) / 255) as u8;
    pixels[offset + 3] = pixels[offset + 3].max(alpha);
}

/// Generate outline offsets for a given outline width.
/// Returns 8 evenly spaced offsets around a circle of the given radius.
fn outline_offsets(width: f32) -> Vec<(f32, f32)> {
    let steps = 8;
    (0..steps)
        .map(|i| {
            let angle = std::f32::consts::TAU * i as f32 / steps as f32;
            (angle.cos() * width, angle.sin() * width)
        })
        .collect()
}

/// Convert a float [0.0..1.0] RGBA color to byte RGBA.
fn color_f32_to_u8(color: &[f32; 4]) -> [u8; 4] {
    [
        (color[0].clamp(0.0, 1.0) * 255.0) as u8,
        (color[1].clamp(0.0, 1.0) * 255.0) as u8,
        (color[2].clamp(0.0, 1.0) * 255.0) as u8,
        (color[3].clamp(0.0, 1.0) * 255.0) as u8,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::TextOutline;

    /// Ensure the font system is initialized for tests.
    fn ensure_font_system() {
        init_font_system();
    }

    fn make_text_props(content: &str) -> SourceProperties {
        SourceProperties::Text {
            content: content.to_string(),
            font_family: "bundled:sans".to_string(),
            font_size: 48.0,
            font_color: [1.0, 1.0, 1.0, 1.0],
            background_color: [0.0, 0.0, 0.0, 0.0],
            bold: false,
            italic: false,
            alignment: TextAlignment::Left,
            outline: None,
            padding: 12.0,
            wrap_width: None,
        }
    }

    #[test]
    fn render_basic_text() {
        ensure_font_system();
        let props = make_text_props("Hello");
        let frame = render_text_source(&props).expect("should render basic text");
        assert!(frame.width > 0);
        assert!(frame.height > 0);
        assert!(!frame.data.is_empty());
    }

    #[test]
    fn empty_text_returns_none() {
        ensure_font_system();
        let props = make_text_props("");
        assert!(render_text_source(&props).is_none());
    }

    #[test]
    fn non_text_props_returns_none() {
        ensure_font_system();
        let props = SourceProperties::Display { screen_index: 0 };
        assert!(render_text_source(&props).is_none());
    }

    #[test]
    fn parse_bundled_families() {
        assert!(matches!(
            parse_font_family("bundled:sans"),
            Family::SansSerif
        ));
        assert!(matches!(parse_font_family("bundled:serif"), Family::Serif));
        assert!(matches!(
            parse_font_family("bundled:mono"),
            Family::Monospace
        ));
        assert!(matches!(
            parse_font_family("bundled:display"),
            Family::SansSerif
        ));
    }

    #[test]
    fn parse_system_family() {
        match parse_font_family("system:Helvetica") {
            Family::Name(name) => assert_eq!(name, "Helvetica"),
            _ => panic!("expected Family::Name"),
        }
    }

    #[test]
    fn text_with_outline_renders() {
        ensure_font_system();
        let props = SourceProperties::Text {
            content: "Bold Outline".to_string(),
            font_family: "bundled:sans".to_string(),
            font_size: 48.0,
            font_color: [1.0, 1.0, 1.0, 1.0],
            background_color: [0.0, 0.0, 0.0, 0.0],
            bold: true,
            italic: false,
            alignment: TextAlignment::Left,
            outline: Some(TextOutline {
                color: [0.0, 0.0, 0.0, 1.0],
                width: 2.0,
            }),
            padding: 12.0,
            wrap_width: None,
        };
        let frame = render_text_source(&props).expect("should render text with outline");
        assert!(frame.width > 0);
        assert!(frame.height > 0);
    }
}
