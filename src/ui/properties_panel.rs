//! Properties panel — context-sensitive property editor for the selected source.
//!
//! Shows transform, opacity, and source-specific settings for whichever source
//! is selected in the Sources panel (`state.selected_source_id`).

use crate::scene::{SourceProperties, SourceType};
use crate::state::AppState;
use crate::ui::layout::tree::PanelId;
use crate::ui::theme::{TEXT_MUTED, TEXT_SECONDARY};

/// Draw the properties panel. Shows an empty-state message when no source is
/// selected, or transform / opacity / source-specific controls when one is.
pub fn draw(ui: &mut egui::Ui, state: &mut AppState, _id: PanelId) {
    let Some(selected_id) = state.selected_source_id else {
        // Empty state: centered muted label.
        ui.vertical_centered(|ui| {
            ui.add_space(ui.available_height() / 3.0);
            ui.label(
                egui::RichText::new("Select a source to view properties")
                    .color(TEXT_MUTED)
                    .size(11.0),
            );
        });
        return;
    };

    // Find the source index so we can get a mutable reference later.
    let Some(source_idx) = state.sources.iter().position(|s| s.id == selected_id) else {
        ui.label(
            egui::RichText::new("Source not found")
                .color(TEXT_MUTED)
                .size(11.0),
        );
        return;
    };

    let mut changed = false;

    // ── TRANSFORM ──

    section_label(ui, "TRANSFORM");

    ui.add_space(4.0);

    {
        let source = &mut state.sources[source_idx];

        // X / Y row
        ui.horizontal(|ui| {
            changed |= drag_field(ui, "X", &mut source.transform.x);
            ui.add_space(8.0);
            changed |= drag_field(ui, "Y", &mut source.transform.y);
        });

        ui.add_space(2.0);

        // W / H row
        ui.horizontal(|ui| {
            changed |= drag_field(ui, "W", &mut source.transform.width);
            ui.add_space(8.0);
            changed |= drag_field(ui, "H", &mut source.transform.height);
        });
    }

    ui.add_space(12.0);

    // ── OPACITY ──

    section_label(ui, "OPACITY");

    ui.add_space(4.0);

    {
        let source = &mut state.sources[source_idx];
        ui.horizontal(|ui| {
            let slider = egui::Slider::new(&mut source.opacity, 0.0..=1.0).show_value(false);
            if ui.add(slider).changed() {
                changed = true;
            }
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(format!("{}%", (source.opacity * 100.0).round() as u32))
                    .color(TEXT_SECONDARY)
                    .size(10.0),
            );
        });
    }

    ui.add_space(12.0);

    // ── SOURCE ──

    let source_type = state.sources[source_idx].source_type.clone();
    match source_type {
        SourceType::Display => {
            section_label(ui, "SOURCE");
            ui.add_space(4.0);

            let monitor_count = state.monitor_count;
            let source = &mut state.sources[source_idx];
            let SourceProperties::Display {
                ref mut screen_index,
            } = source.properties;

            let prev_index = *screen_index;
            let selected_label = format!("Monitor {}", *screen_index);
            egui::ComboBox::from_id_salt(egui::Id::new("props_monitor_combo").with(selected_id.0))
                .selected_text(&selected_label)
                .width(ui.available_width() - 8.0)
                .show_ui(ui, |ui| {
                    for i in 0..monitor_count as u32 {
                        let label = format!("Monitor {i}");
                        ui.selectable_value(screen_index, i, label);
                    }
                });

            if *screen_index != prev_index {
                changed = true;
            }
        }
        _ => {
            // Other source types don't have extra properties yet.
        }
    }

    // Mark dirty so the scene collection gets persisted.
    if changed {
        state.scenes_dirty = true;
        state.scenes_last_changed = std::time::Instant::now();
    }
}

/// Render a section heading in the style: 9px uppercase `TEXT_MUTED` with letter spacing.
fn section_label(ui: &mut egui::Ui, text: &str) {
    ui.label(egui::RichText::new(text).color(TEXT_MUTED).size(9.0));
}

/// Render a labeled `DragValue` field and return whether the value changed.
fn drag_field(ui: &mut egui::Ui, label: &str, value: &mut f32) -> bool {
    ui.label(egui::RichText::new(label).color(TEXT_MUTED).size(10.0));
    ui.add(
        egui::DragValue::new(value)
            .speed(1.0)
            .update_while_editing(false),
    )
    .changed()
}
