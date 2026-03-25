# New Source Types Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add four new source types to Lodestone's source library: Color (solid/gradient), Text (styled labels), Audio (device/file input), and Browser (stubbed placeholder).

**Architecture:** All visual sources (Color, Text, Browser) render to `RgbaFrame` on the CPU and push into the shared frame map via `GstCommand::LoadImageFrame`, matching the existing Image source pattern. Audio sources create GStreamer audio pipelines and mix into the recording/streaming output. No compositor changes needed.

**Tech Stack:** Rust, cosmic-text (CPU text layout/rasterization), fontdb (font discovery), GStreamer (audio pipelines), egui (properties UI), serde/TOML (serialization).

**Spec:** `docs/superpowers/specs/2026-03-25-new-source-types-design.md`

---

## File Structure

### New Files
- `src/text_source.rs` — CPU text rendering to RgbaFrame using cosmic-text
- `src/color_source.rs` — solid color and gradient rendering to RgbaFrame
- `fonts/` — bundled font files (Inter, Noto Serif, JetBrains Mono, display face)

### Modified Files
- `Cargo.toml` — add cosmic-text dependency (fontdb bundled within cosmic-text)
- `src/main.rs` — add `mod text_source; mod color_source;` declarations
- `src/scene.rs` — new SourceType variants (Text, Color), new SourceProperties variants (Text, Color, Audio, Browser), supporting types
- `src/gstreamer/commands.rs` — new CaptureSourceConfig variants (AudioDevice, AudioFile), new GstCommand variants (SetSourceVolume, SetSourceMuted)
- `src/gstreamer/thread.rs` — audio capture pipeline handling, audio mixer integration
- `src/ui/library_panel.rs` — add menu entries for new types, source creation defaults
- `src/ui/draw_helpers.rs` — source_icon() mappings for Text and Color
- `src/ui/properties_panel.rs` — properties UI for all four new types
- `src/ui/sources_panel.rs` — start_capture_from_properties match arms for new types
- `src/ui/scenes_panel.rs` — apply_scene_diff and send_capture_for_scene match arms

---

## Task 1: Data Model — Supporting Types

**Files:**
- Modify: `src/scene.rs:74-116`

- [ ] **Step 1: Add supporting types above SourceProperties**

Add these types after the `Transform` struct (after line 90 in `src/scene.rs`):

```rust
/// Text alignment for text sources.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum TextAlignment {
    Left,
    Center,
    Right,
}

/// Text outline configuration.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TextOutline {
    pub color: [f32; 4],
    pub width: f32,
}

/// Color fill for color sources.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ColorFill {
    Solid {
        color: [f32; 4],
    },
    LinearGradient {
        angle: f32,
        stops: Vec<GradientStop>,
    },
    RadialGradient {
        center: (f32, f32),
        radius: f32,
        stops: Vec<GradientStop>,
    },
}

/// A single color stop in a gradient.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct GradientStop {
    pub position: f32,
    pub color: [f32; 4],
}

/// Audio input configuration for audio sources.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AudioInput {
    Device {
        device_uid: String,
        device_name: String,
    },
    File {
        path: String,
        #[serde(default)]
        looping: bool,
    },
}
```

- [ ] **Step 2: Add Text and Color to SourceType enum**

In the `SourceType` enum (line 74-82), add two new variants before the closing brace:

```rust
pub enum SourceType {
    Display,
    Window,
    Camera,
    Audio,
    Image,
    Browser,
    Text,
    Color,
}
```

- [ ] **Step 3: Add new SourceProperties variants**

In the `SourceProperties` enum (lines 93-110), add four new variants before the closing brace. Use `#[serde(default)]` on all fields for backward-compatible deserialization:

```rust
pub enum SourceProperties {
    Display {
        screen_index: u32,
    },
    Window {
        window_id: u32,
        window_title: String,
        owner_name: String,
    },
    Camera {
        device_index: u32,
        device_name: String,
    },
    Image {
        path: String,
    },
    Text {
        #[serde(default = "default_text_content")]
        content: String,
        #[serde(default = "default_font_family")]
        font_family: String,
        #[serde(default = "default_font_size")]
        font_size: f32,
        #[serde(default = "default_font_color")]
        font_color: [f32; 4],
        #[serde(default = "default_transparent")]
        background_color: [f32; 4],
        #[serde(default)]
        bold: bool,
        #[serde(default)]
        italic: bool,
        #[serde(default = "default_text_alignment")]
        alignment: TextAlignment,
        #[serde(default)]
        outline: Option<TextOutline>,
        #[serde(default = "default_padding")]
        padding: f32,
        #[serde(default)]
        wrap_width: Option<f32>,
    },
    Color {
        #[serde(default = "default_color_fill")]
        fill: ColorFill,
    },
    Audio {
        #[serde(default = "default_audio_input")]
        input: AudioInput,
    },
    Browser {
        #[serde(default)]
        url: String,
        #[serde(default = "default_browser_width")]
        width: u32,
        #[serde(default = "default_browser_height")]
        height: u32,
    },
}
```

- [ ] **Step 4: Add serde default functions**

Add default functions after the `SourceProperties` Default impl:

```rust
fn default_text_content() -> String {
    "Text".to_string()
}

fn default_font_family() -> String {
    "bundled:sans".to_string()
}

fn default_font_size() -> f32 {
    48.0
}

fn default_font_color() -> [f32; 4] {
    [1.0, 1.0, 1.0, 1.0]
}

fn default_transparent() -> [f32; 4] {
    [0.0, 0.0, 0.0, 0.0]
}

fn default_text_alignment() -> TextAlignment {
    TextAlignment::Left
}

fn default_padding() -> f32 {
    12.0
}

fn default_color_fill() -> ColorFill {
    ColorFill::Solid {
        color: [1.0, 1.0, 1.0, 1.0],
    }
}

fn default_audio_input() -> AudioInput {
    AudioInput::Device {
        device_uid: String::new(),
        device_name: String::new(),
    }
}

fn default_browser_width() -> u32 {
    1920
}

fn default_browser_height() -> u32 {
    1080
}
```

- [ ] **Step 5: Add temporary wildcard arms to keep project compiling**

The new enum variants will break exhaustive matches across the codebase. Add temporary `_ => {}` or `_ => unreachable!()` wildcard arms to ALL exhaustive match statements on `SourceType`, `SourceProperties`, and `CaptureSourceConfig` to keep the project compiling between tasks. Key locations:
- `src/ui/draw_helpers.rs` — `source_icon()`: add `_ => egui_phosphor::regular::QUESTION` (temporary)
- `src/ui/properties_panel.rs` — `draw_source_properties()`: add `_ => {}` arm
- `src/ui/sources_panel.rs` — `start_capture_from_properties()`: already has `_ => {}`
- `src/ui/scenes_panel.rs` — `apply_scene_diff()` and `send_capture_for_scene()`: add `_ => {}` arms
- `src/ui/library_panel.rs` — `add_library_source()`: already has `_ =>` fallback

These will be replaced with proper implementations in later tasks.

- [ ] **Step 6: Add source-type/properties migration on load**

In the scene loading code (wherever `SceneCollection` or library sources are deserialized from TOML), add a post-deserialization migration pass. After loading, iterate over `library` and check that each source's `source_type` matches its `properties` variant. If mismatched (e.g., `SourceType::Audio` with `SourceProperties::Display`), replace `properties` with the correct default:

```rust
// Post-load migration for source type/properties mismatch
for source in &mut collection.library {
    let needs_migration = match (&source.source_type, &source.properties) {
        (SourceType::Display, SourceProperties::Display { .. }) => false,
        (SourceType::Window, SourceProperties::Window { .. }) => false,
        (SourceType::Camera, SourceProperties::Camera { .. }) => false,
        (SourceType::Image, SourceProperties::Image { .. }) => false,
        (SourceType::Text, SourceProperties::Text { .. }) => false,
        (SourceType::Color, SourceProperties::Color { .. }) => false,
        (SourceType::Audio, SourceProperties::Audio { .. }) => false,
        (SourceType::Browser, SourceProperties::Browser { .. }) => false,
        _ => true,
    };
    if needs_migration {
        source.properties = match source.source_type {
            SourceType::Text => SourceProperties::Text { /* use defaults */ },
            SourceType::Color => SourceProperties::Color { fill: default_color_fill() },
            SourceType::Audio => SourceProperties::Audio { input: default_audio_input() },
            SourceType::Browser => SourceProperties::Browser { url: String::new(), width: 1920, height: 1080 },
            _ => source.properties.clone(), // Display/Window/Camera/Image keep existing
        };
    }
}
```

- [ ] **Step 7: Build and verify compilation**

Run: `cargo build 2>&1 | head -20`
Expected: Clean compilation — all matches have wildcard arms.

- [ ] **Step 8: Commit**

```bash
git add src/scene.rs src/ui/draw_helpers.rs src/ui/properties_panel.rs src/ui/scenes_panel.rs
git commit -m "feat: add data model types for text, color, audio, browser sources"
```

---

## Task 2: GStreamer Command Types

**Files:**
- Modify: `src/gstreamer/commands.rs:37-96`

- [ ] **Step 1: Add CaptureSourceConfig variants**

In `CaptureSourceConfig` enum (lines 84-96), add two new variants before the closing brace:

```rust
pub enum CaptureSourceConfig {
    Screen {
        screen_index: u32,
        exclude_self: bool,
    },
    Window {
        window_id: u32,
    },
    Camera {
        device_index: u32,
    },
    AudioDevice {
        device_uid: String,
    },
    AudioFile {
        path: String,
        looping: bool,
    },
}
```

- [ ] **Step 2: Add GstCommand variants**

In `GstCommand` enum (lines 37-81), add two new variants before `Shutdown`:

```rust
    /// Per-source volume control (distinct from global SetAudioVolume).
    SetSourceVolume {
        source_id: SourceId,
        volume: f32,
    },
    /// Per-source mute (distinct from global SetAudioMuted).
    SetSourceMuted {
        source_id: SourceId,
        muted: bool,
    },
```

- [ ] **Step 3: Build and verify**

Run: `cargo build 2>&1 | head -40`
Expected: More exhaustive match errors from thread.rs (expected). The commands file itself should compile.

- [ ] **Step 4: Commit**

```bash
git add src/gstreamer/commands.rs
git commit -m "feat: add audio capture config and per-source volume/mute commands"
```

---

## Task 3: Color Source Renderer

**Files:**
- Create: `src/color_source.rs`
- Modify: `src/main.rs:1-9`

- [ ] **Step 1: Create color_source.rs with tests**

Create `src/color_source.rs`:

```rust
//! Color source rendering — generates solid color and gradient RgbaFrames.

use crate::gstreamer::types::RgbaFrame;
use crate::scene::{ColorFill, GradientStop};

/// Render a color source to an RgbaFrame.
///
/// For solid fills, renders at the given dimensions.
/// For gradients, renders at the given dimensions (capped at 1920x1080).
pub fn render_color_source(fill: &ColorFill, width: u32, height: u32) -> RgbaFrame {
    let w = width.min(1920).max(1) as usize;
    let h = height.min(1080).max(1) as usize;

    let mut data = vec![0u8; w * h * 4];

    match fill {
        ColorFill::Solid { color } => {
            let pixel = color_to_bytes(color);
            for chunk in data.chunks_exact_mut(4) {
                chunk.copy_from_slice(&pixel);
            }
        }
        ColorFill::LinearGradient { angle, stops } => {
            let angle_rad = angle.to_radians();
            let dx = angle_rad.cos();
            let dy = angle_rad.sin();

            for y in 0..h {
                for x in 0..w {
                    let nx = x as f32 / (w - 1).max(1) as f32;
                    let ny = y as f32 / (h - 1).max(1) as f32;
                    // Project onto gradient axis, centered at (0.5, 0.5)
                    let t = ((nx - 0.5) * dx + (ny - 0.5) * dy + 0.5).clamp(0.0, 1.0);
                    let color = interpolate_stops(stops, t);
                    let offset = (y * w + x) * 4;
                    data[offset..offset + 4].copy_from_slice(&color_to_bytes(&color));
                }
            }
        }
        ColorFill::RadialGradient {
            center,
            radius,
            stops,
        } => {
            let r = radius.max(0.001);
            for y in 0..h {
                for x in 0..w {
                    let nx = x as f32 / (w - 1).max(1) as f32;
                    let ny = y as f32 / (h - 1).max(1) as f32;
                    let dist =
                        ((nx - center.0).powi(2) + (ny - center.1).powi(2)).sqrt() / r;
                    let t = dist.clamp(0.0, 1.0);
                    let color = interpolate_stops(stops, t);
                    let offset = (y * w + x) * 4;
                    data[offset..offset + 4].copy_from_slice(&color_to_bytes(&color));
                }
            }
        }
    }

    RgbaFrame {
        data,
        width: w as u32,
        height: h as u32,
    }
}

/// Interpolate between gradient stops at position t (0.0..1.0).
fn interpolate_stops(stops: &[GradientStop], t: f32) -> [f32; 4] {
    if stops.is_empty() {
        return [0.0, 0.0, 0.0, 1.0];
    }
    if stops.len() == 1 {
        return stops[0].color;
    }

    // Clamp t to the stop range
    if t <= stops[0].position {
        return stops[0].color;
    }
    if t >= stops[stops.len() - 1].position {
        return stops[stops.len() - 1].color;
    }

    // Find the two surrounding stops
    for i in 0..stops.len() - 1 {
        let s0 = &stops[i];
        let s1 = &stops[i + 1];
        if t >= s0.position && t <= s1.position {
            let range = s1.position - s0.position;
            if range < f32::EPSILON {
                return s0.color;
            }
            let local_t = (t - s0.position) / range;
            return [
                s0.color[0] + (s1.color[0] - s0.color[0]) * local_t,
                s0.color[1] + (s1.color[1] - s0.color[1]) * local_t,
                s0.color[2] + (s1.color[2] - s0.color[2]) * local_t,
                s0.color[3] + (s1.color[3] - s0.color[3]) * local_t,
            ];
        }
    }

    stops[stops.len() - 1].color
}

/// Convert a float RGBA color to byte RGBA.
fn color_to_bytes(color: &[f32; 4]) -> [u8; 4] {
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

    #[test]
    fn solid_color_fills_all_pixels() {
        let fill = ColorFill::Solid {
            color: [1.0, 0.0, 0.0, 1.0],
        };
        let frame = render_color_source(&fill, 4, 4);
        assert_eq!(frame.width, 4);
        assert_eq!(frame.height, 4);
        assert_eq!(frame.data.len(), 64); // 4*4*4
        // Every pixel should be red
        for chunk in frame.data.chunks_exact(4) {
            assert_eq!(chunk, &[255, 0, 0, 255]);
        }
    }

    #[test]
    fn linear_gradient_endpoints() {
        let stops = vec![
            GradientStop {
                position: 0.0,
                color: [0.0, 0.0, 0.0, 1.0],
            },
            GradientStop {
                position: 1.0,
                color: [1.0, 1.0, 1.0, 1.0],
            },
        ];
        let fill = ColorFill::LinearGradient {
            angle: 0.0, // left to right
            stops,
        };
        let frame = render_color_source(&fill, 10, 1);
        // First pixel should be dark, last pixel should be bright
        assert!(frame.data[0] < 50); // R of first pixel
        assert!(frame.data[(9 * 4)] > 200); // R of last pixel
    }

    #[test]
    fn radial_gradient_center_is_first_stop() {
        let stops = vec![
            GradientStop {
                position: 0.0,
                color: [1.0, 0.0, 0.0, 1.0],
            },
            GradientStop {
                position: 1.0,
                color: [0.0, 0.0, 1.0, 1.0],
            },
        ];
        let fill = ColorFill::RadialGradient {
            center: (0.5, 0.5),
            radius: 0.5,
            stops,
        };
        // Use odd dimensions so there's a true center pixel
        let frame = render_color_source(&fill, 11, 11);
        // Center pixel (5,5) should be red
        let center_offset = (5 * 11 + 5) * 4;
        assert_eq!(frame.data[center_offset], 255); // R
        assert_eq!(frame.data[center_offset + 2], 0); // B
    }

    #[test]
    fn empty_stops_returns_black() {
        let fill = ColorFill::LinearGradient {
            angle: 0.0,
            stops: vec![],
        };
        let frame = render_color_source(&fill, 2, 2);
        for chunk in frame.data.chunks_exact(4) {
            assert_eq!(chunk, &[0, 0, 0, 255]);
        }
    }

    #[test]
    fn dimensions_capped_at_1920x1080() {
        let fill = ColorFill::Solid {
            color: [1.0, 1.0, 1.0, 1.0],
        };
        let frame = render_color_source(&fill, 4000, 3000);
        assert_eq!(frame.width, 1920);
        assert_eq!(frame.height, 1080);
    }

    #[test]
    fn color_to_bytes_clamps() {
        assert_eq!(color_to_bytes(&[1.5, -0.5, 0.5, 0.0]), [255, 0, 127, 0]);
    }

    #[test]
    fn interpolate_single_stop() {
        let stops = vec![GradientStop {
            position: 0.5,
            color: [1.0, 0.0, 0.0, 1.0],
        }];
        assert_eq!(interpolate_stops(&stops, 0.0), [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(interpolate_stops(&stops, 1.0), [1.0, 0.0, 0.0, 1.0]);
    }
}
```

- [ ] **Step 2: Add module declaration to main.rs**

In `src/main.rs`, add after the `mod image_source;` line (line 2):

```rust
mod color_source;
```

- [ ] **Step 3: Run tests**

Run: `cargo test color_source -- --nocapture`
Expected: All 7 tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/color_source.rs src/main.rs
git commit -m "feat: add color source renderer with solid and gradient support"
```

---

## Task 4: Text Source Renderer

**Files:**
- Create: `src/text_source.rs`
- Modify: `Cargo.toml`
- Modify: `src/main.rs`

- [ ] **Step 1: Add cosmic-text and fontdb to Cargo.toml**

In `Cargo.toml`, add cosmic-text (it bundles fontdb internally, no separate fontdb dependency needed). Replace the commented lines (12-13) with:

```toml
# glyphon = "0.10"    # TODO: re-enable when glyphon supports wgpu 29 (egui-wgpu compat)
cosmic-text = "0.18"
```

Note: `cosmic-text` has no wgpu dependency — the wgpu incompatibility was only with `glyphon` (the GPU text renderer). `cosmic-text` is a pure CPU text shaping/layout library and works independently.

- [ ] **Step 2: Verify dependency resolves**

Run: `cargo check 2>&1 | tail -5`
Expected: Compiles (with exhaustive match warnings from scene.rs changes, expected).

- [ ] **Step 3: Create text_source.rs**

Create `src/text_source.rs`:

```rust
//! Text source rendering — rasterizes styled text to RgbaFrame using cosmic-text.

use crate::gstreamer::types::RgbaFrame;
use crate::scene::{SourceProperties, TextAlignment, TextOutline};
use cosmic_text::{
    Attrs, Buffer, Color as CosmicColor, Family, FontSystem, Metrics, Shaping, Style, SwashCache,
    Weight,
};
use std::sync::Mutex;

/// Global font system, initialized once and reused across renders.
static FONT_SYSTEM: Mutex<Option<FontSystem>> = Mutex::new(None);

/// Initialize the global font system with system fonts.
/// Call once at startup.
pub fn init_font_system() {
    let mut fs = FontSystem::new();
    // System fonts are loaded automatically by FontSystem::new()
    // TODO: load bundled fonts via fs.db_mut().load_font_data(include_bytes!(...))
    let mut guard = FONT_SYSTEM.lock().unwrap();
    *guard = Some(fs);
}

/// Render a text source to an RgbaFrame.
///
/// Returns `None` if the text is empty or the font system is not initialized.
pub fn render_text_source(props: &SourceProperties) -> Option<RgbaFrame> {
    let (
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
    ) = match props {
        SourceProperties::Text {
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
        } => (
            content,
            font_family,
            *font_size,
            font_color,
            background_color,
            *bold,
            *italic,
            alignment,
            outline,
            *padding,
            *wrap_width,
        ),
        _ => return None,
    };

    if content.is_empty() {
        return None;
    }

    let mut guard = FONT_SYSTEM.lock().unwrap();
    let font_system = guard.as_mut()?;

    let mut swash_cache = SwashCache::new();

    // Parse font family
    let family = parse_font_family(font_family);

    // Build attributes
    let weight = if bold { Weight::BOLD } else { Weight::NORMAL };
    let style = if italic { Style::Italic } else { Style::Normal };
    let attrs = Attrs::new()
        .family(family)
        .weight(weight)
        .style(style);

    // Create buffer
    let metrics = Metrics::new(font_size, font_size * 1.2);
    let wrap = wrap_width.unwrap_or(f32::INFINITY);
    let mut buffer = Buffer::new(font_system, metrics);
    buffer.set_size(font_system, Some(wrap), None);
    buffer.set_text(font_system, content, attrs, Shaping::Advanced);
    buffer.shape_until_scroll(font_system, false);

    // Compute bounding box from layout runs
    let mut max_width: f32 = 0.0;
    let mut total_height: f32 = 0.0;
    for run in buffer.layout_runs() {
        max_width = max_width.max(run.line_w);
        total_height = total_height.max(run.line_y + font_size);
    }

    if max_width <= 0.0 || total_height <= 0.0 {
        return None;
    }

    let outline_width = outline.as_ref().map_or(0.0, |o| o.width);
    let canvas_width = (max_width + padding * 2.0 + outline_width * 2.0).ceil() as u32;
    let canvas_height = (total_height + padding * 2.0 + outline_width * 2.0).ceil() as u32;

    if canvas_width == 0 || canvas_height == 0 {
        return None;
    }

    // Allocate pixel buffer with background
    let mut pixels = vec![0u8; (canvas_width * canvas_height * 4) as usize];
    let bg = color_to_bytes(background_color);
    for chunk in pixels.chunks_exact_mut(4) {
        chunk.copy_from_slice(&bg);
    }

    let offset_x = padding + outline_width;
    let offset_y = padding + outline_width;

    // Render outline first (if any)
    if let Some(outline_cfg) = outline {
        let outline_color = CosmicColor::rgba(
            (outline_cfg.color[0] * 255.0) as u8,
            (outline_cfg.color[1] * 255.0) as u8,
            (outline_cfg.color[2] * 255.0) as u8,
            (outline_cfg.color[3] * 255.0) as u8,
        );
        let steps = 8;
        for i in 0..steps {
            let angle = (i as f32 / steps as f32) * std::f32::consts::TAU;
            let dx = angle.cos() * outline_cfg.width;
            let dy = angle.sin() * outline_cfg.width;
            render_glyphs(
                font_system,
                &mut swash_cache,
                &buffer,
                &mut pixels,
                canvas_width,
                canvas_height,
                offset_x + dx,
                offset_y + dy,
                alignment,
                max_width,
                outline_color,
            );
        }
    }

    // Render foreground text
    let fg_color = CosmicColor::rgba(
        (font_color[0] * 255.0) as u8,
        (font_color[1] * 255.0) as u8,
        (font_color[2] * 255.0) as u8,
        (font_color[3] * 255.0) as u8,
    );
    render_glyphs(
        font_system,
        &mut swash_cache,
        &buffer,
        &mut pixels,
        canvas_width,
        canvas_height,
        offset_x,
        offset_y,
        alignment,
        max_width,
        fg_color,
    );

    Some(RgbaFrame {
        data: pixels,
        width: canvas_width,
        height: canvas_height,
    })
}

/// Rasterize all glyphs in the buffer onto the pixel buffer at the given offset.
fn render_glyphs(
    font_system: &mut FontSystem,
    swash_cache: &mut SwashCache,
    buffer: &Buffer,
    pixels: &mut [u8],
    canvas_width: u32,
    canvas_height: u32,
    offset_x: f32,
    offset_y: f32,
    alignment: &TextAlignment,
    max_width: f32,
    color: CosmicColor,
) {
    for run in buffer.layout_runs() {
        // Apply alignment offset
        let align_offset = match alignment {
            TextAlignment::Left => 0.0,
            TextAlignment::Center => (max_width - run.line_w) / 2.0,
            TextAlignment::Right => max_width - run.line_w,
        };

        for glyph in run.glyphs.iter() {
            let physical = glyph.physical((0.0, 0.0), 1.0);

            if let Some(image) = swash_cache.get_image(font_system, physical.cache_key) {
                let gx = (offset_x + align_offset + physical.x as f32 + image.placement.left as f32) as i32;
                let gy = (offset_y + run.line_y + physical.y as f32 - image.placement.top as f32) as i32;

                let (cr, cg, cb, ca) = (
                    color.r(),
                    color.g(),
                    color.b(),
                    color.a(),
                );

                for row in 0..image.placement.height as i32 {
                    let py = gy + row;
                    if py < 0 || py >= canvas_height as i32 {
                        continue;
                    }
                    for col in 0..image.placement.width as i32 {
                        let px = gx + col;
                        if px < 0 || px >= canvas_width as i32 {
                            continue;
                        }
                        let src_idx = (row * image.placement.width as i32 + col) as usize;
                        let alpha = if src_idx < image.data.len() {
                            image.data[src_idx]
                        } else {
                            0
                        };
                        if alpha == 0 {
                            continue;
                        }

                        let dst_idx = ((py as u32 * canvas_width + px as u32) * 4) as usize;
                        let a = (alpha as u16 * ca as u16) / 255;
                        let inv_a = 255 - a as u16;
                        pixels[dst_idx] = ((cr as u16 * a + pixels[dst_idx] as u16 * inv_a) / 255) as u8;
                        pixels[dst_idx + 1] = ((cg as u16 * a + pixels[dst_idx + 1] as u16 * inv_a) / 255) as u8;
                        pixels[dst_idx + 2] = ((cb as u16 * a + pixels[dst_idx + 2] as u16 * inv_a) / 255) as u8;
                        pixels[dst_idx + 3] = (pixels[dst_idx + 3] as u16 + a - (pixels[dst_idx + 3] as u16 * a / 255)) as u8;
                    }
                }
            }
        }
    }
}

/// Parse a font family string like "bundled:sans" or "system:Helvetica Neue".
fn parse_font_family(family: &str) -> Family<'_> {
    if let Some(name) = family.strip_prefix("bundled:") {
        match name {
            "sans" => Family::SansSerif,
            "serif" => Family::Serif,
            "mono" => Family::Monospace,
            "display" => Family::SansSerif, // TODO: map to bundled display face once added
            _ => Family::SansSerif,
        }
    } else if let Some(name) = family.strip_prefix("system:") {
        Family::Name(name)
    } else {
        Family::Name(family)
    }
}

/// Convert a float RGBA color to byte RGBA.
fn color_to_bytes(color: &[f32; 4]) -> [u8; 4] {
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

    fn setup() {
        init_font_system();
    }

    #[test]
    fn render_basic_text() {
        setup();
        let props = SourceProperties::Text {
            content: "Hello".to_string(),
            font_family: "bundled:sans".to_string(),
            font_size: 48.0,
            font_color: [1.0, 1.0, 1.0, 1.0],
            background_color: [0.0, 0.0, 0.0, 0.0],
            bold: false,
            italic: false,
            alignment: TextAlignment::Left,
            outline: None,
            padding: 0.0,
            wrap_width: None,
        };
        let frame = render_text_source(&props);
        assert!(frame.is_some());
        let frame = frame.unwrap();
        assert!(frame.width > 0);
        assert!(frame.height > 0);
        assert_eq!(frame.data.len(), (frame.width * frame.height * 4) as usize);
    }

    #[test]
    fn empty_text_returns_none() {
        setup();
        let props = SourceProperties::Text {
            content: String::new(),
            font_family: "bundled:sans".to_string(),
            font_size: 48.0,
            font_color: [1.0, 1.0, 1.0, 1.0],
            background_color: [0.0, 0.0, 0.0, 0.0],
            bold: false,
            italic: false,
            alignment: TextAlignment::Left,
            outline: None,
            padding: 0.0,
            wrap_width: None,
        };
        assert!(render_text_source(&props).is_none());
    }

    #[test]
    fn non_text_props_returns_none() {
        setup();
        let props = SourceProperties::Display { screen_index: 0 };
        assert!(render_text_source(&props).is_none());
    }

    #[test]
    fn parse_bundled_families() {
        assert!(matches!(parse_font_family("bundled:sans"), Family::SansSerif));
        assert!(matches!(parse_font_family("bundled:serif"), Family::Serif));
        assert!(matches!(parse_font_family("bundled:mono"), Family::Monospace));
    }

    #[test]
    fn parse_system_family() {
        match parse_font_family("system:Helvetica Neue") {
            Family::Name(n) => assert_eq!(n, "Helvetica Neue"),
            _ => panic!("Expected Family::Name"),
        }
    }

    #[test]
    fn text_with_outline_renders() {
        setup();
        let props = SourceProperties::Text {
            content: "Outlined".to_string(),
            font_family: "bundled:sans".to_string(),
            font_size: 32.0,
            font_color: [1.0, 1.0, 1.0, 1.0],
            background_color: [0.0, 0.0, 0.0, 0.0],
            bold: true,
            italic: false,
            alignment: TextAlignment::Center,
            outline: Some(TextOutline {
                color: [0.0, 0.0, 0.0, 1.0],
                width: 2.0,
            }),
            padding: 8.0,
            wrap_width: None,
        };
        let frame = render_text_source(&props);
        assert!(frame.is_some());
    }
}
```

- [ ] **Step 4: Add module declaration to main.rs**

In `src/main.rs`, add after the `mod color_source;` line:

```rust
mod text_source;
```

- [ ] **Step 5: Run tests**

Run: `cargo test text_source -- --nocapture`
Expected: All 6 tests pass.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml src/text_source.rs src/main.rs
git commit -m "feat: add text source renderer with cosmic-text rasterization"
```

---

## Task 5: UI Wiring — Icon Mappings and Add Menu

**Files:**
- Modify: `src/ui/draw_helpers.rs:13-22`
- Modify: `src/ui/library_panel.rs:239-316`

- [ ] **Step 1: Add icon mappings for Text and Color**

In `src/ui/draw_helpers.rs`, the `source_icon` function (lines 13-22) already has mappings for Audio (`SPEAKER_HIGH`), Browser (`BROWSER`), and Window (`APP_WINDOW`). Add mappings for the two new variants. Add before the closing brace:

```rust
        SourceType::Text => egui_phosphor::regular::TEXT_T,
        SourceType::Color => egui_phosphor::regular::PALETTE,
```

- [ ] **Step 2: Add new types to the add menu**

In `src/ui/library_panel.rs`, modify the `items` array in `draw_add_button` (lines 239-244). Add a separator and new source types:

Replace the items array and loop with:

```rust
                let capture_items: &[(&str, SourceType)] = &[
                    ("Display", SourceType::Display),
                    ("Window", SourceType::Window),
                    ("Camera", SourceType::Camera),
                    ("Image", SourceType::Image),
                ];
                let synthetic_items: &[(&str, SourceType)] = &[
                    ("Text", SourceType::Text),
                    ("Color", SourceType::Color),
                    ("Audio", SourceType::Audio),
                    ("Browser", SourceType::Browser),
                ];

                for (label, source_type) in capture_items {
                    if menu_item_icon(ui, source_icon(source_type), label) {
                        add_library_source(state, source_type.clone());
                        ui.memory_mut(|m| m.close_popup(popup_id));
                    }
                }
                ui.separator();
                for (label, source_type) in synthetic_items {
                    if menu_item_icon(ui, source_icon(source_type), label) {
                        add_library_source(state, source_type.clone());
                        ui.memory_mut(|m| m.close_popup(popup_id));
                    }
                }
```

- [ ] **Step 3: Add match arms in add_library_source**

In `src/ui/library_panel.rs`, in `add_library_source` (around line 316), replace the `_ =>` fallback with explicit arms for the four new types:

```rust
        SourceType::Text => {
            let count = state
                .library
                .iter()
                .filter(|s| matches!(s.source_type, SourceType::Text))
                .count();
            (
                format!("Text {}", count + 1),
                SourceProperties::Text {
                    content: "Text".to_string(),
                    font_family: "bundled:sans".to_string(),
                    font_size: 48.0,
                    font_color: [1.0, 1.0, 1.0, 1.0],
                    background_color: [0.0, 0.0, 0.0, 0.0],
                    bold: false,
                    italic: false,
                    alignment: crate::scene::TextAlignment::Left,
                    outline: None,
                    padding: 12.0,
                    wrap_width: None,
                },
            )
        }
        SourceType::Color => {
            let count = state
                .library
                .iter()
                .filter(|s| matches!(s.source_type, SourceType::Color))
                .count();
            (
                format!("Color {}", count + 1),
                SourceProperties::Color {
                    fill: crate::scene::ColorFill::Solid {
                        color: [1.0, 1.0, 1.0, 1.0],
                    },
                },
            )
        }
        SourceType::Audio => {
            let count = state
                .library
                .iter()
                .filter(|s| matches!(s.source_type, SourceType::Audio))
                .count();
            (
                format!("Audio {}", count + 1),
                SourceProperties::Audio {
                    input: crate::scene::AudioInput::Device {
                        device_uid: String::new(),
                        device_name: String::new(),
                    },
                },
            )
        }
        SourceType::Browser => {
            let count = state
                .library
                .iter()
                .filter(|s| matches!(s.source_type, SourceType::Browser))
                .count();
            (
                format!("Browser {}", count + 1),
                SourceProperties::Browser {
                    url: String::new(),
                    width: 1920,
                    height: 1080,
                },
            )
        }
```

- [ ] **Step 4: Build and verify**

Run: `cargo build 2>&1 | head -40`
Expected: Compiles (remaining exhaustive match errors may come from properties_panel, sources_panel, scenes_panel — those are addressed in later tasks).

- [ ] **Step 5: Commit**

```bash
git add src/ui/draw_helpers.rs src/ui/library_panel.rs
git commit -m "feat: add new source types to library panel add menu with icons"
```

---

## Task 6: Scene Integration — Capture Start/Stop

**Files:**
- Modify: `src/ui/sources_panel.rs:584-622`
- Modify: `src/ui/scenes_panel.rs:369-423`

- [ ] **Step 1: Add match arms in start_capture_from_properties**

In `src/ui/sources_panel.rs`, in `start_capture_from_properties` (around line 620), replace the `_ => {}` fallback with explicit arms:

```rust
        SourceProperties::Audio { input } => {
            let config = match input {
                crate::scene::AudioInput::Device { device_uid, .. } => {
                    CaptureSourceConfig::AudioDevice {
                        device_uid: device_uid.clone(),
                    }
                }
                crate::scene::AudioInput::File { path, looping } => {
                    CaptureSourceConfig::AudioFile {
                        path: path.clone(),
                        looping: *looping,
                    }
                }
            };
            let _ = tx.try_send(GstCommand::AddCaptureSource {
                source_id,
                config,
            });
        }
        // Text, Color, Browser, Image: no capture pipeline (frames pushed via LoadImageFrame)
        _ => {}
```

- [ ] **Step 2: Add match arms in apply_scene_diff**

In `src/ui/scenes_panel.rs`, in `apply_scene_diff` (around line 417), after the `Camera` arm and before the `Image` arm, add an `Audio` arm:

```rust
                crate::scene::SourceProperties::Audio { input } => {
                    let config = match input {
                        crate::scene::AudioInput::Device { device_uid, .. } => {
                            CaptureSourceConfig::AudioDevice {
                                device_uid: device_uid.clone(),
                            }
                        }
                        crate::scene::AudioInput::File { path, looping } => {
                            CaptureSourceConfig::AudioFile {
                                path: path.clone(),
                                looping: *looping,
                            }
                        }
                    };
                    let _ = tx.try_send(GstCommand::AddCaptureSource {
                        source_id: src_id,
                        config,
                    });
                }
```

Then ensure the fallback arm covers Text, Color, Browser, Image as no-ops. Replace `SourceProperties::Image { .. } => { ... }` with a wildcard:

```rust
                // Text, Color, Browser, Image: no capture pipeline
                _ => {}
```

- [ ] **Step 3: Do the same in send_capture_for_scene**

Apply the same pattern to `send_capture_for_scene` (around lines 477-523). Add the Audio match arm and update the fallback.

- [ ] **Step 4: Update stop_capture_for_source**

In `src/ui/sources_panel.rs`, find `stop_capture_for_source` (around line 630). It matches on `SourceType` to decide whether to send `RemoveCaptureSource`. Add `SourceType::Audio` to the match arm that sends the remove command, since audio sources also have capture pipelines that need teardown. Text, Color, and Browser should NOT send remove commands (they have no pipelines).

- [ ] **Step 5: Build and verify**

Run: `cargo build 2>&1 | head -40`
Expected: Compiles. Remaining errors should only be in properties_panel (addressed next).

- [ ] **Step 6: Commit**

```bash
git add src/ui/sources_panel.rs src/ui/scenes_panel.rs
git commit -m "feat: wire scene diff and capture start for new source types"
```

---

## Task 7: GStreamer Thread — Audio Pipeline and New Commands

> **Note on audio mixer:** This task wires up per-source audio pipelines (capture → volume → appsink) but does NOT yet connect them to an `audiomixer` that feeds into the recording/streaming output. The audio data is captured and volume-controlled but not mixed into the final output. Mixer integration is a follow-up task that requires changes to the existing encoding pipeline. For now, audio sources will be fully functional in the UI (device selection, file playback, volume/mute) but silent in recordings/streams until the mixer is connected.

**Files:**
- Modify: `src/gstreamer/thread.rs`

- [ ] **Step 1: Handle new CaptureSourceConfig variants**

In the `add_capture_source` function (around lines 84-132), add match arms for `AudioDevice` and `AudioFile`. These build GStreamer audio pipelines:

```rust
CaptureSourceConfig::AudioDevice { device_uid } => {
    // Build audio device capture pipeline
    match self.build_audio_device_pipeline(source_id, &device_uid) {
        Ok(()) => log::info!("Started audio device capture for {source_id:?}"),
        Err(e) => log::error!("Failed to start audio device capture: {e}"),
    }
    return;
}
CaptureSourceConfig::AudioFile { path, looping } => {
    // Build audio file playback pipeline
    match self.build_audio_file_pipeline(source_id, &path, looping) {
        Ok(()) => log::info!("Started audio file playback for {source_id:?}"),
        Err(e) => log::error!("Failed to start audio file playback: {e}"),
    }
    return;
}
```

- [ ] **Step 2: Implement build_audio_device_pipeline**

Add a new method to `GstThread`:

```rust
fn build_audio_device_pipeline(
    &mut self,
    source_id: SourceId,
    device_uid: &str,
) -> anyhow::Result<()> {
    use gstreamer::prelude::*;

    let pipeline = gstreamer::Pipeline::new();
    let src = gstreamer::ElementFactory::make("osxaudiosrc")
        .property("device", device_uid)
        .build()?;
    let convert = gstreamer::ElementFactory::make("audioconvert").build()?;
    let resample = gstreamer::ElementFactory::make("audioresample").build()?;
    let volume = gstreamer::ElementFactory::make("volume").build()?;
    let sink = gstreamer_app::AppSink::builder().build();

    pipeline.add_many([&src, &convert, &resample, &volume, sink.upcast_ref()])?;
    gstreamer::Element::link_many([&src, &convert, &resample, &volume, sink.upcast_ref()])?;

    pipeline.set_state(gstreamer::State::Playing)?;

    // Store pipeline and volume element for later control
    self.audio_pipelines.insert(source_id, AudioPipeline {
        pipeline,
        volume_element: volume,
        _bus_watch: None,
    });

    Ok(())
}
```

- [ ] **Step 3: Implement build_audio_file_pipeline**

Add another method:

```rust
fn build_audio_file_pipeline(
    &mut self,
    source_id: SourceId,
    path: &str,
    looping: bool,
) -> anyhow::Result<()> {
    use gstreamer::prelude::*;

    let uri = if path.starts_with("file://") {
        path.to_string()
    } else {
        format!("file://{path}")
    };

    let pipeline = gstreamer::Pipeline::new();
    let src = gstreamer::ElementFactory::make("uridecodebin")
        .property("uri", &uri)
        .build()?;
    let convert = gstreamer::ElementFactory::make("audioconvert").build()?;
    let resample = gstreamer::ElementFactory::make("audioresample").build()?;
    let volume = gstreamer::ElementFactory::make("volume").build()?;
    let sink = gstreamer_app::AppSink::builder().build();

    pipeline.add_many([&src, &convert, &resample, &volume, sink.upcast_ref()])?;
    // uridecodebin pads are dynamic — link on pad-added
    gstreamer::Element::link_many([&convert, &resample, &volume, sink.upcast_ref()])?;

    let convert_weak = convert.downgrade();
    src.connect_pad_added(move |_, pad| {
        if let Some(convert) = convert_weak.upgrade() {
            let sink_pad = convert.static_pad("sink").unwrap();
            if !sink_pad.is_linked() {
                let _ = pad.link(&sink_pad);
            }
        }
    });

    let bus_watch = if looping {
        let pipeline_weak = pipeline.downgrade();
        let bus = pipeline.bus().unwrap();
        // Store the BusWatchGuard — dropping it removes the watch!
        let guard = bus.add_watch(move |_, msg| {
            if let gstreamer::MessageView::Eos(..) = msg.view() {
                if let Some(pipeline) = pipeline_weak.upgrade() {
                    let _ = pipeline.seek_simple(
                        gstreamer::SeekFlags::FLUSH,
                        gstreamer::ClockTime::ZERO,
                    );
                }
            }
            gstreamer::glib::ControlFlow::Continue
        })?;
        Some(guard)
    } else {
        None
    };

    pipeline.set_state(gstreamer::State::Playing)?;

    self.audio_pipelines.insert(source_id, AudioPipeline {
        pipeline,
        volume_element: volume,
        _bus_watch: bus_watch,
    });

    Ok(())
}
```

- [ ] **Step 4: Add AudioPipeline struct and storage**

Add to the `GstThread` struct:

```rust
/// Per-source audio pipelines, keyed by SourceId.
audio_pipelines: HashMap<SourceId, AudioPipeline>,
```

And the struct:

```rust
struct AudioPipeline {
    pipeline: gstreamer::Pipeline,
    volume_element: gstreamer::Element,
    /// Holds the bus watch guard alive — dropping it removes the watch.
    _bus_watch: Option<gstreamer::bus::BusWatchGuard>,
}
```

Initialize `audio_pipelines: HashMap::new()` in the GstThread constructor.

- [ ] **Step 5: Handle SetSourceVolume and SetSourceMuted commands**

In the GstCommand match in the run loop, add:

```rust
GstCommand::SetSourceVolume { source_id, volume } => {
    if let Some(audio) = self.audio_pipelines.get(&source_id) {
        audio.volume_element.set_property("volume", volume as f64);
    }
}
GstCommand::SetSourceMuted { source_id, muted } => {
    if let Some(audio) = self.audio_pipelines.get(&source_id) {
        audio.volume_element.set_property("mute", muted);
    }
}
```

- [ ] **Step 6: Handle RemoveCaptureSource for audio**

In the existing `RemoveCaptureSource` handler, add cleanup for audio pipelines:

```rust
// Also remove audio pipeline if present
if let Some(audio) = self.audio_pipelines.remove(&source_id) {
    let _ = audio.pipeline.set_state(gstreamer::State::Null);
}
```

- [ ] **Step 7: Build and verify**

Run: `cargo build 2>&1 | head -40`
Expected: Compiles. May have warnings about unused audio pipelines until the UI is fully wired.

- [ ] **Step 8: Commit**

```bash
git add src/gstreamer/thread.rs
git commit -m "feat: add audio source capture pipelines and per-source volume control"
```

---

## Task 8: Properties Panel — All Four Source Types

**Files:**
- Modify: `src/ui/properties_panel.rs`

This is the largest UI task. Add properties UI for Text, Color, Audio, and Browser sources.

- [ ] **Step 1: Add Text source properties UI**

In `draw_source_properties`, add a match arm for `SourceType::Text`:

```rust
SourceType::Text => {
    if let SourceProperties::Text {
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
    } = &mut state.library[lib_idx].properties
    {
        // Content
        ui.label("Content");
        let mut text = content.clone();
        if ui.text_edit_multiline(&mut text).changed() {
            *content = text;
            changed = true;
        }

        // Font family (simplified — dropdown of bundled options)
        ui.label("Font");
        let font_options = ["bundled:sans", "bundled:serif", "bundled:mono", "bundled:display"];
        let current_label = match font_family.as_str() {
            "bundled:sans" => "Sans (Inter)",
            "bundled:serif" => "Serif (Noto Serif)",
            "bundled:mono" => "Mono (JetBrains Mono)",
            "bundled:display" => "Display",
            other => other,
        };
        egui::ComboBox::from_id_salt("text_font_family")
            .selected_text(current_label)
            .show_ui(ui, |ui| {
                for opt in &font_options {
                    let label = match *opt {
                        "bundled:sans" => "Sans (Inter)",
                        "bundled:serif" => "Serif (Noto Serif)",
                        "bundled:mono" => "Mono (JetBrains Mono)",
                        "bundled:display" => "Display",
                        _ => opt,
                    };
                    if ui.selectable_label(font_family == opt, label).clicked() {
                        *font_family = opt.to_string();
                        changed = true;
                    }
                }
            });

        // Font size
        ui.label("Size");
        if ui.add(egui::Slider::new(font_size, 8.0..=200.0).suffix(" pt")).changed() {
            changed = true;
        }

        // Bold / Italic
        ui.horizontal(|ui| {
            if ui.selectable_label(*bold, "B").clicked() {
                *bold = !*bold;
                changed = true;
            }
            if ui.selectable_label(*italic, "I").clicked() {
                *italic = !*italic;
                changed = true;
            }
        });

        // Text color
        ui.label("Text Color");
        if ui.color_edit_button_rgba_unmultiplied(font_color).changed() {
            changed = true;
        }

        // Background color
        ui.label("Background");
        if ui.color_edit_button_rgba_unmultiplied(background_color).changed() {
            changed = true;
        }

        // Alignment
        ui.label("Alignment");
        ui.horizontal(|ui| {
            use crate::scene::TextAlignment;
            if ui.selectable_label(*alignment == TextAlignment::Left, "Left").clicked() {
                *alignment = TextAlignment::Left;
                changed = true;
            }
            if ui.selectable_label(*alignment == TextAlignment::Center, "Center").clicked() {
                *alignment = TextAlignment::Center;
                changed = true;
            }
            if ui.selectable_label(*alignment == TextAlignment::Right, "Right").clicked() {
                *alignment = TextAlignment::Right;
                changed = true;
            }
        });

        // Outline
        let mut has_outline = outline.is_some();
        if ui.checkbox(&mut has_outline, "Outline").changed() {
            if has_outline {
                *outline = Some(crate::scene::TextOutline {
                    color: [0.0, 0.0, 0.0, 1.0],
                    width: 2.0,
                });
            } else {
                *outline = None;
            }
            changed = true;
        }
        if let Some(ref mut o) = outline {
            ui.horizontal(|ui| {
                ui.label("Color");
                if ui.color_edit_button_rgba_unmultiplied(&mut o.color).changed() {
                    changed = true;
                }
                ui.label("Width");
                if ui.add(egui::Slider::new(&mut o.width, 0.5..=10.0)).changed() {
                    changed = true;
                }
            });
        }

        // Padding
        ui.label("Padding");
        if ui.add(egui::Slider::new(padding, 0.0..=100.0).suffix(" px")).changed() {
            changed = true;
        }

        // Wrap width
        let mut has_wrap = wrap_width.is_some();
        if ui.checkbox(&mut has_wrap, "Word Wrap").changed() {
            if has_wrap {
                *wrap_width = Some(400.0);
            } else {
                *wrap_width = None;
            }
            changed = true;
        }
        if let Some(ref mut w) = wrap_width {
            if ui.add(egui::Slider::new(w, 50.0..=1920.0).suffix(" px")).changed() {
                changed = true;
            }
        }
    }

    // Re-render text frame on any change
    if changed {
        let props = state.library[lib_idx].properties.clone();
        if let Some(frame) = crate::text_source::render_text_source(&props) {
            state.library[lib_idx].native_size = (frame.width as f32, frame.height as f32);
            if let Some(ref tx) = state.command_tx {
                let _ = tx.try_send(crate::gstreamer::commands::GstCommand::LoadImageFrame {
                    source_id: selected_id,
                    frame,
                });
            }
        }
    }
}
```

- [ ] **Step 2: Add Color source properties UI**

Add a match arm for `SourceType::Color`:

```rust
SourceType::Color => {
    if let SourceProperties::Color { fill } = &mut state.library[lib_idx].properties {
        use crate::scene::{ColorFill, GradientStop};

        // Fill type selector
        ui.label("Fill Type");
        ui.horizontal(|ui| {
            if ui.selectable_label(matches!(fill, ColorFill::Solid { .. }), "Solid").clicked() {
                *fill = ColorFill::Solid { color: [1.0, 1.0, 1.0, 1.0] };
                changed = true;
            }
            if ui.selectable_label(matches!(fill, ColorFill::LinearGradient { .. }), "Linear").clicked() {
                *fill = ColorFill::LinearGradient {
                    angle: 0.0,
                    stops: vec![
                        GradientStop { position: 0.0, color: [0.0, 0.0, 0.0, 1.0] },
                        GradientStop { position: 1.0, color: [1.0, 1.0, 1.0, 1.0] },
                    ],
                };
                changed = true;
            }
            if ui.selectable_label(matches!(fill, ColorFill::RadialGradient { .. }), "Radial").clicked() {
                *fill = ColorFill::RadialGradient {
                    center: (0.5, 0.5),
                    radius: 0.5,
                    stops: vec![
                        GradientStop { position: 0.0, color: [1.0, 1.0, 1.0, 1.0] },
                        GradientStop { position: 1.0, color: [0.0, 0.0, 0.0, 1.0] },
                    ],
                };
                changed = true;
            }
        });

        match fill {
            ColorFill::Solid { color } => {
                ui.label("Color");
                if ui.color_edit_button_rgba_unmultiplied(color).changed() {
                    changed = true;
                }
            }
            ColorFill::LinearGradient { angle, stops } => {
                ui.label("Angle");
                if ui.add(egui::Slider::new(angle, 0.0..=360.0).suffix("°")).changed() {
                    changed = true;
                }
                changed |= draw_gradient_stops(ui, stops);
            }
            ColorFill::RadialGradient { center, radius, stops } => {
                ui.label("Center X");
                if ui.add(egui::Slider::new(&mut center.0, 0.0..=1.0)).changed() {
                    changed = true;
                }
                ui.label("Center Y");
                if ui.add(egui::Slider::new(&mut center.1, 0.0..=1.0)).changed() {
                    changed = true;
                }
                ui.label("Radius");
                if ui.add(egui::Slider::new(radius, 0.01..=2.0)).changed() {
                    changed = true;
                }
                changed |= draw_gradient_stops(ui, stops);
            }
        }
    }

    // Re-render color frame on any change
    if changed {
        if let SourceProperties::Color { fill } = &state.library[lib_idx].properties {
            let transform = &state.library[lib_idx].transform;
            let frame = crate::color_source::render_color_source(
                fill,
                transform.width as u32,
                transform.height as u32,
            );
            state.library[lib_idx].native_size = (frame.width as f32, frame.height as f32);
            if let Some(ref tx) = state.command_tx {
                let _ = tx.try_send(crate::gstreamer::commands::GstCommand::LoadImageFrame {
                    source_id: selected_id,
                    frame,
                });
            }
        }
    }
}
```

- [ ] **Step 3: Add gradient stop editor helper**

Add a helper function in the same file:

```rust
/// Draw gradient stop editor UI. Returns true if any stop changed.
fn draw_gradient_stops(ui: &mut egui::Ui, stops: &mut Vec<crate::scene::GradientStop>) -> bool {
    let mut changed = false;
    ui.label("Gradient Stops");

    let mut remove_idx = None;
    for (i, stop) in stops.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            ui.label(format!("Stop {}", i + 1));
            if ui.color_edit_button_rgba_unmultiplied(&mut stop.color).changed() {
                changed = true;
            }
            if ui.add(egui::Slider::new(&mut stop.position, 0.0..=1.0)).changed() {
                changed = true;
            }
            if stops.len() > 2 && ui.small_button("×").clicked() {
                remove_idx = Some(i);
            }
        });
    }

    if let Some(idx) = remove_idx {
        stops.remove(idx);
        changed = true;
    }

    if ui.button("+ Add Stop").clicked() {
        stops.push(crate::scene::GradientStop {
            position: 0.5,
            color: [0.5, 0.5, 0.5, 1.0],
        });
        changed = true;
    }

    changed
}
```

- [ ] **Step 4: Add Audio source properties UI**

Add a match arm for `SourceType::Audio`:

```rust
SourceType::Audio => {
    if let SourceProperties::Audio { input } = &mut state.library[lib_idx].properties {
        use crate::scene::AudioInput;

        // Input type selector
        let is_device = matches!(input, AudioInput::Device { .. });
        ui.label("Input Type");
        ui.horizontal(|ui| {
            if ui.selectable_label(is_device, "Device").clicked() && !is_device {
                *input = AudioInput::Device {
                    device_uid: String::new(),
                    device_name: String::new(),
                };
                changed = true;
            }
            if ui.selectable_label(!is_device, "File").clicked() && is_device {
                *input = AudioInput::File {
                    path: String::new(),
                    looping: false,
                };
                changed = true;
            }
        });

        match input {
            AudioInput::Device { device_uid, device_name } => {
                ui.label("Audio Device");
                // Enumerate audio devices
                if let Ok(devices) = crate::gstreamer::devices::enumerate_audio_input_devices() {
                    let current_label = if device_name.is_empty() {
                        "Select device..."
                    } else {
                        device_name.as_str()
                    };
                    egui::ComboBox::from_id_salt("audio_device")
                        .selected_text(current_label)
                        .show_ui(ui, |ui| {
                            for device in &devices {
                                if ui.selectable_label(
                                    *device_uid == device.uid,
                                    &device.name,
                                ).clicked() {
                                    *device_uid = device.uid.clone();
                                    *device_name = device.name.clone();
                                    changed = true;
                                }
                            }
                        });
                }
            }
            AudioInput::File { path, looping } => {
                ui.label("Audio File");
                ui.horizontal(|ui| {
                    let mut path_text = path.clone();
                    if ui.text_edit_singleline(&mut path_text).changed() {
                        *path = path_text;
                        changed = true;
                    }
                    if ui.button("Browse").clicked() {
                        if let Some(file) = rfd::FileDialog::new()
                            .add_filter("Audio", &["mp3", "wav", "ogg", "flac"])
                            .pick_file()
                        {
                            *path = file.to_string_lossy().to_string();
                            changed = true;
                        }
                    }
                });
                if ui.checkbox(looping, "Loop").changed() {
                    changed = true;
                }
            }
        }

        // Volume (use existing library source fields)
        ui.label("Volume");
        let vol = &mut state.library[lib_idx].volume;
        if ui.add(egui::Slider::new(vol, 0.0..=2.0).suffix("×")).changed() {
            if let Some(ref tx) = state.command_tx {
                let _ = tx.try_send(crate::gstreamer::commands::GstCommand::SetSourceVolume {
                    source_id: selected_id,
                    volume: *vol,
                });
            }
        }

        let muted = &mut state.library[lib_idx].muted;
        if ui.checkbox(muted, "Mute").changed() {
            if let Some(ref tx) = state.command_tx {
                let _ = tx.try_send(crate::gstreamer::commands::GstCommand::SetSourceMuted {
                    source_id: selected_id,
                    muted: *muted,
                });
            }
        }
    }
}
```

- [ ] **Step 5: Add Browser source properties UI (stubbed)**

Add a match arm for `SourceType::Browser`:

```rust
SourceType::Browser => {
    if let SourceProperties::Browser { url, width, height } = &mut state.library[lib_idx].properties {
        ui.label("URL");
        let mut url_text = url.clone();
        if ui.text_edit_singleline(&mut url_text).changed() {
            *url = url_text;
            changed = true;
        }

        ui.horizontal(|ui| {
            ui.label("Width");
            let mut w = *width as i32;
            if ui.add(egui::DragValue::new(&mut w).range(100..=3840)).changed() {
                *width = w as u32;
                changed = true;
            }
            ui.label("Height");
            let mut h = *height as i32;
            if ui.add(egui::DragValue::new(&mut h).range(100..=2160)).changed() {
                *height = h as u32;
                changed = true;
            }
        });

        ui.add_space(8.0);
        ui.colored_label(
            egui::Color32::from_rgb(150, 150, 150),
            "Browser rendering engine not yet available.\nSource will display a placeholder on canvas.",
        );
    }

    // Generate placeholder frame on change
    if changed {
        if let SourceProperties::Browser { width, height, .. } = &state.library[lib_idx].properties {
            let w = *width;
            let h = *height;
            let frame = generate_browser_placeholder(w, h);
            state.library[lib_idx].native_size = (w as f32, h as f32);
            if let Some(ref tx) = state.command_tx {
                let _ = tx.try_send(crate::gstreamer::commands::GstCommand::LoadImageFrame {
                    source_id: selected_id,
                    frame,
                });
            }
        }
    }
}
```

- [ ] **Step 6: Add browser placeholder generator**

Add a helper function:

```rust
/// Generate a dark placeholder frame for browser sources.
fn generate_browser_placeholder(width: u32, height: u32) -> crate::gstreamer::types::RgbaFrame {
    let w = width.max(1) as usize;
    let h = height.max(1) as usize;
    // Dark background (#1a1a2e)
    let mut data = vec![0u8; w * h * 4];
    for chunk in data.chunks_exact_mut(4) {
        chunk.copy_from_slice(&[0x1a, 0x1a, 0x2e, 0xff]);
    }
    crate::gstreamer::types::RgbaFrame {
        data,
        width: w as u32,
        height: h as u32,
    }
}
```

- [ ] **Step 7: Add audio level indicator in source list**

In `src/ui/sources_panel.rs`, in the source list row rendering, add a small speaker icon or level indicator for Audio sources. Since audio sources have no canvas presence, this gives the user visual feedback that the source is active. Check if the source type is `SourceType::Audio` and render a small `SPEAKER_HIGH` icon in the row.

- [ ] **Step 8: Build and verify full compilation**

Run: `cargo build 2>&1`
Expected: Clean compilation with no errors.

- [ ] **Step 9: Commit**

```bash
git add src/ui/properties_panel.rs src/ui/sources_panel.rs
git commit -m "feat: add properties panels for text, color, audio, and browser sources"
```

---

## Task 9: Initial Frame Push on Source Creation

**Files:**
- Modify: `src/ui/library_panel.rs`

When a Text, Color, or Browser source is created, it needs an initial frame pushed into the frame map so it appears on canvas immediately.

- [ ] **Step 1: Add initial frame push after source creation**

In `add_library_source`, after the new `LibrarySource` is pushed to `state.library`, add frame generation for the visual synthetic types. Find the section after the library push and before the function ends:

```rust
    // Push initial frame for synthetic visual sources
    match &state.library.last().unwrap().properties {
        SourceProperties::Text { .. } => {
            let props = state.library.last().unwrap().properties.clone();
            if let Some(frame) = crate::text_source::render_text_source(&props) {
                let source = state.library.last_mut().unwrap();
                source.native_size = (frame.width as f32, frame.height as f32);
                source.transform.width = frame.width as f32;
                source.transform.height = frame.height as f32;
                if let Some(ref tx) = state.command_tx {
                    let _ = tx.try_send(GstCommand::LoadImageFrame {
                        source_id: new_id,
                        frame,
                    });
                }
            }
        }
        SourceProperties::Color { fill } => {
            let transform = &state.library.last().unwrap().transform;
            let frame = crate::color_source::render_color_source(
                fill,
                transform.width as u32,
                transform.height as u32,
            );
            if let Some(ref tx) = state.command_tx {
                let _ = tx.try_send(GstCommand::LoadImageFrame {
                    source_id: new_id,
                    frame,
                });
            }
        }
        SourceProperties::Browser { width, height, .. } => {
            let frame = crate::ui::properties_panel::generate_browser_placeholder(*width, *height);
            let source = state.library.last_mut().unwrap();
            source.native_size = (*width as f32, *height as f32);
            if let Some(ref tx) = state.command_tx {
                let _ = tx.try_send(GstCommand::LoadImageFrame {
                    source_id: new_id,
                    frame,
                });
            }
        }
        _ => {}
    }
```

Note: `generate_browser_placeholder` needs to be made `pub` in `properties_panel.rs`.

- [ ] **Step 2: Make generate_browser_placeholder public**

In `src/ui/properties_panel.rs`, change:
```rust
fn generate_browser_placeholder(...)
```
to:
```rust
pub fn generate_browser_placeholder(...)
```

- [ ] **Step 3: Initialize text font system on startup**

In `src/main.rs`, in the main function, add before the event loop starts:

```rust
text_source::init_font_system();
```

- [ ] **Step 4: Build and test**

Run: `cargo build 2>&1`
Expected: Clean compilation.

- [ ] **Step 5: Run the app and manually test**

Run: `cargo run`
Test:
1. Click "+" in library panel → add Text source → verify it appears with "Text" label
2. Click "+" → add Color source → verify solid white rectangle appears
3. Click "+" → add Browser source → verify dark placeholder appears
4. Click "+" → add Audio source → verify it appears in library (no canvas visual)
5. Select Text source → modify text content → verify canvas updates
6. Select Color source → change to gradient → verify canvas updates

- [ ] **Step 6: Commit**

```bash
git add src/ui/library_panel.rs src/ui/properties_panel.rs src/main.rs
git commit -m "feat: push initial frames on source creation and init font system"
```

---

## Task 10: Final Verification and Cleanup

- [ ] **Step 1: Run full test suite**

Run: `cargo test 2>&1`
Expected: All tests pass.

- [ ] **Step 2: Run clippy**

Run: `cargo clippy 2>&1`
Expected: No errors. Fix any warnings.

- [ ] **Step 3: Run formatter**

Run: `cargo fmt --check`
Expected: No formatting issues. If any, run `cargo fmt` to fix.

- [ ] **Step 4: Commit any cleanup**

```bash
git add -A
git commit -m "style: fix clippy warnings and formatting for new source types"
```
