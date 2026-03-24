use crate::ui::layout::render::LayoutAction;
use crate::ui::layout::tree::{DragState, DropZone, GroupId, SplitDirection};
use crate::window::{DetachRequest, WindowState};

/// Apply a list of layout actions to the window's dock layout.
///
/// Returns any detach-to-OS-window requests produced by the actions.
pub(crate) fn apply_layout_actions(
    win: &mut WindowState,
    actions: Vec<LayoutAction>,
) -> Vec<DetachRequest> {
    let mut detach_requests = Vec::new();

    for action in actions {
        match action {
            LayoutAction::Resize { node_id, new_ratio } => {
                win.layout.resize(node_id, new_ratio);
            }
            LayoutAction::SetActiveTab {
                group_id,
                tab_index,
            } => {
                if let Some(group) = win.layout.groups.get_mut(&group_id)
                    && tab_index < group.tabs.len()
                {
                    group.active_tab = tab_index;
                }
            }
            LayoutAction::Close {
                group_id,
                tab_index,
            } => {
                win.apply_close(group_id, tab_index);
            }
            LayoutAction::CloseOthers {
                group_id,
                tab_index,
            } => {
                if let Some(group) = win.layout.groups.get_mut(&group_id)
                    && tab_index < group.tabs.len()
                {
                    let kept = group.tabs[tab_index].clone();
                    group.tabs = vec![kept];
                    group.active_tab = 0;
                }
            }
            LayoutAction::DetachToFloat {
                group_id,
                tab_index,
            } => {
                if let Some(entry) = win.layout.take_tab(group_id, tab_index) {
                    win.layout
                        .add_floating_group(entry, egui::pos2(200.0, 200.0));
                }
            }
            LayoutAction::DetachToWindow {
                group_id,
                tab_index,
            } => {
                if let Some(entry) = win.layout.take_tab(group_id, tab_index) {
                    detach_requests.push(DetachRequest {
                        panel_type: entry.panel_type,
                        panel_id: entry.panel_id,
                        group_id: GroupId::next(),
                    });
                }
            }
            LayoutAction::StartDrag {
                group_id,
                tab_index,
            } => {
                if let Some(group) = win.layout.groups.get(&group_id)
                    && let Some(tab) = group.tabs.get(tab_index)
                {
                    win.layout.drag = Some(DragState {
                        panel_id: tab.panel_id,
                        panel_type: tab.panel_type,
                        source_group: group_id,
                        tab_index,
                    });
                }
            }
            LayoutAction::DropOnZone { target_group, zone } => {
                if let Some(drag) = win.layout.drag.take()
                    && let Some(entry) = win.layout.take_tab(drag.source_group, drag.tab_index)
                {
                    // Floating groups can't be split — always add as a tab
                    let is_floating = win.layout.is_floating(target_group);
                    match zone {
                        _ if is_floating => {
                            if let Some(group) = win.layout.groups.get_mut(&target_group) {
                                group.add_tab_entry(entry);
                            }
                        }
                        DropZone::TabBar { index } => {
                            if let Some(group) = win.layout.groups.get_mut(&target_group) {
                                // When reordering within the same group, the source
                                // tab was already removed, shifting indices down.
                                // Adjust the insertion index to compensate.
                                let adjusted = if target_group == drag.source_group
                                    && drag.tab_index < index
                                {
                                    index.saturating_sub(1)
                                } else {
                                    index
                                };
                                group.insert_tab(adjusted, entry);
                            }
                        }
                        DropZone::Center => {
                            if let Some(group) = win.layout.groups.get_mut(&target_group) {
                                group.add_tab_entry(entry);
                            }
                        }
                        DropZone::Left => {
                            win.layout.split_group_with_tab(
                                target_group,
                                SplitDirection::Vertical,
                                entry,
                                true,
                            );
                        }
                        DropZone::Right => {
                            win.layout.split_group_with_tab(
                                target_group,
                                SplitDirection::Vertical,
                                entry,
                                false,
                            );
                        }
                        DropZone::Top => {
                            win.layout.split_group_with_tab(
                                target_group,
                                SplitDirection::Horizontal,
                                entry,
                                true,
                            );
                        }
                        DropZone::Bottom => {
                            win.layout.split_group_with_tab(
                                target_group,
                                SplitDirection::Horizontal,
                                entry,
                                false,
                            );
                        }
                    }
                }
            }
            LayoutAction::DropOnEmpty { pos } => {
                if let Some(drag) = win.layout.drag.take()
                    && let Some(entry) = win.layout.take_tab(drag.source_group, drag.tab_index)
                {
                    win.layout.add_floating_group(entry, pos);
                }
            }
            LayoutAction::CancelDrag => {
                win.layout.drag = None;
            }
            LayoutAction::AddPanel {
                target_group,
                panel_type,
            } => {
                if let Some(group) = win.layout.groups.get_mut(&target_group) {
                    group.add_tab(panel_type);
                }
            }
            LayoutAction::AddPanelAtRoot { panel_type } => {
                win.layout.insert_at_root(
                    panel_type,
                    crate::ui::layout::tree::PanelId::next(),
                    SplitDirection::Vertical,
                    0.8,
                );
            }
            LayoutAction::ResetLayout => {
                win.layout = crate::ui::layout::tree::DockLayout::default_layout();
            }
            LayoutAction::ReattachToMain => {
                win.reattach_pending = true;
            }
            LayoutAction::DockFloatingToGrid { group_id } => {
                win.layout.insert_floating_into_grid(group_id);
            }
            LayoutAction::CloseFloatingGroup { group_id } => {
                win.layout.remove_floating(group_id);
                win.layout.groups.remove(&group_id);
            }
            LayoutAction::DetachGroupToFloat { group_id } => {
                win.layout
                    .detach_grid_group_to_floating(group_id, egui::pos2(200.0, 200.0));
            }
            LayoutAction::MoveGroupToTarget {
                source_group,
                target_group,
                zone,
            } => {
                // Skip self-drop
                if source_group == target_group {
                    continue;
                }

                // Take all tabs from the source group
                let source_tabs = win
                    .layout
                    .groups
                    .get(&source_group)
                    .map(|g| g.tabs.clone())
                    .unwrap_or_default();

                if source_tabs.is_empty() {
                    continue;
                }

                // Remove the source group from wherever it is
                let was_floating = win.layout.is_floating(source_group);
                if was_floating {
                    win.layout.remove_floating(source_group);
                    win.layout.groups.remove(&source_group);
                } else {
                    win.layout.remove_group_from_grid(source_group);
                }

                // Add all tabs to the target based on drop zone
                match zone {
                    DropZone::Center | DropZone::TabBar { .. } => {
                        if let Some(group) = win.layout.groups.get_mut(&target_group) {
                            for tab in source_tabs {
                                group.add_tab_entry(tab);
                            }
                        }
                    }
                    _ => {
                        if let Some(first_tab) = source_tabs.first() {
                            let direction = match zone {
                                DropZone::Left | DropZone::Right => SplitDirection::Vertical,
                                _ => SplitDirection::Horizontal,
                            };
                            let before = matches!(zone, DropZone::Left | DropZone::Top);
                            if let Some(new_gid) = win.layout.split_group_with_tab(
                                target_group,
                                direction,
                                first_tab.clone(),
                                before,
                            ) && let Some(group) = win.layout.groups.get_mut(&new_gid)
                            {
                                for tab in &source_tabs[1..] {
                                    group.add_tab_entry(tab.clone());
                                }
                            }
                        }
                    }
                }
            }
            LayoutAction::UpdateFloatingGeometry {
                group_id,
                pos,
                size,
            } => {
                win.layout.update_floating_geometry(group_id, pos, size);
            }
        }
    }

    detach_requests
}
