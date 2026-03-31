# Preview/Live Panel Split Design

## Overview

Replace Studio Mode (a toggle that splits the preview panel in half) with two always-available dockable panels: **Preview** (the scene editor) and **Live** (read-only program output monitor). This separates editing from live output, letting users arrange sources in one scene while a different scene is streaming.

## State Model

### Remove
- `studio_mode: bool` — no toggle, the split is structural
- `preview_scene_id: Option<SceneId>` — replaced by the reinterpretation of `active_scene_id`

### Add
- `program_scene_id: Option<SceneId>` — the scene currently going to stream/record/vcam

### Redefine
- `active_scene_id` — the editing scene. What Preview shows, what the sources panel reflects, what transform handles act on. Clicking a scene in the scenes panel changes only this.

### Startup Behavior
`program_scene_id` initializes to the same value as `active_scene_id`. They stay in sync until the user clicks a different scene (which only changes `active_scene_id`).

### Transition
Pushes `active_scene_id` → `program_scene_id`. After transition completes, `program_scene_id` equals what was in Preview. `active_scene_id` does not change.

### Scene Click
Only changes `active_scene_id`. Does not affect program sources or the live output.

## Panel Changes

### New: Live Panel (`PanelType::Live`)

New panel type in the dockview system with its own `src/ui/live_panel.rs`.

**Rendering:**
- Read-only composited output using the same GPU callback pattern as Preview
- Samples the program scene's canvas (primary canvas when `program_scene_id == active_scene_id`, secondary canvas when they differ)
- Letterboxed to canvas aspect ratio, centered in panel
- No transform handles, no zoom/pan, no grid/thirds/safe zone overlays

**Overlays:**
- Resolution/fps label in the bottom-right corner (same style as Preview)
- Small red "LIVE" dot indicator (pulsing) when streaming or recording is active
- Thin amber transition progress bar at the bottom during active transitions

### Modified: Preview Panel

- Remove Studio Mode dual-pane split entirely
- Always renders the single-pane editor view with full zoom/pan, transform handles, grid overlays
- Shows `active_scene_id` only
- Remove "PREVIEW" / "PROGRAM" labels
- Remove transition progress bar (moved to Live panel)

### Default Layout

The Live panel is present in the initial dockview layout, as a tab alongside Preview. Users can undock/rearrange via dockview.

## Render Loop & Compositor

### Secondary Canvas Allocation

Secondary canvas is allocated only when `program_scene_id != active_scene_id`. When they match (common case), both panels sample the same primary canvas — zero overhead.

When scenes differ:
- Primary canvas: composited from `active_scene_id` sources (for Preview)
- Secondary canvas: composited from `program_scene_id` sources (for Live)

### During Transitions

Both canvases are active. The transition blend pass writes to the output texture. The Live panel shows the blended result during the fade. Preview continues showing the editing scene unaffected.

### Transition Completion

`program_scene_id` updates to the new scene. If `program_scene_id == active_scene_id` again, secondary canvas deallocates.

### Source Lifecycle

Sources for both `program_scene_id` and `active_scene_id` must be running. When scenes differ, the union of both scenes' sources stays active. Source diff logic must account for both scenes when deciding what to start/stop.

### Which Canvas Goes Where

| State | Preview Panel | Live Panel | Readback/Stream |
|-------|--------------|------------|-----------------|
| Same scene | Primary canvas | Primary canvas | Primary canvas |
| Different scenes | Primary canvas | Secondary canvas | Secondary canvas |
| Transitioning | Primary canvas | Blended output | Blended output |

## Scenes Panel & Hotkeys

### Scene Clicking
Only sets `active_scene_id`. No source diff for program — program sources managed separately.

### Transition Bar
- Transition button always visible (not gated on a mode toggle)
- Pushes `active_scene_id` → `program_scene_id`
- Disabled when both scenes match or a transition is in-flight
- Type toggle (Fade/Cut) and duration input remain unchanged
- Studio Mode button removed

### Scene Thumbnail Badges
- **PGM** (red) — scene matching `program_scene_id`
- **PRV** (green) — scene matching `active_scene_id`, only when it differs from program
- When both match, show only PGM

### Hotkeys
- **Enter** — trigger transition (always available, not mode-gated)
- **Space** — quick cut (always available)
- **1-9** — select scene in Preview (sets `active_scene_id` only)
- **Ctrl+S / Cmd+S** — removed (no Studio Mode toggle)

### Edge Cases
- **Delete program scene:** `program_scene_id` falls back to `active_scene_id` (instant cut)
- **Delete active scene:** Falls back to first remaining scene, same as current behavior
- **Transition to same scene:** No-op (button disabled)

## What Gets Removed

- `studio_mode: bool` field from AppState
- `preview_scene_id` field from AppState
- Studio Mode toggle button in transition bar
- `Ctrl+S` hotkey handler
- Dual-pane split logic in `preview_panel.rs`
- "PREVIEW" / "PROGRAM" labels in preview panel
- All `if state.studio_mode` branches in scenes panel and main.rs

## Testing Strategy

- **Unit tests:** Source lifecycle with two active scenes. Transition state with `program_scene_id`. Badge display logic (PGM only, PGM+PRV, etc).
- **Visual verification:** Live panel shows correct scene. Preview editing doesn't affect live output. Transitions animate on Live panel. Progress bar appears on Live panel during fade.
- **Edge cases:** Delete program scene. Delete active scene. Rapid scene clicking during transition. App startup (both IDs same).
