pub mod theme;

pub mod audio_mixer;
pub mod layout;
pub mod preview_panel;
pub mod properties_panel;
pub mod scenes_panel;
pub mod settings_window;
pub mod sources_panel;
pub mod stream_controls;
pub mod toolbar;
pub mod transform_handles;

use crate::state::AppState;
use layout::{PanelId, PanelType};

pub fn draw_panel(panel_type: PanelType, ui: &mut egui::Ui, state: &mut AppState, id: PanelId) {
    match panel_type {
        PanelType::Preview => preview_panel::draw(ui, state, id),
        PanelType::SceneEditor => {
            // Legacy: render sources panel as fallback for saved layouts
            sources_panel::draw(ui, state, id);
        }
        PanelType::AudioMixer => audio_mixer::draw(ui, state, id),
        PanelType::StreamControls => stream_controls::draw(ui, state, id),
        PanelType::Sources => sources_panel::draw(ui, state, id),
        PanelType::Scenes => scenes_panel::draw(ui, state, id),
        PanelType::Properties => properties_panel::draw(ui, state, id),
    }
}
