// Stub — will be replaced in a later task with full dockview rendering.

use super::tree::{DockLayout, GroupId, NodeId, PanelType, SplitDirection};

/// Actions the layout renderer can request.
#[allow(dead_code)]
pub enum LayoutAction {
    Resize {
        node_id: NodeId,
        new_ratio: f32,
    },
    Close {
        group_id: GroupId,
    },
    ResetLayout,
    SplitWithType {
        group_id: GroupId,
        direction: SplitDirection,
        new_type: PanelType,
    },
}

/// All dockable panel types for the type selector dropdown.
#[allow(dead_code)]
pub const DOCKABLE_TYPES: &[PanelType] = &[
    PanelType::Preview,
    PanelType::SceneEditor,
    PanelType::AudioMixer,
    PanelType::StreamControls,
];

/// Render the top menu bar. Returns layout actions and the remaining rect below the bar.
pub fn render_menu_bar(
    ctx: &egui::Context,
    _layout: &DockLayout,
) -> (Vec<LayoutAction>, egui::Rect) {
    egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("View", |ui| {
                if ui.button("Reset Layout").clicked() {
                    ui.close();
                }
            });
        });
    });
    (Vec::new(), ctx.available_rect())
}

/// Render the layout panels. Returns layout actions.
pub fn render_layout(
    ctx: &egui::Context,
    layout: &DockLayout,
    state: &mut crate::state::AppState,
    available_rect: egui::Rect,
) -> Vec<LayoutAction> {
    let groups = layout.collect_groups_with_rects(available_rect);

    for (group_id, rect) in &groups {
        if let Some(group) = layout.groups.get(group_id) {
            let tab = group.active_tab_entry();
            let panel_id = tab.panel_id;
            let panel_type = tab.panel_type;

            egui::Area::new(egui::Id::new(("panel", panel_id.0)))
                .fixed_pos(rect.min)
                .sense(egui::Sense::hover())
                .show(ctx, |ui| {
                    ui.set_min_size(rect.size());
                    ui.set_max_size(rect.size());
                    crate::ui::draw_panel(panel_type, ui, state, panel_id);
                });
        }
    }

    Vec::new()
}
