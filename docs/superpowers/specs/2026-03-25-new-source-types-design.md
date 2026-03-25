# New Source Types: Text, Color, Audio, Browser

Adds four new source types to Lodestone's global source library: Text (styled labels), Color (solid/gradient fills), Audio (device/file input), and Browser (stubbed with placeholder).

## Source Types Overview

| Source | Visual on canvas | Capture pipeline | Render strategy |
|--------|-----------------|------------------|-----------------|
| Text | Yes | No — CPU render to RgbaFrame | `cosmic-text` rasterization on property change |
| Color | Yes | No — CPU render to RgbaFrame | Pixel fill/gradient on property change |
| Audio | No | Yes — GStreamer audio pipeline | No frame; audio mixed into output |
| Browser | Yes (placeholder) | No | Static placeholder frame; engine deferred |

All visual sources push frames into the existing `HashMap<SourceId, RgbaFrame>` via `GstCommand::LoadImageFrame`. No compositor changes required.

### Serialization Compatibility

Adding new `SourceProperties` variants changes the TOML deserialization contract. Since `Audio` and `Browser` exist in `SourceType` but not in `SourceProperties` today, any existing saved project files with those types use `SourceProperties::default()` (Display). To handle this gracefully:

- All new `SourceProperties` variant fields use `#[serde(default)]` so partial/missing fields deserialize without errors.
- The `SourceProperties::default()` fallback remains `Display { screen_index: 0 }` — existing saved files continue to work.
- On load, if `source_type` doesn't match its `properties` variant (e.g., `Audio` type with `Display` properties), the loader migrates it to the correct default properties for that type.

## Data Model

### SourceType Enum

Add `Text` and `Color` variants to the existing enum. `Audio` and `Browser` already exist:

```rust
pub enum SourceType {
    Display,
    Window,
    Camera,
    Audio,    // existing stub
    Image,
    Browser,  // existing stub
    Text,     // new
    Color,    // new
}
```

### SourceProperties Variants

Four new variants on the existing `SourceProperties` enum:

```rust
pub enum SourceProperties {
    // existing: Display, Window, Camera, Image

    Text {
        content: String,
        font_family: String,        // "bundled:sans", "system:Helvetica Neue"
        font_size: f32,             // points
        font_color: [f32; 4],       // RGBA
        background_color: [f32; 4], // RGBA, [0,0,0,0] = transparent
        bold: bool,
        italic: bool,
        alignment: TextAlignment,
        outline: Option<TextOutline>,
        padding: f32,               // uniform padding in pixels
        wrap_width: Option<f32>,    // None = single line
    },
    Color {
        fill: ColorFill,
    },
    Audio {
        input: AudioInput,
    },
    Browser {
        url: String,
        width: u32,
        height: u32,
    },
}
```

### Supporting Types

```rust
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum TextAlignment {
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct TextOutline {
    pub color: [f32; 4],
    pub width: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ColorFill {
    Solid { color: [f32; 4] },
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct GradientStop {
    pub position: f32, // 0.0..1.0
    pub color: [f32; 4],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AudioInput {
    Device {
        device_uid: String,
        device_name: String,
    },
    File {
        path: String,
        looping: bool,
    },
}
```

### CaptureSourceConfig

One new variant for audio (text, color, and browser don't use capture pipelines):

```rust
pub enum CaptureSourceConfig {
    // existing: Screen, Window, Camera
    AudioDevice { device_uid: String },
    AudioFile { path: String, looping: bool },
}
```

### GstCommand Additions

```rust
pub enum GstCommand {
    // existing commands...

    /// Per-source volume control (distinct from global SetAudioVolume).
    SetSourceVolume { source_id: SourceId, volume: f32 },
    /// Per-source mute (distinct from global SetAudioMuted).
    SetSourceMuted { source_id: SourceId, muted: bool },
}
```

## Text Source Rendering

Module: `src/text_source.rs` (parallel to existing `src/image_source.rs`).

### Font Loading

On startup, initialize a shared `fontdb::Database`:
- Bundled fonts via `include_bytes!`: sans (Inter), serif (Noto Serif), mono (JetBrains Mono), display (one bold/display face). Stored in a `fonts/` resources directory.
- System fonts via `fontdb::Database::load_system_fonts()`.
- The `font_family` field uses a prefix convention: `"bundled:sans"`, `"bundled:serif"`, `"bundled:mono"`, `"bundled:display"`, `"system:Helvetica Neue"`.

### Render Pipeline

Called when a text source is added to a scene or any text property changes:

1. Reuse the long-lived `cosmic_text::FontSystem` (created once at startup, not per-render — `FontSystem` creation is expensive). The `text_source` module maintains its own `FontSystem` instance initialized from the shared `fontdb::Database`.
2. Create a `cosmic_text::Buffer`, set metrics (font size, line height).
3. Set font family, weight (bold → `Weight::BOLD`), style (italic → `Style::Italic`).
4. Set text content and wrap width (`f32::INFINITY` for single-line).
5. Shape and layout the buffer.
6. Compute bounding box from `Buffer::layout_runs()`.
7. Canvas size = bounding box + padding (all sides) + outline width.
8. Allocate RGBA pixel buffer at canvas size.
9. Fill with `background_color`.
10. If outline is set: rasterize glyphs at offsets around each glyph position (8-direction or circular sampling) using `cosmic_text::SwashCache`, using `outline.color`.
11. Rasterize foreground glyphs using `SwashCache` with `font_color`.
12. Apply `alignment` by adjusting x-offset of each layout run.
13. Pack into `RgbaFrame`, send via `GstCommand::LoadImageFrame`.
14. Update `LibrarySource.native_size` to match rendered dimensions.

### Frame Lifecycle

Text/Color/Browser frames persist in the `latest_frames` HashMap across scene switches — `RemoveCaptureSource` only tears down GStreamer pipelines, it does not clear frame map entries for non-capture sources. Frames are rendered and pushed:
- When the source is first created (via the "+" menu).
- When any property changes (re-render and push updated frame).
- Frames do NOT need to be re-pushed on scene activation since they persist in the map.

This matches the existing image source behavior.

### Re-render Triggers

Any change to `SourceProperties::Text` fields triggers a full re-render and frame push.

## Color Source Rendering

Module: `src/color_source.rs`.

### Solid Color

Render at the source's transform dimensions (width x height from `Transform`). Using a small buffer and relying on upscaling would require verifying the compositor's texture sampling behavior — rendering at full size is simpler and guaranteed correct. For solid fills the cost is trivial. Re-render only when the color or dimensions change.

### Linear Gradient

Render at a resolution matching the source's transform dimensions, capped at 1920x1080:

1. Compute direction vector from `angle`: `(cos(angle), sin(angle))`.
2. For each pixel `(x, y)`, normalize to `[0,1]` range.
3. Project onto gradient axis: `t = dot(normalized_pos, direction)`.
4. Clamp `t` to `[0,1]`, interpolate between surrounding `GradientStop` colors in sRGB space.
5. Write RGBA pixel.

### Radial Gradient

Same approach, but `t = distance(pixel, center) / radius`, clamped to `[0,1]`.

### Re-render Triggers

Any change to `ColorFill` properties or source dimensions (since gradient pixel density depends on size).

## Audio Source Pipeline

Audio sources produce no frames. They feed audio into the recording/streaming output.

### Device Input Pipeline

GStreamer pipeline: `autoaudiosrc` (with device UID property) → `audioconvert` → `audioresample` → `volume` → `appsink`.

- The `volume` element is controlled by `SetSourceVolume` commands.
- Device UID is set via element properties (platform-specific: `device` property on `osxaudiosrc`).

### File Input Pipeline

GStreamer pipeline: `uridecodebin` (file URI) → `audioconvert` → `audioresample` → `volume` → `appsink`.

- On EOS, if `looping` is true, seek the pipeline back to the start.
- Supports mp3, wav, ogg, flac — anything GStreamer can decode.

### Audio Mixing

Per-source audio output from `appsink` is connected to an `audiomixer` element that combines all active audio sources. The mixer output feeds into the encoding pipeline for recording/streaming.

### Lifecycle

- `AddCaptureSource` with `AudioDevice`/`AudioFile` config starts the audio pipeline.
- `RemoveCaptureSource` tears it down.
- `apply_scene_diff()` (in `scenes_panel.rs`) handles start/stop on scene switching. Audio sources get explicit match arms to start/stop audio pipelines. Text, Color, and Browser get no-op match arms (their frames persist in the map without capture pipelines).
- Muting sets the `volume` element to 0.0 (cheaper than pipeline teardown).

### No Frame Map Entry

Audio sources do not insert into `latest_frames`. The compositor skips them. They have no canvas bounding box and no transform handles.

## Browser Source (Stubbed)

### Data Model

`SourceType::Browser` with `SourceProperties::Browser { url, width, height }`.
Defaults: `url: ""`, `width: 1920`, `height: 1080`.

### Placeholder Frame

When a browser source is added to a scene, generate a static `RgbaFrame` at the configured width x height:
- Dark background (#1a1a2e or similar).
- Centered globe icon and text: "Browser Source — coming soon".
- Pushed via `GstCommand::LoadImageFrame`.
- Re-generated when width/height change.

No GStreamer pipeline. No `CaptureSourceConfig` variant.

### Future Hook

When a browser rendering engine is integrated, it replaces the placeholder generation. The rest of the infrastructure (frame map, compositor, properties panel, scene diffing) is already wired up.

## UI Changes

### Library Panel Add Menu

Add new types to the "+" popup in `library_panel.rs`, with a separator:

```
Display, Window, Camera, Image       (capture sources)
────────────────────────────────
Text, Color, Audio, Browser           (synthetic sources)
```

### Source Icons

`source_icon()` mappings using `egui_phosphor`:

| Source | Icon |
|--------|------|
| Text | `TEXT_T` |
| Color | `PALETTE` |
| Audio | `SPEAKER_HIGH` |
| Browser | `GLOBE` |

Note: `source_icon()` takes `&SourceType` (not properties), so audio uses a single icon regardless of input type.

### Properties Panel

Each new source type gets a properties section when selected:

**Text:** Multiline text input, font family dropdown (bundled section + separator + system fonts), font size slider, text/background color pickers, bold/italic toggles, alignment segmented buttons (L/C/R), outline toggle with color picker + width slider, padding slider, word wrap toggle + width slider.

**Color:** Fill type segmented control (Solid / Linear / Radial). Solid: color picker. Linear: angle slider + gradient stop editor. Radial: center position + radius + gradient stop editor. Gradient stop editor: visual bar with draggable stops, each with a color picker + position field. Add/remove stop buttons.

**Audio:** Input type segmented control (Device / File). Device mode: dropdown of enumerated audio input devices. File mode: file path input with browse button, loop toggle checkbox. Both modes: volume slider, mute button (backed by existing `LibrarySource.volume`/`muted` fields). Live level meter bar.

**Browser:** URL text input, width/height number inputs. Info box indicating engine is not yet available.

### Canvas Behavior

- Text, Color, Browser sources render on canvas (frames in frame map), with full transform handle support.
- Audio sources are invisible on canvas — no bounding box, no transform handles, not composited.
- Audio sources show a small level indicator in the source list row for visual feedback.

### Default Values for New Sources

When created via the "+" menu:

| Source | Defaults |
|--------|----------|
| Text | content: "Text", font: "bundled:sans", size: 48pt, color: white, bg: transparent, no outline, no wrap |
| Color | Solid white (#FFFFFF) |
| Audio | Device input, first available device, volume: 1.0, unmuted |
| Browser | url: "", 1920x1080 |

## File Structure

New files:
- `src/text_source.rs` — text rendering to RgbaFrame
- `src/color_source.rs` — solid/gradient rendering to RgbaFrame
- `fonts/` — bundled font files (Inter, Noto Serif, JetBrains Mono, display face)

Modified files:
- `src/scene.rs` — new SourceProperties variants, supporting types
- `src/gstreamer/commands.rs` — new CaptureSourceConfig variants, new GstCommand variants
- `src/gstreamer/thread.rs` — audio capture pipeline handling, audio mixer integration
- `src/ui/library_panel.rs` — add menu entries, icon mappings, source creation defaults
- `src/ui/properties_panel.rs` — properties UI for all four types
- `src/ui/sources_panel.rs` — start_capture_from_properties for new types, audio level indicator
- `src/ui/scenes_panel.rs` — apply_scene_diff match arms for new source types
- `src/ui/draw_helpers.rs` — source_icon() mappings for new types
