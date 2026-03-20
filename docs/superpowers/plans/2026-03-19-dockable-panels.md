# Dockable Panel System Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace Lodestone's fixed panel layout with a Blender-style tiling window manager — split, merge, resize, detach to OS windows, any area can become any panel type.

**Architecture:** A binary split tree (`LayoutTree`) manages panel placement within each window. Each OS window owns its own tree and `egui::Context`. All windows share one `wgpu::Device`/`Queue`, one `AppState`, and one set of GPU pipelines (`SharedGpuState`). Panel draw functions take `&mut egui::Ui` regions allocated by the tree traversal.

**Tech Stack:** Rust, winit 0.30 (multi-window), wgpu (via `egui_wgpu::wgpu` re-export — use this consistently, not the direct `wgpu` crate), egui 0.33, serde + toml for layout persistence

**Spec:** `docs/superpowers/specs/2026-03-19-dockable-panels-design.md`

---

## File Structure

```text
src/
├── main.rs                      ← AppManager with multi-window event routing
├── window.rs                    ← WindowState, SharedGpuState, window lifecycle
├── state.rs                     ← AppState (UiState removed)
├── renderer/
│   ├── mod.rs                   ← SharedGpuState construction (extracted from Renderer)
│   ├── pipelines.rs             ← unchanged
│   ├── text.rs                  ← unchanged
│   └── preview.rs               ← unchanged
├── ui/
│   ├── mod.rs                   ← panel registry, draw_panel dispatch
│   ├── layout/
│   │   ├── mod.rs               ← LayoutTree, LayoutNode, PanelType, PanelId
│   │   ├── tree.rs              ← tree operations: split, merge, resize, find, collect_leaves
│   │   ├── render.rs            ← tree traversal → egui rect allocation, divider + header drawing
│   │   ├── interactions.rs      ← corner drag (split/merge), divider drag (resize), header clicks
│   │   └── serialize.rs         ← TOML serialization/deserialization
│   ├── scene_editor.rs          ← refactored: draw(ui, state, panel_id)
│   ├── audio_mixer.rs           ← refactored: draw(ui, state, panel_id)
│   ├── stream_controls.rs       ← refactored: draw(ui, state, panel_id)
│   ├── settings_panel.rs        ← renamed from settings_modal.rs, refactored
│   └── preview_panel.rs         ← new: preview as a panel
```

---

### Task 1: Layout Tree Data Structure

**Files:**
- Create: `src/ui/layout/mod.rs`
- Create: `src/ui/layout/tree.rs`

The foundational data structure — pure logic, no GPU, fully testable.

- [ ] **Step 1: Write tests for tree types and operations**

In `src/ui/layout/tree.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_leaf() {
        let node = LayoutNode::leaf(PanelType::Preview);
        assert!(node.is_leaf());
        assert_eq!(node.panel_type(), Some(PanelType::Preview));
    }

    #[test]
    fn split_leaf_vertical() {
        let mut tree = LayoutTree::new(PanelType::Preview);
        let root_id = tree.root_id();
        tree.split(root_id, SplitDirection::Vertical, 0.5);
        assert!(!tree.node(root_id).unwrap().is_leaf());
        let leaves = tree.collect_leaves();
        assert_eq!(leaves.len(), 2);
    }

    #[test]
    fn split_leaf_horizontal() {
        let mut tree = LayoutTree::new(PanelType::SceneEditor);
        let root_id = tree.root_id();
        tree.split(root_id, SplitDirection::Horizontal, 0.3);
        let leaves = tree.collect_leaves();
        assert_eq!(leaves.len(), 2);
        // Both should be SceneEditor (new panel copies type)
        for (_, panel_type, _) in &leaves {
            assert_eq!(*panel_type, PanelType::SceneEditor);
        }
    }

    #[test]
    fn merge_collapses_split() {
        let mut tree = LayoutTree::new(PanelType::Preview);
        let root_id = tree.root_id();
        tree.split(root_id, SplitDirection::Vertical, 0.5);
        let leaves_before = tree.collect_leaves();
        assert_eq!(leaves_before.len(), 2);

        // Merge: keep the first child
        tree.merge(root_id, MergeSide::First);
        assert!(tree.node(tree.root_id()).unwrap().is_leaf());
    }

    #[test]
    fn resize_clamps_ratio() {
        let mut tree = LayoutTree::new(PanelType::Preview);
        let root_id = tree.root_id();
        tree.split(root_id, SplitDirection::Vertical, 0.5);
        tree.resize(root_id, 0.95);
        let node = tree.node(root_id).unwrap();
        // Should be clamped so neither child is below minimum
        assert!(node.ratio().unwrap() <= 0.9);
    }

    #[test]
    fn swap_panel_type() {
        let mut tree = LayoutTree::new(PanelType::Preview);
        let root_id = tree.root_id();
        tree.swap_type(root_id, PanelType::AudioMixer);
        assert_eq!(tree.node(root_id).unwrap().panel_type(), Some(PanelType::AudioMixer));
    }

    #[test]
    fn collect_leaves_with_rects() {
        let mut tree = LayoutTree::new(PanelType::Preview);
        let root_id = tree.root_id();
        tree.split(root_id, SplitDirection::Vertical, 0.3);

        let total_rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1000.0, 600.0));
        let leaves = tree.collect_leaves_with_rects(total_rect);
        assert_eq!(leaves.len(), 2);

        // First leaf should be ~300px wide, second ~700px
        let (_, _, rect1) = &leaves[0];
        let (_, _, rect2) = &leaves[1];
        assert!((rect1.width() - 300.0).abs() < 1.0);
        assert!((rect2.width() - 700.0).abs() < 1.0);
    }

    #[test]
    fn panel_id_auto_increments() {
        let tree = LayoutTree::new(PanelType::Preview);
        let first_id = tree.collect_leaves()[0].0;
        let tree2 = LayoutTree::new(PanelType::SceneEditor);
        let second_id = tree2.collect_leaves()[0].0;
        assert_ne!(first_id, second_id);
    }

    #[test]
    fn default_layout() {
        let tree = LayoutTree::default_layout();
        let leaves = tree.collect_leaves();
        assert_eq!(leaves.len(), 5); // SceneEditor, Settings, Preview, AudioMixer, StreamControls
        let types: Vec<PanelType> = leaves.iter().map(|(_, t, _)| *t).collect();
        assert!(types.contains(&PanelType::SceneEditor));
        assert!(types.contains(&PanelType::Preview));
        assert!(types.contains(&PanelType::AudioMixer));
        assert!(types.contains(&PanelType::StreamControls));
        assert!(types.contains(&PanelType::Settings));
    }

    #[test]
    fn remove_leaf_collapses_parent() {
        let mut tree = LayoutTree::new(PanelType::Preview);
        let root_id = tree.root_id();
        tree.split(root_id, SplitDirection::Vertical, 0.5);
        let leaves = tree.collect_leaves();
        assert_eq!(leaves.len(), 2);

        // Remove one leaf — tree should collapse back to a single leaf
        let removed_node = leaves[1].2; // NodeId of second leaf
        let removed = tree.remove_leaf(removed_node);
        assert!(removed.is_some());
        assert_eq!(tree.collect_leaves().len(), 1);
    }

    #[test]
    fn remove_last_leaf_returns_none() {
        let mut tree = LayoutTree::new(PanelType::Preview);
        let root_id = tree.root_id();
        // Cannot remove the last leaf
        assert!(tree.remove_leaf(root_id).is_none());
    }

    #[test]
    fn insert_at_root_splits_existing() {
        let mut tree = LayoutTree::new(PanelType::Preview);
        assert_eq!(tree.collect_leaves().len(), 1);

        tree.insert_at_root(PanelType::AudioMixer, PanelId::next(), SplitDirection::Vertical, 0.5);
        let leaves = tree.collect_leaves();
        assert_eq!(leaves.len(), 2);
        let types: Vec<PanelType> = leaves.iter().map(|(_, t, _)| *t).collect();
        assert!(types.contains(&PanelType::Preview));
        assert!(types.contains(&PanelType::AudioMixer));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test`
Expected: FAIL — modules don't exist

- [ ] **Step 3: Implement types and tree operations**

Create `src/ui/layout/mod.rs`:

```rust
pub mod interactions;
pub mod render;
pub mod serialize;
pub mod tree;

pub use tree::{
    LayoutNode, LayoutTree, MergeSide, NodeId, PanelId, PanelType, SplitDirection,
};
```

Create `src/ui/layout/tree.rs` with:

- `PanelType` enum: `Preview, SceneEditor, AudioMixer, StreamControls, Settings` — derive `Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize`
- `PanelId(u64)` — derive `Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize`
- Global `AtomicU64` counter for PanelId allocation, with `PanelId::next()` class method
- `SplitDirection` enum: `Horizontal, Vertical`
- `MergeSide` enum: `First, Second`
- `NodeId(u64)` — internal tree node identifier
- `LayoutNode` enum: `Leaf { panel_type, panel_id }` or `Split { direction, ratio, first: NodeId, second: NodeId }`
- `LayoutTree` struct: `HashMap<NodeId, LayoutNode>`, `root: NodeId`, next_node_id counter
- Methods: `new(PanelType)`, `root_id()`, `node(&self, NodeId)`, `split(NodeId, SplitDirection, f32)`, `merge(NodeId, MergeSide)`, `resize(NodeId, f32)`, `swap_type(NodeId, PanelType)`, `collect_leaves() -> Vec<(PanelId, PanelType, NodeId)>`, `collect_leaves_with_rects(Rect) -> Vec<(PanelId, PanelType, Rect)>`, `default_layout()`, `remove_leaf(NodeId) -> Option<(PanelType, PanelId)>`, `insert_at_root(PanelType, PanelId, SplitDirection, f32)`

The `collect_leaves_with_rects` method recursively traverses the tree, splitting the provided rect according to each split node's direction and ratio, and returns the leaf rects.

`resize` should clamp the ratio so neither child is smaller than a minimum fraction (e.g., 0.1 = 10% of available space).

Create placeholder files (empty, just comments):
- `src/ui/layout/render.rs`
- `src/ui/layout/interactions.rs`
- `src/ui/layout/serialize.rs`

- [ ] **Step 4: Add `mod layout;` to `src/ui/mod.rs`**

- [ ] **Step 5: Run tests**

Run: `cargo test`
Expected: all new tree tests pass, all 28 existing tests pass

- [ ] **Step 6: Commit**

```bash
git add src/ui/layout/
git commit -m "Add layout tree data structure with split/merge/resize operations"
```

---

### Task 2: Layout Serialization

**Files:**
- Create: `src/ui/layout/serialize.rs` (replace placeholder)

- [ ] **Step 1: Write serialization tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_leaf_roundtrip() {
        let tree = LayoutTree::new(PanelType::Preview);
        let toml_str = serialize_layout(&tree).unwrap();
        let restored = deserialize_layout(&toml_str).unwrap();
        let leaves = restored.collect_leaves();
        assert_eq!(leaves.len(), 1);
        assert_eq!(leaves[0].1, PanelType::Preview);
    }

    #[test]
    fn split_tree_roundtrip() {
        let mut tree = LayoutTree::new(PanelType::Preview);
        let root = tree.root_id();
        tree.split(root, SplitDirection::Vertical, 0.3);
        let toml_str = serialize_layout(&tree).unwrap();
        let restored = deserialize_layout(&toml_str).unwrap();
        let leaves = restored.collect_leaves();
        assert_eq!(leaves.len(), 2);
    }

    #[test]
    fn default_layout_roundtrip() {
        let tree = LayoutTree::default_layout();
        let toml_str = serialize_layout(&tree).unwrap();
        let restored = deserialize_layout(&toml_str).unwrap();
        assert_eq!(restored.collect_leaves().len(), 5);
    }

    #[test]
    fn panel_ids_preserved() {
        let tree = LayoutTree::new(PanelType::SceneEditor);
        let original_id = tree.collect_leaves()[0].0;
        let toml_str = serialize_layout(&tree).unwrap();
        let restored = deserialize_layout(&toml_str).unwrap();
        let restored_id = restored.collect_leaves()[0].0;
        assert_eq!(original_id, restored_id);
    }

    #[test]
    fn invalid_toml_returns_error() {
        assert!(deserialize_layout("not valid toml {{{}}}").is_err());
    }
}
```

- [ ] **Step 2: Implement serialization**

Use serde with a recursive enum for the TOML representation:

```rust
#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
enum SerializedNode {
    #[serde(rename = "leaf")]
    Leaf { panel: PanelType, id: u64 },
    #[serde(rename = "split")]
    Split {
        direction: SplitDirection,
        ratio: f32,
        first: Box<SerializedNode>,
        second: Box<SerializedNode>,
    },
}
```

Functions:
- `pub fn serialize_layout(tree: &LayoutTree) -> Result<String>` — converts tree to `SerializedNode`, then `toml::to_string_pretty`
- `pub fn deserialize_layout(toml_str: &str) -> Result<LayoutTree>` — parses TOML, rebuilds tree, sets PanelId counter to `max(ids) + 1`

- [ ] **Step 3: Run tests**

Run: `cargo test`
Expected: all serialization tests pass

- [ ] **Step 4: Commit**

```bash
git add src/ui/layout/serialize.rs
git commit -m "Add layout tree TOML serialization and deserialization"
```

---

### Task 3: Refactor Panel Draw Signatures

**Files:**
- Modify: `src/ui/scene_editor.rs`
- Modify: `src/ui/audio_mixer.rs`
- Modify: `src/ui/stream_controls.rs`
- Rename: `src/ui/settings_modal.rs` → `src/ui/settings_panel.rs`
- Create: `src/ui/preview_panel.rs`
- Modify: `src/ui/mod.rs`
- Modify: `src/state.rs` (remove UiState)

Change all panel draw functions from `fn draw(ctx: &egui::Context, state: &mut AppState)` to `fn draw(ui: &mut egui::Ui, state: &mut AppState, panel_id: PanelId)`.

- [ ] **Step 1: Refactor scene_editor.rs**

Remove `SidePanel::left()` wrapper. The function receives a `&mut egui::Ui` already sized by the layout tree. Replace `egui::SidePanel::left("scene_editor").exact_width(220.0).show(ctx, |ui| { ... })` with the inner content directly using the provided `ui`. Use `panel_id` for egui ID scoping instead of hardcoded strings.

New signature: `pub fn draw(ui: &mut egui::Ui, state: &mut AppState, panel_id: PanelId)`

Remove the `if !state.ui_state.scene_panel_open { return; }` guard — the layout tree controls visibility.

- [ ] **Step 2: Refactor audio_mixer.rs**

Same pattern: remove `TopBottomPanel::bottom()` wrapper, draw directly into `ui`. Remove visibility guard.

- [ ] **Step 3: Refactor stream_controls.rs**

Remove `egui::Window::new("Stream Controls")` wrapper. Draw directly into `ui`. Remove visibility guard.

- [ ] **Step 4: Rename settings_modal.rs → settings_panel.rs**

Rename the file and refactor from modal-style `egui::Window` to drawing directly into `ui`. Remove the `settings_modal_open` guard.

- [ ] **Step 5: Create preview_panel.rs**

A new panel that displays the preview texture. For now, just show a placeholder label "Preview" — the actual texture rendering integration comes in a later task when we wire up `SharedGpuState`.

```rust
use crate::state::AppState;
use crate::ui::layout::PanelId;

pub fn draw(ui: &mut egui::Ui, _state: &mut AppState, _panel_id: PanelId) {
    ui.centered_and_justified(|ui| {
        ui.label("Preview");
    });
}
```

- [ ] **Step 6: Update ui/mod.rs with panel registry**

Replace `UiRoot` with a `draw_panel` dispatch function:

```rust
pub mod audio_mixer;
pub mod layout;
pub mod preview_panel;
pub mod scene_editor;
pub mod settings_panel;
pub mod stream_controls;

use crate::state::AppState;
use layout::{PanelId, PanelType};

pub fn draw_panel(panel_type: PanelType, ui: &mut egui::Ui, state: &mut AppState, id: PanelId) {
    match panel_type {
        PanelType::Preview => preview_panel::draw(ui, state, id),
        PanelType::SceneEditor => scene_editor::draw(ui, state, id),
        PanelType::AudioMixer => audio_mixer::draw(ui, state, id),
        PanelType::StreamControls => stream_controls::draw(ui, state, id),
        PanelType::Settings => settings_panel::draw(ui, state, id),
    }
}
```

- [ ] **Step 7: Remove UiState from state.rs**

Remove `UiState` struct and the `ui_state` field from `AppState`. Remove the `settings_modal_open` field. Update `AppState::default()`. Update the `default_app_state` test (remove `ui_state` assertions).

- [ ] **Step 8: Update main.rs temporarily**

The old `UiRoot::run()` no longer exists. Temporarily replace the UI rendering in `RedrawRequested` with a simple egui `CentralPanel` that calls `draw_panel` for one panel (e.g., Preview) to verify the refactored panels compile. This is a stopgap until Task 5 wires up the layout tree rendering.

- [ ] **Step 9: Run tests and verify compile**

Run: `cargo build && cargo test`
Expected: compiles (with warnings about unused layout modules), all tests pass

- [ ] **Step 10: Commit**

```bash
git add src/ui/ src/state.rs src/main.rs
git commit -m "Refactor panel draw signatures for tiling WM integration"
```

---

### Task 4: SharedGpuState & Renderer Refactor

**Files:**
- Create: `src/window.rs`
- Modify: `src/renderer/mod.rs`

Extract shared GPU resources from `Renderer` into `SharedGpuState`. Create `WindowState` for per-window resources.

- [ ] **Step 1: Create SharedGpuState**

In `src/renderer/mod.rs`, extract the shared parts:

```rust
pub struct SharedGpuState {
    pub device: Arc<wgpu::Device>,
    pub queue: Arc<wgpu::Queue>,
    pub preview_renderer: PreviewRenderer,
    pub widget_pipeline: WidgetPipeline,
    pub text_renderer: GlyphonRenderer,
}
```

Add a constructor `SharedGpuState::new(instance, adapter) -> Result<Self>` that creates device, queue, and all shared pipelines. The preview test frame upload stays here.

- [ ] **Step 2: Create WindowState**

In `src/window.rs`:

```rust
use std::sync::Arc;
use winit::window::Window;
use crate::renderer::SharedGpuState;
use crate::ui::layout::LayoutTree;

pub struct WindowState {
    pub window: Arc<Window>,
    pub surface: wgpu::Surface<'static>,
    pub surface_config: wgpu::SurfaceConfiguration,
    pub egui_renderer: egui_wgpu::Renderer,
    pub egui_state: egui_winit::State,
    pub egui_ctx: egui::Context,
    pub layout: LayoutTree,
    pub is_main: bool,
}
```

Methods:
- `new(window, gpu: &SharedGpuState, layout: LayoutTree, is_main: bool) -> Result<Self>` — creates surface, configures it, creates egui renderer and state
- `resize(&mut self, width, height)` — reconfigures surface
- `render(&mut self, gpu: &SharedGpuState, state: &mut AppState)` — runs egui frame, traverses layout, draws panels, renders. For now, just clears and renders egui with a test label (full layout rendering in Task 5).

- [ ] **Step 3: Update main.rs with AppManager**

Replace `App` with `AppManager`:

```rust
struct AppManager {
    gpu: Option<SharedGpuState>,
    windows: HashMap<winit::window::WindowId, WindowState>,
    main_window_id: Option<winit::window::WindowId>,
    state: Arc<Mutex<AppState>>,
    runtime: tokio::runtime::Runtime,
}
```

Route events by `window_id` in `window_event()`. In `resumed()`, create `SharedGpuState` and the main `WindowState` with the default layout.

- [ ] **Step 4: Verify compile and tests**

Run: `cargo build && cargo test`

- [ ] **Step 5: Commit**

```bash
git add src/renderer/mod.rs src/window.rs src/main.rs
git commit -m "Extract SharedGpuState and create multi-window WindowState"
```

---

### Task 5: Layout Tree Rendering

**Files:**
- Modify: `src/ui/layout/render.rs` (replace placeholder)
- Modify: `src/window.rs`

Wire the layout tree into the render loop — traverse the tree, allocate egui rects, draw panel headers and content.

- [ ] **Step 1: Implement layout rendering**

In `src/ui/layout/render.rs`:

```rust
/// Render all panels in the layout tree. Returns a list of deferred actions
/// (resize, swap type, close, detach, split, merge) to apply after drawing.
/// This keeps the tree immutable during rendering, avoiding borrow conflicts.
pub fn render_layout(
    ctx: &egui::Context,
    layout: &LayoutTree,
    state: &mut AppState,
    available_rect: egui::Rect,
) -> Vec<LayoutAction> {
    let mut actions = Vec::new();
    // Snapshot leaves with their rects
    let leaves = layout.collect_leaves_with_rects(available_rect);

    // Draw each panel in its allocated rect
    for (panel_id, panel_type, rect) in leaves {
        let panel_rect = egui::Area::new(egui::Id::new(("panel", panel_id.0)))
            .fixed_pos(rect.min)
            .show(ctx, |ui| {
                ui.set_min_size(rect.size());
                ui.set_max_size(rect.size());

                // Panel header
                ui.horizontal(|ui| {
                    ui.label(panel_type.display_name());
                    // Type selector, close button, etc. — added in Task 7
                });
                ui.separator();

                // Panel content
                crate::ui::draw_panel(panel_type, ui, state, panel_id);
            });
    }
}
```

Add `display_name()` method to `PanelType` returning `&'static str`.

Define `LayoutAction` enum in `render.rs`:

```rust
pub enum LayoutAction {
    Resize { node_id: NodeId, new_ratio: f32 },
    SwapType { node_id: NodeId, new_type: PanelType },
    Close { node_id: NodeId },
    Detach { node_id: NodeId },
    Duplicate { node_id: NodeId },
    Split { node_id: NodeId, direction: SplitDirection },
    Merge { node_id: NodeId, keep: MergeSide },
}
```

`render_layout` collects actions during rendering and returns them. The caller (`WindowState::render()`) applies them to the tree after the draw loop completes. This pattern is used consistently for all mutations — divider resize (Task 6), header actions (Task 7), and corner gestures (Task 8) all push to the same `actions` vec.

- [ ] **Step 2: Wire into WindowState::render()**

In `WindowState::render()`, call `render_layout()` with the window's layout tree and the available rect from the egui central panel.

- [ ] **Step 3: Verify visually**

Run: `cargo build`
The app should now show panels tiled according to the default layout tree.

- [ ] **Step 4: Commit**

```bash
git add src/ui/layout/render.rs src/window.rs
git commit -m "Wire layout tree into render loop with panel rect allocation"
```

---

### Task 6: Divider Rendering & Resize

**Files:**
- Modify: `src/ui/layout/render.rs`
- Modify: `src/ui/layout/interactions.rs` (replace placeholder)

Add draggable dividers between split panels.

- [ ] **Step 1: Write hit-testing tests**

In `src/ui/layout/interactions.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn point_on_vertical_divider() {
        let divider = DividerRect {
            rect: egui::Rect::from_min_size(egui::pos2(300.0, 0.0), egui::vec2(4.0, 600.0)),
            node_id: NodeId(1),
            direction: SplitDirection::Vertical,
        };
        assert!(divider.contains(egui::pos2(302.0, 300.0)));
        assert!(!divider.contains(egui::pos2(100.0, 300.0)));
    }

    #[test]
    fn point_on_horizontal_divider() {
        let divider = DividerRect {
            rect: egui::Rect::from_min_size(egui::pos2(0.0, 450.0), egui::vec2(1000.0, 4.0)),
            node_id: NodeId(2),
            direction: SplitDirection::Horizontal,
        };
        assert!(divider.contains(egui::pos2(500.0, 452.0)));
        assert!(!divider.contains(egui::pos2(500.0, 100.0)));
    }
}
```

- [ ] **Step 2: Implement divider interaction**

`DividerRect` struct: `rect`, `node_id`, `direction`.

`collect_dividers(tree, total_rect) -> Vec<DividerRect>` — walks the tree and computes the 4px-wide rect between each split's children.

In `render_layout`, after drawing panels:
- Compute divider rects
- For each divider, paint a thin line
- Check for drag interaction (egui `Sense::drag()`)
- If dragged, compute new ratio from mouse position and push `LayoutAction::Resize { node_id, new_ratio }` to the actions vec

The divider divides the split's rect. For a vertical split at ratio 0.3 with total width 1000px, the divider is at x=298 to x=302 (4px wide). Dragging changes the ratio.

- [ ] **Step 3: Handle cursor change**

When hovering a divider, set cursor to `ResizeHorizontal` or `ResizeVertical` based on split direction.

- [ ] **Step 4: Verify visually**

Run: `cargo build`
Dividers should be visible and draggable between panels.

- [ ] **Step 5: Run tests**

Run: `cargo test`

- [ ] **Step 6: Commit**

```bash
git add src/ui/layout/render.rs src/ui/layout/interactions.rs
git commit -m "Add divider rendering and drag-to-resize interaction"
```

---

### Task 7: Panel Header

**Files:**
- Modify: `src/ui/layout/render.rs`

Add the panel header bar with type selector, close button, and context menu.

- [ ] **Step 1: Implement panel header**

In the `render_layout` function, replace the simple label header with a full header bar:

- `egui::ComboBox` for panel type selection — lists all `PanelType` variants, changing the leaf's type on selection
- Title label showing `panel_type.display_name()`
- Close button ("×") — calls `tree.merge()` on the parent split, keeping the sibling. Disabled if this is the last leaf in the tree.
- Right-click context menu with: "Detach to Window" (stores a pending detach action), "Duplicate" (splits and creates a copy)

Since the header needs to mutate the tree, collect header actions as a `Vec<LayoutAction>` enum during rendering, then apply them after the draw loop. Actions: `SwapType(NodeId, PanelType)`, `Close(NodeId)`, `Detach(NodeId)`, `Duplicate(NodeId)`.

- [ ] **Step 2: Apply actions after draw**

After `render_layout`, process the collected actions:
- `SwapType` → `tree.swap_type()`
- `Close` → find parent split, `tree.merge()` keeping sibling
- `Detach` → stored as pending for the AppManager to handle (creates new window)
- `Duplicate` → `tree.split()` at the leaf

- [ ] **Step 3: Verify visually**

Run: `cargo build`
Each panel should have a header with a dropdown, title, and close button.

- [ ] **Step 4: Commit**

```bash
git add src/ui/layout/render.rs
git commit -m "Add panel header with type selector, close, and context menu"
```

---

### Task 8: Corner Handles — Split & Merge Gestures

**Files:**
- Modify: `src/ui/layout/interactions.rs`
- Modify: `src/ui/layout/render.rs`

- [ ] **Step 1: Add corner handle hit testing**

Define `CornerHandle` struct: `rect` (small triangle area, ~12x12px), `panel_node_id`, `corner` (TopRight or BottomLeft).

`collect_corner_handles(leaves_with_rects) -> Vec<CornerHandle>` — for each leaf rect, create handles at top-right and bottom-left corners.

- [ ] **Step 2: Implement split gesture**

When a corner handle is dragged:
- Track drag delta
- If horizontal drag > threshold (10px), split vertically
- If vertical drag > threshold (10px), split horizontally
- Call `tree.split()` on the leaf's node

- [ ] **Step 3: Implement merge gesture**

When a corner handle is dragged into the sibling panel area:
- Determine if the leaf has a sibling (parent is a split)
- Check if drag direction matches the parent split's orientation
- Show directional arrow overlay
- On release inside sibling area, call `tree.merge()` on the parent

- [ ] **Step 4: Render corner handles**

Draw small triangles at the corners of each panel. Use egui's painter to draw the triangle shape. Change cursor on hover.

- [ ] **Step 5: Verify visually**

Run: `cargo build`
Corner triangles visible, drag to split/merge works.

- [ ] **Step 6: Commit**

```bash
git add src/ui/layout/interactions.rs src/ui/layout/render.rs
git commit -m "Add corner handles for split and merge gestures"
```

---

### Task 9: Multi-Window — Detach & Reattach

**Files:**
- Modify: `src/main.rs`
- Modify: `src/window.rs`

- [ ] **Step 1: Implement detach**

In `AppManager`, handle the `Detach` action from the panel header:
1. Call `tree.remove_leaf(node_id)` on the source window's tree — returns `(PanelType, PanelId)`
2. Create a new `LayoutTree::new(panel_type)` with the preserved `PanelId`
3. Create a new OS window via `event_loop.create_window()`
4. Create a new `WindowState` with the single-leaf tree
5. Add to `self.windows` HashMap

The new window should be positioned near the mouse cursor (or offset from the main window).

- [ ] **Step 2: Implement reattach on close**

In `window_event` for `CloseRequested`:
- If it's the main window, exit the app
- If it's a detached window:
  1. Get the window's single leaf (panel_type, panel_id)
  2. Call `main_window.layout.insert_at_root(panel_type, panel_id, SplitDirection::Vertical, 0.5)`
  3. Remove the `WindowState` from the HashMap
  4. The winit window drops naturally

- [ ] **Step 3: Handle `RedrawRequested` per window**

Each window in the HashMap gets its own redraw cycle. Request redraw for all windows.

- [ ] **Step 4: Verify visually**

Run: `cargo build`
Right-click header → Detach creates a new OS window. Closing it returns the panel.

- [ ] **Step 5: Commit**

```bash
git add src/main.rs src/window.rs
git commit -m "Add multi-window support with panel detach and reattach"
```

---

### Task 10: Layout Persistence Integration

**Files:**
- Modify: `src/main.rs`
- Modify: `src/window.rs`

- [ ] **Step 1: Define multi-window serialization wrapper**

Extend the serialization module (`src/ui/layout/serialize.rs`) with a top-level wrapper:

```rust
#[derive(Serialize, Deserialize)]
struct SavedLayout {
    layout: SerializedNode,
    #[serde(default)]
    detached: Vec<DetachedEntry>,
}

#[derive(Serialize, Deserialize)]
struct DetachedEntry {
    panel: PanelType,
    id: u64,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}
```

Add functions:
- `pub fn serialize_full_layout(tree: &LayoutTree, detached: &[DetachedEntry]) -> Result<String>`
- `pub fn deserialize_full_layout(toml_str: &str) -> Result<(LayoutTree, Vec<DetachedEntry>)>`

- [ ] **Step 2: Save layout on change**

After any layout action (split, merge, resize, swap, detach, reattach), serialize the main window's layout tree and all detached window entries, save to `<config_dir>/lodestone/layout.toml`. Use a debounced tokio task (500ms delay).

- [ ] **Step 3: Load layout on startup**

In `AppManager::new()`, try to load `layout.toml`. If it exists and parses, use the saved layout for the main window and recreate detached windows. If it doesn't exist or fails to parse, fall back to `LayoutTree::default_layout()`.

- [ ] **Step 4: Add "Reset Layout" keyboard shortcut**

`Ctrl+Shift+R` resets the main window's layout to default and closes all detached windows.

- [ ] **Step 5: Write multi-window persistence test**

```rust
#[test]
fn full_layout_save_load_roundtrip() {
    let tree = LayoutTree::default_layout();
    let detached = vec![DetachedEntry {
        panel: PanelType::StreamControls,
        id: 99,
        x: 100, y: 100, width: 400, height: 300,
    }];
    let toml_str = serialize_full_layout(&tree, &detached).unwrap();
    let (restored_tree, restored_detached) = deserialize_full_layout(&toml_str).unwrap();
    assert_eq!(restored_tree.collect_leaves().len(), 5);
    assert_eq!(restored_detached.len(), 1);
    assert_eq!(restored_detached[0].panel, PanelType::StreamControls);
}
```

- [ ] **Step 6: Commit**

```bash
git add src/main.rs src/window.rs
git commit -m "Add layout persistence with debounced save and startup restore"
```

---

### Task 11: Final Cleanup & Full Test Suite

**Files:**
- All `src/` files

- [ ] **Step 1: Run full test suite**

Run: `cargo test`
Expected: all tests pass (existing 28 + new layout/serialization tests)

- [ ] **Step 2: Run clippy**

Run: `cargo clippy -- -D warnings`
Fix any warnings. Add `#[allow(dead_code)]` only for types genuinely reserved for future use.

- [ ] **Step 3: Run formatter**

Run: `cargo fmt`

- [ ] **Step 4: Verify clean release build**

Run: `cargo build --release`

- [ ] **Step 5: Manual smoke test**

Run: `cargo run`

Verify:
1. Default layout shows all 5 panel types tiled
2. Drag dividers to resize panels
3. Drag corner handles to split a panel
4. Drag corner handles into sibling to merge
5. Change panel type via header dropdown
6. Close a panel via header × button
7. Right-click → Detach creates a new OS window
8. Close detached window returns panel to main
9. Layout persists across app restart
10. Ctrl+Shift+R resets layout

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "Clean up warnings and formatting for dockable panels milestone"
```
