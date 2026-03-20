# Dockview-Style Layout System — Design Spec

## Overview

Replace Lodestone's Blender-style tiling layout with a dockview.dev-style panel system. Groups hold multiple panels as tabs. Drag tabs between groups with 5-zone drop overlays (left/right/top/bottom/center). Floating groups overlay the grid. Modern styling.

**Note:** This spec supersedes the previous dockable panels spec (`2026-03-19-dockable-panels-design.md`). The panel draw contract (`draw(ui, state, panel_id)`) and multi-window architecture (`SharedGpuState`, `WindowState`) are preserved.

## Data Model

### Group

A container holding one or more panels as tabs:

```rust
struct Group {
    id: GroupId,
    tabs: Vec<TabEntry>,
    active_tab: usize,
}

struct TabEntry {
    panel_id: PanelId,
    panel_type: PanelType,
}
```

`GroupId` is a `u64` with the same atomic counter pattern as `PanelId`.

### DockLayout

The top-level layout state per window:

```rust
struct DockLayout {
    grid: SplitTree,                     // binary split tree of GroupIds
    floating: Vec<FloatingGroup>,        // groups floating above the grid
    groups: HashMap<GroupId, Group>,      // all group data
}
```

### SplitTree

Same binary split tree structure as before, but leaves are `GroupId` instead of `(PanelType, PanelId)`:

```rust
enum SplitNode {
    Leaf { group_id: GroupId },
    Split { direction: SplitDirection, ratio: f32, first: NodeId, second: NodeId },
}
```

### FloatingGroup

```rust
struct FloatingGroup {
    group_id: GroupId,
    pos: egui::Pos2,
    size: egui::Vec2,
}
```

### Default Layout

```text
Split(Vertical, 0.2)
├── Group([SceneEditor])
└── Split(Horizontal, 0.75)
    ├── Group([Preview])
    └── Group([AudioMixer, StreamControls])
```

4 panels, 3 groups. AudioMixer and StreamControls share a group as tabs.

## Tab Bar & Group Rendering

Each group renders as:

1. **Tab bar** (top) — horizontal strip with one tab per panel
2. **Active panel content** (below) — selected tab's panel draws into remaining space

### Tab Bar Behavior

- Each tab shows the panel type name
- Active tab visually highlighted (brighter background, colored bottom accent)
- Tabs are draggable — initiates drag-and-drop
- Close button (×) on each tab, visible on hover only
- Right-click tab → context menu: "Detach", "Close", "Close Others", plus "Add" submenu with panel type list
- If last tab closed, group is removed and parent split collapses

### Single-Tab Groups

Still show the tab bar for consistency and drag support. Tab stretches to full width.

### Modern Styling

- Tab bar background: dark (#1e1e2e), subtle bottom border
- Active tab: lighter (#2a2a3e), colored bottom accent (blue/purple #7c6cf0)
- Inactive tabs: darker, text slightly dimmed
- Hover: subtle highlight (#2e2e3e)
- Tab close button appears on hover only
- Group content area: dark background (#181825)

## Drag-and-Drop with Drop Zone Overlays

### Drag Initiation

Dragging a tab from any tab bar starts a drag operation.

### Drag State

```rust
struct DragState {
    panel_id: PanelId,
    panel_type: PanelType,
    source_group: GroupId,
}
```

Stored in `DockLayout` as `Option<DragState>`. Set when drag starts, cleared on drop/cancel.

### During Drag

- Ghost label follows cursor (semi-transparent panel type name)
- Every group becomes a potential drop target
- When cursor enters a group's rect, a 5-zone overlay appears:

```text
┌──────────────────────────┐
│         TOP (20%)        │
├──────┬──────────┬────────┤
│      │          │        │
│ LEFT │  CENTER  │ RIGHT  │
│(20%) │  (60%)   │ (20%)  │
│      │          │        │
├──────┴──────────┴────────┤
│        BOTTOM (20%)      │
└──────────────────────────┘
```

- Active zone highlights with blue tint overlay
- Zone proportions: edges are 20% of the group dimension, center is the remaining 60%

### Drop Actions

- **Left** — split target group vertically, new group on left with the dragged panel
- **Right** — split target group vertically, new group on right
- **Top** — split target group horizontally, new group on top
- **Bottom** — split target group horizontally, new group on bottom
- **Center** — add dragged panel as a new tab in the target group

### Drop Outside

Drop over empty space or outside any group → create a new floating group.

### Drop on Tab Bar

Drop between existing tabs → insert at that position (reorder or cross-group move).

## Floating Groups

Floating groups render as `egui::Window`s above the grid.

### Creating Floating Groups

- Drop a dragged tab outside any grid group
- Right-click tab → "Detach" (floating within the window)
- Right-click tab → "Pop Out" → real OS window (existing multi-window)

### Docking a Floating Group

Drag a tab from a floating group onto a grid group — same drop zone system. If the floating group becomes empty, it disappears.

### Appearance

- Same tab bar as grid groups
- Darker border/shadow to distinguish from grid
- Minimum size: 200x150
- Title shows active tab's name

## Menu Bar

Top menu bar rendered via `egui::TopBottomPanel::top` before the layout:

```
View
├── Add Panel ►
│   ├── Preview
│   ├── Scene Editor
│   ├── Audio Mixer
│   └── Stream Controls
└── Reset Layout
```

"Add Panel" creates a new group at the root with the selected panel type. "Reset Layout" restores the default layout.

## Implementation Impact

### Replaced Files (full rewrite)

- `src/ui/layout/tree.rs` → `DockLayout`, `Group`, `TabEntry`, `SplitTree`, `FloatingGroup`, `DragState`
- `src/ui/layout/render.rs` → group rendering with tab bars, drop zone overlay system, ghost rendering
- `src/ui/layout/interactions.rs` → drag-and-drop state machine, drop zone hit testing
- `src/ui/layout/serialize.rs` → serialization for new data model

### Modified Files

- `src/ui/layout/mod.rs` — re-exports for new types
- `src/window.rs` — `DockLayout` instead of `LayoutTree`
- `src/main.rs` — update references

### Unchanged

- `src/state.rs` — AppState
- `src/obs/` — all OBS types, MockObsEngine
- `src/mock_driver.rs` — mock data driver
- `src/settings.rs` — AppSettings
- `src/renderer/` — SharedGpuState, pipelines, preview, text
- `src/ui/scene_editor.rs` — panel content (draw signature unchanged)
- `src/ui/audio_mixer.rs` — panel content
- `src/ui/stream_controls.rs` — panel content
- `src/ui/settings_panel.rs` — panel content
- `src/ui/preview_panel.rs` — panel content
- `src/ui/mod.rs` — `draw_panel` dispatch (unchanged)

## Testing Strategy

- **Unit tests:** DockLayout operations (create group, add/remove tabs, split group, merge, move tab between groups, float/dock)
- **Unit tests:** serialization roundtrip for grid + floating groups
- **Unit tests:** drop zone hit testing (point in rect → which zone)
- **No GPU tests** — drag-and-drop verified manually
- **Existing tests** for OBS/state/settings remain passing

## Out of Scope

- Drag-and-drop panels between OS windows
- Tab reordering animation (instant repositioning for MVP)
- Custom tab icons per panel type
- Pinned/locked tabs
