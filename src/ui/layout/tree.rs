use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

/// Default size for newly created floating panels.
const DEFAULT_FLOAT_SIZE: egui::Vec2 = egui::vec2(400.0, 300.0);

// ---------------------------------------------------------------------------
// PanelType (unchanged)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum PanelType {
    Preview,
    SceneEditor,
    AudioMixer,
    StreamControls,
}

impl PanelType {
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Preview => "Preview",
            Self::SceneEditor => "Scene Editor",
            Self::AudioMixer => "Audio Mixer",
            Self::StreamControls => "Stream Controls",
        }
    }
}

// ---------------------------------------------------------------------------
// PanelId (unchanged)
// ---------------------------------------------------------------------------

static PANEL_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub struct PanelId(pub u64);

impl PanelId {
    pub fn next() -> Self {
        Self(PANEL_ID_COUNTER.fetch_add(1, Ordering::Relaxed))
    }

    #[allow(dead_code)]
    pub fn set_counter(min_next: u64) {
        let mut current = PANEL_ID_COUNTER.load(Ordering::Relaxed);
        loop {
            if current >= min_next {
                break;
            }
            match PANEL_ID_COUNTER.compare_exchange(
                current,
                min_next,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(actual) => current = actual,
            }
        }
    }
}

// ---------------------------------------------------------------------------
// SplitDirection (unchanged)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum SplitDirection {
    Horizontal,
    Vertical,
}

// ---------------------------------------------------------------------------
// GroupId
// ---------------------------------------------------------------------------

static GROUP_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Unique identifier for a tab group.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub struct GroupId(pub u64);

impl GroupId {
    pub fn next() -> Self {
        Self(GROUP_ID_COUNTER.fetch_add(1, Ordering::Relaxed))
    }

    #[allow(dead_code)]
    pub fn set_counter(min_next: u64) {
        let mut current = GROUP_ID_COUNTER.load(Ordering::Relaxed);
        loop {
            if current >= min_next {
                break;
            }
            match GROUP_ID_COUNTER.compare_exchange(
                current,
                min_next,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(actual) => current = actual,
            }
        }
    }
}

// ---------------------------------------------------------------------------
// TabEntry
// ---------------------------------------------------------------------------

/// A single tab within a group, referencing a panel by ID and type.
#[derive(Clone, Debug)]
pub struct TabEntry {
    pub panel_id: PanelId,
    pub panel_type: PanelType,
}

// ---------------------------------------------------------------------------
// Group
// ---------------------------------------------------------------------------

/// A container holding one or more panels as tabs.
#[derive(Clone, Debug)]
pub struct Group {
    pub id: GroupId,
    pub tabs: Vec<TabEntry>,
    pub active_tab: usize,
}

impl Group {
    /// Create a new group with a single tab.
    pub fn new(panel_type: PanelType) -> Self {
        Self {
            id: GroupId::next(),
            tabs: vec![TabEntry {
                panel_id: PanelId::next(),
                panel_type,
            }],
            active_tab: 0,
        }
    }

    /// Create a new group with a single tab, preserving existing IDs.
    pub fn new_with_ids(group_id: GroupId, panel_id: PanelId, panel_type: PanelType) -> Self {
        Self {
            id: group_id,
            tabs: vec![TabEntry {
                panel_id,
                panel_type,
            }],
            active_tab: 0,
        }
    }

    /// Add a tab and make it active. Returns the new tab's PanelId.
    pub fn add_tab(&mut self, panel_type: PanelType) -> PanelId {
        let panel_id = PanelId::next();
        self.tabs.push(TabEntry {
            panel_id,
            panel_type,
        });
        self.active_tab = self.tabs.len() - 1;
        panel_id
    }

    /// Add a tab with a specific PanelId (for moves between groups).
    #[allow(dead_code)]
    pub fn add_tab_entry(&mut self, entry: TabEntry) {
        self.tabs.push(entry);
        self.active_tab = self.tabs.len() - 1;
    }

    /// Insert a tab at a specific index.
    #[allow(dead_code)]
    pub fn insert_tab(&mut self, index: usize, entry: TabEntry) {
        let index = index.min(self.tabs.len());
        self.tabs.insert(index, entry);
        self.active_tab = index;
    }

    /// Remove a tab by index. Returns None if it's the last tab.
    #[allow(dead_code)]
    pub fn remove_tab(&mut self, index: usize) -> Option<TabEntry> {
        if self.tabs.len() <= 1 || index >= self.tabs.len() {
            return None;
        }
        let entry = self.tabs.remove(index);
        if self.active_tab >= self.tabs.len() {
            self.active_tab = self.tabs.len() - 1;
        }
        Some(entry)
    }

    /// Get the active tab entry. Falls back to first tab if index is out of bounds.
    pub fn active_tab_entry(&self) -> &TabEntry {
        self.tabs.get(self.active_tab).unwrap_or(&self.tabs[0])
    }
}

// ---------------------------------------------------------------------------
// SplitNode — binary split tree with GroupId leaves
// ---------------------------------------------------------------------------

/// Unique identifier for a node in the split tree.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct NodeId(pub u64);

/// A node in the binary split tree. Leaves reference a GroupId; splits divide space.
#[derive(Clone, Debug)]
pub enum SplitNode {
    Leaf {
        group_id: GroupId,
    },
    Split {
        direction: SplitDirection,
        ratio: f32,
        first: NodeId,
        second: NodeId,
    },
}

// ---------------------------------------------------------------------------
// FloatingGroup
// ---------------------------------------------------------------------------

/// A group rendered as a floating window above the grid layout.
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct FloatingGroup {
    pub group_id: GroupId,
    pub pos: egui::Pos2,
    pub size: egui::Vec2,
}

// ---------------------------------------------------------------------------
// DropZone
// ---------------------------------------------------------------------------

/// Where a dragged tab can be dropped relative to a target group.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[allow(dead_code)]
pub enum DropZone {
    Left,
    Right,
    Top,
    Bottom,
    Center,
    TabBar { index: usize },
}

// ---------------------------------------------------------------------------
// DragState
// ---------------------------------------------------------------------------

/// Active drag-and-drop state tracking the tab being dragged.
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct DragState {
    pub panel_id: PanelId,
    pub panel_type: PanelType,
    pub source_group: GroupId,
    pub tab_index: usize,
}

// ---------------------------------------------------------------------------
// DockLayout
// ---------------------------------------------------------------------------

/// Top-level layout state per window. Contains the split tree, groups, floating groups, and drag state.
#[allow(dead_code)]
pub struct DockLayout {
    // Split tree
    nodes: HashMap<NodeId, SplitNode>,
    root: NodeId,
    next_node_id: u64,
    // Groups
    pub groups: HashMap<GroupId, Group>,
    // Floating groups (above the grid)
    pub floating: Vec<FloatingGroup>,
    // Drag-and-drop state
    pub drag: Option<DragState>,
}

#[allow(dead_code)]
impl DockLayout {
    fn alloc_node_id(&mut self) -> NodeId {
        let id = NodeId(self.next_node_id);
        self.next_node_id += 1;
        id
    }

    pub fn root_id(&self) -> NodeId {
        self.root
    }

    pub fn node(&self, id: NodeId) -> Option<&SplitNode> {
        self.nodes.get(&id)
    }

    pub fn nodes(&self) -> &HashMap<NodeId, SplitNode> {
        &self.nodes
    }

    /// Build from deserialized parts.
    pub fn from_parts(
        nodes: HashMap<NodeId, SplitNode>,
        root: NodeId,
        next_node_id: u64,
        groups: HashMap<GroupId, Group>,
        floating: Vec<FloatingGroup>,
    ) -> Self {
        Self {
            nodes,
            root,
            next_node_id,
            groups,
            floating,
            drag: None,
        }
    }

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

    /// The default 4-panel layout per the spec.
    pub fn default_layout() -> Self {
        let mut layout = Self {
            nodes: HashMap::new(),
            root: NodeId(0),
            next_node_id: 0,
            groups: HashMap::new(),
            floating: Vec::new(),
            drag: None,
        };

        let scene_group = Group::new(PanelType::SceneEditor);
        let scene_gid = scene_group.id;
        layout.groups.insert(scene_gid, scene_group);

        let preview_group = Group::new(PanelType::Preview);
        let preview_gid = preview_group.id;
        layout.groups.insert(preview_gid, preview_group);

        let mut right_group = Group::new(PanelType::AudioMixer);
        let right_gid = right_group.id;
        right_group.add_tab(PanelType::StreamControls);
        right_group.active_tab = 0;
        layout.groups.insert(right_gid, right_group);

        let scene_node = layout.alloc_node_id();
        layout.nodes.insert(
            scene_node,
            SplitNode::Leaf {
                group_id: scene_gid,
            },
        );

        let preview_node = layout.alloc_node_id();
        layout.nodes.insert(
            preview_node,
            SplitNode::Leaf {
                group_id: preview_gid,
            },
        );

        let right_node = layout.alloc_node_id();
        layout.nodes.insert(
            right_node,
            SplitNode::Leaf {
                group_id: right_gid,
            },
        );

        let right_split = layout.alloc_node_id();
        layout.nodes.insert(
            right_split,
            SplitNode::Split {
                direction: SplitDirection::Horizontal,
                ratio: 0.75,
                first: preview_node,
                second: right_node,
            },
        );

        let root = layout.alloc_node_id();
        layout.nodes.insert(
            root,
            SplitNode::Split {
                direction: SplitDirection::Vertical,
                ratio: 0.2,
                first: scene_node,
                second: right_split,
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
    pub fn update_floating_geometry(&mut self, group_id: GroupId, pos: egui::Pos2, size: egui::Vec2) {
        if let Some(fg) = self.floating.iter_mut().find(|fg| fg.group_id == group_id) {
            fg.pos = pos;
            fg.size = size;
        }
    }

    /// Check if a group is floating.
    pub fn is_floating(&self, group_id: GroupId) -> bool {
        self.floating.iter().any(|f| f.group_id == group_id)
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

/// Split a rectangle according to direction and ratio.
pub fn split_rect(
    rect: egui::Rect,
    direction: SplitDirection,
    ratio: f32,
) -> (egui::Rect, egui::Rect) {
    match direction {
        SplitDirection::Vertical => {
            let split_x = rect.min.x + rect.width() * ratio;
            let first = egui::Rect::from_min_max(rect.min, egui::pos2(split_x, rect.max.y));
            let second = egui::Rect::from_min_max(egui::pos2(split_x, rect.min.y), rect.max);
            (first, second)
        }
        SplitDirection::Horizontal => {
            let split_y = rect.min.y + rect.height() * ratio;
            let first = egui::Rect::from_min_max(rect.min, egui::pos2(rect.max.x, split_y));
            let second = egui::Rect::from_min_max(egui::pos2(rect.min.x, split_y), rect.max);
            (first, second)
        }
    }
}

// Tests at the bottom
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn group_id_increments() {
        let a = GroupId::next();
        let b = GroupId::next();
        assert_ne!(a, b);
        assert!(b.0 > a.0);
    }

    #[test]
    fn group_active_tab_default() {
        let group = Group::new(PanelType::Preview);
        assert_eq!(group.tabs.len(), 1);
        assert_eq!(group.active_tab, 0);
        assert_eq!(group.tabs[0].panel_type, PanelType::Preview);
    }

    #[test]
    fn group_add_tab() {
        let mut group = Group::new(PanelType::Preview);
        group.add_tab(PanelType::AudioMixer);
        assert_eq!(group.tabs.len(), 2);
        assert_eq!(group.active_tab, 1);
    }

    #[test]
    fn group_remove_tab() {
        let mut group = Group::new(PanelType::Preview);
        group.add_tab(PanelType::AudioMixer);
        group.add_tab(PanelType::StreamControls);
        group.active_tab = 1;
        let removed = group.remove_tab(1);
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().panel_type, PanelType::AudioMixer);
        assert_eq!(group.tabs.len(), 2);
        assert!(group.active_tab <= group.tabs.len().saturating_sub(1));
    }

    #[test]
    fn group_remove_last_tab_returns_none() {
        let mut group = Group::new(PanelType::Preview);
        assert!(group.remove_tab(0).is_none());
    }

    #[test]
    fn default_layout_has_3_groups_4_panels() {
        let layout = DockLayout::default_layout();
        assert_eq!(layout.groups.len(), 3);
        let all_panels = layout.collect_all_panels();
        assert_eq!(all_panels.len(), 4);
        let types: Vec<PanelType> = all_panels.iter().map(|(_, t)| *t).collect();
        assert!(types.contains(&PanelType::SceneEditor));
        assert!(types.contains(&PanelType::Preview));
        assert!(types.contains(&PanelType::AudioMixer));
        assert!(types.contains(&PanelType::StreamControls));
    }

    #[test]
    fn default_layout_group_rects() {
        let layout = DockLayout::default_layout();
        let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1000.0, 600.0));
        let groups = layout.collect_groups_with_rects(rect);
        assert_eq!(groups.len(), 3);
    }

    #[test]
    fn split_group_creates_new_group() {
        let mut layout = DockLayout::new_single(PanelType::Preview);
        let gid = layout.groups.keys().next().copied().unwrap();
        let new_gid =
            layout.split_group(gid, SplitDirection::Vertical, PanelType::AudioMixer, false);
        assert!(new_gid.is_some());
        assert_eq!(layout.groups.len(), 2);
        let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1000.0, 600.0));
        assert_eq!(layout.collect_groups_with_rects(rect).len(), 2);
    }

    #[test]
    fn remove_group_collapses_parent() {
        let mut layout = DockLayout::new_single(PanelType::Preview);
        let original_gid = layout.groups.keys().next().copied().unwrap();
        let new_gid = layout
            .split_group(
                original_gid,
                SplitDirection::Vertical,
                PanelType::AudioMixer,
                false,
            )
            .unwrap();
        assert!(layout.remove_group_from_grid(new_gid));
        assert_eq!(layout.groups.len(), 1);
        assert!(matches!(
            layout.node(layout.root_id()),
            Some(SplitNode::Leaf { .. })
        ));
    }

    #[test]
    fn cannot_remove_root_leaf_group() {
        let mut layout = DockLayout::new_single(PanelType::Preview);
        let gid = layout.groups.keys().next().copied().unwrap();
        assert!(!layout.remove_group_from_grid(gid));
    }

    #[test]
    fn update_floating_geometry() {
        let mut layout = DockLayout::default_layout();
        let entry = TabEntry {
            panel_id: PanelId::next(),
            panel_type: PanelType::AudioMixer,
        };
        let gid = layout.add_floating_group(entry, egui::pos2(100.0, 100.0));
        layout.update_floating_geometry(gid, egui::pos2(200.0, 300.0), egui::vec2(500.0, 400.0));
        let fg = layout.floating.iter().find(|f| f.group_id == gid).unwrap();
        assert_eq!(fg.pos, egui::pos2(200.0, 300.0));
        assert_eq!(fg.size, egui::vec2(500.0, 400.0));
    }

    #[test]
    fn floating_group_lifecycle() {
        let mut layout = DockLayout::new_single(PanelType::Preview);
        let entry = TabEntry {
            panel_id: PanelId::next(),
            panel_type: PanelType::AudioMixer,
        };
        let fgid = layout.add_floating_group(entry, egui::pos2(100.0, 100.0));
        assert!(layout.is_floating(fgid));
        assert_eq!(layout.floating.len(), 1);
        layout.remove_floating(fgid);
        assert!(!layout.is_floating(fgid));
        assert_eq!(layout.floating.len(), 0);
    }

    #[test]
    fn take_tab_from_multi_tab_group() {
        let mut layout = DockLayout::new_single(PanelType::Preview);
        let gid = layout.groups.keys().next().copied().unwrap();
        layout
            .groups
            .get_mut(&gid)
            .unwrap()
            .add_tab(PanelType::AudioMixer);
        assert_eq!(layout.groups[&gid].tabs.len(), 2);
        let taken = layout.take_tab(gid, 1);
        assert!(taken.is_some());
        assert_eq!(taken.unwrap().panel_type, PanelType::AudioMixer);
        assert_eq!(layout.groups[&gid].tabs.len(), 1);
    }

    #[test]
    fn take_last_tab_removes_floating_group() {
        let mut layout = DockLayout::new_single(PanelType::Preview);
        let entry = TabEntry {
            panel_id: PanelId::next(),
            panel_type: PanelType::AudioMixer,
        };
        let fgid = layout.add_floating_group(entry, egui::pos2(100.0, 100.0));
        assert_eq!(layout.groups.len(), 2);
        let taken = layout.take_tab(fgid, 0);
        assert!(taken.is_some());
        assert_eq!(layout.groups.len(), 1);
        assert_eq!(layout.floating.len(), 0);
    }

    #[test]
    fn take_last_tab_removes_grid_group() {
        let mut layout = DockLayout::new_single(PanelType::Preview);
        let original_gid = layout.groups.keys().next().copied().unwrap();
        let new_gid = layout
            .split_group(
                original_gid,
                SplitDirection::Vertical,
                PanelType::AudioMixer,
                false,
            )
            .unwrap();
        assert_eq!(layout.groups.len(), 2);
        let taken = layout.take_tab(new_gid, 0);
        assert!(taken.is_some());
        assert_eq!(layout.groups.len(), 1);
        assert!(matches!(
            layout.node(layout.root_id()),
            Some(SplitNode::Leaf { .. })
        ));
    }

    #[test]
    fn insert_at_root_adds_group() {
        let mut layout = DockLayout::new_single(PanelType::Preview);
        assert_eq!(layout.groups.len(), 1);
        layout.insert_at_root(
            PanelType::AudioMixer,
            PanelId::next(),
            SplitDirection::Vertical,
            0.5,
        );
        assert_eq!(layout.groups.len(), 2);
        let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1000.0, 600.0));
        assert_eq!(layout.collect_groups_with_rects(rect).len(), 2);
    }
}
