use egui::{Align, Layout, Ui};

use crate::settings::AdvancedSettings;

use super::{labeled_row_unimplemented, section_header};

pub(super) fn draw(ui: &mut Ui, settings: &mut AdvancedSettings) -> bool {
    let mut changed = false;

    section_header(ui, "PERFORMANCE");

    ui.horizontal(|ui| {
        labeled_row_unimplemented(ui, "Process Priority");
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let combo = egui::ComboBox::from_id_salt("priority_combo")
                .selected_text(&settings.process_priority)
                .show_ui(ui, |ui| {
                    let mut c = false;
                    for p in &["low", "normal", "high", "realtime"] {
                        c |= ui
                            .selectable_value(&mut settings.process_priority, p.to_string(), *p)
                            .changed();
                    }
                    c
                });
            if let Some(inner) = combo.inner {
                changed |= inner;
            }
        });
    });

    section_header(ui, "NETWORK");

    ui.horizontal(|ui| {
        labeled_row_unimplemented(ui, "Network Buffer Size");
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            changed |= ui
                .add(
                    egui::DragValue::new(&mut settings.network_buffer_size_kb)
                        .range(256..=16384)
                        .suffix(" KB"),
                )
                .changed();
        });
    });

    changed
}
