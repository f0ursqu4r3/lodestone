use egui::{Align, Layout, StrokeKind, Ui};

use crate::state::AppState;
use crate::ui::theme::{
    BORDER, DEFAULT_ACCENT, RADIUS_SM, TEXT_PRIMARY, color_to_hex, parse_hex_color,
};

use super::{labeled_row_unimplemented, section_header};

pub(super) fn draw(ui: &mut Ui, state: &mut AppState) -> bool {
    let mut changed = false;

    section_header(ui, "THEME");

    ui.horizontal(|ui| {
        labeled_row_unimplemented(ui, "Theme");
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let combo = egui::ComboBox::from_id_salt("theme_combo")
                .selected_text(&state.settings.appearance.theme)
                .show_ui(ui, |ui| {
                    let mut c = false;
                    for t in &["dark", "light"] {
                        c |= ui
                            .selectable_value(
                                &mut state.settings.appearance.theme,
                                t.to_string(),
                                *t,
                            )
                            .changed();
                    }
                    c
                });
            if let Some(inner) = combo.inner {
                changed |= inner;
            }
        });
    });

    section_header(ui, "FONT");

    ui.horizontal(|ui| {
        labeled_row_unimplemented(ui, "Font Size");
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            changed |= ui
                .add(
                    egui::DragValue::new(&mut state.settings.appearance.font_size)
                        .range(8.0..=24.0)
                        .speed(0.25)
                        .suffix(" px"),
                )
                .changed();
        });
    });

    // ── Accent Color ──
    ui.add_space(16.0);
    ui.label(
        egui::RichText::new("Accent Color")
            .color(TEXT_PRIMARY)
            .size(13.0),
    );
    ui.add_space(8.0);

    ui.horizontal(|ui| {
        // Color swatch preview
        let accent = parse_hex_color(&state.settings.appearance.accent_color);
        let (swatch_rect, _) =
            ui.allocate_exact_size(egui::Vec2::new(24.0, 24.0), egui::Sense::hover());
        ui.painter().rect_filled(swatch_rect, RADIUS_SM, accent);
        ui.painter().rect_stroke(
            swatch_rect,
            RADIUS_SM,
            egui::Stroke::new(1.0, BORDER),
            StrokeKind::Outside,
        );

        ui.add_space(8.0);

        // Hex input
        let mut hex = state.settings.appearance.accent_color.clone();
        let response = ui.add(
            egui::TextEdit::singleline(&mut hex)
                .desired_width(80.0)
                .font(egui::TextStyle::Monospace),
        );
        if response.changed() {
            state.settings.appearance.accent_color = hex;
            state.settings_dirty = true;
        }

        ui.add_space(8.0);

        // Reset to default
        if ui.button("Reset").clicked() {
            state.settings.appearance.accent_color = color_to_hex(DEFAULT_ACCENT);
            state.settings_dirty = true;
        }
    });

    changed
}
