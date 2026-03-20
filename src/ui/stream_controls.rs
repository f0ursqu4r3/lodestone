use crate::state::{AppState, StreamStatus};
use crate::ui::layout::PanelId;

/// Destination options for the combo box
const DESTINATIONS: &[&str] = &["Twitch", "YouTube", "Custom RTMP"];

pub fn draw(ui: &mut egui::Ui, state: &mut AppState, panel_id: PanelId) {
    // Destination combo box (stored in egui memory)
    let dest_idx_id = egui::Id::new(("stream_dest_idx", panel_id.0));
    let mut dest_idx: usize = ui.memory(|m| m.data.get_temp::<usize>(dest_idx_id).unwrap_or(0));

    egui::ComboBox::from_label("Destination")
        .selected_text(DESTINATIONS[dest_idx])
        .show_ui(ui, |ui| {
            for (i, &name) in DESTINATIONS.iter().enumerate() {
                ui.selectable_value(&mut dest_idx, i, name);
            }
        });
    ui.memory_mut(|m| m.data.insert_temp(dest_idx_id, dest_idx));

    // Stream key input (password style), stored in egui memory
    let key_id = egui::Id::new(("stream_key", panel_id.0));
    let mut stream_key: String =
        ui.memory(|m| m.data.get_temp::<String>(key_id).unwrap_or_default());

    ui.horizontal(|ui| {
        ui.label("Stream Key");
        ui.add(egui::TextEdit::singleline(&mut stream_key).password(true));
    });
    ui.memory_mut(|m| m.data.insert_temp(key_id, stream_key));

    ui.separator();

    // Go Live / Stop button
    let is_live = state.stream_status.is_live();
    let button_text = if is_live { "Stop" } else { "Go Live" };
    let button_color = if is_live {
        egui::Color32::from_rgb(180, 30, 30)
    } else {
        egui::Color32::from_rgb(30, 160, 30)
    };

    let button = egui::Button::new(
        egui::RichText::new(button_text)
            .strong()
            .size(18.0)
            .color(egui::Color32::WHITE),
    )
    .fill(button_color)
    .min_size(egui::vec2(140.0, 40.0));

    if ui.add(button).clicked() {
        state.stream_status = if is_live {
            StreamStatus::Offline
        } else {
            StreamStatus::Live {
                uptime_secs: 0.0,
                bitrate_kbps: 0.0,
                dropped_frames: 0,
            }
        };
    }

    // Stats when live
    if let StreamStatus::Live {
        uptime_secs,
        bitrate_kbps,
        dropped_frames,
    } = &state.stream_status
    {
        ui.separator();

        let total = *uptime_secs as u64;
        let hours = total / 3600;
        let minutes = (total % 3600) / 60;
        let seconds = total % 60;
        ui.label(format!(
            "Uptime: {:02}:{:02}:{:02}",
            hours, minutes, seconds
        ));
        ui.label(format!("Bitrate: {:.0} kbps", bitrate_kbps));
        ui.label(format!("Dropped frames: {}", dropped_frames));
    }
}
