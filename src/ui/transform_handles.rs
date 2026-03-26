//! Interactive transform handles for repositioning and resizing sources in the preview.

use egui::{pos2, Pos2, Rect, Stroke, StrokeKind, Vec2};

use crate::scene::Transform;
use crate::state::AppState;
use crate::ui::theme::{BG_BASE, TEXT_PRIMARY};

const CORNER_SIZE: f32 = 8.0;
const EDGE_SIZE: f32 = 6.0;
const CORNER_HIT_SIZE: f32 = 16.0;
const EDGE_HIT_SIZE: f32 = 12.0;
const MIN_SOURCE_SIZE: f32 = 10.0;
/// Outer zone: pull source toward snap target (canvas pixels, scaled by zoom).
const SNAP_ATTRACT: f32 = 12.0;
/// Inner zone: resist leaving snap target (canvas pixels, scaled by zoom).
const SNAP_DEAD: f32 = 4.0;

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

/// Which edge of the source snapped to a target line.
#[derive(Clone, Copy, Debug, PartialEq)]
enum SnapEdgeX {
    Left,
    Right,
    Center,
}

/// Which edge of the source snapped to a target line.
#[derive(Clone, Copy, Debug, PartialEq)]
enum SnapEdgeY {
    Top,
    Bottom,
    Center,
}

#[derive(Clone, Debug, Default)]
enum DragMode {
    #[default]
    None,
    Move {
        start_mouse: Pos2,
        /// Start transforms for ALL selected sources.
        start_transforms: Vec<(crate::scene::SourceId, Transform)>,
        /// The source being directly dragged (snapping applies to this one).
        anchor_id: crate::scene::SourceId,
        /// Unsnapped X position of the anchor (tracks true mouse intent).
        raw_x: f32,
        /// Unsnapped Y position of the anchor.
        raw_y: f32,
        /// Active X snap: (snap line, which edge snapped).
        snapped_x: Option<(f32, SnapEdgeX)>,
        /// Active Y snap: (snap line, which edge snapped).
        snapped_y: Option<(f32, SnapEdgeY)>,
    },
    Resize {
        handle: HandlePosition,
        start_mouse: Pos2,
        start_transform: Transform,
        aspect_ratio: f32,
    },
    /// Rotation via Cmd+corner drag.
    Rotate {
        /// Source center in screen space.
        center: Pos2,
        /// Angle of initial click relative to center (radians).
        start_angle: f32,
        /// Source's rotation at drag start (degrees).
        start_rotation: f32,
    },
    /// Marquee (rubber-band) selection.
    Marquee {
        /// Screen-space start point.
        start: Pos2,
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

// ── Rotation Helpers ────────────────────────────────────────────────────────

/// Test if a point is inside a rotated rectangle by inverse-rotating the point
/// around the rect center.
fn point_in_rotated_rect(point: Pos2, rect: Rect, rotation_deg: f32) -> bool {
    if rotation_deg == 0.0 {
        return rect.contains(point);
    }
    let center = rect.center();
    let angle = -rotation_deg.to_radians();
    let cos = angle.cos();
    let sin = angle.sin();
    let p = point - center;
    let rotated = pos2(
        p.x * cos - p.y * sin + center.x,
        p.x * sin + p.y * cos + center.y,
    );
    rect.contains(rotated)
}

/// Compute rotated corner positions for a rectangle.
fn rotated_corners(rect: Rect, rotation_deg: f32) -> [Pos2; 4] {
    let center = rect.center();
    let angle = rotation_deg.to_radians();
    let cos = angle.cos();
    let sin = angle.sin();
    let corners = [
        rect.left_top(),
        rect.right_top(),
        rect.right_bottom(),
        rect.left_bottom(),
    ];
    corners.map(|c| {
        let p = c - center;
        pos2(
            p.x * cos - p.y * sin + center.x,
            p.x * sin + p.y * cos + center.y,
        )
    })
}

/// Rotate a single point around a center.
fn rotate_point(point: Pos2, center: Pos2, rotation_deg: f32) -> Pos2 {
    let angle = rotation_deg.to_radians();
    let cos = angle.cos();
    let sin = angle.sin();
    let p = point - center;
    pos2(
        p.x * cos - p.y * sin + center.x,
        p.x * sin + p.y * cos + center.y,
    )
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

fn draw_handles(painter: &egui::Painter, screen_rect: Rect, rotation_deg: f32) {
    if rotation_deg == 0.0 {
        // Fast path: axis-aligned (no rotation).
        painter.rect_stroke(
            screen_rect,
            0.0,
            egui::Stroke::new(1.0, TEXT_PRIMARY),
            StrokeKind::Outside,
        );

        for pos in corner_positions(screen_rect) {
            let r = Rect::from_center_size(pos, Vec2::splat(CORNER_SIZE));
            painter.rect_filled(r, 1.0, TEXT_PRIMARY);
            painter.rect_stroke(r, 1.0, egui::Stroke::new(1.0, BG_BASE), StrokeKind::Outside);
        }

        for pos in edge_positions(screen_rect) {
            let r = Rect::from_center_size(pos, Vec2::splat(EDGE_SIZE));
            painter.rect_filled(r, 1.0, TEXT_PRIMARY);
            painter.rect_stroke(r, 1.0, egui::Stroke::new(1.0, BG_BASE), StrokeKind::Outside);
        }
    } else {
        // Rotated path: draw outline as 4 line segments between rotated corners.
        let corners = rotated_corners(screen_rect, rotation_deg);
        let outline_stroke = egui::Stroke::new(1.0, TEXT_PRIMARY);
        for i in 0..4 {
            painter.line_segment([corners[i], corners[(i + 1) % 4]], outline_stroke);
        }

        // Corner handles at rotated positions.
        for &pos in &corners {
            let r = Rect::from_center_size(pos, Vec2::splat(CORNER_SIZE));
            painter.rect_filled(r, 1.0, TEXT_PRIMARY);
            painter.rect_stroke(r, 1.0, egui::Stroke::new(1.0, BG_BASE), StrokeKind::Outside);
        }

        // Edge handles at rotated midpoints.
        let center = screen_rect.center();
        let edge_unrotated = edge_positions(screen_rect);
        for pos in edge_unrotated {
            let rotated_pos = rotate_point(pos, center, rotation_deg);
            let r = Rect::from_center_size(rotated_pos, Vec2::splat(EDGE_SIZE));
            painter.rect_filled(r, 1.0, TEXT_PRIMARY);
            painter.rect_stroke(r, 1.0, egui::Stroke::new(1.0, BG_BASE), StrokeKind::Outside);
        }
    }
}

// ── Hit Testing ─────────────────────────────────────────────────────────────

fn hit_test_handles(pos: Pos2, screen_rect: Rect, rotation_deg: f32) -> Option<HandlePosition> {
    if rotation_deg == 0.0 {
        // Fast path: axis-aligned.
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
    } else {
        // Rotated path: test handles at rotated positions.
        let rot_corners = rotated_corners(screen_rect, rotation_deg);
        let corner_handles = [
            HandlePosition::TopLeft,
            HandlePosition::TopRight,
            HandlePosition::BottomRight,
            HandlePosition::BottomLeft,
        ];
        for (i, &corner) in rot_corners.iter().enumerate() {
            if Rect::from_center_size(corner, Vec2::splat(CORNER_HIT_SIZE)).contains(pos) {
                return Some(corner_handles[i]);
            }
        }

        let center = screen_rect.center();
        let edge_unrotated = edge_positions(screen_rect);
        let edge_handles = [
            HandlePosition::Top,
            HandlePosition::Bottom,
            HandlePosition::Left,
            HandlePosition::Right,
        ];
        for (i, &edge) in edge_unrotated.iter().enumerate() {
            let rotated_edge = rotate_point(edge, center, rotation_deg);
            if Rect::from_center_size(rotated_edge, Vec2::splat(EDGE_HIT_SIZE)).contains(pos) {
                return Some(edge_handles[i]);
            }
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

/// Result of snap computation for a single axis.
struct SnapResult {
    /// The target X snap line (None = no snap).
    snapped_x: Option<(f32, SnapEdgeX)>,
    /// The target Y snap line (None = no snap).
    snapped_y: Option<(f32, SnapEdgeY)>,
    /// Delta to apply to transform.x to snap.
    offset_x: f32,
    /// Delta to apply to transform.y to snap.
    offset_y: f32,
}

/// Build snap targets and find the closest match for the given transform.
fn compute_snap(
    transform: &Transform,
    canvas_size: Vec2,
    grid_size: f32,
    other_sources: &[&Transform],
    attract: f32,
    extra_x: &[f32],
    extra_y: &[f32],
) -> SnapResult {
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

    // Extra targets from guides, thirds, safe zones.
    x_targets.extend_from_slice(extra_x);
    y_targets.extend_from_slice(extra_y);

    // Other source edges and centers.
    for other in other_sources {
        x_targets.extend_from_slice(&[
            other.x,
            other.x + other.width,
            other.x + other.width / 2.0,
        ]);
        y_targets.extend_from_slice(&[
            other.y,
            other.y + other.height,
            other.y + other.height / 2.0,
        ]);
    }

    // Source edges.
    let left = transform.x;
    let right = transform.x + transform.width;
    let center_x = transform.x + transform.width / 2.0;
    let top = transform.y;
    let bottom = transform.y + transform.height;
    let center_y = transform.y + transform.height / 2.0;

    // Find closest X snap.
    let mut best_x: Option<(f32, SnapEdgeX, f32)> = None; // (snap_line, edge, distance)
    let x_edges = [
        (left, SnapEdgeX::Left),
        (right, SnapEdgeX::Right),
        (center_x, SnapEdgeX::Center),
    ];
    for &(edge_val, edge_kind) in &x_edges {
        for &target in &x_targets {
            let dist = (edge_val - target).abs();
            if dist < attract {
                let is_better = best_x.map_or(true, |(_, _, bd)| dist < bd);
                if is_better {
                    best_x = Some((target, edge_kind, dist));
                }
            }
        }
    }

    // Find closest Y snap.
    let mut best_y: Option<(f32, SnapEdgeY, f32)> = None;
    let y_edges = [
        (top, SnapEdgeY::Top),
        (bottom, SnapEdgeY::Bottom),
        (center_y, SnapEdgeY::Center),
    ];
    for &(edge_val, edge_kind) in &y_edges {
        for &target in &y_targets {
            let dist = (edge_val - target).abs();
            if dist < attract {
                let is_better = best_y.map_or(true, |(_, _, bd)| dist < bd);
                if is_better {
                    best_y = Some((target, edge_kind, dist));
                }
            }
        }
    }

    // Compute offsets.
    let (snapped_x, offset_x) = match best_x {
        Some((line, SnapEdgeX::Left, _)) => (Some((line, SnapEdgeX::Left)), line - left),
        Some((line, SnapEdgeX::Right, _)) => (Some((line, SnapEdgeX::Right)), line - right),
        Some((line, SnapEdgeX::Center, _)) => (Some((line, SnapEdgeX::Center)), line - center_x),
        None => (None, 0.0),
    };

    let (snapped_y, offset_y) = match best_y {
        Some((line, SnapEdgeY::Top, _)) => (Some((line, SnapEdgeY::Top)), line - top),
        Some((line, SnapEdgeY::Bottom, _)) => (Some((line, SnapEdgeY::Bottom)), line - bottom),
        Some((line, SnapEdgeY::Center, _)) => (Some((line, SnapEdgeY::Center)), line - center_y),
        None => (None, 0.0),
    };

    SnapResult {
        snapped_x,
        snapped_y,
        offset_x,
        offset_y,
    }
}

/// Compute the raw edge position for a given snap edge kind.
fn raw_edge_x(raw_x: f32, width: f32, edge: SnapEdgeX) -> f32 {
    match edge {
        SnapEdgeX::Left => raw_x,
        SnapEdgeX::Right => raw_x + width,
        SnapEdgeX::Center => raw_x + width / 2.0,
    }
}

/// Compute the raw edge position for a given snap edge kind.
fn raw_edge_y(raw_y: f32, height: f32, edge: SnapEdgeY) -> f32 {
    match edge {
        SnapEdgeY::Top => raw_y,
        SnapEdgeY::Bottom => raw_y + height,
        SnapEdgeY::Center => raw_y + height / 2.0,
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
    let dock_drag_active: bool = ui.ctx().data(|d| {
        d.get_temp(egui::Id::new("dock_drag_active"))
            .unwrap_or(false)
    });
    if dock_drag_active {
        return;
    }

    let pointer = ui.input(|i| i.pointer.hover_pos());
    let primary_clicked = ui.input(|i| i.pointer.primary_clicked());
    let primary_down = ui.input(|i| i.pointer.primary_down());
    let primary_released = ui.input(|i| i.pointer.primary_released());
    let shift_held = ui.input(|i| i.modifiers.shift);
    let alt_held = ui.input(|i| i.modifiers.alt);
    let secondary_clicked = ui.input(|i| i.pointer.secondary_clicked());

    // ── Click-to-select / deselect ──
    // Collect visible source rects for hit testing (top-to-bottom draw order,
    // reversed so topmost source wins).
    let active_scene_sources: Vec<(SourceId, Rect, f32)> = state
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
                                transform.rotation,
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
            let clicked_handle = state.selected_source_id().and_then(|sel_id| {
                let lib = state.library.iter().find(|s| s.id == sel_id)?;
                let scene = state.active_scene();
                let transform = scene
                    .and_then(|s| s.find_source(sel_id))
                    .map(|ss| ss.resolve_transform(lib))
                    .unwrap_or(lib.transform);
                let r = transform_to_screen_rect(&transform, viewport_rect, canvas_size);
                hit_test_handles(mouse_pos, r, transform.rotation)
            });

            if clicked_handle.is_none() {
                // Find topmost source under the cursor.
                let hit = active_scene_sources
                    .iter()
                    .find(|(_, rect, rot)| point_in_rotated_rect(mouse_pos, *rect, *rot))
                    .map(|(id, _, _)| *id);

                if let Some(hit_id) = hit {
                    if shift_held {
                        state.toggle_source_selection(hit_id);
                    } else {
                        state.select_source(hit_id);
                    }
                    state.selected_library_source_id = None;
                } else {
                    // Clicked empty space — deselect (unless shift held).
                    if !shift_held {
                        state.deselect_all();
                    }
                }
            }
        }

        // Right-click context menu: find source under cursor for context actions.
        if secondary_clicked && panel_rect.contains(mouse_pos) {
            let hit = active_scene_sources
                .iter()
                .find(|(_, rect, rot)| point_in_rotated_rect(mouse_pos, *rect, *rot))
                .map(|(id, _, _)| *id);
            if let Some(hit_id) = hit {
                state.select_source(hit_id);
                state.selected_library_source_id = None;
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
        && state.selected_source_id().is_some()
    {
        ctx_state.open = true;
        ctx_state.pos = mouse_pos;
        ctx_state.source = state.selected_source_id();
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

    // ── Flash outline for library-selected source ──
    if let Some(flash_id) = state.flash_source_id
        && let Some(start) = state.flash_start
    {
        let elapsed = start.elapsed().as_secs_f32();
        let duration = 0.6;
        if elapsed < duration {
            // Find the source's resolved transform in the active scene.
            let flash_transform = state.active_scene().and_then(|scene| {
                let ss = scene.find_source(flash_id)?;
                let lib = state.library.iter().find(|s| s.id == flash_id)?;
                Some(ss.resolve_transform(lib))
            });
            if let Some(t) = flash_transform {
                let r = transform_to_screen_rect(&t, viewport_rect, canvas_size);
                let alpha = 1.0 - elapsed / duration;
                let accent = crate::ui::theme::accent_color_ui(ui);
                let color = egui::Color32::from_rgba_unmultiplied(
                    accent.r(),
                    accent.g(),
                    accent.b(),
                    (255.0 * alpha) as u8,
                );
                ui.painter().rect_stroke(
                    r,
                    0.0,
                    Stroke::new(2.0, color),
                    egui::StrokeKind::Outside,
                );
                ui.ctx().request_repaint();
            }
        }
    }

    // ── Draw selection outlines for all selected sources ──
    let selected_ids = state.selected_source_ids.clone();
    let primary_id = state.primary_selected_id;
    for &sel_id in &selected_ids {
        let Some(lib) = state.library.iter().find(|s| s.id == sel_id) else {
            continue;
        };
        let t = state
            .active_scene()
            .and_then(|s| s.find_source(sel_id))
            .map(|ss| ss.resolve_transform(lib))
            .unwrap_or(lib.transform);
        let r = transform_to_screen_rect(&t, viewport_rect, canvas_size);

        if primary_id == Some(sel_id) {
            // Primary selected: draw full handles (outline + corners + edges).
            draw_handles(ui.painter(), r, t.rotation);
        } else {
            // Non-primary selected: just the outline.
            if t.rotation == 0.0 {
                ui.painter().rect_stroke(
                    r,
                    0.0,
                    egui::Stroke::new(1.0, TEXT_PRIMARY),
                    StrokeKind::Outside,
                );
            } else {
                let corners = rotated_corners(r, t.rotation);
                let outline_stroke = egui::Stroke::new(1.0, TEXT_PRIMARY);
                for i in 0..4 {
                    ui.painter()
                        .line_segment([corners[i], corners[(i + 1) % 4]], outline_stroke);
                }
            }
        }
    }

    // ── Handles + dragging for selected source ──
    let Some(selected_id) = state.selected_source_id() else {
        // No primary selection — still need to handle marquee drag.
        let drag_id = egui::Id::new("transform_drag_marquee");
        let mut drag_mode: DragMode =
            ui.memory(|m| m.data.get_temp(drag_id).unwrap_or_default());

        if let Some(mouse_pos) = pointer {
            match &drag_mode {
                DragMode::None => {
                    if primary_down && panel_rect.contains(mouse_pos) && !ctx_menu_open {
                        // No source hit, no selection — start marquee.
                        let hit = active_scene_sources
                            .iter()
                            .find(|(_, rect, rot)| point_in_rotated_rect(mouse_pos, *rect, *rot))
                            .map(|(id, _, _)| *id);
                        if hit.is_none() {
                            drag_mode = DragMode::Marquee { start: mouse_pos };
                        }
                    }
                }
                DragMode::Marquee { start } => {
                    let marquee_rect = Rect::from_two_pos(*start, mouse_pos);
                    // Draw marquee rectangle.
                    let accent = crate::ui::theme::accent_color_ui(ui);
                    let fill = egui::Color32::from_rgba_unmultiplied(
                        accent.r(),
                        accent.g(),
                        accent.b(),
                        25, // ~10% opacity
                    );
                    ui.painter().rect_filled(marquee_rect, 0.0, fill);
                    ui.painter().rect_stroke(
                        marquee_rect,
                        0.0,
                        Stroke::new(1.0, accent),
                        StrokeKind::Outside,
                    );
                    ui.ctx().request_repaint();
                }
                _ => {}
            }

            if primary_released && matches!(drag_mode, DragMode::Marquee { .. }) {
                if let DragMode::Marquee { start } = drag_mode {
                    let marquee_rect = Rect::from_two_pos(start, mouse_pos);
                    // Select all non-locked sources whose screen rects intersect the marquee.
                    if !shift_held {
                        state.deselect_all();
                    }
                    for (src_id, src_rect, _) in &active_scene_sources {
                        if marquee_rect.intersects(*src_rect) {
                            // Skip locked sources.
                            let is_locked = state
                                .active_scene()
                                .and_then(|s| s.find_source(*src_id))
                                .map(|ss| ss.resolve_locked())
                                .unwrap_or(false);
                            if !is_locked && !state.is_source_selected(*src_id) {
                                state.toggle_source_selection(*src_id);
                            }
                        }
                    }
                }
                drag_mode = DragMode::None;
            }
        }

        ui.memory_mut(|m| m.data.insert_temp(drag_id, drag_mode));
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

    // Drag state from egui memory
    let drag_id = egui::Id::new("transform_drag_main");
    let mut drag_mode: DragMode = ui.memory(|m| m.data.get_temp(drag_id).unwrap_or_default());

    if let Some(mouse_pos) = pointer {
        match &mut drag_mode {
            DragMode::None => {
                if primary_down && panel_rect.contains(mouse_pos) && !ctx_menu_open {
                    if let Some(handle) = hit_test_handles(mouse_pos, screen_rect, transform.rotation) {
                        let is_corner = matches!(
                            handle,
                            HandlePosition::TopLeft
                                | HandlePosition::TopRight
                                | HandlePosition::BottomLeft
                                | HandlePosition::BottomRight
                        );
                        let cmd_held = ui.input(|i| i.modifiers.command);
                        if is_corner && cmd_held {
                            let center = screen_rect.center();
                            let angle = (mouse_pos - center).angle();
                            state.begin_continuous_edit();
                            drag_mode = DragMode::Rotate {
                                center,
                                start_angle: angle,
                                start_rotation: transform.rotation,
                            };
                        } else {
                            state.begin_continuous_edit();
                            drag_mode = DragMode::Resize {
                                handle,
                                start_mouse: mouse_pos,
                                start_transform: transform,
                                aspect_ratio: transform.width / transform.height.max(1.0),
                            };
                        }
                    } else {
                        // Check if we clicked on any selected source (for group move).
                        let clicked_selected = selected_ids.iter().any(|&sid| {
                            state
                                .library
                                .iter()
                                .find(|s| s.id == sid)
                                .and_then(|lib| {
                                    let t = state
                                        .active_scene()
                                        .and_then(|s| s.find_source(sid))
                                        .map(|ss| ss.resolve_transform(lib))
                                        .unwrap_or(lib.transform);
                                    let r =
                                        transform_to_screen_rect(&t, viewport_rect, canvas_size);
                                    Some(r.contains(mouse_pos))
                                })
                                .unwrap_or(false)
                        });

                        if clicked_selected || screen_rect.contains(mouse_pos) {
                            // Determine which source is the anchor (the one under cursor).
                            let anchor = selected_ids
                                .iter()
                                .find(|&&sid| {
                                    state
                                        .library
                                        .iter()
                                        .find(|s| s.id == sid)
                                        .and_then(|lib| {
                                            let t = state
                                                .active_scene()
                                                .and_then(|s| s.find_source(sid))
                                                .map(|ss| ss.resolve_transform(lib))
                                                .unwrap_or(lib.transform);
                                            let r = transform_to_screen_rect(
                                                &t,
                                                viewport_rect,
                                                canvas_size,
                                            );
                                            Some(r.contains(mouse_pos))
                                        })
                                        .unwrap_or(false)
                                })
                                .copied()
                                .unwrap_or(selected_id);

                            // Capture start transforms for ALL selected sources.
                            let start_transforms: Vec<(SourceId, Transform)> = selected_ids
                                .iter()
                                .filter_map(|&sid| {
                                    let lib = state.library.iter().find(|s| s.id == sid)?;
                                    let t = state
                                        .active_scene()
                                        .and_then(|s| s.find_source(sid))
                                        .map(|ss| ss.resolve_transform(lib))
                                        .unwrap_or(lib.transform);
                                    Some((sid, t))
                                })
                                .collect();

                            let anchor_transform = start_transforms
                                .iter()
                                .find(|(sid, _)| *sid == anchor)
                                .map(|(_, t)| *t)
                                .unwrap_or(transform);

                            state.begin_continuous_edit();
                            drag_mode = DragMode::Move {
                                start_mouse: mouse_pos,
                                start_transforms,
                                anchor_id: anchor,
                                raw_x: anchor_transform.x,
                                raw_y: anchor_transform.y,
                                snapped_x: None,
                                snapped_y: None,
                            };
                        } else {
                            // Clicked empty space — start marquee.
                            drag_mode = DragMode::Marquee { start: mouse_pos };
                        }
                    }
                }
            }
            DragMode::Move {
                start_mouse,
                start_transforms,
                anchor_id,
                raw_x,
                raw_y,
                snapped_x,
                snapped_y,
            } => {
                let delta = screen_to_canvas(mouse_pos, viewport_rect, canvas_size)
                    - screen_to_canvas(*start_mouse, viewport_rect, canvas_size);

                // Find anchor's start transform.
                let anchor_start = start_transforms
                    .iter()
                    .find(|(sid, _)| *sid == *anchor_id)
                    .map(|(_, t)| *t)
                    .unwrap_or(transform);

                // 1. Compute raw (unsnapped) position from mouse delta for anchor.
                let new_raw_x = anchor_start.x + delta.x;
                let new_raw_y = anchor_start.y + delta.y;
                *raw_x = new_raw_x;
                *raw_y = new_raw_y;

                let mut final_x = new_raw_x;
                let mut final_y = new_raw_y;

                // 2. Apply magnetic snapping if enabled and Alt is not held.
                //    Snapping only applies to the anchor source.
                let snap_enabled = state.settings.general.snap_to_grid && !alt_held;
                if snap_enabled {
                    let grid = state.settings.general.snap_grid_size;
                    let scale = 1.0; // Will be replaced with 1.0 / zoom in Task 5
                    let attract = SNAP_ATTRACT * scale;
                    let dead = SNAP_DEAD * scale;

                    let width = anchor_start.width;
                    let height = anchor_start.height;

                    // Collect other source transforms (excluding all selected sources).
                    let selected_set: Vec<SourceId> = start_transforms
                        .iter()
                        .map(|(sid, _)| *sid)
                        .collect();
                    let other_transforms: Vec<Transform> = state
                        .active_scene()
                        .map(|scene| {
                            scene
                                .sources
                                .iter()
                                .filter_map(|ss| {
                                    if selected_set.contains(&ss.source_id) {
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

                    // Build extra snap targets from guides, thirds, and safe zones.
                    let mut extra_x: Vec<f32> = Vec::new();
                    let mut extra_y: Vec<f32> = Vec::new();

                    // Custom guides
                    if state.settings.general.show_guides {
                        if let Some(scene) = state.active_scene() {
                            for guide in &scene.guides {
                                match guide.axis {
                                    crate::scene::GuideAxis::Vertical => extra_x.push(guide.position),
                                    crate::scene::GuideAxis::Horizontal => extra_y.push(guide.position),
                                }
                            }
                        }
                    }

                    // Thirds lines
                    if state.settings.general.show_thirds {
                        let cw = canvas_size.x;
                        let ch = canvas_size.y;
                        extra_x.extend_from_slice(&[cw / 3.0, 2.0 * cw / 3.0]);
                        extra_y.extend_from_slice(&[ch / 3.0, 2.0 * ch / 3.0]);
                    }

                    // Safe zone boundaries
                    if state.settings.general.show_safe_zones {
                        let cw = canvas_size.x;
                        let ch = canvas_size.y;
                        // Action-safe: 5%, 95%
                        extra_x.extend_from_slice(&[cw * 0.05, cw * 0.95]);
                        extra_y.extend_from_slice(&[ch * 0.05, ch * 0.95]);
                        // Title-safe: 10%, 90%
                        extra_x.extend_from_slice(&[cw * 0.10, cw * 0.90]);
                        extra_y.extend_from_slice(&[ch * 0.10, ch * 0.90]);
                    }

                    // Build a raw transform for snap computation.
                    let raw_transform = Transform::new(new_raw_x, new_raw_y, width, height);
                    let snap = compute_snap(&raw_transform, canvas_size, grid, &other_refs, attract, &extra_x, &extra_y);

                    // X axis: magnetic two-zone logic.
                    if let Some((line, edge)) = snapped_x {
                        let raw_edge = raw_edge_x(new_raw_x, width, *edge);
                        if (raw_edge - *line).abs() > dead {
                            *snapped_x = None;
                            final_x = new_raw_x;
                        } else {
                            final_x = match edge {
                                SnapEdgeX::Left => *line,
                                SnapEdgeX::Right => *line - width,
                                SnapEdgeX::Center => *line - width / 2.0,
                            };
                        }
                    } else if let Some((line, edge)) = snap.snapped_x {
                        *snapped_x = Some((line, edge));
                        final_x = new_raw_x + snap.offset_x;
                    }

                    // Y axis: magnetic two-zone logic.
                    if let Some((line, edge)) = snapped_y {
                        let raw_edge = raw_edge_y(new_raw_y, height, *edge);
                        if (raw_edge - *line).abs() > dead {
                            *snapped_y = None;
                            final_y = new_raw_y;
                        } else {
                            final_y = match edge {
                                SnapEdgeY::Top => *line,
                                SnapEdgeY::Bottom => *line - height,
                                SnapEdgeY::Center => *line - height / 2.0,
                            };
                        }
                    } else if let Some((line, edge)) = snap.snapped_y {
                        *snapped_y = Some((line, edge));
                        final_y = new_raw_y + snap.offset_y;
                    }
                } else {
                    *snapped_x = None;
                    *snapped_y = None;
                }

                // 3. Compute delta from anchor's start to final position, apply to all.
                let anchor_delta_x = final_x - anchor_start.x;
                let anchor_delta_y = final_y - anchor_start.y;

                for (sid, start_t) in start_transforms.iter() {
                    let mut new_t = *start_t;
                    new_t.x = start_t.x + anchor_delta_x;
                    new_t.y = start_t.y + anchor_delta_y;
                    if let Some(scene) = state.active_scene_mut()
                        && let Some(ss) = scene.find_source_mut(*sid)
                    {
                        ss.overrides.transform = Some(new_t);
                    }
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
                let locked = state
                    .library
                    .iter()
                    .find(|s| s.id == selected_id)
                    .map(|s| s.aspect_ratio_locked)
                    .unwrap_or(false);
                apply_resize(
                    &mut new_transform,
                    start_transform,
                    *handle,
                    delta,
                    shift_held || locked,
                    *aspect_ratio,
                );
                if let Some(scene) = state.active_scene_mut()
                    && let Some(ss) = scene.find_source_mut(selected_id)
                {
                    ss.overrides.transform = Some(new_transform);
                }
            }
            DragMode::Rotate {
                center,
                start_angle,
                start_rotation,
                ..
            } => {
                let current_angle = (mouse_pos - *center).angle();
                let delta_rad = current_angle - *start_angle;
                let delta_deg = delta_rad.to_degrees();
                let mut rotation = (*start_rotation + delta_deg).rem_euclid(360.0);
                if shift_held {
                    rotation = (rotation / 15.0).round() * 15.0;
                }
                if let Some(scene) = state.active_scene_mut()
                    && let Some(ss) = scene.find_source_mut(selected_id)
                {
                    if let Some(ref mut t) = ss.overrides.transform {
                        t.rotation = rotation;
                    } else {
                        let mut new_t = transform;
                        new_t.rotation = rotation;
                        ss.overrides.transform = Some(new_t);
                    }
                }
            }
            DragMode::Marquee { start } => {
                let marquee_rect = Rect::from_two_pos(*start, mouse_pos);
                // Draw marquee rectangle.
                let accent = crate::ui::theme::accent_color_ui(ui);
                let fill = egui::Color32::from_rgba_unmultiplied(
                    accent.r(),
                    accent.g(),
                    accent.b(),
                    25, // ~10% opacity
                );
                ui.painter().rect_filled(marquee_rect, 0.0, fill);
                ui.painter().rect_stroke(
                    marquee_rect,
                    0.0,
                    Stroke::new(1.0, accent),
                    StrokeKind::Outside,
                );
                ui.ctx().request_repaint();
            }
        }

        // Draw snap guides when actively snapped during a move.
        if let DragMode::Move {
            anchor_id,
            snapped_x,
            snapped_y,
            ..
        } = &drag_mode
        {
            if snapped_x.is_some() || snapped_y.is_some() {
                let anchor = *anchor_id;
                // Collect other source transforms for guide drawing (excluding selected).
                let other_transforms: Vec<Transform> = state
                    .active_scene()
                    .map(|scene| {
                        scene
                            .sources
                            .iter()
                            .filter_map(|ss| {
                                if state.is_source_selected(ss.source_id) {
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

                if let Some(t) = state
                    .active_scene()
                    .and_then(|s| s.find_source(anchor))
                    .and_then(|ss| ss.overrides.transform)
                {
                    draw_snap_guides(ui.painter(), &t, canvas_size, viewport_rect, &other_refs);
                }
            }
        }

        // Handle marquee release.
        if primary_released && matches!(drag_mode, DragMode::Marquee { .. }) {
            if let DragMode::Marquee { start } = drag_mode {
                let marquee_rect = Rect::from_two_pos(start, mouse_pos);
                if !shift_held {
                    state.deselect_all();
                }
                for (src_id, src_rect, _) in &active_scene_sources {
                    if marquee_rect.intersects(*src_rect) {
                        let is_locked = state
                            .active_scene()
                            .and_then(|s| s.find_source(*src_id))
                            .map(|ss| ss.resolve_locked())
                            .unwrap_or(false);
                        if !is_locked && !state.is_source_selected(*src_id) {
                            state.toggle_source_selection(*src_id);
                        }
                    }
                }
            }
            drag_mode = DragMode::None;
        }

        let is_dragging = !matches!(drag_mode, DragMode::None);
        if primary_released && is_dragging {
            state.end_continuous_edit();
            state.mark_dirty();
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
        ResetSize,
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
        if menu_item(ui, "Reset Size") {
            action = Some(Action::ResetSize);
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
                Action::ResetSize => {
                    let (nw, nh) = native_size;
                    Transform::new(current.x, current.y, nw, nh)
                }
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
        state.mark_dirty();
    }
    action.is_some()
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
        let result = hit_test_handles(Pos2::new(100.0, 100.0), rect, 0.0);
        assert_eq!(result, Some(HandlePosition::TopLeft));
    }

    #[test]
    fn hit_test_miss() {
        let rect = Rect::from_min_max(Pos2::new(100.0, 100.0), Pos2::new(300.0, 200.0));
        let result = hit_test_handles(Pos2::new(200.0, 150.0), rect, 0.0);
        assert_eq!(result, None);
    }

    #[test]
    fn point_in_rotated_rect_no_rotation() {
        let rect = Rect::from_min_max(Pos2::new(100.0, 100.0), Pos2::new(200.0, 200.0));
        assert!(point_in_rotated_rect(Pos2::new(150.0, 150.0), rect, 0.0));
        assert!(!point_in_rotated_rect(Pos2::new(50.0, 50.0), rect, 0.0));
    }

    #[test]
    fn point_in_rotated_rect_90_degrees() {
        // A 100x50 rect centered at (150, 125), rotated 90 degrees becomes 50x100.
        let rect = Rect::from_min_max(Pos2::new(100.0, 100.0), Pos2::new(200.0, 150.0));
        let center = rect.center(); // (150, 125)
        // Point at (150, 125) — center — should always be inside.
        assert!(point_in_rotated_rect(center, rect, 90.0));
        // Point that would be inside the rotated rect but outside the original.
        // After 90-deg rotation, width and height swap.
        assert!(point_in_rotated_rect(Pos2::new(150.0, 90.0), rect, 90.0));
        // Point far outside.
        assert!(!point_in_rotated_rect(Pos2::new(50.0, 50.0), rect, 90.0));
    }

    #[test]
    fn rotated_corners_identity() {
        let rect = Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(100.0, 100.0));
        let corners = rotated_corners(rect, 0.0);
        assert!((corners[0].x - 0.0).abs() < 0.01);
        assert!((corners[0].y - 0.0).abs() < 0.01);
        assert!((corners[1].x - 100.0).abs() < 0.01);
        assert!((corners[1].y - 0.0).abs() < 0.01);
    }
}
