use std::sync::Arc;

use egui_wgpu::wgpu;
use egui_wgpu::{Callback, CallbackResources, CallbackTrait};

use crate::state::{AppState, StreamStatus};
use crate::ui::layout::PanelId;
use crate::ui::theme::active_theme;

// ── Zoom levels ──────────────────────────────────────────────────────────────

/// Discrete zoom levels used for scroll-wheel stepping.
const ZOOM_LEVELS: &[f32] = &[0.1, 0.25, 0.33, 0.5, 0.67, 0.75, 1.0, 1.5, 2.0, 3.0, 4.0];
const ZOOM_MIN: f32 = 0.1;
const ZOOM_MAX: f32 = 4.0;

// ── Per-session zoom/pan state (stored in egui temp memory) ──────────────────

/// Ephemeral zoom/pan state for the preview panel.
/// Stored in egui's per-frame data store so it persists across frames but
/// is not serialized to disk.
#[derive(Clone)]
struct PreviewViewState {
    /// Multiplier on fit-to-panel. 1.0 = fit canvas to panel.
    zoom: f32,
    /// Canvas-space offset from center.
    pan_offset: egui::Vec2,
    /// Whether spacebar hand-tool mode is active.
    space_held: bool,
}

impl Default for PreviewViewState {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            pan_offset: egui::Vec2::ZERO,
            space_held: false,
        }
    }
}

// ── GPU callback ─────────────────────────────────────────────────────────────

/// GPU resources for the preview callback, stored in `egui_renderer.callback_resources`.
///
/// Pipeline and bind group are `Arc`-cloned from [`crate::renderer::preview::PreviewRenderer`]
/// and shared across all windows.
pub struct PreviewResources {
    pub pipeline: Arc<wgpu::RenderPipeline>,
    pub bind_group: Arc<wgpu::BindGroup>,
    /// Bind group for the secondary canvas (Studio Mode / transition preview).
    /// None when secondary canvas is not allocated.
    pub secondary_bind_group: Option<Arc<wgpu::BindGroup>>,
}

/// Which canvas bind group a preview callback should sample.
#[derive(Clone, Copy, PartialEq, Eq)]
enum CanvasTarget {
    /// Primary canvas — live program output.
    Primary,
    /// Secondary canvas — Studio Mode preview scene.
    Secondary,
}

/// Lightweight struct emitted per preview panel per frame.
/// Carries the zoomed viewport rect so we can set the wgpu viewport manually
/// instead of relying on egui's viewport (which gets clamped on off-screen rects).
struct PreviewCallback {
    /// The zoomed preview rect in logical points (may extend beyond the window).
    zoomed_rect: egui::Rect,
    /// Which canvas to sample.
    target: CanvasTarget,
}

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

        let bind_group = match self.target {
            CanvasTarget::Primary => &resources.bind_group,
            CanvasTarget::Secondary => {
                let Some(ref bg) = resources.secondary_bind_group else {
                    return;
                };
                bg
            }
        };

        // Set scissor to the clip rect (panel bounds) so nothing draws outside.
        let clip = info.clip_rect_in_pixels();
        if clip.width_px > 0 && clip.height_px > 0 {
            render_pass.set_scissor_rect(
                clip.left_px as u32,
                clip.top_px as u32,
                clip.width_px as u32,
                clip.height_px as u32,
            );
        }

        // Override the viewport to the zoomed rect, converting from logical
        // points to physical pixels ourselves.  This avoids egui's viewport
        // clamping which distorts the fullscreen quad when the rect extends
        // beyond the window.
        //
        // Clamp dimensions to max_texture_dimension_2d (8192) to avoid wgpu
        // validation errors on high-DPI displays at high zoom. The scissor
        // rect clips the visible output regardless.
        let ppp = info.pixels_per_point;
        let vp_x = self.zoomed_rect.min.x * ppp;
        let vp_y = self.zoomed_rect.min.y * ppp;
        let vp_w = (self.zoomed_rect.width() * ppp).min(8192.0);
        let vp_h = (self.zoomed_rect.height() * ppp).min(8192.0);
        if vp_w > 0.0 && vp_h > 0.0 {
            render_pass.set_viewport(vp_x, vp_y, vp_w, vp_h, 0.0, 1.0);
        }

        render_pass.set_pipeline(&resources.pipeline);
        render_pass.set_bind_group(0, &**bind_group, &[]);
        render_pass.draw(0..4, 0..1);
    }
}

// ── Viewport helpers ─────────────────────────────────────────────────────────

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

/// Apply zoom and pan to the base letterboxed rect.
fn zoomed_viewport(
    panel: egui::Rect,
    canvas_w: u32,
    canvas_h: u32,
    zoom: f32,
    pan: egui::Vec2,
) -> egui::Rect {
    let base = letterboxed_rect(panel, canvas_w, canvas_h);
    let base_size = base.size();
    let zoomed_size = base_size * zoom;
    let pixels_per_canvas = zoomed_size.x / canvas_w as f32;
    let screen_pan = pan * pixels_per_canvas;
    let center = panel.center() + screen_pan;
    egui::Rect::from_center_size(center, zoomed_size)
}

/// Convert a screen position to canvas coordinates using the given viewport.
fn screen_to_canvas(
    screen_pos: egui::Pos2,
    viewport: egui::Rect,
    canvas_w: u32,
    canvas_h: u32,
) -> egui::Pos2 {
    egui::Pos2::new(
        (screen_pos.x - viewport.min.x) * canvas_w as f32 / viewport.width(),
        (screen_pos.y - viewport.min.y) * canvas_h as f32 / viewport.height(),
    )
}

/// Find the next zoom level up from the current zoom.
fn zoom_level_up(current: f32) -> f32 {
    for &level in ZOOM_LEVELS {
        if level > current + 0.001 {
            return level;
        }
    }
    ZOOM_MAX
}

/// Find the next zoom level down from the current zoom.
fn zoom_level_down(current: f32) -> f32 {
    for &level in ZOOM_LEVELS.iter().rev() {
        if level < current - 0.001 {
            return level;
        }
    }
    ZOOM_MIN
}

/// Clamp pan so at most 90% of the canvas can be off-screen in any direction,
/// keeping at least 10% visible. Expressed in canvas-space coordinates.
fn clamp_pan(
    pan: egui::Vec2,
    _panel: egui::Rect,
    canvas_w: u32,
    canvas_h: u32,
    _zoom: f32,
) -> egui::Vec2 {
    let max_x = canvas_w as f32 * 0.45;
    let max_y = canvas_h as f32 * 0.45;
    egui::Vec2::new(pan.x.clamp(-max_x, max_x), pan.y.clamp(-max_y, max_y))
}

// ── Canvas-to-screen mapping (mirrors transform_handles) ────────────────────

fn canvas_to_screen(
    canvas_pos: egui::Pos2,
    viewport: egui::Rect,
    canvas_size: egui::Vec2,
) -> egui::Pos2 {
    egui::Pos2::new(
        viewport.min.x + canvas_pos.x * viewport.width() / canvas_size.x,
        viewport.min.y + canvas_pos.y * viewport.height() / canvas_size.y,
    )
}

// ── Grid / Thirds / Safe-zones ───────────────────────────────────────────────

/// Draw a pixel-based or preset grid overlay.
fn draw_grid(
    painter: &egui::Painter,
    viewport: egui::Rect,
    canvas_size: egui::Vec2,
    grid_preset: &str,
    snap_grid_size: f32,
    grid_color: [u8; 3],
    grid_opacity: f32,
) {
    let cw = canvas_size.x;
    let ch = canvas_size.y;

    // Compute grid line positions in canvas space.
    let (x_lines, y_lines): (Vec<f32>, Vec<f32>) = match grid_preset {
        "thirds" => (
            vec![cw / 3.0, 2.0 * cw / 3.0],
            vec![ch / 3.0, 2.0 * ch / 3.0],
        ),
        "quarters" => (
            vec![cw / 4.0, cw / 2.0, 3.0 * cw / 4.0],
            vec![ch / 4.0, ch / 2.0, 3.0 * ch / 4.0],
        ),
        preset => {
            let step = match preset {
                "8" => 8.0_f32,
                "16" => 16.0,
                "32" => 32.0,
                "64" => 64.0,
                _ => snap_grid_size.max(1.0), // "custom" or empty
            };

            // Auto-hide when lines are too dense (< 4 screen pixels apart).
            let screen_step = step * viewport.width() / cw;
            if screen_step < 4.0 {
                return;
            }

            let mut xs = Vec::new();
            let mut x = step;
            while x < cw {
                xs.push(x);
                x += step;
            }
            let mut ys = Vec::new();
            let mut y = step;
            while y < ch {
                ys.push(y);
                y += step;
            }
            (xs, ys)
        }
    };

    let is_numeric = matches!(grid_preset, "8" | "16" | "32" | "64" | "custom" | "");

    for (i, &gx) in x_lines.iter().enumerate() {
        let screen_x = canvas_to_screen(egui::pos2(gx, 0.0), viewport, canvas_size).x;
        // Major lines every 4th division (only for numeric/custom grids).
        let is_major = is_numeric && ((i + 1) % 4 == 0);
        let alpha = if is_major {
            (grid_opacity * 1.5).min(1.0)
        } else {
            grid_opacity
        };
        let color = egui::Color32::from_rgba_unmultiplied(
            grid_color[0],
            grid_color[1],
            grid_color[2],
            (255.0 * alpha) as u8,
        );
        let width = if is_major { 1.0 } else { 0.5 };
        painter.line_segment(
            [
                egui::pos2(screen_x, viewport.top()),
                egui::pos2(screen_x, viewport.bottom()),
            ],
            egui::Stroke::new(width, color),
        );
    }

    for (i, &gy) in y_lines.iter().enumerate() {
        let screen_y = canvas_to_screen(egui::pos2(0.0, gy), viewport, canvas_size).y;
        let is_major = is_numeric && ((i + 1) % 4 == 0);
        let alpha = if is_major {
            (grid_opacity * 1.5).min(1.0)
        } else {
            grid_opacity
        };
        let color = egui::Color32::from_rgba_unmultiplied(
            grid_color[0],
            grid_color[1],
            grid_color[2],
            (255.0 * alpha) as u8,
        );
        let width = if is_major { 1.0 } else { 0.5 };
        painter.line_segment(
            [
                egui::pos2(viewport.left(), screen_y),
                egui::pos2(viewport.right(), screen_y),
            ],
            egui::Stroke::new(width, color),
        );
    }
}

/// Draw rule-of-thirds overlay (2 horizontal + 2 vertical lines).
fn draw_thirds(
    painter: &egui::Painter,
    viewport: egui::Rect,
    canvas_size: egui::Vec2,
    guide_color: [u8; 3],
    guide_opacity: f32,
) {
    let alpha = (guide_opacity * 0.6).min(1.0);
    let color = egui::Color32::from_rgba_unmultiplied(
        guide_color[0],
        guide_color[1],
        guide_color[2],
        (255.0 * alpha) as u8,
    );
    let stroke = egui::Stroke::new(1.0, color);

    for i in 1..=2 {
        let frac = i as f32 / 3.0;
        let sx = canvas_to_screen(egui::pos2(canvas_size.x * frac, 0.0), viewport, canvas_size).x;
        painter.line_segment(
            [
                egui::pos2(sx, viewport.top()),
                egui::pos2(sx, viewport.bottom()),
            ],
            stroke,
        );

        let sy = canvas_to_screen(egui::pos2(0.0, canvas_size.y * frac), viewport, canvas_size).y;
        painter.line_segment(
            [
                egui::pos2(viewport.left(), sy),
                egui::pos2(viewport.right(), sy),
            ],
            stroke,
        );
    }
}

/// Draw action-safe (90%) and title-safe (80%) zone outlines.
fn draw_safe_zones(
    painter: &egui::Painter,
    viewport: egui::Rect,
    canvas_size: egui::Vec2,
    guide_color: [u8; 3],
) {
    // Action-safe: 90% of canvas (5% margin each side)
    let action_min = canvas_to_screen(
        egui::pos2(canvas_size.x * 0.05, canvas_size.y * 0.05),
        viewport,
        canvas_size,
    );
    let action_max = canvas_to_screen(
        egui::pos2(canvas_size.x * 0.95, canvas_size.y * 0.95),
        viewport,
        canvas_size,
    );
    let action_rect = egui::Rect::from_min_max(action_min, action_max);
    let action_color = egui::Color32::from_rgba_unmultiplied(
        guide_color[0],
        guide_color[1],
        guide_color[2],
        (255.0 * 0.4) as u8,
    );
    painter.rect_stroke(
        action_rect,
        0.0,
        egui::Stroke::new(1.0, action_color),
        egui::StrokeKind::Inside,
    );

    // Title-safe: 80% of canvas (10% margin each side)
    let title_min = canvas_to_screen(
        egui::pos2(canvas_size.x * 0.10, canvas_size.y * 0.10),
        viewport,
        canvas_size,
    );
    let title_max = canvas_to_screen(
        egui::pos2(canvas_size.x * 0.90, canvas_size.y * 0.90),
        viewport,
        canvas_size,
    );
    let title_rect = egui::Rect::from_min_max(title_min, title_max);
    let title_color = egui::Color32::from_rgba_unmultiplied(
        guide_color[0],
        guide_color[1],
        guide_color[2],
        (255.0 * 0.3) as u8,
    );
    painter.rect_stroke(
        title_rect,
        0.0,
        egui::Stroke::new(1.0, title_color),
        egui::StrokeKind::Inside,
    );
}

// ── Public draw entry point ──────────────────────────────────────────────────

pub fn draw(ui: &mut egui::Ui, state: &mut AppState, _panel_id: PanelId) {
    // Disable scrollbars — all content is painted via the painter and doesn't
    // need to scroll. Without this, floating-point rounding can cause egui to
    // think content overflows by ~1px and show unwanted scrollbars.
    egui::ScrollArea::neither()
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
        .show(ui, |ui| {
            draw_inner(ui, state);
        });
}

fn draw_inner(ui: &mut egui::Ui, state: &mut AppState) {
    let theme = active_theme(ui.ctx());
    let panel_rect = ui.available_rect_before_wrap();

    // Guard against degenerate panels
    if panel_rect.width() < 1.0 || panel_rect.height() < 1.0 {
        return;
    }

    // Read canvas resolution from settings for correct letterboxing.
    let (preview_width, preview_height) = {
        let base = &state.settings.video.base_resolution;
        let parts: Vec<&str> = base.split('x').collect();
        let w = parts
            .first()
            .and_then(|s| s.parse().ok())
            .unwrap_or(1920u32);
        let h = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(1080u32);
        (w, h)
    };

    // ── Retrieve / update zoom-pan state from egui temp memory ──

    let view_id = egui::Id::new("preview_view_state");
    let mut view: PreviewViewState = ui
        .ctx()
        .data(|d| d.get_temp::<PreviewViewState>(view_id))
        .unwrap_or_default();

    // ── Handle AppState zoom signals (from keyboard shortcuts) ──

    let base_rect = letterboxed_rect(panel_rect, preview_width, preview_height);

    if state.reset_preview_zoom {
        view.zoom = 1.0;
        view.pan_offset = egui::Vec2::ZERO;
        state.reset_preview_zoom = false;
    }
    if state.set_preview_zoom_100 {
        // Calculate zoom for 1:1 pixel mapping: canvas pixels == screen pixels
        let zoom_100 = preview_width as f32 / base_rect.width();
        view.zoom = zoom_100.clamp(ZOOM_MIN, ZOOM_MAX);
        view.pan_offset = egui::Vec2::ZERO;
        state.set_preview_zoom_100 = false;
    }

    // ── Spacebar tracking (hand tool) ──

    let wants_keyboard = ui.ctx().wants_keyboard_input();
    if !wants_keyboard {
        ui.input(|i| {
            if i.key_pressed(egui::Key::Space) {
                view.space_held = true;
            }
            if i.key_released(egui::Key::Space) {
                view.space_held = false;
            }
        });
    } else {
        // If egui wants keyboard, release space
        view.space_held = false;
    }

    let cursor_in_panel = ui
        .input(|i| i.pointer.hover_pos())
        .is_some_and(|p| panel_rect.contains(p));

    // ── Scroll: pan (plain) or zoom (Cmd+scroll) ──
    //
    // On macOS, trackpad two-finger scroll generates `raw_scroll_delta`.
    // Plain scroll → pan (Figma/Photoshop convention).
    // Cmd+scroll   → discrete zoom, cursor-centered.

    if cursor_in_panel {
        let (scroll_delta_x, scroll_delta_y, cmd_held) = ui.input(|i| {
            (
                i.raw_scroll_delta.x,
                i.raw_scroll_delta.y,
                i.modifiers.command,
            )
        });

        if cmd_held {
            // Cmd+scroll → zoom (cursor-centered, discrete steps)
            if scroll_delta_y.abs() > 0.5 {
                let old_zoom = view.zoom;
                let new_zoom = if scroll_delta_y > 0.0 {
                    zoom_level_up(old_zoom)
                } else {
                    zoom_level_down(old_zoom)
                };

                if let Some(cursor_pos) = ui.input(|i| i.pointer.hover_pos()) {
                    let old_viewport = zoomed_viewport(
                        panel_rect,
                        preview_width,
                        preview_height,
                        old_zoom,
                        view.pan_offset,
                    );
                    let canvas_pos =
                        screen_to_canvas(cursor_pos, old_viewport, preview_width, preview_height);

                    view.zoom = new_zoom;

                    let new_viewport = zoomed_viewport(
                        panel_rect,
                        preview_width,
                        preview_height,
                        new_zoom,
                        view.pan_offset,
                    );
                    let new_screen = egui::Pos2::new(
                        new_viewport.min.x
                            + canvas_pos.x * new_viewport.width() / preview_width as f32,
                        new_viewport.min.y
                            + canvas_pos.y * new_viewport.height() / preview_height as f32,
                    );

                    let diff = cursor_pos - new_screen;
                    let pixels_per_canvas = (base_rect.width() * new_zoom) / preview_width as f32;
                    view.pan_offset.x += diff.x / pixels_per_canvas;
                    view.pan_offset.y += diff.y / pixels_per_canvas;
                } else {
                    view.zoom = new_zoom;
                }

                view.pan_offset = clamp_pan(
                    view.pan_offset,
                    panel_rect,
                    preview_width,
                    preview_height,
                    view.zoom,
                );
            }
        } else if scroll_delta_x.abs() > 0.5 || scroll_delta_y.abs() > 0.5 {
            // Plain scroll → pan (same delta-to-canvas conversion as drag pan)
            let pixels_per_canvas = (base_rect.width() * view.zoom) / preview_width as f32;
            view.pan_offset.x += scroll_delta_x / pixels_per_canvas;
            view.pan_offset.y += scroll_delta_y / pixels_per_canvas;
            view.pan_offset = clamp_pan(
                view.pan_offset,
                panel_rect,
                preview_width,
                preview_height,
                view.zoom,
            );
        }
    }

    // ── Trackpad pinch zoom ──

    if cursor_in_panel
        && let Some(touch) = ui.input(|i| i.multi_touch())
        && (touch.zoom_delta - 1.0).abs() > 0.001
    {
        let old_zoom = view.zoom;
        let new_zoom = (old_zoom * touch.zoom_delta).clamp(ZOOM_MIN, ZOOM_MAX);

        // Cursor-centered pinch zoom
        if let Some(cursor_pos) = ui.input(|i| i.pointer.hover_pos()) {
            let old_viewport = zoomed_viewport(
                panel_rect,
                preview_width,
                preview_height,
                old_zoom,
                view.pan_offset,
            );
            let canvas_pos =
                screen_to_canvas(cursor_pos, old_viewport, preview_width, preview_height);

            view.zoom = new_zoom;

            let new_viewport = zoomed_viewport(
                panel_rect,
                preview_width,
                preview_height,
                new_zoom,
                view.pan_offset,
            );
            let new_screen = egui::Pos2::new(
                new_viewport.min.x + canvas_pos.x * new_viewport.width() / preview_width as f32,
                new_viewport.min.y + canvas_pos.y * new_viewport.height() / preview_height as f32,
            );

            let diff = cursor_pos - new_screen;
            let pixels_per_canvas = (base_rect.width() * new_zoom) / preview_width as f32;
            view.pan_offset.x += diff.x / pixels_per_canvas;
            view.pan_offset.y += diff.y / pixels_per_canvas;
        } else {
            view.zoom = new_zoom;
        }

        view.pan_offset = clamp_pan(
            view.pan_offset,
            panel_rect,
            preview_width,
            preview_height,
            view.zoom,
        );
    }

    // ── Pan (middle-click drag or spacebar + left-click drag) ──

    let middle_down = ui.input(|i| i.pointer.middle_down());
    let space_left_down = view.space_held && ui.input(|i| i.pointer.primary_down());
    let is_panning = cursor_in_panel && (middle_down || space_left_down);

    if is_panning {
        let delta = ui.input(|i| i.pointer.delta());
        if delta.length_sq() > 0.0 {
            // Convert screen delta to canvas delta
            let pixels_per_canvas = (base_rect.width() * view.zoom) / preview_width as f32;
            view.pan_offset.x += delta.x / pixels_per_canvas;
            view.pan_offset.y += delta.y / pixels_per_canvas;
            view.pan_offset = clamp_pan(
                view.pan_offset,
                panel_rect,
                preview_width,
                preview_height,
                view.zoom,
            );
        }
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
    } else if view.space_held && cursor_in_panel {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
    }

    // ── Compute the zoomed viewport ──

    let preview_rect = zoomed_viewport(
        panel_rect,
        preview_width,
        preview_height,
        view.zoom,
        view.pan_offset,
    );

    // Store view state back for next frame
    ui.ctx().data_mut(|d| d.insert_temp(view_id, view.clone()));

    // ── Studio Mode or single-pane GPU paint callback ──

    // Use Studio Mode dual-pane layout when studio_mode is on.
    let studio_dual = state.studio_mode;

    let canvas_size = egui::Vec2::new(preview_width as f32, preview_height as f32);

    if studio_dual {
        // Split panel into left (Preview) and right (Program) panes.
        let gap = 6.0;
        let half_w = (panel_rect.width() - gap) / 2.0;

        let left_rect =
            egui::Rect::from_min_size(panel_rect.min, egui::vec2(half_w, panel_rect.height()));
        let right_rect = egui::Rect::from_min_size(
            egui::pos2(panel_rect.min.x + half_w + gap, panel_rect.min.y),
            egui::vec2(panel_rect.width() - half_w - gap, panel_rect.height()),
        );

        // Left pane — Preview (secondary canvas, upcoming scene)
        let left_zoomed = letterboxed_rect(left_rect, preview_width, preview_height);
        ui.painter_at(left_rect).add(Callback::new_paint_callback(
            left_rect,
            PreviewCallback {
                zoomed_rect: left_zoomed,
                target: CanvasTarget::Secondary,
            },
        ));

        // Right pane — Program (primary canvas, live output)
        let right_zoomed = letterboxed_rect(right_rect, preview_width, preview_height);
        ui.painter_at(right_rect).add(Callback::new_paint_callback(
            right_rect,
            PreviewCallback {
                zoomed_rect: right_zoomed,
                target: CanvasTarget::Primary,
            },
        ));

        // ── Pane labels ──
        let label_font = egui::FontId::new(9.0, egui::FontFamily::Proportional);
        let label_pad = egui::vec2(5.0, 3.0);
        let label_painter = ui.painter_at(panel_rect);

        // "PREVIEW" label — green, top-left of left pane
        {
            let preview_color = egui::Color32::from_rgb(80, 200, 120);
            let galley = label_painter.layout_no_wrap(
                "PREVIEW".to_string(),
                label_font.clone(),
                preview_color,
            );
            let bg_size = galley.size() + label_pad * 2.0;
            let bg_pos = left_rect.left_top() + egui::vec2(6.0, 6.0);
            let bg_rect = egui::Rect::from_min_size(bg_pos, bg_size);
            let bg = egui::Color32::from_rgba_premultiplied(0, 0, 0, 160);
            label_painter.rect_filled(bg_rect, 3.0, bg);
            label_painter.galley(bg_pos + label_pad, galley, preview_color);
        }

        // "PROGRAM" label — red, top-left of right pane
        {
            let program_color = egui::Color32::from_rgb(220, 80, 80);
            let galley =
                label_painter.layout_no_wrap("PROGRAM".to_string(), label_font, program_color);
            let bg_size = galley.size() + label_pad * 2.0;
            let bg_pos = right_rect.left_top() + egui::vec2(6.0, 6.0);
            let bg_rect = egui::Rect::from_min_size(bg_pos, bg_size);
            let bg = egui::Color32::from_rgba_premultiplied(0, 0, 0, 160);
            label_painter.rect_filled(bg_rect, 3.0, bg);
            label_painter.galley(bg_pos + label_pad, galley, program_color);
        }

        // ── Transition progress bar on Program pane ──
        if let Some(ref trans) = state.active_transition {
            let progress = trans.progress();
            let bar_h = 3.0;
            let bar_w = right_zoomed.width() * progress;
            let bar_rect = egui::Rect::from_min_size(
                egui::pos2(right_zoomed.min.x, right_zoomed.max.y - bar_h),
                egui::vec2(bar_w, bar_h),
            );
            let accent = egui::Color32::from_rgb(224, 175, 104);
            label_painter.rect_filled(bar_rect, 0.0, accent);
            ui.ctx().request_repaint();
        }
    } else {
        // Single-pane: render the primary canvas with zoom/pan.
        // Pass panel_rect as the callback rect (so egui doesn't clamp the viewport
        // when the zoomed rect extends beyond the window). The actual viewport is
        // set manually inside PreviewCallback::paint() using the zoomed_rect.
        ui.painter_at(panel_rect).add(Callback::new_paint_callback(
            panel_rect,
            PreviewCallback {
                zoomed_rect: preview_rect,
                target: CanvasTarget::Primary,
            },
        ));
    }

    // ── Grid / Thirds / Safe Zones ──
    // Rendered after the preview texture but before transform handles and overlays.
    // Only shown in single-pane mode where zoom/pan applies.
    {
        let overlay_painter = ui.painter_at(panel_rect);

        // Grid overlay
        if state.settings.general.show_grid {
            draw_grid(
                &overlay_painter,
                preview_rect,
                canvas_size,
                &state.settings.general.grid_preset,
                state.settings.general.snap_grid_size,
                state.settings.general.grid_color,
                state.settings.general.grid_opacity,
            );
        }

        // Rule-of-thirds overlay
        if state.settings.general.show_thirds {
            draw_thirds(
                &overlay_painter,
                preview_rect,
                canvas_size,
                state.settings.general.guide_color,
                state.settings.general.guide_opacity,
            );
        }

        // Safe zones
        if state.settings.general.show_safe_zones {
            draw_safe_zones(
                &overlay_painter,
                preview_rect,
                canvas_size,
                state.settings.general.guide_color,
            );
        }
    }

    // ── Overlays ──
    // Overlays are anchored to the panel_rect (not the zoomed viewport) so
    // they stay visible regardless of zoom/pan.

    let painter = ui.painter_at(panel_rect);
    let pad = 6.0;

    // LIVE badge (top-left of panel) — only when streaming
    if matches!(state.stream_status, StreamStatus::Live { .. }) {
        let badge_text = "LIVE";
        let font = egui::FontId::new(9.0, egui::FontFamily::Proportional);
        let text_galley =
            painter.layout_no_wrap(badge_text.to_string(), font, egui::Color32::WHITE);
        let text_size = text_galley.size();
        let badge_padding = egui::vec2(5.0, 3.0);
        let badge_size = text_size + badge_padding * 2.0;
        let badge_pos = panel_rect.left_top() + egui::vec2(pad, pad);
        let badge_rect = egui::Rect::from_min_size(badge_pos, badge_size);

        // Glow shadow (larger rect behind)
        let glow_expand = 3.0;
        let glow_rect = badge_rect.expand(glow_expand);
        let red_glow = egui::Color32::from_rgba_premultiplied(
            theme.danger.r(),
            theme.danger.g(),
            theme.danger.b(),
            0x40,
        );
        painter.rect_filled(glow_rect, theme.radius_sm, red_glow);

        // Badge background
        painter.rect_filled(badge_rect, theme.radius_sm, theme.danger);

        // Badge text
        let text_pos = badge_rect.min + badge_padding;
        painter.galley(text_pos, text_galley, egui::Color32::WHITE);
    }

    // Resolution overlay (bottom-right of panel) — always visible
    {
        let fps = state.settings.video.fps;
        let zoom_text = if (view.zoom - 1.0).abs() > 0.001 {
            format!(" \u{00b7} {:.0}%", view.zoom * 100.0)
        } else {
            String::new()
        };
        let overlay_text = format!(
            "{}\u{00d7}{} \u{00b7} {}fps{}",
            preview_width, preview_height, fps, zoom_text,
        );
        let font = egui::FontId::new(9.0, egui::FontFamily::Proportional);
        let text_galley = painter.layout_no_wrap(overlay_text, font, theme.text_muted);
        let text_size = text_galley.size();
        let overlay_padding = egui::vec2(4.0, 2.0);
        let overlay_size = text_size + overlay_padding * 2.0;
        let overlay_pos =
            panel_rect.right_bottom() - egui::vec2(overlay_size.x + pad, overlay_size.y + pad);
        let overlay_rect = egui::Rect::from_min_size(overlay_pos, overlay_size);

        // Semi-transparent black background
        let bg = egui::Color32::from_rgba_premultiplied(0, 0, 0, 128);
        painter.rect_filled(overlay_rect, theme.radius_sm, bg);

        // Text
        let text_pos = overlay_rect.min + overlay_padding;
        painter.galley(text_pos, text_galley, theme.text_muted);
    }

    // Transform handles for selected source — uses zoomed viewport
    crate::ui::transform_handles::draw_transform_handles(
        ui,
        state,
        preview_rect,
        panel_rect,
        canvas_size,
        view.zoom,
        view.space_held,
    );

    // Allocate the space so egui knows it's used
    let panel_response = ui.allocate_rect(panel_rect, egui::Sense::hover());

    // ── Drop zone: accept SourceId dragged from library panel ──
    // Show highlight border when hovering with a library drag payload.
    if panel_response
        .dnd_hover_payload::<crate::scene::SourceId>()
        .is_some()
    {
        ui.painter().rect_stroke(
            preview_rect,
            0.0,
            egui::Stroke::new(2.0, state.accent_color),
            egui::StrokeKind::Inside,
        );
    }

    // Accept the drop — add source to the active scene at top of z-order.
    if let Some(payload) = panel_response.dnd_release_payload::<crate::scene::SourceId>() {
        let src_id = *payload;
        let already_in_scene = state
            .active_scene()
            .map(|s| s.sources.iter().any(|ss| ss.source_id == src_id))
            .unwrap_or(false);
        if !already_in_scene {
            let props = state
                .library
                .iter()
                .find(|l| l.id == src_id)
                .map(|l| l.properties.clone());
            if let Some(scene) = state.active_scene_mut() {
                scene.sources.push(crate::scene::SceneSource::new(src_id));
            }
            if let Some(properties) = props
                && let Some(cmd_tx) = &state.command_tx
            {
                match &properties {
                    crate::scene::SourceProperties::Display { screen_index } => {
                        let _ = cmd_tx.try_send(crate::gstreamer::GstCommand::AddCaptureSource {
                            source_id: src_id,
                            config: crate::gstreamer::CaptureSourceConfig::Screen {
                                screen_index: *screen_index,
                                exclude_self: state.settings.general.exclude_self_from_capture,
                            },
                        });
                        state.capture_active = true;
                    }
                    crate::scene::SourceProperties::Window { mode, .. } => {
                        let _ = cmd_tx.try_send(crate::gstreamer::GstCommand::AddCaptureSource {
                            source_id: src_id,
                            config: crate::gstreamer::CaptureSourceConfig::Window {
                                mode: mode.clone(),
                            },
                        });
                        state.capture_active = true;
                    }
                    crate::scene::SourceProperties::Camera { device_index, .. } => {
                        let _ = cmd_tx.try_send(crate::gstreamer::GstCommand::AddCaptureSource {
                            source_id: src_id,
                            config: crate::gstreamer::CaptureSourceConfig::Camera {
                                device_index: *device_index,
                            },
                        });
                        state.capture_active = true;
                    }
                    _ => {}
                }
            }
            state.select_source(src_id);
            state.selected_library_source_id = None;
        }
    }
}
