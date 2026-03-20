# Preview Panel Wiring Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire the existing `PreviewRenderer` wgpu pipeline into the Preview panel using egui's `CallbackTrait`, so the OBS preview texture displays inside dockview panels with correct z-ordering and letterboxing.

**Architecture:** The preview panel emits an `egui_wgpu::Callback` paint callback during the egui layout phase. During the egui render phase, the callback draws the preview texture into a letterboxed viewport using the existing wgpu pipeline and bind group, retrieved from `CallbackResources`. The current fullscreen preview render pass (Pass 2) is removed.

**Tech Stack:** Rust, wgpu (via egui_wgpu re-export), egui 0.33, egui_wgpu 0.33.3

**Spec:** `docs/superpowers/specs/2026-03-20-preview-panel-wiring-design.md`

---

### Task 1: Add preview dimensions to AppState

**Files:**
- Modify: `src/state.rs:41-48` (AppState struct)
- Modify: `src/state.rs:50-61` (Default impl)

- [ ] **Step 1: Add `preview_width` and `preview_height` fields to `AppState`**

In `src/state.rs`, add two fields to the `AppState` struct:

```rust
#[derive(Debug, Clone)]
pub struct AppState {
    pub scenes: Vec<Scene>,
    pub sources: Vec<Source>,
    pub active_scene_id: Option<SceneId>,
    pub audio_levels: Vec<AudioLevel>,
    pub stream_status: StreamStatus,
    pub settings: AppSettings,
    pub preview_width: u32,
    pub preview_height: u32,
}
```

Update the `Default` impl:

```rust
impl Default for AppState {
    fn default() -> Self {
        Self {
            scenes: Vec::new(),
            sources: Vec::new(),
            active_scene_id: None,
            audio_levels: Vec::new(),
            stream_status: StreamStatus::Offline,
            settings: AppSettings::default(),
            preview_width: 0,
            preview_height: 0,
        }
    }
}
```

- [ ] **Step 2: Run tests to verify nothing breaks**

Run: `cargo test`
Expected: All existing tests pass. The new fields have sensible defaults (0).

- [ ] **Step 3: Commit**

```bash
git add src/state.rs
git commit -m "feat: add preview_width/preview_height to AppState"
```

---

### Task 2: Implement preview callback, wire into render loop, remove fullscreen pass

This task modifies four files atomically to keep the build green at every commit.

**Files:**
- Modify: `src/renderer/preview.rs` (Arc-wrap fields, add accessors, remove render method)
- Rewrite: `src/ui/preview_panel.rs` (PreviewResources, PreviewCallback, letterboxing, draw)
- Modify: `src/window.rs` (accept PreviewResources, insert into callback_resources, remove Pass 2)
- Modify: `src/main.rs` (create PreviewResources, set dimensions, pass to WindowState)

- [ ] **Step 1: Arc-wrap PreviewRenderer fields and add accessors**

In `src/renderer/preview.rs`, add the import:

```rust
use std::sync::Arc;
```

Change the struct fields (remove `#[allow(dead_code)]` from width/height):

```rust
pub struct PreviewRenderer {
    texture: wgpu::Texture,
    bind_group: Arc<wgpu::BindGroup>,
    pipeline: Arc<wgpu::RenderPipeline>,
    pub width: u32,
    pub height: u32,
}
```

In `new()`, wrap the constructed values:

```rust
        Self {
            texture,
            bind_group: Arc::new(bind_group),
            pipeline: Arc::new(pipeline),
            width,
            height,
        }
```

Add public accessors after `upload_frame`:

```rust
    /// Arc-wrapped pipeline for sharing with egui paint callbacks.
    pub fn pipeline(&self) -> Arc<wgpu::RenderPipeline> {
        Arc::clone(&self.pipeline)
    }

    /// Arc-wrapped bind group for sharing with egui paint callbacks.
    pub fn bind_group(&self) -> Arc<wgpu::BindGroup> {
        Arc::clone(&self.bind_group)
    }
```

Delete the `render` method entirely (lines 187-191):

```rust
    // DELETE THIS:
    pub fn render<'a>(&'a self, render_pass: &mut wgpu::RenderPass<'a>) {
        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.bind_group, &[]);
        render_pass.draw(0..4, 0..1);
    }
```

- [ ] **Step 2: Write the preview panel with callback**

Replace the contents of `src/ui/preview_panel.rs` with:

```rust
use std::sync::Arc;

use egui_wgpu::wgpu;
use egui_wgpu::{Callback, CallbackResources, CallbackTrait};

use crate::state::AppState;
use crate::ui::layout::PanelId;

/// GPU resources for the preview callback, stored in `egui_renderer.callback_resources`.
pub struct PreviewResources {
    pub pipeline: Arc<wgpu::RenderPipeline>,
    pub bind_group: Arc<wgpu::BindGroup>,
}

/// Lightweight struct emitted per preview panel per frame.
struct PreviewCallback;

impl CallbackTrait for PreviewCallback {
    fn paint(
        &self,
        info: egui::PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'static>,
        callback_resources: &CallbackResources,
    ) {
        let Some(resources) = callback_resources.get::<PreviewResources>() else {
            return;
        };

        // Set scissor rect — egui does NOT set this for callbacks.
        let clip = info.clip_rect_in_pixels();
        if clip.width_px > 0 && clip.height_px > 0 {
            render_pass.set_scissor_rect(
                clip.left_px as u32,
                clip.top_px as u32,
                clip.width_px as u32,
                clip.height_px as u32,
            );
        }

        render_pass.set_pipeline(&resources.pipeline);
        render_pass.set_bind_group(0, &resources.bind_group, &[]);
        render_pass.draw(0..4, 0..1);
    }
}

/// Compute the largest rect matching the preview aspect ratio that fits
/// inside `panel`, centered, with black bars for the remainder.
fn letterboxed_rect(panel: egui::Rect, preview_width: u32, preview_height: u32) -> egui::Rect {
    let panel_w = panel.width();
    let panel_h = panel.height();
    let preview_aspect = preview_width as f32 / preview_height as f32;
    let panel_aspect = panel_w / panel_h;

    let (w, h) = if panel_aspect > preview_aspect {
        // Panel is wider — pillarbox
        (panel_h * preview_aspect, panel_h)
    } else {
        // Panel is taller — letterbox
        (panel_w, panel_w / preview_aspect)
    };

    let center = panel.center();
    egui::Rect::from_center_size(center, egui::vec2(w, h))
}

pub fn draw(ui: &mut egui::Ui, state: &mut AppState, _panel_id: PanelId) {
    let panel_rect = ui.available_rect_before_wrap();

    // Guard against degenerate panels
    if panel_rect.width() < 1.0 || panel_rect.height() < 1.0 {
        return;
    }

    // Guard against uninitialized preview dimensions
    if state.preview_width == 0 || state.preview_height == 0 {
        ui.centered_and_justified(|ui| {
            ui.label("No preview");
        });
        return;
    }

    // Fill entire panel with black (letterbox bars)
    ui.painter()
        .rect_filled(panel_rect, 0.0, egui::Color32::BLACK);

    // Compute letterboxed rect and emit the paint callback
    let preview_rect = letterboxed_rect(panel_rect, state.preview_width, state.preview_height);

    ui.painter().add(Callback::new_paint_callback(
        preview_rect,
        PreviewCallback,
    ));

    // Allocate the space so egui knows it's used
    ui.allocate_rect(panel_rect, egui::Sense::hover());
}
```

- [ ] **Step 3: Update window.rs — accept PreviewResources and remove Pass 2**

In `src/window.rs`, add the import at the top:

```rust
use crate::ui::preview_panel::PreviewResources;
```

Change the `WindowState::new` signature to accept preview resources:

```rust
    pub fn new(
        window: &'static Window,
        gpu: &SharedGpuState,
        layout: DockLayout,
        is_main: bool,
        preview_resources: Option<PreviewResources>,
    ) -> Result<Self> {
```

Change `let egui_renderer` to `let mut egui_renderer` and insert resources after creation:

```rust
        let mut egui_renderer = egui_wgpu::Renderer::new(
            &gpu.device,
            gpu.format,
            egui_wgpu::RendererOptions::default(),
        );

        if let Some(resources) = preview_resources {
            egui_renderer.callback_resources.insert(resources);
        }
```

Delete the entire Pass 2 block (lines 340-360):

```rust
        // DELETE THIS ENTIRE BLOCK:
        // Pass 2: Preview texture (fullscreen, behind everything)
        {
            let mut preview_pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("preview_pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                })
                .forget_lifetime();
            gpu.preview_renderer.render(&mut preview_pass);
        }
```

Update the comment on the remaining egui pass from "Pass 3" to "Pass 2":

```rust
        // Pass 2: egui (includes preview via paint callbacks)
```

- [ ] **Step 4: Update main.rs — create PreviewResources and pass to WindowState**

In `src/main.rs`, add the import:

```rust
use ui::preview_panel::PreviewResources;
```

In the `resumed` method, after `SharedGpuState` is created and before `WindowState::new`, set the preview dimensions on AppState and create the resources:

```rust
        let gpu =
            pollster::block_on(SharedGpuState::new(window)).expect("initialize shared GPU state");

        // Set preview dimensions on AppState
        {
            let mut app_state = self.state.lock().unwrap();
            app_state.preview_width = gpu.preview_renderer.width;
            app_state.preview_height = gpu.preview_renderer.height;
        }

        let preview_resources = PreviewResources {
            pipeline: gpu.preview_renderer.pipeline(),
            bind_group: gpu.preview_renderer.bind_group(),
        };

        // Try to load saved layout; fall back to default.
        let layout = Self::load_layout();
        let win_state = WindowState::new(window, &gpu, layout, true, Some(preview_resources))
            .expect("create main window state");
```

In the `about_to_wait` method where detached windows are created, pass preview resources:

```rust
                let preview_resources = PreviewResources {
                    pipeline: gpu.preview_renderer.pipeline(),
                    bind_group: gpu.preview_renderer.bind_group(),
                };
                let win_state = WindowState::new(window, gpu, layout, false, Some(preview_resources))
                    .expect("init detached window");
```

- [ ] **Step 5: Build and verify**

Run: `cargo build`
Expected: Clean compile, zero errors.

- [ ] **Step 6: Run all tests**

Run: `cargo test`
Expected: All tests pass.

- [ ] **Step 7: Run clippy**

Run: `cargo clippy`
Expected: No new warnings.

- [ ] **Step 8: Commit**

```bash
git add src/renderer/preview.rs src/ui/preview_panel.rs src/window.rs src/main.rs
git commit -m "feat: wire preview into egui paint callback, remove fullscreen preview pass"
```

---

### Task 3: Manual verification

- [ ] **Step 1: Run the app and verify preview displays**

Run: `cargo run`

Verify:
- The Preview panel shows the dark gray test frame (not just a label).
- The preview maintains 16:9 aspect ratio inside the panel.
- Black letterbox/pillarbox bars appear when resizing the panel to non-16:9 shapes.

- [ ] **Step 2: Verify z-ordering with floating panels**

- Detach a panel to float (right-click tab → "Detach to Float").
- Drag the floating panel over the Preview panel.
- Verify the floating panel renders ON TOP of the preview (not behind it).

- [ ] **Step 3: Verify multiple preview panels**

- Open a second Preview tab (View → Add Panel → Preview, or use the "+" button).
- Verify both preview panels show the same texture.
- Close one preview panel — the other should still work.

- [ ] **Step 4: Verify edge cases**

- Resize the preview panel to be very narrow (near-zero width) — should not panic.
- Resize the preview panel to be very short (near-zero height) — should not panic.
- Close all preview panels — no errors in the console.

- [ ] **Step 5: Verify frame updates propagate**

- The mock driver updates audio levels — confirm the app runs without errors.
- If you add a temporary test in `mock_driver.rs` to upload a different colored frame via `upload_frame()`, confirm both preview panels update.

- [ ] **Step 6: Final commit if any cleanup is needed**

If any issues are found and fixed during manual testing, commit the fixes.
