# Video Settings: FPS & Color Space Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire the video FPS setting into capture pipelines (replacing hardcoded 30fps) and wire color space into encode pipeline caps as GStreamer colorimetry metadata.

**Architecture:** Add `fps: u32` to the `GstCommand::AddCaptureSource` variant so the GStreamer thread receives the configured FPS from settings. Add `color_space: String` to `EncoderConfig` so encode pipelines tag output with correct colorimetry. Update all call sites, remove "not yet implemented" UI labels.

**Tech Stack:** Rust, GStreamer (gstreamer-rs bindings), egui

---

## File Map

| File | Action | Responsibility |
|------|--------|----------------|
| `src/gstreamer/commands.rs` | Modify | Add `fps` to `AddCaptureSource`, `StartVirtualCamera`; add `color_space` to `EncoderConfig` |
| `src/gstreamer/thread.rs` | Modify | Use command fps instead of hardcoded `30u32`; pass fps through internal methods |
| `src/gstreamer/encode.rs` | Modify | Apply colorimetry to appsrc caps based on `EncoderConfig.color_space` |
| `src/ui/toolbar.rs` | Modify | Pass `color_space` when building `EncoderConfig` |
| `src/main.rs` | Modify | Pass `video.fps` in `AddCaptureSource` commands |
| `src/ui/scenes_panel.rs` | Modify | Pass `video.fps` in `AddCaptureSource` commands |
| `src/ui/preview_panel.rs` | Modify | Pass `video.fps` in `AddCaptureSource` commands |
| `src/ui/properties_panel.rs` | Modify | Pass `video.fps` in `AddCaptureSource` commands |
| `src/ui/sources_panel.rs` | Modify | Pass `video.fps` in `AddCaptureSource` commands |
| `src/ui/settings/video.rs` | Modify | Remove "not yet implemented" labels |

---

### Task 1: Add `fps` to `GstCommand::AddCaptureSource` and `StartVirtualCamera`

**Files:**
- Modify: `src/gstreamer/commands.rs:38-94`

- [ ] **Step 1: Add `fps` field to `AddCaptureSource` variant**

In `src/gstreamer/commands.rs`, change:

```rust
    AddCaptureSource {
        source_id: SourceId,
        config: CaptureSourceConfig,
    },
```

to:

```rust
    AddCaptureSource {
        source_id: SourceId,
        config: CaptureSourceConfig,
        fps: u32,
    },
```

- [ ] **Step 2: Add `fps` field to `StartVirtualCamera` variant**

In the same file, change:

```rust
    StartVirtualCamera,
```

to:

```rust
    StartVirtualCamera { fps: u32 },
```

- [ ] **Step 3: Add `color_space` field to `EncoderConfig`**

In the same file, change the `EncoderConfig` struct:

```rust
pub struct EncoderConfig {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub bitrate_kbps: u32,
    pub encoder_type: EncoderType,
}
```

to:

```rust
pub struct EncoderConfig {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub bitrate_kbps: u32,
    pub encoder_type: EncoderType,
    pub color_space: String,
}
```

And update the `Default` impl:

```rust
impl Default for EncoderConfig {
    fn default() -> Self {
        Self {
            width: 1920,
            height: 1080,
            fps: 30,
            bitrate_kbps: 4500,
            encoder_type: EncoderType::H264VideoToolbox,
            color_space: "sRGB".to_string(),
        }
    }
}
```

---

### Task 2: Update GStreamer thread to use command FPS

**Files:**
- Modify: `src/gstreamer/thread.rs`

- [ ] **Step 1: Update command match arm for `AddCaptureSource`**

At line 660, change:

```rust
            GstCommand::AddCaptureSource { source_id, config } => {
                self.handle_add_capture_source(source_id, config)
            }
```

to:

```rust
            GstCommand::AddCaptureSource { source_id, config, fps } => {
                self.handle_add_capture_source(source_id, config, fps)
            }
```

- [ ] **Step 2: Update `handle_add_capture_source` signature and body**

At line 813, change:

```rust
    fn handle_add_capture_source(&mut self, source_id: SourceId, config: CaptureSourceConfig) {
        self.add_capture_source(source_id, &config);
    }
```

to:

```rust
    fn handle_add_capture_source(&mut self, source_id: SourceId, config: CaptureSourceConfig, fps: u32) {
        self.add_capture_source(source_id, &config, fps);
    }
```

- [ ] **Step 3: Update `add_capture_source` to accept and use fps**

At line 102, change the signature:

```rust
    fn add_capture_source(&mut self, source_id: SourceId, config: &CaptureSourceConfig) {
```

to:

```rust
    fn add_capture_source(&mut self, source_id: SourceId, config: &CaptureSourceConfig, fps: u32) {
```

Then update the three internal call sites within `add_capture_source`:

1. The display capture call at line 113:
```rust
            self.add_display_capture_source(source_id, *screen_index, *exclude_self, *capture_size, fps);
```

2. The window capture call at line 120:
```rust
            self.add_window_capture_source(source_id, mode.clone(), *capture_size, fps);
```

3. The fallback `build_capture_pipeline` call at line 140:
```rust
        match build_capture_pipeline(config, 1920, 1080, fps) {
```

- [ ] **Step 4: Update `add_display_capture_source` to accept fps parameter**

At line 325 area, change the signature from:

```rust
    fn add_display_capture_source(
        &mut self,
        source_id: SourceId,
        screen_index: u32,
        exclude_self: bool,
        capture_size: (u32, u32),
    ) {
```

to:

```rust
    fn add_display_capture_source(
        &mut self,
        source_id: SourceId,
        screen_index: u32,
        exclude_self: bool,
        capture_size: (u32, u32),
        fps: u32,
    ) {
```

And remove the hardcoded `let fps = 30u32;` at line 336 (it's now a parameter).

- [ ] **Step 5: Update `add_window_capture_source` to accept fps parameter**

At line 166 area, change:

```rust
    fn add_window_capture_source(
        &mut self,
        source_id: SourceId,
        mode: crate::scene::WindowCaptureMode,
        _capture_size: (u32, u32),
    ) {
```

to:

```rust
    fn add_window_capture_source(
        &mut self,
        source_id: SourceId,
        mode: crate::scene::WindowCaptureMode,
        _capture_size: (u32, u32),
        fps: u32,
    ) {
```

And store fps so `start_sck_window_capture` can use it. Update the call at line 187:

```rust
        self.start_sck_window_capture(source_id, window_id, width, height, fps);
```

- [ ] **Step 6: Update `start_sck_window_capture` to accept fps parameter**

At line 195 area, change:

```rust
    fn start_sck_window_capture(
        &mut self,
        source_id: SourceId,
        window_id: u32,
        width: u32,
        height: u32,
    ) {
```

to:

```rust
    fn start_sck_window_capture(
        &mut self,
        source_id: SourceId,
        window_id: u32,
        width: u32,
        height: u32,
        fps: u32,
    ) {
```

And remove the hardcoded `let fps = 30u32;` at line 208 (it's now a parameter).

- [ ] **Step 7: Update `StartVirtualCamera` match arm**

At line 672, change:

```rust
            GstCommand::StartVirtualCamera => self.handle_start_virtual_camera(),
```

to:

```rust
            GstCommand::StartVirtualCamera { fps } => self.handle_start_virtual_camera(fps),
```

- [ ] **Step 8: Update `handle_start_virtual_camera` to accept fps**

At line 837 area, change:

```rust
    fn handle_start_virtual_camera(&mut self) {
        use super::virtual_camera;
        let width = 1920u32;
        let height = 1080u32;
        let fps = 30u32;
```

to:

```rust
    fn handle_start_virtual_camera(&mut self, fps: u32) {
        use super::virtual_camera;
        let width = 1920u32;
        let height = 1080u32;
```

(Remove the hardcoded `let fps = 30u32;` — fps is now a parameter.)

- [ ] **Step 9: Check for any WindowWatcher re-capture calls that use hardcoded fps**

Search for any other `start_sck_window_capture` calls in `thread.rs` that may need the fps parameter threaded through. The WindowWatcher's `poll_changes` callback may re-capture windows — those calls also need fps. Store fps on `WatchedSource` or the `GstThread` struct if needed.

- [ ] **Step 10: Update the test at line 1313**

Change:

```rust
            .try_send(GstCommand::AddCaptureSource {
                source_id: SourceId(1),
                config: CaptureSourceConfig::Screen {
                    screen_index: 0,
                    exclude_self: false,
                    capture_size: (1920, 1080),
                },
            })
```

to:

```rust
            .try_send(GstCommand::AddCaptureSource {
                source_id: SourceId(1),
                config: CaptureSourceConfig::Screen {
                    screen_index: 0,
                    exclude_self: false,
                    capture_size: (1920, 1080),
                },
                fps: 30,
            })
```

---

### Task 3: Wire colorimetry into encode pipeline caps

**Files:**
- Modify: `src/gstreamer/encode.rs:25-32`

- [ ] **Step 1: Map color space string to GStreamer colorimetry**

Add a helper function in `encode.rs`:

```rust
/// Map a settings color-space name to a GStreamer colorimetry string.
fn colorimetry_for(color_space: &str) -> &'static str {
    match color_space {
        "Rec. 709" => "bt709",
        "Rec. 2100 (PQ)" => "bt2100-pq",
        // sRGB and any unknown value
        _ => "srgb",
    }
}
```

- [ ] **Step 2: Apply colorimetry in `make_appsrc_caps`**

Change the `make_appsrc_caps` function from:

```rust
fn make_appsrc_caps(config: &EncoderConfig) -> gstreamer::Caps {
    gstreamer_video::VideoCapsBuilder::new()
        .format(gstreamer_video::VideoFormat::Rgba)
        .width(config.width as i32)
        .height(config.height as i32)
        .framerate(gstreamer::Fraction::new(config.fps as i32, 1))
        .build()
}
```

to:

```rust
fn make_appsrc_caps(config: &EncoderConfig) -> gstreamer::Caps {
    let colorimetry = colorimetry_for(&config.color_space);
    gstreamer::Caps::builder("video/x-raw")
        .field("format", gstreamer_video::VideoFormat::Rgba.to_str())
        .field("width", config.width as i32)
        .field("height", config.height as i32)
        .field("framerate", gstreamer::Fraction::new(config.fps as i32, 1))
        .field("colorimetry", colorimetry)
        .build()
}
```

Note: We switch from `VideoCapsBuilder` to raw `Caps::builder` because `VideoCapsBuilder` does not expose a colorimetry setter. The raw builder lets us add arbitrary fields to `video/x-raw` caps.

---

### Task 4: Pass `color_space` when building `EncoderConfig` in toolbar

**Files:**
- Modify: `src/ui/toolbar.rs:433-467`

- [ ] **Step 1: Add `color_space` to `stream_encoder_config`**

Change:

```rust
    EncoderConfig {
        width,
        height,
        fps: state.settings.stream.fps,
        bitrate_kbps: bitrate,
        encoder_type: state.settings.stream.encoder,
    }
```

to:

```rust
    EncoderConfig {
        width,
        height,
        fps: state.settings.stream.fps,
        bitrate_kbps: bitrate,
        encoder_type: state.settings.stream.encoder,
        color_space: state.settings.video.color_space.clone(),
    }
```

- [ ] **Step 2: Add `color_space` to `record_encoder_config`**

Change:

```rust
    EncoderConfig {
        width,
        height,
        fps: state.settings.record.fps,
        bitrate_kbps: bitrate,
        encoder_type: state.settings.record.encoder,
    }
```

to:

```rust
    EncoderConfig {
        width,
        height,
        fps: state.settings.record.fps,
        bitrate_kbps: bitrate,
        encoder_type: state.settings.record.encoder,
        color_space: state.settings.video.color_space.clone(),
    }
```

- [ ] **Step 3: Pass fps to `StartVirtualCamera`**

Find the `StartVirtualCamera` send at line 329:

```rust
            let _ = tx.try_send(crate::gstreamer::GstCommand::StartVirtualCamera);
```

Change to:

```rust
            let _ = tx.try_send(crate::gstreamer::GstCommand::StartVirtualCamera {
                fps: state.settings.video.fps,
            });
```

---

### Task 5: Update all `AddCaptureSource` call sites to pass `fps`

This task updates every place that sends `GstCommand::AddCaptureSource` to include `fps: state.settings.video.fps` (or the equivalent access path for that context).

**Files:**
- Modify: `src/main.rs` (lines 735, 750, 762)
- Modify: `src/ui/scenes_panel.rs` (lines 838, 848, 855, 876, 911, 921, 928, 949, 1072, 1083, 1091, 1113)
- Modify: `src/ui/preview_panel.rs` (lines 842, 856, 867)
- Modify: `src/ui/properties_panel.rs` (lines 942, 1177, 1382, 1445, 1957, 2002)
- Modify: `src/ui/sources_panel.rs` (lines 677, 691, 699, 721)

- [ ] **Step 1: Update `src/main.rs`**

For each `AddCaptureSource` at lines 735, 750, 762, add `fps: state.settings.video.fps,` after the `config` field. Example pattern — every instance of:

```rust
                                    let _ = tx.try_send(gstreamer::GstCommand::AddCaptureSource {
                                        source_id: src_id,
                                        config: gstreamer::CaptureSourceConfig::Screen {
                                            ...
                                        },
                                    });
```

becomes:

```rust
                                    let _ = tx.try_send(gstreamer::GstCommand::AddCaptureSource {
                                        source_id: src_id,
                                        config: gstreamer::CaptureSourceConfig::Screen {
                                            ...
                                        },
                                        fps: state.settings.video.fps,
                                    });
```

- [ ] **Step 2: Update `src/ui/scenes_panel.rs`**

Same pattern for all ~12 call sites. The function signatures in scenes_panel.rs have access to `state: &AppState` or settings directly. Add `fps: state.settings.video.fps,` to each `AddCaptureSource` struct literal.

Note: Some call sites may access settings through a different variable name (e.g. a local binding). Check the function signature to find the correct access path. If the function takes `settings: &AppSettings`, use `settings.video.fps`. If it takes `state: &AppState`, use `state.settings.video.fps`.

- [ ] **Step 3: Update `src/ui/preview_panel.rs`**

Same pattern for the 3 call sites at lines 842, 856, 867. Access settings via `state.settings.video.fps`.

- [ ] **Step 4: Update `src/ui/properties_panel.rs`**

Same pattern for the 6 call sites. Access settings via `state.settings.video.fps`.

- [ ] **Step 5: Update `src/ui/sources_panel.rs`**

Same pattern for the 4 call sites. Access settings via `state.settings.video.fps`.

---

### Task 6: Remove "not yet implemented" UI labels

**Files:**
- Modify: `src/ui/settings/video.rs:212-254`

- [ ] **Step 1: Remove stub label from FPS section**

Change:

```rust
            ui.label("FPS (not yet implemented)");
```

to:

```rust
            ui.label("FPS");
```

- [ ] **Step 2: Remove stub label from Color Space section**

Change:

```rust
            ui.label("Color Space (not yet implemented)");
```

to:

```rust
            ui.label("Color Space");
```

---

### Task 7: Build, test, and fix compilation

- [ ] **Step 1: Run `cargo build` and fix any remaining compilation errors**

Run: `cargo build 2>&1`

The compiler will flag any call sites missed in Task 5 (struct literal missing `fps` field, enum variant missing fields). Fix each one by adding the appropriate `fps` or `color_space` field.

- [ ] **Step 2: Run `cargo test`**

Run: `cargo test 2>&1`

Expected: All existing tests pass. The `EncoderConfig::default()` test may need updating if one exists that checks field count.

- [ ] **Step 3: Run `cargo clippy`**

Run: `cargo clippy 2>&1`

Fix any warnings.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat: wire video FPS into capture pipelines and color space into encode caps"
```
