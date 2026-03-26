use std::sync::Arc;

use egui_wgpu::wgpu;
use egui_wgpu::{Callback, CallbackResources, CallbackTrait};

use crate::scene::GuideAxis;
use crate::state::{AppState, StreamStatus};
use crate::ui::layout::PanelId;
use crate::ui::theme::{RADIUS_SM, RED_GLOW, RED_LIVE, TEXT_MUTED};

// ── Zoom levels ──────────────────────────────────────────────────────────────

/// Discrete zoom levels used for scroll-wheel stepping.
const ZOOM_LEVELS: &[f32] = &[
    0.1, 0.25, 0.33, 0.5, 0.67, 0.75, 1.0, 1.5, 2.0, 3.0, 4.0,
];
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
}

/// Lightweight struct emitted per preview panel per frame.
/// Carries the zoomed viewport rect so we can set the wgpu viewport manually
/// instead of relying on egui's viewport (which gets clamped on off-screen rects).
struct PreviewCallback {
    /// The zoomed preview rect in logical points (may extend beyond the window).
    zoomed_rect: egui::Rect,
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
        render_pass.set_bind_group(0, &*resources.bind_group, &[]);
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

// ── Grid / Guides / Thirds / Safe-zones ─────────────────────────────────────

const RULER_WIDTH: f32 = 12.0;

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

/// Draw custom per-scene guide lines as dashed colored lines.
fn draw_custom_guides(
    painter: &egui::Painter,
    viewport: egui::Rect,
    canvas_size: egui::Vec2,
    guides: &[crate::scene::Guide],
    guide_color: [u8; 3],
    guide_opacity: f32,
) {
    let color = egui::Color32::from_rgba_unmultiplied(
        guide_color[0],
        guide_color[1],
        guide_color[2],
        (255.0 * guide_opacity) as u8,
    );
    let stroke = egui::Stroke::new(1.0, color);
    let dash_len = 6.0;
    let gap_len = 4.0;

    for guide in guides {
        match guide.axis {
            GuideAxis::Horizontal => {
                let sy = canvas_to_screen(egui::pos2(0.0, guide.position), viewport, canvas_size).y;
                draw_dashed_horizontal(
                    painter,
                    sy,
                    viewport.left(),
                    viewport.right(),
                    dash_len,
                    gap_len,
                    stroke,
                );
            }
            GuideAxis::Vertical => {
                let sx = canvas_to_screen(egui::pos2(guide.position, 0.0), viewport, canvas_size).x;
                draw_dashed_vertical(
                    painter,
                    sx,
                    viewport.top(),
                    viewport.bottom(),
                    dash_len,
                    gap_len,
                    stroke,
                );
            }
        }
    }
}

/// Draw a dashed horizontal line.
fn draw_dashed_horizontal(
    painter: &egui::Painter,
    y: f32,
    x_start: f32,
    x_end: f32,
    dash: f32,
    gap: f32,
    stroke: egui::Stroke,
) {
    let mut x = x_start;
    while x < x_end {
        let end = (x + dash).min(x_end);
        painter.line_segment([egui::pos2(x, y), egui::pos2(end, y)], stroke);
        x = end + gap;
    }
}

/// Draw a dashed vertical line.
fn draw_dashed_vertical(
    painter: &egui::Painter,
    x: f32,
    y_start: f32,
    y_end: f32,
    dash: f32,
    gap: f32,
    stroke: egui::Stroke,
) {
    let mut y = y_start;
    while y < y_end {
        let end = (y + dash).min(y_end);
        painter.line_segment([egui::pos2(x, y), egui::pos2(x, end)], stroke);
        y = end + gap;
    }
}

/// Ruler drag state stored in egui temp memory.
#[derive(Clone, Default)]
struct RulerDragState {
    /// Whether we are currently dragging a guide from a ruler.
    dragging: bool,
    /// The axis of the guide being created.
    axis: Option<GuideAxis>,
    /// Current canvas-space position of the dragged guide.
    position: f32,
}

/// Draw rulers along the top and left edges and handle guide creation by
/// dragging from rulers. Also handle right-click deletion of existing guides.
fn draw_rulers_and_guide_interaction(
    ui: &mut egui::Ui,
    state: &mut AppState,
    viewport: egui::Rect,
    panel_rect: egui::Rect,
    canvas_size: egui::Vec2,
) {
    let ruler_id = egui::Id::new("ruler_drag_state");
    let mut ruler_state: RulerDragState = ui
        .ctx()
        .data(|d| d.get_temp::<RulerDragState>(ruler_id))
        .unwrap_or_default();

    // Ruler areas
    let top_ruler = egui::Rect::from_min_max(
        egui::pos2(panel_rect.left() + RULER_WIDTH, panel_rect.top()),
        egui::pos2(panel_rect.right(), panel_rect.top() + RULER_WIDTH),
    );
    let left_ruler = egui::Rect::from_min_max(
        egui::pos2(panel_rect.left(), panel_rect.top() + RULER_WIDTH),
        egui::pos2(panel_rect.left() + RULER_WIDTH, panel_rect.bottom()),
    );

    let painter = ui.painter_at(panel_rect);

    // Draw ruler backgrounds
    let ruler_bg = egui::Color32::from_rgba_unmultiplied(0, 0, 0, 60);
    painter.rect_filled(top_ruler, 0.0, ruler_bg);
    painter.rect_filled(left_ruler, 0.0, ruler_bg);
    // Corner square
    let corner = egui::Rect::from_min_max(
        panel_rect.left_top(),
        egui::pos2(
            panel_rect.left() + RULER_WIDTH,
            panel_rect.top() + RULER_WIDTH,
        ),
    );
    painter.rect_filled(corner, 0.0, ruler_bg);

    // Handle pointer interaction
    let pointer = ui.input(|i| i.pointer.hover_pos());
    let primary_down = ui.input(|i| i.pointer.primary_down());
    let primary_clicked = ui.input(|i| i.pointer.primary_clicked());
    let primary_released = ui.input(|i| i.pointer.primary_released());
    let secondary_clicked = ui.input(|i| i.pointer.secondary_clicked());

    if let Some(mouse_pos) = pointer {
        // Start drag from ruler
        if primary_clicked && !ruler_state.dragging {
            if top_ruler.contains(mouse_pos) {
                ruler_state.dragging = true;
                ruler_state.axis = Some(GuideAxis::Vertical);
                let canvas_x = (mouse_pos.x - viewport.min.x) * canvas_size.x / viewport.width();
                ruler_state.position = canvas_x;
            } else if left_ruler.contains(mouse_pos) {
                ruler_state.dragging = true;
                ruler_state.axis = Some(GuideAxis::Horizontal);
                let canvas_y = (mouse_pos.y - viewport.min.y) * canvas_size.y / viewport.height();
                ruler_state.position = canvas_y;
            }
        }

        // Update position during drag
        if ruler_state.dragging
            && primary_down
            && let Some(axis) = ruler_state.axis
        {
            match axis {
                GuideAxis::Vertical => {
                    ruler_state.position =
                        (mouse_pos.x - viewport.min.x) * canvas_size.x / viewport.width();
                }
                GuideAxis::Horizontal => {
                    ruler_state.position =
                        (mouse_pos.y - viewport.min.y) * canvas_size.y / viewport.height();
                }
            }

            // Draw preview of the guide being created
            let guide_color = state.settings.general.guide_color;
            let guide_opacity = state.settings.general.guide_opacity;
            let color = egui::Color32::from_rgba_unmultiplied(
                guide_color[0],
                guide_color[1],
                guide_color[2],
                (255.0 * guide_opacity) as u8,
            );
            let stroke = egui::Stroke::new(1.0, color);
            match axis {
                GuideAxis::Vertical => {
                    let sx = canvas_to_screen(
                        egui::pos2(ruler_state.position, 0.0),
                        viewport,
                        canvas_size,
                    )
                    .x;
                    painter.line_segment(
                        [
                            egui::pos2(sx, viewport.top()),
                            egui::pos2(sx, viewport.bottom()),
                        ],
                        stroke,
                    );
                }
                GuideAxis::Horizontal => {
                    let sy = canvas_to_screen(
                        egui::pos2(0.0, ruler_state.position),
                        viewport,
                        canvas_size,
                    )
                    .y;
                    painter.line_segment(
                        [
                            egui::pos2(viewport.left(), sy),
                            egui::pos2(viewport.right(), sy),
                        ],
                        stroke,
                    );
                }
            }
            ui.ctx().request_repaint();
        }

        // Release: create the guide
        if ruler_state.dragging && primary_released {
            if let Some(axis) = ruler_state.axis {
                let pos = ruler_state.position;
                // Only create if within canvas bounds
                let in_bounds = match axis {
                    GuideAxis::Vertical => pos >= 0.0 && pos <= canvas_size.x,
                    GuideAxis::Horizontal => pos >= 0.0 && pos <= canvas_size.y,
                };
                if in_bounds {
                    if let Some(scene) = state.active_scene_mut() {
                        scene.guides.push(crate::scene::Guide {
                            axis,
                            position: pos,
                        });
                    }
                    state.mark_dirty();
                }
            }
            ruler_state.dragging = false;
            ruler_state.axis = None;
        }

        // Right-click near a guide: context menu for deletion
        if secondary_clicked && state.settings.general.show_guides {
            let guides: Vec<crate::scene::Guide> = state
                .active_scene()
                .map(|s| s.guides.clone())
                .unwrap_or_default();
            let hit_threshold_px = 4.0;

            let mut hit_guide_idx: Option<usize> = None;
            for (i, guide) in guides.iter().enumerate() {
                match guide.axis {
                    GuideAxis::Horizontal => {
                        let sy = canvas_to_screen(
                            egui::pos2(0.0, guide.position),
                            viewport,
                            canvas_size,
                        )
                        .y;
                        if (mouse_pos.y - sy).abs() <= hit_threshold_px {
                            hit_guide_idx = Some(i);
                            break;
                        }
                    }
                    GuideAxis::Vertical => {
                        let sx = canvas_to_screen(
                            egui::pos2(guide.position, 0.0),
                            viewport,
                            canvas_size,
                        )
                        .x;
                        if (mouse_pos.x - sx).abs() <= hit_threshold_px {
                            hit_guide_idx = Some(i);
                            break;
                        }
                    }
                }
            }

            if hit_guide_idx.is_some() || !guides.is_empty() {
                // Store which guide was hit for the context menu
                ui.ctx().data_mut(|d| {
                    d.insert_temp(
                        egui::Id::new("guide_ctx_hit"),
                        hit_guide_idx.map(|i| i as i64).unwrap_or(-1),
                    );
                    d.insert_temp(egui::Id::new("guide_ctx_pos"), mouse_pos);
                    d.insert_temp(egui::Id::new("guide_ctx_open"), true);
                    d.insert_temp(
                        egui::Id::new("guide_ctx_frame"),
                        ui.ctx().cumulative_frame_nr(),
                    );
                });
            }
        }
    }

    // Draw guide context menu if open
    let ctx_open: bool = ui
        .ctx()
        .data(|d| d.get_temp(egui::Id::new("guide_ctx_open")).unwrap_or(false));
    if ctx_open {
        let ctx_pos: egui::Pos2 = ui.ctx().data(|d| {
            d.get_temp(egui::Id::new("guide_ctx_pos"))
                .unwrap_or(egui::Pos2::ZERO)
        });
        let hit_idx: i64 = ui
            .ctx()
            .data(|d| d.get_temp(egui::Id::new("guide_ctx_hit")).unwrap_or(-1));
        let open_frame: u64 = ui
            .ctx()
            .data(|d| d.get_temp(egui::Id::new("guide_ctx_frame")).unwrap_or(0));

        let mut close = false;
        let area_resp = egui::Area::new(egui::Id::new("guide_ctx_area"))
            .order(egui::Order::Foreground)
            .fixed_pos(ctx_pos)
            .show(ui.ctx(), |ui| {
                egui::Frame::menu(ui.style()).show(ui, |ui| {
                    use crate::ui::theme::{menu_item, styled_menu};
                    styled_menu(ui, |ui| {
                        if hit_idx >= 0 && menu_item(ui, "Delete Guide") {
                            if let Some(scene) = state.active_scene_mut() {
                                let idx = hit_idx as usize;
                                if idx < scene.guides.len() {
                                    scene.guides.remove(idx);
                                }
                            }
                            state.mark_dirty();
                            close = true;
                        }
                        if menu_item(ui, "Clear All Guides") {
                            if let Some(scene) = state.active_scene_mut() {
                                scene.guides.clear();
                            }
                            state.mark_dirty();
                            close = true;
                        }
                    });
                });
            });

        // Close if clicked outside
        let frame_nr = ui.ctx().cumulative_frame_nr();
        if frame_nr > open_frame && !close {
            let any_click =
                ui.input(|i| i.pointer.primary_clicked() || i.pointer.secondary_clicked());
            let in_menu = area_resp
                .response
                .rect
                .contains(pointer.unwrap_or(egui::Pos2::ZERO));
            if any_click && !in_menu {
                close = true;
            }
        }

        if close {
            ui.ctx().data_mut(|d| {
                d.insert_temp(egui::Id::new("guide_ctx_open"), false);
            });
        }
    }

    ui.ctx().data_mut(|d| d.insert_temp(ruler_id, ruler_state));
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
                    let pixels_per_canvas =
                        (base_rect.width() * new_zoom) / preview_width as f32;
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

    // ── GPU paint callback ──
    // Pass panel_rect as the callback rect (so egui doesn't clamp the viewport
    // when the zoomed rect extends beyond the window). The actual viewport is
    // set manually inside PreviewCallback::paint() using the zoomed_rect.
    ui.painter_at(panel_rect).add(Callback::new_paint_callback(
        panel_rect,
        PreviewCallback {
            zoomed_rect: preview_rect,
        },
    ));

    // ── Grid / Guides / Thirds / Safe Zones ──
    // Rendered after the preview texture but before transform handles and overlays.
    let canvas_size = egui::Vec2::new(preview_width as f32, preview_height as f32);
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

        // Custom per-scene guides
        if state.settings.general.show_guides {
            let guides: Vec<crate::scene::Guide> = state
                .active_scene()
                .map(|s| s.guides.clone())
                .unwrap_or_default();
            draw_custom_guides(
                &overlay_painter,
                preview_rect,
                canvas_size,
                &guides,
                state.settings.general.guide_color,
                state.settings.general.guide_opacity,
            );
        }
    }

    // ── Rulers and guide interaction ──
    draw_rulers_and_guide_interaction(ui, state, preview_rect, panel_rect, canvas_size);

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
        painter.rect_filled(glow_rect, RADIUS_SM, RED_GLOW);

        // Badge background
        painter.rect_filled(badge_rect, RADIUS_SM, RED_LIVE);

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
        let text_galley = painter.layout_no_wrap(overlay_text, font, TEXT_MUTED);
        let text_size = text_galley.size();
        let overlay_padding = egui::vec2(4.0, 2.0);
        let overlay_size = text_size + overlay_padding * 2.0;
        let overlay_pos =
            panel_rect.right_bottom() - egui::vec2(overlay_size.x + pad, overlay_size.y + pad);
        let overlay_rect = egui::Rect::from_min_size(overlay_pos, overlay_size);

        // Semi-transparent black background
        let bg = egui::Color32::from_rgba_premultiplied(0, 0, 0, 128);
        painter.rect_filled(overlay_rect, RADIUS_SM, bg);

        // Text
        let text_pos = overlay_rect.min + overlay_padding;
        painter.galley(text_pos, text_galley, TEXT_MUTED);
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
                    crate::scene::SourceProperties::Window { window_id, .. } if *window_id != 0 => {
                        let _ = cmd_tx.try_send(crate::gstreamer::GstCommand::AddCaptureSource {
                            source_id: src_id,
                            config: crate::gstreamer::CaptureSourceConfig::Window {
                                window_id: *window_id,
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
