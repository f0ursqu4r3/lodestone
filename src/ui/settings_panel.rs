use crate::settings::{AppSettings, settings_path};
use crate::state::AppState;
use crate::ui::layout::PanelId;

const DESTINATIONS: &[&str] = &["Twitch", "YouTube", "Custom RTMP"];

pub fn draw(ui: &mut egui::Ui, state: &mut AppState, panel_id: PanelId) {
    // Local edit buffer stored in egui memory
    let edit_id = egui::Id::new(("settings_edit_buf", panel_id.0));

    let mut edit: SettingsEdit = ui.memory(|m| {
        m.data
            .get_temp::<SettingsEdit>(edit_id)
            .unwrap_or_else(|| SettingsEdit::from_settings(&state.settings))
    });

    egui::Grid::new(egui::Id::new(("settings_grid", panel_id.0)))
        .num_columns(2)
        .spacing([12.0, 6.0])
        .show(ui, |ui| {
            // Stream key
            ui.label("Stream Key");
            ui.add(egui::TextEdit::singleline(&mut edit.stream_key).password(true));
            ui.end_row();

            // Destination
            ui.label("Destination");
            egui::ComboBox::from_id_salt(egui::Id::new(("settings_dest", panel_id.0)))
                .selected_text(DESTINATIONS[edit.dest_idx])
                .show_ui(ui, |ui| {
                    for (i, &name) in DESTINATIONS.iter().enumerate() {
                        ui.selectable_value(&mut edit.dest_idx, i, name);
                    }
                });
            ui.end_row();

            // Resolution width
            ui.label("Width");
            ui.add(egui::DragValue::new(&mut edit.width).range(320..=7680));
            ui.end_row();

            // Resolution height
            ui.label("Height");
            ui.add(egui::DragValue::new(&mut edit.height).range(240..=4320));
            ui.end_row();

            // Bitrate slider
            ui.label("Bitrate (kbps)");
            ui.add(egui::Slider::new(&mut edit.bitrate_kbps, 500..=20000));
            ui.end_row();
        });

    ui.separator();

    if ui.button("Save").clicked() {
        // Persist the edit buffer back into settings
        state.settings.active_profile = edit.stream_key.clone();
        let path = settings_path();
        if let Err(e) = state.settings.save_to(&path) {
            log::error!("Failed to save settings: {e}");
        }
    }

    ui.memory_mut(|m| m.data.insert_temp(edit_id, edit));
}

#[derive(Clone)]
struct SettingsEdit {
    stream_key: String,
    dest_idx: usize,
    width: u32,
    height: u32,
    bitrate_kbps: u32,
}

impl SettingsEdit {
    fn from_settings(settings: &AppSettings) -> Self {
        Self {
            stream_key: settings.active_profile.clone(),
            dest_idx: 0,
            width: 1920,
            height: 1080,
            bitrate_kbps: 4500,
        }
    }
}
