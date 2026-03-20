use std::sync::Arc;

use egui_wgpu::wgpu;
use egui_wgpu::{Callback, CallbackResources, CallbackTrait};

use crate::state::AppState;
use crate::ui::layout::PanelId;

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
