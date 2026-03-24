# Source Library Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Redesign source management from per-scene to a global source library with inheritance-based per-scene property overrides.

**Architecture:** Replace `Source` with `LibrarySource` (canonical definition) and `SceneSource` (per-scene reference with optional overrides). Each overridable property uses `Option<T>` — `None` means inherit from library, `Some(v)` means scene-local override. A new Library panel provides source CRUD; the existing Sources panel becomes a composition tool.

**Tech Stack:** Rust, serde (TOML persistence), egui (UI), wgpu (rendering), GStreamer (capture pipelines)

**Spec:** `docs/superpowers/specs/2026-03-23-source-library-design.md`

---

## File Structure

| File | Action | Responsibility |
|------|--------|---------------|
| `src/scene.rs` | Modify | Replace `Source`/`Scene`/`SceneCollection` with `LibrarySource`/`SceneSource`/`SourceOverrides` + migration logic |
| `src/state.rs` | Modify | Rename `sources` → `library`, add helper methods for resolved property access |
| `src/ui/library_panel.rs` | Create | New Library panel — source CRUD, By Type / Folders views |
| `src/ui/sources_panel.rs` | Modify | Remove source creation; become scene composition tool with "Add from Library" picker |
| `src/ui/properties_panel.rs` | Modify | Override indicators, reset-to-library, dual mode (library defaults vs scene overrides) |
| `src/ui/scenes_panel.rs` | Modify | Update `apply_scene_diff` and `delete_scene_by_id` for new data model |
| `src/ui/mod.rs` | Modify | Register `library_panel` module and `Library` panel type |
| `src/ui/layout/tree.rs` | Modify | Add `PanelType::Library`, update default layout |
| `src/main.rs` | Modify | Load new `SceneCollection` format, populate `state.library` |
| `src/renderer/compositor.rs` | Modify | Update `compose()` to accept resolved source data |

---

### Task 1: Data Model — LibrarySource, SceneSource, SourceOverrides

**Files:**
- Modify: `src/scene.rs:11-184`
- Test: `src/scene.rs` (existing tests block, lines 186-342)

- [ ] **Step 1: Write failing tests for the new data model**

Add tests to `src/scene.rs` that exercise the new types:

```rust
#[test]
fn scene_source_inherits_library_defaults() {
    let lib = LibrarySource {
        id: SourceId(1),
        name: "Cam".into(),
        source_type: SourceType::Camera,
        properties: SourceProperties::Camera { device_index: 0, device_name: "Cam".into() },
        folder: None,
        transform: Transform::new(0.0, 0.0, 640.0, 480.0),
        native_size: (640.0, 480.0),
        opacity: 1.0,
        visible: true,
        muted: false,
        volume: 1.0,
    };
    let scene_src = SceneSource {
        source_id: SourceId(1),
        overrides: SourceOverrides::default(),
    };
    assert_eq!(scene_src.resolve_opacity(&lib), 1.0);
    assert!(!scene_src.is_opacity_overridden());
}

#[test]
fn scene_source_override_takes_precedence() {
    let lib = LibrarySource {
        id: SourceId(1),
        name: "Cam".into(),
        source_type: SourceType::Camera,
        properties: SourceProperties::Camera { device_index: 0, device_name: "Cam".into() },
        folder: None,
        transform: Transform::new(0.0, 0.0, 640.0, 480.0),
        native_size: (640.0, 480.0),
        opacity: 1.0,
        visible: true,
        muted: false,
        volume: 1.0,
    };
    let scene_src = SceneSource {
        source_id: SourceId(1),
        overrides: SourceOverrides {
            opacity: Some(0.5),
            ..Default::default()
        },
    };
    assert_eq!(scene_src.resolve_opacity(&lib), 0.5);
    assert!(scene_src.is_opacity_overridden());
}

#[test]
fn scene_source_reset_override() {
    let mut scene_src = SceneSource {
        source_id: SourceId(1),
        overrides: SourceOverrides {
            opacity: Some(0.5),
            ..Default::default()
        },
    };
    scene_src.overrides.opacity = None;
    assert!(!scene_src.is_opacity_overridden());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib scene::tests`
Expected: FAIL — `LibrarySource`, `SceneSource`, `SourceOverrides` do not exist yet.

- [ ] **Step 3: Implement the new types**

In `src/scene.rs`, replace `Source` with `LibrarySource` and add `SceneSource`/`SourceOverrides`:

```rust
/// A source defined in the library. Single source of truth for defaults.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibrarySource {
    pub id: SourceId,
    pub name: String,
    pub source_type: SourceType,
    #[serde(default)]
    pub properties: SourceProperties,
    #[serde(default)]
    pub folder: Option<String>,
    pub transform: Transform,
    #[serde(default = "default_native_size")]
    pub native_size: (f32, f32),
    #[serde(default = "default_opacity")]
    pub opacity: f32,
    pub visible: bool,
    pub muted: bool,
    pub volume: f32,
}

/// Optional per-scene property overrides. None = inherit from library.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SourceOverrides {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transform: Option<Transform>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opacity: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visible: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub muted: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volume: Option<f32>,
}

/// A source's presence in a scene. References a library source by ID.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneSource {
    pub source_id: SourceId,
    #[serde(default)]
    pub overrides: SourceOverrides,
}
```

Add resolution and query methods to `SceneSource`:

```rust
impl SceneSource {
    /// Resolve transform: override if set, else library default.
    pub fn resolve_transform(&self, lib: &LibrarySource) -> Transform {
        self.overrides.transform.unwrap_or(lib.transform)
    }
    pub fn is_transform_overridden(&self) -> bool {
        self.overrides.transform.is_some()
    }

    pub fn resolve_opacity(&self, lib: &LibrarySource) -> f32 {
        self.overrides.opacity.unwrap_or(lib.opacity)
    }
    pub fn is_opacity_overridden(&self) -> bool {
        self.overrides.opacity.is_some()
    }

    pub fn resolve_visible(&self, lib: &LibrarySource) -> bool {
        self.overrides.visible.unwrap_or(lib.visible)
    }
    pub fn is_visible_overridden(&self) -> bool {
        self.overrides.visible.is_some()
    }

    pub fn resolve_muted(&self, lib: &LibrarySource) -> bool {
        self.overrides.muted.unwrap_or(lib.muted)
    }
    pub fn is_muted_overridden(&self) -> bool {
        self.overrides.muted.is_some()
    }

    pub fn resolve_volume(&self, lib: &LibrarySource) -> f32 {
        self.overrides.volume.unwrap_or(lib.volume)
    }
    pub fn is_volume_overridden(&self) -> bool {
        self.overrides.volume.is_some()
    }
}
```

Update `Scene` to use `SceneSource`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scene {
    pub id: SceneId,
    pub name: String,
    pub sources: Vec<SceneSource>,
}
```

Keep the old `Source` type as a private type aliased only for migration (or inline the migration logic).

- [ ] **Step 4: Update Scene helper methods**

Update `move_source_up` and `move_source_down` to work with `Vec<SceneSource>`:

```rust
impl Scene {
    pub fn move_source_up(&mut self, source_id: SourceId) {
        if let Some(pos) = self.sources.iter().position(|s| s.source_id == source_id)
            && pos > 0
        {
            self.sources.swap(pos, pos - 1);
        }
    }

    pub fn move_source_down(&mut self, source_id: SourceId) {
        if let Some(pos) = self.sources.iter().position(|s| s.source_id == source_id)
            && pos + 1 < self.sources.len()
        {
            self.sources.swap(pos, pos + 1);
        }
    }

    /// Get the list of source IDs in this scene (convenience for iteration).
    pub fn source_ids(&self) -> Vec<SourceId> {
        self.sources.iter().map(|s| s.source_id).collect()
    }

    /// Find the SceneSource entry for a given SourceId.
    pub fn find_source(&self, id: SourceId) -> Option<&SceneSource> {
        self.sources.iter().find(|s| s.source_id == id)
    }

    /// Find the mutable SceneSource entry for a given SourceId.
    pub fn find_source_mut(&mut self, id: SourceId) -> Option<&mut SceneSource> {
        self.sources.iter_mut().find(|s| s.source_id == id)
    }
}
```

- [ ] **Step 5: Update SceneCollection**

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneCollection {
    pub library: Vec<LibrarySource>,
    pub scenes: Vec<Scene>,
    pub active_scene_id: Option<SceneId>,
    #[serde(default = "default_next_id")]
    pub next_scene_id: u64,
    #[serde(default = "default_next_id")]
    pub next_source_id: u64,
}
```

Update `default_collection()` and `save_to()` / `load_from()` accordingly. The default collection creates one `LibrarySource` (Display) and one Scene referencing it via `SceneSource`.

- [ ] **Step 6: Update existing tests**

Update all tests in `src/scene.rs` that reference the old `Source` type to use `LibrarySource`. Update tests that create `Scene` with `sources: vec![SourceId(..)]` to use `sources: vec![SceneSource { source_id: SourceId(..), overrides: SourceOverrides::default() }]`. Update `scene_collection_*` tests to use `library` field.

- [ ] **Step 7: Run tests to verify they pass**

Run: `cargo test --lib scene::tests`
Expected: ALL PASS

- [ ] **Step 8: Commit**

```bash
git add src/scene.rs
git commit -m "refactor: replace Source with LibrarySource/SceneSource/SourceOverrides data model"
```

---

### Task 2: Migration — Old Format to New Format

**Files:**
- Modify: `src/scene.rs` (add migration module)

- [ ] **Step 1: Write failing test for migration**

```rust
#[test]
fn migrate_legacy_scene_collection() {
    // Old format TOML with `sources` instead of `library`
    let legacy_toml = r#"
        next_scene_id = 2
        next_source_id = 2

        [[sources]]
        id = 1
        name = "Display"
        source_type = "Display"
        visible = true
        muted = false
        volume = 1.0
        opacity = 1.0
        [sources.properties.Display]
        screen_index = 0
        [sources.transform]
        x = 0.0
        y = 0.0
        width = 1920.0
        height = 1080.0

        [[scenes]]
        id = 1
        name = "Scene 1"
        sources = [1]

        [active_scene_id]
    "#;
    let collection = SceneCollection::from_toml_str(legacy_toml).unwrap();
    assert_eq!(collection.library.len(), 1);
    assert_eq!(collection.library[0].name, "Display");
    assert_eq!(collection.scenes[0].sources.len(), 1);
    assert_eq!(collection.scenes[0].sources[0].source_id, SourceId(1));
    assert!(collection.scenes[0].sources[0].overrides.transform.is_none());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib scene::tests::migrate_legacy_scene_collection`
Expected: FAIL — `from_toml_str` doesn't exist yet.

- [ ] **Step 3: Implement migration in `load_from`**

Add a `from_toml_str` method that tries the new format first, falls back to parsing the legacy format:

```rust
/// Legacy types for migration only.
mod legacy {
    use super::*;

    #[derive(Deserialize)]
    pub struct LegacySceneCollection {
        pub scenes: Vec<LegacyScene>,
        pub sources: Vec<LibrarySource>,  // old Source fields match LibrarySource
        pub active_scene_id: Option<SceneId>,
        #[serde(default = "super::default_next_id")]
        pub next_scene_id: u64,
        #[serde(default = "super::default_next_id")]
        pub next_source_id: u64,
    }

    #[derive(Deserialize)]
    pub struct LegacyScene {
        pub id: SceneId,
        pub name: String,
        pub sources: Vec<SourceId>,
    }
}

impl SceneCollection {
    pub fn from_toml_str(toml_str: &str) -> anyhow::Result<Self> {
        // Try new format first.
        if let Ok(collection) = toml::from_str::<SceneCollection>(toml_str) {
            return Ok(collection);
        }
        // Try legacy format.
        let legacy: legacy::LegacySceneCollection = toml::from_str(toml_str)?;
        Ok(Self {
            library: legacy.sources,
            scenes: legacy.scenes.into_iter().map(|s| Scene {
                id: s.id,
                name: s.name,
                sources: s.sources.into_iter().map(|id| SceneSource {
                    source_id: id,
                    overrides: SourceOverrides::default(),
                }).collect(),
            }).collect(),
            active_scene_id: legacy.active_scene_id,
            next_scene_id: legacy.next_scene_id,
            next_source_id: legacy.next_source_id,
        })
    }

    pub fn load_from(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(contents) => Self::from_toml_str(&contents).unwrap_or_else(|e| {
                log::warn!("Failed to parse scenes.toml, using default: {e}");
                Self::default_collection()
            }),
            Err(_) => Self::default_collection(),
        }
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib scene::tests`
Expected: ALL PASS

- [ ] **Step 5: Commit**

```bash
git add src/scene.rs
git commit -m "feat: add legacy scenes.toml migration to new library format"
```

---

### Task 3: AppState — Rename `sources` to `library` and Add Helpers

**Files:**
- Modify: `src/state.rs:30-81`
- Modify: `src/main.rs:183-193`

- [ ] **Step 1: Update AppState struct**

In `src/state.rs`, rename `sources: Vec<Source>` to `library: Vec<LibrarySource>`:

```rust
use crate::scene::{Scene, SceneId, LibrarySource, SourceId};
// ...
pub struct AppState {
    pub scenes: Vec<Scene>,
    pub library: Vec<LibrarySource>,  // was: sources: Vec<Source>
    // ... rest unchanged
}
```

Update `Default` impl to use `library: Vec::new()`.

- [ ] **Step 2: Add helper methods to AppState**

```rust
impl AppState {
    /// Find a library source by ID.
    pub fn find_library_source(&self, id: SourceId) -> Option<&LibrarySource> {
        self.library.iter().find(|s| s.id == id)
    }

    /// Find a mutable library source by ID.
    pub fn find_library_source_mut(&mut self, id: SourceId) -> Option<&mut LibrarySource> {
        self.library.iter_mut().find(|s| s.id == id)
    }

    /// Get the active scene.
    pub fn active_scene(&self) -> Option<&Scene> {
        self.active_scene_id.and_then(|id| self.scenes.iter().find(|s| s.id == id))
    }

    /// Get the active scene mutably.
    pub fn active_scene_mut(&mut self) -> Option<&mut Scene> {
        self.active_scene_id.and_then(|id| self.scenes.iter_mut().find(|s| s.id == id))
    }

    /// Count how many scenes reference a given source.
    pub fn source_usage_count(&self, source_id: SourceId) -> usize {
        self.scenes.iter().filter(|s| s.sources.iter().any(|ss| ss.source_id == source_id)).count()
    }

    /// Get scene names that reference a given source.
    pub fn scenes_using_source(&self, source_id: SourceId) -> Vec<String> {
        self.scenes.iter()
            .filter(|s| s.sources.iter().any(|ss| ss.source_id == source_id))
            .map(|s| s.name.clone())
            .collect()
    }
}
```

- [ ] **Step 3: Update main.rs to use new field names**

In `src/main.rs:183-193`, change `sources: collection.sources` to `library: collection.library`:

```rust
let initial_state = AppState {
    scenes: collection.scenes,
    library: collection.library,  // was: sources: collection.sources
    active_scene_id: collection.active_scene_id,
    // ... rest unchanged
};
```

- [ ] **Step 4: Fix all compilation errors**

The rename from `state.sources` to `state.library` will cause compilation errors across multiple files. Fix each one — this is a mechanical find-and-replace of `state.sources` → `state.library` in:
- `src/ui/sources_panel.rs` — all references to `state.sources`
- `src/ui/properties_panel.rs` — all references to `state.sources`
- `src/ui/scenes_panel.rs` — all references to `state.sources` and `sources` parameter in helper functions
- `src/renderer/compositor.rs` — if it references `Source` type in `compose()` signature
- Any other files that reference `state.sources` or `crate::scene::Source`

Note: At this point, many of these files will have further type errors because `Scene.sources` is now `Vec<SceneSource>` instead of `Vec<SourceId>`. Fix the minimal amount needed to compile — the full UI rework happens in later tasks.

For files that iterate `scene.sources` expecting `SourceId`, use `scene.source_ids()` or `scene.sources.iter().map(|s| s.source_id)` as a temporary bridge.

- [ ] **Step 5: Run tests and verify compilation**

Run: `cargo test`
Expected: ALL PASS (or at least compilation succeeds; some UI behavior may be temporarily simplified)

- [ ] **Step 6: Commit**

```bash
git add src/state.rs src/main.rs src/ui/ src/scene.rs
git commit -m "refactor: rename state.sources to state.library, update all references"
```

---

### Task 4: Sources Panel — Scene Composition Tool

**Files:**
- Modify: `src/ui/sources_panel.rs`

- [ ] **Step 1: Replace add-source menu with "Add from Library" picker**

Replace the current `add_display_source`/`add_window_source`/`add_camera_source`/`add_image_source` functions and the popup menu (lines 56-94) with an "Add from Library" popup that lists library sources not already in the current scene:

```rust
// In the popup, show library sources not already in this scene
let scene_source_ids: Vec<SourceId> = state
    .active_scene()
    .map(|s| s.source_ids())
    .unwrap_or_default();

styled_menu(ui, |ui| {
    let mut any = false;
    for lib_src in &state.library {
        if !scene_source_ids.contains(&lib_src.id) {
            any = true;
            if menu_item_icon(ui, source_icon(&lib_src.source_type), &lib_src.name) {
                // Add to scene
                if let Some(scene) = state.active_scene_mut() {
                    scene.sources.push(SceneSource {
                        source_id: lib_src.id,
                        overrides: SourceOverrides::default(),
                    });
                }
                // Start capture if needed
                start_capture_for_source(state, &cmd_tx, lib_src);
                state.selected_source_id = Some(lib_src.id);
                state.scenes_dirty = true;
                state.scenes_last_changed = std::time::Instant::now();
                ui.memory_mut(|m| m.close_popup(popup_id));
            }
        }
    }
    if !any {
        ui.label(egui::RichText::new("All sources added").color(TEXT_MUTED).size(11.0));
    }
});
```

- [ ] **Step 2: Update remove button to detach from scene only**

The remove button should remove the `SceneSource` entry from the scene but NOT delete from `state.library`. Update `remove_source()`:

```rust
fn remove_source_from_scene(
    state: &mut AppState,
    cmd_tx: &Option<tokio::sync::mpsc::Sender<GstCommand>>,
    active_id: SceneId,
    src_id: SourceId,
) {
    // Remove SceneSource from scene.
    if let Some(scene) = state.scenes.iter_mut().find(|s| s.id == active_id) {
        scene.sources.retain(|s| s.source_id != src_id);
    }
    // Stop capture if this source is no longer in the active scene.
    let still_in_scene = state
        .active_scene()
        .map(|s| s.sources.iter().any(|ss| ss.source_id == src_id))
        .unwrap_or(false);
    if !still_in_scene {
        stop_capture_for_source(cmd_tx, src_id, state);
    }
    if state.selected_source_id == Some(src_id) {
        state.selected_source_id = None;
    }
    state.scenes_dirty = true;
    state.scenes_last_changed = std::time::Instant::now();
}
```

- [ ] **Step 3: Update source list rendering for new data model**

The source list currently looks up `state.sources` by SourceId. Update to look up `state.library` instead, and use resolved properties:

```rust
let scene_sources: Vec<SceneSource> = state
    .active_scene()
    .map(|s| s.sources.clone())
    .unwrap_or_default();

for (idx, scene_src) in scene_sources.iter().enumerate() {
    let Some(lib_src) = state.find_library_source(scene_src.source_id) else { continue };
    let is_visible = scene_src.resolve_visible(lib_src);
    let source_name = lib_src.name.clone();
    let source_type = lib_src.source_type.clone();
    // ... rest of row rendering uses lib_src for icon, name
    // ... visibility toggle updates scene_src.overrides.visible
}
```

- [ ] **Step 4: Update visibility toggle to set override**

When the eye icon is clicked, set the scene override instead of modifying the library source directly:

```rust
if eye_hovered && row_response.clicked() {
    if let Some(scene) = state.active_scene_mut() {
        if let Some(ss) = scene.find_source_mut(src_id) {
            let lib = state.library.iter().find(|l| l.id == src_id);
            let current = lib.map(|l| ss.resolve_visible(l)).unwrap_or(true);
            ss.overrides.visible = Some(!current);
        }
    }
    state.scenes_dirty = true;
    state.scenes_last_changed = std::time::Instant::now();
}
```

- [ ] **Step 5: Add helper functions for capture start/stop**

Add `start_capture_for_source` and `stop_capture_for_source` helper functions that send the appropriate GstCommand based on source type:

```rust
fn start_capture_for_source(
    state: &mut AppState,
    cmd_tx: &Option<tokio::sync::mpsc::Sender<GstCommand>>,
    lib_src: &LibrarySource,
) {
    let Some(tx) = cmd_tx else { return };
    match &lib_src.properties {
        SourceProperties::Display { screen_index } => {
            let _ = tx.try_send(GstCommand::AddCaptureSource {
                source_id: lib_src.id,
                config: CaptureSourceConfig::Screen { screen_index: *screen_index },
            });
            state.capture_active = true;
        }
        SourceProperties::Window { window_id, .. } if *window_id != 0 => {
            let _ = tx.try_send(GstCommand::AddCaptureSource {
                source_id: lib_src.id,
                config: CaptureSourceConfig::Window { window_id: *window_id },
            });
            state.capture_active = true;
        }
        SourceProperties::Camera { device_index, .. } => {
            let _ = tx.try_send(GstCommand::AddCaptureSource {
                source_id: lib_src.id,
                config: CaptureSourceConfig::Camera { device_index: *device_index },
            });
            state.capture_active = true;
        }
        _ => {}
    }
}

fn stop_capture_for_source(
    cmd_tx: &Option<tokio::sync::mpsc::Sender<GstCommand>>,
    src_id: SourceId,
    state: &mut AppState,
) {
    if let Some(lib_src) = state.library.iter().find(|s| s.id == src_id) {
        let has_pipeline = matches!(
            lib_src.source_type,
            SourceType::Display | SourceType::Window | SourceType::Camera
        );
        if has_pipeline {
            if let Some(tx) = cmd_tx {
                let _ = tx.try_send(GstCommand::RemoveCaptureSource { source_id: src_id });
            }
        }
    }
}
```

- [ ] **Step 6: Delete the old `add_*_source` and `remove_source` functions**

Remove `add_display_source`, `add_window_source`, `add_camera_source`, `add_image_source`, and the old `remove_source` function entirely.

- [ ] **Step 7: Verify compilation**

Run: `cargo build`
Expected: Compiles successfully (some warnings about unused imports OK at this stage).

- [ ] **Step 8: Commit**

```bash
git add src/ui/sources_panel.rs
git commit -m "refactor: sources panel becomes scene composition tool with Add from Library"
```

---

### Task 5: Library Panel — Source CRUD and Views

**Files:**
- Create: `src/ui/library_panel.rs`
- Modify: `src/ui/mod.rs:1-30`
- Modify: `src/ui/layout/tree.rs:13-36` (PanelType enum)

- [ ] **Step 1: Add PanelType::Library variant**

In `src/ui/layout/tree.rs`, add `Library` to the `PanelType` enum and its `display_name`:

```rust
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub enum PanelType {
    Preview,
    SceneEditor,
    AudioMixer,
    StreamControls,
    Sources,
    Scenes,
    Properties,
    Library,  // NEW
}

impl PanelType {
    pub fn display_name(&self) -> &'static str {
        match self {
            // ... existing arms ...
            Self::Library => "Library",
        }
    }
}
```

- [ ] **Step 2: Register the library panel module and routing**

In `src/ui/mod.rs`, add `pub mod library_panel;` and add the routing case:

```rust
pub mod library_panel;

pub fn draw_panel(panel_type: PanelType, ui: &mut egui::Ui, state: &mut AppState, id: PanelId) {
    match panel_type {
        // ... existing arms ...
        PanelType::Library => library_panel::draw(ui, state, id),
    }
}
```

- [ ] **Step 3: Create the library panel file**

Create `src/ui/library_panel.rs` with the core structure:

```rust
//! Library panel — global source library management.
//!
//! Provides source CRUD operations and two views: "By Type" and "Folders".
//! Sources can only be created here. Drag or pick to add to scenes.

use crate::gstreamer::{CaptureSourceConfig, GstCommand};
use crate::scene::{LibrarySource, SourceId, SourceOverrides, SourceProperties, SourceType, SceneSource, Transform};
use crate::state::AppState;
use crate::ui::layout::tree::PanelId;
use crate::ui::theme::{
    BG_ELEVATED, BORDER, DEFAULT_ACCENT, RADIUS_SM, TEXT_MUTED, TEXT_PRIMARY, TEXT_SECONDARY,
    accent_dim, styled_menu, menu_item_icon,
};
use egui::{Color32, CornerRadius, Rect, Sense, Stroke, vec2};

/// Which view mode the library panel is displaying.
#[derive(Clone, Copy, PartialEq)]
enum LibraryView {
    ByType,
    Folders,
}

pub fn draw(ui: &mut egui::Ui, state: &mut AppState, _id: PanelId) {
    let cmd_tx = state.command_tx.clone();

    // Persistent view toggle state
    let view_id = ui.make_persistent_id("library_view_mode");
    let mut view: LibraryView = ui.data_mut(|d| {
        *d.get_persisted_mut_or(view_id, LibraryView::ByType)
    });

    // ── Header row: title + view toggle + add button ──
    ui.horizontal(|ui| {
        ui.colored_label(TEXT_PRIMARY, "Library");

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // Add source button + popup
            let add_resp = ui.button(egui_phosphor::regular::PLUS)
                .on_hover_text("Create source");
            let popup_id = ui.make_persistent_id("library_add_source");
            if add_resp.clicked() {
                #[allow(deprecated)]
                ui.memory_mut(|m| m.toggle_popup(popup_id));
            }
            #[allow(deprecated)]
            egui::popup_below_widget(ui, popup_id, &add_resp,
                egui::PopupCloseBehavior::CloseOnClickOutside, |ui| {
                styled_menu(ui, |ui| {
                    if menu_item_icon(ui, egui_phosphor::regular::MONITOR, "Display") {
                        add_library_source(state, SourceType::Display);
                        ui.memory_mut(|m| m.close_popup(popup_id));
                    }
                    if menu_item_icon(ui, egui_phosphor::regular::APP_WINDOW, "Window") {
                        add_library_source(state, SourceType::Window);
                        ui.memory_mut(|m| m.close_popup(popup_id));
                    }
                    if menu_item_icon(ui, egui_phosphor::regular::VIDEO_CAMERA, "Camera") {
                        add_library_source(state, SourceType::Camera);
                        ui.memory_mut(|m| m.close_popup(popup_id));
                    }
                    if menu_item_icon(ui, egui_phosphor::regular::IMAGE, "Image") {
                        add_library_source(state, SourceType::Image);
                        ui.memory_mut(|m| m.close_popup(popup_id));
                    }
                });
            });

            // View toggle buttons
            let folders_selected = view == LibraryView::Folders;
            let type_selected = view == LibraryView::ByType;
            if ui.selectable_label(folders_selected, egui_phosphor::regular::FOLDER)
                .on_hover_text("Folders view").clicked() {
                view = LibraryView::Folders;
            }
            if ui.selectable_label(type_selected, egui_phosphor::regular::LIST)
                .on_hover_text("By type view").clicked() {
                view = LibraryView::ByType;
            }
        });
    });

    ui.data_mut(|d| d.insert_persisted(view_id, view));
    ui.add_space(4.0);

    // ── Source list ──
    match view {
        LibraryView::ByType => draw_by_type(ui, state),
        LibraryView::Folders => draw_folders(ui, state),
    }
}
```

- [ ] **Step 4: Implement "By Type" view**

```rust
fn draw_by_type(ui: &mut egui::Ui, state: &mut AppState) {
    let type_order: &[(SourceType, &str, &str)] = &[
        (SourceType::Display, "Displays", egui_phosphor::regular::MONITOR),
        (SourceType::Camera, "Cameras", egui_phosphor::regular::VIDEO_CAMERA),
        (SourceType::Window, "Windows", egui_phosphor::regular::APP_WINDOW),
        (SourceType::Image, "Images", egui_phosphor::regular::IMAGE),
    ];

    let selected_bg = accent_dim(DEFAULT_ACCENT);

    egui::ScrollArea::vertical().show(ui, |ui| {
        for (stype, label, _icon) in type_order {
            let sources_of_type: Vec<&LibrarySource> = state.library.iter()
                .filter(|s| std::mem::discriminant(&s.source_type) == std::mem::discriminant(stype))
                .collect();
            if sources_of_type.is_empty() { continue; }

            // Collapsible section header
            let header_id = ui.make_persistent_id(format!("lib_type_{label}"));
            egui::CollapsingHeader::new(
                egui::RichText::new(*label).color(TEXT_MUTED).size(9.0)
            )
            .id_salt(header_id)
            .default_open(true)
            .show(ui, |ui| {
                for lib_src in &sources_of_type {
                    let usage = state.source_usage_count(lib_src.id);
                    draw_library_row(ui, state, lib_src.id, &lib_src.name,
                        &lib_src.source_type, usage, selected_bg);
                }
            });
        }
    });
}
```

- [ ] **Step 5: Implement "Folders" view**

```rust
fn draw_folders(ui: &mut egui::Ui, state: &mut AppState) {
    let selected_bg = accent_dim(DEFAULT_ACCENT);

    // Collect unique folder names
    let mut folders: Vec<String> = state.library.iter()
        .filter_map(|s| s.folder.clone())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();

    let unfiled: Vec<&LibrarySource> = state.library.iter()
        .filter(|s| s.folder.is_none())
        .collect();

    egui::ScrollArea::vertical().show(ui, |ui| {
        // User folders
        for folder in &folders {
            let sources: Vec<&LibrarySource> = state.library.iter()
                .filter(|s| s.folder.as_deref() == Some(folder))
                .collect();

            egui::CollapsingHeader::new(
                egui::RichText::new(folder).color(TEXT_MUTED).size(9.0)
            )
            .default_open(true)
            .show(ui, |ui| {
                for lib_src in &sources {
                    let usage = state.source_usage_count(lib_src.id);
                    draw_library_row(ui, state, lib_src.id, &lib_src.name,
                        &lib_src.source_type, usage, selected_bg);
                }
            });
        }

        // Unfiled section
        if !unfiled.is_empty() {
            egui::CollapsingHeader::new(
                egui::RichText::new("Unfiled").color(TEXT_MUTED).size(9.0)
            )
            .default_open(true)
            .show(ui, |ui| {
                for lib_src in &unfiled {
                    let usage = state.source_usage_count(lib_src.id);
                    draw_library_row(ui, state, lib_src.id, &lib_src.name,
                        &lib_src.source_type, usage, selected_bg);
                }
            });
        }

        // New Folder button
        ui.add_space(8.0);
        // (folder creation UI — future enhancement, placeholder for now)
    });
}
```

- [ ] **Step 6: Implement shared row rendering**

```rust
/// Shared source icon helper (re-exported from sources_panel or duplicated).
fn source_icon(source_type: &SourceType) -> &'static str {
    match source_type {
        SourceType::Display => egui_phosphor::regular::MONITOR,
        SourceType::Camera => egui_phosphor::regular::VIDEO_CAMERA,
        SourceType::Image => egui_phosphor::regular::IMAGE,
        SourceType::Browser => egui_phosphor::regular::BROWSER,
        SourceType::Audio => egui_phosphor::regular::SPEAKER_HIGH,
        SourceType::Window => egui_phosphor::regular::APP_WINDOW,
    }
}

fn draw_library_row(
    ui: &mut egui::Ui,
    state: &mut AppState,
    src_id: SourceId,
    name: &str,
    source_type: &SourceType,
    usage_count: usize,
    selected_bg: Color32,
) {
    let is_selected = state.selected_source_id == Some(src_id);
    let row_height = 28.0;
    let available_width = ui.available_width();
    let (row_rect, row_response) =
        ui.allocate_exact_size(vec2(available_width, row_height), Sense::click());

    if is_selected {
        ui.painter().rect_filled(
            row_rect,
            CornerRadius::same(RADIUS_SM as u8),
            selected_bg,
        );
    }

    if row_response.clicked() {
        state.selected_source_id = Some(src_id);
    }

    let painter = ui.painter_at(row_rect);
    let mut cursor_x = row_rect.left() + 4.0;
    let center_y = row_rect.center().y;

    // Icon
    let icon_size = 16.0;
    let icon_rect = Rect::from_center_size(
        egui::pos2(cursor_x + icon_size / 2.0, center_y),
        vec2(icon_size, icon_size),
    );
    painter.rect_filled(icon_rect, CornerRadius::same(RADIUS_SM as u8), BG_ELEVATED);
    painter.text(
        icon_rect.center(),
        egui::Align2::CENTER_CENTER,
        source_icon(source_type),
        egui::FontId::proportional(10.0),
        TEXT_PRIMARY,
    );
    cursor_x += icon_size + 6.0;

    // Name
    painter.text(
        egui::pos2(cursor_x, center_y),
        egui::Align2::LEFT_CENTER,
        name,
        egui::FontId::proportional(11.0),
        TEXT_PRIMARY,
    );

    // Usage count badge (right-aligned)
    if usage_count > 0 {
        let badge_text = format!("{usage_count}");
        let right_x = row_rect.right() - 8.0;
        painter.text(
            egui::pos2(right_x, center_y),
            egui::Align2::RIGHT_CENTER,
            &badge_text,
            egui::FontId::proportional(9.0),
            TEXT_MUTED,
        );
    }

    // Context menu: Rename, Delete
    row_response.context_menu(|ui| {
        if ui.button("Delete").clicked() {
            // Cascade delete with confirmation would go here.
            // For now, direct delete.
            let scene_names = state.scenes_using_source(src_id);
            if scene_names.is_empty() {
                state.library.retain(|s| s.id != src_id);
            } else {
                // Remove from all scenes, then library
                for scene in &mut state.scenes {
                    scene.sources.retain(|s| s.source_id != src_id);
                }
                state.library.retain(|s| s.id != src_id);
                if let Some(tx) = &state.command_tx {
                    let _ = tx.try_send(GstCommand::RemoveCaptureSource { source_id: src_id });
                }
            }
            if state.selected_source_id == Some(src_id) {
                state.selected_source_id = None;
            }
            state.scenes_dirty = true;
            state.scenes_last_changed = std::time::Instant::now();
            ui.close();
        }
    });
}
```

- [ ] **Step 7: Implement `add_library_source`**

```rust
fn add_library_source(state: &mut AppState, source_type: SourceType) {
    let new_id = SourceId(state.next_source_id);
    state.next_source_id += 1;

    let (name, properties) = match source_type {
        SourceType::Display => {
            let count = state.library.iter().filter(|s| matches!(s.source_type, SourceType::Display)).count();
            (format!("Display {}", count + 1), SourceProperties::Display { screen_index: 0 })
        }
        SourceType::Window => {
            let count = state.library.iter().filter(|s| matches!(s.source_type, SourceType::Window)).count();
            (format!("Window {}", count + 1), SourceProperties::Window {
                window_id: 0, window_title: String::new(), owner_name: String::new(),
            })
        }
        SourceType::Camera => {
            let count = state.library.iter().filter(|s| matches!(s.source_type, SourceType::Camera)).count();
            (format!("Camera {}", count + 1), SourceProperties::Camera {
                device_index: 0, device_name: String::new(),
            })
        }
        SourceType::Image => {
            let count = state.library.iter().filter(|s| matches!(s.source_type, SourceType::Image)).count();
            (format!("Image {}", count + 1), SourceProperties::Image { path: String::new() })
        }
        _ => ("Source".to_string(), SourceProperties::default()),
    };

    let lib_source = LibrarySource {
        id: new_id,
        name,
        source_type,
        properties,
        folder: None,
        transform: Transform::new(0.0, 0.0, 1920.0, 1080.0),
        native_size: (1920.0, 1080.0),
        opacity: 1.0,
        visible: true,
        muted: false,
        volume: 1.0,
    };

    state.library.push(lib_source);
    state.selected_source_id = Some(new_id);
    state.scenes_dirty = true;
    state.scenes_last_changed = std::time::Instant::now();
}
```

- [ ] **Step 8: Update default layout to include Library panel**

In `src/ui/layout/tree.rs`, update `default_layout()` to add the Library panel. Add it as a tab alongside Sources in the left sidebar:

```rust
// In default_layout(), after creating sources_group:
let library_group = Group::new(PanelType::Library);
let library_gid = library_group.id;
layout.groups.insert(library_gid, library_group);
// Add Library as a tab in the sources group, or as a separate panel above Sources
```

- [ ] **Step 9: Verify compilation and basic functionality**

Run: `cargo build`
Expected: Compiles. The Library panel should render with source creation and listing.

- [ ] **Step 10: Commit**

```bash
git add src/ui/library_panel.rs src/ui/mod.rs src/ui/layout/tree.rs
git commit -m "feat: add Library panel with source CRUD and By Type / Folders views"
```

---

### Task 6: Properties Panel — Override Indicators and Dual Mode

**Files:**
- Modify: `src/ui/properties_panel.rs:14-383`

- [ ] **Step 1: Determine editing context**

The properties panel needs to know whether it's editing a library source directly or a scene source with overrides. Add context detection at the top of `draw()`:

```rust
pub fn draw(ui: &mut egui::Ui, state: &mut AppState, _id: PanelId) {
    let Some(selected_id) = state.selected_source_id else {
        // ... empty state unchanged
        return;
    };

    let Some(lib_idx) = state.library.iter().position(|s| s.id == selected_id) else {
        ui.label(egui::RichText::new("Source not found").color(TEXT_MUTED).size(11.0));
        return;
    };

    // Check if this source is in the active scene (scene override mode)
    // vs selected from the library panel (library defaults mode).
    let active_scene_id = state.active_scene_id;
    let scene_source: Option<SceneSource> = active_scene_id.and_then(|sid| {
        state.scenes.iter()
            .find(|s| s.id == sid)
            .and_then(|s| s.find_source(selected_id).cloned())
    });
    let is_scene_mode = scene_source.is_some();

    // Header
    if is_scene_mode {
        let scene_name = state.active_scene().map(|s| s.name.clone()).unwrap_or_default();
        section_label(ui, &format!("SCENE OVERRIDE — {}", scene_name.to_uppercase()));
    } else {
        section_label(ui, "LIBRARY DEFAULTS");
    }
    ui.add_space(4.0);

    // ... render fields with override indicators
}
```

- [ ] **Step 2: Add override dot indicator helper**

```rust
/// Draw an override indicator dot. Returns true if the user right-clicked to reset.
fn override_dot(ui: &mut egui::Ui, is_overridden: bool) -> bool {
    let size = 6.0;
    let (rect, response) = ui.allocate_exact_size(vec2(size, size), Sense::click());

    if is_overridden {
        ui.painter().circle_filled(
            rect.center(),
            size / 2.0,
            DEFAULT_ACCENT,
        );
    }

    // Right-click to reset
    let mut reset = false;
    if is_overridden {
        response.context_menu(|ui| {
            if ui.button("Reset to library default").clicked() {
                reset = true;
                ui.close();
            }
        });
    }

    reset
}
```

- [ ] **Step 3: Refactor transform fields with override support**

Replace the direct `source.transform.x` mutation with a pattern that:
- In library mode: edits `state.library[lib_idx].transform` directly
- In scene mode: edits the scene override, creating it on first edit

```rust
// TRANSFORM section
section_label(ui, "TRANSFORM");
ui.add_space(4.0);

if let Some(ref scene_src) = scene_source {
    // Scene mode — show with override dots
    let lib = &state.library[lib_idx];
    let mut transform = scene_src.resolve_transform(lib);
    let is_overridden = scene_src.is_transform_overridden();

    ui.horizontal(|ui| {
        let reset = override_dot(ui, is_overridden);
        if reset {
            if let Some(scene) = state.active_scene_mut() {
                if let Some(ss) = scene.find_source_mut(selected_id) {
                    ss.overrides.transform = None;
                }
            }
            changed = true;
        }
        if drag_field(ui, "X", &mut transform.x) {
            set_transform_override(state, selected_id, transform);
            changed = true;
        }
        ui.add_space(8.0);
        if drag_field(ui, "Y", &mut transform.y) {
            set_transform_override(state, selected_id, transform);
            changed = true;
        }
    });
    // ... W/H row similar
} else {
    // Library mode — direct edit, no dots
    let source = &mut state.library[lib_idx];
    ui.horizontal(|ui| {
        changed |= drag_field(ui, "X", &mut source.transform.x);
        ui.add_space(8.0);
        changed |= drag_field(ui, "Y", &mut source.transform.y);
    });
    // ... W/H row similar
}
```

- [ ] **Step 4: Refactor opacity with override support**

Same pattern: in scene mode, show the dot and edit the override. In library mode, edit directly.

- [ ] **Step 5: Refactor source-specific properties**

Source-specific properties (Display monitor, Camera device, Window selection, Image path) always edit the library source directly — these are device config, not per-scene overrides. Keep the existing logic but change `state.sources[source_idx]` references to `state.library[lib_idx]`.

- [ ] **Step 6: Add helper for setting transform override**

```rust
fn set_transform_override(state: &mut AppState, source_id: SourceId, transform: Transform) {
    if let Some(scene) = state.active_scene_mut() {
        if let Some(ss) = scene.find_source_mut(source_id) {
            ss.overrides.transform = Some(transform);
        }
    }
}
```

- [ ] **Step 7: Use dimmer text for inherited values**

When rendering field values in scene mode, use `TEXT_MUTED` for inherited values and `TEXT_PRIMARY` for overridden values:

```rust
let text_color = if is_overridden { TEXT_PRIMARY } else { TEXT_MUTED };
```

Apply this to the DragValue and Slider widgets.

- [ ] **Step 8: Verify compilation and test**

Run: `cargo build`
Expected: Compiles. Properties panel renders correctly in both modes.

- [ ] **Step 9: Commit**

```bash
git add src/ui/properties_panel.rs
git commit -m "feat: properties panel override indicators with library/scene dual mode"
```

---

### Task 7: Scenes Panel — Update for New Data Model

**Files:**
- Modify: `src/ui/scenes_panel.rs:225-374`

- [ ] **Step 1: Update `apply_scene_diff` for `SceneSource`**

The function currently takes `&[Source]` and compares `SourceId` sets. Update to work with `&[LibrarySource]` and `Vec<SceneSource>`:

```rust
fn apply_scene_diff(
    cmd_tx: &Option<tokio::sync::mpsc::Sender<GstCommand>>,
    library: &[LibrarySource],
    old_scene: Option<&Scene>,
    new_scene: Option<&Scene>,
) {
    let Some(tx) = cmd_tx else { return };

    let old_ids: std::collections::HashSet<SourceId> = old_scene
        .map(|s| s.sources.iter().map(|ss| ss.source_id).collect())
        .unwrap_or_default();
    let new_ids: std::collections::HashSet<SourceId> = new_scene
        .map(|s| s.sources.iter().map(|ss| ss.source_id).collect())
        .unwrap_or_default();

    for &src_id in old_ids.difference(&new_ids) {
        let _ = tx.try_send(GstCommand::RemoveCaptureSource { source_id: src_id });
    }

    for &src_id in new_ids.difference(&old_ids) {
        if let Some(source) = library.iter().find(|s| s.id == src_id) {
            // ... same match on source.properties as before
        }
    }
}
```

- [ ] **Step 2: Update call sites**

Update the call to `apply_scene_diff` in `draw()` (around line 158) to pass `&state.library` instead of `&state.sources`.

Update `send_capture_for_scene` similarly.

- [ ] **Step 3: Update `delete_scene_by_id`**

The current function deletes all sources owned by the scene. In the new model, deleting a scene should NOT delete library sources — just remove the scene:

```rust
fn delete_scene_by_id(
    state: &mut AppState,
    cmd_tx: &Option<tokio::sync::mpsc::Sender<GstCommand>>,
    scene_id: SceneId,
) {
    if state.scenes.len() <= 1 {
        let new_id = SceneId(state.next_scene_id);
        state.next_scene_id += 1;
        state.scenes.push(Scene {
            id: new_id,
            name: "Scene 1".to_string(),
            sources: Vec::new(),
        });
    }

    // Stop captures for sources in the deleted scene.
    if let Some(scene) = state.scenes.iter().find(|s| s.id == scene_id) {
        for ss in &scene.sources {
            if let Some(tx) = cmd_tx {
                let _ = tx.try_send(GstCommand::RemoveCaptureSource { source_id: ss.source_id });
            }
        }
    }
    // NOTE: Do NOT delete from state.library — sources persist in the library.

    state.scenes.retain(|s| s.id != scene_id);
    state.selected_source_id = None;

    let first_scene = state.scenes.first().cloned();
    if let Some(ref scene) = first_scene {
        state.active_scene_id = Some(scene.id);
        send_capture_for_scene(cmd_tx, &state.library, scene);
        state.capture_active = !scene.sources.is_empty();
    } else {
        state.active_scene_id = None;
        state.capture_active = false;
    }

    state.scenes_dirty = true;
    state.scenes_last_changed = std::time::Instant::now();
}
```

- [ ] **Step 4: Update `send_capture_for_scene`**

Change parameter type from `&[Source]` to `&[LibrarySource]`:

```rust
fn send_capture_for_scene(
    cmd_tx: &Option<tokio::sync::mpsc::Sender<GstCommand>>,
    library: &[LibrarySource],
    scene: &Scene,
) {
    let Some(tx) = cmd_tx else { return };
    let mut any_started = false;
    for ss in &scene.sources {
        if let Some(source) = library.iter().find(|s| s.id == ss.source_id) {
            // ... same match arms as before, using source.properties
        }
    }
    if !any_started {
        let _ = tx.try_send(GstCommand::StopCapture);
    }
}
```

- [ ] **Step 5: Verify compilation**

Run: `cargo build`
Expected: Compiles successfully.

- [ ] **Step 6: Commit**

```bash
git add src/ui/scenes_panel.rs
git commit -m "refactor: scenes panel uses library sources, scene delete preserves library"
```

---

### Task 8: Compositor and Render Integration

**Files:**
- Modify: `src/renderer/compositor.rs:624+` (compose function)
- Modify: any file that calls `compose()` with `&[&Source]`

- [ ] **Step 1: Find all call sites of `compose()`**

Search for `compose(` across the codebase to find where `&[&Source]` is passed. Update the signature or the call sites to pass resolved source data.

The compositor needs: `source_id`, `transform`, `opacity`, `visible` for each source. These are now resolved from `SceneSource` + `LibrarySource`.

- [ ] **Step 2: Create a resolved source struct for the compositor**

Rather than changing the compositor API significantly, create a lightweight struct that the render loop builds from resolved data:

```rust
/// Resolved source data for rendering. Built from LibrarySource + SceneSource overrides.
pub struct ResolvedSource {
    pub id: SourceId,
    pub transform: Transform,
    pub opacity: f32,
    pub visible: bool,
}
```

Or, simpler: keep the old `Source` type as a transient struct used only for compositor calls. But since we removed it, the cleanest approach is to update `compose()` to accept `&[ResolvedSource]` or similar.

- [ ] **Step 3: Update compose() signature and implementation**

Update `compose()` to work with the new data. The key fields it reads are `source.visible`, `source.opacity`, `source.transform`, and `source.id`. Pass these as a resolved struct or update the render loop to build resolved data before calling compose.

- [ ] **Step 4: Update the render loop**

In the render loop (wherever compose is called), build the resolved source list:

```rust
let active_scene = state.active_scene();
let resolved: Vec<ResolvedSource> = active_scene
    .map(|scene| {
        scene.sources.iter().filter_map(|ss| {
            state.find_library_source(ss.source_id).map(|lib| {
                ResolvedSource {
                    id: lib.id,
                    transform: ss.resolve_transform(lib),
                    opacity: ss.resolve_opacity(lib),
                    visible: ss.resolve_visible(lib),
                }
            })
        }).collect()
    })
    .unwrap_or_default();
```

- [ ] **Step 5: Verify compilation and rendering**

Run: `cargo build && cargo run`
Expected: Sources render correctly on the canvas with correct position/size/opacity.

- [ ] **Step 6: Commit**

```bash
git add src/renderer/
git commit -m "refactor: compositor uses resolved source data from library + scene overrides"
```

---

### Task 9: Transform Handles Integration

**Files:**
- Modify: `src/ui/transform_handles.rs`

- [ ] **Step 1: Update transform handle interactions**

Transform handles currently read/write `source.transform` directly. Update to:
- Read the resolved transform (from scene override or library default)
- Write back as a scene override (not to the library source)

Find all references to `state.sources` in transform_handles.rs and update to use `state.library` for reading, and scene overrides for writing:

```rust
// When reading transform for rendering handles:
let lib_src = state.find_library_source(src_id);
let scene_src = state.active_scene().and_then(|s| s.find_source(src_id));
let transform = match (scene_src, lib_src) {
    (Some(ss), Some(lib)) => ss.resolve_transform(lib),
    (None, Some(lib)) => lib.transform,
    _ => continue,
};

// When writing transform after drag:
if let Some(scene) = state.active_scene_mut() {
    if let Some(ss) = scene.find_source_mut(src_id) {
        ss.overrides.transform = Some(new_transform);
    }
}
```

- [ ] **Step 2: Update context menu source references**

The `show_source_context_menu` function accesses source data. Update to use `state.library` for reading and scene overrides for mutations.

- [ ] **Step 3: Verify compilation**

Run: `cargo build`
Expected: Compiles. Transform handles work correctly.

- [ ] **Step 4: Commit**

```bash
git add src/ui/transform_handles.rs
git commit -m "refactor: transform handles read/write scene overrides instead of library source"
```

---

### Task 10: Persistence — Save/Load with New Format

**Files:**
- Modify: `src/main.rs` (scene save logic)

- [ ] **Step 1: Find the save logic**

Search for where `SceneCollection` is built and `save_to()` is called. Update to build from the new AppState fields:

```rust
let collection = SceneCollection {
    library: state.library.clone(),
    scenes: state.scenes.clone(),
    active_scene_id: state.active_scene_id,
    next_scene_id: state.next_scene_id,
    next_source_id: state.next_source_id,
};
collection.save_to(&scenes_path)?;
```

- [ ] **Step 2: Verify save/load roundtrip**

Run: `cargo run`
- Create a source in the library
- Add it to a scene
- Override a property
- Close and reopen
- Expected: Source persists in library, override persists in scene

- [ ] **Step 3: Commit**

```bash
git add src/main.rs
git commit -m "fix: persistence uses new library/SceneSource format with overrides"
```

---

### Task 11: Final Integration Test and Cleanup

**Files:**
- All modified files

- [ ] **Step 1: Run full test suite**

Run: `cargo test`
Expected: ALL PASS

- [ ] **Step 2: Run clippy**

Run: `cargo clippy`
Expected: No errors. Fix any new warnings introduced by the changes.

- [ ] **Step 3: Run fmt check**

Run: `cargo fmt --check`
Expected: No formatting issues. Run `cargo fmt` if needed.

- [ ] **Step 4: Remove dead code**

Remove any unused imports, functions, or types left over from the refactor (e.g., the old `Source` type if it's still lingering, unused `with_opacity` helper if moved).

- [ ] **Step 5: Smoke test the full workflow**

Run: `cargo run`
Test the complete workflow:
1. Open Library panel — create Display, Camera, Image sources
2. Open Sources panel — "Add from Library" adds sources to scene
3. Reorder sources in scene
4. Select scene source → Properties panel shows override dots
5. Edit transform → dot appears → right-click to reset
6. Select library source → Properties panel shows "Library Defaults"
7. Create second scene → add same source → different transform
8. Switch between scenes → captures start/stop correctly
9. Delete source from library → cascades to all scenes
10. Close and reopen → everything persists

- [ ] **Step 6: Final commit**

```bash
git add -A
git commit -m "chore: cleanup dead code and fix warnings after source library refactor"
```
