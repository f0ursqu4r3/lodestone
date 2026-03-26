use egui::{Align, Layout, Ui};

use crate::gstreamer::{EncoderType, QualityPreset, StreamDestination};
use crate::settings::StreamSettings;

use super::{labeled_row, labeled_row_unimplemented, section_header};

pub(super) fn draw(ui: &mut Ui, settings: &mut StreamSettings) -> bool {
    let mut changed = false;

    section_header(ui, "DESTINATION");

    ui.horizontal(|ui| {
        labeled_row(ui, "Service");
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let current_label = match &settings.destination {
                StreamDestination::Twitch => "Twitch",
                StreamDestination::YouTube => "YouTube",
                StreamDestination::CustomRtmp { .. } => "Custom RTMP",
            };
            let combo = egui::ComboBox::from_id_salt("stream_dest")
                .selected_text(current_label)
                .show_ui(ui, |ui| {
                    let mut c = false;
                    c |= ui
                        .selectable_value(
                            &mut settings.destination,
                            StreamDestination::Twitch,
                            "Twitch",
                        )
                        .changed();
                    c |= ui
                        .selectable_value(
                            &mut settings.destination,
                            StreamDestination::YouTube,
                            "YouTube",
                        )
                        .changed();
                    if ui
                        .selectable_label(
                            matches!(settings.destination, StreamDestination::CustomRtmp { .. }),
                            "Custom RTMP",
                        )
                        .clicked()
                        && !matches!(settings.destination, StreamDestination::CustomRtmp { .. })
                    {
                        settings.destination = StreamDestination::CustomRtmp { url: String::new() };
                        c = true;
                    }
                    c
                });
            if let Some(inner) = combo.inner {
                changed |= inner;
            }
        });
    });

    // Show custom RTMP URL field if applicable
    if let StreamDestination::CustomRtmp { url } = &mut settings.destination {
        ui.horizontal(|ui| {
            labeled_row(ui, "RTMP URL");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .add(egui::TextEdit::singleline(url).desired_width(250.0))
                    .changed()
                {
                    changed = true;
                }
            });
        });
    }

    ui.horizontal(|ui| {
        labeled_row(ui, "Stream Key");
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui
                .add(
                    egui::TextEdit::singleline(&mut settings.stream_key)
                        .password(true)
                        .desired_width(250.0),
                )
                .changed()
            {
                changed = true;
            }
        });
    });

    section_header(ui, "ENCODER");

    ui.horizontal(|ui| {
        labeled_row_unimplemented(ui, "Encoder");
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let combo = egui::ComboBox::from_id_salt("encoder_combo")
                .selected_text(settings.encoder.display_name())
                .show_ui(ui, |ui| {
                    let mut c = false;
                    for enc in EncoderType::all() {
                        c |= ui
                            .selectable_value(
                                &mut settings.encoder,
                                *enc,
                                enc.display_name(),
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

    ui.horizontal(|ui| {
        labeled_row_unimplemented(ui, "Quality");
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let combo = egui::ComboBox::from_id_salt("quality_preset_combo")
                .selected_text(format!("{:?}", settings.quality_preset))
                .show_ui(ui, |ui| {
                    let mut c = false;
                    for preset in QualityPreset::all() {
                        c |= ui
                            .selectable_value(
                                &mut settings.quality_preset,
                                *preset,
                                format!("{preset:?}"),
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

    ui.horizontal(|ui| {
        labeled_row_unimplemented(ui, "Bitrate (kbps)");
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            changed |= ui
                .add(egui::DragValue::new(&mut settings.bitrate_kbps).range(500..=50000))
                .changed();
        });
    });

    ui.horizontal(|ui| {
        labeled_row_unimplemented(ui, "FPS");
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let combo = egui::ComboBox::from_id_salt("stream_fps")
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

    changed
}
