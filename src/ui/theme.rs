//! Centralized color tokens for the Pro Neutral theme.
//!
//! All UI colors flow through this module. The accent color is user-configurable;
//! every other token is fixed.

use egui::Color32;
use serde::{Deserialize, Serialize};

// ── ThemeId ──

/// Identifies a built-in theme variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThemeId {
    DefaultDark,
    Light,
    SolarizedDark,
    SolarizedLight,
    RosePine,
    CatppuccinMocha,
    HighContrast,
    Nord,
}

impl ThemeId {
    /// All available theme IDs.
    pub fn all() -> &'static [ThemeId] {
        &[
            ThemeId::DefaultDark,
            ThemeId::Light,
            ThemeId::SolarizedDark,
            ThemeId::SolarizedLight,
            ThemeId::RosePine,
            ThemeId::CatppuccinMocha,
            ThemeId::HighContrast,
            ThemeId::Nord,
        ]
    }

    /// Human-readable name for the theme.
    pub fn label(&self) -> &'static str {
        match self {
            ThemeId::DefaultDark => "Default Dark",
            ThemeId::Light => "Light",
            ThemeId::SolarizedDark => "Solarized Dark",
            ThemeId::SolarizedLight => "Solarized Light",
            ThemeId::RosePine => "Rosé Pine",
            ThemeId::CatppuccinMocha => "Catppuccin Mocha",
            ThemeId::HighContrast => "High Contrast",
            ThemeId::Nord => "Nord",
        }
    }
}

// ── Theme struct ──

/// A complete set of visual tokens for a UI theme.
#[derive(Debug, Clone)]
pub struct Theme {
    pub name: &'static str,
    pub id: ThemeId,
    // Backgrounds
    pub bg_base: Color32,
    pub bg_surface: Color32,
    pub bg_elevated: Color32,
    pub bg_panel: Color32,
    // Text
    pub text_primary: Color32,
    pub text_secondary: Color32,
    pub text_muted: Color32,
    // Borders
    pub border: Color32,
    pub border_subtle: Color32,
    // Accent
    pub accent: Color32,
    pub accent_hover: Color32,
    pub accent_dim: Color32,
    // Semantic
    pub danger: Color32,
    pub success: Color32,
    pub warning: Color32,
    // Toolbar
    pub toolbar_bg: Color32,
    // Scrollbar
    pub scrollbar: Color32,
    pub scrollbar_hover: Color32,
    // Spacing
    pub panel_padding: f32,
    pub item_spacing: f32,
    // Radii
    pub radius_sm: f32,
    pub radius_md: f32,
    pub radius_lg: f32,
    // Sizing
    pub toolbar_height: f32,
    pub tab_bar_height: f32,
}

impl Theme {
    /// Return the built-in theme for the given [`ThemeId`].
    /// All non-`DefaultDark` variants are stubs that return a clone of `DEFAULT_DARK`
    /// until they are fully defined in a later task.
    pub fn builtin(id: ThemeId) -> Theme {
        match id {
            ThemeId::DefaultDark => DEFAULT_DARK.clone(),
            // Stubs — will be filled in Task 2
            ThemeId::Light => {
                let mut t = DEFAULT_DARK.clone();
                t.id = ThemeId::Light;
                t.name = "Light";
                t
            }
            ThemeId::SolarizedDark => {
                let mut t = DEFAULT_DARK.clone();
                t.id = ThemeId::SolarizedDark;
                t.name = "Solarized Dark";
                t
            }
            ThemeId::SolarizedLight => {
                let mut t = DEFAULT_DARK.clone();
                t.id = ThemeId::SolarizedLight;
                t.name = "Solarized Light";
                t
            }
            ThemeId::RosePine => {
                let mut t = DEFAULT_DARK.clone();
                t.id = ThemeId::RosePine;
                t.name = "Rosé Pine";
                t
            }
            ThemeId::CatppuccinMocha => {
                let mut t = DEFAULT_DARK.clone();
                t.id = ThemeId::CatppuccinMocha;
                t.name = "Catppuccin Mocha";
                t
            }
            ThemeId::HighContrast => {
                let mut t = DEFAULT_DARK.clone();
                t.id = ThemeId::HighContrast;
                t.name = "High Contrast";
                t
            }
            ThemeId::Nord => {
                let mut t = DEFAULT_DARK.clone();
                t.id = ThemeId::Nord;
                t.name = "Nord";
                t
            }
        }
    }
}

// ── Default Dark theme ──

/// The default dark theme. Color values match the legacy constants exactly.
pub const DEFAULT_DARK: Theme = Theme {
    name: "Default Dark",
    id: ThemeId::DefaultDark,
    // Backgrounds
    bg_base: Color32::from_rgb(0x11, 0x11, 0x16),
    bg_surface: Color32::from_rgb(0x1a, 0x1a, 0x21),
    bg_elevated: Color32::from_rgb(0x22, 0x22, 0x2c),
    bg_panel: Color32::from_rgb(0x16, 0x16, 0x1c),
    // Text
    text_primary: Color32::from_rgb(0xe0, 0xe0, 0xe8),
    text_secondary: Color32::from_rgb(0x88, 0x88, 0xa0),
    text_muted: Color32::from_rgb(0x55, 0x55, 0x68),
    // Borders
    border: Color32::from_rgb(0x2a, 0x2a, 0x34),
    border_subtle: Color32::from_rgb(0x22, 0x22, 0x30),
    // Accent — same as DEFAULT_ACCENT
    accent: Color32::from_rgb(0xe0, 0xe0, 0xe8),
    accent_hover: Color32::from_rgb(0xe0, 0xe0, 0xe8),
    // accent_dim at ~15% opacity: premultiplied(0xe0*38/255, 0xe0*38/255, 0xe8*38/255, 38)
    accent_dim: Color32::from_rgba_premultiplied(
        (0xe0u16 * 38 / 255) as u8,
        (0xe0u16 * 38 / 255) as u8,
        (0xe8u16 * 38 / 255) as u8,
        38,
    ),
    // Semantic
    danger: Color32::from_rgb(0xe7, 0x4c, 0x3c),
    success: Color32::from_rgb(0x2e, 0xcc, 0x71),
    warning: Color32::from_rgb(0xf1, 0xc4, 0x0f),
    // Toolbar
    toolbar_bg: Color32::from_rgb(0x1a, 0x1a, 0x21),
    // Scrollbar
    scrollbar: Color32::from_rgb(0x55, 0x55, 0x68),
    scrollbar_hover: Color32::from_rgb(0x88, 0x88, 0xa0),
    // Spacing
    panel_padding: 8.0,
    item_spacing: 6.0,
    // Radii
    radius_sm: 4.0,
    radius_md: 6.0,
    radius_lg: 12.0,
    // Sizing
    toolbar_height: 40.0,
    tab_bar_height: 28.0,
};

// ── Active theme accessor ──

/// Read the active [`Theme`] from egui context data.
/// Falls back to [`DEFAULT_DARK`] if none has been stored.
pub fn active_theme(ctx: &egui::Context) -> Theme {
    ctx.data(|d| d.get_temp::<ThemeId>(egui::Id::new("active_theme")))
        .map(Theme::builtin)
        .unwrap_or_else(|| DEFAULT_DARK.clone())
}

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

/// Read the current accent color from egui context data (set per-frame by the render loop).
/// Falls back to `DEFAULT_ACCENT` if not set.
pub fn accent_color(ctx: &egui::Context) -> Color32 {
    ctx.data(|d| d.get_temp(egui::Id::new("accent_color")))
        .unwrap_or(DEFAULT_ACCENT)
}

/// Read the accent color from a `Ui` handle.
pub fn accent_color_ui(ui: &egui::Ui) -> Color32 {
    ui.data(|d| d.get_temp(egui::Id::new("accent_color")))
        .unwrap_or(DEFAULT_ACCENT)
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

    #[test]
    fn all_builtin_themes_exist() {
        for &id in ThemeId::all() {
            let theme = Theme::builtin(id);
            assert!(!theme.name.is_empty());
        }
    }

    #[test]
    fn default_dark_matches_old_constants() {
        let theme = Theme::builtin(ThemeId::DefaultDark);
        assert_eq!(theme.bg_base, Color32::from_rgb(0x11, 0x11, 0x16));
        assert_eq!(theme.text_primary, Color32::from_rgb(0xe0, 0xe0, 0xe8));
        assert_eq!(theme.accent, Color32::from_rgb(0xe0, 0xe0, 0xe8));
        assert_eq!(theme.danger, Color32::from_rgb(0xe7, 0x4c, 0x3c));
        assert_eq!(theme.success, Color32::from_rgb(0x2e, 0xcc, 0x71));
        assert_eq!(theme.radius_sm, 4.0);
        assert_eq!(theme.toolbar_height, 40.0);
    }

    #[test]
    fn theme_id_roundtrip() {
        // TOML requires a top-level table, so wrap the enum in a struct for serialization.
        #[derive(Serialize, Deserialize)]
        struct Wrapper {
            id: ThemeId,
        }
        for &id in ThemeId::all() {
            let w = Wrapper { id };
            let s = toml::to_string(&w).unwrap();
            let parsed: Wrapper = toml::from_str(&s).unwrap();
            assert_eq!(parsed.id, id);
        }
    }
}
