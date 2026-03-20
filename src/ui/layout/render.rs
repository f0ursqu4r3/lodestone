use super::interactions::collect_dividers;
use super::{LayoutTree, MergeSide, NodeId, PanelType, SplitDirection};

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
        keep: MergeSide,
    },
}

/// All dockable panel types for the type selector dropdown.
const DOCKABLE_TYPES: &[PanelType] = &[
    PanelType::Preview,
    PanelType::SceneEditor,
    PanelType::AudioMixer,
    PanelType::StreamControls,
];

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
        // to corner handle Areas which are rendered at the same Order::Foreground layer.
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

                    // Spacer to push close button to the right
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // Close button — disabled if this is the only leaf
                        let close_btn =
                            ui.add_enabled(leaf_count > 1, egui::Button::new("×").small());
                        if close_btn.clicked() {
                            actions.push(LayoutAction::Close { node_id });
                        }
                    });
                });

                // Context menu on the header.
                // The InnerResponse from ui.horizontal() carries a response with no Sense
                // allocation, so right-click events pass through it. Calling .interact() adds
                // Sense::click() to that rect so context_menu() can detect secondary clicks.
                let header_interactive = header_response.response.interact(egui::Sense::click());
                header_interactive.context_menu(|ui| {
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

    // --- Corner handles for split & merge gestures ---
    render_corner_handles(ctx, layout, &leaves, &mut actions);

    actions
}

/// Size of each corner handle in logical pixels.
const HANDLE_SIZE: f32 = 12.0;
/// Drag threshold in pixels before a split/merge gesture triggers.
const DRAG_THRESHOLD: f32 = 10.0;

fn render_corner_handles(
    ctx: &egui::Context,
    layout: &LayoutTree,
    leaves: &[(super::PanelId, PanelType, egui::Rect, NodeId)],
    actions: &mut Vec<LayoutAction>,
) {
    let handle_painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("corner_handles"),
    ));

    let handle_color = egui::Color32::from_rgba_premultiplied(120, 120, 120, 160);
    let handle_hover_color = egui::Color32::from_rgba_premultiplied(180, 180, 180, 200);

    for (panel_id, _panel_type, rect, node_id) in leaves {
        let node_id = *node_id;

        // Top-right corner handle
        let tr_rect = egui::Rect::from_min_size(
            egui::pos2(rect.max.x - HANDLE_SIZE, rect.min.y),
            egui::vec2(HANDLE_SIZE, HANDLE_SIZE),
        );

        // Bottom-left corner handle
        let bl_rect = egui::Rect::from_min_size(
            egui::pos2(rect.min.x, rect.max.y - HANDLE_SIZE),
            egui::vec2(HANDLE_SIZE, HANDLE_SIZE),
        );

        // Render and interact with each handle
        for (i, (handle_rect, is_top_right)) in
            [(tr_rect, true), (bl_rect, false)].iter().enumerate()
        {
            let handle_id = egui::Id::new(("corner_handle", panel_id.0, i));
            // Per-handle storage key for accumulated drag delta.
            let drag_accum_id = egui::Id::new(("corner_drag_accum", panel_id.0, i));

            // Corner handle Areas are rendered after panel Areas. Using Order::Tooltip
            // places them above Order::Foreground, ensuring they receive pointer events
            // before the panel Area when the two overlap.
            let area_response = egui::Area::new(handle_id)
                .fixed_pos(handle_rect.min)
                .order(egui::Order::Tooltip)
                .sense(egui::Sense::drag())
                .show(ctx, |ui| {
                    ui.allocate_rect(
                        egui::Rect::from_min_size(handle_rect.min, handle_rect.size()),
                        egui::Sense::drag(),
                    )
                });

            let response = area_response.inner;

            // Draw the triangle
            let color = if response.hovered() || response.dragged() {
                ctx.set_cursor_icon(egui::CursorIcon::Move);
                handle_hover_color
            } else {
                handle_color
            };

            let triangle_points = if *is_top_right {
                // Top-right triangle: points at top-right, top-left, bottom-right
                vec![
                    egui::pos2(handle_rect.max.x, handle_rect.min.y),
                    egui::pos2(handle_rect.min.x, handle_rect.min.y),
                    egui::pos2(handle_rect.max.x, handle_rect.max.y),
                ]
            } else {
                // Bottom-left triangle: points at bottom-left, top-left, bottom-right
                vec![
                    egui::pos2(handle_rect.min.x, handle_rect.max.y),
                    egui::pos2(handle_rect.min.x, handle_rect.min.y),
                    egui::pos2(handle_rect.max.x, handle_rect.max.y),
                ]
            };

            let triangle = egui::Shape::convex_polygon(triangle_points, color, egui::Stroke::NONE);
            handle_painter.add(triangle);

            // Accumulate drag delta every frame while the handle is being dragged.
            // total_drag_delta() only returns Some while dragged() is true, so we
            // must store the running total ourselves across frames and read it on
            // drag_stopped().
            if response.drag_started() {
                // Reset accumulator at the start of a new drag.
                ctx.memory_mut(|mem| {
                    mem.data.insert_temp(drag_accum_id, egui::Vec2::ZERO);
                });
            }

            if response.dragged() {
                let delta = response.drag_delta();
                ctx.memory_mut(|mem| {
                    let accum: egui::Vec2 =
                        mem.data.get_temp(drag_accum_id).unwrap_or(egui::Vec2::ZERO);
                    mem.data.insert_temp(drag_accum_id, accum + delta);
                });
            }

            if response.drag_stopped() {
                let total_delta: egui::Vec2 = ctx
                    .memory(|mem| mem.data.get_temp(drag_accum_id))
                    .unwrap_or(egui::Vec2::ZERO);

                // Clear the accumulator.
                ctx.memory_mut(|mem| {
                    mem.data.insert_temp(drag_accum_id, egui::Vec2::ZERO);
                });

                let abs_x = total_delta.x.abs();
                let abs_y = total_delta.y.abs();

                if abs_x > DRAG_THRESHOLD || abs_y > DRAG_THRESHOLD {
                    // Check if this is a merge gesture (toward sibling) or split gesture (outward)
                    if let Some(action) =
                        determine_handle_action(layout, node_id, total_delta, *is_top_right)
                    {
                        actions.push(action);
                    }
                }
            }
        }
    }
}

/// Determine whether a corner handle drag should produce a split or merge action.
fn determine_handle_action(
    layout: &LayoutTree,
    node_id: NodeId,
    drag_delta: egui::Vec2,
    _is_top_right: bool,
) -> Option<LayoutAction> {
    let abs_x = drag_delta.x.abs();
    let abs_y = drag_delta.y.abs();

    // Check if we can do a merge gesture (drag toward sibling)
    if let Some((parent_id, my_side)) = layout.find_parent_with_side(node_id)
        && let Some(super::LayoutNode::Split { direction, .. }) = layout.node(parent_id)
    {
        let direction = *direction;
        match direction {
            SplitDirection::Vertical => {
                // Horizontal drag along the split axis
                if abs_x > abs_y && abs_x > DRAG_THRESHOLD {
                    // Dragging toward sibling = merge
                    let toward_sibling = match my_side {
                        MergeSide::First => drag_delta.x > 0.0, // first panel, drag right toward second
                        MergeSide::Second => drag_delta.x < 0.0, // second panel, drag left toward first
                    };
                    if toward_sibling {
                        return Some(LayoutAction::Merge {
                            node_id: parent_id,
                            keep: my_side,
                        });
                    }
                }
            }
            SplitDirection::Horizontal => {
                // Vertical drag along the split axis
                if abs_y > abs_x && abs_y > DRAG_THRESHOLD {
                    let toward_sibling = match my_side {
                        MergeSide::First => drag_delta.y > 0.0,
                        MergeSide::Second => drag_delta.y < 0.0,
                    };
                    if toward_sibling {
                        return Some(LayoutAction::Merge {
                            node_id: parent_id,
                            keep: my_side,
                        });
                    }
                }
            }
        }
    }

    // Otherwise, it's a split gesture
    if abs_x > abs_y && abs_x > DRAG_THRESHOLD {
        Some(LayoutAction::Split {
            node_id,
            direction: SplitDirection::Vertical,
        })
    } else if abs_y > DRAG_THRESHOLD {
        Some(LayoutAction::Split {
            node_id,
            direction: SplitDirection::Horizontal,
        })
    } else {
        None
    }
}
