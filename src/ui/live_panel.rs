//! Live panel — read-only monitor showing the program (live) output.
//!
//! This is a simplified version of the preview panel: same GPU-rendered canvas
//! texture, but no zoom/pan, no transform handles, no grid overlays. It shows
//! the composited program scene output with status overlays.

use std::sync::Arc;

use egui_wgpu::wgpu;
use egui_wgpu::{Callback, CallbackResources, CallbackTrait};

use crate::state::{AppState, RecordingStatus};
use crate::ui::layout::PanelId;
use crate::ui::theme::active_theme;

// ── GPU callback ─────────────────────────────────────────────────────────────

/// GPU resources for the live panel callback, stored in `egui_renderer.callback_resources`.
///
/// Pipeline and bind group are `Arc`-cloned from the compositor. For now these
/// share the same canvas as the preview panel; Task 5 will differentiate them
/// so the live panel samples from the program scene's canvas.
pub struct LiveResources {
    pub pipeline: Arc<wgpu::RenderPipeline>,
    pub bind_group: Arc<wgpu::BindGroup>,
}

/// Lightweight struct emitted per live panel per frame.
/// Carries the letterboxed viewport rect for the wgpu viewport.
struct LiveCallback {
    /// The letterboxed rect in logical points.
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

        // Set viewport to the letterboxed rect, converting from logical points
        // to physical pixels.
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

// ── Viewport helper ──────────────────────────────────────────────────────────

/// Compute the largest rect matching the canvas aspect ratio that fits
/// inside `panel`, centered, with black bars for the remainder.
fn letterboxed_rect(panel: egui::Rect, canvas_w: u32, canvas_h: u32) -> egui::Rect {
    let panel_w = panel.width();
    let panel_h = panel.height();
    let canvas_aspect = canvas_w as f32 / canvas_h as f32;
    let panel_aspect = panel_w / panel_h;

    let (w, h) = if panel_aspect > canvas_aspect {
        // Panel is wider — pillarbox
        (panel_h * canvas_aspect, panel_h)
    } else {
        // Panel is taller — letterbox
        (panel_w, panel_w / canvas_aspect)
    };

    let center = panel.center();
    egui::Rect::from_center_size(center, egui::vec2(w, h))
}

// ── Public draw entry point ──────────────────────────────────────────────────

pub fn draw(ui: &mut egui::Ui, state: &mut AppState, _panel_id: PanelId) {
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
    let (canvas_w, canvas_h) = {
        let base = &state.settings.video.base_resolution;
        let parts: Vec<&str> = base.split('x').collect();
        let w = parts
            .first()
            .and_then(|s| s.parse().ok())
            .unwrap_or(1920u32);
        let h = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(1080u32);
        (w, h)
    };

    let live_rect = letterboxed_rect(panel_rect, canvas_w, canvas_h);

    // ── GPU paint callback ──

    ui.painter_at(panel_rect).add(Callback::new_paint_callback(
        panel_rect,
        LiveCallback {
            letterboxed_rect: live_rect,
        },
    ));

    // ── Overlays ──
    // Anchored to panel_rect so they stay visible regardless of aspect ratio.

    let painter = ui.painter_at(panel_rect);
    let pad = 6.0;

    // LIVE indicator (top-left) — shown when streaming, recording, or vcam active
    let is_outputting = state.stream_status.is_live()
        || matches!(state.recording_status, RecordingStatus::Recording { .. })
        || state.virtual_camera_active;

    if is_outputting {
        let badge_text = "LIVE";
        let font = egui::FontId::new(9.0, egui::FontFamily::Proportional);
        let text_galley =
            painter.layout_no_wrap(badge_text.to_string(), font, egui::Color32::WHITE);
        let text_size = text_galley.size();

        // Red dot before text
        let dot_radius = 3.0;
        let dot_gap = 4.0;
        let badge_padding = egui::vec2(5.0, 3.0);
        let badge_size = egui::vec2(
            badge_padding.x + dot_radius * 2.0 + dot_gap + text_size.x + badge_padding.x,
            text_size.y + badge_padding.y * 2.0,
        );
        let badge_pos = panel_rect.left_top() + egui::vec2(pad, pad);
        let badge_rect = egui::Rect::from_min_size(badge_pos, badge_size);

        // Glow shadow
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

        // Red dot
        let dot_center = egui::pos2(
            badge_rect.min.x + badge_padding.x + dot_radius,
            badge_rect.center().y,
        );
        painter.circle_filled(dot_center, dot_radius, egui::Color32::WHITE);

        // Badge text
        let text_pos = egui::pos2(
            dot_center.x + dot_radius + dot_gap,
            badge_rect.min.y + badge_padding.y,
        );
        painter.galley(text_pos, text_galley, egui::Color32::WHITE);
    }

    // Resolution label (bottom-right)
    {
        let fps = state.settings.video.fps;
        let overlay_text = format!("{}\u{00d7}{} \u{00b7} {}fps", canvas_w, canvas_h, fps);
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

    // Transition progress bar (thin amber bar at bottom of live rect)
    if let Some(ref transition) = state.active_transition {
        let progress = transition.progress();
        if progress < 1.0 {
            let bar_height = 3.0;
            let bar_rect = egui::Rect::from_min_size(
                egui::pos2(live_rect.min.x, live_rect.max.y - bar_height),
                egui::vec2(live_rect.width() * progress, bar_height),
            );
            // Amber color
            let amber = egui::Color32::from_rgb(0xFF, 0xBF, 0x00);
            painter.rect_filled(bar_rect, 0.0, amber);

            // Request continuous repaints during transition for animation
            ui.ctx().request_repaint();
        }
    }

    // Allocate the space so egui knows it's used
    ui.allocate_rect(panel_rect, egui::Sense::hover());
}
