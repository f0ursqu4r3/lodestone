use super::interactions::collect_dividers;
use super::{LayoutTree, MergeSide, NodeId, PanelType, SplitDirection};

pub enum LayoutAction {
    Resize { node_id: NodeId, new_ratio: f32 },
    SwapType { node_id: NodeId, new_type: PanelType },
    Close { node_id: NodeId },
    Detach { node_id: NodeId },
    Duplicate { node_id: NodeId },
    Split { node_id: NodeId, direction: SplitDirection },
    Merge { node_id: NodeId, keep: MergeSide },
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

    // Draw each panel in its allocated rect
    for (panel_id, panel_type, rect) in leaves {
        // Use egui::Area to position content at the exact rect
        egui::Area::new(egui::Id::new(("panel", panel_id.0)))
            .fixed_pos(rect.min)
            .show(ctx, |ui| {
                ui.set_min_size(rect.size());
                ui.set_max_size(rect.size());

                // Panel header (minimal for now — expanded in Task 7)
                ui.horizontal(|ui| {
                    ui.label(panel_type.display_name());
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

        // Drag interaction: use an Area to place a drag-sensitive rect at the divider.
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

        // Set cursor on hover
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

        // Handle drag
        if response.dragged() {
            if let Some(pointer_pos) = ctx.pointer_interact_pos() {
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
    }

    actions
}
