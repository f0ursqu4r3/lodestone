use super::interactions::collect_dividers;
use super::{LayoutTree, NodeId, PanelType, SplitDirection};

#[allow(dead_code)]
pub enum LayoutAction {
    Resize {
        node_id: NodeId,
        new_ratio: f32,
    },
    SwapType {
        node_id: NodeId,
        new_type: PanelType,
    },
    Close {
        node_id: NodeId,
    },
    Detach {
        node_id: NodeId,
    },
    Duplicate {
        node_id: NodeId,
    },
    Split {
        node_id: NodeId,
        direction: SplitDirection,
    },
    Merge {
        node_id: NodeId,
        keep: super::MergeSide,
    },
    SplitWithType {
        node_id: NodeId,
        direction: SplitDirection,
        new_type: PanelType,
    },
    ResetLayout,
}

/// All dockable panel types for the type selector dropdown.
pub const DOCKABLE_TYPES: &[PanelType] = &[
    PanelType::Preview,
    PanelType::SceneEditor,
    PanelType::AudioMixer,
    PanelType::StreamControls,
];

/// Render the top menu bar. Returns layout actions and the remaining rect below the bar.
pub fn render_menu_bar(
    ctx: &egui::Context,
    layout: &LayoutTree,
) -> (Vec<LayoutAction>, egui::Rect) {
    let mut actions = Vec::new();
    let root_node_id = layout.root_id();

    egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("View", |ui| {
                ui.menu_button("Add Panel", |ui| {
                    for &pt in DOCKABLE_TYPES {
                        if ui.button(pt.display_name()).clicked() {
                            actions.push(LayoutAction::SplitWithType {
                                node_id: root_node_id,
                                direction: SplitDirection::Vertical,
                                new_type: pt,
                            });
                            ui.close();
                        }
                    }
                });
                if ui.button("Reset Layout").clicked() {
                    actions.push(LayoutAction::ResetLayout);
                    ui.close();
                }
            });
        });
    });

    let available_rect = ctx.available_rect();
    (actions, available_rect)
}

pub fn render_layout(
    ctx: &egui::Context,
    layout: &LayoutTree,
    state: &mut crate::state::AppState,
    available_rect: egui::Rect,
) -> Vec<LayoutAction> {
    let mut actions = Vec::new();

    // Snapshot leaves with their computed rects
    let leaves = layout.collect_leaves_with_rects(available_rect);
    let leaf_count = leaves.len();

    // Draw each panel in its allocated rect
    for (panel_id, panel_type, rect, node_id) in &leaves {
        let panel_id = *panel_id;
        let panel_type = *panel_type;
        let node_id = *node_id;
        let rect = *rect;

        // Use Sense::hover() so the panel Area does not consume drag events that belong
        // to divider Areas which are rendered at the same Order::Foreground layer.
        egui::Area::new(egui::Id::new(("panel", panel_id.0)))
            .fixed_pos(rect.min)
            .sense(egui::Sense::hover())
            .show(ctx, |ui| {
                ui.set_min_size(rect.size());
                ui.set_max_size(rect.size());

                // --- Panel Header ---
                let header_response = ui.horizontal(|ui| {
                    // Panel type dropdown
                    let combo_id = egui::Id::new(("panel_type_combo", panel_id.0));
                    let mut selected = panel_type;
                    let combo = egui::ComboBox::from_id_salt(combo_id)
                        .selected_text(selected.display_name())
                        .width(120.0)
                        .show_ui(ui, |ui| {
                            for &pt in DOCKABLE_TYPES {
                                if ui
                                    .selectable_value(&mut selected, pt, pt.display_name())
                                    .clicked()
                                {
                                    // selection changed
                                }
                            }
                        });
                    let _ = combo;
                    if selected != panel_type {
                        actions.push(LayoutAction::SwapType {
                            node_id,
                            new_type: selected,
                        });
                    }

                    // Spacer to push buttons to the right
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // Close button -- disabled if this is the only leaf
                        let close_btn =
                            ui.add_enabled(leaf_count > 1, egui::Button::new("\u{00d7}").small());
                        if close_btn.clicked() {
                            actions.push(LayoutAction::Close { node_id });
                        }

                        // Move/drag glyph -- visual-only for now
                        let _move_btn = ui.add_enabled(
                            false,
                            egui::Button::new("\u{2807}\u{2807}").small(),
                        );
                    });
                });

                // Context menu on the header.
                let header_interactive = header_response.response.interact(egui::Sense::click());
                header_interactive.context_menu(|ui| {
                    ui.menu_button("Add", |ui| {
                        for &pt in DOCKABLE_TYPES {
                            if ui.button(pt.display_name()).clicked() {
                                actions.push(LayoutAction::SplitWithType {
                                    node_id,
                                    direction: SplitDirection::Vertical,
                                    new_type: pt,
                                });
                                ui.close();
                            }
                        }
                    });
                    if ui.button("Detach to Window").clicked() {
                        actions.push(LayoutAction::Detach { node_id });
                        ui.close();
                    }
                    if ui.button("Duplicate").clicked() {
                        actions.push(LayoutAction::Duplicate { node_id });
                        ui.close();
                    }
                });

                ui.separator();

                // Panel content
                crate::ui::draw_panel(panel_type, ui, state, panel_id);
            });
    }

    // --- Divider rendering and drag interaction ---
    let dividers = collect_dividers(layout, available_rect);
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("layout_dividers"),
    ));

    for divider in &dividers {
        // Paint a thin visible line (1px) at the center of the 4px hit area.
        let line_color = egui::Color32::from_gray(60);
        match divider.direction {
            SplitDirection::Vertical => {
                let center_x = divider.rect.center().x;
                painter.line_segment(
                    [
                        egui::pos2(center_x, divider.rect.min.y),
                        egui::pos2(center_x, divider.rect.max.y),
                    ],
                    egui::Stroke::new(1.0, line_color),
                );
            }
            SplitDirection::Horizontal => {
                let center_y = divider.rect.center().y;
                painter.line_segment(
                    [
                        egui::pos2(divider.rect.min.x, center_y),
                        egui::pos2(divider.rect.max.x, center_y),
                    ],
                    egui::Stroke::new(1.0, line_color),
                );
            }
        }

        // Drag interaction
        let divider_id = egui::Id::new(("divider", divider.node_id.0));
        let node_id = divider.node_id;
        let direction = divider.direction;
        let parent_rect = divider.parent_rect;
        let hit_rect = divider.rect;

        let area_response = egui::Area::new(divider_id)
            .fixed_pos(hit_rect.min)
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                ui.allocate_rect(
                    egui::Rect::from_min_size(hit_rect.min, hit_rect.size()),
                    egui::Sense::drag(),
                )
            });

        let response = area_response.inner;

        if response.hovered() || response.dragged() {
            match direction {
                SplitDirection::Vertical => {
                    ctx.set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
                }
                SplitDirection::Horizontal => {
                    ctx.set_cursor_icon(egui::CursorIcon::ResizeVertical);
                }
            }
        }

        if response.dragged()
            && let Some(pointer_pos) = ctx.pointer_interact_pos()
        {
            let new_ratio = match direction {
                SplitDirection::Vertical => {
                    (pointer_pos.x - parent_rect.min.x) / parent_rect.width()
                }
                SplitDirection::Horizontal => {
                    (pointer_pos.y - parent_rect.min.y) / parent_rect.height()
                }
            };
            actions.push(LayoutAction::Resize {
                node_id,
                new_ratio: new_ratio.clamp(0.1, 0.9),
            });
        }
    }

    actions
}
