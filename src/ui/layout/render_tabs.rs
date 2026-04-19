//! Tab bar and content area rendering for dockview groups.

use super::render::{DOCKABLE_TYPES, LayoutAction, paint_grip_dots};
use super::tree::{DockLayout, GroupId};

use crate::ui::theme::{ADD_BUTTON_WIDTH, DOCK_GRIP_WIDTH, active_theme};

/// Context flags for tab bar rendering.
pub(crate) struct TabBarContext {
    pub is_main: bool,
    pub is_floating: bool,
    /// Layer order for painters and areas. Docked groups use `Middle`,
    /// floating groups use `Foreground` so they paint above docked content.
    pub order: egui::Order,
}

/// Render the tab bar for a group. Emits actions for tab clicks, drags, close, and context menus.
pub(crate) fn render_tab_bar(
    ctx: &egui::Context,
    layout: &DockLayout,
    group_id: GroupId,
    group: &super::tree::Group,
    tab_bar_rect: egui::Rect,
    actions: &mut Vec<LayoutAction>,
    tctx: TabBarContext,
) {
    let theme = active_theme(ctx);
    let painter = ctx.layer_painter(egui::LayerId::new(
        tctx.order,
        egui::Id::new(("tab_bar_bg", group_id.0)),
    ));
    painter.rect_filled(tab_bar_rect, 0.0, theme.bg_surface);

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
            egui::vec2(tab_width, theme.tab_bar_height),
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
                let tab_theme = active_theme(ui.ctx());
                let bg = if is_active || response.hovered() {
                    tab_theme.bg_elevated
                } else {
                    tab_theme.bg_surface
                };
                painter.rect_filled(tab_rect, 0.0, bg);

                // Accent line at bottom of active tab
                if is_active {
                    let accent_rect = egui::Rect::from_min_size(
                        egui::pos2(tab_rect.min.x, tab_rect.max.y - 2.0),
                        egui::vec2(tab_width, 2.0),
                    );
                    painter.rect_filled(accent_rect, 0.0, tab_theme.accent);
                }

                // Label — truncate with ellipsis when too wide
                let text_color = if is_active {
                    tab_theme.text_primary
                } else {
                    tab_theme.text_secondary
                };
                let label_pos = egui::pos2(tab_rect.min.x + 8.0, tab_rect.center().y - 6.0);
                let available_text_width = (tab_width - 28.0).max(10.0);
                let font = egui::FontId::proportional(12.0);
                let full_name = tab.panel_type.display_name();
                let galley =
                    painter.layout_no_wrap(full_name.to_string(), font.clone(), text_color);
                if galley.size().x > available_text_width {
                    let ellipsis = "\u{2026}";
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
                        tab_theme.text_primary
                    } else {
                        tab_theme.text_secondary
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
                    use crate::ui::widgets::menu::{menu_item, styled_menu};
                    styled_menu(ui, |ui| {
                        if tctx.is_floating {
                            if menu_item(ui, "Dock to Grid") {
                                actions.push(LayoutAction::DockFloatingToGrid { group_id: gid });
                                ui.close();
                            }
                            if menu_item(ui, "Pop Out to Window") {
                                actions.push(LayoutAction::DetachToWindow {
                                    group_id: gid,
                                    tab_index: tab_idx,
                                });
                                ui.close();
                            }
                        } else if tctx.is_main {
                            if menu_item(ui, "Detach") {
                                actions.push(LayoutAction::DetachToFloat {
                                    group_id: gid,
                                    tab_index: tab_idx,
                                });
                                ui.close();
                            }
                            if menu_item(ui, "Pop Out to Window") {
                                actions.push(LayoutAction::DetachToWindow {
                                    group_id: gid,
                                    tab_index: tab_idx,
                                });
                                ui.close();
                            }
                        } else {
                            if menu_item(ui, "Reattach to Main Window") {
                                actions.push(LayoutAction::ReattachToMain);
                                ui.close();
                            }
                        }
                        ui.separator();
                        if tab_count > 1 && menu_item(ui, "Close Others") {
                            actions.push(LayoutAction::CloseOthers {
                                group_id: gid,
                                tab_index: tab_idx,
                            });
                            ui.close();
                        }
                        if menu_item(ui, "Close") {
                            actions.push(LayoutAction::Close {
                                group_id: gid,
                                tab_index: tab_idx,
                            });
                            ui.close();
                        }
                    });
                });
            });
    }

    // "+" button after the last tab — painted inline, no separate Area.
    // We use the tab bar painter for visuals and a dedicated egui::Area only
    // for the menu popup (which needs its own layer to size freely).
    let plus_x = tab_bar_rect.min.x + tab_count as f32 * tab_width;
    let plus_rect = egui::Rect::from_min_size(
        egui::pos2(plus_x, tab_bar_rect.min.y),
        egui::vec2(ADD_BUTTON_WIDTH, theme.tab_bar_height),
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
        painter.rect_filled(plus_rect, 0.0, theme.bg_elevated);
    }
    painter.text(
        plus_rect.center(),
        egui::Align2::CENTER_CENTER,
        "+",
        egui::FontId::proportional(14.0),
        if plus_hovered {
            theme.text_primary
        } else {
            theme.text_secondary
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
                    let mut sorted_types = DOCKABLE_TYPES.to_vec();
                    sorted_types.sort_by_key(|pt| pt.display_name());
                    for &pt in &sorted_types {
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
        egui::vec2(DOCK_GRIP_WIDTH, theme.tab_bar_height),
    );
    // Paint grip dots (2x3 grid)
    paint_grip_dots(&painter, dock_rect.center(), theme.text_secondary);
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
        use crate::ui::widgets::menu::{menu_item, styled_menu};
        styled_menu(ui, |ui| {
            if tctx.is_floating {
                if menu_item(ui, "Dock to Grid") {
                    actions.push(LayoutAction::DockFloatingToGrid { group_id: gid });
                    ui.close();
                }
            } else if tctx.is_main {
                if menu_item(ui, "Detach to Float") {
                    actions.push(LayoutAction::DetachGroupToFloat { group_id: gid });
                    ui.close();
                }
            }
            if menu_item(ui, "Pop Out to Window") {
                actions.push(LayoutAction::DetachToWindow {
                    group_id: gid,
                    tab_index: group.active_tab,
                });
                ui.close();
            }
            if tab_count > 0 {
                ui.separator();
                if menu_item(ui, "Close Group") {
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
    });
}

/// Render the active panel content for a group.
pub(crate) fn render_content(
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
            let content_theme = active_theme(ui.ctx());
            ui.painter()
                .rect_filled(content_rect, 0.0, content_theme.bg_panel);

            ui.set_min_size(content_rect.size());
            ui.set_max_size(content_rect.size());

            let padded_rect = content_rect.shrink(content_theme.panel_padding);
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
