# Scene Construction & Persistence Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add scene/source persistence to `scenes.toml`, improve the scene editor with delete/properties/monitor selection, and make scene switching drive GStreamer capture.

**Architecture:** `SceneCollection` is a persistence-only wrapper for scenes/sources/IDs. It loads into `AppState` on startup and saves via debounced dirty flag. Scene switching sends `SetCaptureSource` or `StopCapture` to the GStreamer thread. One Display source per scene.

**Tech Stack:** Rust, serde/toml, egui, winit (monitor enumeration)

**Spec:** `docs/superpowers/specs/2026-03-22-scene-construction-design.md`

---

## File Structure

### Modified files
- `src/scene.rs` — add `SourceProperties`, `SceneCollection`, save/load, remove `SourceConfig`
- `src/state.rs` — add `scenes_dirty`, `scenes_last_changed`, `next_scene_id`, `next_source_id`, `monitor_count`
- `src/gstreamer/commands.rs` — add `StopCapture` to `GstCommand`
- `src/gstreamer/thread.rs` — handle `StopCapture`, remove auto-start of capture
- `src/main.rs` — load `scenes.toml`, debounced save, send initial capture command, store monitor count
- `src/ui/scene_editor.rs` — rewrite: scene delete, source properties, monitor selector, scene switching sends commands

---

### Task 1: Add SourceProperties and SceneCollection to scene.rs

**Files:**
- Modify: `src/scene.rs`

- [ ] **Step 1: Add `SourceProperties` enum with Default**

After the `Transform` struct, add:

```rust
/// Type-specific source configuration.
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

- [ ] **Step 2: Add `properties` field to `Source` with `#[serde(default)]`**

Add the field after `source_type`:

```rust
#[serde(default)]
pub properties: SourceProperties,
```

- [ ] **Step 3: Remove `SourceConfig` struct and its `#[allow(dead_code)]`**

Delete lines 45-51 (the `SourceConfig` struct).

- [ ] **Step 4: Add `SceneCollection` struct with save/load**

```rust
use std::path::Path;

/// Persistence wrapper for scene/source data.
/// Does not own data at runtime — used only for serialization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneCollection {
    pub scenes: Vec<Scene>,
    pub sources: Vec<Source>,
    pub active_scene_id: Option<SceneId>,
    #[serde(default = "default_next_id")]
    pub next_scene_id: u64,
    #[serde(default = "default_next_id")]
    pub next_source_id: u64,
}

fn default_next_id() -> u64 {
    1
}

impl SceneCollection {
    /// Create a default collection with one scene and one Display source.
    pub fn default_collection() -> Self {
        let scene_id = SceneId(1);
        let source_id = SourceId(1);
        Self {
            scenes: vec![Scene {
                id: scene_id,
                name: "Scene 1".to_string(),
                sources: vec![source_id],
            }],
            sources: vec![Source {
                id: source_id,
                name: "Display".to_string(),
                source_type: SourceType::Display,
                properties: SourceProperties::Display { screen_index: 0 },
                transform: Transform::new(0.0, 0.0, 1920.0, 1080.0),
                visible: true,
                muted: false,
                volume: 1.0,
            }],
            active_scene_id: Some(scene_id),
            next_scene_id: 2,
            next_source_id: 2,
        }
    }

    /// Load from a TOML file. Returns default collection on any error.
    pub fn load_from(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(contents) => toml::from_str(&contents).unwrap_or_else(|e| {
                log::warn!("Failed to parse scenes.toml, using default: {e}");
                Self::default_collection()
            }),
            Err(_) => Self::default_collection(),
        }
    }

    /// Save to a TOML file.
    pub fn save_to(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let toml_str = toml::to_string_pretty(self)?;
        std::fs::write(path, toml_str)?;
        Ok(())
    }
}
```

- [ ] **Step 5: Update existing tests for the new `properties` field**

The `source_defaults_visible_unmuted` test creates a `Source` — add the `properties` field:

```rust
properties: SourceProperties::default(),
```

Add new tests:

```rust
#[test]
fn source_properties_default_is_display_0() {
    let props = SourceProperties::default();
    assert!(matches!(props, SourceProperties::Display { screen_index: 0 }));
}

#[test]
fn scene_collection_default_has_one_scene() {
    let coll = SceneCollection::default_collection();
    assert_eq!(coll.scenes.len(), 1);
    assert_eq!(coll.sources.len(), 1);
    assert_eq!(coll.next_scene_id, 2);
    assert_eq!(coll.next_source_id, 2);
}

#[test]
fn scene_collection_roundtrip() {
    let coll = SceneCollection::default_collection();
    let toml_str = toml::to_string_pretty(&coll).unwrap();
    let parsed: SceneCollection = toml::from_str(&toml_str).unwrap();
    assert_eq!(parsed.scenes.len(), 1);
    assert_eq!(parsed.sources.len(), 1);
    assert_eq!(parsed.next_scene_id, 2);
}
```

- [ ] **Step 6: Run tests**

Run: `cargo test scene`
Expected: all scene tests pass

- [ ] **Step 7: Commit**

```bash
git add src/scene.rs
git commit -m "feat: add SourceProperties, SceneCollection with save/load"
```

---

### Task 2: Update AppState with scene persistence fields

**Files:**
- Modify: `src/state.rs`

- [ ] **Step 1: Add new fields to `AppState`**

Add after `settings_last_changed`:

```rust
pub scenes_dirty: bool,
pub scenes_last_changed: std::time::Instant,
pub next_scene_id: u64,
pub next_source_id: u64,
pub monitor_count: usize,
pub capture_active: bool,
```

- [ ] **Step 2: Update `Default` impl**

Add:

```rust
scenes_dirty: false,
scenes_last_changed: std::time::Instant::now(),
next_scene_id: 1,
next_source_id: 1,
monitor_count: 1,
capture_active: true,
```

- [ ] **Step 3: Run tests**

Run: `cargo test`
Expected: all tests pass

- [ ] **Step 4: Commit**

```bash
git add src/state.rs
git commit -m "feat: add scene persistence and monitor count fields to AppState"
```

---

### Task 3: Add StopCapture command and handler

**Files:**
- Modify: `src/gstreamer/commands.rs`
- Modify: `src/gstreamer/thread.rs`

- [ ] **Step 1: Add `StopCapture` variant to `GstCommand`**

In `src/gstreamer/commands.rs`, add to the `GstCommand` enum:

```rust
StopCapture,
```

- [ ] **Step 2: Handle `StopCapture` in `thread.rs`**

In `handle_command()`, add a match arm:

```rust
GstCommand::StopCapture => {
    self.stop_capture();
    log::info!("Capture stopped");
}
```

- [ ] **Step 3: Remove auto-start of capture in `run()`**

In `thread.rs::run()`, remove or comment out the line:
```rust
self.start_capture(&CaptureSourceConfig::Screen { screen_index: 0 });
```

The GStreamer thread now waits for the first `SetCaptureSource` command from `main.rs`.

- [ ] **Step 4: Run tests**

Run: `cargo test`
Expected: all tests pass

- [ ] **Step 5: Commit**

```bash
git add src/gstreamer/commands.rs src/gstreamer/thread.rs
git commit -m "feat: add StopCapture command, remove auto-start capture"
```

---

### Task 4: Wire scene loading and saving into main.rs

**Files:**
- Modify: `src/main.rs`
- Modify: `src/settings.rs`

- [ ] **Step 1: Add `scenes_path()` helper to `src/settings.rs`**

Add to the bottom of `settings.rs`:

```rust
pub fn scenes_path() -> PathBuf {
    config_dir().join("scenes.toml")
}
```

- [ ] **Step 2: Load scenes in `AppManager::new()`**

Replace the hard-coded scene initialization with:

```rust
use crate::scene::SceneCollection;

let collection = SceneCollection::load_from(&settings::scenes_path());
let initial_state = AppState {
    scenes: collection.scenes,
    sources: collection.sources,
    active_scene_id: collection.active_scene_id,
    next_scene_id: collection.next_scene_id,
    next_source_id: collection.next_source_id,
    command_tx: Some(main_channels.command_tx.clone()),
    ..AppState::default()
};
```

- [ ] **Step 3: Send initial capture command in `resumed()`**

After the GPU is initialized and the GStreamer thread is running, send the initial capture command based on the active scene's source. Add after the existing setup in `resumed()`:

```rust
// Send initial capture command based on active scene
{
    let state = self.state.lock().unwrap();
    if let Some(scene_id) = state.active_scene_id {
        if let Some(scene) = state.scenes.iter().find(|s| s.id == scene_id) {
            if let Some(&src_id) = scene.sources.first() {
                if let Some(source) = state.sources.iter().find(|s| s.id == src_id) {
                    if let crate::scene::SourceProperties::Display { screen_index } = source.properties {
                        if let Some(ref tx) = state.command_tx {
                            let _ = tx.try_send(gstreamer::GstCommand::SetCaptureSource(
                                gstreamer::CaptureSourceConfig::Screen { screen_index },
                            ));
                        }
                    }
                }
            }
        }
    }
}
```

- [ ] **Step 4: Store monitor count in `resumed()`**

After creating the window, store the monitor count:

```rust
{
    let monitor_count = event_loop.available_monitors().count().max(1);
    let mut state = self.state.lock().unwrap();
    state.monitor_count = monitor_count;
}
```

- [ ] **Step 5: Add debounced scene save in the render loop**

In the `RedrawRequested` handler (alongside the existing settings save), add scene saving. Find the existing `settings_dirty` debounce block and add a similar block for scenes:

```rust
if app_state.scenes_dirty
    && app_state.scenes_last_changed.elapsed() > std::time::Duration::from_millis(500)
{
    let collection = crate::scene::SceneCollection {
        scenes: app_state.scenes.clone(),
        sources: app_state.sources.clone(),
        active_scene_id: app_state.active_scene_id,
        next_scene_id: app_state.next_scene_id,
        next_source_id: app_state.next_source_id,
    };
    let path = settings::scenes_path();
    if let Err(e) = collection.save_to(&path) {
        log::warn!("Failed to save scenes: {e}");
    }
    app_state.scenes_dirty = false;
}
```

- [ ] **Step 6: Upload blank frame when StopCapture is received**

In `about_to_wait()`, after the frame polling loop, add a check: when the GStreamer thread's capture is stopped (no frames coming), upload a blank frame. The simplest approach is to track whether capture is active via a flag. Add a `capture_active: bool` field to `AppState` (default `true`). When the scene editor sends `StopCapture`, set `capture_active = false`. When it sends `SetCaptureSource`, set `capture_active = true`.

In `about_to_wait()`, after frame polling:
```rust
if !app_state.capture_active {
    if let Some(ref gpu) = self.gpu {
        let w = gpu.preview_renderer.width;
        let h = gpu.preview_renderer.height;
        let blank = crate::gstreamer::RgbaFrame {
            data: vec![30u8; (w * h * 4) as usize], // dark gray RGBA
            width: w,
            height: h,
        };
        gpu.preview_renderer.upload_frame(&gpu.queue, &blank);
    }
}
```

Note: This uploads a blank frame every tick while capture is stopped. This is fine — it's just writing to a texture that's already being rendered. An optimization would be to only do it once (via a `blank_uploaded` flag), but that's unnecessary complexity for now.

- [ ] **Step 7: Run `cargo check`**

Expected: compiles. The app won't auto-capture on launch anymore — it waits for the command from `resumed()`.

- [ ] **Step 8: Commit**

```bash
git add src/main.rs src/settings.rs
git commit -m "feat: load/save scenes.toml, send initial capture from active scene"
```

---

### Task 5: Rewrite scene editor UI

**Files:**
- Modify: `src/ui/scene_editor.rs`

This is the largest task. The entire `draw()` function is rewritten.

- [ ] **Step 1: Rewrite the scene editor**

Replace the entire contents of `src/ui/scene_editor.rs` with:

```rust
use crate::scene::{Scene, SceneId, Source, SourceId, SourceProperties, SourceType, Transform};
use crate::state::AppState;
use crate::ui::layout::PanelId;

/// Helper: send a capture command for the given source, or stop capture if no source.
/// Takes individual fields to avoid borrowing all of AppState.
fn send_capture_for_scene(
    cmd_tx: &Option<tokio::sync::mpsc::Sender<crate::gstreamer::GstCommand>>,
    sources: &[Source],
    scene: &Scene,
) {
    let Some(ref tx) = cmd_tx else { return };
    if let Some(&src_id) = scene.sources.first() {
        if let Some(source) = sources.iter().find(|s| s.id == src_id) {
            if let SourceProperties::Display { screen_index } = source.properties {
                let _ = tx.try_send(crate::gstreamer::GstCommand::SetCaptureSource(
                    crate::gstreamer::CaptureSourceConfig::Screen { screen_index },
                ));
                return;
            }
        }
    }
    let _ = tx.try_send(crate::gstreamer::GstCommand::StopCapture);
}

// Note: mark_dirty is NOT a helper function — inline the two lines everywhere
// to avoid borrow checker issues when state fields are already borrowed:
//   state.scenes_dirty = true;
//   state.scenes_last_changed = std::time::Instant::now();

pub fn draw(ui: &mut egui::Ui, state: &mut AppState, panel_id: PanelId) {
    // ---- Scenes section ----
    ui.horizontal(|ui| {
        ui.heading("Scenes");
        if ui.button("+").clicked() {
            let new_id = SceneId(state.next_scene_id);
            state.next_scene_id += 1;
            state.scenes.push(Scene {
                id: new_id,
                name: format!("Scene {}", state.scenes.len() + 1),
                sources: Vec::new(),
            });
            state.scenes_dirty = true;
            state.scenes_last_changed = std::time::Instant::now();
        }
        // Delete button
        if ui.button("-").clicked() {
            if let Some(active_id) = state.active_scene_id {
                // Remove sources belonging to this scene
                if let Some(scene) = state.scenes.iter().find(|s| s.id == active_id) {
                    let source_ids: Vec<SourceId> = scene.sources.clone();
                    state.sources.retain(|s| !source_ids.contains(&s.id));
                }
                state.scenes.retain(|s| s.id != active_id);
                // If no scenes left, create a new default
                if state.scenes.is_empty() {
                    let new_id = SceneId(state.next_scene_id);
                    state.next_scene_id += 1;
                    state.scenes.push(Scene {
                        id: new_id,
                        name: "Scene 1".to_string(),
                        sources: Vec::new(),
                    });
                }
                // Select the first remaining scene
                state.active_scene_id = state.scenes.first().map(|s| s.id);
                // Update capture for newly active scene
                let cmd_tx = state.command_tx.clone();
                if let Some(id) = state.active_scene_id {
                    let scene = state.scenes.iter().find(|s| s.id == id).cloned();
                    if let Some(scene) = scene {
                        send_capture_for_scene(&cmd_tx, &state.sources, &scene);
                    }
                }
                state.scenes_dirty = true;
                state.scenes_last_changed = std::time::Instant::now();
            }
        }
    });

    // Scene list with switching
    let mut switched_scene: Option<SceneId> = None;
    for scene in &state.scenes {
        let is_selected = state.active_scene_id == Some(scene.id);
        if ui.selectable_label(is_selected, &scene.name).clicked() && !is_selected {
            switched_scene = Some(scene.id);
        }
    }
    if let Some(new_id) = switched_scene {
        state.active_scene_id = Some(new_id);
        let cmd_tx = state.command_tx.clone();
        let scene = state.scenes.iter().find(|s| s.id == new_id).cloned();
        if let Some(scene) = scene {
            send_capture_for_scene(&cmd_tx, &state.sources, &scene);
        }
        mark_dirty(state);
    }

    ui.separator();

    // ---- Source section (one source per scene) ----
    let active_scene = state.active_scene_id
        .and_then(|id| state.scenes.iter().find(|s| s.id == id));
    let source_id = active_scene.and_then(|s| s.sources.first().copied());

    let Some(src_id) = source_id else {
        // No source — show add button
        ui.heading("Sources");
        if ui.button("Add Display Source").clicked() {
            if let Some(active_id) = state.active_scene_id {
                let new_src_id = SourceId(state.next_source_id);
                state.next_source_id += 1;
                let new_source = Source {
                    id: new_src_id,
                    name: "Display".to_string(),
                    source_type: SourceType::Display,
                    properties: SourceProperties::Display { screen_index: 0 },
                    transform: Transform::new(0.0, 0.0, 1920.0, 1080.0),
                    visible: true,
                    muted: false,
                    volume: 1.0,
                };
                state.sources.push(new_source);
                if let Some(scene) = state.scenes.iter_mut().find(|s| s.id == active_id) {
                    scene.sources.push(new_src_id);
                }
                // Start capturing from the new source
                if let Some(ref tx) = state.command_tx {
                    let _ = tx.try_send(crate::gstreamer::GstCommand::SetCaptureSource(
                        crate::gstreamer::CaptureSourceConfig::Screen { screen_index: 0 },
                    ));
                }
                state.scenes_dirty = true;
            state.scenes_last_changed = std::time::Instant::now();
            }
        }
        return;
    };

    // Source header with delete button
    ui.horizontal(|ui| {
        ui.heading("Source");
        if ui.button("Delete").clicked() {
            if let Some(active_id) = state.active_scene_id {
                if let Some(scene) = state.scenes.iter_mut().find(|s| s.id == active_id) {
                    scene.sources.retain(|&id| id != src_id);
                }
                state.sources.retain(|s| s.id != src_id);
                // Stop capture since scene now has no source
                if let Some(ref tx) = state.command_tx {
                    let _ = tx.try_send(crate::gstreamer::GstCommand::StopCapture);
                }
                state.scenes_dirty = true;
            state.scenes_last_changed = std::time::Instant::now();
            }
            return;
        }
    });

    // Source properties
    let monitor_count = state.monitor_count;
    if let Some(source) = state.sources.iter_mut().find(|s| s.id == src_id) {
        // Name
        ui.horizontal(|ui| {
            ui.label("Name");
            if ui.text_edit_singleline(&mut source.name).changed() {
                state.scenes_dirty = true;
            state.scenes_last_changed = std::time::Instant::now();
            }
        });

        // Visible
        if ui.checkbox(&mut source.visible, "Visible").changed() {
            state.scenes_dirty = true;
            state.scenes_last_changed = std::time::Instant::now();
        }

        // Monitor selector
        if let SourceProperties::Display { ref mut screen_index } = source.properties {
            ui.horizontal(|ui| {
                ui.label("Monitor");
                let prev_index = *screen_index;
                egui::ComboBox::from_id_salt(egui::Id::new(("monitor_select", panel_id.0)))
                    .selected_text(format!("Monitor {}", screen_index))
                    .show_ui(ui, |ui| {
                        for i in 0..monitor_count as u32 {
                            ui.selectable_value(screen_index, i, format!("Monitor {i}"));
                        }
                    });
                if *screen_index != prev_index {
                    // Send capture command for new monitor
                    if let Some(ref tx) = state.command_tx {
                        let _ = tx.try_send(crate::gstreamer::GstCommand::SetCaptureSource(
                            crate::gstreamer::CaptureSourceConfig::Screen { screen_index: *screen_index },
                        ));
                    }
                    state.scenes_dirty = true;
            state.scenes_last_changed = std::time::Instant::now();
                }
            });
        }

        // Transform
        ui.separator();
        ui.label("Transform");
        let mut transform_changed = false;
        egui::Grid::new(egui::Id::new(("transform_grid", panel_id.0)))
            .num_columns(2)
            .show(ui, |ui| {
                ui.label("X");
                transform_changed |= ui.add(egui::DragValue::new(&mut source.transform.x).speed(1.0)).changed();
                ui.end_row();
                ui.label("Y");
                transform_changed |= ui.add(egui::DragValue::new(&mut source.transform.y).speed(1.0)).changed();
                ui.end_row();
                ui.label("Width");
                transform_changed |= ui.add(egui::DragValue::new(&mut source.transform.width).speed(1.0)).changed();
                ui.end_row();
                ui.label("Height");
                transform_changed |= ui.add(egui::DragValue::new(&mut source.transform.height).speed(1.0)).changed();
                ui.end_row();
            });
        if transform_changed {
            state.scenes_dirty = true;
            state.scenes_last_changed = std::time::Instant::now();
        }
    }
}
```

- [ ] **Step 2: Run `cargo check`**

Expected: compiles. There will be a borrow checker issue with `mark_dirty` taking `&mut state` inside closures that also borrow `state`. If so, inline the dirty flag setting: `state.scenes_dirty = true; state.scenes_last_changed = std::time::Instant::now();`

- [ ] **Step 3: Run `cargo test`**

Expected: all tests pass

- [ ] **Step 4: Commit**

```bash
git add src/ui/scene_editor.rs
git commit -m "feat: rewrite scene editor with delete, properties, monitor selector, and scene switching"
```

---

### Task 6: End-to-end verification

**Files:** None (manual testing)

- [ ] **Step 1: Run the app**

Run: `cargo run`
Expected:
- Default "Scene 1" with Display source loads
- Preview shows screen capture
- `~/.config/lodestone/scenes.toml` is created on first edit

- [ ] **Step 2: Test scene creation and switching**

Add a second scene, select it (preview should go blank — no source), add a Display source (preview starts capturing).

- [ ] **Step 3: Test persistence**

Close the app. Reopen. Scenes, sources, and active scene should be restored.

- [ ] **Step 4: Test scene/source deletion**

Delete a source (preview goes blank). Delete a scene (next scene becomes active).

- [ ] **Step 5: Test monitor selector**

If multiple monitors, change the monitor dropdown. Preview should switch to the other screen.

- [ ] **Step 6: Commit any fixes**

---

### Task 7: Final cleanup

**Files:** Various

- [ ] **Step 1: Run `cargo clippy` and fix warnings**
- [ ] **Step 2: Run `cargo fmt`**
- [ ] **Step 3: Run `cargo test` — all pass**
- [ ] **Step 4: Commit**

```bash
git add src/
git commit -m "fix: clippy, fmt, and cleanup for scene construction"
```
