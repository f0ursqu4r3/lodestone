# UI Polish Pass Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix 7 visual polish items — settings title bug, toolbar button states, scene card elevation, source panel row styling, toggle switch consistency, settings section headers, and properties panel alignment.

**Architecture:** All changes are UI-only — no state model, serialization, or GStreamer changes. Each task touches 1-2 files and can be verified visually by running the app. All accent-colored elements must use `theme.accent` / `theme.accent_dim` / `theme.accent_hover` from the resolved theme, never hardcoded color values.

**Tech Stack:** Rust, egui (immediate-mode GUI), wgpu (GPU rendering), Phosphor Icons

---

### Task 1: Fix settings window title showing "Preview"

**Files:**
- Modify: `src/main.rs:1547-1558`

- [ ] **Step 1: Add settings window exclusion to title update loop**

In `src/main.rs`, the title-update block at line 1547 runs for all non-main windows. Add a check to skip the settings window:

```rust
// Update detached window title to match the active panel name
if Some(window_id) != self.main_window_id
    && Some(window_id) != self.settings_window_id  // <-- add this line
    && let Some(win) = self.windows.get(&window_id)
{
    let title = win
        .layout
        .groups
        .values()
        .next()
        .map(|g| g.active_tab_entry().panel_type.display_name())
        .unwrap_or("Lodestone");
    win.window.set_title(title);
}
```

- [ ] **Step 2: Verify**

Run: `cargo run`
Expected: Settings window title bar shows "Settings" instead of "Preview".

- [ ] **Step 3: Commit**

```bash
git add src/main.rs
git commit -m "fix: settings window title showing 'Preview' instead of 'Settings'"
```

---

### Task 2: Subdue toolbar action buttons when inactive

**Files:**
- Modify: `src/ui/toolbar.rs:191-316` (three button functions)

- [ ] **Step 1: Update `draw_go_live_button` inactive state**

In `src/ui/toolbar.rs`, change the inactive branch of `draw_go_live_button` (line ~204-206):

```rust
// Before:
("Go Live", Color32::TRANSPARENT, theme.danger)

// After:
("Go Live", theme.bg_elevated, theme.text_secondary)
```

And change the stroke (line ~210) to use `theme.border` when inactive:

```rust
// Before:
.stroke(egui::Stroke::new(1.0, theme.danger))

// After:
let stroke_color = if is_live { theme.danger } else { theme.border };
// ... then:
.stroke(egui::Stroke::new(1.0, stroke_color))
```

Full replacement for the button construction (lines 208-212):

```rust
let stroke_color = if is_live { theme.danger } else { theme.border };
let btn = egui::Button::new(RichText::new(label).size(11.0).strong().color(text_color))
    .fill(fill)
    .stroke(egui::Stroke::new(1.0, stroke_color))
    .corner_radius(theme.radius_sm)
    .min_size(Vec2::new(64.0, 26.0));
```

- [ ] **Step 2: Update `draw_record_button` inactive state**

In `draw_record_button` (line ~294-298), change the inactive branch:

```rust
// Before:
(Color32::TRANSPARENT, theme.danger)

// After:
(theme.bg_elevated, theme.text_secondary)
```

And update the stroke the same way:

```rust
let stroke_color = if is_recording { theme.danger } else { theme.border };
let btn = egui::Button::new(RichText::new(&label).size(11.0).strong().color(text_color))
    .fill(fill)
    .stroke(egui::Stroke::new(1.0, stroke_color))
    .corner_radius(theme.radius_sm)
    .min_size(Vec2::new(64.0, 26.0));
```

- [ ] **Step 3: Update `draw_virtual_camera_button` inactive state**

In `draw_virtual_camera_button` (line ~250-253), change the inactive branch:

```rust
// Before:
(format!("{icon} V-Cam"), Color32::TRANSPARENT, theme.success)

// After:
(format!("{icon} V-Cam"), theme.bg_elevated, theme.text_secondary)
```

And update the stroke:

```rust
let stroke_color = if is_active { theme.success } else { theme.border };
let btn = egui::Button::new(RichText::new(label).size(11.0).strong().color(text_color))
    .fill(fill)
    .stroke(egui::Stroke::new(1.0, stroke_color))
    .corner_radius(theme.radius_sm)
    .min_size(Vec2::new(64.0, 26.0));
```

- [ ] **Step 4: Verify**

Run: `cargo run`
Expected: Toolbar buttons (Go Live, Record, V-Cam) appear neutral/subdued when inactive — gray text on dark elevated background. When activated, they pop with their semantic colors (red for live/recording, green for V-Cam).

- [ ] **Step 5: Commit**

```bash
git add src/ui/toolbar.rs
git commit -m "style: subdue toolbar action buttons when inactive"
```

---

### Task 3: Improve scene card borders and active indication

**Files:**
- Modify: `src/ui/scenes_panel.rs:219-232` (card borders), `src/ui/scenes_panel.rs:276-322` (PGM badge), `src/ui/scenes_panel.rs:374-385` (label colors), `src/ui/scenes_panel.rs:727-767` (add card)

- [ ] **Step 1: Update scene card border for active/PGM state**

In `draw_scene_card`, lines 219-232, change the border logic. Currently active uses `text_primary` with 1px stroke. Change to use `theme.danger` with 2px for the PGM (program) scene:

```rust
// Border: program = danger 2px, active-only = text_primary 1px,
// hovered = text_muted, default = border_subtle.
let is_program = state.program_scene_id == Some(scene_id);
let (border_color, border_width) = if is_program {
    (theme.danger, 2.0)
} else if is_active {
    (theme.text_primary, 1.0)
} else if is_hovered {
    (theme.text_muted, 1.0)
} else {
    (theme.border_subtle, 1.0)
};
painter.rect_stroke(
    thumb_rect,
    CornerRadius::same(theme.radius_sm as u8),
    Stroke::new(border_width, border_color),
    egui::StrokeKind::Outside,
);
```

Note: this replaces the current `border_color` block AND the `painter.rect_stroke` call. The `is_active` check that uses `theme.border` for the default case now uses `theme.border_subtle`, ensuring all cards always have a visible border (not just active/hovered).

- [ ] **Step 2: Increase PGM badge padding**

In the PGM/PRV badge section (lines 292-322), increase badge padding:

```rust
// Before:
let badge_w = text_w + 6.0;
let badge_h = 11.0;

// After:
let badge_w = text_w + 10.0;
let badge_h = 13.0;
```

- [ ] **Step 3: Make inactive card labels dimmer**

The label code (lines 374-385) already uses `theme.text_secondary` for inactive cards. Verify this is correct — no change needed if already implemented. If the active label is not bold, make it bold:

```rust
let label_color = if is_active {
    theme.text_primary
} else {
    theme.text_secondary
};
```

This is already correct per the current code. No change needed here.

- [ ] **Step 4: Replace dashed "Add" card with solid border**

In `draw_add_card` (lines 727-767), replace the dashed border drawing loop with a simple `rect_stroke`:

```rust
fn draw_add_card(
    painter: &egui::Painter,
    thumb_rect: Rect,
    label_pos: Pos2,
    hovered: bool,
    theme: &crate::ui::theme::Theme,
) {
    let border_color = if hovered {
        theme.text_muted
    } else {
        theme.border_subtle
    };
    let fill = if hovered {
        theme.bg_elevated
    } else {
        egui::Color32::TRANSPARENT
    };

    // Solid border + hover fill (replaces dashed border).
    painter.rect_filled(
        thumb_rect,
        CornerRadius::same(theme.radius_sm as u8),
        fill,
    );
    painter.rect_stroke(
        thumb_rect,
        CornerRadius::same(theme.radius_sm as u8),
        Stroke::new(1.0, border_color),
        egui::StrokeKind::Outside,
    );

    // "+" icon in center of thumbnail.
    let icon_color = if hovered {
        theme.text_muted
    } else {
        theme.border
    };
    painter.text(
        thumb_rect.center(),
        egui::Align2::CENTER_CENTER,
        egui_phosphor::regular::PLUS,
        egui::FontId::proportional(20.0),
        icon_color,
    );

    // "Add" label below thumbnail.
    painter.text(
        label_pos,
        egui::Align2::CENTER_CENTER,
        "Add",
        egui::FontId::proportional(9.0),
        theme.text_muted,
    );
}
```

- [ ] **Step 5: Verify**

Run: `cargo run`
Expected:
- All scene cards have a subtle border (not floating in the void).
- PGM scene has a 2px red border. Non-PGM active scene has 1px `text_primary` border.
- PGM badge is slightly larger and easier to read.
- "Add" card has solid border with hover fill, no dashes.

- [ ] **Step 6: Commit**

```bash
git add src/ui/scenes_panel.rs
git commit -m "style: improve scene card borders, active indication, and add card"
```

---

### Task 4: Add hover and selection states to source rows

**Files:**
- Modify: `src/ui/sources_panel.rs:433-620` (draw_source_row function)

- [ ] **Step 1: Add hover background to source rows**

In `draw_source_row`, after the selection highlight block (lines 474-477), add a hover background:

```rust
// Selection highlight.
if is_selected && !is_being_dragged {
    draw_selection_highlight(ui.painter(), paint_rect, selected_bg);
} else if row_response.hovered() && !is_being_dragged {
    // Hover highlight.
    ui.painter().rect_filled(
        paint_rect,
        CornerRadius::same(theme.radius_sm as u8),
        theme.bg_elevated,
    );
}
```

- [ ] **Step 2: Color selected row text with accent**

In the name painting (lines 543-550), change the text color based on selection:

```rust
// Before:
with_opacity(theme.text_primary, effective_opacity),

// After — for both icon (line 539) and name (line 549):
let text_color = if is_selected { theme.accent } else { theme.text_primary };
// ... then use:
with_opacity(text_color, effective_opacity),
```

Apply this to both the icon text color (line 539) and the name text color (line 549). You'll need to compute `text_color` once before the icon block:

```rust
// Compute row text color based on selection state.
let row_text_color = if is_selected { theme.accent } else { theme.text_primary };
```

Then use `with_opacity(row_text_color, effective_opacity)` for both the icon (line 539) and name (line 549).

- [ ] **Step 3: Make eye icon always visible**

Currently (lines 567-590), the eye icon only shows when the source is hidden OR hovered. Change to always show it:

```rust
// Eye icon — always visible.
let eye_rect =
    Rect::from_center_size(egui::pos2(right_x - 8.0, center_y), vec2(16.0, row_height));
let eye_hovered = ui.rect_contains_pointer(eye_rect);
let eye_text = if row.visible {
    egui_phosphor::regular::EYE
} else {
    egui_phosphor::regular::EYE_SLASH
};
let eye_color = if eye_hovered {
    with_opacity(theme.text_primary, effective_opacity)
} else if !row.visible {
    with_opacity(theme.text_secondary, effective_opacity)
} else {
    with_opacity(theme.text_muted, effective_opacity)
};
painter.text(
    eye_rect.center(),
    egui::Align2::CENTER_CENTER,
    eye_text,
    egui::FontId::proportional(11.0),
    eye_color,
);
```

Remove the `if !row.visible || eye_hovered {` guard — the icon is always rendered now.

- [ ] **Step 4: Verify**

Run: `cargo run`
Expected:
- Source rows show `bg_elevated` background on hover.
- Selected source row has `accent_dim` background and `accent` text.
- Eye icon is always visible (dimmed when visible, brighter when hidden or hovered).

- [ ] **Step 5: Commit**

```bash
git add src/ui/sources_panel.rs
git commit -m "style: add hover/selection states and always-visible icons to source rows"
```

---

### Task 5: Fix toggle switch OFF state and deduplicate implementations

**Files:**
- Modify: `src/ui/widgets/toggle.rs:86-130`
- Modify: `src/ui/settings/mod.rs:329-365` (settings toggle_switch)
- Modify: `src/ui/settings/general.rs:49-96` (dependent disabling)

- [ ] **Step 1: Update OFF state in `src/ui/widgets/toggle.rs`**

Change the toggle_switch function (lines 97-127):

```rust
if ui.is_rect_visible(rect) {
    let anim_id = response.id.with("toggle_anim");
    let t = ui.ctx().animate_bool_with_time(anim_id, *on, 0.15);

    let bg_color = if *on { theme.accent } else { theme.border_subtle };

    let knob_radius = 7.0;
    let knob_x = egui::lerp(
        rect.left() + knob_radius + 3.0..=rect.right() - knob_radius - 3.0,
        t,
    );
    let knob_center = egui::pos2(knob_x, rect.center().y);

    // Track background
    ui.painter()
        .rect_filled(rect, CornerRadius::same(10), bg_color);

    // No border stroke for either state — track fill provides enough definition.

    // Knob: white when on, muted when off
    let knob_color = if *on { Color32::WHITE } else { theme.text_muted };
    ui.painter()
        .circle_filled(knob_center, knob_radius, knob_color);
}
```

Key changes:
- OFF track: `theme.border_subtle` instead of `theme.bg_elevated`
- Removed the `if !*on` border stroke entirely
- OFF knob: `theme.text_muted` instead of `Color32::WHITE`

- [ ] **Step 2: Replace settings toggle_switch with widget version**

In `src/ui/settings/mod.rs`, find the `toggle_switch` function (line ~329) and replace it to delegate to the widget version:

```rust
pub(super) fn toggle_switch(on: &mut bool) -> impl Widget + '_ {
    move |ui: &mut Ui| -> Response {
        // Delegate to the canonical widget implementation.
        let changed = crate::ui::widgets::toggle::toggle_switch(ui, on);
        // Return a dummy response — the draw_toggle caller checks changed via this response.
        let (_, response) = ui.allocate_exact_size(Vec2::ZERO, Sense::hover());
        if changed {
            response
        } else {
            response
        }
    }
}
```

Actually, this won't work cleanly because `toggle_switch` in widgets returns a `bool` while the settings version returns `impl Widget`. The simplest approach: update the settings version's rendering code to match the widget version exactly (same colors, same logic), keeping both as separate implementations but visually identical.

Replace the rendering block in the settings `toggle_switch` (the `if ui.is_rect_visible(rect)` block) with the same code from Step 1:

```rust
if ui.is_rect_visible(rect) {
    let theme = active_theme(ui.ctx());
    let anim_id = response.id.with("toggle_anim");
    let t = ui.ctx().animate_bool_with_time(anim_id, *on, 0.15);

    let bg_color = if *on { theme.accent } else { theme.border_subtle };

    let knob_radius = 7.0;
    let knob_x = egui::lerp(
        rect.left() + knob_radius + 3.0..=rect.right() - knob_radius - 3.0,
        t,
    );
    let knob_center = egui::pos2(knob_x, rect.center().y);

    // Track
    ui.painter()
        .rect_filled(rect, CornerRadius::same(10), bg_color);

    // Knob
    let knob_color = if *on { Color32::WHITE } else { theme.text_muted };
    ui.painter()
        .circle_filled(knob_center, knob_radius, knob_color);
}
```

- [ ] **Step 3: Add dependent disabling for grid controls**

In `src/ui/settings/general.rs`, wrap the grid-dependent controls with `ui.add_enabled_ui(settings.show_grid, ...)`. Replace lines 50-96:

```rust
changed |= draw_toggle(ui, "Show grid overlay", &mut settings.show_grid);

// Grid-dependent controls: disabled when grid overlay is off.
ui.add_enabled_ui(settings.show_grid, |ui| {
    changed |= draw_toggle(ui, "Snap to grid", &mut settings.snap_to_grid);

    // Grid preset combo
    ui.horizontal(|ui| {
        labeled_row(ui, "Grid preset");
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            const PRESETS: &[&str] = &["8", "16", "32", "64", "thirds", "quarters", "custom"];
            let display = if settings.grid_preset.is_empty() {
                "custom"
            } else {
                settings.grid_preset.as_str()
            };
            let combo = egui::ComboBox::from_id_salt("grid_preset_combo")
                .selected_text(display)
                .show_ui(ui, |ui| {
                    let mut c = false;
                    for &preset in PRESETS {
                        c |= ui
                            .selectable_value(&mut settings.grid_preset, preset.to_string(), preset)
                            .changed();
                    }
                    c
                });
            if let Some(inner) = combo.inner {
                changed |= inner;
            }
        });
    });

    // Grid size slider — only shown when preset is "custom" or empty
    let show_grid_size = settings.grid_preset.is_empty() || settings.grid_preset == "custom";
    if show_grid_size {
        ui.horizontal(|ui| {
            labeled_row(ui, "Grid size (px)");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let slider = ui.add(
                    egui::Slider::new(&mut settings.snap_grid_size, 1.0..=200.0)
                        .clamping(egui::SliderClamping::Always),
                );
                if slider.changed() {
                    changed = true;
                }
            });
        });
    }
});

changed |= draw_toggle(ui, "Rule of thirds", &mut settings.show_thirds);
changed |= draw_toggle(ui, "Safe zones", &mut settings.show_safe_zones);
```

Note: "Rule of thirds" and "Safe zones" stay outside the `add_enabled_ui` block because they're independent overlay features, not dependent on the grid.

- [ ] **Step 4: Verify**

Run: `cargo run`
Expected:
- Toggle switches have a softer OFF state (muted knob, subtle track fill, no border).
- ON state unchanged (accent fill, white knob).
- Both settings window and main window toggles look identical.
- When "Show grid overlay" is OFF, "Snap to grid", "Grid preset", and "Grid size" are visually grayed out and non-interactive.

- [ ] **Step 5: Commit**

```bash
git add src/ui/widgets/toggle.rs src/ui/settings/mod.rs src/ui/settings/general.rs
git commit -m "style: consistent toggle OFF state, disable grid controls when overlay off"
```

---

### Task 6: Improve settings section headers

**Files:**
- Modify: `src/ui/settings/mod.rs:254-264` (section_header function)

- [ ] **Step 1: Update `section_header` styling**

Replace the `section_header` function:

```rust
pub(super) fn section_header(ui: &mut Ui, label: &str) {
    let theme = active_theme(ui.ctx());

    // Use larger top margin for separation, but check if we're near the top
    // of the scroll area to avoid excessive space on the first header.
    let cursor_y = ui.cursor().top();
    let min_y = ui.min_rect().top();
    let is_first = (cursor_y - min_y) < 20.0;
    let top_space = if is_first { 4.0 } else { 16.0 };

    ui.add_space(top_space);
    ui.label(
        egui::RichText::new(label)
            .size(11.0)
            .color(theme.text_secondary)
            .strong(),
    );
    // Subtle separator line below header text.
    let rect = ui.cursor();
    let line_y = rect.top() + 2.0;
    let left = ui.min_rect().left();
    let right = ui.max_rect().right();
    ui.painter().line_segment(
        [egui::pos2(left, line_y), egui::pos2(right, line_y)],
        egui::Stroke::new(1.0, theme.border_subtle),
    );
    ui.add_space(8.0);
}
```

Key changes:
- Color from `text_muted` to `text_secondary` (brighter).
- Top margin: 16px (up from 12px) except for first header (4px).
- Added 1px `border_subtle` underline below header text.
- Bottom margin: 8px (up from 4px) for more breathing room.

- [ ] **Step 2: Verify**

Run: `cargo run`, open Settings window.
Expected: Section headers (STARTUP, BEHAVIOR, CAPTURE, etc.) are more visible — brighter text with a subtle underline. More vertical space separates sections. First section doesn't have excessive top padding.

- [ ] **Step 3: Commit**

```bash
git add src/ui/settings/mod.rs
git commit -m "style: improve settings section header visibility with underline and spacing"
```

---

### Task 7: Align properties panel layout grid

**Files:**
- Modify: `src/ui/properties_panel.rs:123-310` (draw_transform_section), `src/ui/properties_panel.rs:343-428` (draw_opacity_section), `src/ui/properties_panel.rs:1691-1694` (section_label)

- [ ] **Step 1: Update `section_label` to match settings style**

In `src/ui/properties_panel.rs`, update the `section_label` function (line ~1691):

```rust
fn section_label(ui: &mut egui::Ui, text: &str) {
    let theme = active_theme(ui.ctx());
    ui.label(
        egui::RichText::new(text)
            .color(theme.text_secondary)
            .size(10.0)
            .strong(),
    );
}
```

Changes: `text_muted` → `text_secondary`, size 9 → 10, keeps `strong()`.

- [ ] **Step 2: Refactor transform X/Y row to use consistent column widths**

In `draw_transform_section`, replace the X/Y row layout (scene override branch, lines 172-177). Instead of a plain `ui.horizontal`, use fixed-width columns:

```rust
// X / Y row — fixed label width for alignment.
ui.horizontal(|ui| {
    let label_w = 16.0;
    ui.allocate_ui_with_layout(
        egui::vec2(label_w, ui.available_height()),
        egui::Layout::right_to_left(egui::Align::Center),
        |ui| {
            ui.label(egui::RichText::new("X").color(text_color).size(10.0));
        },
    );
    let field_w = (ui.available_width() - label_w - 12.0) / 2.0;
    ui.add_sized(
        [field_w, 20.0],
        egui::DragValue::new(&mut transform.x)
            .speed(1.0)
            .update_while_editing(false),
    );
    ui.allocate_ui_with_layout(
        egui::vec2(label_w, ui.available_height()),
        egui::Layout::right_to_left(egui::Align::Center),
        |ui| {
            ui.label(egui::RichText::new("Y").color(text_color).size(10.0));
        },
    );
    ui.add_sized(
        [field_w, 20.0],
        egui::DragValue::new(&mut transform.y)
            .speed(1.0)
            .update_while_editing(false),
    );
    transform_changed |= true; // Check individually below
});
```

Actually, this approach is fragile with egui's layout. A simpler approach: use `ui.columns(4, ...)` which egui provides for equal-width column layouts. But we need unequal widths (narrow label, wide input).

**Better approach:** Create a helper function `transform_row_2` that takes two label/value pairs and renders them in a consistent grid:

```rust
/// Render a row with two labeled drag fields in a [label][input][label][input] grid.
/// Returns true if either value changed.
fn transform_row_2(
    ui: &mut egui::Ui,
    label_a: &str,
    val_a: &mut f32,
    label_b: &str,
    val_b: &mut f32,
    label_color: egui::Color32,
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        let label_w = 16.0;
        let spacing = 8.0;
        let total_labels = label_w * 2.0 + spacing;
        let field_w = ((ui.available_width() - total_labels) / 2.0).max(30.0);

        ui.label(egui::RichText::new(label_a).color(label_color).size(10.0));
        changed |= ui
            .add_sized(
                [field_w, 20.0],
                egui::DragValue::new(val_a)
                    .speed(1.0)
                    .update_while_editing(false),
            )
            .changed();
        ui.add_space(spacing);
        ui.label(egui::RichText::new(label_b).color(label_color).size(10.0));
        changed |= ui
            .add_sized(
                [field_w, 20.0],
                egui::DragValue::new(val_b)
                    .speed(1.0)
                    .update_while_editing(false),
            )
            .changed();
    });
    changed
}
```

Place this near the existing `drag_field` / `drag_field_colored` helpers at the bottom of the file.

- [ ] **Step 3: Replace X/Y and W/H rows in scene override branch**

Replace lines 172-210 in the scene override branch:

```rust
ui.add_space(4.0);

let mut transform_changed = false;

// X / Y row
transform_changed |= transform_row_2(
    ui, "X", &mut transform.x, "Y", &mut transform.y, text_color,
);

ui.add_space(2.0);

// W / H row with aspect-ratio lock + reset
let prev_w = transform.width;
let prev_h = transform.height;
ui.horizontal(|ui| {
    let label_w = 16.0;
    let spacing = 4.0;
    // Account for lock button (~16px) and reset button (~16px)
    let extra_buttons = 36.0;
    let total_labels = label_w * 2.0 + spacing + extra_buttons;
    let field_w = ((ui.available_width() - total_labels) / 2.0).max(30.0);

    ui.label(egui::RichText::new("W").color(text_color).size(10.0));
    transform_changed |= ui
        .add_sized(
            [field_w, 20.0],
            egui::DragValue::new(&mut transform.width)
                .speed(1.0)
                .update_while_editing(false),
        )
        .changed();
    if aspect_lock_button(ui, aspect_locked) {
        state.library[lib_idx].aspect_ratio_locked = !aspect_locked;
        changed = true;
    }
    ui.label(egui::RichText::new("H").color(text_color).size(10.0));
    transform_changed |= ui
        .add_sized(
            [field_w, 20.0],
            egui::DragValue::new(&mut transform.height)
                .speed(1.0)
                .update_while_editing(false),
        )
        .changed();
    if ui
        .add(
            egui::Button::new(
                egui::RichText::new(egui_phosphor::regular::ARROW_COUNTER_CLOCKWISE)
                    .size(12.0)
                    .color(theme.text_secondary),
            )
            .frame(false),
        )
        .on_hover_text("Reset to native size")
        .clicked()
    {
        transform.width = native_size.0;
        transform.height = native_size.1;
        transform_changed = true;
    }
});
```

- [ ] **Step 4: Update rotation row alignment**

Replace the rotation row (lines 219-234):

```rust
ui.add_space(2.0);

// Rotation row — aligned to same grid
ui.horizontal(|ui| {
    let mut rotation = transform.rotation;
    // Skip first label column to align with X/W inputs
    ui.add_space(16.0);
    let field_w = 60.0;
    let response = ui.add_sized(
        [field_w, 20.0],
        egui::DragValue::new(&mut rotation)
            .speed(1.0)
            .suffix("°")
            .range(0.0..=360.0)
            .update_while_editing(false),
    );
    ui.add_space(4.0);
    ui.label(egui::RichText::new("Rotation").color(text_color).size(10.0));
    if response.changed() {
        transform.rotation = rotation.rem_euclid(360.0);
        transform_changed = true;
    }
});
```

- [ ] **Step 5: Apply same changes to library mode branch**

Apply the same layout changes to the library mode branch (lines 243-310). Use `transform_row_2` for X/Y:

```rust
// X / Y row
{
    let source = &mut state.library[lib_idx];
    changed |= transform_row_2(
        ui,
        "X", &mut source.transform.x,
        "Y", &mut source.transform.y,
        theme.text_muted,
    );
}
```

And similarly restructure the W/H and rotation rows to match the scene override branch layout, using `theme.text_muted` as the label color.

- [ ] **Step 6: Standardize section spacing**

Add consistent 12px spacing between sections. After the transform section and before opacity:

In the main `draw` function around line 96-100, ensure:

```rust
changed |= draw_transform_section(ui, state, selected_id, lib_idx, in_active_scene);
ui.add_space(12.0);
changed |= draw_opacity_section(ui, state, selected_id, lib_idx, in_active_scene);
ui.add_space(12.0);
changed |= draw_source_properties(ui, state, selected_id, lib_idx);
```

- [ ] **Step 7: Verify**

Run: `cargo run`
Expected:
- Transform X/Y and W/H fields align in a consistent 4-column grid.
- Rotation input aligns under the X/W input column.
- Section headers (TRANSFORM, OPACITY, SOURCE) are brighter and consistent.
- 12px gaps between sections.
- Fields stretch to fill available width evenly.

- [ ] **Step 8: Commit**

```bash
git add src/ui/properties_panel.rs
git commit -m "style: align properties panel with consistent column grid and section spacing"
```
