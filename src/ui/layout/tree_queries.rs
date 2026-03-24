use super::tree::{DockLayout, GroupId, NodeId, PanelId, PanelType, SplitNode, split_rect};

#[allow(dead_code)]
impl DockLayout {
    /// Find the parent of a node in the split tree.
    pub fn find_parent(&self, target: NodeId) -> Option<NodeId> {
        for (id, node) in &self.nodes {
            if let SplitNode::Split { first, second, .. } = node
                && (*first == target || *second == target)
            {
                return Some(*id);
            }
        }
        None
    }

    /// Find the node (leaf) that contains a given GroupId.
    pub fn find_node_for_group(&self, group_id: GroupId) -> Option<NodeId> {
        for (id, node) in &self.nodes {
            if let SplitNode::Leaf { group_id: gid } = node
                && *gid == group_id
            {
                return Some(*id);
            }
        }
        None
    }

    /// Collect all grid groups with their computed screen rects.
    pub fn collect_groups_with_rects(&self, rect: egui::Rect) -> Vec<(GroupId, egui::Rect)> {
        let mut result = Vec::new();
        self.collect_groups_recursive(self.root, rect, &mut result);
        result
    }

    fn collect_groups_recursive(
        &self,
        node_id: NodeId,
        rect: egui::Rect,
        result: &mut Vec<(GroupId, egui::Rect)>,
    ) {
        match self.nodes.get(&node_id) {
            Some(SplitNode::Leaf { group_id }) => {
                result.push((*group_id, rect));
            }
            Some(SplitNode::Split {
                direction,
                ratio,
                first,
                second,
            }) => {
                let (first_rect, second_rect) = split_rect(rect, *direction, *ratio);
                self.collect_groups_recursive(*first, first_rect, result);
                self.collect_groups_recursive(*second, second_rect, result);
            }
            None => {}
        }
    }

    /// Collect all panels across all groups (grid + floating).
    pub fn collect_all_panels(&self) -> Vec<(PanelId, PanelType)> {
        let mut result = Vec::new();
        for group in self.groups.values() {
            for tab in &group.tabs {
                result.push((tab.panel_id, tab.panel_type));
            }
        }
        result
    }

    /// Check if a group is floating.
    pub fn is_floating(&self, group_id: GroupId) -> bool {
        self.floating.iter().any(|f| f.group_id == group_id)
    }
}
