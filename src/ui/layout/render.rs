//! Rendering for the dockview-style layout system.
//!
//! Provides [`render_menu_bar`] and [`render_layout`] which produce [`LayoutAction`]
//! values for the window to apply after the egui frame completes.

use super::interactions::{collect_dividers, drop_zone_highlight_rect, hit_test_drop_zone};
use super::tree::{DockLayout, DropZone, GroupId, NodeId, PanelType, SplitDirection};

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
const PANEL_PADDING: f32 = 6.0;
const ADD_BUTTON_WIDTH: f32 = 28.0;

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
                    },
                );
                render_content(ctx, group_id, group, content_rect, state);
            }
        }
    }

    // --- Floating groups ---
    // Rendered as egui::Window with custom dark styling. We render tab bar inline
    // using the window's own UI so the window sizes correctly.
    let floating_snapshot: Vec<_> = layout.floating.clone();
    for fg in &floating_snapshot {
        if let Some(group) = layout.groups.get(&fg.group_id) {
            let win_id = egui::Id::new(("floating_group", fg.group_id.0));
            let dark_frame = egui::Frame {
                fill: CONTENT_BG,
                stroke: egui::Stroke::new(1.0, egui::Color32::from_gray(50)),
                shadow: egui::Shadow {
                    offset: [0, 4],
                    blur: 16,
                    spread: 4,
                    color: egui::Color32::from_black_alpha(120),
                },
                inner_margin: egui::Margin::ZERO,
                ..Default::default()
            };

            // Track open state for close button
            let open_state_id = egui::Id::new(("floating_open", fg.group_id.0));
            let mut is_open = ctx.data(|d| d.get_temp::<bool>(open_state_id).unwrap_or(true));

            let active_name = group.active_tab_entry().panel_type.display_name();
            let win_response = egui::Window::new(active_name)
                .id(win_id)
                .title_bar(true)
                .frame(dark_frame)
                .default_pos(fg.pos)
                .default_size(fg.size)
                .min_size(egui::vec2(200.0, 100.0))
                .resizable(true)
                .order(egui::Order::Foreground)
                .collapsible(true)
                .open(&mut is_open)
                .show(ctx, |ui| {
                    let fgid = fg.group_id;
                    let tab_count = group.tabs.len();

                    // --- Inline tab bar ---
                    let (tab_bar_rect, _) = ui.allocate_exact_size(
                        egui::vec2(ui.available_width(), TAB_BAR_HEIGHT),
                        egui::Sense::hover(),
                    );
                    let painter = ui.painter();

                    // Tab bar background
                    painter.rect_filled(tab_bar_rect, 0.0, TAB_BAR_BG);

                    // Tabs start at the left edge (no collapse button)
                    let tabs_start_x = tab_bar_rect.min.x;

                    let available_for_tabs =
                        tab_bar_rect.width() - ADD_BUTTON_WIDTH;
                    let max_tab_width = 160.0_f32;
                    let tab_width = if tab_count > 0 {
                        (available_for_tabs / tab_count as f32).min(max_tab_width)
                    } else {
                        max_tab_width
                    };

                    for (i, tab) in group.tabs.iter().enumerate() {
                        let is_active = i == group.active_tab;
                        let tab_rect = egui::Rect::from_min_size(
                            egui::pos2(tabs_start_x + i as f32 * tab_width, tab_bar_rect.min.y),
                            egui::vec2(tab_width, TAB_BAR_HEIGHT),
                        );
                        let tab_id = egui::Id::new(("ftab", fgid.0, i));
                        let response = ui.interact(tab_rect, tab_id, egui::Sense::click_and_drag());

                        // Background
                        let bg = if is_active {
                            TAB_ACTIVE_BG
                        } else if response.hovered() {
                            TAB_HOVER_BG
                        } else {
                            TAB_BAR_BG
                        };
                        painter.rect_filled(tab_rect, 0.0, bg);

                        // Accent
                        if is_active {
                            painter.rect_filled(
                                egui::Rect::from_min_size(
                                    egui::pos2(tab_rect.min.x, tab_rect.max.y - 2.0),
                                    egui::vec2(tab_width, 2.0),
                                ),
                                0.0,
                                TAB_ACCENT,
                            );
                        }

                        // Label
                        let text_color = if is_active { TEXT_BRIGHT } else { TEXT_DIM };
                        let galley = painter.layout(
                            tab.panel_type.display_name().to_string(),
                            egui::FontId::proportional(12.0),
                            text_color,
                            (tab_width - 28.0).max(10.0),
                        );
                        painter.galley(
                            egui::pos2(tab_rect.min.x + 8.0, tab_rect.center().y - 6.0),
                            galley,
                            text_color,
                        );

                        // Close button (visible only when hovering the tab)
                        if response.hovered() {
                            let cc = egui::pos2(tab_rect.max.x - 12.0, tab_rect.center().y);
                            let cr = egui::Rect::from_center_size(cc, egui::vec2(14.0, 14.0));
                            let close_resp = ui.interact(
                                cr,
                                egui::Id::new(("ftab_close", fgid.0, i)),
                                egui::Sense::click(),
                            );
                            let close_color = if close_resp.hovered() {
                                TEXT_BRIGHT
                            } else {
                                TEXT_DIM
                            };
                            let s = 3.5;
                            painter.line_segment(
                                [cc - egui::vec2(s, s), cc + egui::vec2(s, s)],
                                egui::Stroke::new(1.5, close_color),
                            );
                            painter.line_segment(
                                [cc + egui::vec2(-s, s), cc + egui::vec2(s, -s)],
                                egui::Stroke::new(1.5, close_color),
                            );
                            if close_resp.clicked() {
                                actions.push(LayoutAction::Close {
                                    group_id: fgid,
                                    tab_index: i,
                                });
                            }
                        }

                        if response.clicked() {
                            actions.push(LayoutAction::SetActiveTab {
                                group_id: fgid,
                                tab_index: i,
                            });
                        }

                        // Context menu
                        let tab_idx = i;
                        response.context_menu(|ui: &mut egui::Ui| {
                            if ui.button("Dock to Grid").clicked() {
                                actions.push(LayoutAction::DockFloatingToGrid { group_id: fgid });
                                ui.close();
                            }
                            if ui.button("Pop Out to Window").clicked() {
                                actions.push(LayoutAction::DetachToWindow {
                                    group_id: fgid,
                                    tab_index: tab_idx,
                                });
                                ui.close();
                            }
                            ui.separator();
                            if tab_count > 1 && ui.button("Close Others").clicked() {
                                actions.push(LayoutAction::CloseOthers {
                                    group_id: fgid,
                                    tab_index: tab_idx,
                                });
                                ui.close();
                            }
                            if ui.button("Close").clicked() {
                                actions.push(LayoutAction::Close {
                                    group_id: fgid,
                                    tab_index: tab_idx,
                                });
                                ui.close();
                            }
                        });
                    }

                    // "+" button (after collapse button + tabs)
                    let plus_x = tabs_start_x + tab_count as f32 * tab_width;
                    let plus_rect = egui::Rect::from_min_size(
                        egui::pos2(plus_x, tab_bar_rect.min.y),
                        egui::vec2(ADD_BUTTON_WIDTH, TAB_BAR_HEIGHT),
                    );
                    let plus_resp = ui.interact(
                        plus_rect,
                        egui::Id::new(("ftab_add", fgid.0)),
                        egui::Sense::click(),
                    );
                    let pc = if plus_resp.hovered() { TEXT_BRIGHT } else { TEXT_DIM };
                    if plus_resp.hovered() {
                        painter.rect_filled(plus_rect, 0.0, TAB_HOVER_BG);
                    }
                    painter.text(
                        plus_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "+",
                        egui::FontId::proportional(14.0),
                        pc,
                    );

                    // Toggle popup on click
                    let popup_state_id = egui::Id::new(("fadd_popup_open", fgid.0));
                    let was_open: bool = ctx.data(|d| d.get_temp(popup_state_id).unwrap_or(false));
                    let mut is_open = was_open;
                    if plus_resp.clicked() {
                        is_open = !is_open;
                    }
                    if is_open {
                        let popup_pos = egui::pos2(plus_rect.min.x, plus_rect.max.y + 2.0);
                        egui::Area::new(egui::Id::new(("fadd_panel_popup", fgid.0)))
                            .fixed_pos(popup_pos)
                            .order(egui::Order::Foreground)
                            .show(ctx, |ui| {
                                egui::Frame::popup(ui.style()).show(ui, |ui| {
                                    ui.set_min_width(120.0);
                                    for &pt in DOCKABLE_TYPES {
                                        if ui.selectable_label(false, pt.display_name()).clicked() {
                                            actions.push(LayoutAction::AddPanel {
                                                target_group: fgid,
                                                panel_type: pt,
                                            });
                                            is_open = false;
                                        }
                                    }
                                });
                            });
                        if was_open
                            && !plus_resp.clicked()
                            && ctx.input(|i| i.pointer.any_pressed())
                            && let Some(pos) = ctx.pointer_interact_pos()
                        {
                            let popup_id = egui::Id::new(("fadd_panel_popup", fgid.0));
                            let on_popup = ctx
                                .layer_id_at(pos)
                                .is_some_and(|layer| layer.id == popup_id);
                            if !on_popup && !plus_rect.contains(pos) {
                                is_open = false;
                            }
                        }
                    }
                    ctx.data_mut(|d| d.insert_temp(popup_state_id, is_open));

                    // --- Content area (frame fill is already CONTENT_BG) ---
                    let active = group.active_tab_entry();
                    ui.add_space(PANEL_PADDING);
                    crate::ui::draw_panel(active.panel_type, ui, state, active.panel_id);
                });

            // Store the floating window's actual rect for drop target hit testing
            if let Some(ref inner_response) = win_response {
                let rect_id = egui::Id::new(("floating_rect", fg.group_id.0));
                ctx.data_mut(|d| d.insert_temp(rect_id, inner_response.response.rect));

                // Shift+drag on title bar: enter group dock mode
                let shift_held = ctx.input(|i| i.modifiers.shift);
                if shift_held && inner_response.response.dragged() {
                    let dock_drag_id = egui::Id::new("floating_dock_drag");
                    ctx.data_mut(|d| d.insert_temp(dock_drag_id, fg.group_id));
                }
            }

            // Handle close button (is_open set to false by egui)
            ctx.data_mut(|d| d.insert_temp(open_state_id, is_open));
            if !is_open {
                actions.push(LayoutAction::CloseFloatingGroup {
                    group_id: fg.group_id,
                });
            }
        }
    }

    // --- Collect floating group rects for drop targeting ---
    // Floating groups are checked first (higher z-order) so they take
    // priority over grid groups they overlap.
    let mut all_drop_rects: Vec<(GroupId, egui::Rect)> = Vec::new();
    for fg in &floating_snapshot {
        let rect_id = egui::Id::new(("floating_rect", fg.group_id.0));
        if let Some(rect) = ctx.data(|d| d.get_temp::<egui::Rect>(rect_id)) {
            all_drop_rects.push((fg.group_id, rect));
        }
    }
    all_drop_rects.extend_from_slice(&group_rects);

    // --- Dividers ---
    render_dividers(ctx, layout, available_rect, &mut actions);

    // --- Drag ghost and drop zones ---
    if let Some(drag) = &layout.drag {
        render_drag_overlay(ctx, drag, &all_drop_rects, layout, &mut actions);
    }

    // --- Shift+drag floating group dock overlay ---
    let dock_drag_id = egui::Id::new("floating_dock_drag");
    let shift_held = ctx.input(|i| i.modifiers.shift);
    if let Some(dragging_gid) = ctx.data(|d| d.get_temp::<GroupId>(dock_drag_id)) {
        if shift_held {
            if let Some(pointer_pos) = ctx.pointer_interact_pos() {
                // Show drop zone overlay on grid groups
                let mut hovered_group: Option<(GroupId, DropZone, egui::Rect)> = None;
                for &(gid, rect) in &group_rects {
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
                        egui::Id::new("dock_group_overlay"),
                    );
                    let overlay_painter = ctx.layer_painter(overlay_layer);
                    overlay_painter.rect_filled(highlight, 0.0, DROP_ZONE_TINT);
                }

                // On release: dock the floating group into the target
                if ctx.input(|i| i.pointer.any_released()) {
                    if let Some((_target_gid, _zone, _)) = hovered_group {
                        actions.push(LayoutAction::DockFloatingToGrid {
                            group_id: dragging_gid,
                        });
                    }
                    ctx.data_mut(|d| d.remove::<GroupId>(dock_drag_id));
                }
            }
        } else {
            // Shift released — cancel dock drag
            ctx.data_mut(|d| d.remove::<GroupId>(dock_drag_id));
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
        let galley = painter.layout_no_wrap(text.to_string(), font, TEXT_BRIGHT);
        let text_rect =
            egui::Rect::from_min_size(pointer_pos + egui::vec2(12.0, -8.0), galley.size())
                .expand(4.0);
        painter.rect_filled(
            text_rect,
            4.0,
            egui::Color32::from_rgba_premultiplied(0x1e, 0x1e, 0x2e, 0xd0),
        );
        painter.galley(text_rect.min + egui::vec2(4.0, 4.0), galley, TEXT_BRIGHT);

        // Drop zone overlays on grid groups
        let mut hovered_group: Option<(GroupId, DropZone, egui::Rect)> = None;
        for &(gid, rect) in group_rects {
            if gid == drag.source_group
                && layout.groups.get(&gid).is_some_and(|g| g.tabs.len() <= 1)
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
        egui::Order::Middle,
        egui::Id::new(("tab_bar_bg", group_id.0)),
    ));
    painter.rect_filled(tab_bar_rect, 0.0, TAB_BAR_BG);

    let tab_count = group.tabs.len();
    let max_tab_width = 160.0_f32;
    // Reserve space for the "+" button at the end of the tab bar
    let available_for_tabs = tab_bar_rect.width() - ADD_BUTTON_WIDTH;
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
            .sense(egui::Sense::click_and_drag())
            .show(ctx, |ui| {
                let response = ui.allocate_response(tab_rect.size(), egui::Sense::click_and_drag());

                // Background
                let bg = if is_active {
                    TAB_ACTIVE_BG
                } else if response.hovered() {
                    TAB_HOVER_BG
                } else {
                    TAB_BAR_BG
                };
                painter.rect_filled(tab_rect, 0.0, bg);

                // Accent line at bottom of active tab
                if is_active {
                    let accent_rect = egui::Rect::from_min_size(
                        egui::pos2(tab_rect.min.x, tab_rect.max.y - 2.0),
                        egui::vec2(tab_width, 2.0),
                    );
                    painter.rect_filled(accent_rect, 0.0, TAB_ACCENT);
                }

                // Label
                let text_color = if is_active { TEXT_BRIGHT } else { TEXT_DIM };
                let label_pos = egui::pos2(tab_rect.min.x + 8.0, tab_rect.center().y - 6.0);
                let available_text_width = tab_width - 28.0;
                let font = egui::FontId::proportional(12.0);
                let galley = painter.layout(
                    tab.panel_type.display_name().to_string(),
                    font,
                    text_color,
                    available_text_width.max(10.0),
                );
                painter.galley(label_pos, galley, text_color);

                // Close button (visible only when hovering the tab)
                if response.hovered() {
                    let close_center = egui::pos2(tab_rect.max.x - 12.0, tab_rect.center().y);
                    let close_rect =
                        egui::Rect::from_center_size(close_center, egui::vec2(14.0, 14.0));
                    let close_resp = ui.allocate_rect(close_rect, egui::Sense::click());

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
        .sense(egui::Sense::click())
        .show(ctx, |ui| {
            ui.allocate_exact_size(plus_rect.size(), egui::Sense::click())
                .1
        })
        .inner;

    // Paint the "+" on the tab bar painter (not inside the Area)
    let plus_hovered = plus_response.hovered();
    if plus_hovered {
        painter.rect_filled(plus_rect, 0.0, TAB_HOVER_BG);
    }
    painter.text(
        plus_rect.center(),
        egui::Align2::CENTER_CENTER,
        "+",
        egui::FontId::proportional(14.0),
        if plus_hovered { TEXT_BRIGHT } else { TEXT_DIM },
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

            // Add inner padding around panel content
            let padded_rect = content_rect.shrink(PANEL_PADDING);
            let mut padded_ui = ui.new_child(egui::UiBuilder::new().max_rect(padded_rect));
            crate::ui::draw_panel(panel_type, &mut padded_ui, state, panel_id);
        });
}
