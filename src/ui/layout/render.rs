//! Rendering for the dockview-style layout system.
//!
//! Provides [`render_menu_bar`] and [`render_layout`] which produce [`LayoutAction`]
//! values for the window to apply after the egui frame completes.

use super::interactions::{collect_dividers, drop_zone_highlight_rect, hit_test_drop_zone};
use super::tree::{DockLayout, DropZone, GroupId, NodeId, PanelType, SplitDirection};

use crate::ui::theme::{
    ADD_BUTTON_WIDTH, BG_ELEVATED, BG_PANEL, BG_SURFACE, BORDER, DEFAULT_ACCENT, DOCK_GRIP_WIDTH,
    FLOATING_HEADER_HEIGHT, FLOATING_MIN_SIZE, PANEL_PADDING, TAB_BAR_HEIGHT, TEXT_PRIMARY,
    TEXT_SECONDARY,
};

/// Drop-zone highlight: accent at ~15% opacity.
const DROP_ZONE_TINT: egui::Color32 = egui::Color32::from_rgba_premultiplied(
    DEFAULT_ACCENT.r(),
    DEFAULT_ACCENT.g(),
    DEFAULT_ACCENT.b(),
    38,
);

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
fn paint_grip_dots(painter: &egui::Painter, center: egui::Pos2, color: egui::Color32) {
    for dy in [-3.0_f32, 0.0, 3.0] {
        for dx in [-2.5_f32, 2.5] {
            painter.circle_filled(center + egui::vec2(dx, dy), 1.0, color);
        }
    }
}

/// All dockable panel types for the type selector dropdown.
pub const DOCKABLE_TYPES: &[PanelType] = &[
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
                overlay_painter.rect_filled(highlight, 0.0, DROP_ZONE_TINT);
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
// Divider rendering
// ---------------------------------------------------------------------------

/// Render split dividers with drag interaction.
fn render_dividers(
    ctx: &egui::Context,
    layout: &DockLayout,
    available_rect: egui::Rect,
    actions: &mut Vec<LayoutAction>,
) {
    let dividers = collect_dividers(layout, available_rect);
    for div in &dividers {
        // Draw the 1px visible line
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Middle,
            egui::Id::new(("divider_line", div.node_id.0)),
        ));
        let line_rect = match div.direction {
            SplitDirection::Vertical => {
                let cx = div.rect.center().x;
                egui::Rect::from_min_size(
                    egui::pos2(cx - 0.5, div.rect.min.y),
                    egui::vec2(1.0, div.rect.height()),
                )
            }
            SplitDirection::Horizontal => {
                let cy = div.rect.center().y;
                egui::Rect::from_min_size(
                    egui::pos2(div.rect.min.x, cy - 0.5),
                    egui::vec2(div.rect.width(), 1.0),
                )
            }
        };
        painter.rect_filled(line_rect, 0.0, BORDER);

        // Invisible Area for drag interaction
        let area_id = egui::Id::new(("divider_area", div.node_id.0));
        let node_id = div.node_id;
        let direction = div.direction;
        let parent_rect = div.parent_rect;
        let div_rect = div.rect;

        egui::Area::new(area_id)
            .fixed_pos(div_rect.min)
            .sense(egui::Sense::drag())
            .show(ctx, |ui| {
                let response = ui.allocate_response(div_rect.size(), egui::Sense::drag());

                if response.hovered() || response.dragged() {
                    match direction {
                        SplitDirection::Vertical => {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeColumn);
                        }
                        SplitDirection::Horizontal => {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeRow);
                        }
                    }
                }

                if response.dragged()
                    && let Some(pos) = ui.ctx().pointer_interact_pos()
                {
                    let new_ratio = match direction {
                        SplitDirection::Vertical => {
                            (pos.x - parent_rect.min.x) / parent_rect.width()
                        }
                        SplitDirection::Horizontal => {
                            (pos.y - parent_rect.min.y) / parent_rect.height()
                        }
                    };
                    actions.push(LayoutAction::Resize { node_id, new_ratio });
                }
            });
    }
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
            overlay_painter.rect_filled(highlight, 0.0, DROP_ZONE_TINT);
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

// ---------------------------------------------------------------------------
// Tab bar rendering
// ---------------------------------------------------------------------------

/// Context flags for tab bar rendering.
struct TabBarContext {
    is_main: bool,
    is_floating: bool,
    /// Layer order for painters and areas. Docked groups use `Middle`,
    /// floating groups use `Foreground` so they paint above docked content.
    order: egui::Order,
}

/// Render the tab bar for a group. Emits actions for tab clicks, drags, close, and context menus.
fn render_tab_bar(
    ctx: &egui::Context,
    layout: &DockLayout,
    group_id: GroupId,
    group: &super::tree::Group,
    tab_bar_rect: egui::Rect,
    actions: &mut Vec<LayoutAction>,
    tctx: TabBarContext,
) {
    let painter = ctx.layer_painter(egui::LayerId::new(
        tctx.order,
        egui::Id::new(("tab_bar_bg", group_id.0)),
    ));
    painter.rect_filled(tab_bar_rect, 0.0, BG_SURFACE);

    let tab_count = group.tabs.len();
    let max_tab_width = 160.0_f32;
    let available_for_tabs = tab_bar_rect.width() - ADD_BUTTON_WIDTH - DOCK_GRIP_WIDTH;
    let tab_width = if tab_count > 0 {
        (available_for_tabs / tab_count as f32).min(max_tab_width)
    } else {
        max_tab_width
    };

    for (i, tab) in group.tabs.iter().enumerate() {
        let is_active = i == group.active_tab;
        let tab_rect = egui::Rect::from_min_size(
            egui::pos2(
                tab_bar_rect.min.x + i as f32 * tab_width,
                tab_bar_rect.min.y,
            ),
            egui::vec2(tab_width, TAB_BAR_HEIGHT),
        );

        // Use an Area for each tab to get click + drag interaction
        let tab_area_id = egui::Id::new(("tab_area", group_id.0, i));
        let gid = group_id;
        let tab_idx = i;
        let no_active_drag = layout.drag.is_none();

        egui::Area::new(tab_area_id)
            .fixed_pos(tab_rect.min)
            .order(tctx.order)
            .sense(egui::Sense::click_and_drag())
            .show(ctx, |ui| {
                let response = ui.allocate_response(tab_rect.size(), egui::Sense::click_and_drag());

                // Background
                let bg = if is_active || response.hovered() {
                    BG_ELEVATED
                } else {
                    BG_SURFACE
                };
                painter.rect_filled(tab_rect, 0.0, bg);

                // Accent line at bottom of active tab
                if is_active {
                    let accent_rect = egui::Rect::from_min_size(
                        egui::pos2(tab_rect.min.x, tab_rect.max.y - 2.0),
                        egui::vec2(tab_width, 2.0),
                    );
                    painter.rect_filled(accent_rect, 0.0, DEFAULT_ACCENT);
                }

                // Label — truncate with ellipsis when too wide
                let text_color = if is_active {
                    TEXT_PRIMARY
                } else {
                    TEXT_SECONDARY
                };
                let label_pos = egui::pos2(tab_rect.min.x + 8.0, tab_rect.center().y - 6.0);
                let available_text_width = (tab_width - 28.0).max(10.0);
                let font = egui::FontId::proportional(12.0);
                let full_name = tab.panel_type.display_name();
                let galley =
                    painter.layout_no_wrap(full_name.to_string(), font.clone(), text_color);
                if galley.size().x > available_text_width {
                    let ellipsis = "…";
                    let ellipsis_galley =
                        painter.layout_no_wrap(ellipsis.to_string(), font.clone(), text_color);
                    let text_budget = available_text_width - ellipsis_galley.size().x;
                    // Find how many chars fit within the budget
                    let mut truncated = String::new();
                    for ch in full_name.chars() {
                        truncated.push(ch);
                        let test =
                            painter.layout_no_wrap(truncated.clone(), font.clone(), text_color);
                        if test.size().x > text_budget {
                            truncated.pop();
                            break;
                        }
                    }
                    truncated.push_str(ellipsis);
                    let truncated_galley = painter.layout_no_wrap(truncated, font, text_color);
                    painter.galley(label_pos, truncated_galley, text_color);
                } else {
                    painter.galley(label_pos, galley, text_color);
                }

                // Close button (visible only when hovering the tab)
                // Use manual pointer detection — the tab Area's click_and_drag
                // sense consumes clicks before child widgets can receive them.
                let close_center = egui::pos2(tab_rect.max.x - 12.0, tab_rect.center().y);
                let close_rect = egui::Rect::from_center_size(close_center, egui::vec2(14.0, 14.0));
                let pointer_pos = ui.ctx().input(|i| i.pointer.hover_pos());
                let close_hovered = pointer_pos.is_some_and(|p| close_rect.contains(p));
                let mut close_clicked = false;

                if response.hovered() {
                    let close_color = if close_hovered {
                        TEXT_PRIMARY
                    } else {
                        TEXT_SECONDARY
                    };

                    let s = 3.5;
                    painter.line_segment(
                        [
                            close_center - egui::vec2(s, s),
                            close_center + egui::vec2(s, s),
                        ],
                        egui::Stroke::new(1.5, close_color),
                    );
                    painter.line_segment(
                        [
                            close_center + egui::vec2(-s, s),
                            close_center + egui::vec2(s, -s),
                        ],
                        egui::Stroke::new(1.5, close_color),
                    );

                    if close_hovered && response.clicked() {
                        close_clicked = true;
                        actions.push(LayoutAction::Close {
                            group_id: gid,
                            tab_index: tab_idx,
                        });
                    }
                }

                // Click to activate (skip if close button was clicked)
                if response.clicked() && !close_clicked {
                    actions.push(LayoutAction::SetActiveTab {
                        group_id: gid,
                        tab_index: tab_idx,
                    });
                }

                // Drag to start
                if response.drag_started() && no_active_drag {
                    actions.push(LayoutAction::StartDrag {
                        group_id: gid,
                        tab_index: tab_idx,
                    });
                }

                // Context menu
                response.context_menu(|ui: &mut egui::Ui| {
                    if tctx.is_floating {
                        // Floating group inside main window — offer to dock back
                        if ui.button("Dock to Grid").clicked() {
                            actions.push(LayoutAction::DockFloatingToGrid { group_id: gid });
                            ui.close();
                        }
                        if ui.button("Pop Out to Window").clicked() {
                            actions.push(LayoutAction::DetachToWindow {
                                group_id: gid,
                                tab_index: tab_idx,
                            });
                            ui.close();
                        }
                    } else if tctx.is_main {
                        // Grid group in main window
                        if ui.button("Detach").clicked() {
                            actions.push(LayoutAction::DetachToFloat {
                                group_id: gid,
                                tab_index: tab_idx,
                            });
                            ui.close();
                        }
                        if ui.button("Pop Out to Window").clicked() {
                            actions.push(LayoutAction::DetachToWindow {
                                group_id: gid,
                                tab_index: tab_idx,
                            });
                            ui.close();
                        }
                    } else {
                        // Detached OS window
                        if ui.button("Reattach to Main Window").clicked() {
                            actions.push(LayoutAction::ReattachToMain);
                            ui.close();
                        }
                    }
                    ui.separator();
                    if tab_count > 1 && ui.button("Close Others").clicked() {
                        actions.push(LayoutAction::CloseOthers {
                            group_id: gid,
                            tab_index: tab_idx,
                        });
                        ui.close();
                    }
                    if ui.button("Close").clicked() {
                        actions.push(LayoutAction::Close {
                            group_id: gid,
                            tab_index: tab_idx,
                        });
                        ui.close();
                    }
                });
            });
    }

    // "+" button after the last tab — painted inline, no separate Area.
    // We use the tab bar painter for visuals and a dedicated egui::Area only
    // for the menu popup (which needs its own layer to size freely).
    let plus_x = tab_bar_rect.min.x + tab_count as f32 * tab_width;
    let plus_rect = egui::Rect::from_min_size(
        egui::pos2(plus_x, tab_bar_rect.min.y),
        egui::vec2(ADD_BUTTON_WIDTH, TAB_BAR_HEIGHT),
    );
    let gid = group_id;

    // Detect hover/click via a minimal Area exactly sized to the button
    let plus_area_id = egui::Id::new(("tab_add_btn", gid.0));
    let plus_response = egui::Area::new(plus_area_id)
        .fixed_pos(plus_rect.min)
        .order(tctx.order)
        .sense(egui::Sense::click())
        .show(ctx, |ui| {
            ui.allocate_exact_size(plus_rect.size(), egui::Sense::click())
                .1
        })
        .inner;

    // Paint the "+" on the tab bar painter (not inside the Area)
    let plus_hovered = plus_response.hovered();
    if plus_hovered {
        painter.rect_filled(plus_rect, 0.0, BG_ELEVATED);
    }
    painter.text(
        plus_rect.center(),
        egui::Align2::CENTER_CENTER,
        "+",
        egui::FontId::proportional(14.0),
        if plus_hovered {
            TEXT_PRIMARY
        } else {
            TEXT_SECONDARY
        },
    );

    // Toggle popup on click, using manual state to avoid same-frame close
    let popup_state_id = egui::Id::new(("add_popup_open", gid.0));
    let was_open: bool = ctx.data(|d| d.get_temp(popup_state_id).unwrap_or(false));
    let mut is_open = was_open;

    if plus_response.clicked() {
        is_open = !is_open;
    }

    if is_open {
        let popup_pos = egui::pos2(plus_rect.min.x, plus_rect.max.y + 2.0);
        egui::Area::new(egui::Id::new(("add_panel_popup", gid.0)))
            .fixed_pos(popup_pos)
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_min_width(120.0);
                    for &pt in DOCKABLE_TYPES {
                        if ui.selectable_label(false, pt.display_name()).clicked() {
                            actions.push(LayoutAction::AddPanel {
                                target_group: gid,
                                panel_type: pt,
                            });
                            is_open = false;
                        }
                    }
                });
            });

        // Close on click outside (only if popup was open last frame)
        if was_open
            && !plus_response.clicked()
            && ctx.input(|i| i.pointer.any_pressed())
            && let Some(pos) = ctx.pointer_interact_pos()
        {
            let popup_id = egui::Id::new(("add_panel_popup", gid.0));
            let on_popup = ctx
                .layer_id_at(pos)
                .is_some_and(|layer| layer.id == popup_id);
            if !on_popup && !plus_rect.contains(pos) {
                is_open = false;
            }
        }
    }

    ctx.data_mut(|d| d.insert_temp(popup_state_id, is_open));

    // Dock grip (right-aligned in tab bar) — drag to move the group
    let dock_x = tab_bar_rect.max.x - DOCK_GRIP_WIDTH;
    let dock_rect = egui::Rect::from_min_size(
        egui::pos2(dock_x, tab_bar_rect.min.y),
        egui::vec2(DOCK_GRIP_WIDTH, TAB_BAR_HEIGHT),
    );
    // Paint grip dots (2x3 grid)
    paint_grip_dots(&painter, dock_rect.center(), TEXT_SECONDARY);
    // Drag + context menu interaction via a tightly-sized Area
    let grip_area_resp = egui::Area::new(egui::Id::new(("grip_area", gid.0)))
        .fixed_pos(dock_rect.min)
        .sense(egui::Sense::click_and_drag())
        .order(tctx.order)
        .default_size(dock_rect.size())
        .show(ctx, |ui| {
            ui.set_min_size(dock_rect.size());
            ui.set_max_size(dock_rect.size());
            ui.allocate_exact_size(dock_rect.size(), egui::Sense::click_and_drag())
                .1
        });
    let grip_resp = &grip_area_resp.inner;
    if grip_resp.hovered() || grip_resp.dragged() {
        ctx.set_cursor_icon(egui::CursorIcon::Grab);
    }
    if grip_resp.drag_started() || grip_resp.dragged() {
        let group_drag_id = egui::Id::new("group_dock_drag");
        ctx.data_mut(|d| d.insert_temp(group_drag_id, gid));
    }
    grip_resp.context_menu(|ui| {
        if tctx.is_floating {
            // Floating group — offer to dock back to grid
            if ui.button("Dock to Grid").clicked() {
                actions.push(LayoutAction::DockFloatingToGrid { group_id: gid });
                ui.close();
            }
        } else if tctx.is_main {
            // Docked group in main window — offer to detach
            if ui.button("Detach to Float").clicked() {
                actions.push(LayoutAction::DetachGroupToFloat { group_id: gid });
                ui.close();
            }
        }
        if ui.button("Pop Out to Window").clicked() {
            // Pop out the active tab to an OS-level window
            actions.push(LayoutAction::DetachToWindow {
                group_id: gid,
                tab_index: group.active_tab,
            });
            ui.close();
        }
        if tab_count > 0 {
            ui.separator();
            if ui.button("Close Group").clicked() {
                for i in (0..tab_count).rev() {
                    actions.push(LayoutAction::Close {
                        group_id: gid,
                        tab_index: i,
                    });
                }
                ui.close();
            }
        }
    });
}

// ---------------------------------------------------------------------------
// Content area rendering
// ---------------------------------------------------------------------------

/// Render the active panel content for a group.
fn render_content(
    ctx: &egui::Context,
    group_id: GroupId,
    group: &super::tree::Group,
    content_rect: egui::Rect,
    state: &mut crate::state::AppState,
    order: egui::Order,
) {
    let active = group.active_tab_entry();
    let panel_id = active.panel_id;
    let panel_type = active.panel_type;

    let area_id = egui::Id::new(("panel_content", group_id.0));
    egui::Area::new(area_id)
        .fixed_pos(content_rect.min)
        .order(order)
        .sense(egui::Sense::hover())
        .show(ctx, |ui| {
            ui.painter().rect_filled(content_rect, 0.0, BG_PANEL);

            ui.set_min_size(content_rect.size());
            ui.set_max_size(content_rect.size());

            let padded_rect = content_rect.shrink(PANEL_PADDING);
            let mut padded_ui = ui.new_child(egui::UiBuilder::new().max_rect(padded_rect));

            // Hide scrollbars on the dockview wrapper — panels that need
            // scrolling (Sources, Properties) use their own internal ScrollArea.
            // The wrapper ScrollArea exists only to clip content, not to scroll.
            egui::ScrollArea::both()
                .auto_shrink(false)
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                .show(&mut padded_ui, |ui| {
                    crate::ui::draw_panel(panel_type, ui, state, panel_id);
                });
        });
}

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
        egui::Stroke::new(1.0, BORDER),
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
    chrome_painter.rect_filled(chrome_rect, 0.0, BG_SURFACE);

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
        TEXT_PRIMARY
    } else {
        TEXT_SECONDARY
    };
    let s = 4.0;
    if is_collapsed {
        // Right-pointing chevron ›
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
        // Downward chevron ∨
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
        TEXT_PRIMARY
    } else {
        TEXT_SECONDARY
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
        TEXT_SECONDARY,
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
