use crate::state::{AppState, RecordingStatus};
use crate::ui::layout::PanelId;
use crate::ui::theme::active_theme;
use crate::gstreamer::StreamDestination;

/// Stream configuration panel: destination, RTMP URL, and stream key.
///
/// Go Live / Record buttons now live in the toolbar (`toolbar.rs`).
pub fn draw(ui: &mut egui::Ui, state: &mut AppState, panel_id: PanelId) {
    let theme = active_theme(ui.ctx());
    let mut changed = false;

    // --- Destination selector ---
    ui.label(
        egui::RichText::new("Destination")
            .color(theme.text_primary)
            .strong(),
    );
    ui.add_space(2.0);

    let current_label = match &state.settings.stream.destination {
        StreamDestination::Twitch => "Twitch",
        StreamDestination::YouTube => "YouTube",
        StreamDestination::CustomRtmp { .. } => "Custom RTMP",
    };

    egui::ComboBox::from_id_salt(("stream_dest_combo", panel_id.0))
        .selected_text(current_label)
        .width(ui.available_width() - 8.0)
        .show_ui(ui, |ui| {
            changed |= ui
                .selectable_value(
                    &mut state.settings.stream.destination,
                    StreamDestination::Twitch,
                    "Twitch",
                )
                .changed();
            changed |= ui
                .selectable_value(
                    &mut state.settings.stream.destination,
                    StreamDestination::YouTube,
                    "YouTube",
                )
                .changed();
            if ui
                .selectable_label(
                    matches!(
                        state.settings.stream.destination,
                        StreamDestination::CustomRtmp { .. }
                    ),
                    "Custom RTMP",
                )
                .clicked()
                && !matches!(
                    state.settings.stream.destination,
                    StreamDestination::CustomRtmp { .. }
                )
            {
                state.settings.stream.destination =
                    StreamDestination::CustomRtmp { url: String::new() };
                changed = true;
            }
        });

    ui.add_space(8.0);

    // --- Custom RTMP URL input (shown only for Custom RTMP destination) ---
    if let StreamDestination::CustomRtmp { url } = &mut state.settings.stream.destination {
        ui.label(
            egui::RichText::new("RTMP URL")
                .color(theme.text_secondary)
                .size(12.0),
        );
        ui.add_space(2.0);

        let input = egui::TextEdit::singleline(url)
            .hint_text("rtmp://your.server/live")
            .text_color(theme.text_primary)
            .desired_width(ui.available_width() - 8.0);
        changed |= ui.add(input).changed();

        ui.add_space(8.0);
    }

    // --- Stream key input (password style) ---
    if !matches!(
        state.settings.stream.destination,
        StreamDestination::CustomRtmp { .. }
    ) {
        ui.label(
            egui::RichText::new("Stream Key")
                .color(theme.text_secondary)
                .size(12.0),
        );
        ui.add_space(2.0);

        let key_input = egui::TextEdit::singleline(&mut state.settings.stream.stream_key)
            .password(true)
            .text_color(theme.text_primary)
            .desired_width(ui.available_width() - 8.0);
        changed |= ui.add(key_input).changed();
    }

    if changed {
        state.settings_dirty = true;
        state.settings_last_changed = std::time::Instant::now();
    }

    // --- Recording path indicator ---
    if let RecordingStatus::Recording { ref path } = state.recording_status {
        ui.add_space(8.0);
        ui.separator();
        ui.add_space(4.0);
        let path_str = path.to_string_lossy();
        ui.label(
            egui::RichText::new(format!("REC  {}", path_str))
                .color(theme.danger)
                .size(11.0),
        );
    }
}
