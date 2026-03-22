use crate::state::{AppState, RecordingStatus, StreamStatus};
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
    ui.memory_mut(|m| m.data.insert_temp(key_id, stream_key.clone()));

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
        if let Some(ref tx) = state.command_tx {
            if is_live {
                let _ = tx.try_send(crate::gstreamer::GstCommand::StopStream);
                state.stream_status = StreamStatus::Offline;
            } else {
                let destination = match dest_idx {
                    1 => crate::gstreamer::StreamDestination::YouTube,
                    2 => crate::gstreamer::StreamDestination::CustomRtmp {
                        url: String::new(), // TODO: add custom URL field
                    },
                    _ => crate::gstreamer::StreamDestination::Twitch,
                };
                let config = crate::gstreamer::StreamConfig {
                    destination,
                    stream_key: stream_key.clone(),
                };
                let _ = tx.try_send(crate::gstreamer::GstCommand::StartStream(config));
                state.stream_status = StreamStatus::Live {
                    uptime_secs: 0.0,
                    bitrate_kbps: 0.0,
                    dropped_frames: 0,
                };
            }
        }
    }

    ui.add_space(6.0);

    // Record / Stop Recording button
    let is_recording = matches!(state.recording_status, RecordingStatus::Recording { .. });
    let rec_button_text = if is_recording {
        "Stop Recording"
    } else {
        "Record"
    };
    let rec_button_color = if is_recording {
        egui::Color32::from_rgb(160, 20, 20)
    } else {
        egui::Color32::from_rgb(200, 60, 60)
    };

    let rec_button = egui::Button::new(
        egui::RichText::new(rec_button_text)
            .strong()
            .size(18.0)
            .color(egui::Color32::WHITE),
    )
    .fill(rec_button_color)
    .min_size(egui::vec2(140.0, 40.0));

    if ui.add(rec_button).clicked() {
        if let Some(ref tx) = state.command_tx {
            if is_recording {
                let _ = tx.try_send(crate::gstreamer::GstCommand::StopRecording);
                state.recording_status = RecordingStatus::Idle;
            } else {
                let video_dir = dirs::video_dir()
                    .or_else(dirs::home_dir)
                    .unwrap_or_else(|| std::path::PathBuf::from("."));
                let timestamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                let filename = format!("lodestone-{}.mkv", timestamp);
                let path = video_dir.join(filename);
                let _ = tx.try_send(crate::gstreamer::GstCommand::StartRecording {
                    path: path.clone(),
                    format: crate::gstreamer::RecordingFormat::Mkv,
                });
                state.recording_status = RecordingStatus::Recording { path };
            }
        }
    }

    // Recording path indicator
    if let RecordingStatus::Recording { ref path } = state.recording_status {
        ui.add_space(4.0);
        let path_str = path.to_string_lossy();
        ui.label(
            egui::RichText::new(format!("REC  {}", path_str))
                .color(egui::Color32::from_rgb(220, 60, 60))
                .size(11.0),
        );
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
