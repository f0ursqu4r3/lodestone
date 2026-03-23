use crate::state::AppState;
use crate::ui::layout::tree::PanelId;

pub fn draw(ui: &mut egui::Ui, _state: &mut AppState, _id: PanelId) {
    ui.label("Sources (coming soon)");
}
