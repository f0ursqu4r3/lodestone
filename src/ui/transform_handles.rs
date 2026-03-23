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

// ── Public Entry Point ──────────────────────────────────────────────────────

/// Draw transform handles and process drag interactions for the selected source.
pub fn draw_transform_handles(
    ui: &mut egui::Ui,
    state: &mut AppState,
    viewport_rect: Rect,
    canvas_size: Vec2,
) {
    let Some(selected_id) = state.selected_source_id else {
        return;
    };
    let Some(source) = state.sources.iter().find(|s| s.id == selected_id) else {
        return;
    };
    let transform = source.transform;
    let screen_rect = transform_to_screen_rect(&transform, viewport_rect, canvas_size);

    draw_handles(ui.painter(), screen_rect);

    // Drag state from egui memory
    let drag_id = egui::Id::new(("transform_drag", selected_id.0));
    let mut drag_mode: DragMode = ui.memory(|m| m.data.get_temp(drag_id).unwrap_or_default());

    let pointer = ui.input(|i| i.pointer.hover_pos());
    let primary_down = ui.input(|i| i.pointer.primary_down());
    let primary_released = ui.input(|i| i.pointer.primary_released());
    let shift_held = ui.input(|i| i.modifiers.shift);

    if let Some(mouse_pos) = pointer {
        match &drag_mode {
            DragMode::None => {
                if primary_down && viewport_rect.contains(mouse_pos) {
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
                if let Some(s) = state.sources.iter_mut().find(|s| s.id == selected_id) {
                    s.transform.x = start_transform.x + delta.x;
                    s.transform.y = start_transform.y + delta.y;
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
                if let Some(s) = state.sources.iter_mut().find(|s| s.id == selected_id) {
                    apply_resize(
                        &mut s.transform,
                        start_transform,
                        *handle,
                        delta,
                        shift_held,
                        *aspect_ratio,
                    );
                }
            }
        }

        if primary_released && !matches!(drag_mode, DragMode::None) {
            state.scenes_dirty = true;
            state.scenes_last_changed = std::time::Instant::now();
            drag_mode = DragMode::None;
        }
    }

    ui.memory_mut(|m| m.data.insert_temp(drag_id, drag_mode));
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
