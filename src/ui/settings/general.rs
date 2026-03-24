use egui::{Align, Layout, Ui};

use crate::settings::GeneralSettings;

use super::{
    draw_toggle, draw_toggle_unimplemented, labeled_row, labeled_row_unimplemented, section_header,
};

pub(super) fn draw(ui: &mut Ui, settings: &mut GeneralSettings) -> bool {
    let mut changed = false;

    section_header(ui, "STARTUP");

    draw_toggle_unimplemented(ui, "Launch on startup", &mut settings.launch_on_startup);
    draw_toggle_unimplemented(
        ui,
        "Check for updates automatically",
        &mut settings.check_for_updates,
    );

    section_header(ui, "BEHAVIOR");

    draw_toggle_unimplemented(
        ui,
        "Confirm close while streaming",
        &mut settings.confirm_close_while_streaming,
    );

    section_header(ui, "CAPTURE");

    changed |= draw_toggle(
        ui,
        "Exclude Lodestone from capture",
        &mut settings.exclude_self_from_capture,
    );

    section_header(ui, "EDITOR");

    changed |= draw_toggle(ui, "Snap to grid", &mut settings.snap_to_grid);

    ui.horizontal(|ui| {
        labeled_row(ui, "Grid size (px)");
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let drag = ui.add(
                egui::DragValue::new(&mut settings.snap_grid_size)
                    .range(1.0..=100.0)
                    .speed(1.0),
            );
            if drag.changed() {
                changed = true;
            }
        });
    });

    section_header(ui, "LANGUAGE");

    ui.horizontal(|ui| {
        labeled_row_unimplemented(ui, "Language");
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let combo = egui::ComboBox::from_id_salt("language_combo")
                .selected_text(&settings.language)
                .show_ui(ui, |ui| {
                    let mut c = false;
                    for lang in &["en-US", "en-GB", "es", "fr", "de", "ja", "ko", "zh-CN"] {
                        c |= ui
                            .selectable_value(&mut settings.language, lang.to_string(), *lang)
                            .changed();
                    }
                    c
                });
            if let Some(inner) = combo.inner {
                changed |= inner;
            }
        });
    });

    changed
}
