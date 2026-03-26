# UI Theming, Widget Library & Appearance Settings Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace hardcoded color constants with a complete theming system, extract a reusable widget library, ship 8 built-in themes, and add an Appearance settings panel.

**Architecture:** A `Theme` struct holds all visual tokens (colors, spacing, radii). Built-in themes are static instances. The active theme is resolved per-frame and stored in egui context data. A `ui/widgets/` module provides themed, reusable components. All UI code reads from the theme instead of constants. Migration is additive — `DEFAULT_DARK` starts with identical values to current constants.

**Tech Stack:** Rust, egui, wgpu, serde/TOML

**Spec:** `docs/superpowers/specs/2026-03-26-ui-theming-design.md`

---

## File Structure

| File | Action | Responsibility |
|------|--------|---------------|
| `src/ui/theme.rs` | Rewrite | `Theme` struct, `ThemeId` enum, 8 built-in themes, `active_theme()` accessor, `parse_hex_color`/`color_to_hex` utils, `accent_dim()` helper. Old constants removed. |
| `src/ui/widgets/mod.rs` | Create | Re-export all widget modules |
| `src/ui/widgets/button.rs` | Create | `StyledButton` with Primary/Danger/Success/Ghost/Toolbar variants |
| `src/ui/widgets/input.rs` | Create | `TextInput`, `DragInput`, `ColorPicker` |
| `src/ui/widgets/dropdown.rs` | Create | Themed `ComboBox` wrapper |
| `src/ui/widgets/toggle.rs` | Create | `Toggle` switch, `ToggleRow` (mutually exclusive options) |
| `src/ui/widgets/layout.rs` | Create | `section`, `labeled_row`, `separator` |
| `src/ui/widgets/menu.rs` | Create | `menu_item`, `menu_item_icon`, `styled_menu` (moved from theme.rs) |
| `src/ui/widgets/composite.rs` | Create | `EncoderDropdown`, `QualityPresets`, `FpsToggles` (moved from stream.rs) |
| `src/ui/mod.rs` | Modify | Add `pub mod widgets;` |
| `src/settings.rs` | Modify | `AppearanceSettings` with `ThemeId`, `Option<String>` accent, font fields |
| `src/state.rs` | Modify | Add `system_fonts: Vec<String>` |
| `src/window.rs` | Modify | Theme resolution per-frame (replaces accent-only sync) |
| `src/ui/settings/appearance.rs` | Rewrite | Theme picker grid, accent color, font size, font family |
| `src/ui/settings/stream.rs` | Refactor | Use widgets, remove pub(super) helpers |
| `src/ui/settings/record.rs` | Refactor | Use widgets |
| `src/ui/settings/video.rs` | Refactor | Use widgets |
| `src/ui/settings/audio.rs` | Refactor | Use widgets |
| `src/ui/settings/mod.rs` | Modify | Remove section_header/labeled_row helpers, update dispatch |
| `src/ui/toolbar.rs` | Refactor | Use `StyledButton`, read from theme |
| `src/ui/layout/` | Modify | Replace constant imports with theme lookups |
| `src/ui/*.rs` (remaining panels) | Modify | Replace constant imports with theme lookups |

---

### Task 1: Theme struct, ThemeId, and Default Dark theme

**Files:**
- Modify: `src/ui/theme.rs` (rewrite)

- [ ] **Step 1: Write tests for Theme and ThemeId**

Add at the bottom of `src/ui/theme.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_builtin_themes_exist() {
        for &id in ThemeId::all() {
            let theme = Theme::builtin(id);
            assert!(!theme.name.is_empty(), "{:?} has no name", id);
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
        for &id in ThemeId::all() {
            let s = toml::to_string(&id).unwrap();
            let parsed: ThemeId = toml::from_str(&s).unwrap();
            assert_eq!(parsed, id);
        }
    }

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
        // Should return a fallback, not crash
        assert_eq!(c, Color32::from_rgb(0xe0, 0xe0, 0xe8));
    }

    #[test]
    fn accent_dim_produces_low_alpha() {
        let dim = accent_dim(Color32::from_rgb(0xff, 0x00, 0x00));
        assert!(dim.a() < 50);
    }

    #[test]
    fn color_to_hex_roundtrip() {
        let c = Color32::from_rgb(0xab, 0xcd, 0xef);
        let hex = color_to_hex(c);
        assert_eq!(hex, "#abcdef");
        let parsed = parse_hex_color(&hex);
        assert_eq!(parsed, c);
    }
}
```

- [ ] **Step 2: Implement Theme struct and ThemeId enum**

Rewrite `src/ui/theme.rs`. Keep the file structure:
1. `Theme` struct with all fields (colors, spacing, radii, sizing)
2. `ThemeId` enum with `Serialize`/`Deserialize`, `all()` method, `label()` method
3. `Theme::builtin(id: ThemeId) -> Theme` — returns the static theme for a given ID
4. `DEFAULT_DARK` theme using exact current constant values
5. Stub other themes (just clone DEFAULT_DARK for now — they'll be filled in Task 2)
6. Preserve utility functions: `parse_hex_color`, `color_to_hex`, `accent_dim`
7. `active_theme(ctx: &egui::Context) -> Theme` — read from context data, fallback to DEFAULT_DARK
8. Keep old constants as `pub const` aliases pointing to DEFAULT_DARK values (for compilation — removed in Task 8)
9. Keep menu helpers temporarily (moved to widgets in Task 5)

The old constants must remain as aliases so the 21 importing files continue to compile. They will be migrated in Tasks 8-10.

```rust
// Temporary aliases — removed after migration
pub const BG_BASE: Color32 = Color32::from_rgb(0x11, 0x11, 0x16);
// ... etc
```

- [ ] **Step 3: Run tests**

Run: `cargo test -q 2>&1`
Expected: all tests pass (old + new)

- [ ] **Step 4: Commit**

```bash
git add src/ui/theme.rs
git commit -m "feat: Theme struct, ThemeId enum, and Default Dark theme"
```

---

### Task 2: Implement all 8 built-in themes

**Files:**
- Modify: `src/ui/theme.rs` (fill in theme definitions)

- [ ] **Step 1: Write theme contrast tests**

```rust
#[test]
fn all_themes_have_sufficient_text_contrast() {
    for &id in ThemeId::all() {
        let t = Theme::builtin(id);
        // Primary text should be visually distinct from base background
        let diff = color_distance(t.text_primary, t.bg_base);
        assert!(diff > 100.0, "{:?} has insufficient text contrast: {}", id, diff);
    }
}

fn color_distance(a: Color32, b: Color32) -> f64 {
    let dr = a.r() as f64 - b.r() as f64;
    let dg = a.g() as f64 - b.g() as f64;
    let db = a.b() as f64 - b.b() as f64;
    (dr * dr + dg * dg + db * db).sqrt()
}
```

- [ ] **Step 2: Fill in all 8 themes**

Replace stubs in `Theme::builtin()` with full color definitions for: Light, SolarizedDark, SolarizedLight, RosePine, CatppuccinMocha, HighContrast, Nord. Use the exact colors from the spec (Section 2).

Each theme must define ALL fields — no `..DEFAULT_DARK` spread. Every color is intentional per theme. Spacing/radii values are the same across all themes.

Semantic colors per theme:
- **Dark themes**: bright danger/success/warning for contrast on dark bg
- **Light themes**: deeper/saturated danger/success/warning for contrast on light bg
- **High Contrast**: pure red (#ff0000), green (#00ff00), yellow (#ffff00)

- [ ] **Step 3: Run tests**

Run: `cargo test -q 2>&1`
Expected: all tests pass, including contrast test

- [ ] **Step 4: Commit**

```bash
git add src/ui/theme.rs
git commit -m "feat: 8 built-in themes (Dark, Light, Solarized, Rose Pine, Catppuccin, Nord, High Contrast)"
```

---

### Task 3: Theme resolution in the render loop

**Files:**
- Modify: `src/settings.rs` (AppearanceSettings)
- Modify: `src/window.rs` (per-frame theme sync)
- Modify: `src/state.rs` (system_fonts field)

- [ ] **Step 1: Update AppearanceSettings**

In `src/settings.rs`, replace the existing `AppearanceSettings`:

```rust
use crate::ui::theme::ThemeId;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppearanceSettings {
    pub theme: ThemeId,
    pub accent_color: Option<String>,
    pub font_size: f32,
    pub font_family: String,
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

- [ ] **Step 2: Update window.rs theme resolution**

Replace the accent-color-only sync with full theme resolution. Find the block that parses `state.settings.appearance.accent_color` and sets `egui::Id::new("accent_color")` in context data. Replace with:

```rust
// Resolve active theme
let mut theme = crate::ui::theme::Theme::builtin(state.settings.appearance.theme);
if let Some(ref hex) = state.settings.appearance.accent_color {
    let accent = crate::ui::theme::parse_hex_color(hex);
    theme.accent = accent;
    theme.accent_hover = accent; // slightly lighter in future
    theme.accent_dim = crate::ui::theme::accent_dim(accent);
}
// Store resolved theme in context
self.egui_ctx.data_mut(|d| {
    d.insert_temp(egui::Id::new("active_theme"), theme.clone());
});
// Also maintain old accent_color key for backward compat during migration
state.accent_color = theme.accent;
self.egui_ctx.data_mut(|d| {
    d.insert_temp(egui::Id::new("accent_color"), theme.accent);
});
// Set egui visuals based on theme brightness
let is_dark = (theme.bg_base.r() as u16 + theme.bg_base.g() as u16 + theme.bg_base.b() as u16) < 384;
if is_dark {
    self.egui_ctx.set_visuals(egui::Visuals::dark());
} else {
    self.egui_ctx.set_visuals(egui::Visuals::light());
}
```

Do the same in the settings window render path.

- [ ] **Step 3: Add system_fonts to AppState**

In `src/state.rs`, add:
```rust
pub system_fonts: Vec<String>,
```
Initialize to `vec!["Default".to_string()]`. Font enumeration will be added in the appearance settings task.

- [ ] **Step 4: Fix compilation — update settings references**

Any code that references `state.settings.appearance.accent_color` as a `String` (non-optional) needs updating to handle `Option<String>`. Key locations:
- `settings/mod.rs` where it parses the accent color
- `settings/appearance.rs` where it reads/writes the hex value

- [ ] **Step 5: Run tests**

Run: `cargo build 2>&1 && cargo test -q 2>&1`
Expected: compiles and all tests pass

- [ ] **Step 6: Commit**

```bash
git add src/settings.rs src/window.rs src/state.rs src/ui/settings/mod.rs src/ui/settings/appearance.rs
git commit -m "feat: per-frame theme resolution with ThemeId-based settings"
```

---

### Task 4: Widget library — layout and input widgets

**Files:**
- Create: `src/ui/widgets/mod.rs`
- Create: `src/ui/widgets/layout.rs`
- Create: `src/ui/widgets/button.rs`
- Create: `src/ui/widgets/input.rs`
- Create: `src/ui/widgets/dropdown.rs`
- Create: `src/ui/widgets/toggle.rs`
- Modify: `src/ui/mod.rs` (add `pub mod widgets;`)

- [ ] **Step 1: Create widgets/mod.rs**

```rust
pub mod button;
pub mod dropdown;
pub mod input;
pub mod layout;
pub mod toggle;
```

- [ ] **Step 2: Create widgets/layout.rs**

```rust
use egui::{RichText, Ui};
use crate::ui::theme::active_theme;

/// Labeled section with header and content.
pub fn section(ui: &mut Ui, label: &str, content: impl FnOnce(&mut Ui)) {
    let theme = active_theme(ui.ctx());
    ui.label(RichText::new(label).color(theme.text_secondary).size(11.0).strong());
    ui.add_space(4.0);
    content(ui);
    ui.add_space(12.0);
}

/// Horizontal label + control row.
pub fn labeled_row(ui: &mut Ui, label: &str, content: impl FnOnce(&mut Ui)) {
    let theme = active_theme(ui.ctx());
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).color(theme.text_primary));
        content(ui);
    });
}

/// Themed horizontal separator.
pub fn separator(ui: &mut Ui) {
    let theme = active_theme(ui.ctx());
    let rect = ui.available_rect_before_wrap();
    let y = rect.top() + 4.0;
    ui.painter().line_segment(
        [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
        egui::Stroke::new(1.0, theme.border),
    );
    ui.add_space(9.0);
}
```

- [ ] **Step 3: Create widgets/button.rs**

Implement `StyledButton` with `ButtonVariant` enum (Primary, Danger, Success, Ghost, Toolbar). Each variant reads colors from the active theme. The widget implements `egui::Widget` for use with `ui.add()`.

```rust
use egui::{self, Color32, Response, RichText, Ui, Vec2, Widget};
use crate::ui::theme::active_theme;

#[derive(Debug, Clone, Copy)]
pub enum ButtonVariant {
    Primary,
    Danger,
    Success,
    Ghost,
    Toolbar,
}

pub struct StyledButton {
    text: String,
    variant: ButtonVariant,
    min_size: Option<Vec2>,
}

impl StyledButton {
    pub fn new(text: impl Into<String>, variant: ButtonVariant) -> Self {
        Self { text: text.into(), variant, min_size: None }
    }

    pub fn min_size(mut self, size: Vec2) -> Self {
        self.min_size = Some(size);
        self
    }
}

impl Widget for StyledButton {
    fn ui(self, ui: &mut Ui) -> Response {
        let theme = active_theme(ui.ctx());
        let (fill, text_color, stroke_color) = match self.variant {
            ButtonVariant::Primary => (theme.accent, Color32::WHITE, theme.accent),
            ButtonVariant::Danger => (theme.danger, Color32::WHITE, theme.danger),
            ButtonVariant::Success => (theme.success, Color32::WHITE, theme.success),
            ButtonVariant::Ghost => (Color32::TRANSPARENT, theme.text_secondary, theme.border),
            ButtonVariant::Toolbar => (Color32::TRANSPARENT, theme.text_muted, Color32::TRANSPARENT),
        };

        let mut btn = egui::Button::new(
            RichText::new(&self.text).color(text_color).size(11.0).strong()
        )
        .fill(fill)
        .stroke(egui::Stroke::new(1.0, stroke_color))
        .corner_radius(theme.radius_sm);

        if let Some(size) = self.min_size {
            btn = btn.min_size(size);
        }

        ui.add(btn)
    }
}
```

- [ ] **Step 4: Create widgets/input.rs**

Implement `TextInput` (regular and password), `DragInput` (with suffix), and `ColorPicker` (hex + swatch).

- [ ] **Step 5: Create widgets/dropdown.rs**

Themed `ComboBox` wrapper.

- [ ] **Step 6: Create widgets/toggle.rs**

`ToggleRow` — horizontal row of mutually exclusive options. Reads from theme for selected/unselected colors. Also move `toggle_switch` from `settings/mod.rs`.

- [ ] **Step 7: Add widgets module to ui/mod.rs**

Add `pub mod widgets;` after `pub mod theme;`.

- [ ] **Step 8: Run tests and build**

Run: `cargo build 2>&1 && cargo test -q 2>&1`
Expected: compiles and all tests pass

- [ ] **Step 9: Commit**

```bash
git add src/ui/widgets/ src/ui/mod.rs
git commit -m "feat: widget library with themed button, input, dropdown, toggle, layout components"
```

---

### Task 5: Widget library — menu helpers and composite widgets

**Files:**
- Create: `src/ui/widgets/menu.rs`
- Create: `src/ui/widgets/composite.rs`
- Modify: `src/ui/widgets/mod.rs` (add modules)

- [ ] **Step 1: Create widgets/menu.rs**

Move `menu_item`, `menu_item_icon`, `styled_menu` from `theme.rs`. Update to read colors from active theme.

- [ ] **Step 2: Create widgets/composite.rs**

Move `draw_encoder_dropdown`, `draw_quality_presets`, `draw_fps_toggles` from `settings/stream.rs`. Update to use `ToggleRow` and `Dropdown` from widgets. Make them `pub` functions.

- [ ] **Step 3: Update widgets/mod.rs**

Add `pub mod menu;` and `pub mod composite;`.

- [ ] **Step 4: Run tests and build**

Run: `cargo build 2>&1 && cargo test -q 2>&1`

- [ ] **Step 5: Commit**

```bash
git add src/ui/widgets/
git commit -m "feat: menu helpers and composite widgets (encoder, quality, fps)"
```

---

### Task 6: Refactor settings panels to use widgets

**Files:**
- Modify: `src/ui/settings/stream.rs`
- Modify: `src/ui/settings/record.rs`
- Modify: `src/ui/settings/video.rs`
- Modify: `src/ui/settings/audio.rs`
- Modify: `src/ui/settings/mod.rs` (remove old helpers)

- [ ] **Step 1: Refactor stream.rs**

Replace hand-rolled UI patterns with widget calls. Remove the `pub(super)` helper functions (now in `widgets/composite.rs`). Use `section()`, `labeled_row()`, `ToggleRow`, `Dropdown`, etc.

- [ ] **Step 2: Refactor record.rs**

Same pattern. Replace `super::stream::draw_encoder_dropdown` calls with `crate::ui::widgets::composite::encoder_dropdown`.

- [ ] **Step 3: Refactor video.rs**

Replace `section_header` and `labeled_row` calls with widget versions.

- [ ] **Step 4: Refactor audio.rs**

Replace helper calls with widget versions.

- [ ] **Step 5: Remove old helpers from settings/mod.rs**

Remove `section_header`, `labeled_row`, `labeled_row_unimplemented`, `draw_toggle_unimplemented`, `draw_toggle`, `toggle_switch`. These are all replaced by widgets.

- [ ] **Step 6: Run tests and build**

Run: `cargo build 2>&1 && cargo test -q 2>&1`

- [ ] **Step 7: Commit**

```bash
git add src/ui/settings/
git commit -m "refactor: settings panels use widget library"
```

---

### Task 7: Refactor toolbar to use StyledButton and theme

**Files:**
- Modify: `src/ui/toolbar.rs`

- [ ] **Step 1: Replace hardcoded colors with theme lookups**

Replace all `RED_LIVE`, `GREEN_ONLINE`, inline color literals with `theme.danger`, `theme.success`, etc. Read theme via `active_theme(ui.ctx())` at the top of each draw function.

- [ ] **Step 2: Use StyledButton for Go Live, Record, V-Cam**

Replace hand-built `egui::Button::new(...)` patterns with `StyledButton::new("Go Live", ButtonVariant::Danger)` etc.

- [ ] **Step 3: Run tests and build**

Run: `cargo build 2>&1 && cargo test -q 2>&1`

- [ ] **Step 4: Commit**

```bash
git add src/ui/toolbar.rs
git commit -m "refactor: toolbar uses StyledButton and theme tokens"
```

---

### Task 8: Migrate remaining UI panels to theme lookups

**Files:**
- Modify: `src/ui/sources_panel.rs`
- Modify: `src/ui/scenes_panel.rs`
- Modify: `src/ui/library_panel.rs`
- Modify: `src/ui/properties_panel.rs`
- Modify: `src/ui/preview_panel.rs`
- Modify: `src/ui/audio_mixer.rs`
- Modify: `src/ui/stream_controls.rs`
- Modify: `src/ui/draw_helpers.rs`
- Modify: `src/ui/transform_handles.rs`
- Modify: `src/ui/settings/hotkeys.rs`
- Modify: `src/ui/settings/general.rs`
- Modify: `src/ui/settings/advanced.rs`

- [ ] **Step 1: For each file, replace constant imports with theme lookups**

Pattern: replace `use crate::ui::theme::{BG_BASE, TEXT_PRIMARY, ...};` with reading from `active_theme(ctx)` or `active_theme(ui.ctx())` at the function entry point. Store in a local `let theme = ...` and use `theme.bg_base`, `theme.text_primary`, etc.

Also replace `accent_color(ctx)` / `accent_color_ui(ui)` calls with `theme.accent`.

Include `settings/hotkeys.rs`, `settings/general.rs`, `settings/advanced.rs` — these have minimal theme usage (just `TEXT_MUTED` etc.) but still need constant-to-theme migration to compile after Task 9 removes the constants.

Replace `RED_GLOW` with a computed value: `Color32::from_rgba_premultiplied(theme.danger.r(), theme.danger.g(), theme.danger.b(), 0x40)`.

Replace `VU_GREEN`/`VU_YELLOW`/`VU_RED` with `theme.success`/`theme.warning`/`theme.danger`.

- [ ] **Step 2: Migrate layout module**

Modify `src/ui/layout/render.rs`, `render_tabs.rs`, `render_grid.rs`, `render_floating.rs` — same pattern: replace constant imports with theme lookups.

- [ ] **Step 3: Run tests and build**

Run: `cargo build 2>&1 && cargo test -q 2>&1`

- [ ] **Step 4: Commit**

```bash
git add src/ui/
git commit -m "refactor: all UI panels use theme lookups instead of constants"
```

---

### Task 9: Remove old constants and menu helpers from theme.rs

**Files:**
- Modify: `src/ui/theme.rs`

- [ ] **Step 1: Remove all old constant aliases**

Remove all `pub const BG_BASE`, `pub const TEXT_PRIMARY`, etc. Also remove `pub const DEFAULT_ACCENT`, `TOOLBAR_HEIGHT`, `TAB_BAR_HEIGHT`, `PANEL_PADDING`, `RADIUS_SM/MD/LG`, `RED_LIVE`, `RED_GLOW`, `GREEN_ONLINE`, `YELLOW_WARN`, `VU_GREEN/YELLOW/RED`, `BORDER`, `BORDER_SUBTLE`.

Keep as standalone constants (NOT in Theme): `ADD_BUTTON_WIDTH`, `DOCK_GRIP_WIDTH`, `FLOATING_HEADER_HEIGHT`, `FLOATING_MIN_SIZE`, `BTN_PADDING`, `BTN_PILL_PADDING` — these are layout-mechanical.

- [ ] **Step 2: Remove old menu helpers**

Remove `menu_item`, `menu_item_icon`, `styled_menu` from theme.rs (now in `widgets/menu.rs`). Remove `accent_color`, `accent_color_ui` functions (replaced by `active_theme(ctx).accent`).

- [ ] **Step 3: Update any remaining imports**

Search for `use crate::ui::theme::` across the entire codebase. Fix any remaining references to removed items.

- [ ] **Step 4: Run tests and build**

Run: `cargo build 2>&1 && cargo test -q 2>&1`
Expected: compiles cleanly — all consumers now use theme lookups or widgets

- [ ] **Step 5: Commit**

```bash
git add src/ui/theme.rs src/ui/
git commit -m "chore: remove old color constants and menu helpers from theme.rs"
```

---

### Task 10: Appearance settings UI

**Files:**
- Rewrite: `src/ui/settings/appearance.rs`

- [ ] **Step 1: Implement theme picker grid**

2-column grid of theme cards. Each card shows:
- Theme name (`theme.name`)
- 5-swatch color strip (bg_base, bg_surface, bg_elevated, text_primary, accent)
- Border with `theme.border`; selected theme gets `theme.accent` border

Click a card → `settings.appearance.theme = id`.

```rust
let current_theme = active_theme(ui.ctx());
section(ui, "Theme", |ui| {
    let columns = 2;
    egui::Grid::new("theme_grid").num_columns(columns).spacing([8.0, 8.0]).show(ui, |ui| {
        for (i, &id) in ThemeId::all().iter().enumerate() {
            let t = Theme::builtin(id);
            let is_selected = state.settings.appearance.theme == id;
            let stroke = if is_selected {
                egui::Stroke::new(2.0, current_theme.accent)
            } else {
                egui::Stroke::new(1.0, current_theme.border)
            };

            let (rect, response) = ui.allocate_exact_size(
                egui::vec2(ui.available_width(), 48.0),
                egui::Sense::click(),
            );

            if response.clicked() {
                state.settings.appearance.theme = id;
                changed = true;
            }

            // Draw card background, swatches, and name
            ui.painter().rect_filled(rect, current_theme.radius_md, current_theme.bg_elevated);
            ui.painter().rect_stroke(rect, current_theme.radius_md, stroke);
            // ... draw 5 swatches and theme name text

            if (i + 1) % columns == 0 {
                ui.end_row();
            }
        }
    });
});
```

- [ ] **Step 2: Implement accent color override**

```rust
section(ui, "Accent Color", |ui| {
    // ColorPicker widget for accent
    // Reset button that sets accent_color = None
});
```

- [ ] **Step 3: Implement font controls**

```rust
section(ui, "Font", |ui| {
    // Font size DragInput (8.0-24.0 px)
    // Font family Dropdown from state.system_fonts
});
```

- [ ] **Step 4: Run tests and build**

Run: `cargo build 2>&1 && cargo test -q 2>&1`

- [ ] **Step 5: Commit**

```bash
git add src/ui/settings/appearance.rs
git commit -m "feat: appearance settings with theme picker, accent color, font controls"
```

---

### Task 11: Font enumeration and application

**Files:**
- Modify: `src/main.rs` (enumerate system fonts at startup)
- Modify: `src/window.rs` (apply font size/family per-frame)

- [ ] **Step 1: Enumerate system fonts at startup**

In `AppManager::new()`, query available font families. On macOS, use `core_text` or a simpler approach — provide a curated list: `["Default", "SF Pro", "Helvetica Neue", "Menlo", "Monaco"]`. Store in `AppState.system_fonts`.

- [ ] **Step 2: Apply font size per-frame**

In the theme resolution block in `window.rs`, apply font size:

```rust
let font_size = state.settings.appearance.font_size;
let mut style = (*self.egui_ctx.style()).clone();
if let Some(body) = style.text_styles.get_mut(&egui::TextStyle::Body) {
    body.size = font_size;
}
if let Some(button) = style.text_styles.get_mut(&egui::TextStyle::Button) {
    button.size = font_size;
}
self.egui_ctx.set_style(style);
```

- [ ] **Step 3: Run tests and build**

Run: `cargo build 2>&1 && cargo test -q 2>&1`

- [ ] **Step 4: Commit**

```bash
git add src/main.rs src/window.rs
git commit -m "feat: font enumeration and per-frame font size application"
```

---

### Task 12: Final cleanup and verification

**Files:**
- Various

- [ ] **Step 1: Run clippy**

Run: `cargo clippy 2>&1`
Fix any warnings.

- [ ] **Step 2: Run full test suite**

Run: `cargo test -q 2>&1`
Expected: all tests pass

- [ ] **Step 3: Manual smoke test**

Run: `RUST_LOG=info cargo run`

Verify:
1. App launches with Default Dark theme (looks identical to before)
2. Settings → Appearance: theme picker shows 8 themes with color swatches
3. Click each theme — UI updates immediately
4. Light themes switch egui to light visuals
5. Accent color override works
6. Font size slider works
7. Record and Stream buttons still function
8. All settings panels use consistent widget styling

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "chore: final cleanup after UI theming implementation"
```
