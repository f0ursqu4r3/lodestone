# UI Theming, Widget Library & Appearance Settings — Design Spec

**Date:** 2026-03-26
**Status:** Draft
**Scope:** Theme system, reusable widget library, appearance settings UI, code consolidation

## Overview

Replace Lodestone's hardcoded color constants with a complete theming system. Extract repeated UI patterns into a reusable widget library. Ship 8 built-in themes and an Appearance settings panel for theme selection, accent color customization, and font controls. All existing functionality is preserved — the migration is additive, swapping constant references for theme struct lookups with identical initial values.

## 1. Theme Data Model

### 1.1 Theme Struct

```rust
/// A complete visual theme for the application.
#[derive(Debug, Clone)]
pub struct Theme {
    pub name: &'static str,
    pub id: ThemeId,

    // Backgrounds (layered surfaces)
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

    // Accent (overridable per-user)
    pub accent: Color32,
    pub accent_hover: Color32,
    pub accent_dim: Color32,

    // Semantic
    pub danger: Color32,
    pub success: Color32,
    pub warning: Color32,

    // Toolbar (defaults to bg_surface, separate token allows themes to differentiate)
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
```

### 1.2 ThemeId Enum

```rust
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
```

`ThemeId` is serializable for settings persistence. Each variant maps to a `const fn` or `static` returning the full `Theme`.

### 1.3 Theme Resolution

Each frame, the active theme is resolved and stored in egui's context data:

1. Load the built-in theme by `ThemeId` from `AppSettings.appearance.theme`
2. If `AppSettings.appearance.accent_color` is set, override `accent`, `accent_hover`, and `accent_dim` on the loaded theme
3. Store the resolved `Theme` in `ctx.data_mut()` using a `TypeId` key
4. Set egui's `Visuals` (dark or light) based on whether the theme's `bg_base` luminance is below or above a threshold

UI code accesses the theme via:

```rust
pub fn active_theme(ctx: &egui::Context) -> Theme
```

This replaces all direct references to color constants.

### 1.4 Spacing and Radii

All built-in themes use identical spacing/radii values (matching current constants):

- `panel_padding`: 8.0
- `item_spacing`: 6.0
- `radius_sm`: 4.0, `radius_md`: 6.0, `radius_lg`: 12.0
- `toolbar_height`: 40.0, `tab_bar_height`: 28.0

These are included in the `Theme` struct for future flexibility (e.g. a "Compact" theme) but all 8 shipped themes share the same values.

### 1.5 Constants That Stay as Constants

The following values are layout-mechanical, not visual — they do NOT go in the `Theme` struct and remain as standalone constants:

- `ADD_BUTTON_WIDTH` (28.0) — dockview internal sizing
- `DOCK_GRIP_WIDTH` (28.0) — dockview grip handle
- `FLOATING_HEADER_HEIGHT` (28.0) — floating window chrome
- `FLOATING_MIN_SIZE` (200x100) — minimum floating window dimensions
- `BTN_PADDING` (10x4) — standard button inner padding
- `BTN_PILL_PADDING` (12x4) — pill button inner padding

These are used by the dockview layout engine and button sizing logic. They don't vary per theme.

### 1.6 VU Meter and Glow Colors

VU meter colors (`VU_GREEN`, `VU_YELLOW`, `VU_RED`) and `RED_GLOW` (premultiplied alpha for pulse animation) are derived from the theme's semantic colors at resolution time:

- `vu_green` = `theme.success`
- `vu_yellow` = `theme.warning`
- `vu_red` = `theme.danger`
- `danger_glow` = `theme.danger` at 25% alpha (computed helper, not a stored field)

These don't need separate fields — they reuse the semantic colors. The `accent_dim()` helper pattern is used for computing glow/dim variants.

### 1.7 Scrollbar Colors

`scrollbar` and `scrollbar_hover` are new tokens with no existing call sites. They are forward-looking — used when the widget library styles egui's scrollbar via `Visuals`. No migration of existing code is needed for these fields.

### 1.8 Accent Color Handling

The current `DEFAULT_ACCENT` is `#e0e0e8` (neutral gray). The Default Dark theme will use this same value as its `accent` field to maintain zero visual change at migration time. Other themes define their own accent colors (e.g. Catppuccin uses `#89b4fa`). The user's accent override in `AppearanceSettings.accent_color` is `Option<String>` (hex). The existing `parse_hex_color()` and `color_to_hex()` utility functions are preserved and moved into the theme module.

## 2. Built-in Themes

### 2.1 Default Dark

Current colors, unchanged. `bg_base: #111116`, `text_primary: #e0e0e8`, `accent: #e0e0e8` (neutral gray, matching current `DEFAULT_ACCENT`). The baseline — what users see today. Zero visual change from pre-migration.

### 2.2 Light

`bg_base: #f5f5f7`, `bg_surface: #ffffff`, `text_primary: #1a1a1e`, `accent: #4f6af0`. Clean warm-white surfaces with high contrast text.

### 2.3 Solarized Dark

Ethan Schoonover's palette. `bg_base: #002b36`, `bg_surface: #073642`, `text_primary: #93a1a1`, `accent: #268bd2`.

### 2.4 Solarized Light

`bg_base: #fdf6e3`, `bg_surface: #eee8d5`, `text_primary: #586e75`, `accent: #268bd2`. Cream base, shared accent with Solarized Dark.

### 2.5 Rosé Pine

`bg_base: #191724`, `bg_surface: #1f1d2e`, `text_primary: #e0def4`, `accent: #c4a7e7`. Soft muted purples.

### 2.6 Catppuccin Mocha

`bg_base: #1e1e2e`, `bg_surface: #24243b`, `text_primary: #cdd6f4`, `accent: #89b4fa`. Warm pastels on dark base.

### 2.7 High Contrast

`bg_base: #000000`, `bg_surface: #0a0a0a`, `text_primary: #ffffff`, `accent: #ffff00`. Maximum contrast, yellow accent. Accessibility-focused.

### 2.8 Nord

`bg_base: #2e3440`, `bg_surface: #3b4252`, `text_primary: #d8dee9`, `accent: #88c0d0`. Arctic blue palette.

### 2.9 Semantic Colors Per Theme

Each theme defines its own `danger`, `success`, `warning` colors tuned for contrast against that theme's backgrounds. Dark themes use brighter variants; light themes use deeper variants. High Contrast uses pure red/green/yellow.

## 3. Widget Library

### 3.1 Module Structure

```
src/ui/widgets/
  mod.rs          — re-exports all widgets
  button.rs       — StyledButton with variants
  input.rs        — TextInput, DragInput, ColorPicker
  dropdown.rs     — themed ComboBox wrapper
  toggle.rs       — Toggle switch, ToggleRow
  layout.rs       — Section, LabeledRow, Separator
  composite.rs    — EncoderDropdown, QualityPresets, FpsToggles
```

### 3.2 Layout Widgets

**Section** — labeled section with header and consistent spacing:
```rust
pub fn section(ui: &mut Ui, label: &str, content: impl FnOnce(&mut Ui)) {
    let theme = active_theme(ui.ctx());
    ui.label(RichText::new(label).color(theme.text_secondary).size(11.0).strong());
    ui.add_space(4.0);
    content(ui);
    ui.add_space(12.0);
}
```

**LabeledRow** — horizontal label + control with consistent alignment:
```rust
pub fn labeled_row(ui: &mut Ui, label: &str, content: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        let theme = active_theme(ui.ctx());
        ui.label(RichText::new(label).color(theme.text_primary));
        content(ui);
    });
}
```

**Separator** — themed horizontal rule using `theme.border`.

### 3.3 Input Widgets

**StyledButton** — four variants:

```rust
pub enum ButtonVariant {
    Primary,   // accent fill, white text
    Danger,    // danger fill (red), white text
    Success,   // success fill (green), white text — V-Cam button
    Ghost,     // transparent, text_secondary, border on hover
    Toolbar,   // compact, no border, text_muted, hover shows bg_elevated
}
```

Each variant reads fill, text, hover, and active colors from the theme. Used by toolbar buttons, settings controls, and dialog actions.

- `Primary` uses `theme.accent` fill
- `Danger` uses `theme.danger` fill (Go Live, Record buttons)
- `Success` uses `theme.success` fill (V-Cam button — currently hardcoded `#22AA55`)
- `Ghost` uses transparent fill, `theme.text_secondary` text
- `Toolbar` uses `theme.bg_elevated` on hover, no fill at rest

The toolbar's inline color constants (`vcam_color`, `rec_color`) are replaced by the `Success` and `Danger` variants respectively.

**TextInput** — themed single-line input with `bg_elevated` background, `border` outline, `text_primary` text. Password variant masks with bullet characters.

**DragInput** — themed `DragValue` wrapper. Accepts range and suffix string (e.g. "kbps", "px").

**ColorPicker** — hex text input + 24x24 color swatch. Currently lives in `appearance.rs`, extracted into widgets.

**Dropdown** — themed `ComboBox` wrapper with `bg_elevated` background, `border` outline, consistent width.

**Toggle** — on/off switch. Off: `bg_elevated` track, `border` knob. On: `accent` track, white knob.

**ToggleRow** — horizontal row of mutually exclusive options (replaces `selectable_label` patterns). Used for FPS, quality presets, format picker. Each option is a pill-shaped button; selected shows `accent_dim` background with `accent` text.

### 3.4 Composite Widgets

These combine primitives for domain-specific controls. Currently hand-rolled in `stream.rs` as `pub(super)` functions — promoted to proper widgets.

**EncoderDropdown** — `Dropdown` that lists `AvailableEncoder` entries, appends "— Recommended" to the recommended one.

**QualityPresets** — `ToggleRow` for Low/Medium/High/Custom + conditional `DragInput` for custom bitrate.

**FpsToggles** — `ToggleRow` for 24/30/60.

### 3.5 Menu Helpers

The current `theme.rs` contains `menu_item()`, `menu_item_icon()`, and `styled_menu()` — context menu helpers used across the UI. These move to `ui/widgets/menu.rs` and are updated to read colors from the active theme.

```
src/ui/widgets/menu.rs  — menu_item, menu_item_icon, styled_menu
```

## 4. Appearance Settings

### 4.1 Settings Data

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppearanceSettings {
    pub theme: ThemeId,
    pub accent_color: Option<String>,  // hex, None = use theme default
    pub font_size: f32,
    pub font_family: String,           // "Default" or system font name
}

impl Default for AppearanceSettings {
    fn default() -> Self {
        Self {
            theme: ThemeId::DefaultDark,
            accent_color: None,
            font_size: 13.0,
            font_family: "Default".to_string(),
        }
    }
}
```

### 4.2 Settings UI Layout

1. **Theme** — 2-column grid of theme cards. Each card: theme name, 5-swatch color strip (bg_base → bg_surface → bg_elevated → text_primary → accent), subtle `border` outline, `accent` outline when selected. Click to apply immediately.

2. **Accent Color** — `ColorPicker` widget. "Reset" button clears the override (reverts to theme default). Shown only when an override is set or user clicks to customize.

3. **Font Size** — `DragInput` with "px" suffix, range 8.0–24.0.

4. **Font Family** — `Dropdown` listing "Default" plus available system fonts. On macOS, include: SF Pro, Helvetica Neue, Menlo. Font list queried once at startup.

### 4.3 Immediate Application

Changes apply on the current frame — no "Apply" or "Save" button. The settings struct is mutated directly, theme resolution picks up the change next frame, all widgets re-render with new values.

### 4.4 TOML Schema Change

The existing `AppearanceSettings` has `theme: String` (value `"dark"`) and `accent_color: String` (non-optional). The new struct changes `theme` to `ThemeId` (enum) and `accent_color` to `Option<String>`. This is a breaking TOML change — existing `settings.toml` files with `theme = "dark"` will fail to parse as `ThemeId::DefaultDark`. Since the app is pre-release, this is acceptable. Old settings files can be deleted. No migration logic needed.

## 5. Code Consolidation

### 5.1 Settings Panels Refactored

Settings panels that contain color references or hand-rolled controls are refactored to use widgets from `ui/widgets/`. The panels become thin orchestration layers — declaring what controls exist and binding them to settings fields.

**Panels to refactor:** `stream.rs`, `record.rs`, `video.rs`, `appearance.rs`, `audio.rs`.

**Panels intentionally excluded:** `general.rs`, `hotkeys.rs`, `advanced.rs` — these have minimal UI (mostly text labels or placeholder content). They get their colors migrated from constants to theme tokens but don't need widget extraction. Refactoring them would be YAGNI.

Example — current `stream.rs` has hand-rolled dropdown, toggle, and label patterns. After refactoring:

```rust
pub fn draw(ui: &mut Ui, settings: &mut StreamSettings, encoders: &[AvailableEncoder]) -> bool {
    let mut changed = false;

    section(ui, "Destination", |ui| {
        // Destination dropdown, stream key input, RTMP URL
        // ... using Dropdown, TextInput widgets
    });

    section(ui, "Encoder", |ui| {
        changed |= encoder_dropdown(ui, &mut settings.encoder, encoders);
    });

    section(ui, "Quality", |ui| {
        changed |= quality_presets(ui, &mut settings.quality_preset, &mut settings.bitrate_kbps);
    });

    section(ui, "Frame Rate", |ui| {
        changed |= fps_toggles(ui, &mut settings.fps);
    });

    changed
}
```

### 5.2 Toolbar Refactored

Toolbar buttons (`Go Live`, `Record`, `V-Cam`) use `StyledButton` with `Danger` and `Toolbar` variants instead of hand-building `egui::Button` with inline colors.

### 5.3 Panel Chrome

Tab bars, panel headers, and dockview dividers read from theme tokens. The dockview rendering code (`layout/` module) uses `theme.bg_panel`, `theme.border`, `theme.tab_bar_height` instead of hardcoded values.

## 6. File Changes Summary

| File | Action | Purpose |
|------|--------|---------|
| `src/ui/theme.rs` | Rewrite | `Theme` struct, `ThemeId`, built-in theme definitions, `active_theme()` accessor, remove old constants |
| `src/ui/widgets/mod.rs` | Create | Re-export all widgets |
| `src/ui/widgets/button.rs` | Create | `StyledButton` with variants |
| `src/ui/widgets/input.rs` | Create | `TextInput`, `DragInput`, `ColorPicker` |
| `src/ui/widgets/dropdown.rs` | Create | Themed `ComboBox` wrapper |
| `src/ui/widgets/toggle.rs` | Create | `Toggle`, `ToggleRow` |
| `src/ui/widgets/layout.rs` | Create | `Section`, `LabeledRow`, `Separator` |
| `src/ui/widgets/composite.rs` | Create | `EncoderDropdown`, `QualityPresets`, `FpsToggles` |
| `src/ui/widgets/menu.rs` | Create | `menu_item`, `menu_item_icon`, `styled_menu` (moved from theme.rs) |
| `src/ui/settings/appearance.rs` | Rewrite | Theme picker grid, accent color, font controls |
| `src/ui/settings/stream.rs` | Refactor | Use widgets, remove hand-rolled helpers |
| `src/ui/settings/record.rs` | Refactor | Use widgets |
| `src/ui/settings/video.rs` | Refactor | Use widgets |
| `src/ui/settings/mod.rs` | Modify | Pass theme to panels, update dispatch, remove `section_header`/`labeled_row` helpers (replaced by widget library) |
| `src/ui/toolbar.rs` | Refactor | Use `StyledButton` variants |
| `src/ui/layout/` | Modify | Read colors/sizes from theme |
| `src/settings.rs` | Modify | `AppearanceSettings` struct with `ThemeId` |
| `src/main.rs` | Modify | Theme resolution in frame loop, font enumeration |
| `src/state.rs` | Modify | Store system font list |

## 7. Testing

- **Theme struct**: unit tests verifying all 8 themes have valid colors (no transparent where solid expected, text contrasts against backgrounds)
- **ThemeId roundtrip**: serialize/deserialize each `ThemeId` to TOML and back
- **active_theme**: test that theme resolution applies accent override correctly
- **Widget rendering**: no unit tests for visual output (egui doesn't support headless rendering well) — rely on manual verification
- **Settings roundtrip**: `AppearanceSettings` serialize/deserialize
- **Existing tests**: all 167 existing tests continue to pass — the refactoring changes how colors are sourced, not what they are

## 8. Migration Strategy

1. `Theme` struct and built-in themes land first. `DEFAULT_DARK` uses exact same values as current constants. Zero visual change.
2. `active_theme()` helper added. Old constants remain temporarily as aliases.
3. Widget library built using theme tokens. Settings panels refactored one at a time.
4. Toolbar and panel chrome migrated.
5. Old constants in `theme.rs` removed.
6. Appearance settings UI added last — the reward for all the plumbing.

Each step is independently deployable and testable. At no point does the app look different unless a user actively switches themes.
