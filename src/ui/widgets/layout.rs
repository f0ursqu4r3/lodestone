//! Layout helpers: section headers, labeled rows, and separators.
#![allow(dead_code)]

use egui::{FontId, RichText, Ui};

use crate::ui::theme::active_theme;

/// Labeled section with a styled header and content callback.
///
/// The header is rendered at 11px in `theme.text_secondary` with strong weight,
/// followed by 4px spacing, the content, then 12px bottom spacing.
pub fn section(ui: &mut Ui, label: &str, content: impl FnOnce(&mut Ui)) {
    let theme = active_theme(ui.ctx());

    ui.label(
        RichText::new(label)
            .font(FontId::proportional(11.0))
            .color(theme.text_secondary)
            .strong(),
    );
    ui.add_space(4.0);
    content(ui);
    ui.add_space(12.0);
}

/// Horizontal label + control row.
///
/// Renders `label` in `theme.text_primary`, then calls `content` for the
/// control(s) to the right.
pub fn labeled_row(ui: &mut Ui, label: &str, content: impl FnOnce(&mut Ui)) {
    let theme = active_theme(ui.ctx());

    ui.horizontal(|ui| {
        ui.label(RichText::new(label).color(theme.text_primary));
        content(ui);
    });
}

/// Themed horizontal separator.
///
/// Draws a 1px line using `theme.border` with 9px of total vertical spacing.
pub fn separator(ui: &mut Ui) {
    let theme = active_theme(ui.ctx());

    ui.add_space(4.0);
    let available = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(available, 1.0), egui::Sense::hover());
    ui.painter()
        .rect_filled(rect, 0.0, theme.border);
    ui.add_space(4.0);
}
