//! Toggle widgets: pill-shaped toggle row and iOS-style toggle switch.
#![allow(dead_code)]

use egui::{Color32, CornerRadius, Sense, Ui, Vec2};

use crate::ui::theme::active_theme;

// ── Toggle row ───────────────────────────────────────────────────────────────

/// Horizontal row of mutually exclusive pill-shaped option buttons.
///
/// Each option is `(value, label)`. Selected option uses `theme.accent_dim`
/// background with `theme.accent` text; unselected uses `theme.bg_elevated`
/// with `theme.text_secondary`.
///
/// Returns `true` if the selection changed.
pub fn toggle_row<T: PartialEq + Copy>(
    ui: &mut Ui,
    selected: &mut T,
    options: &[(T, &str)],
) -> bool {
    let theme = active_theme(ui.ctx());
    let mut changed = false;

    ui.horizontal(|ui| {
        ui.style_mut().spacing.item_spacing.x = 2.0;

        for (value, label) in options {
            let is_selected = selected == value;

            let padding = egui::vec2(10.0, 4.0);
            let galley = ui.painter().layout_no_wrap(
                (*label).to_owned(),
                egui::FontId::proportional(12.0),
                Color32::WHITE,
            );
            let text_size = galley.size();
            let desired = text_size + padding * 2.0;

            let (rect, response) = ui.allocate_exact_size(desired, Sense::click());

            if response.clicked() && !is_selected {
                *selected = *value;
                changed = true;
            }

            if ui.is_rect_visible(rect) {
                let radius = CornerRadius::same(theme.radius_lg as u8);

                let fill = if is_selected {
                    theme.accent_dim
                } else {
                    theme.bg_elevated
                };
                let text_color = if is_selected {
                    theme.accent
                } else {
                    theme.text_secondary
                };

                ui.painter().rect_filled(rect, radius, fill);

                let text_pos = rect.center() - text_size / 2.0;
                ui.painter().galley(
                    text_pos,
                    ui.painter().layout_no_wrap(
                        (*label).to_owned(),
                        egui::FontId::proportional(12.0),
                        text_color,
                    ),
                    text_color,
                );
            }
        }
    });

    changed
}

// ── Toggle switch ────────────────────────────────────────────────────────────

/// iOS-style animated toggle switch. Returns `true` if the value changed.
///
/// This is a standalone function version of the toggle switch previously in
/// `settings/mod.rs`.
pub fn toggle_switch(ui: &mut Ui, on: &mut bool) -> bool {
    let theme = active_theme(ui.ctx());

    let desired_size = Vec2::new(36.0, 20.0);
    let (rect, mut response) = ui.allocate_exact_size(desired_size, Sense::click());

    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }

    if ui.is_rect_visible(rect) {
        let anim_id = response.id.with("toggle_anim");
        let t = ui.ctx().animate_bool_with_time(anim_id, *on, 0.15);

        let bg_color = if *on { theme.accent } else { theme.border_subtle };

        let knob_radius = 7.0;
        let knob_x = egui::lerp(
            rect.left() + knob_radius + 3.0..=rect.right() - knob_radius - 3.0,
            t,
        );
        let knob_center = egui::pos2(knob_x, rect.center().y);

        // Track background
        ui.painter()
            .rect_filled(rect, CornerRadius::same(10), bg_color);

        // Knob: white when on, muted when off
        let knob_color = if *on { Color32::WHITE } else { theme.text_muted };
        ui.painter()
            .circle_filled(knob_center, knob_radius, knob_color);
    }

    response.changed()
}
