use egui::{Align, Layout, Ui};

use crate::gstreamer::{AudioSourceKind, GstCommand};
use crate::state::AppState;
use crate::ui::widgets::layout;

pub(super) fn draw(ui: &mut Ui, state: &mut AppState) -> bool {
    let mut changed = false;

    layout::section(ui, "DEVICES", |ui| {
        ui.horizontal(|ui| {
            ui.label("Input Device");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                // Build the list of non-loopback (microphone) devices from runtime enumeration.
                let mic_devices: Vec<_> = state
                    .available_audio_devices
                    .iter()
                    .filter(|d| !d.is_loopback)
                    .cloned()
                    .collect();

                let selected_text = mic_devices
                    .iter()
                    .find(|d| d.uid == state.settings.audio.input_device)
                    .map(|d| d.name.as_str())
                    .unwrap_or(&state.settings.audio.input_device);

                let combo = egui::ComboBox::from_id_salt("audio_input")
                    .selected_text(selected_text.to_string())
                    .show_ui(ui, |ui| {
                        let mut c = false;
                        for dev in &mic_devices {
                            let selected = ui
                                .selectable_value(
                                    &mut state.settings.audio.input_device,
                                    dev.uid.clone(),
                                    &dev.name,
                                )
                                .changed();
                            if selected {
                                // Notify the GStreamer thread of the new device.
                                if let Some(tx) = &state.command_tx {
                                    let _ = tx.try_send(GstCommand::SetAudioDevice {
                                        source: AudioSourceKind::Mic,
                                        device_uid: dev.uid.clone(),
                                    });
                                    let _ = tx.try_send(GstCommand::SetAudioVolume {
                                        source: AudioSourceKind::Mic,
                                        volume: state.mic_volume,
                                    });
                                    let _ = tx.try_send(GstCommand::SetAudioMuted {
                                        source: AudioSourceKind::Mic,
                                        muted: state.mic_muted,
                                    });
                                }
                                c = true;
                            }
                        }
                        c
                    });
                if let Some(inner) = combo.inner {
                    changed |= inner;
                }
            });
        });

        ui.horizontal(|ui| {
            ui.label("Output Device");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let loopback_devices: Vec<_> = state
                    .available_audio_devices
                    .iter()
                    .filter(|d| d.is_loopback)
                    .cloned()
                    .collect();

                let selected_text = loopback_devices
                    .iter()
                    .find(|d| d.uid == state.settings.audio.output_device)
                    .map(|d| d.name.as_str())
                    .unwrap_or_else(|| {
                        if loopback_devices.is_empty() {
                            "No loopback device"
                        } else {
                            &state.settings.audio.output_device
                        }
                    });

                let combo = egui::ComboBox::from_id_salt("audio_output")
                    .selected_text(selected_text.to_string())
                    .show_ui(ui, |ui| {
                        let mut c = false;
                        for dev in &loopback_devices {
                            let selected = ui
                                .selectable_value(
                                    &mut state.settings.audio.output_device,
                                    dev.uid.clone(),
                                    &dev.name,
                                )
                                .changed();
                            if selected {
                                if let Some(tx) = &state.command_tx {
                                    let _ = tx.try_send(GstCommand::SetAudioDevice {
                                        source: AudioSourceKind::System,
                                        device_uid: dev.uid.clone(),
                                    });
                                    let _ = tx.try_send(GstCommand::SetAudioVolume {
                                        source: AudioSourceKind::System,
                                        volume: state.system_volume,
                                    });
                                    let _ = tx.try_send(GstCommand::SetAudioMuted {
                                        source: AudioSourceKind::System,
                                        muted: state.system_muted,
                                    });
                                }
                                c = true;
                            }
                        }
                        c
                    });
                if let Some(inner) = combo.inner {
                    changed |= inner;
                }
            });
        });
    });

    layout::section(ui, "FORMAT", |ui| {
        ui.horizontal(|ui| {
            ui.label("Sample Rate (not yet implemented)");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let combo = egui::ComboBox::from_id_salt("sample_rate")
                    .selected_text(format!("{} Hz", state.settings.audio.sample_rate))
                    .show_ui(ui, |ui| {
                        let mut c = false;
                        for rate in &[44100u32, 48000, 96000] {
                            c |= ui
                                .selectable_value(
                                    &mut state.settings.audio.sample_rate,
                                    *rate,
                                    format!("{rate} Hz"),
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
            ui.label("Monitoring (not yet implemented)");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let combo = egui::ComboBox::from_id_salt("monitoring")
                    .selected_text(&state.settings.audio.monitoring)
                    .show_ui(ui, |ui| {
                        let mut c = false;
                        for mode in &["off", "monitor only", "monitor and output"] {
                            c |= ui
                                .selectable_value(
                                    &mut state.settings.audio.monitoring,
                                    mode.to_string(),
                                    *mode,
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
    });

    changed
}
