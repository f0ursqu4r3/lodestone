use egui::{Align, Layout, Ui};

use crate::settings::VideoSettings;

use super::{labeled_row, labeled_row_unimplemented, section_header};

pub(super) fn draw(ui: &mut Ui, settings: &mut VideoSettings) -> bool {
    let mut changed = false;

    section_header(ui, "RESOLUTION");

    ui.horizontal(|ui| {
        labeled_row(ui, "Base (Canvas) Resolution");
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let combo = egui::ComboBox::from_id_salt("base_res")
                .selected_text(&settings.base_resolution)
                .show_ui(ui, |ui| {
                    let mut c = false;
                    for res in &["1920x1080", "2560x1440", "3840x2160"] {
                        c |= ui
                            .selectable_value(&mut settings.base_resolution, res.to_string(), *res)
                            .changed();
                    }
                    c
                });
            if let Some(inner) = combo.inner {
                changed |= inner;
            }
        });
    });

    ui.horizontal(|ui| {
        labeled_row(ui, "Output (Scaled) Resolution");
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let combo = egui::ComboBox::from_id_salt("output_res")
                .selected_text(&settings.output_resolution)
                .show_ui(ui, |ui| {
                    let mut c = false;
                    for res in &["1280x720", "1920x1080", "2560x1440", "3840x2160"] {
                        c |= ui
                            .selectable_value(
                                &mut settings.output_resolution,
                                res.to_string(),
                                *res,
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

    section_header(ui, "FRAME RATE");

    ui.horizontal(|ui| {
        labeled_row_unimplemented(ui, "FPS");
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let combo = egui::ComboBox::from_id_salt("video_fps")
                .selected_text(format!("{}", settings.fps))
                .show_ui(ui, |ui| {
                    let mut c = false;
                    for fps in &[24u32, 30, 48, 60, 120, 144] {
                        c |= ui
                            .selectable_value(&mut settings.fps, *fps, format!("{fps}"))
                            .changed();
                    }
                    c
                });
            if let Some(inner) = combo.inner {
                changed |= inner;
            }
        });
    });

    section_header(ui, "COLOR");

    ui.horizontal(|ui| {
        labeled_row_unimplemented(ui, "Color Space");
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let combo = egui::ComboBox::from_id_salt("color_space")
                .selected_text(&settings.color_space)
                .show_ui(ui, |ui| {
                    let mut c = false;
                    for cs in &["sRGB", "Rec. 709", "Rec. 2100 (PQ)"] {
                        c |= ui
                            .selectable_value(&mut settings.color_space, cs.to_string(), *cs)
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
