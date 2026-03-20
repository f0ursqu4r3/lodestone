# Preview Panel Wiring — Design Spec

## Goal

Wire the existing `PreviewRenderer` wgpu pipeline into the Preview panel so the OBS preview texture displays inside the dockview panel system with correct z-ordering, letterboxing, and multi-panel support.

## Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Rendering approach | egui `CallbackTrait` paint callback | Renders inline with egui's pass — correct z-order with floating panels, popups, overlays. Zero texture copies. |
| Aspect ratio | Letterbox/pillarbox, maintain 16:9 | Matches OBS behavior. No distortion. |
| Letterbox bar color | Black (`Color32::BLACK`) | Visually separates preview from panel chrome. |
| Fullscreen preview pass | Remove | Was Pass 2 in window.rs. No longer needed — preview renders via egui callback. |
| Multiple preview panels | Shared texture, independent viewports | Each panel emits its own callback. Same `Arc` pipeline/bind_group, different viewport rect. |
| Resource sharing | `CallbackResources` type-map on egui renderer | Idiomatic egui_wgpu approach. Avoids putting GPU types in `AppState` (which derives `Debug`). |

## Architecture

### Data flow per frame

```
egui layout phase
  └─ preview_panel::draw()
       ├─ Early return if panel width or height < 1.0
       ├─ Fill panel rect with Color32::BLACK
       ├─ Compute letterboxed rect (16:9 within panel, all in logical points)
       └─ ui.painter().add(Callback::new_paint_callback(letterboxed_rect, PreviewCallback))

egui render phase (inside egui_renderer.render())
  ├─ PreviewCallback::prepare() — no-op (texture already uploaded elsewhere)
  └─ PreviewCallback::paint()
       ├─ Viewport already set by egui (from letterboxed_rect passed to new_paint_callback)
       ├─ Set scissor rect from clip_rect_in_pixels() (REQUIRED — egui does NOT set this for callbacks)
       ├─ Retrieve pipeline + bind_group from callback_resources
       └─ Draw fullscreen quad (0..4 vertices)
```

All coordinates in the layout phase are in egui logical points. The `Callback::new_paint_callback` rect is in points; egui handles points-to-pixels conversion for the viewport automatically.

### Render passes (before → after)

**Before:** Clear → Preview (fullscreen) → egui overlay (3 passes)

**After:** Clear → egui (with inline preview callbacks) (2 passes)

Note: removing Pass 2 from `window.rs` and removing `PreviewRenderer::render()` must happen atomically — they are coupled.

## Changes by file

### `src/renderer/preview.rs` — PreviewRenderer

- Wrap `pipeline: Arc<RenderPipeline>` and `bind_group: Arc<BindGroup>`.
- Add public accessors for the `Arc` values (needed for `CallbackResources` insertion).
- Remove `render(&self, render_pass)` method — no longer called directly.
- Keep `upload_frame()` unchanged.
- Remove `#[allow(dead_code)]` from `width` and `height`, make public — used for aspect ratio calculation.

### `src/ui/preview_panel.rs` — Preview panel + callback

New `PreviewResources` struct (inserted into `CallbackResources` once at init):

```rust
pub struct PreviewResources {
    pub pipeline: Arc<RenderPipeline>,
    pub bind_group: Arc<BindGroup>,
    pub width: u32,
    pub height: u32,
}
```

New `PreviewCallback` struct (created per panel per frame, lightweight):

```rust
struct PreviewCallback;
```

Implements `CallbackTrait`:
- `paint()`: retrieves `PreviewResources` from `callback_resources.get::<PreviewResources>()`, sets scissor rect from `clip_rect_in_pixels()`, binds pipeline and bind group, draws 0..4 vertices. Does NOT set viewport (egui already did).
- `prepare()`: default no-op.

Updated `draw()` function:
1. Early return if panel rect width or height < 1.0.
2. Fill the full panel rect with `Color32::BLACK` (letterbox bars).
3. Read preview dimensions from `AppState` (stored as `preview_width`/`preview_height` — plain u32, no GPU types).
4. Compute letterboxed rect from preview aspect ratio and panel size.
5. Add callback: `ui.painter().add(Callback::new_paint_callback(letterboxed_rect, PreviewCallback))`.

### `src/renderer/mod.rs` — SharedGpuState

- No type changes needed. `preview_renderer` stays as-is.

### `src/state.rs` — AppState

- Add `preview_width: u32` and `preview_height: u32` fields (default 0).
- No GPU types — `AppState` keeps its `#[derive(Debug, Clone)]`.
- Populated after GPU init in `main.rs`.

### `src/window.rs` — Render loop

- Remove Pass 2 (fullscreen preview pass) and `PreviewRenderer::render()` call together.
- Render goes: clear pass → egui pass (which includes preview callbacks).
- After creating `egui_renderer`, insert `PreviewResources` into `egui_renderer.callback_resources`.

### `src/main.rs` — Initialization

- After creating `SharedGpuState`, set `preview_width`/`preview_height` on `AppState`.
- Pass `PreviewResources` to `WindowState::new()` for insertion into `callback_resources`.

## Letterboxing logic

All in logical points (egui handles pixel conversion):

```
Early return if panel_width < 1.0 or panel_height < 1.0.

panel_aspect = panel_width / panel_height
preview_aspect = preview_width as f32 / preview_height as f32

If panel_aspect > preview_aspect (panel is wider):
    Pillarbox: preview_h = panel_h, preview_w = panel_h * preview_aspect
    Black bars on left and right

If panel_aspect < preview_aspect (panel is taller):
    Letterbox: preview_w = panel_w, preview_h = panel_w / preview_aspect
    Black bars on top and bottom

Center the computed rect within the panel rect.
```

Precondition: `preview_width` and `preview_height` must be > 0 (guaranteed by `SharedGpuState::new` which initializes to 1920x1080).

## Testing

- Verify preview displays inside the panel with correct aspect ratio.
- Resize the panel to various aspect ratios — confirm letterbox/pillarbox bars appear correctly in black.
- Open two Preview tabs — both should display the same texture.
- Float a panel over the preview — confirm the floating panel renders on top (z-order).
- Close all preview panels — confirm no errors, no orphaned GPU resources.
- Upload a new frame via `upload_frame()` — confirm it updates in all open preview panels.
- Resize panel to near-zero width or height — confirm no panics or visual artifacts.
