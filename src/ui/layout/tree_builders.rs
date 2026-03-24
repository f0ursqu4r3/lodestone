use std::collections::HashMap;

use super::tree::{
    DEFAULT_FLOAT_SIZE, DockLayout, FloatingGroup, Group, GroupId, NodeId, PanelId, PanelType,
    SplitDirection, SplitNode, TabEntry,
};

#[allow(dead_code)]
impl DockLayout {
    /// Create a layout with a single group containing one panel.
    pub fn new_single(panel_type: PanelType) -> Self {
        let group = Group::new(panel_type);
        let group_id = group.id;
        let mut groups = HashMap::new();
        groups.insert(group_id, group);

        let root_id = NodeId(0);
        let mut nodes = HashMap::new();
        nodes.insert(root_id, SplitNode::Leaf { group_id });

        Self {
            nodes,
            root: root_id,
            next_node_id: 1,
            groups,
            floating: Vec::new(),
            drag: None,
        }
    }

    /// Create a layout with a single group containing one panel, preserving IDs.
    pub fn new_with_ids(group_id: GroupId, panel_id: PanelId, panel_type: PanelType) -> Self {
        let group = Group::new_with_ids(group_id, panel_id, panel_type);
        let mut groups = HashMap::new();
        groups.insert(group_id, group);

        let root_id = NodeId(0);
        let mut nodes = HashMap::new();
        nodes.insert(root_id, SplitNode::Leaf { group_id });

        Self {
            nodes,
            root: root_id,
            next_node_id: 1,
            groups,
            floating: Vec::new(),
            drag: None,
        }
    }

    /// The default 5-panel layout: left sidebar (Sources/Scenes), center Preview, right sidebar (Properties/Audio).
    pub fn default_layout() -> Self {
        let mut layout = Self {
            nodes: HashMap::new(),
            root: NodeId(0),
            next_node_id: 0,
            groups: HashMap::new(),
            floating: Vec::new(),
            drag: None,
        };

        let mut sources_group = Group::new(PanelType::Sources);
        sources_group.add_tab(PanelType::Library);
        sources_group.active_tab = 0; // Sources tab active by default
        let sources_gid = sources_group.id;
        layout.groups.insert(sources_gid, sources_group);

        let scenes_group = Group::new(PanelType::Scenes);
        let scenes_gid = scenes_group.id;
        layout.groups.insert(scenes_gid, scenes_group);

        let preview_group = Group::new(PanelType::Preview);
        let preview_gid = preview_group.id;
        layout.groups.insert(preview_gid, preview_group);

        let properties_group = Group::new(PanelType::Properties);
        let properties_gid = properties_group.id;
        layout.groups.insert(properties_gid, properties_group);

        let audio_group = Group::new(PanelType::AudioMixer);
        let audio_gid = audio_group.id;
        layout.groups.insert(audio_gid, audio_group);

        let sources_node = layout.alloc_node_id();
        layout.nodes.insert(
            sources_node,
            SplitNode::Leaf {
                group_id: sources_gid,
            },
        );

        let scenes_node = layout.alloc_node_id();
        layout.nodes.insert(
            scenes_node,
            SplitNode::Leaf {
                group_id: scenes_gid,
            },
        );

        let preview_node = layout.alloc_node_id();
        layout.nodes.insert(
            preview_node,
            SplitNode::Leaf {
                group_id: preview_gid,
            },
        );

        let properties_node = layout.alloc_node_id();
        layout.nodes.insert(
            properties_node,
            SplitNode::Leaf {
                group_id: properties_gid,
            },
        );

        let audio_node = layout.alloc_node_id();
        layout.nodes.insert(
            audio_node,
            SplitNode::Leaf {
                group_id: audio_gid,
            },
        );

        // Left sidebar: Sources on top, Scenes on bottom
        let left_split = layout.alloc_node_id();
        layout.nodes.insert(
            left_split,
            SplitNode::Split {
                direction: SplitDirection::Horizontal,
                ratio: 0.6,
                first: sources_node,
                second: scenes_node,
            },
        );

        // Right sidebar bottom: Properties + Audio
        let right_split = layout.alloc_node_id();
        layout.nodes.insert(
            right_split,
            SplitNode::Split {
                direction: SplitDirection::Horizontal,
                ratio: 0.6,
                first: properties_node,
                second: audio_node,
            },
        );

        // Center-right: Preview on top, right sidebar on bottom
        let center_right = layout.alloc_node_id();
        layout.nodes.insert(
            center_right,
            SplitNode::Split {
                direction: SplitDirection::Vertical,
                ratio: 0.75,
                first: preview_node,
                second: right_split,
            },
        );

        // Root: left sidebar | center-right
        let root = layout.alloc_node_id();
        layout.nodes.insert(
            root,
            SplitNode::Split {
                direction: SplitDirection::Vertical,
                ratio: 0.2,
                first: left_split,
                second: center_right,
            },
        );
        layout.root = root;

        layout
    }

    /// Resize a split node's ratio (clamped to 0.1..0.9).
    pub fn resize(&mut self, node_id: NodeId, ratio: f32) {
        if let Some(SplitNode::Split { ratio: r, .. }) = self.nodes.get_mut(&node_id) {
            *r = ratio.clamp(0.1, 0.9);
        }
    }

    /// Split a group's node in the grid, creating a new group on one side.
    pub fn split_group(
        &mut self,
        target_group: GroupId,
        direction: SplitDirection,
        new_panel_type: PanelType,
        new_first: bool,
    ) -> Option<GroupId> {
        let node_id = self.find_node_for_group(target_group)?;

        let new_group = Group::new(new_panel_type);
        let new_gid = new_group.id;
        self.groups.insert(new_gid, new_group);

        let existing_child = self.alloc_node_id();
        let new_child = self.alloc_node_id();

        self.nodes.insert(
            existing_child,
            SplitNode::Leaf {
                group_id: target_group,
            },
        );
        self.nodes
            .insert(new_child, SplitNode::Leaf { group_id: new_gid });

        let (first, second) = if new_first {
            (new_child, existing_child)
        } else {
            (existing_child, new_child)
        };

        self.nodes.insert(
            node_id,
            SplitNode::Split {
                direction,
                ratio: 0.5,
                first,
                second,
            },
        );

        Some(new_gid)
    }

    /// Split a group's node by placing a specific TabEntry in the new group.
    pub fn split_group_with_tab(
        &mut self,
        target_group: GroupId,
        direction: SplitDirection,
        tab_entry: TabEntry,
        new_first: bool,
    ) -> Option<GroupId> {
        let node_id = self.find_node_for_group(target_group)?;

        let new_group =
            Group::new_with_ids(GroupId::next(), tab_entry.panel_id, tab_entry.panel_type);
        let new_gid = new_group.id;
        self.groups.insert(new_gid, new_group);

        let existing_child = self.alloc_node_id();
        let new_child = self.alloc_node_id();

        self.nodes.insert(
            existing_child,
            SplitNode::Leaf {
                group_id: target_group,
            },
        );
        self.nodes
            .insert(new_child, SplitNode::Leaf { group_id: new_gid });

        let (first, second) = if new_first {
            (new_child, existing_child)
        } else {
            (existing_child, new_child)
        };

        self.nodes.insert(
            node_id,
            SplitNode::Split {
                direction,
                ratio: 0.5,
                first,
                second,
            },
        );

        Some(new_gid)
    }

    /// Remove a group from the grid. Collapses the parent split, promoting the sibling.
    pub fn remove_group_from_grid(&mut self, group_id: GroupId) -> bool {
        let node_id = match self.find_node_for_group(group_id) {
            Some(id) => id,
            None => return false,
        };

        if node_id == self.root {
            return false;
        }

        let parent_id = match self.find_parent(node_id) {
            Some(id) => id,
            None => return false,
        };

        let sibling_id = match self.nodes.get(&parent_id) {
            Some(SplitNode::Split { first, second, .. }) => {
                if *first == node_id {
                    *second
                } else {
                    *first
                }
            }
            _ => return false,
        };

        self.nodes.remove(&node_id);

        if let Some(sibling_node) = self.nodes.remove(&sibling_id) {
            self.nodes.insert(parent_id, sibling_node);
        }

        self.groups.remove(&group_id);

        true
    }

    /// Add a panel to the root by splitting at the root level.
    pub fn insert_at_root(
        &mut self,
        panel_type: PanelType,
        panel_id: PanelId,
        direction: SplitDirection,
        ratio: f32,
    ) {
        let group = Group::new_with_ids(GroupId::next(), panel_id, panel_type);
        let new_gid = group.id;
        self.groups.insert(new_gid, group);

        let old_root = self.root;
        let old_root_new_id = self.alloc_node_id();
        if let Some(old_root_node) = self.nodes.remove(&old_root) {
            self.nodes.insert(old_root_new_id, old_root_node);
        }

        let new_leaf_id = self.alloc_node_id();
        self.nodes
            .insert(new_leaf_id, SplitNode::Leaf { group_id: new_gid });

        self.nodes.insert(
            old_root,
            SplitNode::Split {
                direction,
                ratio,
                first: old_root_new_id,
                second: new_leaf_id,
            },
        );
    }

    /// Move an existing floating group into the grid by splitting at the root.
    /// The group must already exist in `self.groups` and be in `self.floating`.
    pub fn insert_floating_into_grid(&mut self, group_id: GroupId) {
        // Remove from floating list (group stays in self.groups)
        self.remove_floating(group_id);

        // Add to the split tree at root level
        let old_root = self.root;
        let old_root_new_id = self.alloc_node_id();
        if let Some(old_root_node) = self.nodes.remove(&old_root) {
            self.nodes.insert(old_root_new_id, old_root_node);
        }

        let new_leaf_id = self.alloc_node_id();
        self.nodes.insert(new_leaf_id, SplitNode::Leaf { group_id });

        self.nodes.insert(
            old_root,
            SplitNode::Split {
                direction: SplitDirection::Vertical,
                ratio: 0.5,
                first: old_root_new_id,
                second: new_leaf_id,
            },
        );
    }

    /// Detach a grid group and make it a floating panel.
    /// Returns false if the group is the root (can't detach the last grid group).
    pub fn detach_grid_group_to_floating(&mut self, group_id: GroupId, pos: egui::Pos2) -> bool {
        // Save the group data before remove_group_from_grid deletes it
        let group_data = self.groups.get(&group_id).cloned();
        if !self.remove_group_from_grid(group_id) {
            return false;
        }
        // Restore the group data so the floating panel has its tabs
        if let Some(group) = group_data {
            self.groups.insert(group_id, group);
        }
        self.floating.push(FloatingGroup {
            group_id,
            pos,
            size: DEFAULT_FLOAT_SIZE,
        });
        true
    }

    /// Create a floating group from a tab entry.
    pub fn add_floating_group(&mut self, entry: TabEntry, pos: egui::Pos2) -> GroupId {
        let group = Group::new_with_ids(GroupId::next(), entry.panel_id, entry.panel_type);
        let gid = group.id;
        self.groups.insert(gid, group);
        self.floating.push(FloatingGroup {
            group_id: gid,
            pos,
            size: DEFAULT_FLOAT_SIZE,
        });
        gid
    }

    /// Remove a floating group entry (does NOT remove from self.groups).
    pub fn remove_floating(&mut self, group_id: GroupId) {
        self.floating.retain(|f| f.group_id != group_id);
    }

    /// Update a floating group's position and size.
    pub fn update_floating_geometry(
        &mut self,
        group_id: GroupId,
        pos: egui::Pos2,
        size: egui::Vec2,
    ) {
        if let Some(fg) = self.floating.iter_mut().find(|fg| fg.group_id == group_id) {
            fg.pos = pos;
            fg.size = size;
        }
    }

    /// Remove a tab from its source group, cleaning up empty groups.
    pub fn take_tab(&mut self, group_id: GroupId, tab_index: usize) -> Option<TabEntry> {
        let group = self.groups.get_mut(&group_id)?;
        if group.tabs.len() <= 1 {
            let entry = group.tabs[0].clone();
            if self.is_floating(group_id) {
                self.remove_floating(group_id);
                self.groups.remove(&group_id);
            } else {
                self.remove_group_from_grid(group_id);
            }
            Some(entry)
        } else {
            group.remove_tab(tab_index)
        }
    }
}
