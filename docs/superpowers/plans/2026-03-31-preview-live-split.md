# Preview/Live Panel Split Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace Studio Mode toggle with always-available Preview (editor) and Live (read-only program monitor) panels.

**Architecture:** Add `program_scene_id` to AppState, remove `studio_mode` and `preview_scene_id`. Create a new `PanelType::Live` with a read-only `live_panel.rs`. Refactor the render loop so `program_scene_id` drives what gets streamed/recorded, while `active_scene_id` drives the editor. Secondary canvas allocated only when the two IDs differ.

**Tech Stack:** Rust, wgpu, egui, WGSL shaders.

---

## File Structure

| Action | File | Responsibility |
|--------|------|---------------|
| Create | `src/ui/live_panel.rs` | Read-only live output monitor panel |
| Modify | `src/state.rs` | Replace `studio_mode`/`preview_scene_id` with `program_scene_id` |
| Modify | `src/ui/layout/tree.rs` | Add `PanelType::Live` variant |
| Modify | `src/ui/layout/tree_builders.rs` | Add Live panel to default layout |
| Modify | `src/ui/mod.rs` | Register `live_panel` module and dispatch |
| Modify | `src/ui/preview_panel.rs` | Remove Studio Mode dual-pane split |
| Modify | `src/ui/scenes_panel.rs` | Replace `studio_mode`/`preview_scene_id` refs with `program_scene_id` |
| Modify | `src/main.rs` | Refactor render loop for `program_scene_id`, update hotkeys |

---

## Task 1: State Model — Replace `studio_mode`/`preview_scene_id` with `program_scene_id`

**Files:**
- Modify: `src/state.rs:184-189` (AppState fields)
- Modify: `src/state.rs:241-243` (Default impl)

- [ ] **Step 1: Replace fields in AppState**

In `src/state.rs`, replace these three fields (lines 184-189):
```rust
/// Whether Studio Mode is active (dual preview/program layout).
pub studio_mode: bool,
/// In Studio Mode, the scene loaded in the Preview pane. None = no scene selected.
pub preview_scene_id: Option<SceneId>,
/// In-progress transition state. None = no transition active.
pub active_transition: Option<crate::transition::TransitionState>,
```

With:
```rust
/// The scene currently going to stream/record/vcam. When `None`, nothing is live.
/// Initialized to `active_scene_id` on first scene creation.
pub program_scene_id: Option<SceneId>,
/// In-progress transition state. None = no transition active.
pub active_transition: Option<crate::transition::TransitionState>,
```

- [ ] **Step 2: Update Default impl**

In the `Default` impl for `AppState` (around line 241-243), replace:
```rust
studio_mode: false,
preview_scene_id: None,
active_transition: None,
```

With:
```rust
program_scene_id: None,
active_transition: None,
```

- [ ] **Step 3: Fix all compilation errors from removed fields**

Run `cargo build 2>&1 | grep "studio_mode\|preview_scene_id"` to find all references. Every file that uses `studio_mode` or `preview_scene_id` will fail to compile. Don't fix the logic yet — just comment out or stub the broken lines with `todo!()` markers so the project compiles. The subsequent tasks will fix the logic properly.

Actually, a cleaner approach: do a find-and-replace across all files:
- Replace `state.studio_mode` → `false` (temporary — removes the toggle checks)
- Replace `state.preview_scene_id` → `state.program_scene_id` (close enough for compilation)

Then fix each file properly in subsequent tasks.

- [ ] **Step 4: Run `cargo build` to verify compilation**

Run: `cargo build 2>&1 | tail -20`
Expected: Compiles (possibly with warnings about unused code).

- [ ] **Step 5: Run tests**

Run: `cargo test 2>&1 | tail -10`
Expected: All tests pass.

- [ ] **Step 6: Commit**

```bash
git add src/state.rs src/main.rs src/ui/scenes_panel.rs src/ui/preview_panel.rs
git commit -m "refactor: replace studio_mode/preview_scene_id with program_scene_id"
```

---

## Task 2: Add `PanelType::Live` and Register in Layout System

**Files:**
- Modify: `src/ui/layout/tree.rs:13-37` (PanelType enum + display_name)
- Modify: `src/ui/layout/tree_builders.rs:51-130` (default_layout)
- Create: `src/ui/live_panel.rs` (stub)
- Modify: `src/ui/mod.rs:1-35` (module + dispatch)

- [ ] **Step 1: Add `Live` variant to PanelType**

In `src/ui/layout/tree.rs`, add `Live` to the enum (after `Preview`):

```rust
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub enum PanelType {
    Preview,
    Live,
    SceneEditor,
    AudioMixer,
    StreamControls,
    Sources,
    Scenes,
    Properties,
    Library,
}
```

Add the display name in `display_name()`:

```rust
Self::Live => "Live",
```

- [ ] **Step 2: Add Live panel to default layout**

In `src/ui/layout/tree_builders.rs`, inside `default_layout()`, add the Live panel as a tab in the Preview group (so it appears alongside Preview):

After the line `let preview_group = Group::new(PanelType::Preview);` (line 72), add:

```rust
let mut preview_group = Group::new(PanelType::Preview);
preview_group.add_tab(PanelType::Live);
```

(Change `let preview_group` to `let mut preview_group` and add the tab.)

- [ ] **Step 3: Create stub `src/ui/live_panel.rs`**

```rust
//! Live panel — read-only monitor showing the program (live) output.
//!
//! Displays the composited frame that goes to stream/record/vcam.
//! No transform handles, no zoom/pan, no grid overlays.

use crate::state::AppState;
use crate::ui::layout::PanelId;

pub fn draw(ui: &mut egui::Ui, state: &mut AppState, _panel_id: PanelId) {
    let theme = crate::ui::theme::active_theme(&state.settings);
    let panel_rect = ui.available_rect_before_wrap();
    ui.allocate_rect(panel_rect, egui::Sense::hover());

    // Placeholder — will be replaced with GPU callback in Task 4.
    let painter = ui.painter_at(panel_rect);
    painter.rect_filled(panel_rect, 0.0, theme.panel_bg());
    painter.text(
        panel_rect.center(),
        egui::Align2::CENTER_CENTER,
        "Live Output",
        egui::FontId::proportional(14.0),
        theme.text_muted(),
    );
}
```

- [ ] **Step 4: Register module and dispatch**

In `src/ui/mod.rs`, add the module declaration:

```rust
pub mod live_panel;
```

And add the dispatch arm in `draw_panel()`:

```rust
PanelType::Live => live_panel::draw(ui, state, id),
```

- [ ] **Step 5: Fix any layout tests that hardcode panel counts**

Search for tests referencing `default_layout` — they may assert specific group/panel counts. Update them to account for the new Live tab in the Preview group.

Run: `cargo test layout 2>&1 | tail -20`

- [ ] **Step 6: Run full build and tests**

Run: `cargo build && cargo test 2>&1 | tail -20`
Expected: Compiles and all tests pass.

- [ ] **Step 7: Commit**

```bash
git add src/ui/layout/tree.rs src/ui/layout/tree_builders.rs src/ui/live_panel.rs src/ui/mod.rs
git commit -m "feat: add PanelType::Live and register in layout system"
```

---

## Task 3: Remove Studio Mode from Preview Panel

**Files:**
- Modify: `src/ui/preview_panel.rs`

- [ ] **Step 1: Remove `CanvasTarget` enum and `target` field from PreviewCallback**

The `CanvasTarget` enum and the `target` field on `PreviewCallback` were added for Studio Mode dual-pane rendering. Remove them. `PreviewCallback` should go back to always sampling from `resources.bind_group` (the primary canvas).

Remove the `CanvasTarget` enum entirely. Remove the `target` field from `PreviewCallback`. In the `paint()` method, always use `resources.bind_group`.

- [ ] **Step 2: Remove `secondary_bind_group` from PreviewResources**

In the `PreviewResources` struct, remove the `secondary_bind_group` field. It will be used by the Live panel instead (Task 4).

```rust
pub struct PreviewResources {
    pub pipeline: Arc<wgpu::RenderPipeline>,
    pub bind_group: Arc<wgpu::BindGroup>,
}
```

- [ ] **Step 3: Remove the Studio Mode dual-pane branch**

In `draw_inner()`, remove the entire `if studio_dual { ... }` branch (lines 695-773). Keep only the single-pane `else` branch (lines 774-786), and remove the `else` keyword so it runs unconditionally. Also remove the `let studio_dual = state.studio_mode;` line.

The result should be just the single-pane render:

```rust
// Single-pane: render the primary canvas with zoom/pan.
ui.painter_at(panel_rect).add(Callback::new_paint_callback(
    panel_rect,
    PreviewCallback {
        zoomed_rect: preview_rect,
    },
));
```

- [ ] **Step 4: Run `cargo build` and fix any remaining references**

Fix any references to `CanvasTarget`, `secondary_bind_group`, or `studio_mode` in this file.

Run: `cargo build 2>&1 | tail -20`
Expected: Compiles.

- [ ] **Step 5: Commit**

```bash
git add src/ui/preview_panel.rs
git commit -m "refactor: remove Studio Mode dual-pane split from preview panel"
```

---

## Task 4: Implement Live Panel GPU Rendering

**Files:**
- Modify: `src/ui/live_panel.rs` (replace stub with GPU rendering)
- Modify: `src/ui/preview_panel.rs` (export `PreviewResources` if not already public)

- [ ] **Step 1: Create `LiveResources` struct**

The Live panel needs its own GPU callback resources, separate from Preview. It samples whichever canvas represents the program output.

In `src/ui/live_panel.rs`:

```rust
use std::sync::Arc;
use egui_wgpu::wgpu;
use egui_wgpu::{Callback, CallbackResources, CallbackTrait};

/// GPU resources for the live panel, stored in `egui_renderer.callback_resources`.
pub struct LiveResources {
    pub pipeline: Arc<wgpu::RenderPipeline>,
    /// Bind group for the program output canvas.
    /// When program_scene_id == active_scene_id, this is the primary canvas bind group.
    /// When they differ, this is the secondary canvas bind group.
    pub bind_group: Arc<wgpu::BindGroup>,
}
```

- [ ] **Step 2: Create `LiveCallback` paint callback**

```rust
struct LiveCallback {
    letterboxed_rect: egui::Rect,
}

impl CallbackTrait for LiveCallback {
    fn paint(
        &self,
        info: egui::PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'static>,
        callback_resources: &CallbackResources,
    ) {
        let Some(resources) = callback_resources.get::<LiveResources>() else {
            return;
        };

        let clip = info.clip_rect_in_pixels();
        if clip.width_px > 0 && clip.height_px > 0 {
            render_pass.set_scissor_rect(
                clip.left_px as u32,
                clip.top_px as u32,
                clip.width_px as u32,
                clip.height_px as u32,
            );
        }

        let ppp = info.pixels_per_point;
        let vp_x = self.letterboxed_rect.min.x * ppp;
        let vp_y = self.letterboxed_rect.min.y * ppp;
        let vp_w = (self.letterboxed_rect.width() * ppp).min(8192.0);
        let vp_h = (self.letterboxed_rect.height() * ppp).min(8192.0);
        if vp_w > 0.0 && vp_h > 0.0 {
            render_pass.set_viewport(vp_x, vp_y, vp_w, vp_h, 0.0, 1.0);
        }

        render_pass.set_pipeline(&resources.pipeline);
        render_pass.set_bind_group(0, &*resources.bind_group, &[]);
        render_pass.draw(0..4, 0..1);
    }
}
```

- [ ] **Step 3: Implement `draw()` with letterboxing, overlays, and GPU callback**

Replace the stub `draw()` with the full implementation:

```rust
pub fn draw(ui: &mut egui::Ui, state: &mut AppState, _panel_id: PanelId) {
    let theme = crate::ui::theme::active_theme(&state.settings);
    let panel_rect = ui.available_rect_before_wrap();
    ui.allocate_rect(panel_rect, egui::Sense::hover());

    let painter = ui.painter_at(panel_rect);

    // Background
    painter.rect_filled(panel_rect, 0.0, theme.panel_bg());

    // Canvas dimensions for aspect ratio
    let preview_width = state.settings.video.base_resolution.split('x')
        .next().and_then(|s| s.trim().parse::<u32>().ok()).unwrap_or(1920);
    let preview_height = state.settings.video.base_resolution.split('x')
        .nth(1).and_then(|s| s.trim().parse::<u32>().ok()).unwrap_or(1080);

    // Letterbox the canvas into the panel
    let letterboxed = letterboxed_rect(panel_rect, preview_width, preview_height);

    // GPU paint callback
    painter.add(Callback::new_paint_callback(
        panel_rect,
        LiveCallback { letterboxed_rect: letterboxed },
    ));

    // Resolution/fps label (bottom-right)
    let res_text = format!("{}x{}", preview_width, preview_height);
    painter.text(
        egui::pos2(letterboxed.right() - 4.0, letterboxed.bottom() - 4.0),
        egui::Align2::RIGHT_BOTTOM,
        &res_text,
        egui::FontId::proportional(10.0),
        theme.text_muted(),
    );

    // LIVE indicator (top-left, red dot) when streaming or recording
    let is_live = state.stream_status.is_live()
        || matches!(state.recording_status, crate::state::RecordingStatus::Recording { .. })
        || state.virtual_camera_active;
    if is_live {
        let dot_center = egui::pos2(letterboxed.min.x + 14.0, letterboxed.min.y + 14.0);
        painter.circle_filled(dot_center, 4.0, egui::Color32::from_rgb(220, 50, 50));
        painter.text(
            egui::pos2(dot_center.x + 8.0, dot_center.y),
            egui::Align2::LEFT_CENTER,
            "LIVE",
            egui::FontId::proportional(10.0),
            egui::Color32::from_rgb(220, 50, 50),
        );
    }

    // Transition progress bar
    if let Some(ref trans) = state.active_transition {
        let progress = trans.progress();
        let bar_h = 3.0;
        let bar_w = letterboxed.width() * progress;
        let bar_rect = egui::Rect::from_min_size(
            egui::pos2(letterboxed.min.x, letterboxed.max.y - bar_h),
            egui::vec2(bar_w, bar_h),
        );
        painter.rect_filled(bar_rect, 0.0, egui::Color32::from_rgb(224, 175, 104));
        ui.ctx().request_repaint();
    }
}

/// Compute the largest rect matching the canvas aspect ratio that fits inside `panel`, centered.
fn letterboxed_rect(panel: egui::Rect, width: u32, height: u32) -> egui::Rect {
    let panel_w = panel.width();
    let panel_h = panel.height();
    let aspect = width as f32 / height as f32;
    let panel_aspect = panel_w / panel_h;

    let (w, h) = if panel_aspect > aspect {
        (panel_h * aspect, panel_h)
    } else {
        (panel_w, panel_w / aspect)
    };

    let x = panel.min.x + (panel_w - w) / 2.0;
    let y = panel.min.y + (panel_h - h) / 2.0;
    egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(w, h))
}
```

- [ ] **Step 4: Insert `LiveResources` in main.rs**

In `src/main.rs`, wherever `PreviewResources` is inserted into `callback_resources`, also insert `LiveResources`. The bind group for `LiveResources` depends on whether `program_scene_id == active_scene_id`:

```rust
// After inserting PreviewResources:
let live_bind_group = if state_program_id == state_active_id {
    gpu.compositor.canvas_bind_group()  // Same scene — use primary canvas
} else if let Some(ref secondary) = gpu.secondary_canvas {
    Arc::clone(&secondary.bind_group)   // Different scene — use secondary canvas
} else {
    gpu.compositor.canvas_bind_group()  // Fallback
};
let live_resources = crate::ui::live_panel::LiveResources {
    pipeline: gpu.compositor.canvas_pipeline(),
    bind_group: live_bind_group,
};
win.egui_renderer.callback_resources.insert(live_resources);
```

Do this for all PreviewResources insertion sites (initial setup, after resize, per-frame sync).

- [ ] **Step 5: Run `cargo build` to verify compilation**

Run: `cargo build 2>&1 | tail -20`
Expected: Compiles.

- [ ] **Step 6: Commit**

```bash
git add src/ui/live_panel.rs src/main.rs
git commit -m "feat: implement Live panel with GPU rendering and overlays"
```

---

## Task 5: Refactor Render Loop for `program_scene_id`

**Files:**
- Modify: `src/main.rs` (render loop in `about_to_wait`)

The render loop currently uses `active_scene_id` for both the editor preview and the live output. It needs to render `active_scene_id` on the primary canvas (for Preview) and `program_scene_id` on the secondary canvas (for Live) when they differ.

- [ ] **Step 1: Refactor the composition section**

In `about_to_wait()`, replace the current dual-canvas logic. The new logic:

1. Always compose `active_scene_id` onto the primary canvas (this is what Preview shows).
2. If `program_scene_id != active_scene_id`, allocate secondary canvas and compose `program_scene_id` onto it.
3. If `program_scene_id == active_scene_id`, deallocate secondary canvas (not needed).
4. During transitions: compose both scenes, run blend pass, write to output texture.
5. Readback should read from the **program** output (primary canvas if same scene, secondary canvas if different, blended output if transitioning).

Key change: the readback/encode path should use the program scene's canvas, not the editing scene's canvas. This means `scale_to_output()` and `start_readback()` should use the secondary canvas texture when `program_scene_id != active_scene_id`.

The compositor needs a way to readback from the secondary canvas. The simplest approach: when program differs from active, after composing secondary canvas, copy it to the output texture (using the existing scale pipeline with a different bind group), then readback from output texture.

- [ ] **Step 2: Update transition completion**

When a transition completes:
- Set `program_scene_id = to_scene` (the transition target becomes live)
- `active_scene_id` stays unchanged (user was editing in Preview)
- If `program_scene_id == active_scene_id`, deallocate secondary canvas
- Stop sources exclusive to the old program scene (that aren't needed by the active scene)

- [ ] **Step 3: Update `LiveResources` bind group per-frame**

At the end of the composition section, update `LiveResources` to point to the correct canvas:
- During transition: use the blended output texture
- When `program_scene_id == active_scene_id`: use primary canvas
- When different: use secondary canvas

- [ ] **Step 4: Source lifecycle — keep both scenes' sources running**

When `active_scene_id` changes (user clicks a scene in Preview), run a source diff:
- Start sources needed by the new active scene that aren't already running
- Stop sources only needed by the old active scene AND not needed by `program_scene_id`

When `program_scene_id` changes (transition completes):
- Start sources needed by the new program scene that aren't already running
- Stop sources only needed by the old program scene AND not needed by `active_scene_id`

The union of both scenes' sources must always be running.

- [ ] **Step 5: Run `cargo build` and tests**

Run: `cargo build && cargo test 2>&1 | tail -20`
Expected: Compiles and all tests pass.

- [ ] **Step 6: Commit**

```bash
git add src/main.rs
git commit -m "refactor: render loop uses program_scene_id for live output, active_scene_id for editor"
```

---

## Task 6: Refactor Scenes Panel for Preview/Live Model

**Files:**
- Modify: `src/ui/scenes_panel.rs`

- [ ] **Step 1: Update scene click behavior**

The `SceneAction::Switch` handler currently has three branches (same scene, studio mode, normal mode). Replace with simpler logic:

Clicking a scene should only change `active_scene_id` (the editing target). It should NOT change `program_scene_id` or trigger a transition. Start sources needed by the new active scene.

```rust
Some(SceneAction::Switch(new_id)) => {
    if state.active_scene_id == Some(new_id) {
        // Already editing this scene — no-op.
    } else {
        let old_scene = state
            .active_scene_id
            .and_then(|id| state.scenes.iter().find(|s| s.id == id))
            .cloned();
        let new_scene = state.scenes.iter().find(|s| s.id == new_id).cloned();

        state.active_scene_id = Some(new_id);
        state.deselect_all();

        // Start new sources, stop sources not needed by either active or program scene.
        // For now, use apply_scene_diff which starts/stops based on old→new.
        // Sources also needed by program_scene_id must NOT be stopped.
        // TODO: The implementer should ensure sources needed by program_scene_id
        // are not removed. Check if source is in program scene before removing.
        apply_scene_diff(
            &cmd_tx,
            &state.library,
            old_scene.as_ref(),
            new_scene.as_ref(),
            state.settings.general.exclude_self_from_capture,
        );

        if let Some(ref scene) = new_scene {
            state.capture_active = !scene.sources.is_empty();
        }
        state.mark_dirty();
    }
}
```

**Important:** The `apply_scene_diff` sends `RemoveCaptureSource` for sources in the old scene but not the new. But if those sources are still needed by `program_scene_id`, they must NOT be stopped. The implementer should modify `apply_scene_diff` to accept an optional set of "protected" source IDs (sources in the program scene), or filter the removals before sending.

- [ ] **Step 2: Update the Transition button logic**

The Transition button in `draw_transition_bar()` currently checks `state.studio_mode`. Replace with:
- Always show the Transition button
- Enabled when `active_scene_id != program_scene_id` and no transition is in-flight
- On click: start transition from `program_scene_id` → `active_scene_id`

- [ ] **Step 3: Remove Studio Mode toggle button**

Remove the "Studio" button from the transition bar. Remove all `state.studio_mode` references.

- [ ] **Step 4: Update PGM/PRV badges**

In `draw_scene_card()`, update badge logic:
- **PGM** (red) on scene matching `state.program_scene_id`
- **PRV** (green) on scene matching `state.active_scene_id`, only when it differs from `program_scene_id`
- When both match, show only PGM

- [ ] **Step 5: Handle program scene deletion**

In `delete_scene_by_id()`, if the deleted scene is `program_scene_id`, set `program_scene_id = active_scene_id` (instant fallback).

- [ ] **Step 6: Initialize `program_scene_id` on first scene creation**

When the first scene is created (or loaded from disk), set `program_scene_id` to match `active_scene_id` if it's `None`.

- [ ] **Step 7: Run `cargo build` and tests**

Run: `cargo build && cargo test 2>&1 | tail -20`
Expected: Compiles and all tests pass.

- [ ] **Step 8: Commit**

```bash
git add src/ui/scenes_panel.rs
git commit -m "refactor: scenes panel uses program_scene_id, always shows Transition button"
```

---

## Task 7: Update Hotkeys

**Files:**
- Modify: `src/main.rs` (keyboard event handling)

- [ ] **Step 1: Remove Ctrl+S Studio Mode toggle**

Find and remove the `KeyCode::KeyS` hotkey handler that toggles `studio_mode`.

- [ ] **Step 2: Update Enter hotkey**

Enter should always trigger a transition from `program_scene_id` → `active_scene_id` (not gated on studio mode). Only fires when they differ and no transition is in-flight.

- [ ] **Step 3: Update Space hotkey**

Space (quick cut) should always push `active_scene_id` to `program_scene_id` instantly. Cancel any in-flight transition. Not gated on studio mode.

- [ ] **Step 4: Update 1-9 hotkeys**

Number keys should only change `active_scene_id` (select scene for editing). They should NOT trigger a transition. Remove the normal-mode transition-triggering branch.

- [ ] **Step 5: Run `cargo build` and tests**

Run: `cargo build && cargo test 2>&1 | tail -20`
Expected: Compiles and all tests pass.

- [ ] **Step 6: Commit**

```bash
git add src/main.rs
git commit -m "refactor: update hotkeys for Preview/Live model, remove Ctrl+S toggle"
```

---

## Task 8: Final Cleanup and Verification

**Files:** All modified files

- [ ] **Step 1: Search for any remaining `studio_mode` references**

Run: `grep -rn "studio_mode" src/`
Expected: Zero results. If any remain, remove them.

- [ ] **Step 2: Search for any remaining `preview_scene_id` references**

Run: `grep -rn "preview_scene_id" src/`
Expected: Zero results. If any remain, replace with `program_scene_id` or remove.

- [ ] **Step 3: Run clippy**

Run: `cargo clippy 2>&1 | tail -30`
Fix any warnings in changed files.

- [ ] **Step 4: Run fmt**

Run: `cargo fmt --check` — fix if needed.

- [ ] **Step 5: Run full test suite**

Run: `cargo test 2>&1 | tail -10`
Expected: All tests pass.

- [ ] **Step 6: Commit any fixes**

```bash
git add -A
git commit -m "chore: final cleanup — remove all studio_mode references, fix clippy/fmt"
```
