use crate::state::{AppState, RecordingStatus};
use crate::ui::layout::PanelId;
use crate::ui::theme;

/// Destination options for the combo box
const DESTINATIONS: &[&str] = &["Twitch", "YouTube", "Custom RTMP"];

/// Stream configuration panel: destination, RTMP URL, and stream key.
///
/// Go Live / Record buttons now live in the toolbar (`toolbar.rs`).
pub fn draw(ui: &mut egui::Ui, state: &mut AppState, panel_id: PanelId) {
    // --- Destination selector ---
    ui.label(
        egui::RichText::new("Destination")
            .color(theme::TEXT_PRIMARY)
            .strong(),
    );
    ui.add_space(2.0);

    let dest_idx_id = egui::Id::new(("stream_dest_idx", panel_id.0));
    let mut dest_idx: usize = ui.memory(|m| m.data.get_temp::<usize>(dest_idx_id).unwrap_or(0));

    egui::ComboBox::from_id_salt(("stream_dest_combo", panel_id.0))
        .selected_text(DESTINATIONS[dest_idx])
        .width(ui.available_width() - 8.0)
        .show_ui(ui, |ui| {
            for (i, &name) in DESTINATIONS.iter().enumerate() {
                ui.selectable_value(&mut dest_idx, i, name);
            }
        });
    ui.memory_mut(|m| m.data.insert_temp(dest_idx_id, dest_idx));

    ui.add_space(8.0);

    // --- Custom RTMP URL input (shown only for Custom RTMP destination) ---
    if dest_idx == 2 {
        ui.label(
            egui::RichText::new("RTMP URL")
                .color(theme::TEXT_SECONDARY)
                .size(12.0),
        );
        ui.add_space(2.0);

        let rtmp_url_id = egui::Id::new(("rtmp_url", panel_id.0));
        let mut rtmp_url: String =
            ui.memory(|m| m.data.get_temp::<String>(rtmp_url_id).unwrap_or_default());

        let input = egui::TextEdit::singleline(&mut rtmp_url)
            .hint_text("rtmp://your.server/live")
            .text_color(theme::TEXT_PRIMARY)
            .desired_width(ui.available_width() - 8.0);
        ui.add(input);
        ui.memory_mut(|m| m.data.insert_temp(rtmp_url_id, rtmp_url));

        ui.add_space(8.0);
    }

    // --- Stream key input (password style) ---
    ui.label(
        egui::RichText::new("Stream Key")
            .color(theme::TEXT_SECONDARY)
            .size(12.0),
    );
    ui.add_space(2.0);

    let key_id = egui::Id::new(("stream_key", panel_id.0));
    let mut stream_key: String =
        ui.memory(|m| m.data.get_temp::<String>(key_id).unwrap_or_default());

    let key_input = egui::TextEdit::singleline(&mut stream_key)
        .password(true)
        .text_color(theme::TEXT_PRIMARY)
        .desired_width(ui.available_width() - 8.0);
    ui.add(key_input);
    ui.memory_mut(|m| m.data.insert_temp(key_id, stream_key));

    // --- Recording path indicator ---
    if let RecordingStatus::Recording { ref path } = state.recording_status {
        ui.add_space(8.0);
        ui.separator();
        ui.add_space(4.0);
        let path_str = path.to_string_lossy();
        ui.label(
            egui::RichText::new(format!("REC  {}", path_str))
                .color(theme::RED_LIVE)
                .size(11.0),
        );
    }
}
