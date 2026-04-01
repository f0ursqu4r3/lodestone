# Scene Properties in Properties Panel

## Summary

Extend the Properties panel with a third mode: when no source is selected but a scene is active, show scene-level properties. This gives per-scene transition overrides (type, duration, color pickers) a proper home and removes them from the cramped right-click context menu.

## Current State

The Properties panel has two modes:
1. **Scene Override** — source selected in the active scene → edits per-scene overrides
2. **Library Defaults** — source selected in the library → edits library defaults
3. **Empty** — nothing selected → "Select a source to view properties"

Scene properties (name, pinned, transition override) are currently:
- Name: inline rename in the Scenes panel thumbnail grid
- Pinned: toggle in the scene context menu
- Transition override: submenu in the scene context menu (type dropdown, duration input)

The transition override context menu has no room for color pickers and is hard to discover.

## Design

### Selection Priority

The Properties panel's `draw()` function already checks selections in order. Add a third fallback before the empty state:

1. Source selected in active scene → source properties (scene override mode)
2. Source selected in library → source properties (library defaults mode)
3. **`active_scene_id` is set, no source selected → scene properties** (new)
4. Nothing → empty state

This is zero-friction: click a scene thumbnail, source selections clear (already happens via `deselect_all()`), Properties panel automatically shows scene properties. Click any source, it switches back to source properties.

### Scene Properties Panel Layout

```
┌─────────────────────────────────────┐
│ SCENE PROPERTIES — SCENE 1          │  ← accent color header
├─────────────────────────────────────┤
│                                     │
│ Name  [Scene 1            ]         │  ← editable text field
│                                     │
│ ── Transition In ──────────────     │  ← section header
│                                     │
│ Type     [Fade          ▾]          │  ← dropdown from registry, "Default" option
│ Duration [300           ] ms        │  ← input field, empty = inherit default
│                                     │
│ Color    [■]                        │  ← shown only if transition has @params
│ From     [■]                        │  ← shown only if transition has @params
│ To       [■]                        │  ← shown only if transition has @params
│                                     │
│ ☐ Pinned                            │  ← checkbox
│                                     │
└─────────────────────────────────────┘
```

### Scene Properties — Section Details

**Header:** `"SCENE PROPERTIES — {SCENE_NAME}"` in accent color, same style as the existing `"SCENE OVERRIDE — ..."` header.

**Name field:** Single-line `TextEdit`. On change, updates `scene.name` and marks dirty. This replaces the inline rename in the Scenes panel as the primary editing mechanism (inline rename can stay as a convenience shortcut).

**Transition In section:**

- **Type dropdown:** `ComboBox` populated from `state.transition_registry.all()`. First option is "Default" (maps to `None` in `SceneTransitionOverride.transition`), followed by all transitions from the registry. Shows the current override or "Default" if inheriting.

- **Duration input:** Text field that accepts milliseconds. Empty string = `None` (inherit global default). Shows placeholder text "Default" when empty. Same parsing logic as the existing transition bar duration input.

- **Color pickers:** Conditionally shown based on the *effective* transition's `@params`. The effective transition is the scene override if set, otherwise the global default. For each declared param (`color`, `from_color`, `to_color`), show a label + color swatch that opens a color picker popup on click.

  Colors are stored in `SceneTransitionOverride.colors: Option<TransitionColors>`. When the user picks a color, create a `Some(TransitionColors)` if none exists (copying from global defaults as the starting point). When reset to defaults, set back to `None`.

**Pinned checkbox:** Simple checkbox. Updates `scene.pinned` and marks dirty.

### Context Menu Cleanup

Remove the "Transition Override" submenu from the scene context menu in `scenes_panel.rs`. The context menu retains:
- Rename
- Duplicate (if it exists)
- Delete

The transition bar in the Scenes panel continues to show *global default* transition controls (dropdown + duration). Per-scene overrides are now exclusively in the Properties panel.

### File Changes

| File | Change |
|------|--------|
| `src/ui/properties_panel.rs` | Add scene properties mode to `draw()`, add `draw_scene_properties()` function |
| `src/ui/scenes_panel.rs` | Remove "Transition Override" submenu from scene context menu |
| `src/transition_registry.rs` | Remove `#[allow(dead_code)]` from `params`, `author`, `description` fields (now used by Properties panel) |

### Testing

No new unit tests needed — this is UI-only code. The underlying data structures (`SceneTransitionOverride`, `TransitionColors`, `TransitionRegistry`) are already tested. Manual verification:

- Click scene with no source selected → scene properties appear
- Click a source → switches to source properties
- Click back to scene (deselect source) → back to scene properties
- Change transition type → dropdown updates, color pickers appear/disappear based on `@params`
- Change duration → persists correctly, empty = inherits default
- Edit name → scene name updates in Scenes panel grid
- Toggle pinned → pin state changes
- Transition override colors persist through save/load
