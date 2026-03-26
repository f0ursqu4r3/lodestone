use egui::{StrokeKind, Ui};

use crate::state::AppState;
use crate::ui::theme::{Theme, active_theme, color_to_hex, parse_hex_color};
use crate::ui::widgets::layout;

pub(super) fn draw(ui: &mut Ui, state: &mut AppState) -> bool {
    let mut changed = false;

    // ── Theme Picker ──────────────────────────────────────────────────────────

    let current_theme = active_theme(ui.ctx());

    layout::section(ui, "Theme", |ui| {
        let all = crate::ui::theme::ThemeId::all();
        egui::Grid::new("theme_picker")
            .num_columns(2)
            .spacing([8.0, 8.0])
            .show(ui, |ui| {
                for (i, &id) in all.iter().enumerate() {
                    let t = Theme::builtin(id);
                    let is_selected = state.settings.appearance.theme == id;

                    let desired_size = egui::vec2(ui.available_width().min(200.0), 52.0);
                    let (rect, response) =
                        ui.allocate_exact_size(desired_size, egui::Sense::click());

                    if response.clicked() {
                        state.settings.appearance.theme = id;
                        changed = true;
                    }

                    let painter = ui.painter();

                    // Card background
                    painter.rect_filled(rect, current_theme.radius_md, current_theme.bg_elevated);

                    // Border — accent when selected, normal border otherwise
                    let stroke = if is_selected {
                        egui::Stroke::new(2.0, current_theme.accent)
                    } else {
                        egui::Stroke::new(1.0, current_theme.border)
                    };
                    painter.rect_stroke(rect, current_theme.radius_md, stroke, StrokeKind::Outside);

                    // Theme name
                    painter.text(
                        rect.left_top() + egui::vec2(8.0, 6.0),
                        egui::Align2::LEFT_TOP,
                        id.label(),
                        egui::FontId::proportional(11.0),
                        current_theme.text_primary,
                    );

                    // 5 color swatches: bg_base, bg_surface, bg_elevated, text_primary, accent
                    let swatch_y = rect.top() + 26.0;
                    let swatch_size = 16.0;
                    let swatches = [
                        t.bg_base,
                        t.bg_surface,
                        t.bg_elevated,
                        t.text_primary,
                        t.accent,
                    ];
                    for (j, &color) in swatches.iter().enumerate() {
                        let swatch_rect = egui::Rect::from_min_size(
                            egui::pos2(
                                rect.left() + 8.0 + j as f32 * (swatch_size + 3.0),
                                swatch_y,
                            ),
                            egui::vec2(swatch_size, swatch_size),
                        );
                        painter.rect_filled(swatch_rect, 2.0, color);
                        painter.rect_stroke(
                            swatch_rect,
                            2.0,
                            egui::Stroke::new(0.5, current_theme.border),
                            StrokeKind::Outside,
                        );
                    }

                    if (i + 1) % 2 == 0 {
                        ui.end_row();
                    }
                }
            });
    });

    // ── Accent Color ─────────────────────────────────────────────────────────

    layout::section(ui, "Accent Color", |ui| {
        let builtin = Theme::builtin(state.settings.appearance.theme);
        let effective = state
            .settings
            .appearance
            .accent_color
            .as_deref()
            .map(parse_hex_color)
            .unwrap_or(builtin.accent);

        ui.horizontal(|ui| {
            // Color swatch preview
            let (swatch_rect, _) =
                ui.allocate_exact_size(egui::vec2(24.0, 24.0), egui::Sense::hover());
            ui.painter()
                .rect_filled(swatch_rect, current_theme.radius_sm, effective);
            ui.painter().rect_stroke(
                swatch_rect,
                current_theme.radius_sm,
                egui::Stroke::new(1.0, current_theme.border),
                StrokeKind::Outside,
            );

            // Hex input
            let mut hex_str = state
                .settings
                .appearance
                .accent_color
                .clone()
                .unwrap_or_else(|| color_to_hex(builtin.accent));
            let response = ui.add(
                egui::TextEdit::singleline(&mut hex_str)
                    .desired_width(80.0)
                    .font(egui::TextStyle::Monospace),
            );
            if response.changed() {
                state.settings.appearance.accent_color = Some(hex_str);
                changed = true;
            }

            // Reset button — only shown when an override is active
            if state.settings.appearance.accent_color.is_some()
                && ui.small_button("Reset").clicked()
            {
                state.settings.appearance.accent_color = None;
                changed = true;
            }
        });
    });

    // ── Font Size ────────────────────────────────────────────────────────────

    layout::section(ui, "Font Size", |ui| {
        let mut size = state.settings.appearance.font_size;
        if ui
            .add(
                egui::DragValue::new(&mut size)
                    .range(8.0..=24.0)
                    .suffix(" px"),
            )
            .changed()
        {
            state.settings.appearance.font_size = size;
            changed = true;
        }
    });

    // ── Font Family ──────────────────────────────────────────────────────────

    layout::section(ui, "Font Family", |ui| {
        egui::ComboBox::from_id_salt("font_family")
            .selected_text(&state.settings.appearance.font_family)
            .show_ui(ui, |ui| {
                for font in &state.system_fonts.clone() {
                    if ui
                        .selectable_value(
                            &mut state.settings.appearance.font_family,
                            font.clone(),
                            font,
                        )
                        .changed()
                    {
                        changed = true;
                    }
                }
            });
    });

    changed
}
