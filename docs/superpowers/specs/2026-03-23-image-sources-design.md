# Image Sources Design Spec

Add static image files (PNG, JPEG, etc.) as a source type in the compositor.

## Design Decisions

- **No GStreamer pipeline.** Image sources are a single static frame pushed directly into the compositor's shared frame map (`Arc<Mutex<HashMap<SourceId, RgbaFrame>>>`). The compositor already handles per-source textures from raw RGBA data — no special rendering path needed.
- **Manual reload.** Images are loaded once on add/path change. A "Reload" button in Properties re-reads the file. No file watching, no polling, no new dependencies for change detection.
- **Native file dialog.** The `rfd` crate provides a cross-platform file picker for selecting image files.

## Data Model

### SourceProperties (src/scene.rs)

Add one new variant:

```rust
SourceProperties::Image { path: String }
```

The path is an absolute file path to the image on disk.

### CaptureSourceConfig

No new variant. Image sources bypass the GStreamer capture pipeline entirely. They don't use `AddCaptureSource` or `RemoveCaptureSource` commands.

## Loading Flow

1. User adds an Image source via the "+" type picker or sets a file path in Properties.
2. `load_image_source(path: &str) -> Result<RgbaFrame>` decodes the file:
   - Opens the file, decodes via the `image` crate
   - Converts to RGBA8 pixel buffer
   - Returns `RgbaFrame { data: Vec<u8>, width: u32, height: u32 }`
3. The frame is inserted into the shared `latest_frames` map (same `Arc<Mutex<HashMap<SourceId, RgbaFrame>>>` that capture pipelines write to).
4. The compositor picks it up on the next render cycle — no special code path.

### Where Loading Happens

Image loading is triggered from the UI (Properties panel) but the frame map write needs access to `GstChannels.latest_frames`. Two options:

**Chosen approach:** Add a `LoadImageFrame` command to `GstCommand` so image frames flow through the same channel as capture frames. This preserves the architecture rule that the UI never writes directly to the frame map — all frame data goes through GStreamer channels.

```rust
GstCommand::LoadImageFrame { source_id: SourceId, frame: RgbaFrame }
```

The UI decodes the image, then sends this command. The GStreamer thread inserts the frame into `latest_frames`. This keeps a single write path to the frame map.

## Image Loading Function

```rust
/// Load an image file and convert to an RgbaFrame.
pub fn load_image_source(path: &str) -> anyhow::Result<RgbaFrame> {
    let img = image::open(path)
        .with_context(|| format!("Failed to open image: {path}"))?
        .into_rgba8();
    let width = img.width();
    let height = img.height();
    let data = img.into_raw();
    Ok(RgbaFrame { data, width, height })
}
```

This function lives in a new module `src/image_source.rs` to keep it isolated.

## UI Changes

### Sources Panel (src/ui/sources_panel.rs)

Add "Image" to the source type picker popup (alongside Display, Window, Camera).

`add_image_source()`:
- Creates a Source with `SourceType::Image`, `SourceProperties::Image { path: String::new() }`
- Does NOT load anything yet — the user picks a file in Properties
- Sets `selected_source_id` so Properties panel opens immediately

### Properties Panel (src/ui/properties_panel.rs)

For `SourceProperties::Image`:

- **PATH section:**
  - Text input showing the current file path (read-only or editable)
  - "Browse" button: opens `rfd::FileDialog` for image files (png, jpg, jpeg, bmp, gif, webp, tiff)
  - "Reload" button: re-reads the current path and pushes updated frame

When a path is set or changed:
1. Call `load_image_source(&path)`
2. On success: send `GstCommand::LoadImageFrame { source_id, frame }` via `state.command_tx`, update `source.properties`, set transform to image's native dimensions
3. On error: push error to `state.active_errors`, don't update path

## Dependencies

### New Crates

- `image` — image decoding (PNG, JPEG, BMP, GIF, WebP, TIFF)
- `rfd` — native file dialog (cross-platform)

## Error Handling

| Scenario | Behavior |
|----------|----------|
| File not found | Error in `state.active_errors`, source stays with no frame |
| Unsupported format | Same — `image` crate returns error |
| File too large | Let `image` crate handle it (will OOM on truly massive files, but that's user error) |
| Path empty | No-op, show "Select an image..." placeholder text |
| Reload fails | Error in `state.active_errors`, keep previous frame |

## Integration Notes

### Exhaustive match arms

Adding `SourceProperties::Image` will break exhaustive matches in:
- `src/main.rs` — the startup loop that maps `SourceProperties` to `CaptureSourceConfig`. Image sources should be skipped (no capture pipeline). Add an arm that does nothing.
- `src/ui/scenes_panel.rs` — same pattern. Image sources don't trigger capture commands on scene switch.
- Any other `match source.properties` sites.

### Source removal

`remove_source()` in `sources_panel.rs` unconditionally sends `GstCommand::RemoveCaptureSource`. Image sources never registered a capture, so this is a no-op on the GStreamer side (it won't find the source_id in the captures map). This is harmless but the implementer should add a guard: only send `RemoveCaptureSource` for source types that use capture pipelines (Display, Window, Camera). For Image sources, send `LoadImageFrame` with a zero-size frame or simply skip the command.

### Transform defaults

When an image is loaded, set the source's transform width/height to the image's native pixel dimensions rather than the default 1920x1080. This ensures the image renders at its correct aspect ratio.

## File Structure

```
src/image_source.rs           # NEW — load_image_source() function
src/scene.rs                  # MODIFY — SourceProperties::Image variant
src/gstreamer/commands.rs     # MODIFY — GstCommand::LoadImageFrame variant
src/gstreamer/thread.rs       # MODIFY — handle LoadImageFrame command
src/ui/sources_panel.rs       # MODIFY — Image in type picker, guard remove
src/ui/properties_panel.rs    # MODIFY — path input, browse, reload
src/main.rs                   # MODIFY — skip Image in capture startup loop
Cargo.toml                    # MODIFY — add image, rfd dependencies
```
