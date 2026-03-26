//! Themed ComboBox dropdown wrapper.

use egui::Ui;

use crate::ui::theme::active_theme;

/// Themed ComboBox dropdown.
///
/// `id` is a unique string identifier for the combo widget. `options` is a
/// slice of `(value, display_label)` pairs. Returns `true` if the selection
/// changed.
pub fn dropdown<T: PartialEq + Clone>(
    ui: &mut Ui,
    id: &str,
    selected: &mut T,
    options: &[(T, String)],
) -> bool {
    let theme = active_theme(ui.ctx());
    let _ = theme; // available for future theming overrides

    let selected_label = options
        .iter()
        .find(|(v, _)| v == selected)
        .map(|(_, label)| label.as_str())
        .unwrap_or("—");

    let mut changed = false;

    egui::ComboBox::from_id_salt(id)
        .selected_text(selected_label)
        .show_ui(ui, |ui| {
            for (value, label) in options {
                if ui
                    .selectable_label(selected == value, label.as_str())
                    .clicked()
                {
                    *selected = value.clone();
                    changed = true;
                }
            }
        });

    changed
}
