//! Rendering for the dockview-style layout system.
//!
//! Provides [`render_menu_bar`] and [`render_layout`] which produce [`LayoutAction`]
//! values for the window to apply after the egui frame completes.
//!
//! Submodules handle specific rendering responsibilities:
//! - [`super::render_grid`] — split divider rendering
//! - [`super::render_tabs`] — tab bar and content area rendering
//! - [`super::render_floating`] — floating window chrome rendering

use super::interactions::{drop_zone_highlight_rect, hit_test_drop_zone};
use super::render_floating::render_floating_chrome;
use super::render_grid::render_dividers;
use super::render_tabs::{TabBarContext, render_content, render_tab_bar};
use super::tree::{DockLayout, DropZone, GroupId, NodeId, PanelType};

use crate::ui::theme::{TAB_BAR_HEIGHT, TEXT_PRIMARY, accent_color};

/// Drop-zone highlight: accent at ~15% opacity.
fn drop_zone_tint(ctx: &egui::Context) -> egui::Color32 {
    let c = accent_color(ctx);
    egui::Color32::from_rgba_premultiplied(c.r(), c.g(), c.b(), 38)
}

// ---------------------------------------------------------------------------
// LayoutAction
// ---------------------------------------------------------------------------

/// Actions the layout renderer can request.
#[allow(dead_code)]
pub enum LayoutAction {
    /// Resize a split node divider.
    Resize { node_id: NodeId, new_ratio: f32 },
    /// Close a single tab.
    Close { group_id: GroupId, tab_index: usize },
    /// Close all tabs except the specified one.
    CloseOthers { group_id: GroupId, tab_index: usize },
    /// Switch the active tab in a group.
    SetActiveTab { group_id: GroupId, tab_index: usize },
    /// Detach a tab into a floating group within the same window.
    DetachToFloat { group_id: GroupId, tab_index: usize },
    /// Detach a tab into a new OS-level window.
    DetachToWindow { group_id: GroupId, tab_index: usize },
    /// Begin dragging a tab.
    StartDrag { group_id: GroupId, tab_index: usize },
    /// Drop a dragged tab onto a target group zone.
    DropOnZone {
        target_group: GroupId,
        zone: DropZone,
    },
    /// Drop a dragged tab into empty space (creates a floating group).
    DropOnEmpty { pos: egui::Pos2 },
    /// Cancel the current drag operation.
    CancelDrag,
    /// Add a new panel tab to an existing group.
    AddPanel {
        target_group: GroupId,
        panel_type: PanelType,
    },
    /// Add a new panel at the root level of the split tree.
    AddPanelAtRoot { panel_type: PanelType },
    /// Reset the layout to the default configuration.
    ResetLayout,
    /// Reattach all panels from this window back to the main window.
    ReattachToMain,
    /// Dock a floating group back into the grid.
    DockFloatingToGrid { group_id: GroupId },
    /// Close an entire floating group (all tabs).
    CloseFloatingGroup { group_id: GroupId },
    /// Detach an entire grid group to a floating panel.
    DetachGroupToFloat { group_id: GroupId },
    /// Move an entire group to a target group's zone (merge tabs or split).
    MoveGroupToTarget {
        source_group: GroupId,
        target_group: GroupId,
        zone: DropZone,
    },
    /// Update a floating group's position and/or size.
    UpdateFloatingGeometry {
        group_id: GroupId,
        pos: egui::Pos2,
        size: egui::Vec2,
    },
}

/// Paint a 2x3 grid of dots as a grip/drag handle indicator.
pub(crate) fn paint_grip_dots(painter: &egui::Painter, center: egui::Pos2, color: egui::Color32) {
    for dy in [-3.0_f32, 0.0, 3.0] {
        for dx in [-2.5_f32, 2.5] {
            painter.circle_filled(center + egui::vec2(dx, dy), 1.0, color);
        }
    }
}

/// All dockable panel types for the type selector dropdown.
pub(crate) const DOCKABLE_TYPES: &[PanelType] = &[
    PanelType::Preview,
    PanelType::Library,
    PanelType::Sources,
    PanelType::Scenes,
    PanelType::Properties,
    PanelType::AudioMixer,
    PanelType::StreamControls,
];

// ---------------------------------------------------------------------------
// Menu bar
// ---------------------------------------------------------------------------

/// Returns the available rect for the layout (full window area since the menu is now native).
pub fn render_menu_bar(
    ctx: &egui::Context,
    _layout: &DockLayout,
) -> (Vec<LayoutAction>, egui::Rect) {
    (Vec::new(), ctx.available_rect())
}

// ---------------------------------------------------------------------------
// Main layout renderer
// ---------------------------------------------------------------------------

/// Render the full layout (grid groups, floating groups, dividers, drag overlays).
/// Returns layout actions to be applied after the egui frame.
///
/// When `is_main` is true, this is the main window. When false, the tab context menu
/// includes a "Reattach to Main Window" option.
pub fn render_layout(
    ctx: &egui::Context,
    layout: &DockLayout,
    state: &mut crate::state::AppState,
    available_rect: egui::Rect,
    is_main: bool,
) -> Vec<LayoutAction> {
    let mut actions = Vec::new();

    // --- Grid groups ---
    let group_rects = layout.collect_groups_with_rects(available_rect);

    for &(group_id, rect) in &group_rects {
        if let Some(group) = layout.groups.get(&group_id) {
            {
                let tab_bar_rect =
                    egui::Rect::from_min_size(rect.min, egui::vec2(rect.width(), TAB_BAR_HEIGHT));
                let content_rect = egui::Rect::from_min_max(
                    egui::pos2(rect.min.x, rect.min.y + TAB_BAR_HEIGHT),
                    rect.max,
                );

                render_tab_bar(
                    ctx,
                    layout,
                    group_id,
                    group,
                    tab_bar_rect,
                    &mut actions,
                    TabBarContext {
                        is_main,
                        is_floating: false,
                        order: egui::Order::Middle,
                    },
                );
                render_content(
                    ctx,
                    group_id,
                    group,
                    content_rect,
                    state,
                    egui::Order::Middle,
                );
            }
        }
    }

    // --- Floating groups ---
    for fg in &layout.floating {
        if let Some(group) = layout.groups.get(&fg.group_id) {
            render_floating_chrome(ctx, layout, fg, group, state, &mut actions, is_main);
        }
    }

    // --- Dividers ---
    render_dividers(ctx, layout, available_rect, &mut actions);

    // --- Build drop rects only when a drag is active ---
    let group_drag_id = egui::Id::new("group_dock_drag");
    let has_tab_drag = layout.drag.is_some();
    let dragging_group = ctx.data(|d| d.get_temp::<GroupId>(group_drag_id));

    // Signal to other systems (e.g. transform handles) that a dock drag is active,
    // so they can suppress pointer interaction during panel rearrangement.
    let dock_drag_active = has_tab_drag || dragging_group.is_some();
    ctx.data_mut(|d| d.insert_temp(egui::Id::new("dock_drag_active"), dock_drag_active));

    let all_drop_rects = if has_tab_drag || dragging_group.is_some() {
        // Floating groups checked first (higher z-order)
        let mut rects: Vec<(GroupId, egui::Rect)> = Vec::new();
        for fg in &layout.floating {
            let rect_id = egui::Id::new(("floating_rect", fg.group_id.0));
            if let Some(rect) = ctx.data(|d| d.get_temp::<egui::Rect>(rect_id)) {
                rects.push((fg.group_id, rect));
            }
        }
        rects.extend_from_slice(&group_rects);
        rects
    } else {
        Vec::new()
    };

    // --- Drag ghost and drop zones ---
    if let Some(drag) = &layout.drag {
        render_drag_overlay(ctx, drag, &all_drop_rects, layout, &mut actions);
    }

    // --- Group drag overlay (grip handle drag) ---
    if let Some(dragging_gid) = dragging_group {
        if let Some(pointer_pos) = ctx.pointer_interact_pos() {
            // Ghost label following cursor
            let ghost_layer =
                egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("group_drag_ghost"));
            let ghost_painter = ctx.layer_painter(ghost_layer);
            let group_name = layout
                .groups
                .get(&dragging_gid)
                .map(|g| g.active_tab_entry().panel_type.display_name())
                .unwrap_or("Group");
            let font = egui::FontId::proportional(13.0);
            let galley = ghost_painter.layout_no_wrap(group_name.to_string(), font, TEXT_PRIMARY);
            let text_rect =
                egui::Rect::from_min_size(pointer_pos + egui::vec2(12.0, -8.0), galley.size())
                    .expand(4.0);
            ghost_painter.rect_filled(
                text_rect,
                4.0,
                egui::Color32::from_rgba_premultiplied(0x1e, 0x1e, 0x2e, 0xd0),
            );
            ghost_painter.galley(text_rect.min + egui::vec2(4.0, 4.0), galley, TEXT_PRIMARY);

            // Show drop zone overlays on all groups (excluding the dragged group)
            let mut hovered_group: Option<(GroupId, DropZone, egui::Rect)> = None;
            for &(gid, rect) in &all_drop_rects {
                if gid == dragging_gid {
                    continue;
                }
                if rect.contains(pointer_pos) {
                    let tc = layout.groups.get(&gid).map_or(0, |g| g.tabs.len());
                    let zone = hit_test_drop_zone(rect, pointer_pos, tc);
                    hovered_group = Some((gid, zone, rect));
                    break;
                }
            }

            if let Some((gid, zone, group_rect)) = &hovered_group {
                let tc = layout.groups.get(gid).map_or(0, |g| g.tabs.len());
                let highlight = drop_zone_highlight_rect(*group_rect, *zone, tc);
                let overlay_layer = egui::LayerId::new(
                    egui::Order::Foreground,
                    egui::Id::new("group_dock_overlay"),
                );
                let overlay_painter = ctx.layer_painter(overlay_layer);
                overlay_painter.rect_filled(highlight, 0.0, drop_zone_tint(ctx));
            }

            // On release: move group to target or detach to float
            if ctx.input(|i| i.pointer.any_released()) {
                let is_floating = layout.is_floating(dragging_gid);
                if let Some((target_gid, zone, _)) = hovered_group {
                    // Merge all tabs from dragged group into target
                    actions.push(LayoutAction::MoveGroupToTarget {
                        source_group: dragging_gid,
                        target_group: target_gid,
                        zone,
                    });
                } else if !is_floating {
                    // Dropped in empty space from grid → detach to float
                    actions.push(LayoutAction::DetachGroupToFloat {
                        group_id: dragging_gid,
                    });
                }
                ctx.data_mut(|d| d.remove::<GroupId>(group_drag_id));
            }
        } else {
            // No pointer — cancel
            ctx.data_mut(|d| d.remove::<GroupId>(group_drag_id));
        }
    }

    actions
}

// ---------------------------------------------------------------------------
// Drag overlay rendering
// ---------------------------------------------------------------------------

/// Render the drag ghost label and drop zone overlays.
fn render_drag_overlay(
    ctx: &egui::Context,
    drag: &super::tree::DragState,
    group_rects: &[(GroupId, egui::Rect)],
    layout: &DockLayout,
    actions: &mut Vec<LayoutAction>,
) {
    if let Some(pointer_pos) = ctx.pointer_interact_pos() {
        // Ghost label following cursor
        let ghost_layer = egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("drag_ghost"));
        let painter = ctx.layer_painter(ghost_layer);
        let text = drag.panel_type.display_name();
        let font = egui::FontId::proportional(13.0);
        let galley = painter.layout_no_wrap(text.to_string(), font, TEXT_PRIMARY);
        let text_rect =
            egui::Rect::from_min_size(pointer_pos + egui::vec2(12.0, -8.0), galley.size())
                .expand(4.0);
        painter.rect_filled(
            text_rect,
            4.0,
            egui::Color32::from_rgba_premultiplied(0x1e, 0x1e, 0x2e, 0xd0),
        );
        painter.galley(text_rect.min + egui::vec2(4.0, 4.0), galley, TEXT_PRIMARY);

        // Drop zone overlays on grid groups
        let mut hovered_group: Option<(GroupId, DropZone, egui::Rect)> = None;
        for &(gid, rect) in group_rects {
            if rect.contains(pointer_pos) {
                // Skip source group with only 1 tab (can't drop on itself),
                // but still break to prevent falling through to panels behind.
                if gid == drag.source_group
                    && layout.groups.get(&gid).is_some_and(|g| g.tabs.len() <= 1)
                {
                    break;
                }
                let tc = layout.groups.get(&gid).map_or(0, |g| g.tabs.len());
                let mut zone = hit_test_drop_zone(rect, pointer_pos, tc);
                // Floating groups can't be split — only allow tab bar or center
                if layout.is_floating(gid) && !matches!(zone, DropZone::TabBar { .. }) {
                    zone = DropZone::Center;
                }
                hovered_group = Some((gid, zone, rect));
                break;
            }
        }

        if let Some((gid, zone, group_rect)) = &hovered_group {
            let tc = layout.groups.get(gid).map_or(0, |g| g.tabs.len());
            let highlight = drop_zone_highlight_rect(*group_rect, *zone, tc);
            let overlay_layer =
                egui::LayerId::new(egui::Order::Foreground, egui::Id::new("drop_overlay"));
            let overlay_painter = ctx.layer_painter(overlay_layer);
            overlay_painter.rect_filled(highlight, 0.0, drop_zone_tint(ctx));
        }

        // On mouse release: emit drop action or cancel
        if ctx.input(|i| i.pointer.any_released()) {
            if let Some((target_gid, zone, _)) = hovered_group {
                // Drop on the same group center = cancel (no-op reorder)
                let is_self_center =
                    target_gid == drag.source_group && matches!(zone, DropZone::Center);
                if is_self_center {
                    actions.push(LayoutAction::CancelDrag);
                } else {
                    actions.push(LayoutAction::DropOnZone {
                        target_group: target_gid,
                        zone,
                    });
                }
            } else {
                // Drop outside any group = cancel (detach only via context menu)
                actions.push(LayoutAction::CancelDrag);
            }
        }
    } else {
        actions.push(LayoutAction::CancelDrag);
    }
}
