use crate::state::AppState;
use crate::ui::layout::PanelId;

pub fn draw(ui: &mut egui::Ui, _state: &mut AppState, _panel_id: PanelId) {
    ui.centered_and_justified(|ui| {
        ui.label("Preview");
    });
}
