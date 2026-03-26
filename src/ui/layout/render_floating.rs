//! Floating window chrome rendering for dockview floating groups.

use super::render::LayoutAction;
use super::render_tabs::{TabBarContext, render_content, render_tab_bar};
use super::tree::DockLayout;

use crate::ui::theme::{FLOATING_HEADER_HEIGHT, FLOATING_MIN_SIZE, TAB_BAR_HEIGHT, active_theme};

/// Render a floating panel container with custom chrome header, then delegate
/// to the shared `render_tab_bar()` and `render_content()` for the panel group.
pub(crate) fn render_floating_chrome(
    ctx: &egui::Context,
    layout: &DockLayout,
    fg: &super::tree::FloatingGroup,
    group: &super::tree::Group,
    state: &mut crate::state::AppState,
    actions: &mut Vec<LayoutAction>,
    is_main: bool,
) {
    let theme = active_theme(ctx);
    let group_id = fg.group_id;

    // Collapsed state — when collapsed, only show the chrome header
    let collapsed_id = egui::Id::new(("floating_collapsed", group_id.0));
    let is_collapsed: bool = ctx.data(|d| d.get_temp(collapsed_id).unwrap_or(false));

    // fg.size represents the total interior (tab bar + content), matching the old
    // egui::Window default_size semantics. We add only the chrome header on top.
    let total_height = if is_collapsed {
        FLOATING_HEADER_HEIGHT
    } else {
        FLOATING_HEADER_HEIGHT + fg.size.y
    };
    let total_size = egui::vec2(fg.size.x, total_height);

    let outer_rect = egui::Rect::from_min_size(fg.pos, total_size);

    // --- Shadow ---
    // Shadow at Middle order so it appears above docked panels but below
    // the floating panel content (which is at Foreground).
    let shadow_layer = egui::LayerId::new(
        egui::Order::Middle,
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

    // --- Border (at Foreground, same as the floating content) ---
    let border_layer = egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new(("floating_border", group_id.0)),
    );
    let border_painter = ctx.layer_painter(border_layer);
    border_painter.rect_stroke(
        outer_rect,
        0.0,
        egui::Stroke::new(1.0, theme.border),
        egui::StrokeKind::Inside,
    );

    // --- Chrome header (collapse, title, close) ---
    let chrome_rect =
        egui::Rect::from_min_size(fg.pos, egui::vec2(fg.size.x, FLOATING_HEADER_HEIGHT));
    let chrome_layer = egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new(("floating_chrome_bar", group_id.0)),
    );
    let chrome_painter = ctx.layer_painter(chrome_layer);
    chrome_painter.rect_filled(chrome_rect, 0.0, theme.bg_surface);

    let button_size = 20.0;
    let button_margin = 4.0;

    // Collapse button (left) — docks to grid
    let collapse_center = egui::pos2(
        chrome_rect.min.x + button_margin + button_size / 2.0,
        chrome_rect.center().y,
    );
    let collapse_rect =
        egui::Rect::from_center_size(collapse_center, egui::vec2(button_size, button_size));
    let collapse_id = egui::Id::new(("floating_collapse", group_id.0));
    let collapse_resp = egui::Area::new(collapse_id)
        .fixed_pos(collapse_rect.min)
        .order(egui::Order::Foreground)
        .sense(egui::Sense::click())
        .show(ctx, |ui| {
            ui.allocate_exact_size(collapse_rect.size(), egui::Sense::click())
                .1
        })
        .inner;

    // Draw collapse icon — chevron down (expanded) or right (collapsed)
    let collapse_color = if collapse_resp.hovered() {
        theme.text_primary
    } else {
        theme.text_secondary
    };
    let s = 4.0;
    if is_collapsed {
        // Right-pointing chevron
        chrome_painter.line_segment(
            [
                collapse_center + egui::vec2(-s * 0.5, -s),
                collapse_center + egui::vec2(s * 0.5, 0.0),
            ],
            egui::Stroke::new(1.5, collapse_color),
        );
        chrome_painter.line_segment(
            [
                collapse_center + egui::vec2(-s * 0.5, s),
                collapse_center + egui::vec2(s * 0.5, 0.0),
            ],
            egui::Stroke::new(1.5, collapse_color),
        );
    } else {
        // Downward chevron
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
    }
    if collapse_resp.clicked() {
        ctx.data_mut(|d| d.insert_temp(collapsed_id, !is_collapsed));
    }

    // Close button (right)
    let close_center = egui::pos2(
        chrome_rect.max.x - button_margin - button_size / 2.0,
        chrome_rect.center().y,
    );
    let close_rect =
        egui::Rect::from_center_size(close_center, egui::vec2(button_size, button_size));
    let close_id = egui::Id::new(("floating_close", group_id.0));
    let close_resp = egui::Area::new(close_id)
        .fixed_pos(close_rect.min)
        .order(egui::Order::Foreground)
        .sense(egui::Sense::click())
        .show(ctx, |ui| {
            ui.allocate_exact_size(close_rect.size(), egui::Sense::click())
                .1
        })
        .inner;

    let close_color = if close_resp.hovered() {
        theme.text_primary
    } else {
        theme.text_secondary
    };
    let xs = 3.5;
    chrome_painter.line_segment(
        [
            close_center - egui::vec2(xs, xs),
            close_center + egui::vec2(xs, xs),
        ],
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
        theme.text_secondary,
    );

    // --- Title bar drag (move floating container) ---
    // Use manual pointer tracking instead of egui::Area to avoid stale state
    // issues when floating groups are destroyed and recreated.
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
    let drag_state_id = egui::Id::new(("floating_dragging", group_id.0));
    let was_dragging: bool = ctx.data(|d| d.get_temp(drag_state_id).unwrap_or(false));

    if let Some(pointer_pos) = ctx.input(|i| i.pointer.interact_pos()) {
        let primary_down = ctx.input(|i| i.pointer.primary_down());
        let primary_pressed = ctx.input(|i| i.pointer.primary_pressed());

        if primary_pressed
            && drag_rect.contains(pointer_pos)
            && !collapse_rect.contains(pointer_pos)
            && !close_rect.contains(pointer_pos)
        {
            // Start drag
            ctx.data_mut(|d| d.insert_temp(drag_state_id, true));
        } else if was_dragging && primary_down {
            // Continue drag — compute delta from pointer movement
            let delta = ctx.input(|i| i.pointer.delta());
            if delta != egui::Vec2::ZERO {
                let new_pos = fg.pos + delta;
                actions.push(LayoutAction::UpdateFloatingGeometry {
                    group_id,
                    pos: new_pos,
                    size: fg.size,
                });
            }
        } else if was_dragging && !primary_down {
            // End drag
            ctx.data_mut(|d| d.insert_temp(drag_state_id, false));
        }
    } else if was_dragging {
        ctx.data_mut(|d| d.insert_temp(drag_state_id, false));
    }

    // Show grab cursor when hovering the drag area
    if let Some(pos) = ctx.input(|i| i.pointer.hover_pos())
        && drag_rect.contains(pos)
        && !collapse_rect.contains(pos)
        && !close_rect.contains(pos)
    {
        ctx.set_cursor_icon(egui::CursorIcon::Grab);
    }

    // Only render resize handles, tab bar, and content when expanded
    if !is_collapsed {
        // --- Edge/corner resize handles ---
        let resize_margin = 4.0;

        // --- Edge/corner resize highlight colors ---
        let resize_hover = egui::Color32::from_rgba_premultiplied(0x7c, 0x6c, 0xf0, 0x30);
        let resize_active = egui::Color32::from_rgba_premultiplied(0x7c, 0x6c, 0xf0, 0x90);
        let edge_thickness = 2.0;
        let corner_size = egui::vec2(12.0, 12.0);
        let corner_len = 16.0;

        // Helper: create a resize Area and return its response
        let make_resize_area =
            |ctx: &egui::Context, id: egui::Id, rect: egui::Rect| -> egui::Response {
                egui::Area::new(id)
                    .fixed_pos(rect.min)
                    .order(egui::Order::Foreground)
                    .sense(egui::Sense::drag())
                    .show(ctx, |ui| {
                        ui.allocate_exact_size(rect.size(), egui::Sense::drag()).1
                    })
                    .inner
            };

        // --- Four edges (between corners) ---
        // Top edge
        let top_edge = egui::Rect::from_center_size(
            egui::pos2(outer_rect.center().x, outer_rect.min.y),
            egui::vec2(outer_rect.width() - corner_size.x, resize_margin * 2.0),
        );
        let top_resp = make_resize_area(
            ctx,
            egui::Id::new(("floating_resize_t", group_id.0)),
            top_edge,
        );
        if top_resp.hovered() || top_resp.dragged() {
            ctx.set_cursor_icon(egui::CursorIcon::ResizeRow);
            let color = if top_resp.dragged() {
                resize_active
            } else {
                resize_hover
            };
            border_painter.rect_filled(
                egui::Rect::from_min_size(
                    outer_rect.min,
                    egui::vec2(outer_rect.width(), edge_thickness),
                ),
                0.0,
                color,
            );
        }
        if top_resp.dragged() {
            let delta = top_resp.drag_delta().y;
            let new_height = (fg.size.y - delta).max(FLOATING_MIN_SIZE.y);
            let actual_delta = fg.size.y - new_height;
            actions.push(LayoutAction::UpdateFloatingGeometry {
                group_id,
                pos: egui::pos2(fg.pos.x, fg.pos.y + actual_delta),
                size: egui::vec2(fg.size.x, new_height),
            });
        }

        // Right edge
        let right_edge = egui::Rect::from_center_size(
            egui::pos2(outer_rect.max.x, outer_rect.center().y),
            egui::vec2(resize_margin * 2.0, outer_rect.height() - corner_size.y),
        );
        let right_resp = make_resize_area(
            ctx,
            egui::Id::new(("floating_resize_r", group_id.0)),
            right_edge,
        );
        if right_resp.hovered() || right_resp.dragged() {
            ctx.set_cursor_icon(egui::CursorIcon::ResizeColumn);
            let color = if right_resp.dragged() {
                resize_active
            } else {
                resize_hover
            };
            border_painter.rect_filled(
                egui::Rect::from_min_size(
                    egui::pos2(outer_rect.max.x - edge_thickness, outer_rect.min.y),
                    egui::vec2(edge_thickness, outer_rect.height()),
                ),
                0.0,
                color,
            );
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
        let bottom_edge = egui::Rect::from_center_size(
            egui::pos2(outer_rect.center().x, outer_rect.max.y),
            egui::vec2(outer_rect.width() - corner_size.x, resize_margin * 2.0),
        );
        let bottom_resp = make_resize_area(
            ctx,
            egui::Id::new(("floating_resize_b", group_id.0)),
            bottom_edge,
        );
        if bottom_resp.hovered() || bottom_resp.dragged() {
            ctx.set_cursor_icon(egui::CursorIcon::ResizeRow);
            let color = if bottom_resp.dragged() {
                resize_active
            } else {
                resize_hover
            };
            border_painter.rect_filled(
                egui::Rect::from_min_size(
                    egui::pos2(outer_rect.min.x, outer_rect.max.y - edge_thickness),
                    egui::vec2(outer_rect.width(), edge_thickness),
                ),
                0.0,
                color,
            );
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
        let left_edge = egui::Rect::from_center_size(
            egui::pos2(outer_rect.min.x, outer_rect.center().y),
            egui::vec2(resize_margin * 2.0, outer_rect.height() - corner_size.y),
        );
        let left_resp = make_resize_area(
            ctx,
            egui::Id::new(("floating_resize_l", group_id.0)),
            left_edge,
        );
        if left_resp.hovered() || left_resp.dragged() {
            ctx.set_cursor_icon(egui::CursorIcon::ResizeColumn);
            let color = if left_resp.dragged() {
                resize_active
            } else {
                resize_hover
            };
            border_painter.rect_filled(
                egui::Rect::from_min_size(
                    egui::pos2(outer_rect.min.x, outer_rect.min.y),
                    egui::vec2(edge_thickness, outer_rect.height()),
                ),
                0.0,
                color,
            );
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

        // --- Four corners ---
        // Top-left corner
        let tl_rect = egui::Rect::from_center_size(outer_rect.left_top(), corner_size);
        let tl_resp = make_resize_area(
            ctx,
            egui::Id::new(("floating_resize_tl", group_id.0)),
            tl_rect,
        );
        if tl_resp.hovered() || tl_resp.dragged() {
            ctx.set_cursor_icon(egui::CursorIcon::ResizeNwSe);
            let color = if tl_resp.dragged() {
                resize_active
            } else {
                resize_hover
            };
            border_painter.rect_filled(
                egui::Rect::from_min_size(
                    outer_rect.left_top(),
                    egui::vec2(corner_len, edge_thickness),
                ),
                0.0,
                color,
            );
            border_painter.rect_filled(
                egui::Rect::from_min_size(
                    outer_rect.left_top(),
                    egui::vec2(edge_thickness, corner_len),
                ),
                0.0,
                color,
            );
        }
        if tl_resp.dragged() {
            let d = tl_resp.drag_delta();
            let new_width = (fg.size.x - d.x).max(FLOATING_MIN_SIZE.x);
            let new_height = (fg.size.y - d.y).max(FLOATING_MIN_SIZE.y);
            let dx = fg.size.x - new_width;
            let dy = fg.size.y - new_height;
            actions.push(LayoutAction::UpdateFloatingGeometry {
                group_id,
                pos: egui::pos2(fg.pos.x + dx, fg.pos.y + dy),
                size: egui::vec2(new_width, new_height),
            });
        }

        // Top-right corner
        let tr_rect = egui::Rect::from_center_size(outer_rect.right_top(), corner_size);
        let tr_resp = make_resize_area(
            ctx,
            egui::Id::new(("floating_resize_tr", group_id.0)),
            tr_rect,
        );
        if tr_resp.hovered() || tr_resp.dragged() {
            ctx.set_cursor_icon(egui::CursorIcon::ResizeNeSw);
            let color = if tr_resp.dragged() {
                resize_active
            } else {
                resize_hover
            };
            border_painter.rect_filled(
                egui::Rect::from_min_size(
                    egui::pos2(outer_rect.max.x - corner_len, outer_rect.min.y),
                    egui::vec2(corner_len, edge_thickness),
                ),
                0.0,
                color,
            );
            border_painter.rect_filled(
                egui::Rect::from_min_size(
                    egui::pos2(outer_rect.max.x - edge_thickness, outer_rect.min.y),
                    egui::vec2(edge_thickness, corner_len),
                ),
                0.0,
                color,
            );
        }
        if tr_resp.dragged() {
            let d = tr_resp.drag_delta();
            let new_width = (fg.size.x + d.x).max(FLOATING_MIN_SIZE.x);
            let new_height = (fg.size.y - d.y).max(FLOATING_MIN_SIZE.y);
            let dy = fg.size.y - new_height;
            actions.push(LayoutAction::UpdateFloatingGeometry {
                group_id,
                pos: egui::pos2(fg.pos.x, fg.pos.y + dy),
                size: egui::vec2(new_width, new_height),
            });
        }

        // Bottom-left corner
        let bl_rect = egui::Rect::from_center_size(outer_rect.left_bottom(), corner_size);
        let bl_resp = make_resize_area(
            ctx,
            egui::Id::new(("floating_resize_bl", group_id.0)),
            bl_rect,
        );
        if bl_resp.hovered() || bl_resp.dragged() {
            ctx.set_cursor_icon(egui::CursorIcon::ResizeNeSw);
            let color = if bl_resp.dragged() {
                resize_active
            } else {
                resize_hover
            };
            border_painter.rect_filled(
                egui::Rect::from_min_size(
                    egui::pos2(outer_rect.min.x, outer_rect.max.y - edge_thickness),
                    egui::vec2(corner_len, edge_thickness),
                ),
                0.0,
                color,
            );
            border_painter.rect_filled(
                egui::Rect::from_min_size(
                    egui::pos2(outer_rect.min.x, outer_rect.max.y - corner_len),
                    egui::vec2(edge_thickness, corner_len),
                ),
                0.0,
                color,
            );
        }
        if bl_resp.dragged() {
            let d = bl_resp.drag_delta();
            let new_width = (fg.size.x - d.x).max(FLOATING_MIN_SIZE.x);
            let new_height = (fg.size.y + d.y).max(FLOATING_MIN_SIZE.y);
            let dx = fg.size.x - new_width;
            actions.push(LayoutAction::UpdateFloatingGeometry {
                group_id,
                pos: egui::pos2(fg.pos.x + dx, fg.pos.y),
                size: egui::vec2(new_width, new_height),
            });
        }

        // Bottom-right corner
        let br_rect = egui::Rect::from_center_size(outer_rect.right_bottom(), corner_size);
        let br_resp = make_resize_area(
            ctx,
            egui::Id::new(("floating_resize_br", group_id.0)),
            br_rect,
        );
        if br_resp.hovered() || br_resp.dragged() {
            ctx.set_cursor_icon(egui::CursorIcon::ResizeNwSe);
            let color = if br_resp.dragged() {
                resize_active
            } else {
                resize_hover
            };
            border_painter.rect_filled(
                egui::Rect::from_min_size(
                    egui::pos2(
                        outer_rect.max.x - corner_len,
                        outer_rect.max.y - edge_thickness,
                    ),
                    egui::vec2(corner_len, edge_thickness),
                ),
                0.0,
                color,
            );
            border_painter.rect_filled(
                egui::Rect::from_min_size(
                    egui::pos2(
                        outer_rect.max.x - edge_thickness,
                        outer_rect.max.y - corner_len,
                    ),
                    egui::vec2(edge_thickness, corner_len),
                ),
                0.0,
                color,
            );
        }
        if br_resp.dragged() {
            let d = br_resp.drag_delta();
            let new_width = (fg.size.x + d.x).max(FLOATING_MIN_SIZE.x);
            let new_height = (fg.size.y + d.y).max(FLOATING_MIN_SIZE.y);
            actions.push(LayoutAction::UpdateFloatingGeometry {
                group_id,
                pos: fg.pos,
                size: egui::vec2(new_width, new_height),
            });
        }

        // --- Shared tab bar ---
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
                order: egui::Order::Foreground,
            },
        );

        // --- Shared content area ---
        let content_rect = egui::Rect::from_min_max(
            egui::pos2(fg.pos.x, fg.pos.y + FLOATING_HEADER_HEIGHT + TAB_BAR_HEIGHT),
            egui::pos2(fg.pos.x + fg.size.x, fg.pos.y + total_height),
        );
        render_content(
            ctx,
            group_id,
            group,
            content_rect,
            state,
            egui::Order::Foreground,
        );
    }

    // --- Store rect for drop target hit testing ---
    // Store only the tab bar + content area (exclude chrome header) so drop
    // zones are constrained to the panel group area, matching docked panels.
    let drop_rect = egui::Rect::from_min_max(
        egui::pos2(fg.pos.x, fg.pos.y + FLOATING_HEADER_HEIGHT),
        egui::pos2(fg.pos.x + fg.size.x, fg.pos.y + total_height),
    );
    let rect_id = egui::Id::new(("floating_rect", group_id.0));
    ctx.data_mut(|d| d.insert_temp(rect_id, drop_rect));
}
