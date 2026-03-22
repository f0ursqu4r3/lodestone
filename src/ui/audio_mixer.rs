use crate::gstreamer::{AudioSourceKind, GstCommand};
use crate::state::AppState;
use crate::ui::layout::PanelId;

/// Draw the audio mixer panel with per-source VU meters, faders, and mute controls.
pub fn draw(ui: &mut egui::Ui, state: &mut AppState, _panel_id: PanelId) {
    // Clone levels and device info before the closure to avoid borrow conflicts.
    let mic_levels = state.audio_levels.mic.clone();
    let system_levels = state.audio_levels.system.clone();
    let has_loopback = state.available_audio_devices.iter().any(|d| d.is_loopback);

    ui.horizontal(|ui| {
        // Mic channel
        draw_channel_strip(ui, state, "Mic", AudioSourceKind::Mic, mic_levels.as_ref());

        ui.separator();

        // System channel — only show if a loopback device is available
        if has_loopback {
            draw_channel_strip(
                ui,
                state,
                "System",
                AudioSourceKind::System,
                system_levels.as_ref(),
            );
        } else {
            ui.vertical(|ui| {
                ui.set_min_width(60.0);
                ui.label("System");
                ui.add_space(10.0);
                ui.weak("Install\nBlackHole\nfor system\naudio");
            });
        }
    });
}

fn draw_channel_strip(
    ui: &mut egui::Ui,
    state: &AppState,
    name: &str,
    kind: AudioSourceKind,
    levels: Option<&crate::gstreamer::AudioLevels>,
) {
    let current_db = levels.map(|l| l.rms_db).unwrap_or(-60.0);
    let peak_db = levels.map(|l| l.peak_db).unwrap_or(-60.0);

    ui.vertical(|ui| {
        ui.set_min_width(60.0);

        // Source name
        ui.label(name);

        // Volume fader
        let vol_id = egui::Id::new(("audio_vol", name));
        let mut volume: f32 = ui.memory(|m| m.data.get_temp(vol_id).unwrap_or(1.0));
        let slider = egui::Slider::new(&mut volume, 0.0..=1.0)
            .vertical()
            .show_value(false);
        if ui.add(slider).changed()
            && let Some(ref tx) = state.command_tx
        {
            let _ = tx.try_send(GstCommand::SetAudioVolume {
                source: kind,
                volume,
            });
        }
        ui.memory_mut(|m| m.data.insert_temp(vol_id, volume));

        // VU meter
        let vu_height = 60.0;
        let vu_width = 12.0;
        let fill_frac = ((current_db + 60.0) / 60.0).clamp(0.0, 1.0);
        let filled_height = vu_height * fill_frac;

        let (rect, _) =
            ui.allocate_exact_size(egui::vec2(vu_width, vu_height), egui::Sense::hover());
        ui.painter()
            .rect_filled(rect, 0.0, egui::Color32::DARK_GRAY);

        let fill_rect =
            egui::Rect::from_min_max(egui::pos2(rect.min.x, rect.max.y - filled_height), rect.max);
        let vu_color = if peak_db > -6.0 {
            egui::Color32::RED
        } else if peak_db > -18.0 {
            egui::Color32::YELLOW
        } else {
            egui::Color32::GREEN
        };
        ui.painter().rect_filled(fill_rect, 0.0, vu_color);

        // Mute toggle
        let mute_id = egui::Id::new(("audio_mute", name));
        let mut muted: bool = ui.memory(|m| m.data.get_temp(mute_id).unwrap_or(false));
        let mute_label = if muted { "M" } else { "m" };
        if ui.button(mute_label).clicked() {
            muted = !muted;
            if let Some(ref tx) = state.command_tx {
                let _ = tx.try_send(GstCommand::SetAudioMuted {
                    source: kind,
                    muted,
                });
            }
        }
        ui.memory_mut(|m| m.data.insert_temp(mute_id, muted));
    });
}
