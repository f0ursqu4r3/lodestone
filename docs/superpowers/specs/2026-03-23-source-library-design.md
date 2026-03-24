# Source Library Design

## Summary

Redesign source management from a per-scene model to a global source library with inheritance-based property overrides per scene. Sources are defined once in a library, then composed into scenes. Each scene can override any property, with visual indicators showing what's overridden and the ability to reset to library defaults.

## Motivation

The current model creates duplicated work when the same source (e.g. webcam, display capture) needs to appear in multiple scenes. Each scene has its own copy of the source, and changes don't propagate. Users expect OBS-like behavior: define a source once, reuse it across scenes.

## Data Model

### Core Types

```rust
/// A source defined in the library. Single source of truth for defaults.
pub struct LibrarySource {
    pub id: SourceId,
    pub name: String,
    pub source_type: SourceType,
    pub properties: SourceProperties,  // device config (which camera, display, etc.)
    pub folder: Option<String>,        // user-defined folder, or None

    // Default values (inherited by scenes unless overridden)
    pub transform: Transform,
    pub native_size: (f32, f32),
    pub opacity: f32,
    pub visible: bool,
    pub muted: bool,
    pub volume: f32,
}

/// A source's presence in a scene. References a library source by ID.
pub struct SceneSource {
    pub source_id: SourceId,          // points to LibrarySource
    pub overrides: SourceOverrides,   // per-scene overrides
}

/// Optional per-scene overrides. None = inherit from library default.
pub struct SourceOverrides {
    pub transform: Option<Transform>,
    pub opacity: Option<f32>,
    pub visible: Option<bool>,
    pub muted: Option<bool>,
    pub volume: Option<f32>,
}

/// Scene holds ordered SceneSource refs (order = z-index, first = bottom).
pub struct Scene {
    pub id: SceneId,
    pub name: String,
    pub sources: Vec<SceneSource>,
}
```

### Property Resolution

When rendering or displaying properties, each field resolves as: `scene_override.unwrap_or(library_default)`.

A helper method on `SceneSource` + `LibrarySource` returns both the resolved value and a boolean indicating whether it's overridden, for UI display.

### Persistence

The `scenes.toml` file adopts a new structure:

```toml
[[library]]
id = 1
name = "Main Camera"
source_type = "Camera"
folder = "Cameras"
# ... all default properties

[[scenes]]
id = 1
name = "Gaming"

[[scenes.sources]]
source_id = 1
# only overridden fields appear
[scenes.sources.overrides]
transform = { x = 100, y = 50, w = 320, h = 240 }
opacity = 0.75
```

Fields without overrides are omitted from the TOML entirely.

### AppState Changes

```rust
// Before
pub sources: Vec<Source>,

// After
pub library: Vec<LibrarySource>,
```

`SceneCollection` changes similarly — `sources` becomes `library`.

## UI Design

### Library Panel (New)

A new dockview panel, dockable anywhere. Contains two togglable views via icons in the panel header.

**By Type view:**
- Collapsible sections: Displays, Windows, Cameras, Images. Only sections with sources are shown.
- Each row: type icon, source name, usage count badge (number of scenes referencing it).
- "+" button at the bottom opens the add-source menu (Display/Window/Camera/Image). This is the only place to create sources.

**Folders view:**
- User-created folders as collapsible sections, plus an "Unfiled" section for sources without a folder.
- Same row format as By Type view.
- Right-click folder: rename, delete. Right-click source: "Move to folder..."
- "New Folder" button at the bottom.

**Folders are purely organizational** — no functional meaning, no folder-level defaults.

**Shared behavior across both views:**
- Click a source to select it → properties panel shows library defaults.
- Drag a source onto the canvas or scene source list → adds it to the active scene.
- Right-click context menu: Rename, Duplicate, Delete.
- Delete shows a confirmation dialog listing which scenes use the source, then cascades (removes from library and all scenes).
- Double-click to rename inline.

### Scene Sources Panel (Revised)

Changes from "create and manage sources" to "compose scenes from library sources."

**Changes:**
- "+" button opens an "Add from Library" picker — a popup listing library sources not already in this scene, grouped by type. Click to add.
- Remove button removes the source from the scene only, not the library. No confirmation needed.
- Reorder (up/down or drag) controls z-order, same as today.
- Each row: type icon, source name, visibility toggle.

**Drag from library:**
- Dragging from the library panel onto the scene source list inserts at the drop position.
- Dragging onto the canvas adds to the scene at the top of the z-order.

### Properties Panel — Override System

**When a scene source is selected:**
- Header shows "Scene Override — {scene name}".
- All fields show resolved values (override if set, library default if not).
- Each field has a small blue dot indicator next to its label when the value is overridden in this scene. No dot when inheriting.
- Inherited values are shown with dimmer text to reinforce that they come from the library.
- Editing an inherited value automatically creates a scene override (dot appears).
- Right-clicking an override dot (or the field label when overridden) shows "Reset to library default" — removes the override, field snaps back to library value, dot disappears.

**When a library source is selected (from the library panel):**
- Header shows "Library Defaults".
- No dots — every field is the canonical value.
- Editing here changes the default for all scenes that haven't overridden that field.

## Capture Pipeline Lifecycle

The GStreamer integration rules remain essentially the same as today's `apply_scene_diff()`:

- **Scene becomes active:** Start capture pipelines for all sources in the new scene that weren't in the previous scene.
- **Scene switch:** Stop captures for sources only in the old scene. Start captures for sources only in the new scene. Sources in both scenes keep running (no interruption).
- **Source removed from active scene:** Stop its capture if no other active scene uses it (only one scene is active at a time, so this simplifies to: stop if removed from the active scene).
- **Source exists in library but not in any scene:** No capture runs. Pipeline starts when a scene containing it becomes active.

Image sources continue to bypass GStreamer — `LoadImageFrame` command works the same way, keyed by `SourceId`.

The frame pipeline is unchanged: `latest_frames: HashMap<SourceId, RgbaFrame>` stays the same. The compositor reads frames by `SourceId` from the active scene's source list.

## Migration

On startup, detect the old `scenes.toml` format (no `[[library]]` section) and auto-migrate:

1. Each existing `Source` becomes a `LibrarySource` with all its current property values as library defaults.
2. Each scene's `sources: Vec<SourceId>` becomes `sources: Vec<SceneSource>` with empty overrides (everything inherits from the newly created library defaults).
3. No folder assignments — all sources start unfiled.
4. Save immediately in the new format.

This is a one-way migration. No data loss — the migrated state is functionally identical to the old state. Old versions of Lodestone will not read the new format.

## Decisions Log

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Property inheritance model | Library defaults with per-scene overrides (VS Code-style) | Two-tier cascade is simple to reason about. Override indicators make state visible. |
| Source creation location | Library only | Library is the single source of truth. Scene panel is for composition only. |
| Library panel views | By Type + Folders (togglable) | Covers both automatic organization and user-defined grouping. |
| Folder semantics | Purely organizational | Avoids three-tier inheritance complexity. |
| Capture lifecycle for unused sources | No capture when not in active scene | Resource-efficient. Extends existing `apply_scene_diff()` logic. |
| Z-order control | Per-scene only | Z-order is a spatial/composition concern, not a source property. |
| Library source deletion | Cascade with confirmation | Warn which scenes are affected, then remove from library and all scenes. |
| Migration | One-way auto-migration on startup | No rollback path needed for a personal project. |
