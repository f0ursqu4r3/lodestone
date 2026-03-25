# Undo/Redo Stack & DEL Key Deletion

## Overview

Add a snapshot-based undo/redo system and wire the DEL key to delete the currently selected source. Every mutation that marks `scenes_dirty` becomes undoable via Cmd+Z / Cmd+Shift+Z.

## Undo/Redo Stack

### Data Structure

A new `UndoStack` struct stored in `AppState`:

```rust
pub struct UndoSnapshot {
    pub scenes: Vec<Scene>,
    pub library: Vec<LibrarySource>,
    pub next_scene_id: u64,
    pub next_source_id: u64,
}

pub struct UndoStack {
    undo: Vec<UndoSnapshot>,
    redo: Vec<UndoSnapshot>,
    max_depth: usize, // 50
}
```

### API

- `push_snapshot(state)` — clones undoable fields from AppState, pushes onto undo stack, clears redo stack. Drops oldest if exceeding max_depth.
- `undo(state)` — pushes current state onto redo, pops undo and restores into current state. Returns false if nothing to undo.
- `redo(state)` — pushes current state onto undo, pops redo and restores into current state. Returns false if nothing to redo.

### Snapshot Placement

Every call site that sets `scenes_dirty = true` gets a `state.undo_stack.push_snapshot(state)` call immediately before the mutation. This covers ~15-20 sites across:

- `sources_panel.rs` — add/remove source from scene, reorder
- `library_panel.rs` — add source, delete cascade, rename, move to folder
- `scenes_panel.rs` — add/rename/delete/reorder scenes
- `properties_panel.rs` — transform, opacity, source property changes
- `transform_handles.rs` — drag move/resize in preview, context menu actions

For continuous drags (transform handle resize, opacity slider), push the snapshot only at drag start, not on every frame. This means one undo step reverses the entire drag, not each pixel of movement.

### Keyboard Bindings

Handled in `main.rs` `KeyboardInput` handler:

- `Cmd+Z` → `undo_stack.undo(state)`
- `Cmd+Shift+Z` → `undo_stack.redo(state)`

The Edit menu already has `PredefinedMenuItem::undo()` and `PredefinedMenuItem::redo()` stubs. Wire these to the same logic via menu event handling.

### After Undo/Redo

1. Set `scenes_dirty = true` and `scenes_last_changed = now` so the restored state persists.
2. Run `reconcile_captures(state)` to sync GStreamer (see below).
3. If `selected_source_id` refers to a source no longer in the restored state, clear it. Same for `selected_library_source_id`.

## DEL Key Deletion

### Keybinding

Both `KeyCode::Delete` and `KeyCode::Backspace` trigger deletion. Mac keyboards send Backspace for the Delete key.

### Context-Dependent Behavior

- `selected_source_id` is Some → remove from active scene only (calls existing `remove_source_from_scene`). Source remains in library.
- `selected_library_source_id` is Some → cascade delete from all scenes and library (calls existing `delete_source_cascade`).
- Neither set → no-op.

### Guards

- Skip if `renaming_source_id` or `renaming_scene_id` is Some (user is editing text inline).
- Skip if egui `wants_keyboard_input()` (text field has focus — path input, combo box, etc.).
- Push undo snapshot before performing the delete.

### Handler Location

`main.rs` keyboard handler, after existing key combos. Calls into the existing deletion functions in `sources_panel.rs` and `library_panel.rs`.

## Capture Reconciliation

After undo/redo, GStreamer captures may be out of sync with the restored scene state.

### Approach

`reconcile_captures(state)` runs after every undo/redo:

1. Stop all active captures (send `RemoveCaptureSource` for each).
2. Restart captures for all visible sources in the active scene (send `AddCaptureSource` for each).

This reuses the same pattern as scene switching. It's simple and correct — the brief capture restart is imperceptible.

### Tracking

No new tracking needed. The GStreamer thread already manages its own `captures` HashMap. We just send the commands to bring it in sync.

## Files Modified

| File | Changes |
|------|---------|
| `src/state.rs` | Add `UndoStack`, `UndoSnapshot` structs and `undo_stack` field to `AppState` |
| `src/main.rs` | Handle Cmd+Z, Cmd+Shift+Z, DEL/Backspace key events; wire Edit menu items |
| `src/ui/sources_panel.rs` | Add `push_snapshot()` before mutations; export `remove_source_from_scene` for main.rs |
| `src/ui/library_panel.rs` | Add `push_snapshot()` before mutations; export `delete_source_cascade` for main.rs |
| `src/ui/scenes_panel.rs` | Add `push_snapshot()` before mutations |
| `src/ui/properties_panel.rs` | Add `push_snapshot()` before mutations |
| `src/ui/transform_handles.rs` | Add `push_snapshot()` at drag start (not every frame) |

## Design Decisions

- **Snapshot-based over command-based**: Scene/library data is tiny (few KB per snapshot). Snapshot simplicity eliminates the risk of inverse-command bugs.
- **Max 50 snapshots**: ~250KB worst case. Trivial memory cost.
- **Both Delete and Backspace**: Mac ergonomics — the key labeled "delete" sends Backspace.
- **Context-dependent DEL**: Matches the existing two-mode UI pattern (scene selection vs library selection).
- **Full capture restart on undo/redo**: Simple and correct. Same pattern as scene switching.
- **Snapshot at drag start only**: One undo step per drag gesture, not per pixel. Natural user expectation.
