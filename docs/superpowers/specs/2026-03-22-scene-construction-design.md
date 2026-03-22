# Scene Construction & Persistence Design

## Overview

Add scene/source persistence to disk and improve the scene editor UI with delete, source configuration, and scene switching that drives the GStreamer capture pipeline. One Display source per scene (until compositor is built).

## Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Persistence location | `~/.config/lodestone/scenes.toml` | Separate from settings.toml, same debounced save pattern |
| Source types | Display only | Only screen capture works via GStreamer today |
| Source properties | Transform + visibility + name + screen_index | Full source configuration for what's functional |
| Scene switching | Drives GStreamer capture | Active scene's source determines what's captured |
| Sources per scene | One (for now) | Without compositor, multiple sources can't be rendered |

## Section 1: Scene Data Model & Persistence

`SceneCollection` is a **persistence-only** wrapper — it does not own scene data at runtime. On load, its fields are destructured into `AppState.scenes`, `AppState.sources`, and `AppState.active_scene_id`. On save, a `SceneCollection` is constructed from those `AppState` fields.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneCollection {
    pub scenes: Vec<Scene>,
    pub sources: Vec<Source>,
    pub active_scene_id: Option<SceneId>,
    pub next_scene_id: u64,
    pub next_source_id: u64,
}
```

`next_scene_id` and `next_source_id` are persisted counters that monotonically increase. They prevent ID reuse across save/load cycles (e.g., deleting scene 3 and restarting won't reuse ID 3). These counters are also stored on `AppState` at runtime.

Saved to `~/.config/lodestone/scenes.toml`. Uses the same debounced save pattern as settings — `scenes_dirty` and `scenes_last_changed: Instant` flags on `AppState`, 500ms debounce before writing.

`SceneCollection` has `load_from(path)` and `save_to(path)` methods following the `AppSettings` pattern. On startup, `AppManager::new()` loads the collection. If the file doesn't exist or is corrupt, falls back to a default "Scene 1" with one Display source on screen 0.

Replaces the hard-coded initialization in `main.rs`. The unused `SourceConfig` struct in `scene.rs` is removed as part of this work.

## Section 2: Source Configuration

Each Display source stores which monitor to capture via a new `SourceProperties` enum:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SourceProperties {
    Display { screen_index: u32 },
}

impl Default for SourceProperties {
    fn default() -> Self {
        Self::Display { screen_index: 0 }
    }
}
```

Added as a field on `Source` with `#[serde(default)]` for backwards compatibility with any serialized data that lacks the field:

```rust
pub struct Source {
    pub id: SourceId,
    pub name: String,
    pub source_type: SourceType,
    #[serde(default)]
    pub properties: SourceProperties,
    pub transform: Transform,
    pub visible: bool,
    pub muted: bool,
    pub volume: f32,
}
```

New Display sources default to `SourceProperties::Display { screen_index: 0 }`. The scene editor shows a monitor selector for Display sources. Changing `screen_index` on the active scene's source sends `GstCommand::SetCaptureSource` to update capture immediately.

**Monitor enumeration:** The number of available monitors comes from `winit`'s `event_loop.available_monitors().count()` at startup, stored as `monitor_count: usize` on `AppState`. The scene editor dropdown shows indices 0 through `monitor_count - 1`.

## Section 3: Scene Editor UI

**Scene list:**
- "+" button to add scenes (existing, updated to use `next_scene_id` counter)
- "-" button to delete selected scene. At least one scene must exist — if deleting the last scene, create a new default scene first.
- Clicking a scene switches `active_scene_id` and sends `GstCommand::SetCaptureSource` with the scene's source's `screen_index`. If the scene has no source, sends `GstCommand::StopCapture`.

**Source management (one source per scene):**
- The existing generic "+" button for sources is replaced with conditional UI
- If active scene has no source: show "Add Display Source" button (uses `next_source_id` counter)
- If it has a source: show the source with properties and a delete button
- Delete removes the source from the scene and the sources list, sends `StopCapture`

**Source properties panel:**
- Name: editable text field
- Visible: checkbox toggle
- Monitor: dropdown of available screen indices (0 through `monitor_count - 1`)
- Transform: x/y/width/height drag values (existing)

**Dirty flag:** Any change sets `scenes_dirty = true` and `scenes_last_changed = Instant::now()` on `AppState`. The debounced save in `main.rs` writes `scenes.toml`.

## Section 4: Scene Switching & GStreamer Integration

When the user selects a different scene:
1. `state.active_scene_id` updated
2. Look up the scene's source
3. If source exists with `SourceProperties::Display { screen_index }`: send `GstCommand::SetCaptureSource(CaptureSourceConfig::Screen { screen_index })`
4. If no source: send `GstCommand::StopCapture`

**`GstCommand::StopCapture`** — new variant added to the `GstCommand` enum in `commands.rs`. Handled in `thread.rs::handle_command()` by calling the existing `self.stop_capture()` method (line 93 of thread.rs), which sets the capture pipeline to Null and clears the appsink. The main thread then calls `clear_preview()` to upload a blank frame.

**Blank preview:** When capture stops, `clear_preview()` on `SharedGpuState` uploads a solid dark gray `RgbaFrame` to the preview texture using the existing `upload_frame()` method. No new renderer methods needed — just construct a dark gray `RgbaFrame` and call `upload_frame()`.

**Startup:** After loading `scenes.toml`, `main.rs` sends the active scene's `SetCaptureSource` command (or `StopCapture` if no source). This replaces the hard-coded `Screen { screen_index: 0 }` in `thread.rs::run()`.

The GStreamer thread's `run()` method no longer starts capture automatically — it waits for the first command from main.

**Scene switching during stream/record:** Brief frame starvation occurs during the capture pipeline restart (~1-2 frames). The GStreamer encode pipelines handle missing frames gracefully (they wait for the next frame). This is a known limitation — scene transitions will address it in a future spec.

## Section 5: Module Changes

```
src/
├── scene.rs              — add SceneCollection, SourceProperties (with Default + serde(default)),
│                           save/load functions, remove dead SourceConfig struct
├── state.rs              — add scenes_dirty, scenes_last_changed, next_scene_id, next_source_id,
│                           monitor_count fields
├── main.rs               — load scenes.toml, debounced scene save, send initial capture command,
│                           store monitor_count from winit
├── gstreamer/
│   ├── commands.rs       — add StopCapture variant to GstCommand
│   └── thread.rs         — handle StopCapture (calls existing stop_capture()),
│                           remove auto-start of capture in run()
└── ui/
    └── scene_editor.rs   — scene/source delete, source properties panel, monitor selector,
                            scene switching sends commands, replace generic "+" with conditional UI
```

No new files. All changes modify existing code. Preview blank frame uses existing `upload_frame()` — no renderer changes needed.

## Future Extensions

Out of scope but accommodated by the design:
- **Multi-source scenes** — remove the one-source restriction when compositor is built. `SourceProperties` enum gains Window, Camera, Image variants.
- **Scene duplication** — clone a scene with its sources.
- **Scene transitions** — crossfade/cut between scenes on switch.
- **Source ordering** — z-index for layering when compositor handles multiple sources.
