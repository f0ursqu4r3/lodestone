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
#[allow(dead_code)]
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
            ThemeId::Light => Theme {
                name: "Light",
                id: ThemeId::Light,
                bg_base: Color32::from_rgb(0xf5, 0xf5, 0xf7),
                bg_surface: Color32::from_rgb(0xff, 0xff, 0xff),
                bg_elevated: Color32::from_rgb(0xea, 0xea, 0xed),
                bg_panel: Color32::from_rgb(0xf0, 0xf0, 0xf3),
                text_primary: Color32::from_rgb(0x1a, 0x1a, 0x1e),
                text_secondary: Color32::from_rgb(0x71, 0x71, 0x7a),
                text_muted: Color32::from_rgb(0xa1, 0xa1, 0xaa),
                border: Color32::from_rgb(0xd4, 0xd4, 0xd8),
                border_subtle: Color32::from_rgb(0xe4, 0xe4, 0xe7),
                accent: Color32::from_rgb(0x4f, 0x6a, 0xf0),
                accent_hover: Color32::from_rgb(0x3d, 0x58, 0xd0),
                accent_dim: accent_dim(Color32::from_rgb(0x4f, 0x6a, 0xf0)),
                danger: Color32::from_rgb(0xdc, 0x26, 0x26),
                success: Color32::from_rgb(0x16, 0xa3, 0x4a),
                warning: Color32::from_rgb(0xca, 0x8a, 0x04),
                toolbar_bg: Color32::from_rgb(0xff, 0xff, 0xff),
                scrollbar: Color32::from_rgb(0xc4, 0xc4, 0xcc),
                scrollbar_hover: Color32::from_rgb(0xa1, 0xa1, 0xaa),
                panel_padding: 8.0,
                item_spacing: 6.0,
                radius_sm: 4.0,
                radius_md: 6.0,
                radius_lg: 12.0,
                toolbar_height: 40.0,
                tab_bar_height: 28.0,
            },
            ThemeId::SolarizedDark => Theme {
                name: "Solarized Dark",
                id: ThemeId::SolarizedDark,
                bg_base: Color32::from_rgb(0x00, 0x2b, 0x36),
                bg_surface: Color32::from_rgb(0x07, 0x36, 0x42),
                bg_elevated: Color32::from_rgb(0x0a, 0x40, 0x50),
                bg_panel: Color32::from_rgb(0x00, 0x38, 0x45),
                text_primary: Color32::from_rgb(0x93, 0xa1, 0xa1),
                text_secondary: Color32::from_rgb(0x65, 0x7b, 0x83),
                text_muted: Color32::from_rgb(0x58, 0x6e, 0x75),
                border: Color32::from_rgb(0x0d, 0x4f, 0x5a),
                border_subtle: Color32::from_rgb(0x09, 0x46, 0x52),
                accent: Color32::from_rgb(0x26, 0x8b, 0xd2),
                accent_hover: Color32::from_rgb(0x2e, 0x9e, 0xe6),
                accent_dim: accent_dim(Color32::from_rgb(0x26, 0x8b, 0xd2)),
                danger: Color32::from_rgb(0xdc, 0x32, 0x2f),
                success: Color32::from_rgb(0x85, 0x99, 0x00),
                warning: Color32::from_rgb(0xb5, 0x89, 0x00),
                toolbar_bg: Color32::from_rgb(0x07, 0x36, 0x42),
                scrollbar: Color32::from_rgb(0x58, 0x6e, 0x75),
                scrollbar_hover: Color32::from_rgb(0x65, 0x7b, 0x83),
                panel_padding: 8.0,
                item_spacing: 6.0,
                radius_sm: 4.0,
                radius_md: 6.0,
                radius_lg: 12.0,
                toolbar_height: 40.0,
                tab_bar_height: 28.0,
            },
            ThemeId::SolarizedLight => Theme {
                name: "Solarized Light",
                id: ThemeId::SolarizedLight,
                bg_base: Color32::from_rgb(0xfd, 0xf6, 0xe3),
                bg_surface: Color32::from_rgb(0xee, 0xe8, 0xd5),
                bg_elevated: Color32::from_rgb(0xe4, 0xdd, 0xca),
                bg_panel: Color32::from_rgb(0xf5, 0xee, 0xd9),
                text_primary: Color32::from_rgb(0x58, 0x6e, 0x75),
                text_secondary: Color32::from_rgb(0x65, 0x7b, 0x83),
                text_muted: Color32::from_rgb(0x93, 0xa1, 0xa1),
                border: Color32::from_rgb(0xd6, 0xcd, 0xb5),
                border_subtle: Color32::from_rgb(0xe0, 0xd8, 0xc2),
                accent: Color32::from_rgb(0x26, 0x8b, 0xd2),
                accent_hover: Color32::from_rgb(0x20, 0x76, 0xb5),
                accent_dim: accent_dim(Color32::from_rgb(0x26, 0x8b, 0xd2)),
                danger: Color32::from_rgb(0xdc, 0x32, 0x2f),
                success: Color32::from_rgb(0x85, 0x99, 0x00),
                warning: Color32::from_rgb(0xb5, 0x89, 0x00),
                toolbar_bg: Color32::from_rgb(0xee, 0xe8, 0xd5),
                scrollbar: Color32::from_rgb(0xc5, 0xbd, 0xa5),
                scrollbar_hover: Color32::from_rgb(0x93, 0xa1, 0xa1),
                panel_padding: 8.0,
                item_spacing: 6.0,
                radius_sm: 4.0,
                radius_md: 6.0,
                radius_lg: 12.0,
                toolbar_height: 40.0,
                tab_bar_height: 28.0,
            },
            ThemeId::RosePine => Theme {
                name: "Rosé Pine",
                id: ThemeId::RosePine,
                bg_base: Color32::from_rgb(0x19, 0x17, 0x24),
                bg_surface: Color32::from_rgb(0x1f, 0x1d, 0x2e),
                bg_elevated: Color32::from_rgb(0x26, 0x23, 0x3a),
                bg_panel: Color32::from_rgb(0x1c, 0x1a, 0x2a),
                text_primary: Color32::from_rgb(0xe0, 0xde, 0xf4),
                text_secondary: Color32::from_rgb(0x90, 0x8c, 0xaa),
                text_muted: Color32::from_rgb(0x6e, 0x6a, 0x86),
                border: Color32::from_rgb(0x2a, 0x27, 0x40),
                border_subtle: Color32::from_rgb(0x23, 0x20, 0x38),
                accent: Color32::from_rgb(0xc4, 0xa7, 0xe7),
                accent_hover: Color32::from_rgb(0xd4, 0xbd, 0xf7),
                accent_dim: accent_dim(Color32::from_rgb(0xc4, 0xa7, 0xe7)),
                danger: Color32::from_rgb(0xeb, 0x6f, 0x92),
                success: Color32::from_rgb(0x9c, 0xcf, 0xd8),
                warning: Color32::from_rgb(0xf6, 0xc1, 0x77),
                toolbar_bg: Color32::from_rgb(0x1f, 0x1d, 0x2e),
                scrollbar: Color32::from_rgb(0x6e, 0x6a, 0x86),
                scrollbar_hover: Color32::from_rgb(0x90, 0x8c, 0xaa),
                panel_padding: 8.0,
                item_spacing: 6.0,
                radius_sm: 4.0,
                radius_md: 6.0,
                radius_lg: 12.0,
                toolbar_height: 40.0,
                tab_bar_height: 28.0,
            },
            ThemeId::CatppuccinMocha => Theme {
                name: "Catppuccin Mocha",
                id: ThemeId::CatppuccinMocha,
                bg_base: Color32::from_rgb(0x1e, 0x1e, 0x2e),
                bg_surface: Color32::from_rgb(0x24, 0x24, 0x3b),
                bg_elevated: Color32::from_rgb(0x31, 0x32, 0x44),
                bg_panel: Color32::from_rgb(0x21, 0x21, 0x3a),
                text_primary: Color32::from_rgb(0xcd, 0xd6, 0xf4),
                text_secondary: Color32::from_rgb(0xa6, 0xad, 0xc8),
                text_muted: Color32::from_rgb(0x6c, 0x70, 0x86),
                border: Color32::from_rgb(0x45, 0x47, 0x5a),
                border_subtle: Color32::from_rgb(0x38, 0x3a, 0x4f),
                accent: Color32::from_rgb(0x89, 0xb4, 0xfa),
                accent_hover: Color32::from_rgb(0x9c, 0xc4, 0xff),
                accent_dim: accent_dim(Color32::from_rgb(0x89, 0xb4, 0xfa)),
                danger: Color32::from_rgb(0xf3, 0x8b, 0xa8),
                success: Color32::from_rgb(0xa6, 0xe3, 0xa1),
                warning: Color32::from_rgb(0xf9, 0xe2, 0xaf),
                toolbar_bg: Color32::from_rgb(0x24, 0x24, 0x3b),
                scrollbar: Color32::from_rgb(0x58, 0x5b, 0x70),
                scrollbar_hover: Color32::from_rgb(0x6c, 0x70, 0x86),
                panel_padding: 8.0,
                item_spacing: 6.0,
                radius_sm: 4.0,
                radius_md: 6.0,
                radius_lg: 12.0,
                toolbar_height: 40.0,
                tab_bar_height: 28.0,
            },
            ThemeId::HighContrast => Theme {
                name: "High Contrast",
                id: ThemeId::HighContrast,
                bg_base: Color32::from_rgb(0x00, 0x00, 0x00),
                bg_surface: Color32::from_rgb(0x0a, 0x0a, 0x0a),
                bg_elevated: Color32::from_rgb(0x1a, 0x1a, 0x1a),
                bg_panel: Color32::from_rgb(0x05, 0x05, 0x05),
                text_primary: Color32::from_rgb(0xff, 0xff, 0xff),
                text_secondary: Color32::from_rgb(0xcc, 0xcc, 0xcc),
                text_muted: Color32::from_rgb(0x88, 0x88, 0x88),
                border: Color32::from_rgb(0xff, 0xff, 0xff),
                border_subtle: Color32::from_rgb(0x66, 0x66, 0x66),
                accent: Color32::from_rgb(0xff, 0xff, 0x00),
                accent_hover: Color32::from_rgb(0xff, 0xff, 0x55),
                accent_dim: accent_dim(Color32::from_rgb(0xff, 0xff, 0x00)),
                danger: Color32::from_rgb(0xff, 0x00, 0x00),
                success: Color32::from_rgb(0x00, 0xff, 0x00),
                warning: Color32::from_rgb(0xff, 0xff, 0x00),
                toolbar_bg: Color32::from_rgb(0x0a, 0x0a, 0x0a),
                scrollbar: Color32::from_rgb(0x88, 0x88, 0x88),
                scrollbar_hover: Color32::from_rgb(0xcc, 0xcc, 0xcc),
                panel_padding: 8.0,
                item_spacing: 6.0,
                radius_sm: 4.0,
                radius_md: 6.0,
                radius_lg: 12.0,
                toolbar_height: 40.0,
                tab_bar_height: 28.0,
            },
            ThemeId::Nord => Theme {
                name: "Nord",
                id: ThemeId::Nord,
                bg_base: Color32::from_rgb(0x2e, 0x34, 0x40),
                bg_surface: Color32::from_rgb(0x3b, 0x42, 0x52),
                bg_elevated: Color32::from_rgb(0x43, 0x4c, 0x5e),
                bg_panel: Color32::from_rgb(0x34, 0x3b, 0x49),
                text_primary: Color32::from_rgb(0xd8, 0xde, 0xe9),
                text_secondary: Color32::from_rgb(0x9d, 0xa5, 0xb4),
                text_muted: Color32::from_rgb(0x61, 0x6e, 0x88),
                border: Color32::from_rgb(0x4c, 0x56, 0x6a),
                border_subtle: Color32::from_rgb(0x43, 0x4c, 0x5e),
                accent: Color32::from_rgb(0x88, 0xc0, 0xd0),
                accent_hover: Color32::from_rgb(0x8f, 0xbc, 0xbb),
                accent_dim: accent_dim(Color32::from_rgb(0x88, 0xc0, 0xd0)),
                danger: Color32::from_rgb(0xbf, 0x61, 0x6a),
                success: Color32::from_rgb(0xa3, 0xbe, 0x8c),
                warning: Color32::from_rgb(0xeb, 0xcb, 0x8b),
                toolbar_bg: Color32::from_rgb(0x3b, 0x42, 0x52),
                scrollbar: Color32::from_rgb(0x61, 0x6e, 0x88),
                scrollbar_hover: Color32::from_rgb(0x9d, 0xa5, 0xb4),
                panel_padding: 8.0,
                item_spacing: 6.0,
                radius_sm: 4.0,
                radius_md: 6.0,
                radius_lg: 12.0,
                toolbar_height: 40.0,
                tab_bar_height: 28.0,
            },
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

// ── Layout Constants ──

pub const ADD_BUTTON_WIDTH: f32 = 28.0;
pub const DOCK_GRIP_WIDTH: f32 = 28.0;
pub const FLOATING_HEADER_HEIGHT: f32 = 28.0;
pub const FLOATING_MIN_SIZE: egui::Vec2 = egui::Vec2::new(200.0, 100.0);

// ── Button Padding ──

/// Inner padding for standard buttons (horizontal, vertical).
pub const BTN_PADDING: egui::Vec2 = egui::Vec2::new(10.0, 4.0);
/// Inner padding for pill-shaped buttons (scene switcher).
pub const BTN_PILL_PADDING: egui::Vec2 = egui::Vec2::new(12.0, 4.0);

// ── Accent Color Helpers ──

/// Parse a hex color string like "#e0e0e8" into a Color32.
/// Returns the Default Dark accent on parse failure.
pub fn parse_hex_color(hex: &str) -> Color32 {
    let hex = hex.trim_start_matches('#');
    if hex.len() != 6 {
        return DEFAULT_DARK.accent;
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
        assert_eq!(c, DEFAULT_DARK.accent);
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
    fn all_themes_have_sufficient_text_contrast() {
        for &id in ThemeId::all() {
            let t = Theme::builtin(id);
            let dr = t.text_primary.r() as f64 - t.bg_base.r() as f64;
            let dg = t.text_primary.g() as f64 - t.bg_base.g() as f64;
            let db = t.text_primary.b() as f64 - t.bg_base.b() as f64;
            let diff = (dr * dr + dg * dg + db * db).sqrt();
            assert!(diff > 100.0, "{:?} has insufficient text contrast: {}", id, diff);
        }
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
