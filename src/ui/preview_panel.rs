use std::sync::Arc;

use egui_wgpu::wgpu;
use egui_wgpu::{Callback, CallbackResources, CallbackTrait};

use crate::state::{AppState, StreamStatus};
use crate::ui::layout::PanelId;
use crate::ui::theme::{RADIUS_SM, RED_GLOW, RED_LIVE, TEXT_MUTED};

// ── Zoom levels ──────────────────────────────────────────────────────────────

/// Discrete zoom levels used for scroll-wheel stepping.
const ZOOM_LEVELS: &[f32] = &[0.1, 0.25, 0.33, 0.5, 0.67, 0.75, 1.0, 1.5, 2.0, 3.0, 4.0, 6.0, 8.0];
const ZOOM_MIN: f32 = 0.1;
const ZOOM_MAX: f32 = 8.0;

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

        // The viewport is set by egui to the (possibly zoomed) preview rect.
        // The fullscreen quad shader fills this viewport exactly.
        // The scissor/clip rect clips to the panel bounds so the quad doesn't
        // draw outside the panel when zoomed in.
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

/// Clamp pan so at least 10% of canvas remains visible in the panel.
fn clamp_pan(pan: egui::Vec2, panel: egui::Rect, canvas_w: u32, canvas_h: u32, zoom: f32) -> egui::Vec2 {
    let base = letterboxed_rect(panel, canvas_w, canvas_h);
    let base_size = base.size();
    let zoomed_size = base_size * zoom;
    // Allow panning until only 10% of the canvas is visible
    let max_offset_x = (zoomed_size.x * 0.9 + panel.width() * 0.5) / (zoomed_size.x / canvas_w as f32);
    let max_offset_y = (zoomed_size.y * 0.9 + panel.height() * 0.5) / (zoomed_size.y / canvas_h as f32);
    egui::Vec2::new(
        pan.x.clamp(-max_offset_x, max_offset_x),
        pan.y.clamp(-max_offset_y, max_offset_y),
    )
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

    // ── Scroll-wheel zoom (cursor-centered) ──

    if cursor_in_panel {
        let scroll_delta = ui.input(|i| i.raw_scroll_delta.y);
        if scroll_delta.abs() > 0.5 {
            let old_zoom = view.zoom;
            let new_zoom = if scroll_delta > 0.0 {
                zoom_level_up(old_zoom)
            } else {
                zoom_level_down(old_zoom)
            };

            // Cursor-centered zoom: keep the canvas point under the cursor fixed
            if let Some(cursor_pos) = ui.input(|i| i.pointer.hover_pos()) {
                let old_viewport =
                    zoomed_viewport(panel_rect, preview_width, preview_height, old_zoom, view.pan_offset);
                let canvas_pos =
                    screen_to_canvas(cursor_pos, old_viewport, preview_width, preview_height);

                view.zoom = new_zoom;

                // Recompute viewport with new zoom, find where canvas_pos ended up
                let new_viewport =
                    zoomed_viewport(panel_rect, preview_width, preview_height, new_zoom, view.pan_offset);
                let new_screen = egui::Pos2::new(
                    new_viewport.min.x + canvas_pos.x * new_viewport.width() / preview_width as f32,
                    new_viewport.min.y
                        + canvas_pos.y * new_viewport.height() / preview_height as f32,
                );

                // Adjust pan to compensate
                let diff = cursor_pos - new_screen;
                let pixels_per_canvas =
                    (base_rect.width() * new_zoom) / preview_width as f32;
                view.pan_offset.x += diff.x / pixels_per_canvas;
                view.pan_offset.y += diff.y / pixels_per_canvas;
            } else {
                view.zoom = new_zoom;
            }

            view.pan_offset =
                clamp_pan(view.pan_offset, panel_rect, preview_width, preview_height, view.zoom);
        }
    }

    // ── Trackpad pinch zoom ──

    if cursor_in_panel {
        if let Some(touch) = ui.input(|i| i.multi_touch()) {
            if (touch.zoom_delta - 1.0).abs() > 0.001 {
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
        }
    }

    // ── Pan (middle-click drag or spacebar + left-click drag) ──

    let middle_down = ui.input(|i| i.pointer.middle_down());
    let space_left_down = view.space_held && ui.input(|i| i.pointer.primary_down());
    let is_panning = cursor_in_panel && (middle_down || space_left_down);

    if is_panning {
        let delta = ui.input(|i| i.pointer.delta());
        if delta.length_sq() > 0.0 {
            // Convert screen delta to canvas delta
            let pixels_per_canvas =
                (base_rect.width() * view.zoom) / preview_width as f32;
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
    ui.ctx()
        .data_mut(|d| d.insert_temp(view_id, view.clone()));

    // ── GPU paint callback ──
    // Render into the zoomed rect, but clip to panel_rect so the quad doesn't
    // draw outside the panel when zoomed in.
    // Use `painter_at(panel_rect)` so the clip_rect is the panel boundary.

    ui.painter_at(panel_rect)
        .add(Callback::new_paint_callback(preview_rect, PreviewCallback));

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
    let canvas_size = egui::Vec2::new(preview_width as f32, preview_height as f32);
    crate::ui::transform_handles::draw_transform_handles(
        ui,
        state,
        preview_rect,
        panel_rect,
        canvas_size,
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
