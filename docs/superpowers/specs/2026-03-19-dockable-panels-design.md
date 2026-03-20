# Dockable Panel System — Design Spec

## Overview

A Blender-style tiling window manager for Lodestone. Every panel area can become any panel type. Panels can be split, merged, resized, and detached to real OS windows. Layout is persisted and restored across sessions.

## Layout Tree Data Model

The core data structure is a binary split tree. Every node is either:

- **Leaf** — contains a `PanelType` and a unique `PanelId`
- **Split** — contains a direction (Horizontal or Vertical), a split ratio (0.0–1.0), and two children

```text
Split(Vertical, 0.25)
├── Leaf(SceneEditor)
└── Split(Horizontal, 0.7)
    ├── Leaf(Preview)
    └── Leaf(AudioMixer)
```

### PanelType Enum

```rust
enum PanelType {
    Preview,
    SceneEditor,
    AudioMixer,
    StreamControls,
    Settings,
}
```

New types added here automatically appear in the panel type dropdown. Each variant maps to a draw function.

### PanelId

A `u64` unique identifier per panel instance. Allows multiple instances of the same `PanelType` to maintain independent UI state (scroll position, selections) via egui's ID system.

### Tree Operations

- **Split** — replace a leaf with a split node containing two children (the original panel + a new panel of the same type)
- **Merge** — replace a split node with one of its children, discarding the other
- **Resize** — adjust the ratio of a split node by dragging the divider
- **Swap type** — change a leaf's `PanelType` via a dropdown in the panel header

## Interactions

### Splitting

Each panel has small drag handles in two corners (top-right and bottom-left, like Blender's triangles):
- Dragging horizontally from a corner splits vertically (left/right pair)
- Dragging vertically from a corner splits horizontally (top/bottom pair)
- The new panel starts as the same type as the original

### Merging

Drag a corner handle *into* an adjacent panel. The adjacent panel is absorbed and the split node collapses to the remaining leaf. A directional arrow overlay indicates which panel will be consumed.

### Resizing

Each split node renders a thin divider bar (3-4px) between its children. Dragging the divider adjusts the split ratio. Minimum panel size enforced at 100px in either dimension.

### Detaching

Right-click a panel header → "Detach to Window". This:
1. Removes the leaf from the tree (collapsing the parent split)
2. Creates a new OS window via `winit`
3. Renders that panel in the new window with its own `wgpu::Surface`

### Reattaching

Close a detached window to return its panel to a default position in the main window's tree.

### Panel Header

Every panel has a thin header bar containing:
- Panel type selector (dropdown/combo) — switch any panel to any type
- Panel title (derived from type)
- Close button (merges with adjacent panel; disabled if the panel is the last leaf)
- Right-click context menu: detach, duplicate

## Multi-Window Architecture

### WindowState

One per OS window:

```rust
struct WindowState {
    window: &'static Window,
    surface: wgpu::Surface<'static>,
    surface_config: wgpu::SurfaceConfiguration,
    egui_renderer: egui_wgpu::Renderer,
    egui_state: egui_winit::State,
    layout: LayoutTree,
    preview_renderer: PreviewRenderer,
    widget_pipeline: WidgetPipeline,
    id: WindowId,
}
```

### Shared Resources

All windows share:
- `wgpu::Device` and `wgpu::Queue` — one GPU context for the app
- `egui::Context` — shared for consistent styling across windows
- `Arc<Mutex<AppState>>` — all panels read/write the same state

### Event Routing

`winit` delivers `window_event` with a `window_id`. The `AppManager` (replacing the current `App`) maintains a `HashMap<WindowId, WindowState>` and routes events to the correct window. Each window handles its own `RedrawRequested` independently.

### Window Lifecycle

- **Main window** closing exits the app
- **Detached windows** closing returns their panel to the main window's tree
- On app exit, all windows are closed

## Panel Drawing

### Draw Contract

Panels implement:

```rust
fn draw(ui: &mut egui::Ui, state: &mut AppState, panel_id: PanelId)
```

Panels receive a `&mut egui::Ui` (a region allocated by the layout tree), not `&egui::Context`. Panels don't control their own placement — the tiling WM does.

### Panel Registry

```rust
fn draw_panel(panel_type: PanelType, ui: &mut egui::Ui, state: &mut AppState, id: PanelId) {
    match panel_type {
        PanelType::Preview => preview_panel::draw(ui, state, id),
        PanelType::SceneEditor => scene_editor::draw(ui, state, id),
        PanelType::AudioMixer => audio_mixer::draw(ui, state, id),
        PanelType::StreamControls => stream_controls::draw(ui, state, id),
        PanelType::Settings => settings_panel::draw(ui, state, id),
    }
}
```

### Preview as a Panel

The current fullscreen preview becomes a panel type. `PreviewRenderer`'s texture is rendered into the panel's allocated rect, scaled to fit with aspect ratio preserved. No special treatment — it's just another panel.

## Layout Persistence

### Serialization Format

The layout tree serializes to TOML:

```toml
[layout]
type = "split"
direction = "vertical"
ratio = 0.25

[layout.first]
type = "leaf"
panel = "SceneEditor"
id = 1

[layout.second]
type = "split"
direction = "horizontal"
ratio = 0.75

[layout.second.first]
type = "leaf"
panel = "Preview"
id = 2

[layout.second.second]
type = "leaf"
panel = "AudioMixer"
id = 3
```

Saved to `<config_dir>/lodestone/layout.toml`. Loaded at startup, saved on layout change (debounced).

### Detached Windows

Saved as separate entries with window position and size:

```toml
[[detached]]
panel = "StreamControls"
id = 4
x = 1200
y = 100
width = 400
height = 300
```

On restart, detached windows reopen in their saved positions.

### Default Layout

When no saved layout exists:

```text
Split(Vertical, 0.2)
├── Split(Horizontal, 0.6)
│   ├── SceneEditor
│   └── Settings
└── Split(Horizontal, 0.75)
    ├── Preview
    └── Split(Vertical, 0.5)
        ├── AudioMixer
        └── StreamControls
```

A "Reset Layout" option restores this default.

## Implementation Impact

### New Files

- `src/ui/layout/mod.rs` — `LayoutTree`, `LayoutNode`, tree operations
- `src/ui/layout/tree.rs` — binary tree data structure, split/merge/resize
- `src/ui/layout/divider.rs` — divider rendering, hit testing, drag handling
- `src/ui/layout/corner.rs` — corner handle rendering, split/merge gesture detection
- `src/ui/layout/header.rs` — panel header bar with type selector, close, context menu
- `src/ui/layout/serialize.rs` — TOML serialization/deserialization
- `src/window.rs` — `WindowState`, multi-window management, window lifecycle
- `src/ui/preview_panel.rs` — preview as a panel (wraps PreviewRenderer)

### Refactored Files

- `src/main.rs` — single-window `App` → multi-window `AppManager` with event routing by window ID
- `src/renderer/mod.rs` — `Renderer` becomes per-window; `Device`/`Queue` shared via `Arc`
- `src/ui/mod.rs` — `UiRoot::run()` replaced with layout tree traversal that allocates rects and calls panel draw functions
- `src/ui/scene_editor.rs` — `draw(ctx, state)` → `draw(ui, state, panel_id)`, remove `SidePanel` wrapper
- `src/ui/audio_mixer.rs` — `draw(ctx, state)` → `draw(ui, state, panel_id)`, remove `TopBottomPanel` wrapper
- `src/ui/stream_controls.rs` — `draw(ctx, state)` → `draw(ui, state, panel_id)`, remove `Window` wrapper
- `src/ui/settings_modal.rs` — becomes `settings_panel.rs`, draws into a `Ui` region instead of a modal

### Unchanged

- `src/state.rs` — AppState, types
- `src/obs/` — all OBS types, MockObsEngine
- `src/mock_driver.rs` — mock data driver
- `src/settings.rs` — AppSettings (layout settings live in separate file)
- `src/renderer/pipelines.rs` — SDF pipeline
- `src/renderer/text.rs` — text renderer
- `src/renderer/preview.rs` — PreviewRenderer (used by preview panel)

### Removed

- `src/state.rs: UiState` — panel visibility flags replaced by layout tree (panels exist in the tree or they don't)
- F1/F2/F3 keyboard shortcuts for panel toggles — replaced by layout manipulation
- Settings modal — replaced by Settings panel type

## Testing Strategy

- **Unit tests:** layout tree operations (split, merge, resize), serialization roundtrip, hit testing (point-in-divider, point-in-corner-handle), minimum size enforcement
- **No GPU tests** — multi-window behavior verified manually
- **Existing 28 tests** remain passing (state/OBS/settings tests are unchanged)

## Out of Scope

- Tabs (multiple panels stacked in one area with tab headers)
- Drag-and-drop panels between windows (right-click detach/close-to-reattach only)
- Undo/redo on layout changes
- Panel-specific toolbar/menu bars
