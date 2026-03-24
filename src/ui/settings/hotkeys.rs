use egui::{Align, Layout, Ui};

use crate::settings::HotkeySettings;
use crate::ui::theme::TEXT_MUTED;

use super::{labeled_row, section_header};

pub(super) fn draw(ui: &mut Ui, settings: &mut HotkeySettings) -> bool {
    let mut changed = false;

    ui.label(
        egui::RichText::new(
            "Hotkeys are not yet implemented. Bindings are saved but have no effect.",
        )
        .size(11.0)
        .color(TEXT_MUTED)
        .italics(),
    );
    ui.add_space(8.0);

    section_header(ui, "BINDINGS");

    // Default hotkey actions to display even when the map is empty
    let default_actions = [
        ("start_stream", "Start Streaming"),
        ("stop_stream", "Stop Streaming"),
        ("start_recording", "Start Recording"),
        ("stop_recording", "Stop Recording"),
        ("toggle_mute_mic", "Toggle Mute Mic"),
        ("toggle_mute_desktop", "Toggle Mute Desktop"),
    ];

    for (key, label) in &default_actions {
        ui.horizontal(|ui| {
            labeled_row(ui, label);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let current = settings.bindings.get(*key).cloned().unwrap_or_default();
                let mut binding = current;
                if ui
                    .add(
                        egui::TextEdit::singleline(&mut binding)
                            .desired_width(150.0)
                            .hint_text("Not set"),
                    )
                    .changed()
                {
                    settings.bindings.insert(key.to_string(), binding);
                    changed = true;
                }
            });
        });
    }

    changed
}
