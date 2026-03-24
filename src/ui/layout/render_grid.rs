//! Split-divider rendering for the dockview grid layout.

use super::interactions::collect_dividers;
use super::render::LayoutAction;
use super::tree::{DockLayout, SplitDirection};

use crate::ui::theme::BORDER;

/// Render split dividers with drag interaction.
pub(crate) fn render_dividers(
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
