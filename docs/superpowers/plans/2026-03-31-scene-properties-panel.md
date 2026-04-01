# Scene Properties Panel Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show scene properties (name, transition override with color pickers, pinned) in the Properties panel when no source is selected.

**Architecture:** Add a third fallback mode to the Properties panel's `draw()` function. When no source is selected but `active_scene_id` is set, call a new `draw_scene_properties()` function. Remove the transition override submenu from the scene context menu. Remove `#[allow(dead_code)]` from registry fields now consumed by the UI.

**Tech Stack:** Rust, egui, wgpu (indirectly via TransitionRegistry)

---

## File Structure

| File | Action | Responsibility |
|------|--------|---------------|
| `src/ui/properties_panel.rs` | Modify | Add scene properties mode to `draw()`, add `draw_scene_properties()` function |
| `src/ui/scenes_panel.rs` | Modify | Remove "Transition Override" submenu from scene context menu |
| `src/transition_registry.rs` | Modify | Remove `#[allow(dead_code)]` from `author`, `description`, `params` fields |

---

## Task 1: Add Scene Properties Mode to Properties Panel

**Files:**
- Modify: `src/ui/properties_panel.rs`

This task adds the `draw_scene_properties()` function and wires it into the existing `draw()` function as a third fallback.

- [ ] **Step 1: Update the `draw()` function's empty-state fallback**

In `src/ui/properties_panel.rs`, replace the empty-state block (lines 26-36):

```rust
    } else {
        // Empty state: centered muted label.
        ui.vertical_centered(|ui| {
            ui.add_space(ui.available_height() / 3.0);
            ui.label(
                egui::RichText::new("Select a source to view properties")
                    .color(theme.text_muted)
                    .size(11.0),
            );
        });
        return;
    };
```

With:

```rust
    } else {
        // No source selected — show scene properties if a scene is active.
        if state.active_scene_id.is_some() {
            draw_scene_properties(ui, state);
            return;
        }
        // Empty state: centered muted label.
        ui.vertical_centered(|ui| {
            ui.add_space(ui.available_height() / 3.0);
            ui.label(
                egui::RichText::new("Select a source to view properties")
                    .color(theme.text_muted)
                    .size(11.0),
            );
        });
        return;
    };
```

- [ ] **Step 2: Update the module doc comment**

Replace the module doc comment (lines 1-5) with:

```rust
//! Properties panel — context-sensitive property editor.
//!
//! Works in three modes:
//! - **Scene mode** — source selected in active scene: edits scene overrides, shows override dots
//! - **Library mode** — source selected in library: edits library defaults directly
//! - **Scene properties mode** — no source selected, scene active: edits scene name, transition, pinned
```

- [ ] **Step 3: Add the `draw_scene_properties()` function**

Add this function at the end of the file (before any `#[cfg(test)]` block if one exists, or at the very end):

```rust
/// Draw scene-level properties when no source is selected.
///
/// Shows: scene name, transition override (type + duration + color pickers), pinned toggle.
fn draw_scene_properties(ui: &mut egui::Ui, state: &mut AppState) {
    let theme = active_theme(ui.ctx());

    let Some(scene_id) = state.active_scene_id else {
        return;
    };
    let scene_name = state
        .active_scene()
        .map(|s| s.name.clone())
        .unwrap_or_default();

    // ── Header ──
    ui.label(
        egui::RichText::new(format!("SCENE PROPERTIES — {}", scene_name.to_uppercase()))
            .color(theme.accent)
            .size(9.0),
    );
    ui.add_space(8.0);

    let mut changed = false;

    // ── Name field ──
    ui.label(
        egui::RichText::new("Name")
            .color(theme.text_muted)
            .size(9.0),
    );
    ui.add_space(2.0);

    let name_key = egui::Id::new(("scene_props_name", scene_id.0));
    let mut name_str: String = ui.data_mut(|d| {
        d.get_temp::<String>(name_key)
            .unwrap_or_else(|| scene_name.clone())
    });

    let name_resp = ui.add(
        egui::TextEdit::singleline(&mut name_str)
            .desired_width(ui.available_width())
            .font(egui::FontId::proportional(12.0)),
    );

    if name_resp.changed() {
        ui.data_mut(|d| d.insert_temp(name_key, name_str.clone()));
        if let Some(scene) = state.scenes.iter_mut().find(|s| s.id == scene_id) {
            scene.name = name_str;
        }
        changed = true;
    }
    // Sync temp storage when scene changes externally (e.g. undo).
    if !name_resp.has_focus() {
        let current = state
            .active_scene()
            .map(|s| s.name.clone())
            .unwrap_or_default();
        ui.data_mut(|d| d.insert_temp(name_key, current));
    }

    ui.add_space(12.0);

    // ── Transition In section ──
    ui.label(
        egui::RichText::new("Transition In")
            .color(theme.text_secondary)
            .size(10.0),
    );
    ui.add_space(4.0);

    changed |= draw_scene_transition_override(ui, state, scene_id);

    ui.add_space(12.0);

    // ── Pinned toggle ──
    let mut pinned = state
        .scenes
        .iter()
        .find(|s| s.id == scene_id)
        .map(|s| s.pinned)
        .unwrap_or(false);

    if ui.checkbox(&mut pinned, "Pinned").changed() {
        if let Some(scene) = state.scenes.iter_mut().find(|s| s.id == scene_id) {
            scene.pinned = pinned;
        }
        changed = true;
    }

    if changed {
        state.mark_dirty();
    }
}

/// Draw the transition override controls for a scene (type dropdown, duration, color pickers).
///
/// Returns true if any value changed.
fn draw_scene_transition_override(
    ui: &mut egui::Ui,
    state: &mut AppState,
    scene_id: crate::scene::SceneId,
) -> bool {
    let theme = active_theme(ui.ctx());
    let mut changed = false;

    // Read current override.
    let (current_transition, current_duration_ms) = state
        .scenes
        .iter()
        .find(|s| s.id == scene_id)
        .map(|s| (
            s.transition_override.transition.clone(),
            s.transition_override.duration_ms,
        ))
        .unwrap_or((None, None));

    // ── Type dropdown ──
    ui.label(
        egui::RichText::new("Type")
            .color(theme.text_muted)
            .size(9.0),
    );
    ui.add_space(2.0);

    let type_label = current_transition
        .as_ref()
        .and_then(|id| state.transition_registry.get(id))
        .map(|t| t.name.clone())
        .unwrap_or_else(|| "Default".to_string());

    let all_transitions: Vec<_> = state
        .transition_registry
        .all()
        .iter()
        .map(|d| (d.id.clone(), d.name.clone()))
        .collect();

    egui::ComboBox::from_id_salt(egui::Id::new(("scene_props_tx_type", scene_id.0)))
        .selected_text(&type_label)
        .width(ui.available_width() - 16.0)
        .show_ui(ui, |ui| {
            if ui
                .selectable_label(current_transition.is_none(), "Default")
                .clicked()
            {
                if let Some(scene) = state.scenes.iter_mut().find(|s| s.id == scene_id) {
                    scene.transition_override.transition = None;
                }
                changed = true;
            }
            for (id, name) in &all_transitions {
                if ui
                    .selectable_label(
                        current_transition.as_deref() == Some(id.as_str()),
                        name,
                    )
                    .clicked()
                {
                    if let Some(scene) = state.scenes.iter_mut().find(|s| s.id == scene_id) {
                        scene.transition_override.transition = Some(id.clone());
                    }
                    changed = true;
                }
            }
        });

    ui.add_space(4.0);

    // ── Duration input ──
    ui.label(
        egui::RichText::new("Duration (ms)")
            .color(theme.text_muted)
            .size(9.0),
    );
    ui.add_space(2.0);

    let dur_key = egui::Id::new(("scene_props_dur", scene_id.0));
    let mut dur_str: String = ui.data_mut(|d| {
        d.get_temp::<String>(dur_key).unwrap_or_else(|| {
            current_duration_ms
                .map(|v| v.to_string())
                .unwrap_or_default()
        })
    });

    let dur_resp = ui.add(
        egui::TextEdit::singleline(&mut dur_str)
            .desired_width(80.0)
            .hint_text("Default")
            .font(egui::FontId::proportional(12.0)),
    );

    if dur_resp.changed() {
        ui.data_mut(|d| d.insert_temp(dur_key, dur_str.clone()));
        if let Some(scene) = state.scenes.iter_mut().find(|s| s.id == scene_id) {
            scene.transition_override.duration_ms = if dur_str.trim().is_empty() {
                None
            } else {
                dur_str.trim().parse::<u32>().ok()
            };
        }
        changed = true;
    }
    if dur_resp.lost_focus() {
        ui.data_mut(|d| d.remove::<String>(dur_key));
    }

    ui.add_space(4.0);

    // ── Color pickers (based on effective transition's @params) ──
    // Effective transition: scene override if set, else global default.
    let effective_id = current_transition
        .as_deref()
        .unwrap_or(&state.settings.transitions.default_transition);

    let params: Vec<crate::transition_registry::TransitionParam> = state
        .transition_registry
        .get(effective_id)
        .map(|d| d.params.clone())
        .unwrap_or_default();

    if !params.is_empty() {
        ui.add_space(4.0);

        // Ensure colors exist in override when we need to edit them.
        // Clone the current colors or initialize from global defaults.
        let current_colors = state
            .scenes
            .iter()
            .find(|s| s.id == scene_id)
            .and_then(|s| s.transition_override.colors)
            .unwrap_or(state.settings.transitions.default_colors);

        for param in &params {
            let (label, field_getter): (&str, fn(&crate::transition::TransitionColors) -> [f32; 4]) = match param {
                crate::transition_registry::TransitionParam::Color => ("Color", |c| c.color),
                crate::transition_registry::TransitionParam::FromColor => ("From Color", |c| c.from_color),
                crate::transition_registry::TransitionParam::ToColor => ("To Color", |c| c.to_color),
            };

            let color_val = field_getter(&current_colors);

            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(label)
                        .color(theme.text_muted)
                        .size(9.0),
                );

                let mut rgba = egui::ecolor::Rgba::from_rgba_unmultiplied(
                    color_val[0], color_val[1], color_val[2], color_val[3],
                );

                if egui::color_picker::color_edit_button_rgba(
                    ui,
                    &mut rgba,
                    egui::color_picker::Alpha::OnlyBlend,
                )
                .changed()
                {
                    let new_val = [rgba.r(), rgba.g(), rgba.b(), rgba.a()];

                    if let Some(scene) = state.scenes.iter_mut().find(|s| s.id == scene_id) {
                        // Initialize colors from current state if None.
                        let colors = scene
                            .transition_override
                            .colors
                            .get_or_insert(current_colors);

                        match param {
                            crate::transition_registry::TransitionParam::Color => {
                                colors.color = new_val;
                            }
                            crate::transition_registry::TransitionParam::FromColor => {
                                colors.from_color = new_val;
                            }
                            crate::transition_registry::TransitionParam::ToColor => {
                                colors.to_color = new_val;
                            }
                        }
                    }
                    changed = true;
                }
            });
        }
    }

    changed
}
```

- [ ] **Step 4: Verify it compiles**

Run: `cargo check 2>&1 | head -20`
Expected: clean compilation (or warnings about dead code on registry fields — fixed in Task 3).

- [ ] **Step 5: Commit**

```bash
git add src/ui/properties_panel.rs
git commit -m "feat: add scene properties mode to Properties panel"
```

---

## Task 2: Remove Transition Override from Scene Context Menu

**Files:**
- Modify: `src/ui/scenes_panel.rs`

- [ ] **Step 1: Remove the "Transition Override" submenu**

In `src/ui/scenes_panel.rs`, find the scene context menu code. Remove the block from `ui.separator();` (line 442) through the end of the `ui.menu_button("Transition Override", ...)` closure (line 551, the `});` that closes the `menu_button`).

Specifically, remove these lines (442-551):

```rust
        ui.separator();

        ui.menu_button("Transition Override", |ui| {
            // ... all the transition override UI code ...
        });
```

This removes the separator and the entire "Transition Override" submenu. The context menu retains Rename and Delete.

- [ ] **Step 2: Verify it compiles**

Run: `cargo check 2>&1 | head -10`
Expected: clean compilation.

- [ ] **Step 3: Run all tests**

Run: `cargo test 2>&1 | tail -5`
Expected: all tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/ui/scenes_panel.rs
git commit -m "refactor: remove transition override from scene context menu (now in Properties panel)"
```

---

## Task 3: Remove dead_code Allows from TransitionRegistry Fields

**Files:**
- Modify: `src/transition_registry.rs`

- [ ] **Step 1: Remove `#[allow(dead_code)]` from `author`, `description`, and `params` fields**

In `src/transition_registry.rs`, remove the three `#[allow(dead_code)]` attributes from the `TransitionDef` struct fields (lines 17, 20, 23):

Before:
```rust
    /// Author from `@author` header, or empty.
    #[allow(dead_code)]
    pub author: String,
    /// Description from `@description` header, or empty.
    #[allow(dead_code)]
    pub description: String,
    /// Which color uniforms to expose in the UI, from `@params` header.
    #[allow(dead_code)]
    pub params: Vec<TransitionParam>,
```

After:
```rust
    /// Author from `@author` header, or empty.
    pub author: String,
    /// Description from `@description` header, or empty.
    pub description: String,
    /// Which color uniforms to expose in the UI, from `@params` header.
    pub params: Vec<TransitionParam>,
```

- [ ] **Step 2: Run clippy**

Run: `cargo clippy 2>&1 | grep "transition_registry" | head -5`
Expected: no warnings from transition_registry.rs.

- [ ] **Step 3: Run all tests**

Run: `cargo test 2>&1 | tail -5`
Expected: all tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/transition_registry.rs
git commit -m "chore: remove dead_code allows from TransitionDef fields (now used by Properties panel)"
```

---

## Summary

| Task | What it does | Key files |
|------|-------------|-----------|
| 1 | Add `draw_scene_properties()` with name, transition dropdown, duration, color pickers, pinned toggle | `properties_panel.rs` |
| 2 | Remove transition override submenu from scene context menu | `scenes_panel.rs` |
| 3 | Remove `#[allow(dead_code)]` from registry fields now consumed by UI | `transition_registry.rs` |
