use egui::{Align, Layout, StrokeKind, Ui};

use crate::state::AppState;
use crate::ui::theme::{Theme, active_theme, color_to_hex, parse_hex_color};

use super::{labeled_row_unimplemented, section_header};

pub(super) fn draw(ui: &mut Ui, state: &mut AppState) -> bool {
    let mut changed = false;

    section_header(ui, "THEME");

    ui.horizontal(|ui| {
        labeled_row_unimplemented(ui, "Theme");
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let combo = egui::ComboBox::from_id_salt("theme_combo")
                .selected_text(
                    crate::ui::theme::ThemeId::all()
                        .iter()
                        .find(|&&id| id == state.settings.appearance.theme)
                        .map(|id| id.label())
                        .unwrap_or("Unknown"),
                )
                .show_ui(ui, |ui| {
                    let mut c = false;
                    for &id in crate::ui::theme::ThemeId::all() {
                        c |= ui
                            .selectable_value(
                                &mut state.settings.appearance.theme,
                                id,
                                id.label(),
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
    let theme = active_theme(ui.ctx());
    ui.add_space(16.0);
    ui.label(
        egui::RichText::new("Accent Color")
            .color(theme.text_primary)
            .size(13.0),
    );
    ui.add_space(8.0);

    ui.horizontal(|ui| {
        // Resolve the effective accent: override hex if set, else theme default.
        let effective_accent = state
            .settings
            .appearance
            .accent_color
            .as_deref()
            .map(parse_hex_color)
            .unwrap_or_else(|| Theme::builtin(state.settings.appearance.theme).accent);

        // Color swatch preview
        let (swatch_rect, _) =
            ui.allocate_exact_size(egui::Vec2::new(24.0, 24.0), egui::Sense::hover());
        ui.painter()
            .rect_filled(swatch_rect, theme.radius_sm, effective_accent);
        ui.painter().rect_stroke(
            swatch_rect,
            theme.radius_sm,
            egui::Stroke::new(1.0, theme.border),
            StrokeKind::Outside,
        );

        ui.add_space(8.0);

        // Hex input — show current effective accent as the editable value.
        let mut hex = state
            .settings
            .appearance
            .accent_color
            .clone()
            .unwrap_or_else(|| color_to_hex(effective_accent));
        let response = ui.add(
            egui::TextEdit::singleline(&mut hex)
                .desired_width(80.0)
                .font(egui::TextStyle::Monospace),
        );
        if response.changed() {
            state.settings.appearance.accent_color = Some(hex);
            state.settings_dirty = true;
        }

        ui.add_space(8.0);

        // Reset clears the override so the theme default takes effect.
        if ui.button("Reset").clicked() {
            state.settings.appearance.accent_color = None;
            state.settings_dirty = true;
        }
    });

    changed
}
