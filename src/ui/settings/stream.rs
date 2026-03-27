use egui::{Align, Layout, Ui};

use crate::gstreamer::StreamDestination;
use crate::settings::StreamSettings;
use crate::ui::widgets::{composite, layout};

pub(super) fn draw(
    ui: &mut Ui,
    settings: &mut StreamSettings,
    available_encoders: &[crate::gstreamer::AvailableEncoder],
) -> bool {
    let mut changed = false;

    layout::section(ui, "DESTINATION", |ui| {
        // Destination dropdown
        ui.horizontal(|ui| {
            ui.label("Service");
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
                                matches!(
                                    settings.destination,
                                    StreamDestination::CustomRtmp { .. }
                                ),
                                "Custom RTMP",
                            )
                            .clicked()
                            && !matches!(settings.destination, StreamDestination::CustomRtmp { .. })
                        {
                            settings.destination =
                                StreamDestination::CustomRtmp { url: String::new() };
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
                ui.label("Stream Key");
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
                ui.label("RTMP URL");
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
    });

    ui.separator();

    layout::section(ui, "ENCODER", |ui| {
        // Encoder dropdown
        ui.horizontal(|ui| {
            ui.label("Encoder");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if composite::encoder_dropdown(ui, &mut settings.encoder, available_encoders) {
                    changed = true;
                }
            });
        });

        // Quality toggle buttons + custom bitrate
        ui.horizontal(|ui| {
            ui.label("Quality");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if composite::quality_presets(
                    ui,
                    &mut settings.quality_preset,
                    &mut settings.bitrate_kbps,
                ) {
                    changed = true;
                }
            });
        });

        // FPS toggle buttons
        ui.horizontal(|ui| {
            ui.label("FPS");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if composite::fps_toggles(ui, &mut settings.fps) {
                    changed = true;
                }
            });
        });
    });

    changed
}
