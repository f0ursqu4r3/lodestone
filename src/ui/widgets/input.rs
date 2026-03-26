//! Themed input widgets: text, drag values, and color picker.
#![allow(dead_code)]

use egui::{Color32, RichText, Stroke, TextEdit, Ui};

use crate::ui::theme::active_theme;

// ── Text inputs ──────────────────────────────────────────────────────────────

/// Themed single-line text input. Returns `true` if the value changed.
pub fn text_input(ui: &mut Ui, value: &mut String) -> bool {
    let theme = active_theme(ui.ctx());
    let response = ui.add(
        TextEdit::singleline(value)
            .text_color(theme.text_primary)
            .frame(true),
    );
    // Apply themed background via visuals override is complex; use standard TextEdit
    // with response-based change detection.
    response.changed()
}

/// Themed password input (characters hidden). Returns `true` if the value changed.
pub fn password_input(ui: &mut Ui, value: &mut String) -> bool {
    let theme = active_theme(ui.ctx());
    let response = ui.add(
        TextEdit::singleline(value)
            .text_color(theme.text_primary)
            .password(true)
            .frame(true),
    );
    response.changed()
}

// ── Drag inputs ──────────────────────────────────────────────────────────────

/// Themed drag-value input for `f32` with range and suffix label.
/// Returns `true` if the value changed.
pub fn drag_input(
    ui: &mut Ui,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    suffix: &str,
) -> bool {
    let theme = active_theme(ui.ctx());
    let response = ui.add(
        egui::DragValue::new(value)
            .range(range)
            .suffix(suffix)
            .speed(0.5),
    );
    // Tint label with theme color via painter override is not needed; egui uses
    // the visuals system. Signal change.
    let _ = theme; // theme available for future use
    response.changed()
}

/// Themed drag-value input for `u32` with range and suffix label.
/// Returns `true` if the value changed.
pub fn drag_input_u32(
    ui: &mut Ui,
    value: &mut u32,
    range: std::ops::RangeInclusive<u32>,
    suffix: &str,
) -> bool {
    let theme = active_theme(ui.ctx());
    let response = ui.add(
        egui::DragValue::new(value)
            .range(range)
            .suffix(suffix)
            .speed(1.0),
    );
    let _ = theme;
    response.changed()
}

// ── Color picker ─────────────────────────────────────────────────────────────

/// Hex-input color picker with a 24×24 color swatch and a reset button.
///
/// `hex` is an optional override hex string (e.g. `"#ff8800"`). When `None`,
/// `default_color` is used as the displayed color. Returns `true` if the value
/// changed.
pub fn color_picker(
    ui: &mut Ui,
    hex: &mut Option<String>,
    default_color: Color32,
) -> bool {
    let theme = active_theme(ui.ctx());
    let mut changed = false;

    let effective_color = hex
        .as_deref()
        .and_then(parse_hex)
        .unwrap_or(default_color);

    ui.horizontal(|ui| {
        // Color swatch
        let (rect, _) = ui.allocate_exact_size(egui::vec2(24.0, 24.0), egui::Sense::hover());
        ui.painter().rect_filled(rect, theme.radius_sm, effective_color);
        ui.painter().rect_stroke(
            rect,
            theme.radius_sm,
            Stroke::new(1.0, theme.border),
            egui::StrokeKind::Outside,
        );

        // Hex text input
        let mut text = hex.clone().unwrap_or_else(|| format_hex(default_color));
        let response = ui.add(
            TextEdit::singleline(&mut text)
                .desired_width(80.0)
                .text_color(theme.text_primary)
                .frame(true),
        );
        if response.changed() {
            *hex = Some(text);
            changed = true;
        }

        // Reset button
        if ui
            .add(
                egui::Button::new(
                    RichText::new("Reset").color(theme.text_muted),
                )
                .frame(false),
            )
            .clicked()
        {
            *hex = None;
            changed = true;
        }
    });

    changed
}

// ── Internal helpers ─────────────────────────────────────────────────────────

fn parse_hex(hex: &str) -> Option<Color32> {
    let hex = hex.trim_start_matches('#');
    if hex.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some(Color32::from_rgb(r, g, b))
}

fn format_hex(c: Color32) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b())
}
