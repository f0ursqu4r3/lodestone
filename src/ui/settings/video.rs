use egui::{Align, Layout, Ui};

use crate::settings::VideoSettings;

use super::{labeled_row, labeled_row_unimplemented, section_header};

struct ResolutionOption {
    value: String,
    label: String,
}

fn build_resolution_options(
    detected: Option<(u32, u32)>,
    include_720p: bool,
) -> Vec<ResolutionOption> {
    let presets: Vec<(u32, u32)> = if include_720p {
        vec![(1280, 720), (1920, 1080), (2560, 1440), (3840, 2160)]
    } else {
        vec![(1920, 1080), (2560, 1440), (3840, 2160)]
    };

    let mut options: Vec<ResolutionOption> = presets
        .iter()
        .map(|&(w, h)| {
            let value = format!("{w}x{h}");
            let is_detected = detected == Some((w, h));
            let label = if is_detected {
                format!("{value} (Display)")
            } else {
                value.clone()
            };
            ResolutionOption { value, label }
        })
        .collect();

    // Insert detected resolution in sorted position if not already a preset
    if let Some((w, h)) = detected
        && !presets.contains(&(w, h))
    {
        let value = format!("{w}x{h}");
        let label = format!("{value} (Display)");
        let pixels = w as u64 * h as u64;
        let pos = options
            .iter()
            .position(|o| {
                let (ow, oh) = crate::renderer::compositor::parse_resolution(&o.value);
                (ow as u64 * oh as u64) > pixels
            })
            .unwrap_or(options.len());
        options.insert(pos, ResolutionOption { value, label });
    }

    options
}

pub(super) fn draw(
    ui: &mut Ui,
    settings: &mut VideoSettings,
    detected_resolution: Option<(u32, u32)>,
) -> bool {
    let mut changed = false;

    section_header(ui, "RESOLUTION");

    let base_options = build_resolution_options(detected_resolution, false);
    let is_custom_base = !base_options.iter().any(|o| o.value == settings.base_resolution);
    let base_display_text = if is_custom_base {
        format!("Custom ({})", settings.base_resolution)
    } else {
        base_options
            .iter()
            .find(|o| o.value == settings.base_resolution)
            .map(|o| o.label.clone())
            .unwrap_or_else(|| settings.base_resolution.clone())
    };

    ui.horizontal(|ui| {
        labeled_row(ui, "Base (Canvas) Resolution");
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let combo = egui::ComboBox::from_id_salt("base_res")
                .selected_text(&base_display_text)
                .show_ui(ui, |ui| {
                    let mut c = false;
                    for opt in &base_options {
                        c |= ui
                            .selectable_value(
                                &mut settings.base_resolution,
                                opt.value.clone(),
                                &opt.label,
                            )
                            .changed();
                    }
                    if ui.selectable_label(is_custom_base, "Custom...").clicked() && !is_custom_base
                    {
                        settings.base_resolution = "custom".to_string();
                        c = true;
                    }
                    c
                });
            if let Some(inner) = combo.inner {
                changed |= inner;
            }
        });
    });

    // Custom resolution input for base resolution
    if is_custom_base || settings.base_resolution == "custom" {
        if settings.base_resolution == "custom" {
            settings.base_resolution = "1920x1080".to_string();
            changed = true;
        }
        ui.horizontal(|ui| {
            labeled_row(ui, "");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let (mut w, mut h) =
                    crate::renderer::compositor::parse_resolution(&settings.base_resolution);
                let w_changed = ui
                    .add(egui::DragValue::new(&mut w).range(2..=7680).suffix("w"))
                    .changed();
                ui.label("x");
                let h_changed = ui
                    .add(egui::DragValue::new(&mut h).range(2..=7680).suffix("h"))
                    .changed();
                if w_changed || h_changed {
                    w = (w / 2) * 2;
                    h = (h / 2) * 2;
                    w = w.max(2);
                    h = h.max(2);
                    settings.base_resolution = format!("{w}x{h}");
                    changed = true;
                }
            });
        });
    }

    let output_options = build_resolution_options(detected_resolution, true);
    let is_custom_output = !output_options
        .iter()
        .any(|o| o.value == settings.output_resolution);
    let output_display_text = if is_custom_output {
        format!("Custom ({})", settings.output_resolution)
    } else {
        output_options
            .iter()
            .find(|o| o.value == settings.output_resolution)
            .map(|o| o.label.clone())
            .unwrap_or_else(|| settings.output_resolution.clone())
    };

    ui.horizontal(|ui| {
        labeled_row(ui, "Output (Scaled) Resolution");
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let combo = egui::ComboBox::from_id_salt("output_res")
                .selected_text(&output_display_text)
                .show_ui(ui, |ui| {
                    let mut c = false;
                    for opt in &output_options {
                        c |= ui
                            .selectable_value(
                                &mut settings.output_resolution,
                                opt.value.clone(),
                                &opt.label,
                            )
                            .changed();
                    }
                    if ui
                        .selectable_label(is_custom_output, "Custom...")
                        .clicked()
                        && !is_custom_output
                    {
                        settings.output_resolution = "custom".to_string();
                        c = true;
                    }
                    c
                });
            if let Some(inner) = combo.inner {
                changed |= inner;
            }
        });
    });

    // Custom resolution input for output resolution
    if is_custom_output || settings.output_resolution == "custom" {
        if settings.output_resolution == "custom" {
            settings.output_resolution = "1920x1080".to_string();
            changed = true;
        }
        ui.horizontal(|ui| {
            labeled_row(ui, "");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let (mut w, mut h) =
                    crate::renderer::compositor::parse_resolution(&settings.output_resolution);
                let w_changed = ui
                    .add(egui::DragValue::new(&mut w).range(2..=7680).suffix("w"))
                    .changed();
                ui.label("x");
                let h_changed = ui
                    .add(egui::DragValue::new(&mut h).range(2..=7680).suffix("h"))
                    .changed();
                if w_changed || h_changed {
                    w = (w / 2) * 2;
                    h = (h / 2) * 2;
                    w = w.max(2);
                    h = h.max(2);
                    settings.output_resolution = format!("{w}x{h}");
                    changed = true;
                }
            });
        });
    }

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
