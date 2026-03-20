//! Rendering for the dockview-style layout system.
//!
//! Provides [`render_menu_bar`] and [`render_layout`] which produce [`LayoutAction`]
//! values for the window to apply after the egui frame completes.

use super::interactions::{collect_dividers, drop_zone_highlight_rect, hit_test_drop_zone};
use super::tree::{
    DockLayout, DropZone, GroupId, NodeId, PanelType, SplitDirection,
};

// ---------------------------------------------------------------------------
// Theme constants
// ---------------------------------------------------------------------------

const TAB_BAR_BG: egui::Color32 = egui::Color32::from_rgb(0x1e, 0x1e, 0x2e);
const TAB_ACTIVE_BG: egui::Color32 = egui::Color32::from_rgb(0x2a, 0x2a, 0x3e);
const TAB_HOVER_BG: egui::Color32 = egui::Color32::from_rgb(0x2e, 0x2e, 0x3e);
const TAB_ACCENT: egui::Color32 = egui::Color32::from_rgb(0x7c, 0x6c, 0xf0);
const CONTENT_BG: egui::Color32 = egui::Color32::from_rgb(0x18, 0x18, 0x25);
const DROP_ZONE_TINT: egui::Color32 =
    egui::Color32::from_rgba_premultiplied(0x7c, 0x6c, 0xf0, 0x40);
const TEXT_DIM: egui::Color32 = egui::Color32::from_gray(0xa0);
const TEXT_BRIGHT: egui::Color32 = egui::Color32::from_gray(0xe0);
const DIVIDER_COLOR: egui::Color32 = egui::Color32::from_gray(60);
const TAB_BAR_HEIGHT: f32 = 28.0;

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
    DropOnZone { target_group: GroupId, zone: DropZone },
    /// Drop a dragged tab into empty space (creates a floating group).
    DropOnEmpty { pos: egui::Pos2 },
    /// Cancel the current drag operation.
    CancelDrag,
    /// Add a new panel tab to an existing group.
    AddPanel { target_group: GroupId, panel_type: PanelType },
    /// Add a new panel at the root level of the split tree.
    AddPanelAtRoot { panel_type: PanelType },
    /// Reset the layout to the default configuration.
    ResetLayout,
}

/// All dockable panel types for the type selector dropdown.
pub const DOCKABLE_TYPES: &[PanelType] = &[
    PanelType::Preview,
    PanelType::SceneEditor,
    PanelType::AudioMixer,
    PanelType::StreamControls,
];

// ---------------------------------------------------------------------------
// Menu bar
// ---------------------------------------------------------------------------

/// Render the top menu bar. Returns layout actions and the remaining rect below the bar.
pub fn render_menu_bar(
    ctx: &egui::Context,
    _layout: &DockLayout,
) -> (Vec<LayoutAction>, egui::Rect) {
    let mut actions = Vec::new();

    egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("View", |ui| {
                ui.menu_button("Add Panel", |ui| {
                    for &pt in DOCKABLE_TYPES {
                        if ui.button(pt.display_name()).clicked() {
                            actions.push(LayoutAction::AddPanelAtRoot { panel_type: pt });
                            ui.close();
                        }
                    }
                });
                ui.separator();
                if ui.button("Reset Layout").clicked() {
                    actions.push(LayoutAction::ResetLayout);
                    ui.close();
                }
            });
        });
    });

    (actions, ctx.available_rect())
}

// ---------------------------------------------------------------------------
// Main layout renderer
// ---------------------------------------------------------------------------

/// Render the full layout (grid groups, floating groups, dividers, drag overlays).
/// Returns layout actions to be applied after the egui frame.
pub fn render_layout(
    ctx: &egui::Context,
    layout: &DockLayout,
    state: &mut crate::state::AppState,
    available_rect: egui::Rect,
) -> Vec<LayoutAction> {
    let mut actions = Vec::new();

    // --- Grid groups ---
    let group_rects = layout.collect_groups_with_rects(available_rect);

    for &(group_id, rect) in &group_rects {
        if let Some(group) = layout.groups.get(&group_id) {
            let tab_bar_rect = egui::Rect::from_min_size(
                rect.min,
                egui::vec2(rect.width(), TAB_BAR_HEIGHT),
            );
            let content_rect = egui::Rect::from_min_max(
                egui::pos2(rect.min.x, rect.min.y + TAB_BAR_HEIGHT),
                rect.max,
            );

            render_tab_bar(ctx, layout, group_id, group, tab_bar_rect, &mut actions);
            render_content(ctx, group_id, group, content_rect, state);
        }
    }

    // --- Floating groups ---
    let floating_snapshot: Vec<_> = layout.floating.clone();
    for fg in &floating_snapshot {
        if let Some(group) = layout.groups.get(&fg.group_id) {
            let win_id = egui::Id::new(("floating_group", fg.group_id.0));
            let mut open = true;
            egui::Window::new(group.active_tab_entry().panel_type.display_name())
                .id(win_id)
                .open(&mut open)
                .default_pos(fg.pos)
                .default_size(fg.size)
                .show(ctx, |ui| {
                    // Tab bar inside floating window
                    let tab_bar_rect =
                        ui.allocate_space(egui::vec2(ui.available_width(), TAB_BAR_HEIGHT)).1;
                    render_tab_bar(ctx, layout, fg.group_id, group, tab_bar_rect, &mut actions);

                    // Content
                    let active = group.active_tab_entry();
                    let panel_id = active.panel_id;
                    let panel_type = active.panel_type;
                    crate::ui::draw_panel(panel_type, ui, state, panel_id);
                });

            if !open {
                // Close all tabs in the floating group
                for i in (0..group.tabs.len()).rev() {
                    actions.push(LayoutAction::Close {
                        group_id: fg.group_id,
                        tab_index: i,
                    });
                }
            }
        }
    }

    // --- Dividers ---
    render_dividers(ctx, layout, available_rect, &mut actions);

    // --- Drag ghost and drop zones ---
    if let Some(drag) = &layout.drag {
        render_drag_overlay(ctx, drag, &group_rects, layout, &mut actions);
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
        painter.rect_filled(line_rect, 0.0, DIVIDER_COLOR);

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
                    && let Some(pos) = ui.ctx().pointer_interact_pos() {
                        let new_ratio = match direction {
                            SplitDirection::Vertical => {
                                (pos.x - parent_rect.min.x) / parent_rect.width()
                            }
                            SplitDirection::Horizontal => {
                                (pos.y - parent_rect.min.y) / parent_rect.height()
                            }
                        };
                        actions.push(LayoutAction::Resize {
                            node_id,
                            new_ratio,
                        });
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
        let ghost_layer = egui::LayerId::new(
            egui::Order::Tooltip,
            egui::Id::new("drag_ghost"),
        );
        let painter = ctx.layer_painter(ghost_layer);
        let text = drag.panel_type.display_name();
        let font = egui::FontId::proportional(13.0);
        let galley = painter.layout_no_wrap(text.to_string(), font, TEXT_BRIGHT);
        let text_rect = egui::Rect::from_min_size(
            pointer_pos + egui::vec2(12.0, -8.0),
            galley.size(),
        )
        .expand(4.0);
        painter.rect_filled(
            text_rect,
            4.0,
            egui::Color32::from_rgba_premultiplied(0x1e, 0x1e, 0x2e, 0xd0),
        );
        painter.galley(
            text_rect.min + egui::vec2(4.0, 4.0),
            galley,
            TEXT_BRIGHT,
        );

        // Drop zone overlays on grid groups
        let mut hovered_group: Option<(GroupId, DropZone, egui::Rect)> = None;
        for &(gid, rect) in group_rects {
            if gid == drag.source_group
                && layout
                    .groups
                    .get(&gid)
                    .is_some_and(|g| g.tabs.len() <= 1)
            {
                continue;
            }
            if rect.contains(pointer_pos) {
                let zone = hit_test_drop_zone(rect, pointer_pos);
                hovered_group = Some((gid, zone, rect));
                break;
            }
        }

        if let Some((_, zone, group_rect)) = &hovered_group {
            let highlight = drop_zone_highlight_rect(*group_rect, *zone);
            let overlay_layer = egui::LayerId::new(
                egui::Order::Foreground,
                egui::Id::new("drop_overlay"),
            );
            let overlay_painter = ctx.layer_painter(overlay_layer);
            overlay_painter.rect_filled(highlight, 0.0, DROP_ZONE_TINT);
        }

        // On mouse release: emit drop action
        if ctx.input(|i| i.pointer.any_released()) {
            if let Some((target_gid, zone, _)) = hovered_group {
                actions.push(LayoutAction::DropOnZone {
                    target_group: target_gid,
                    zone,
                });
            } else {
                actions.push(LayoutAction::DropOnEmpty { pos: pointer_pos });
            }
        }
    } else {
        actions.push(LayoutAction::CancelDrag);
    }
}

// ---------------------------------------------------------------------------
// Tab bar rendering
// ---------------------------------------------------------------------------

/// Render the tab bar for a group. Emits actions for tab clicks, drags, close, and context menus.
fn render_tab_bar(
    ctx: &egui::Context,
    layout: &DockLayout,
    group_id: GroupId,
    group: &super::tree::Group,
    tab_bar_rect: egui::Rect,
    actions: &mut Vec<LayoutAction>,
) {
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Middle,
        egui::Id::new(("tab_bar_bg", group_id.0)),
    ));
    painter.rect_filled(tab_bar_rect, 0.0, TAB_BAR_BG);

    let tab_count = group.tabs.len();
    let max_tab_width = 160.0_f32;
    let tab_width = if tab_count > 0 {
        (tab_bar_rect.width() / tab_count as f32).min(max_tab_width)
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
        let is_dragging = layout.drag.is_none();

        egui::Area::new(tab_area_id)
            .fixed_pos(tab_rect.min)
            .sense(egui::Sense::click_and_drag())
            .show(ctx, |ui| {
                let response =
                    ui.allocate_response(tab_rect.size(), egui::Sense::click_and_drag());

                // Background
                let bg = if is_active {
                    TAB_ACTIVE_BG
                } else if response.hovered() {
                    TAB_HOVER_BG
                } else {
                    TAB_BAR_BG
                };
                painter.rect_filled(tab_rect, 0.0, bg);

                // Accent line for active tab
                if is_active {
                    let accent_rect =
                        egui::Rect::from_min_size(tab_rect.min, egui::vec2(tab_width, 2.0));
                    painter.rect_filled(accent_rect, 0.0, TAB_ACCENT);
                }

                // Label
                let text_color = if is_active { TEXT_BRIGHT } else { TEXT_DIM };
                let label_pos =
                    egui::pos2(tab_rect.min.x + 8.0, tab_rect.center().y - 6.0);
                let available_text_width = tab_width - 28.0;
                let font = egui::FontId::proportional(12.0);
                let galley = painter.layout(
                    tab.panel_type.display_name().to_string(),
                    font,
                    text_color,
                    available_text_width.max(10.0),
                );
                painter.galley(label_pos, galley, text_color);

                // Close button (visible on hover or if active)
                if response.hovered() || is_active {
                    let close_center =
                        egui::pos2(tab_rect.max.x - 12.0, tab_rect.center().y);
                    let close_rect =
                        egui::Rect::from_center_size(close_center, egui::vec2(14.0, 14.0));
                    let close_resp =
                        ui.allocate_rect(close_rect, egui::Sense::click());

                    let close_color = if close_resp.hovered() {
                        TEXT_BRIGHT
                    } else {
                        TEXT_DIM
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

                    if close_resp.clicked() {
                        actions.push(LayoutAction::Close {
                            group_id: gid,
                            tab_index: tab_idx,
                        });
                    }
                }

                // Click to activate
                if response.clicked() {
                    actions.push(LayoutAction::SetActiveTab {
                        group_id: gid,
                        tab_index: tab_idx,
                    });
                }

                // Drag to start
                if response.drag_started() && is_dragging {
                    actions.push(LayoutAction::StartDrag {
                        group_id: gid,
                        tab_index: tab_idx,
                    });
                }

                // Context menu
                response.context_menu(|ui: &mut egui::Ui| {
                    ui.menu_button("Add Panel", |ui: &mut egui::Ui| {
                        for &pt in DOCKABLE_TYPES {
                            if ui.button(pt.display_name()).clicked() {
                                actions.push(LayoutAction::AddPanel {
                                    target_group: gid,
                                    panel_type: pt,
                                });
                                ui.close();
                            }
                        }
                    });
                    ui.separator();
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
) {
    let active = group.active_tab_entry();
    let panel_id = active.panel_id;
    let panel_type = active.panel_type;

    egui::Area::new(egui::Id::new(("panel_content", group_id.0)))
        .fixed_pos(content_rect.min)
        .sense(egui::Sense::hover())
        .show(ctx, |ui| {
            let painter = ui.painter();
            painter.rect_filled(content_rect, 0.0, CONTENT_BG);

            ui.set_min_size(content_rect.size());
            ui.set_max_size(content_rect.size());
            crate::ui::draw_panel(panel_type, ui, state, panel_id);
        });
}
