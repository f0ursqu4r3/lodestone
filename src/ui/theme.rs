//! Centralized color tokens for the Pro Neutral theme.
//!
//! All UI colors flow through this module. The accent color is user-configurable;
//! every other token is fixed.

use egui::Color32;

// ── Base Surfaces ──

pub const BG_BASE: Color32 = Color32::from_rgb(0x11, 0x11, 0x16);
pub const BG_SURFACE: Color32 = Color32::from_rgb(0x1a, 0x1a, 0x21);
pub const BG_ELEVATED: Color32 = Color32::from_rgb(0x22, 0x22, 0x2c);
pub const BG_PANEL: Color32 = Color32::from_rgb(0x16, 0x16, 0x1c);

// ── Borders ──

pub const BORDER: Color32 = Color32::from_rgb(0x2a, 0x2a, 0x34);
#[allow(dead_code)]
pub const BORDER_SUBTLE: Color32 = Color32::from_rgb(0x22, 0x22, 0x30);

// ── Text ──

pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(0xe0, 0xe0, 0xe8);
pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(0x88, 0x88, 0xa0);
pub const TEXT_MUTED: Color32 = Color32::from_rgb(0x55, 0x55, 0x68);

// ── Functional Color ──

pub const RED_LIVE: Color32 = Color32::from_rgb(0xe7, 0x4c, 0x3c);
pub const RED_GLOW: Color32 = Color32::from_rgba_premultiplied(0xe7, 0x4c, 0x3c, 0x40);
pub const GREEN_ONLINE: Color32 = Color32::from_rgb(0x2e, 0xcc, 0x71);
#[allow(dead_code)]
pub const YELLOW_WARN: Color32 = Color32::from_rgb(0xf1, 0xc4, 0x0f);

// ── VU Meter ──

pub const VU_GREEN: Color32 = Color32::from_rgb(0x2e, 0xcc, 0x71);
pub const VU_YELLOW: Color32 = Color32::from_rgb(0xf1, 0xc4, 0x0f);
pub const VU_RED: Color32 = Color32::from_rgb(0xe7, 0x4c, 0x3c);

// ── Layout Constants ──

pub const TOOLBAR_HEIGHT: f32 = 40.0;
pub const TAB_BAR_HEIGHT: f32 = 28.0;
pub const PANEL_PADDING: f32 = 8.0;
pub const ADD_BUTTON_WIDTH: f32 = 28.0;
pub const DOCK_GRIP_WIDTH: f32 = 28.0;
pub const FLOATING_HEADER_HEIGHT: f32 = 28.0;
pub const FLOATING_MIN_SIZE: egui::Vec2 = egui::Vec2::new(200.0, 100.0);

// ── Button Padding ──

/// Inner padding for standard buttons (horizontal, vertical).
pub const BTN_PADDING: egui::Vec2 = egui::Vec2::new(10.0, 4.0);
/// Inner padding for pill-shaped buttons (scene switcher).
pub const BTN_PILL_PADDING: egui::Vec2 = egui::Vec2::new(12.0, 4.0);

// ── Border Radii ──

/// Small radius for buttons, inputs, badges, overlays.
pub const RADIUS_SM: f32 = 4.0;
/// Medium radius for cards, thumbnails, panels.
pub const RADIUS_MD: f32 = 6.0;
/// Large radius for pill-shaped elements (scene switcher).
pub const RADIUS_LG: f32 = 12.0;

// ── Accent Color Helpers ──

/// Default accent color (neutral white-gray).
pub const DEFAULT_ACCENT: Color32 = Color32::from_rgb(0xe0, 0xe0, 0xe8);

/// Parse a hex color string like "#e0e0e8" into a Color32.
/// Returns `DEFAULT_ACCENT` on parse failure.
pub fn parse_hex_color(hex: &str) -> Color32 {
    let hex = hex.trim_start_matches('#');
    if hex.len() != 6 {
        return DEFAULT_ACCENT;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(0xe0);
    let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(0xe0);
    let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(0xe8);
    Color32::from_rgb(r, g, b)
}

/// Derive a dim version of the accent color at ~15% opacity for selection backgrounds.
pub fn accent_dim(accent: Color32) -> Color32 {
    Color32::from_rgba_premultiplied(
        (accent.r() as u16 * 38 / 255) as u8,
        (accent.g() as u16 * 38 / 255) as u8,
        (accent.b() as u16 * 38 / 255) as u8,
        38,
    )
}

/// Format a Color32 as a hex string like "#e0e0e8".
pub fn color_to_hex(c: Color32) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b())
}

// ── Shared Menu Helpers ──

/// A single menu item that matches the native context menu look (full-width
/// hover highlight, no button frame). Returns `true` if clicked.
pub fn menu_item(ui: &mut egui::Ui, label: &str) -> bool {
    ui.add(egui::Button::new(label).frame(false)).clicked()
}

/// A menu item with a Phosphor icon prefix.
pub fn menu_item_icon(ui: &mut egui::Ui, icon: &str, label: &str) -> bool {
    let text = format!("{icon}  {label}");
    ui.add(egui::Button::new(text).frame(false)).clicked()
}

/// Render a block of menu items with consistent styling: justified layout,
/// compact padding, minimum width. Use `menu_item()` inside the closure.
pub fn styled_menu(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui)) {
    ui.allocate_ui_with_layout(
        egui::vec2(160.0, 0.0),
        egui::Layout::top_down_justified(egui::Align::LEFT),
        |ui| {
            ui.style_mut().spacing.button_padding = egui::vec2(6.0, 2.0);
            add_contents(ui);
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_valid_hex() {
        let c = parse_hex_color("#ff8800");
        assert_eq!(c, Color32::from_rgb(0xff, 0x88, 0x00));
    }

    #[test]
    fn parse_hex_without_hash() {
        let c = parse_hex_color("ff8800");
        assert_eq!(c, Color32::from_rgb(0xff, 0x88, 0x00));
    }

    #[test]
    fn parse_invalid_hex_returns_default() {
        let c = parse_hex_color("nope");
        assert_eq!(c, DEFAULT_ACCENT);
    }

    #[test]
    fn accent_dim_produces_low_alpha() {
        let dim = accent_dim(Color32::from_rgb(0xff, 0xff, 0xff));
        assert_eq!(dim.a(), 38);
    }

    #[test]
    fn color_to_hex_roundtrip() {
        let hex = color_to_hex(Color32::from_rgb(0xe0, 0xe0, 0xe8));
        assert_eq!(hex, "#e0e0e8");
    }
}
