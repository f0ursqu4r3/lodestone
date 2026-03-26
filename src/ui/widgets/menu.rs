//! Menu helper widgets for context menus and popup menus.

use egui::Ui;

#[allow(unused_imports)]
use crate::ui::theme::active_theme;

/// A single menu item matching native context menu look (full-width hover
/// highlight, no button frame). Returns `true` if clicked.
pub fn menu_item(ui: &mut Ui, label: &str) -> bool {
    ui.add(egui::Button::new(label).frame(false)).clicked()
}

/// Menu item with a Phosphor icon prefix. Returns `true` if clicked.
pub fn menu_item_icon(ui: &mut Ui, icon: &str, label: &str) -> bool {
    let text = format!("{icon}  {label}");
    ui.add(egui::Button::new(text).frame(false)).clicked()
}

/// Render a block of menu items with consistent styling: justified layout,
/// compact padding, minimum width. Use [`menu_item`] inside the closure.
pub fn styled_menu(ui: &mut Ui, add_contents: impl FnOnce(&mut Ui)) {
    ui.allocate_ui_with_layout(
        egui::vec2(160.0, 0.0),
        egui::Layout::top_down_justified(egui::Align::LEFT),
        |ui| {
            ui.style_mut().spacing.button_padding = egui::vec2(6.0, 2.0);
            add_contents(ui);
        },
    );
}
