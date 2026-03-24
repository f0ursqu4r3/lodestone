use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

/// Default size for newly created floating panels.
pub(crate) const DEFAULT_FLOAT_SIZE: egui::Vec2 = egui::vec2(400.0, 300.0);

// ---------------------------------------------------------------------------
// PanelType (unchanged)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub enum PanelType {
    Preview,
    SceneEditor, // kept for backward compat with saved layouts
    AudioMixer,
    StreamControls,
    Sources,
    Scenes,
    Properties,
    Library,
}

impl PanelType {
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Preview => "Preview",
            Self::SceneEditor => "Sources", // Legacy compat — renders as Sources panel
            Self::AudioMixer => "Audio",
            Self::StreamControls => "Stream Controls",
            Self::Sources => "Sources",
            Self::Scenes => "Scenes",
            Self::Properties => "Properties",
            Self::Library => "Library",
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
    pub(crate) nodes: HashMap<NodeId, SplitNode>,
    pub(crate) root: NodeId,
    pub(crate) next_node_id: u64,
    // Groups
    pub groups: HashMap<GroupId, Group>,
    // Floating groups (above the grid)
    pub floating: Vec<FloatingGroup>,
    // Drag-and-drop state
    pub drag: Option<DragState>,
}

#[allow(dead_code)]
impl DockLayout {
    pub(crate) fn alloc_node_id(&mut self) -> NodeId {
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
    fn default_layout_has_5_groups_6_panels() {
        let layout = DockLayout::default_layout();
        assert_eq!(layout.groups.len(), 5);
        let all_panels = layout.collect_all_panels();
        assert_eq!(all_panels.len(), 6);
        let types: Vec<PanelType> = all_panels.iter().map(|(_, t)| *t).collect();
        assert!(types.contains(&PanelType::Sources));
        assert!(types.contains(&PanelType::Library));
        assert!(types.contains(&PanelType::Scenes));
        assert!(types.contains(&PanelType::Preview));
        assert!(types.contains(&PanelType::Properties));
        assert!(types.contains(&PanelType::AudioMixer));
    }

    #[test]
    fn default_layout_group_rects() {
        let layout = DockLayout::default_layout();
        let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1000.0, 600.0));
        let groups = layout.collect_groups_with_rects(rect);
        assert_eq!(groups.len(), 5);
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
