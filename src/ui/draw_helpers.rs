//! Shared drawing utilities for UI panels.
//!
//! Contains small helper functions used across multiple panels to avoid
//! duplication and keep panel code focused on layout and logic.

use crate::scene::SourceType;
use crate::ui::theme::{
    BG_ELEVATED, BORDER, DEFAULT_ACCENT, RADIUS_SM, TEXT_MUTED, TEXT_PRIMARY, accent_dim,
};
use egui::{Color32, CornerRadius, Painter, Rect, Sense, Stroke, vec2};

/// Return a Phosphor icon for a given source type.
pub fn source_icon(source_type: &SourceType) -> &'static str {
    match source_type {
        SourceType::Display => egui_phosphor::regular::MONITOR,
        SourceType::Camera => egui_phosphor::regular::VIDEO_CAMERA,
        SourceType::Image => egui_phosphor::regular::IMAGE,
        SourceType::Browser => egui_phosphor::regular::BROWSER,
        SourceType::Audio => egui_phosphor::regular::SPEAKER_HIGH,
        SourceType::Window => egui_phosphor::regular::APP_WINDOW,
    }
}

/// Draw a segmented button group: connected icon toggles with a shared background.
/// Returns `Some(index)` if a button was clicked.
pub fn draw_segmented_buttons(
    ui: &mut egui::Ui,
    id_salt: &str,
    buttons: &[(&str, &str, bool)], // (icon, tooltip, is_active)
) -> Option<usize> {
    let mut clicked = None;
    let btn_size = 20.0_f32;
    let total_width = btn_size * buttons.len() as f32;
    let height = 18.0_f32;

    // Allocate the full segment rect.
    let (seg_rect, _) = ui.allocate_exact_size(vec2(total_width, height), Sense::hover());

    // Draw shared background.
    let painter = ui.painter_at(seg_rect);
    painter.rect_filled(seg_rect, CornerRadius::same(RADIUS_SM as u8), BG_ELEVATED);

    // Draw each button.
    for (i, (icon, tooltip, active)) in buttons.iter().enumerate() {
        let btn_rect = egui::Rect::from_min_size(
            egui::pos2(seg_rect.left() + i as f32 * btn_size, seg_rect.top()),
            vec2(btn_size, height),
        );

        // Invisible click target.
        let btn_id = ui.make_persistent_id((id_salt, i));
        let response = ui.interact(btn_rect, btn_id, Sense::click());

        if response.clicked() {
            clicked = Some(i);
        }

        // Active highlight.
        if *active {
            painter.rect_filled(
                btn_rect,
                CornerRadius::same(RADIUS_SM as u8),
                accent_dim(DEFAULT_ACCENT),
            );
        } else if response.hovered() {
            painter.rect_filled(btn_rect, CornerRadius::same(RADIUS_SM as u8), BORDER);
        }

        // Icon.
        let icon_color = if *active { TEXT_PRIMARY } else { TEXT_MUTED };
        painter.text(
            btn_rect.center(),
            egui::Align2::CENTER_CENTER,
            *icon,
            egui::FontId::proportional(11.0),
            icon_color,
        );

        if response.hovered() {
            response.on_hover_text(*tooltip);
        }
    }

    clicked
}

/// Apply an opacity multiplier to a Color32.
pub fn with_opacity(color: Color32, opacity: f32) -> Color32 {
    Color32::from_rgba_premultiplied(
        (color.r() as f32 * opacity) as u8,
        (color.g() as f32 * opacity) as u8,
        (color.b() as f32 * opacity) as u8,
        (color.a() as f32 * opacity) as u8,
    )
}

/// Draw a selection highlight: a filled rect with `RADIUS_SM` corner radius.
pub fn draw_selection_highlight(painter: &Painter, rect: Rect, color: Color32) {
    painter.rect_filled(rect, CornerRadius::same(RADIUS_SM as u8), color);
}

/// Draw a styled rect: a filled rect with an optional stroke border.
#[allow(dead_code)]
pub fn draw_styled_rect(painter: &Painter, rect: Rect, fill: Color32, border: Option<Stroke>) {
    painter.rect_filled(rect, CornerRadius::same(RADIUS_SM as u8), fill);
    if let Some(stroke) = border {
        painter.rect_stroke(
            rect,
            CornerRadius::same(RADIUS_SM as u8),
            stroke,
            egui::StrokeKind::Inside,
        );
    }
}
