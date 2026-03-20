use crate::state::AppState;
use crate::ui::layout::PanelId;

pub fn draw(ui: &mut egui::Ui, state: &mut AppState, _panel_id: PanelId) {
    ui.horizontal(|ui| {
        if state.sources.is_empty() {
            ui.label("No sources");
            return;
        }

        // Clone ids to avoid borrow issues
        let source_ids: Vec<_> = state.sources.iter().map(|s| s.id).collect();

        for src_id in source_ids {
            // Look up audio level for this source
            let current_db = state
                .audio_levels
                .iter()
                .find(|l| l.source_id == src_id)
                .map(|l| l.current_db)
                .unwrap_or(-60.0);

            if let Some(source) = state.sources.iter_mut().find(|s| s.id == src_id) {
                ui.vertical(|ui| {
                    ui.set_min_width(60.0);

                    // Source name label (truncated)
                    let name = if source.name.len() > 8 {
                        format!("{}…", &source.name[..7])
                    } else {
                        source.name.clone()
                    };
                    ui.label(name);

                    // Vertical volume fader
                    let slider = egui::Slider::new(&mut source.volume, 0.0..=1.0)
                        .vertical()
                        .show_value(false);
                    ui.add(slider);

                    // Simple VU meter: filled rect proportional to current_db (-60..0)
                    let vu_height = 60.0;
                    let vu_width = 12.0;
                    let fill_frac = ((current_db + 60.0) / 60.0).clamp(0.0, 1.0);
                    let filled_height = vu_height * fill_frac;

                    let (rect, _) = ui.allocate_exact_size(
                        egui::vec2(vu_width, vu_height),
                        egui::Sense::hover(),
                    );
                    // Background
                    ui.painter()
                        .rect_filled(rect, 0.0, egui::Color32::DARK_GRAY);
                    // Fill from the bottom
                    let fill_rect = egui::Rect::from_min_max(
                        egui::pos2(rect.min.x, rect.max.y - filled_height),
                        rect.max,
                    );
                    let vu_color = if current_db > -6.0 {
                        egui::Color32::RED
                    } else if current_db > -18.0 {
                        egui::Color32::YELLOW
                    } else {
                        egui::Color32::GREEN
                    };
                    ui.painter().rect_filled(fill_rect, 0.0, vu_color);

                    // Mute toggle
                    let mute_label = if source.muted { "M" } else { "m" };
                    if ui.button(mute_label).clicked() {
                        source.muted = !source.muted;
                    }
                });

                ui.separator();
            }
        }
    });
}
