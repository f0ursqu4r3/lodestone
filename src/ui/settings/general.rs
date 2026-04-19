use egui::{Align, Layout, Ui};

use crate::gstreamer::GstCommand;
use crate::state::AppState;

use super::{
    draw_toggle, draw_toggle_unimplemented, labeled_row, labeled_row_unimplemented, section_header,
};

pub(super) fn draw(ui: &mut Ui, state: &mut AppState) -> bool {
    let mut changed = false;
    let settings = &mut state.settings.general;

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

    let prev_exclude = settings.exclude_self_from_capture;
    changed |= draw_toggle(
        ui,
        "Exclude Lodestone from capture",
        &mut settings.exclude_self_from_capture,
    );
    if settings.exclude_self_from_capture != prev_exclude {
        if let Some(tx) = &state.command_tx {
            let _ = tx.try_send(GstCommand::UpdateDisplayExclusion {
                exclude_self: settings.exclude_self_from_capture,
            });
        }
        // Signal the render loop to update window display affinity (Windows).
        state.display_exclusion_changed = true;
    }

    section_header(ui, "GRID & GUIDES");

    changed |= draw_toggle(ui, "Show grid overlay", &mut settings.show_grid);

    // Grid-dependent controls: disabled when grid overlay is off.
    ui.add_enabled_ui(settings.show_grid, |ui| {
        changed |= draw_toggle(ui, "Snap to grid", &mut settings.snap_to_grid);

        // Grid preset combo
        ui.horizontal(|ui| {
            labeled_row(ui, "Grid preset");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                const PRESETS: &[&str] = &["8", "16", "32", "64", "thirds", "quarters", "custom"];
                let display = if settings.grid_preset.is_empty() {
                    "custom"
                } else {
                    settings.grid_preset.as_str()
                };
                let combo = egui::ComboBox::from_id_salt("grid_preset_combo")
                    .selected_text(display)
                    .show_ui(ui, |ui| {
                        let mut c = false;
                        for &preset in PRESETS {
                            c |= ui
                                .selectable_value(
                                    &mut settings.grid_preset,
                                    preset.to_string(),
                                    preset,
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

        // Grid size slider — only shown when preset is "custom" or empty
        let show_grid_size = settings.grid_preset.is_empty() || settings.grid_preset == "custom";
        if show_grid_size {
            ui.horizontal(|ui| {
                labeled_row(ui, "Grid size (px)");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let slider = ui.add(
                        egui::Slider::new(&mut settings.snap_grid_size, 1.0..=200.0)
                            .clamping(egui::SliderClamping::Always),
                    );
                    if slider.changed() {
                        changed = true;
                    }
                });
            });
        }
    });

    changed |= draw_toggle(ui, "Rule of thirds", &mut settings.show_thirds);
    changed |= draw_toggle(ui, "Safe zones", &mut settings.show_safe_zones);

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
