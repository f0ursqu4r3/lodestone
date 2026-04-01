# UI Polish Pass — Design Spec

**Date:** 2026-04-01
**Scope:** 7 targeted improvements across toolbar, scenes, sources, properties, settings, and toggle widgets.
**Principle:** All accent-colored elements use the resolved `theme.accent` / `theme.accent_dim` / `theme.accent_hover` — never hardcoded color values. The user's selected accent color must be respected everywhere.

---

## 1. Settings window title bug

**File:** `src/main.rs` (line ~2179)

The settings window is initialized with `DockLayout::new_single(PanelType::Preview)` as a dummy layout. The tab bar renders `PanelType::Preview`'s display name ("Preview") in the title area. The actual OS window title is already "Settings" — the bug is the visible tab/panel header inside the window.

**Fix:** The settings window's `render_settings()` method doesn't use the dockview layout for content dispatch — it renders settings directly. The fix is to either:
- (a) Hide the tab bar entirely for the settings window (it has no tabs to switch), or
- (b) Change the dummy panel type to something that won't display, or add a `PanelType::Settings` variant.

Option (a) is simpler and correct — the settings window has its own sidebar navigation and doesn't need dockview chrome.

---

## 2. Toolbar action buttons — subdued inactive state

**File:** `src/ui/toolbar.rs`
**Functions:** `draw_go_live_button`, `draw_record_button`, `draw_virtual_camera_button`

### Current behavior
Inactive buttons use `Color32::TRANSPARENT` fill with a colored stroke (`theme.danger` for Go Live/Record, `theme.success` for V-Cam) and colored text. This draws significant visual attention even when nothing is active.

### New behavior

**Inactive state:**
- Fill: `theme.bg_elevated`
- Stroke: `Stroke::new(1.0, theme.border)`
- Text color: `theme.text_secondary`
- Hover: stroke brightens to `theme.text_muted`

**Active state:** Unchanged — solid colored fill (`theme.danger` / `theme.success`) with white text. The active state already works well and provides strong contrast against the now-subdued inactive state.

**Rationale:** Inactive controls shouldn't compete for attention with the preview canvas. Color appears only when state changes (live/recording/vcam active), making the transition more meaningful.

---

## 3. Scene cards — elevation and active indication

**File:** `src/ui/scenes_panel.rs`

### Current behavior
Scene card thumbnails have no border and blend into the dark panel background. The active (PGM) scene uses a small red pill badge that's easy to miss. Inactive card labels use `text_primary` — same weight as active.

### New behavior

**All cards:**
- Add `Stroke::new(1.0, theme.border_subtle)` border to every thumbnail rect.
- Card corner radius remains `theme.radius_sm` (4px).

**Active/PGM card:**
- Border changes to `Stroke::new(2.0, theme.danger)`.
- Label text uses `theme.text_primary` + strong/bold weight.
- PGM badge padding increases from `(1px, 4px)` to `(2px, 6px)` for readability.

**Inactive cards:**
- Label text uses `theme.text_secondary` (dimmer than active).

**"Add" card:**
- Gets a solid `border_subtle` border instead of the current dashed style, with `bg_elevated` fill on hover.

---

## 4. Sources panel — row backgrounds, hover, and selection

**File:** `src/ui/sources_panel.rs`

### Current behavior
Source rows are plain text + icon with no background, no hover feedback, and no visible selection state differentiation. Visibility and lock icons are present but lack visual weight.

### New behavior

**Row backgrounds:**
- Default: transparent (no fill).
- Hover: `theme.bg_elevated` fill, `theme.radius_sm` corners.
- Selected: `theme.accent_dim` fill, `theme.radius_sm` corners.

**Row content:**
- Selected row text: `theme.accent` color.
- Unselected row text: `theme.text_primary`.
- Visibility (eye) and lock icons: always visible at row end in `theme.text_muted`. When toggled (hidden/locked), icon uses `theme.text_secondary` with a strikethrough variant or dimmed opacity.

**Row padding:** Consistent `6px` vertical, `8px` horizontal.

**Rationale:** Mirrors the existing pattern used in `library_panel.rs` for selected items, creating consistency across the two source-browsing panels.

---

## 5. Toggle switches — consistent OFF state and dependent disabling

**File:** `src/ui/widgets/toggle.rs` (the `toggle_switch` function), `src/ui/settings/mod.rs` (the settings-specific `toggle_switch` widget)

### Current behavior
OFF state uses `theme.bg_elevated` fill with a `text_muted` border stroke and a bright white knob. The white-on-dark contrast makes "off" toggles visually prominent. Grid-dependent controls (snap to grid, grid preset) remain enabled even when the parent "Show grid overlay" toggle is off.

### New behavior

**OFF state:**
- Track fill: `theme.border_subtle` (slightly lighter than `bg_elevated`, no outline stroke).
- Remove the `Stroke::new(1.0, theme.text_muted)` border entirely — the lighter fill provides enough definition.
- Knob color: `theme.text_muted` instead of `Color32::WHITE` — softer, clearly "off".

**ON state:** Unchanged — `theme.accent` fill, white knob.

**Dependent disabling (settings/general.rs):**
When "Show grid overlay" is OFF:
- "Snap to grid" toggle renders disabled (grayed out, non-interactive).
- "Grid preset" dropdown renders disabled.
- "Grid size" slider (if visible) renders disabled.

Implementation: wrap these controls in `ui.add_enabled_ui(settings.show_grid, ...)`.

### Both toggle_switch implementations

There are two implementations: `src/ui/widgets/toggle.rs::toggle_switch()` and `src/ui/settings/mod.rs::toggle_switch()`. Both must be updated to match. Ideally, deduplicate — the settings version should call the widget version.

---

## 6. Settings section headers — more visual presence

**File:** `src/ui/settings/mod.rs`, function `section_header`

### Current behavior
Section headers (STARTUP, BEHAVIOR, CAPTURE, etc.) use 11px `text_muted` bold text with 12px top margin and 4px bottom margin. They're functional but easy to overlook.

### New behavior
- Color: `theme.text_secondary` (brighter than `text_muted`).
- Add a 1px `theme.border_subtle` horizontal rule below the header text.
- Top margin: increase from 12px to 16px (stronger visual separation between sections).
- Bottom margin: increase from 4px to 6px (more breathing room before first row).
- Letter spacing: if egui supports it, add slight tracking. If not, skip — not critical.

**First section exception:** The first `section_header` call on a page should use the existing 12px top margin (not 16px) to avoid excessive space at the top. This can be handled by checking if the cursor is near the top of the scroll area, or by using a separate `section_header_first` variant.

---

## 7. Properties panel alignment grid

**File:** `src/ui/properties_panel.rs`

### Current behavior
Transform fields (X, Y, W, H, Rotation), Opacity, and Source controls are laid out with inconsistent left margins and no shared column grid. Labels and inputs don't align vertically across rows.

### New behavior

**Layout grid for transform section:**
All transform rows use a consistent 4-column pattern:
```
[label 20px] [input flex] [label 20px] [input flex]
```

- **Row 1:** `X [input]  Y [input]`
- **Row 2:** `W [input]  H [input]` (with lock/reset icons inline after H input)
- **Row 3:** `  [rotation input]     Rotation` (label in cell 4, input in cell 2)

Implementation approach: use `ui.columns()` or manual `ui.allocate_ui_with_layout()` to enforce fixed-width label columns (20px) with flex input columns.

**Opacity section:**
- Slider spans full content width minus the percentage label.
- Percentage label right-aligned, fixed width (~36px).
- Remove the checkbox if it's the small square shown in screenshots — opacity is already controlled by the slider.

**Source section:**
- "SOURCE" header left-aligns to the same margin as "TRANSFORM" and "OPACITY".
- Dropdown spans full content width.

**Section spacing:**
- 12px vertical gap between sections (TRANSFORM → OPACITY → SOURCE).
- Section headers use the same style as settings section headers (item 6) for consistency — uppercase, `text_secondary`, with subtle underline.

---

## Files changed (summary)

| File | Changes |
|------|---------|
| `src/main.rs` | Fix settings window — hide dockview tab bar |
| `src/ui/toolbar.rs` | Subdued inactive button states |
| `src/ui/scenes_panel.rs` | Card borders, active indication, label hierarchy |
| `src/ui/sources_panel.rs` | Row backgrounds, hover, selection states |
| `src/ui/widgets/toggle.rs` | OFF state track/knob colors, remove border |
| `src/ui/settings/mod.rs` | Section header styling, toggle dedup, toggle_switch OFF state |
| `src/ui/settings/general.rs` | Dependent control disabling (grid controls) |
| `src/ui/properties_panel.rs` | Alignment grid for transform/opacity/source |
