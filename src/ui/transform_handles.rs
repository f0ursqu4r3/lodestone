//! Interactive transform handles for repositioning and resizing sources in the preview.

use egui::{Pos2, Rect, StrokeKind, Vec2};

use crate::scene::Transform;
use crate::state::AppState;
use crate::ui::theme::{BG_BASE, TEXT_PRIMARY};

const CORNER_SIZE: f32 = 8.0;
const EDGE_SIZE: f32 = 6.0;
const CORNER_HIT_SIZE: f32 = 16.0;
const EDGE_HIT_SIZE: f32 = 12.0;
const MIN_SOURCE_SIZE: f32 = 10.0;
/// How close (in canvas pixels) a value must be to a snap target to snap.
const SNAP_THRESHOLD: f32 = 8.0;

#[derive(Clone, Copy, Debug, PartialEq)]
enum HandlePosition {
    TopLeft,
    Top,
    TopRight,
    Left,
    Right,
    BottomLeft,
    Bottom,
    BottomRight,
}

#[derive(Clone, Debug, Default)]
enum DragMode {
    #[default]
    None,
    Move {
        start_mouse: Pos2,
        start_transform: Transform,
    },
    Resize {
        handle: HandlePosition,
        start_mouse: Pos2,
        start_transform: Transform,
        aspect_ratio: f32,
    },
}

// ── Coordinate Mapping ──────────────────────────────────────────────────────

fn canvas_to_screen(canvas_pos: Pos2, viewport: Rect, canvas_size: Vec2) -> Pos2 {
    Pos2::new(
        viewport.min.x + canvas_pos.x * viewport.width() / canvas_size.x,
        viewport.min.y + canvas_pos.y * viewport.height() / canvas_size.y,
    )
}

fn screen_to_canvas(screen_pos: Pos2, viewport: Rect, canvas_size: Vec2) -> Pos2 {
    Pos2::new(
        (screen_pos.x - viewport.min.x) * canvas_size.x / viewport.width(),
        (screen_pos.y - viewport.min.y) * canvas_size.y / viewport.height(),
    )
}

fn transform_to_screen_rect(t: &Transform, viewport: Rect, canvas_size: Vec2) -> Rect {
    let min = canvas_to_screen(Pos2::new(t.x, t.y), viewport, canvas_size);
    let max = canvas_to_screen(
        Pos2::new(t.x + t.width, t.y + t.height),
        viewport,
        canvas_size,
    );
    Rect::from_min_max(min, max)
}

// ── Handle Drawing ──────────────────────────────────────────────────────────

fn corner_positions(r: Rect) -> [Pos2; 4] {
    [
        r.left_top(),
        r.right_top(),
        r.left_bottom(),
        r.right_bottom(),
    ]
}

fn edge_positions(r: Rect) -> [Pos2; 4] {
    [
        Pos2::new(r.center().x, r.top()),
        Pos2::new(r.center().x, r.bottom()),
        Pos2::new(r.left(), r.center().y),
        Pos2::new(r.right(), r.center().y),
    ]
}

fn draw_handles(painter: &egui::Painter, screen_rect: Rect) {
    // Selection outline
    painter.rect_stroke(
        screen_rect,
        0.0,
        egui::Stroke::new(1.0, TEXT_PRIMARY),
        StrokeKind::Outside,
    );

    // Corner handles (8x8, filled)
    for pos in corner_positions(screen_rect) {
        let r = Rect::from_center_size(pos, Vec2::splat(CORNER_SIZE));
        painter.rect_filled(r, 1.0, TEXT_PRIMARY);
        painter.rect_stroke(r, 1.0, egui::Stroke::new(1.0, BG_BASE), StrokeKind::Outside);
    }

    // Edge handles (6x6, filled)
    for pos in edge_positions(screen_rect) {
        let r = Rect::from_center_size(pos, Vec2::splat(EDGE_SIZE));
        painter.rect_filled(r, 1.0, TEXT_PRIMARY);
        painter.rect_stroke(r, 1.0, egui::Stroke::new(1.0, BG_BASE), StrokeKind::Outside);
    }
}

// ── Hit Testing ─────────────────────────────────────────────────────────────

fn hit_test_handles(pos: Pos2, screen_rect: Rect) -> Option<HandlePosition> {
    let corners = corner_positions(screen_rect);
    let corner_handles = [
        HandlePosition::TopLeft,
        HandlePosition::TopRight,
        HandlePosition::BottomLeft,
        HandlePosition::BottomRight,
    ];
    for (i, &corner) in corners.iter().enumerate() {
        if Rect::from_center_size(corner, Vec2::splat(CORNER_HIT_SIZE)).contains(pos) {
            return Some(corner_handles[i]);
        }
    }

    let edges = edge_positions(screen_rect);
    let edge_handles = [
        HandlePosition::Top,
        HandlePosition::Bottom,
        HandlePosition::Left,
        HandlePosition::Right,
    ];
    for (i, &edge) in edges.iter().enumerate() {
        if Rect::from_center_size(edge, Vec2::splat(EDGE_HIT_SIZE)).contains(pos) {
            return Some(edge_handles[i]);
        }
    }

    None
}

// ── Drag Logic ──────────────────────────────────────────────────────────────

fn apply_resize(
    transform: &mut Transform,
    start: &Transform,
    handle: HandlePosition,
    delta: Vec2,
    constrain: bool,
    aspect: f32,
) {
    let (mut x, mut y, mut w, mut h) = (start.x, start.y, start.width, start.height);

    match handle {
        HandlePosition::TopLeft => {
            x += delta.x;
            y += delta.y;
            w -= delta.x;
            h -= delta.y;
        }
        HandlePosition::Top => {
            y += delta.y;
            h -= delta.y;
        }
        HandlePosition::TopRight => {
            y += delta.y;
            w += delta.x;
            h -= delta.y;
        }
        HandlePosition::Left => {
            x += delta.x;
            w -= delta.x;
        }
        HandlePosition::Right => {
            w += delta.x;
        }
        HandlePosition::BottomLeft => {
            x += delta.x;
            w -= delta.x;
            h += delta.y;
        }
        HandlePosition::Bottom => {
            h += delta.y;
        }
        HandlePosition::BottomRight => {
            w += delta.x;
            h += delta.y;
        }
    }

    if constrain
        && matches!(
            handle,
            HandlePosition::TopLeft
                | HandlePosition::TopRight
                | HandlePosition::BottomLeft
                | HandlePosition::BottomRight
        )
    {
        h = w / aspect;
        if matches!(handle, HandlePosition::TopLeft | HandlePosition::TopRight) {
            y = start.y + start.height - h;
        }
    }

    w = w.max(MIN_SOURCE_SIZE);
    h = h.max(MIN_SOURCE_SIZE);

    transform.x = x;
    transform.y = y;
    transform.width = w;
    transform.height = h;
}

// ── Snapping ───────────────────────────────────────────────────────────────

/// Snap a value to the nearest target if within threshold. Returns the snapped value.
fn snap_value(value: f32, targets: &[f32], threshold: f32) -> f32 {
    let mut best = value;
    let mut best_dist = threshold;
    for &t in targets {
        let dist = (value - t).abs();
        if dist < best_dist {
            best = t;
            best_dist = dist;
        }
    }
    best
}

/// Apply snapping to a transform. Snaps edges to canvas boundaries, canvas center,
/// grid lines, and other source edges.
fn snap_transform(
    transform: &mut Transform,
    canvas_size: Vec2,
    grid_size: f32,
    other_sources: &[&Transform],
) {
    let mut x_targets = Vec::new();
    let mut y_targets = Vec::new();

    // Canvas edges and center.
    x_targets.extend_from_slice(&[0.0, canvas_size.x, canvas_size.x / 2.0]);
    y_targets.extend_from_slice(&[0.0, canvas_size.y, canvas_size.y / 2.0]);

    // Grid lines.
    if grid_size > 0.0 {
        let mut gx = 0.0;
        while gx <= canvas_size.x {
            x_targets.push(gx);
            gx += grid_size;
        }
        let mut gy = 0.0;
        while gy <= canvas_size.y {
            y_targets.push(gy);
            gy += grid_size;
        }
    }

    // Other source edges and centers.
    for other in other_sources {
        x_targets.extend_from_slice(&[other.x, other.x + other.width, other.x + other.width / 2.0]);
        y_targets.extend_from_slice(&[
            other.y,
            other.y + other.height,
            other.y + other.height / 2.0,
        ]);
    }

    // Snap all four edges + center of the source.
    let left = transform.x;
    let right = transform.x + transform.width;
    let center_x = transform.x + transform.width / 2.0;
    let top = transform.y;
    let bottom = transform.y + transform.height;
    let center_y = transform.y + transform.height / 2.0;

    // Find the best snap for X (check left edge, right edge, center).
    let snap_left = snap_value(left, &x_targets, SNAP_THRESHOLD);
    let snap_right = snap_value(right, &x_targets, SNAP_THRESHOLD);
    let snap_cx = snap_value(center_x, &x_targets, SNAP_THRESHOLD);

    let dx_left = (snap_left - left).abs();
    let dx_right = (snap_right - right).abs();
    let dx_cx = (snap_cx - center_x).abs();

    if dx_left <= dx_right && dx_left <= dx_cx && dx_left < SNAP_THRESHOLD {
        transform.x = snap_left;
    } else if dx_right <= dx_cx && dx_right < SNAP_THRESHOLD {
        transform.x = snap_right - transform.width;
    } else if dx_cx < SNAP_THRESHOLD {
        transform.x = snap_cx - transform.width / 2.0;
    }

    // Find the best snap for Y.
    let snap_top = snap_value(top, &y_targets, SNAP_THRESHOLD);
    let snap_bottom = snap_value(bottom, &y_targets, SNAP_THRESHOLD);
    let snap_cy = snap_value(center_y, &y_targets, SNAP_THRESHOLD);

    let dy_top = (snap_top - top).abs();
    let dy_bottom = (snap_bottom - bottom).abs();
    let dy_cy = (snap_cy - center_y).abs();

    if dy_top <= dy_bottom && dy_top <= dy_cy && dy_top < SNAP_THRESHOLD {
        transform.y = snap_top;
    } else if dy_bottom <= dy_cy && dy_bottom < SNAP_THRESHOLD {
        transform.y = snap_bottom - transform.height;
    } else if dy_cy < SNAP_THRESHOLD {
        transform.y = snap_cy - transform.height / 2.0;
    }
}

// ── Snap Guide Lines ───────────────────────────────────────────────────────

/// Draw visual guide lines when a source edge is snapped.
fn draw_snap_guides(
    painter: &egui::Painter,
    transform: &Transform,
    canvas_size: Vec2,
    viewport: Rect,
    other_sources: &[&Transform],
) {
    use crate::ui::theme::TEXT_MUTED;

    let mut x_targets = vec![0.0, canvas_size.x, canvas_size.x / 2.0];
    let mut y_targets = vec![0.0, canvas_size.y, canvas_size.y / 2.0];
    for other in other_sources {
        x_targets.extend_from_slice(&[other.x, other.x + other.width, other.x + other.width / 2.0]);
        y_targets.extend_from_slice(&[
            other.y,
            other.y + other.height,
            other.y + other.height / 2.0,
        ]);
    }

    let edges_x = [
        transform.x,
        transform.x + transform.width,
        transform.x + transform.width / 2.0,
    ];
    let edges_y = [
        transform.y,
        transform.y + transform.height,
        transform.y + transform.height / 2.0,
    ];

    let guide_stroke = egui::Stroke::new(1.0, TEXT_MUTED);

    for &ex in &edges_x {
        for &tx in &x_targets {
            if (ex - tx).abs() < 1.0 {
                let screen_x = canvas_to_screen(Pos2::new(tx, 0.0), viewport, canvas_size).x;
                painter.line_segment(
                    [
                        Pos2::new(screen_x, viewport.top()),
                        Pos2::new(screen_x, viewport.bottom()),
                    ],
                    guide_stroke,
                );
            }
        }
    }

    for &ey in &edges_y {
        for &ty in &y_targets {
            if (ey - ty).abs() < 1.0 {
                let screen_y = canvas_to_screen(Pos2::new(0.0, ty), viewport, canvas_size).y;
                painter.line_segment(
                    [
                        Pos2::new(viewport.left(), screen_y),
                        Pos2::new(viewport.right(), screen_y),
                    ],
                    guide_stroke,
                );
            }
        }
    }
}

// ── Public Entry Point ──────────────────────────────────────────────────────

/// Handle source selection, transform handles, dragging, snapping, and context
/// menus in the preview panel.
///
/// - `viewport_rect`: the letterboxed preview area (used for coordinate mapping)
/// - `panel_rect`: the full preview panel area (used for interaction hit testing —
///   handles and sources may extend beyond the letterboxed viewport)
pub fn draw_transform_handles(
    ui: &mut egui::Ui,
    state: &mut AppState,
    viewport_rect: Rect,
    panel_rect: Rect,
    canvas_size: Vec2,
) {
    use crate::scene::SourceId;

    // Skip all interaction when a dockview panel drag is active — otherwise
    // dragging a panel tab over the preview would select/move sources underneath.
    let dock_drag_active: bool = ui
        .ctx()
        .data(|d| d.get_temp(egui::Id::new("dock_drag_active")).unwrap_or(false));
    if dock_drag_active {
        return;
    }

    let pointer = ui.input(|i| i.pointer.hover_pos());
    let primary_clicked = ui.input(|i| i.pointer.primary_clicked());
    let primary_down = ui.input(|i| i.pointer.primary_down());
    let primary_released = ui.input(|i| i.pointer.primary_released());
    let shift_held = ui.input(|i| i.modifiers.shift);
    let secondary_clicked = ui.input(|i| i.pointer.secondary_clicked());

    // ── Click-to-select / deselect ──
    // Collect visible source rects for hit testing (top-to-bottom draw order,
    // reversed so topmost source wins).
    let active_scene_sources: Vec<(SourceId, Rect)> = state
        .active_scene()
        .map(|scene| {
            scene
                .sources
                .iter()
                .rev() // topmost source first
                .filter_map(|ss| {
                    state
                        .library
                        .iter()
                        .find(|s| s.id == ss.source_id)
                        .and_then(|lib| {
                            let visible = ss.resolve_visible(lib);
                            if !visible {
                                return None;
                            }
                            let transform = ss.resolve_transform(lib);
                            Some((
                                ss.source_id,
                                transform_to_screen_rect(&transform, viewport_rect, canvas_size),
                            ))
                        })
                })
                .collect()
        })
        .unwrap_or_default();

    // Only process selection on a fresh click (not drag continuation).
    if let Some(mouse_pos) = pointer {
        if primary_clicked && panel_rect.contains(mouse_pos) {
            // Check if we clicked a handle on the selected source first — don't re-select.
            let clicked_handle = state.selected_source_id.and_then(|sel_id| {
                let lib = state.library.iter().find(|s| s.id == sel_id)?;
                let scene = state.active_scene();
                let transform = scene
                    .and_then(|s| s.find_source(sel_id))
                    .map(|ss| ss.resolve_transform(lib))
                    .unwrap_or(lib.transform);
                let r = transform_to_screen_rect(&transform, viewport_rect, canvas_size);
                hit_test_handles(mouse_pos, r)
            });

            if clicked_handle.is_none() {
                // Find topmost source under the cursor.
                let hit = active_scene_sources
                    .iter()
                    .find(|(_, rect)| rect.contains(mouse_pos))
                    .map(|(id, _)| *id);

                state.selected_source_id = hit; // None = deselect
            }
        }

        // Right-click context menu: find source under cursor for context actions.
        if secondary_clicked && panel_rect.contains(mouse_pos) {
            let hit = active_scene_sources
                .iter()
                .find(|(_, rect)| rect.contains(mouse_pos))
                .map(|(id, _)| *id);
            if let Some(hit_id) = hit {
                state.selected_source_id = Some(hit_id);
            }
        }
    }

    // ── Context menu for right-click in preview ──
    // Track popup state manually to avoid egui popup API issues with
    // secondary_clicked + primary_clicked firing on the same frame (macOS trackpad).
    let ctx_state_id = egui::Id::new("preview_ctx_state");

    #[derive(Clone, Default)]
    struct CtxMenuState {
        open: bool,
        pos: Pos2,
        source: Option<crate::scene::SourceId>,
        /// Frame counter when opened — skip close checks on the opening frame.
        open_frame: u64,
    }

    let frame_nr = ui.ctx().cumulative_frame_nr();
    let mut ctx_state: CtxMenuState =
        ui.memory(|m| m.data.get_temp(ctx_state_id).unwrap_or_default());

    // Open on right-click over a source.
    if let Some(mouse_pos) = pointer
        && secondary_clicked
        && panel_rect.contains(mouse_pos)
        && state.selected_source_id.is_some()
    {
        ctx_state.open = true;
        ctx_state.pos = mouse_pos;
        ctx_state.source = state.selected_source_id;
        ctx_state.open_frame = frame_nr;
    }

    // Draw the menu if open.
    if ctx_state.open {
        if let Some(source_id) = ctx_state.source {
            let mut action_taken = false;
            let area_resp = egui::Area::new(egui::Id::new("preview_ctx_area"))
                .order(egui::Order::Foreground)
                .fixed_pos(ctx_state.pos)
                .show(ui.ctx(), |ui| {
                    egui::Frame::menu(ui.style()).show(ui, |ui| {
                        action_taken =
                            show_source_context_menu_items(ui, state, source_id, canvas_size);
                    });
                });

            // Close if an action was taken.
            if action_taken {
                ctx_state.open = false;
            }

            // Close if clicked outside the menu (but not on the frame it opened).
            if frame_nr > ctx_state.open_frame && !action_taken {
                let any_click =
                    ui.input(|i| i.pointer.primary_clicked() || i.pointer.secondary_clicked());
                let in_menu = area_resp
                    .response
                    .rect
                    .contains(pointer.unwrap_or(Pos2::ZERO));
                if any_click && !in_menu {
                    ctx_state.open = false;
                }
            }
        } else {
            ctx_state.open = false;
        }
    }

    let ctx_menu_open = ctx_state.open;
    ui.memory_mut(|m| m.data.insert_temp(ctx_state_id, ctx_state));

    // ── Handles + dragging for selected source ──
    let Some(selected_id) = state.selected_source_id else {
        return;
    };
    let Some(source) = state.library.iter().find(|s| s.id == selected_id) else {
        return;
    };
    let transform = state
        .active_scene()
        .and_then(|s| s.find_source(selected_id))
        .map(|ss| ss.resolve_transform(source))
        .unwrap_or(source.transform);
    let screen_rect = transform_to_screen_rect(&transform, viewport_rect, canvas_size);

    draw_handles(ui.painter(), screen_rect);

    // Drag state from egui memory
    let drag_id = egui::Id::new(("transform_drag", selected_id.0));
    let mut drag_mode: DragMode = ui.memory(|m| m.data.get_temp(drag_id).unwrap_or_default());

    if let Some(mouse_pos) = pointer {
        match &drag_mode {
            DragMode::None => {
                if primary_down && panel_rect.contains(mouse_pos) && !ctx_menu_open {
                    if let Some(handle) = hit_test_handles(mouse_pos, screen_rect) {
                        drag_mode = DragMode::Resize {
                            handle,
                            start_mouse: mouse_pos,
                            start_transform: transform,
                            aspect_ratio: transform.width / transform.height.max(1.0),
                        };
                    } else if screen_rect.contains(mouse_pos) {
                        drag_mode = DragMode::Move {
                            start_mouse: mouse_pos,
                            start_transform: transform,
                        };
                    }
                }
            }
            DragMode::Move {
                start_mouse,
                start_transform,
            } => {
                let delta = screen_to_canvas(mouse_pos, viewport_rect, canvas_size)
                    - screen_to_canvas(*start_mouse, viewport_rect, canvas_size);
                let mut new_transform = *start_transform;
                new_transform.x = start_transform.x + delta.x;
                new_transform.y = start_transform.y + delta.y;
                if let Some(scene) = state.active_scene_mut()
                    && let Some(ss) = scene.find_source_mut(selected_id)
                {
                    ss.overrides.transform = Some(new_transform);
                }
            }
            DragMode::Resize {
                handle,
                start_mouse,
                start_transform,
                aspect_ratio,
            } => {
                let delta = screen_to_canvas(mouse_pos, viewport_rect, canvas_size)
                    - screen_to_canvas(*start_mouse, viewport_rect, canvas_size);
                let mut new_transform = *start_transform;
                apply_resize(
                    &mut new_transform,
                    start_transform,
                    *handle,
                    delta,
                    shift_held,
                    *aspect_ratio,
                );
                if let Some(scene) = state.active_scene_mut()
                    && let Some(ss) = scene.find_source_mut(selected_id)
                {
                    ss.overrides.transform = Some(new_transform);
                }
            }
        }

        // Apply snapping if enabled and actively dragging.
        let is_dragging = !matches!(drag_mode, DragMode::None);
        if is_dragging && state.settings.general.snap_to_grid {
            let grid = state.settings.general.snap_grid_size;

            // Collect resolved transforms for other visible sources in the scene.
            let other_transforms: Vec<Transform> = state
                .active_scene()
                .map(|scene| {
                    scene
                        .sources
                        .iter()
                        .filter_map(|ss| {
                            if ss.source_id == selected_id {
                                return None;
                            }
                            state
                                .library
                                .iter()
                                .find(|s| s.id == ss.source_id)
                                .and_then(|lib| {
                                    if !ss.resolve_visible(lib) {
                                        return None;
                                    }
                                    Some(ss.resolve_transform(lib))
                                })
                        })
                        .collect()
                })
                .unwrap_or_default();
            let other_refs: Vec<&Transform> = other_transforms.iter().collect();

            // Read the current scene override transform, snap it, then write back.
            let current_transform = state
                .active_scene()
                .and_then(|s| s.find_source(selected_id))
                .and_then(|ss| ss.overrides.transform);
            if let Some(mut t) = current_transform {
                snap_transform(&mut t, canvas_size, grid, &other_refs);
                if let Some(scene) = state.active_scene_mut()
                    && let Some(ss) = scene.find_source_mut(selected_id)
                {
                    ss.overrides.transform = Some(t);
                }
                draw_snap_guides(ui.painter(), &t, canvas_size, viewport_rect, &other_refs);
            }
        }

        if primary_released && is_dragging {
            state.scenes_dirty = true;
            state.scenes_last_changed = std::time::Instant::now();
            drag_mode = DragMode::None;
        }
    }

    ui.memory_mut(|m| m.data.insert_temp(drag_id, drag_mode));
}

// ── Source Context Menu ────────────────────────────────────────────────────

/// Show a context menu via `response.context_menu()` — used by the sources panel.
pub fn show_source_context_menu(
    _ui: &mut egui::Ui,
    response: &egui::Response,
    state: &mut AppState,
    source_id: crate::scene::SourceId,
    canvas_size: Vec2,
) {
    response.context_menu(|ui| {
        show_source_context_menu_items(ui, state, source_id, canvas_size);
    });
}

/// The actual menu items — shared between `response.context_menu()` and the
/// manual popup used in the preview viewport. Returns `true` if an action was taken.
pub fn show_source_context_menu_items(
    ui: &mut egui::Ui,
    state: &mut AppState,
    source_id: crate::scene::SourceId,
    canvas_size: Vec2,
) -> bool {
    let cw = canvas_size.x;
    let ch = canvas_size.y;

    let (src_w, src_h) = state
        .library
        .iter()
        .find(|s| s.id == source_id)
        .map(|lib| {
            let transform = state
                .active_scene()
                .and_then(|s| s.find_source(source_id))
                .map(|ss| ss.resolve_transform(lib))
                .unwrap_or(lib.transform);
            (transform.width, transform.height)
        })
        .unwrap_or((cw, ch));
    let src_aspect = src_w / src_h.max(1.0);
    let canvas_aspect = cw / ch.max(1.0);

    // Determine which action was clicked (if any). We collect the click first,
    // then apply the mutation, to keep the layout code clean.
    #[derive(Clone, Copy)]
    enum Action {
        Fit,
        Stretch,
        Fill,
        Center,
        Reset,
    }
    let mut action: Option<Action> = None;

    use crate::ui::theme::{menu_item, styled_menu};
    styled_menu(ui, |ui| {
        if menu_item(ui, "Fit to Canvas") {
            action = Some(Action::Fit);
        }
        if menu_item(ui, "Stretch to Canvas") {
            action = Some(Action::Stretch);
        }
        if menu_item(ui, "Fill Canvas") {
            action = Some(Action::Fill);
        }
        ui.separator();
        if menu_item(ui, "Center on Canvas") {
            action = Some(Action::Center);
        }
        if menu_item(ui, "Reset Transform") {
            action = Some(Action::Reset);
        }
    });

    // Apply the action — compute the new transform locally, then write as scene override.
    if let Some(act) = action {
        // Read current resolved transform and native size from the library source.
        let lib_data = state.library.iter().find(|s| s.id == source_id).map(|lib| {
            let current = state
                .active_scene()
                .and_then(|s| s.find_source(source_id))
                .map(|ss| ss.resolve_transform(lib))
                .unwrap_or(lib.transform);
            (current, lib.native_size)
        });

        if let Some((current, native_size)) = lib_data {
            let new_transform = match act {
                Action::Fit => {
                    let (w, h) = if src_aspect > canvas_aspect {
                        (cw, cw / src_aspect)
                    } else {
                        (ch * src_aspect, ch)
                    };
                    Transform::new((cw - w) / 2.0, (ch - h) / 2.0, w, h)
                }
                Action::Stretch => Transform::new(0.0, 0.0, cw, ch),
                Action::Fill => {
                    let (w, h) = if src_aspect > canvas_aspect {
                        (ch * src_aspect, ch)
                    } else {
                        (cw, cw / src_aspect)
                    };
                    Transform::new((cw - w) / 2.0, (ch - h) / 2.0, w, h)
                }
                Action::Center => Transform::new(
                    (cw - current.width) / 2.0,
                    (ch - current.height) / 2.0,
                    current.width,
                    current.height,
                ),
                Action::Reset => {
                    let (nw, nh) = native_size;
                    Transform::new((cw - nw) / 2.0, (ch - nh) / 2.0, nw, nh)
                }
            };

            if let Some(scene) = state.active_scene_mut()
                && let Some(ss) = scene.find_source_mut(source_id)
            {
                ss.overrides.transform = Some(new_transform);
            }
        }
        mark_dirty(state);
    }
    action.is_some()
}

fn mark_dirty(state: &mut AppState) {
    state.scenes_dirty = true;
    state.scenes_last_changed = std::time::Instant::now();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canvas_to_screen_maps_origin() {
        let viewport = Rect::from_min_size(Pos2::new(100.0, 50.0), Vec2::new(960.0, 540.0));
        let canvas = Vec2::new(1920.0, 1080.0);
        let result = canvas_to_screen(Pos2::new(0.0, 0.0), viewport, canvas);
        assert!((result.x - 100.0).abs() < 0.01);
        assert!((result.y - 50.0).abs() < 0.01);
    }

    #[test]
    fn screen_to_canvas_roundtrip() {
        let viewport = Rect::from_min_size(Pos2::new(100.0, 50.0), Vec2::new(960.0, 540.0));
        let canvas = Vec2::new(1920.0, 1080.0);
        let original = Pos2::new(480.0, 270.0);
        let screen = canvas_to_screen(original, viewport, canvas);
        let back = screen_to_canvas(screen, viewport, canvas);
        assert!((back.x - original.x).abs() < 0.1);
        assert!((back.y - original.y).abs() < 0.1);
    }

    #[test]
    fn hit_test_corner() {
        let rect = Rect::from_min_max(Pos2::new(100.0, 100.0), Pos2::new(300.0, 200.0));
        let result = hit_test_handles(Pos2::new(100.0, 100.0), rect);
        assert_eq!(result, Some(HandlePosition::TopLeft));
    }

    #[test]
    fn hit_test_miss() {
        let rect = Rect::from_min_max(Pos2::new(100.0, 100.0), Pos2::new(300.0, 200.0));
        let result = hit_test_handles(Pos2::new(200.0, 150.0), rect);
        assert_eq!(result, None);
    }
}
