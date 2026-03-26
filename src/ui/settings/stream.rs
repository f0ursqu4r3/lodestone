use egui::{Align, Layout, Ui};

use crate::gstreamer::{QualityPreset, StreamDestination};
use crate::settings::StreamSettings;

use super::{labeled_row, section_header};

pub(super) fn draw(
    ui: &mut Ui,
    settings: &mut StreamSettings,
    available_encoders: &[crate::gstreamer::AvailableEncoder],
) -> bool {
    let mut changed = false;

    section_header(ui, "DESTINATION");

    // Destination dropdown
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

    // Stream key — shown for Twitch/YouTube
    if !matches!(settings.destination, StreamDestination::CustomRtmp { .. }) {
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
    }

    // RTMP URL — shown only for Custom RTMP
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

    ui.separator();

    section_header(ui, "ENCODER");

    // Encoder dropdown
    ui.horizontal(|ui| {
        labeled_row(ui, "Encoder");
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if draw_encoder_dropdown(ui, &mut settings.encoder, available_encoders) {
                changed = true;
            }
        });
    });

    // Quality toggle buttons + custom bitrate
    ui.horizontal(|ui| {
        labeled_row(ui, "Quality");
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if draw_quality_presets(ui, &mut settings.quality_preset, &mut settings.bitrate_kbps) {
                changed = true;
            }
        });
    });

    // FPS toggle buttons
    ui.horizontal(|ui| {
        labeled_row(ui, "FPS");
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if draw_fps_toggles(ui, &mut settings.fps) {
                changed = true;
            }
        });
    });

    changed
}

// ── Shared helpers (pub(super) so record.rs can reuse) ────────────────────────

pub(super) fn draw_encoder_dropdown(
    ui: &mut egui::Ui,
    selected: &mut crate::gstreamer::EncoderType,
    available: &[crate::gstreamer::AvailableEncoder],
) -> bool {
    let mut changed = false;
    let current_label = available
        .iter()
        .find(|e| e.encoder_type == *selected)
        .map(|e| {
            let name = e.encoder_type.display_name();
            if e.is_recommended {
                format!("{name} — Recommended")
            } else {
                name.to_string()
            }
        })
        .unwrap_or_else(|| selected.display_name().to_string());

    egui::ComboBox::from_label("")
        .selected_text(&current_label)
        .show_ui(ui, |ui| {
            for enc in available {
                let label = if enc.is_recommended {
                    format!("{} — Recommended", enc.encoder_type.display_name())
                } else {
                    enc.encoder_type.display_name().to_string()
                };
                if ui
                    .selectable_value(selected, enc.encoder_type, &label)
                    .changed()
                {
                    changed = true;
                }
            }
        });
    changed
}

pub(super) fn draw_quality_presets(
    ui: &mut egui::Ui,
    preset: &mut crate::gstreamer::QualityPreset,
    custom_bitrate: &mut u32,
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        for &p in QualityPreset::all() {
            let label = match p {
                QualityPreset::Low => "Low",
                QualityPreset::Medium => "Medium",
                QualityPreset::High => "High",
                QualityPreset::Custom => "Custom",
            };
            if ui.selectable_label(*preset == p, label).clicked() {
                *preset = p;
                changed = true;
            }
        }
    });
    if *preset == QualityPreset::Custom {
        if ui
            .add(
                egui::DragValue::new(custom_bitrate)
                    .range(500..=50000)
                    .suffix(" kbps"),
            )
            .changed()
        {
            changed = true;
        }
    } else {
        ui.label(
            egui::RichText::new(format!("{} kbps", preset.bitrate_kbps()))
                .weak()
                .size(11.0),
        );
    }
    changed
}

pub(super) fn draw_fps_toggles(ui: &mut egui::Ui, fps: &mut u32) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        for &f in &[24u32, 30, 60] {
            if ui.selectable_label(*fps == f, f.to_string()).clicked() {
                *fps = f;
                changed = true;
            }
        }
    });
    changed
}
