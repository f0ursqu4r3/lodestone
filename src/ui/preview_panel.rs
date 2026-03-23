use std::sync::Arc;

use egui_wgpu::wgpu;
use egui_wgpu::{Callback, CallbackResources, CallbackTrait};

use crate::state::{AppState, StreamStatus};
use crate::ui::layout::PanelId;
use crate::ui::theme::{BG_BASE, RED_GLOW, RED_LIVE, TEXT_MUTED};

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

        // The viewport is already set by egui to the letterboxed preview rect
        // (passed to new_paint_callback). The fullscreen quad shader fills this
        // viewport exactly. The scissor rect clips further when floating panels
        // or other UI elements overlap the preview area.
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

    // Use fixed preview dimensions (1920x1080); per-source dimensions tracked in Task 6.
    let preview_width: u32 = 1920;
    let preview_height: u32 = 1080;

    // Fill entire panel with theme base color (letterbox bars)
    ui.painter().rect_filled(panel_rect, 0.0, BG_BASE);

    // Compute letterboxed rect and emit the paint callback
    let preview_rect = letterboxed_rect(panel_rect, preview_width, preview_height);

    ui.painter()
        .add(Callback::new_paint_callback(preview_rect, PreviewCallback));

    // ── Overlays ──

    let painter = ui.painter();
    let pad = 6.0;

    // LIVE badge (top-left of viewport) — only when streaming
    if matches!(state.stream_status, StreamStatus::Live { .. }) {
        let badge_text = "LIVE";
        let font = egui::FontId::new(9.0, egui::FontFamily::Proportional);
        let text_galley =
            painter.layout_no_wrap(badge_text.to_string(), font, egui::Color32::WHITE);
        let text_size = text_galley.size();
        let badge_padding = egui::vec2(5.0, 3.0);
        let badge_size = text_size + badge_padding * 2.0;
        let badge_pos = preview_rect.left_top() + egui::vec2(pad, pad);
        let badge_rect = egui::Rect::from_min_size(badge_pos, badge_size);

        // Glow shadow (larger rect behind)
        let glow_expand = 3.0;
        let glow_rect = badge_rect.expand(glow_expand);
        painter.rect_filled(glow_rect, 6.0, RED_GLOW);

        // Badge background
        painter.rect_filled(badge_rect, 3.0, RED_LIVE);

        // Badge text
        let text_pos = badge_rect.min + badge_padding;
        painter.galley(text_pos, text_galley, egui::Color32::WHITE);
    }

    // Resolution overlay (bottom-right of viewport) — always visible
    {
        let video = &state.settings.video;
        let resolution = &video.output_resolution;
        let fps = video.fps;
        let overlay_text = format!(
            "{}\u{00d7}{} \u{00b7} {}fps",
            resolution.split('x').next().unwrap_or("1920"),
            resolution.split('x').nth(1).unwrap_or("1080"),
            fps,
        );
        let font = egui::FontId::new(9.0, egui::FontFamily::Proportional);
        let text_galley = painter.layout_no_wrap(overlay_text, font, TEXT_MUTED);
        let text_size = text_galley.size();
        let overlay_padding = egui::vec2(4.0, 2.0);
        let overlay_size = text_size + overlay_padding * 2.0;
        let overlay_pos =
            preview_rect.right_bottom() - egui::vec2(overlay_size.x + pad, overlay_size.y + pad);
        let overlay_rect = egui::Rect::from_min_size(overlay_pos, overlay_size);

        // Semi-transparent black background
        let bg = egui::Color32::from_rgba_premultiplied(0, 0, 0, 128);
        painter.rect_filled(overlay_rect, 2.0, bg);

        // Text
        let text_pos = overlay_rect.min + overlay_padding;
        painter.galley(text_pos, text_galley, TEXT_MUTED);
    }

    // Allocate the space so egui knows it's used
    ui.allocate_rect(panel_rect, egui::Sense::hover());
}
