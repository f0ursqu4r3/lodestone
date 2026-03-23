pub mod theme;

pub mod audio_mixer;
pub mod layout;
pub mod preview_panel;
pub mod scene_editor;
pub mod settings_window;
pub mod stream_controls;

use crate::state::AppState;
use layout::{PanelId, PanelType};

pub fn draw_panel(panel_type: PanelType, ui: &mut egui::Ui, state: &mut AppState, id: PanelId) {
    match panel_type {
        PanelType::Preview => preview_panel::draw(ui, state, id),
        PanelType::SceneEditor => scene_editor::draw(ui, state, id),
        PanelType::AudioMixer => audio_mixer::draw(ui, state, id),
        PanelType::StreamControls => stream_controls::draw(ui, state, id),
    }
}
