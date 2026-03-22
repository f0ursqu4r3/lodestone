use crate::state::AppState;
use crate::ui::layout::PanelId;

pub fn draw(ui: &mut egui::Ui, _state: &mut AppState, _panel_id: PanelId) {
    ui.vertical_centered(|ui| {
        ui.add_space(20.0);
        ui.label("No audio sources");
        ui.label("Audio capture coming soon");
    });
}
