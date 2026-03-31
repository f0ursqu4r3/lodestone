//! Live panel — read-only monitor showing the program (live) output.

use crate::state::AppState;
use crate::ui::layout::PanelId;

pub fn draw(ui: &mut egui::Ui, _state: &mut AppState, _panel_id: PanelId) {
    let theme = crate::ui::theme::active_theme(ui.ctx());
    let panel_rect = ui.available_rect_before_wrap();
    ui.allocate_rect(panel_rect, egui::Sense::hover());

    let painter = ui.painter_at(panel_rect);
    painter.rect_filled(panel_rect, 0.0, theme.bg_panel);
    painter.text(
        panel_rect.center(),
        egui::Align2::CENTER_CENTER,
        "Live Output",
        egui::FontId::proportional(14.0),
        theme.text_muted,
    );
}
