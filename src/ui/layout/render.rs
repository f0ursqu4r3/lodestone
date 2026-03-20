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
    let actions = Vec::new();

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

    actions
}
