# Unify Panel Logic Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the duplicated floating panel rendering (inline `egui::Window` code) with a custom `egui::Area`-based container that delegates to the shared `render_tab_bar()` and `render_content()` functions, making all panel group rendering consistent.

**Architecture:** Floating panels switch from `egui::Window` to a custom `egui::Area` with a floating chrome header (collapse, title, close) + edge resize handles. The existing `render_tab_bar()` and `render_content()` functions become the single rendering path for all panel groups. A new `LayoutAction::UpdateFloatingGeometry` action propagates position/size changes from the renderer to `DockLayout`.

**Tech Stack:** Rust, egui 0.33, wgpu (via egui_wgpu)

**Spec:** `docs/superpowers/specs/2026-03-21-unify-panel-logic-design.md`

---

### Task 1: Add `UpdateFloatingGeometry` action and tree.rs helper

**Files:**
- Modify: `src/ui/layout/render.rs:34-81` (LayoutAction enum)
- Modify: `src/ui/layout/tree.rs:238-245` (near FloatingGroup)

- [ ] **Step 1: Add `UpdateFloatingGeometry` variant to `LayoutAction`**

In `src/ui/layout/render.rs`, add a new variant to the `LayoutAction` enum after `MoveGroupToTarget`:

```rust
    /// Update a floating group's position and/or size.
    UpdateFloatingGeometry {
        group_id: GroupId,
        pos: egui::Pos2,
        size: egui::Vec2,
    },
```

- [ ] **Step 2: Add `update_floating_geometry()` to `DockLayout`**

In `src/ui/layout/tree.rs`, add a method to `DockLayout` (near the other floating methods like `remove_floating` at line 740):

```rust
    /// Update a floating group's position and size.
    pub fn update_floating_geometry(&mut self, group_id: GroupId, pos: egui::Pos2, size: egui::Vec2) {
        if let Some(fg) = self.floating.iter_mut().find(|fg| fg.group_id == group_id) {
            fg.pos = pos;
            fg.size = size;
        }
    }
```

- [ ] **Step 3: Add test for `update_floating_geometry`**

In `src/ui/layout/tree.rs`, add to the `#[cfg(test)]` module:

```rust
    #[test]
    fn update_floating_geometry() {
        let mut layout = DockLayout::default_layout();
        let entry = TabEntry {
            panel_id: PanelId::next(),
            panel_type: PanelType::AudioMixer,
        };
        let gid = layout.add_floating_group(entry, egui::pos2(100.0, 100.0));
        layout.update_floating_geometry(gid, egui::pos2(200.0, 300.0), egui::vec2(500.0, 400.0));
        let fg = layout.floating.iter().find(|f| f.group_id == gid).unwrap();
        assert_eq!(fg.pos, egui::pos2(200.0, 300.0));
        assert_eq!(fg.size, egui::vec2(500.0, 400.0));
    }
```

- [ ] **Step 4: Run tests**

Run: `cargo test update_floating_geometry`
Expected: PASS

- [ ] **Step 5: Add handler in `window.rs`**

In `src/window.rs`, add a match arm in the action handling block (after the `MoveGroupToTarget` handler, around line 296):

```rust
                LayoutAction::UpdateFloatingGeometry {
                    group_id,
                    pos,
                    size,
                } => {
                    self.layout.update_floating_geometry(group_id, pos, size);
                }
```

- [ ] **Step 6: Verify build**

Run: `cargo build`
Expected: Compiles with no errors (may have warnings about unused variant until Task 3 emits it)

- [ ] **Step 7: Commit**

```bash
git add src/ui/layout/render.rs src/ui/layout/tree.rs src/window.rs
git commit -m "feat: add UpdateFloatingGeometry action and tree helper"
```

---

### Task 2: Add `FLOATING_HEADER_HEIGHT` constant and `render_floating_chrome()` skeleton

**Files:**
- Modify: `src/ui/layout/render.rs` (constants section, new function)

- [ ] **Step 1: Add the constant**

In `src/ui/layout/render.rs`, after `DOCK_GRIP_WIDTH` (line 26), add:

```rust
const FLOATING_HEADER_HEIGHT: f32 = 28.0;
const FLOATING_BORDER: egui::Color32 = egui::Color32::from_gray(50);
const FLOATING_MIN_SIZE: egui::Vec2 = egui::vec2(200.0, 100.0);
```

- [ ] **Step 2: Write `render_floating_chrome()` skeleton**

Add a new function after `render_content()` (after line 1064). This skeleton does the chrome header + delegates to existing functions. The full implementation handles: shadow/border painting, chrome header (collapse, title, close), title bar drag, edge resize, drop target rect registration, then calls `render_tab_bar()` + `render_content()`.

```rust
// ---------------------------------------------------------------------------
// Floating chrome rendering
// ---------------------------------------------------------------------------

/// Render a floating panel container with custom chrome header, then delegate
/// to the shared `render_tab_bar()` and `render_content()` for the panel group.
fn render_floating_chrome(
    ctx: &egui::Context,
    layout: &DockLayout,
    fg: &super::tree::FloatingGroup,
    group: &super::tree::Group,
    state: &mut crate::state::AppState,
    actions: &mut Vec<LayoutAction>,
    is_main: bool,
) {
    let group_id = fg.group_id;
    // fg.size represents the total interior (tab bar + content), matching the old
    // egui::Window default_size semantics. We add only the chrome header on top.
    let total_height = FLOATING_HEADER_HEIGHT + fg.size.y;
    let total_size = egui::vec2(fg.size.x, total_height);

    // --- Click-to-raise: bring this floating panel to front on click ---
    let area_id = egui::Id::new(("floating_chrome", group_id.0));
    let area_resp = egui::Area::new(area_id)
        .fixed_pos(fg.pos)
        .order(egui::Order::Foreground)
        .sense(egui::Sense::click())
        .show(ctx, |ui| {
            ui.set_min_size(total_size);
            ui.set_max_size(total_size);
            ui.allocate_exact_size(total_size, egui::Sense::click())
        });
    // Use the response to detect clicks for z-ordering (egui::Area with
    // Order::Foreground already handles z-order on interaction — clicking
    // an Area brings its layer to the top of the Foreground order).

    let outer_rect = egui::Rect::from_min_size(fg.pos, total_size);

    // --- Shadow + border ---
    let shadow_layer = egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new(("floating_shadow", group_id.0)),
    );
    let shadow_painter = ctx.layer_painter(shadow_layer);
    let shadow = egui::Shadow {
        offset: [0, 4],
        blur: 16,
        spread: 4,
        color: egui::Color32::from_black_alpha(120),
    };
    shadow_painter.add(shadow.as_shape(outer_rect, 0.0));
    shadow_painter.rect(
        outer_rect,
        0.0,
        CONTENT_BG,
        egui::Stroke::new(1.0, FLOATING_BORDER),
    );

    // --- Chrome header (collapse, title, close) ---
    let chrome_rect = egui::Rect::from_min_size(
        fg.pos,
        egui::vec2(fg.size.x, FLOATING_HEADER_HEIGHT),
    );
    let chrome_layer = egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new(("floating_chrome_bar", group_id.0)),
    );
    let chrome_painter = ctx.layer_painter(chrome_layer);
    chrome_painter.rect_filled(chrome_rect, 0.0, TAB_BAR_BG);

    let button_size = 20.0;
    let button_margin = 4.0;

    // Collapse button (left) — docks to grid
    let collapse_center = egui::pos2(
        chrome_rect.min.x + button_margin + button_size / 2.0,
        chrome_rect.center().y,
    );
    let collapse_rect = egui::Rect::from_center_size(
        collapse_center,
        egui::vec2(button_size, button_size),
    );
    let collapse_id = egui::Id::new(("floating_collapse", group_id.0));
    let collapse_resp = egui::Area::new(collapse_id)
        .fixed_pos(collapse_rect.min)
        .order(egui::Order::Foreground)
        .sense(egui::Sense::click())
        .show(ctx, |ui| {
            ui.allocate_exact_size(collapse_rect.size(), egui::Sense::click()).1
        })
        .inner;

    // Draw collapse icon (downward chevron ∨)
    let collapse_color = if collapse_resp.hovered() { TEXT_BRIGHT } else { TEXT_DIM };
    let s = 4.0;
    chrome_painter.line_segment(
        [
            collapse_center + egui::vec2(-s, -s * 0.5),
            collapse_center + egui::vec2(0.0, s * 0.5),
        ],
        egui::Stroke::new(1.5, collapse_color),
    );
    chrome_painter.line_segment(
        [
            collapse_center + egui::vec2(s, -s * 0.5),
            collapse_center + egui::vec2(0.0, s * 0.5),
        ],
        egui::Stroke::new(1.5, collapse_color),
    );
    if collapse_resp.clicked() {
        actions.push(LayoutAction::DockFloatingToGrid { group_id });
    }

    // Close button (right)
    let close_center = egui::pos2(
        chrome_rect.max.x - button_margin - button_size / 2.0,
        chrome_rect.center().y,
    );
    let close_rect = egui::Rect::from_center_size(
        close_center,
        egui::vec2(button_size, button_size),
    );
    let close_id = egui::Id::new(("floating_close", group_id.0));
    let close_resp = egui::Area::new(close_id)
        .fixed_pos(close_rect.min)
        .order(egui::Order::Foreground)
        .sense(egui::Sense::click())
        .show(ctx, |ui| {
            ui.allocate_exact_size(close_rect.size(), egui::Sense::click()).1
        })
        .inner;

    let close_color = if close_resp.hovered() { TEXT_BRIGHT } else { TEXT_DIM };
    let xs = 3.5;
    chrome_painter.line_segment(
        [close_center - egui::vec2(xs, xs), close_center + egui::vec2(xs, xs)],
        egui::Stroke::new(1.5, close_color),
    );
    chrome_painter.line_segment(
        [
            close_center + egui::vec2(-xs, xs),
            close_center + egui::vec2(xs, -xs),
        ],
        egui::Stroke::new(1.5, close_color),
    );
    if close_resp.clicked() {
        actions.push(LayoutAction::CloseFloatingGroup { group_id });
    }

    // Title (center)
    let active_name = group.active_tab_entry().panel_type.display_name();
    chrome_painter.text(
        chrome_rect.center(),
        egui::Align2::CENTER_CENTER,
        active_name,
        egui::FontId::proportional(12.0),
        TEXT_DIM,
    );

    // --- Title bar drag (move floating container) ---
    let drag_area_id = egui::Id::new(("floating_drag", group_id.0));
    let drag_resp = egui::Area::new(drag_area_id)
        .fixed_pos(chrome_rect.min)
        .order(egui::Order::Foreground)
        .sense(egui::Sense::drag())
        .show(ctx, |ui| {
            // Leave space for collapse and close buttons
            let drag_rect = egui::Rect::from_min_max(
                egui::pos2(
                    chrome_rect.min.x + button_margin + button_size + 4.0,
                    chrome_rect.min.y,
                ),
                egui::pos2(
                    chrome_rect.max.x - button_margin - button_size - 4.0,
                    chrome_rect.max.y,
                ),
            );
            ui.allocate_exact_size(drag_rect.size(), egui::Sense::drag()).1
        })
        .inner;

    if drag_resp.dragged() {
        let delta = drag_resp.drag_delta();
        let new_pos = fg.pos + delta;
        actions.push(LayoutAction::UpdateFloatingGeometry {
            group_id,
            pos: new_pos,
            size: fg.size,
        });
    }

    // --- Edge/corner resize handles ---
    let resize_margin = 4.0;
    // Right edge
    let right_edge = egui::Rect::from_min_size(
        egui::pos2(outer_rect.max.x - resize_margin, outer_rect.min.y + FLOATING_HEADER_HEIGHT),
        egui::vec2(resize_margin * 2.0, outer_rect.height() - FLOATING_HEADER_HEIGHT),
    );
    let right_id = egui::Id::new(("floating_resize_r", group_id.0));
    let right_resp = egui::Area::new(right_id)
        .fixed_pos(right_edge.min)
        .order(egui::Order::Foreground)
        .sense(egui::Sense::drag())
        .show(ctx, |ui| {
            ui.allocate_exact_size(right_edge.size(), egui::Sense::drag()).1
        })
        .inner;
    if right_resp.hovered() || right_resp.dragged() {
        ctx.set_cursor_icon(egui::CursorIcon::ResizeColumn);
    }
    if right_resp.dragged() {
        let new_width = (fg.size.x + right_resp.drag_delta().x).max(FLOATING_MIN_SIZE.x);
        actions.push(LayoutAction::UpdateFloatingGeometry {
            group_id,
            pos: fg.pos,
            size: egui::vec2(new_width, fg.size.y),
        });
    }

    // Bottom edge
    let bottom_edge = egui::Rect::from_min_size(
        egui::pos2(outer_rect.min.x, outer_rect.max.y - resize_margin),
        egui::vec2(outer_rect.width(), resize_margin * 2.0),
    );
    let bottom_id = egui::Id::new(("floating_resize_b", group_id.0));
    let bottom_resp = egui::Area::new(bottom_id)
        .fixed_pos(bottom_edge.min)
        .order(egui::Order::Foreground)
        .sense(egui::Sense::drag())
        .show(ctx, |ui| {
            ui.allocate_exact_size(bottom_edge.size(), egui::Sense::drag()).1
        })
        .inner;
    if bottom_resp.hovered() || bottom_resp.dragged() {
        ctx.set_cursor_icon(egui::CursorIcon::ResizeRow);
    }
    if bottom_resp.dragged() {
        let new_height = (fg.size.y + bottom_resp.drag_delta().y).max(FLOATING_MIN_SIZE.y);
        actions.push(LayoutAction::UpdateFloatingGeometry {
            group_id,
            pos: fg.pos,
            size: egui::vec2(fg.size.x, new_height),
        });
    }

    // Left edge
    let left_edge = egui::Rect::from_min_size(
        egui::pos2(outer_rect.min.x - resize_margin, outer_rect.min.y + FLOATING_HEADER_HEIGHT),
        egui::vec2(resize_margin * 2.0, outer_rect.height() - FLOATING_HEADER_HEIGHT),
    );
    let left_id = egui::Id::new(("floating_resize_l", group_id.0));
    let left_resp = egui::Area::new(left_id)
        .fixed_pos(left_edge.min)
        .order(egui::Order::Foreground)
        .sense(egui::Sense::drag())
        .show(ctx, |ui| {
            ui.allocate_exact_size(left_edge.size(), egui::Sense::drag()).1
        })
        .inner;
    if left_resp.hovered() || left_resp.dragged() {
        ctx.set_cursor_icon(egui::CursorIcon::ResizeColumn);
    }
    if left_resp.dragged() {
        let delta = left_resp.drag_delta().x;
        let new_width = (fg.size.x - delta).max(FLOATING_MIN_SIZE.x);
        let actual_delta = fg.size.x - new_width;
        actions.push(LayoutAction::UpdateFloatingGeometry {
            group_id,
            pos: egui::pos2(fg.pos.x + actual_delta, fg.pos.y),
            size: egui::vec2(new_width, fg.size.y),
        });
    }

    // Bottom-right corner
    let corner_rect = egui::Rect::from_min_size(
        egui::pos2(
            outer_rect.max.x - resize_margin,
            outer_rect.max.y - resize_margin,
        ),
        egui::vec2(resize_margin * 2.0, resize_margin * 2.0),
    );
    let corner_id = egui::Id::new(("floating_resize_br", group_id.0));
    let corner_resp = egui::Area::new(corner_id)
        .fixed_pos(corner_rect.min)
        .order(egui::Order::Foreground)
        .sense(egui::Sense::drag())
        .show(ctx, |ui| {
            ui.allocate_exact_size(corner_rect.size(), egui::Sense::drag()).1
        })
        .inner;
    if corner_resp.hovered() || corner_resp.dragged() {
        ctx.set_cursor_icon(egui::CursorIcon::ResizeNwSe);
    }
    if corner_resp.dragged() {
        let d = corner_resp.drag_delta();
        let new_width = (fg.size.x + d.x).max(FLOATING_MIN_SIZE.x);
        let new_height = (fg.size.y + d.y).max(FLOATING_MIN_SIZE.y);
        actions.push(LayoutAction::UpdateFloatingGeometry {
            group_id,
            pos: fg.pos,
            size: egui::vec2(new_width, new_height),
        });
    }

    // --- Shared tab bar ---
    // Tab bar sits directly below the chrome header
    let tab_bar_rect = egui::Rect::from_min_size(
        egui::pos2(fg.pos.x, fg.pos.y + FLOATING_HEADER_HEIGHT),
        egui::vec2(fg.size.x, TAB_BAR_HEIGHT),
    );
    render_tab_bar(
        ctx,
        layout,
        group_id,
        group,
        tab_bar_rect,
        actions,
        TabBarContext {
            is_main,
            is_floating: true,
        },
    );

    // --- Shared content area ---
    // Content fills the remaining space below the tab bar
    let content_rect = egui::Rect::from_min_max(
        egui::pos2(fg.pos.x, fg.pos.y + FLOATING_HEADER_HEIGHT + TAB_BAR_HEIGHT),
        egui::pos2(fg.pos.x + fg.size.x, fg.pos.y + total_height),
    );
    render_content(ctx, group_id, group, content_rect, state);

    // --- Store rect for drop target hit testing ---
    let rect_id = egui::Id::new(("floating_rect", group_id.0));
    ctx.data_mut(|d| d.insert_temp(rect_id, outer_rect));
}
```

- [ ] **Step 3: Verify build**

Run: `cargo build`
Expected: Compiles (function is defined but not yet called — may warn about dead code)

- [ ] **Step 4: Commit**

```bash
git add src/ui/layout/render.rs
git commit -m "feat: add render_floating_chrome() with custom header, drag, and resize"
```

---

### Task 3: Replace `egui::Window` floating code with `render_floating_chrome()` call

**Files:**
- Modify: `src/ui/layout/render.rs:160-440` (floating groups section in `render_layout()`)

- [ ] **Step 1: Replace the floating groups block**

In `src/ui/layout/render.rs`, replace the entire `// --- Floating groups ---` block (lines 160-440) with a simple loop that calls `render_floating_chrome()`:

```rust
    // --- Floating groups ---
    for fg in &layout.floating {
        if let Some(group) = layout.groups.get(&fg.group_id) {
            render_floating_chrome(ctx, layout, fg, group, state, actions, is_main);
        }
    }
```

This replaces ~280 lines of inline `egui::Window` code with 4 lines that delegate to the shared function.

- [ ] **Step 2: Verify build**

Run: `cargo build`
Expected: Compiles with no errors. The `egui::Window` import may become unused — remove if needed.

- [ ] **Step 3: Run all tests**

Run: `cargo test`
Expected: All existing tests pass. The data structures and action handlers are unchanged.

- [ ] **Step 4: Run clippy**

Run: `cargo clippy`
Expected: No new warnings (fix any that appear).

- [ ] **Step 5: Commit**

```bash
git add src/ui/layout/render.rs
git commit -m "refactor: replace floating egui::Window with shared render_floating_chrome()"
```

---

### Task 4: Manual smoke test and fix-up

**Files:**
- Possibly modify: `src/ui/layout/render.rs` (adjustments from testing)

- [ ] **Step 1: Run the app**

Run: `cargo run`

Test the following interactions:

1. **Docked panels**: Click tabs, drag tabs, use "+" button, use grip context menu — should work exactly as before.
2. **Detach to float**: Right-click a docked tab → "Detach" — should create a floating panel with the custom chrome header (collapse, title, close).
3. **Floating chrome header**: Verify collapse button docks to grid, close button removes the floating panel, title shows active panel name.
4. **Floating drag**: Drag the floating panel's title bar — should move smoothly.
5. **Floating resize**: Drag edges and bottom-right corner — should resize with min size enforced.
6. **Floating tab bar**: Click tabs, use "+" button, use grip — should work identically to docked panels.
7. **Floating tab drag**: Drag a tab from a floating panel — should work (this is new behavior from unification).
8. **Drop onto floating panel**: Drag a tab from a docked panel onto a floating panel — should add as tab.
9. **Dock grip drag**: Drag the grip on a floating panel to a docked group — should merge/split.
10. **Pop Out to Window**: Right-click tab → "Pop Out to Window" — should still create detached OS window.
11. **Detached window panels**: All interactions in a detached OS window should work identically.

- [ ] **Step 2: Fix any issues found during smoke test**

Address visual, interaction, or layout issues discovered. Common things to check:
- Z-ordering of overlapping floating panels (click to raise)
- Shadow rendering position
- Resize handle responsiveness
- Content area sizing

- [ ] **Step 3: Run tests and clippy after fixes**

Run: `cargo test && cargo clippy`
Expected: All pass

- [ ] **Step 4: Commit fixes**

```bash
git add -u
git commit -m "fix: address floating panel rendering issues from smoke test"
```
