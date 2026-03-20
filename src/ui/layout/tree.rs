use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// PanelType
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum PanelType {
    Preview,
    SceneEditor,
    AudioMixer,
    StreamControls,
    Settings,
}

// ---------------------------------------------------------------------------
// PanelId
// ---------------------------------------------------------------------------

static PANEL_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub struct PanelId(pub u64);

impl PanelId {
    pub fn next() -> Self {
        Self(PANEL_ID_COUNTER.fetch_add(1, Ordering::Relaxed))
    }

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
// SplitDirection
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum SplitDirection {
    Horizontal,
    Vertical,
}

// ---------------------------------------------------------------------------
// MergeSide
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MergeSide {
    First,
    Second,
}

// ---------------------------------------------------------------------------
// NodeId
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct NodeId(pub u64);

// ---------------------------------------------------------------------------
// LayoutNode
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub enum LayoutNode {
    Leaf {
        panel_type: PanelType,
        panel_id: PanelId,
    },
    Split {
        direction: SplitDirection,
        ratio: f32,
        first: NodeId,
        second: NodeId,
    },
}

impl PanelType {
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Preview => "Preview",
            Self::SceneEditor => "Scene Editor",
            Self::AudioMixer => "Audio Mixer",
            Self::StreamControls => "Stream Controls",
            Self::Settings => "Settings",
        }
    }

    /// Whether this panel type can be placed in the tiling layout.
    /// Settings is rendered in a dedicated window, not as a docked panel.
    pub fn is_dockable(&self) -> bool {
        !matches!(self, Self::Settings)
    }
}

impl LayoutNode {
    pub fn leaf(panel_type: PanelType) -> Self {
        LayoutNode::Leaf {
            panel_type,
            panel_id: PanelId::next(),
        }
    }

    pub fn is_leaf(&self) -> bool {
        matches!(self, LayoutNode::Leaf { .. })
    }

    pub fn panel_type(&self) -> Option<PanelType> {
        match self {
            LayoutNode::Leaf { panel_type, .. } => Some(*panel_type),
            _ => None,
        }
    }

    pub fn ratio(&self) -> Option<f32> {
        match self {
            LayoutNode::Split { ratio, .. } => Some(*ratio),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// LayoutTree
// ---------------------------------------------------------------------------

pub struct LayoutTree {
    nodes: HashMap<NodeId, LayoutNode>,
    root: NodeId,
    next_node_id: u64,
}

impl LayoutTree {
    /// Construct a tree directly from its parts (used by deserialization).
    pub fn from_parts(nodes: HashMap<NodeId, LayoutNode>, root: NodeId, next_node_id: u64) -> Self {
        Self { nodes, root, next_node_id }
    }

    pub fn new(panel_type: PanelType) -> Self {
        let root_id = NodeId(0);
        let mut nodes = HashMap::new();
        nodes.insert(root_id, LayoutNode::leaf(panel_type));
        Self {
            nodes,
            root: root_id,
            next_node_id: 1,
        }
    }

    /// Create a single-leaf tree preserving an existing PanelId.
    pub fn new_with_id(panel_type: PanelType, panel_id: PanelId) -> Self {
        let root_id = NodeId(0);
        let mut nodes = HashMap::new();
        nodes.insert(root_id, LayoutNode::Leaf { panel_type, panel_id });
        Self {
            nodes,
            root: root_id,
            next_node_id: 1,
        }
    }

    fn alloc_node_id(&mut self) -> NodeId {
        let id = NodeId(self.next_node_id);
        self.next_node_id += 1;
        id
    }

    pub fn root_id(&self) -> NodeId {
        self.root
    }

    pub fn node(&self, id: NodeId) -> Option<&LayoutNode> {
        self.nodes.get(&id)
    }

    pub fn split(&mut self, node_id: NodeId, direction: SplitDirection, ratio: f32) {
        let existing = match self.nodes.get(&node_id) {
            Some(LayoutNode::Leaf { panel_type, panel_id }) => (*panel_type, *panel_id),
            _ => return,
        };

        let first_id = self.alloc_node_id();
        let second_id = self.alloc_node_id();

        // Original leaf keeps its PanelId, new leaf gets a fresh one.
        self.nodes.insert(
            first_id,
            LayoutNode::Leaf {
                panel_type: existing.0,
                panel_id: existing.1,
            },
        );
        self.nodes.insert(second_id, LayoutNode::leaf(existing.0));

        self.nodes.insert(
            node_id,
            LayoutNode::Split {
                direction,
                ratio,
                first: first_id,
                second: second_id,
            },
        );
    }

    pub fn merge(&mut self, node_id: NodeId, side: MergeSide) {
        let (keep_id, remove_id) = match self.nodes.get(&node_id) {
            Some(LayoutNode::Split { first, second, .. }) => match side {
                MergeSide::First => (*first, *second),
                MergeSide::Second => (*second, *first),
            },
            _ => return,
        };

        // Remove the discarded subtree.
        self.remove_subtree(remove_id);

        // Replace the split node with the kept child's content.
        if let Some(kept_node) = self.nodes.remove(&keep_id) {
            self.nodes.insert(node_id, kept_node);

            // Update children that pointed to keep_id — they now live under node_id.
            // We need to fix nothing because children reference their own children by NodeId,
            // and those NodeIds haven't changed. The kept subtree is intact.
        }
    }

    fn remove_subtree(&mut self, node_id: NodeId) {
        if let Some(node) = self.nodes.remove(&node_id) {
            if let LayoutNode::Split { first, second, .. } = node {
                self.remove_subtree(first);
                self.remove_subtree(second);
            }
        }
    }

    pub fn resize(&mut self, node_id: NodeId, ratio: f32) {
        if let Some(LayoutNode::Split {
            ratio: r,
            ..
        }) = self.nodes.get_mut(&node_id)
        {
            *r = ratio.clamp(0.1, 0.9);
        }
    }

    pub fn swap_type(&mut self, node_id: NodeId, new_type: PanelType) {
        if let Some(LayoutNode::Leaf { panel_type, .. }) = self.nodes.get_mut(&node_id) {
            *panel_type = new_type;
        }
    }

    pub fn collect_leaves(&self) -> Vec<(PanelId, PanelType, NodeId)> {
        let mut result = Vec::new();
        self.collect_leaves_recursive(self.root, &mut result);
        result
    }

    fn collect_leaves_recursive(
        &self,
        node_id: NodeId,
        result: &mut Vec<(PanelId, PanelType, NodeId)>,
    ) {
        match self.nodes.get(&node_id) {
            Some(LayoutNode::Leaf {
                panel_type,
                panel_id,
            }) => {
                result.push((*panel_id, *panel_type, node_id));
            }
            Some(LayoutNode::Split { first, second, .. }) => {
                self.collect_leaves_recursive(*first, result);
                self.collect_leaves_recursive(*second, result);
            }
            None => {}
        }
    }

    pub fn collect_leaves_with_rects(
        &self,
        rect: egui::Rect,
    ) -> Vec<(PanelId, PanelType, egui::Rect, NodeId)> {
        let mut result = Vec::new();
        self.collect_rects_recursive(self.root, rect, &mut result);
        result
    }

    fn collect_rects_recursive(
        &self,
        node_id: NodeId,
        rect: egui::Rect,
        result: &mut Vec<(PanelId, PanelType, egui::Rect, NodeId)>,
    ) {
        match self.nodes.get(&node_id) {
            Some(LayoutNode::Leaf {
                panel_type,
                panel_id,
            }) => {
                result.push((*panel_id, *panel_type, rect, node_id));
            }
            Some(LayoutNode::Split {
                direction,
                ratio,
                first,
                second,
            }) => {
                let (first_rect, second_rect) = split_rect(rect, *direction, *ratio);
                self.collect_rects_recursive(*first, first_rect, result);
                self.collect_rects_recursive(*second, second_rect, result);
            }
            None => {}
        }
    }

    pub fn default_layout() -> Self {
        let mut tree = Self {
            nodes: HashMap::new(),
            root: NodeId(0),
            next_node_id: 0,
        };

        // Build bottom-up:
        // Leaves
        let scene_editor_id = tree.alloc_node_id();
        tree.nodes
            .insert(scene_editor_id, LayoutNode::leaf(PanelType::SceneEditor));

        let preview_id = tree.alloc_node_id();
        tree.nodes
            .insert(preview_id, LayoutNode::leaf(PanelType::Preview));

        let audio_mixer_id = tree.alloc_node_id();
        tree.nodes
            .insert(audio_mixer_id, LayoutNode::leaf(PanelType::AudioMixer));

        let stream_controls_id = tree.alloc_node_id();
        tree.nodes.insert(
            stream_controls_id,
            LayoutNode::leaf(PanelType::StreamControls),
        );

        // Split(Vertical, 0.5) -> AudioMixer, StreamControls
        let bottom_right_split_id = tree.alloc_node_id();
        tree.nodes.insert(
            bottom_right_split_id,
            LayoutNode::Split {
                direction: SplitDirection::Vertical,
                ratio: 0.5,
                first: audio_mixer_id,
                second: stream_controls_id,
            },
        );

        // Split(Horizontal, 0.75) -> Preview, bottom_right_split
        let right_split_id = tree.alloc_node_id();
        tree.nodes.insert(
            right_split_id,
            LayoutNode::Split {
                direction: SplitDirection::Horizontal,
                ratio: 0.75,
                first: preview_id,
                second: bottom_right_split_id,
            },
        );

        // Root: Split(Vertical, 0.2) -> SceneEditor, right_split
        let root_id = tree.alloc_node_id();
        tree.nodes.insert(
            root_id,
            LayoutNode::Split {
                direction: SplitDirection::Vertical,
                ratio: 0.2,
                first: scene_editor_id,
                second: right_split_id,
            },
        );

        tree.root = root_id;
        tree
    }

    pub fn remove_leaf(&mut self, node_id: NodeId) -> Option<(PanelType, PanelId)> {
        // Can't remove the last leaf (root is a leaf).
        if node_id == self.root {
            if self.nodes.get(&node_id)?.is_leaf() {
                return None;
            }
        }

        let leaf = match self.nodes.get(&node_id) {
            Some(LayoutNode::Leaf {
                panel_type,
                panel_id,
            }) => (*panel_type, *panel_id),
            _ => return None,
        };

        // Find the parent of this node.
        let parent_id = self.find_parent(node_id)?;

        // Get the sibling.
        let sibling_id = match self.nodes.get(&parent_id) {
            Some(LayoutNode::Split { first, second, .. }) => {
                if *first == node_id {
                    *second
                } else {
                    *first
                }
            }
            _ => return None,
        };

        // Remove the leaf node.
        self.nodes.remove(&node_id);

        // Replace parent with sibling's content.
        if let Some(sibling_node) = self.nodes.remove(&sibling_id) {
            self.nodes.insert(parent_id, sibling_node);
        }

        Some(leaf)
    }

    pub fn find_parent(&self, target: NodeId) -> Option<NodeId> {
        for (id, node) in &self.nodes {
            if let LayoutNode::Split { first, second, .. } = node {
                if *first == target || *second == target {
                    return Some(*id);
                }
            }
        }
        None
    }

    /// Returns the parent split's NodeId and which side (First/Second) this node is on.
    pub fn find_parent_with_side(&self, target: NodeId) -> Option<(NodeId, MergeSide)> {
        for (id, node) in &self.nodes {
            if let LayoutNode::Split { first, second, .. } = node {
                if *first == target {
                    return Some((*id, MergeSide::First));
                }
                if *second == target {
                    return Some((*id, MergeSide::Second));
                }
            }
        }
        None
    }

    pub fn insert_at_root(
        &mut self,
        panel_type: PanelType,
        panel_id: PanelId,
        direction: SplitDirection,
        ratio: f32,
    ) {
        let old_root = self.root;

        // Move old root content to a new node.
        let old_root_new_id = self.alloc_node_id();
        if let Some(old_root_node) = self.nodes.remove(&old_root) {
            self.nodes.insert(old_root_new_id, old_root_node);

            // Fix: if old root was a split, its children still reference their own NodeIds,
            // which is fine. But we need to make sure the tree is consistent.
            // Actually, children of the old root still exist with their own NodeIds,
            // and the moved node still references them correctly. No fixup needed.
        }

        // Create the new leaf node.
        let new_leaf_id = self.alloc_node_id();
        self.nodes.insert(
            new_leaf_id,
            LayoutNode::Leaf {
                panel_type,
                panel_id,
            },
        );

        // Create the new root split at the old root's NodeId.
        self.nodes.insert(
            old_root,
            LayoutNode::Split {
                direction,
                ratio,
                first: old_root_new_id,
                second: new_leaf_id,
            },
        );
        // root NodeId stays the same.
    }
}

fn split_rect(rect: egui::Rect, direction: SplitDirection, ratio: f32) -> (egui::Rect, egui::Rect) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_leaf() {
        let node = LayoutNode::leaf(PanelType::Preview);
        assert!(node.is_leaf());
        assert_eq!(node.panel_type(), Some(PanelType::Preview));
    }

    #[test]
    fn split_leaf_vertical() {
        let mut tree = LayoutTree::new(PanelType::Preview);
        let root_id = tree.root_id();
        tree.split(root_id, SplitDirection::Vertical, 0.5);
        assert!(!tree.node(root_id).unwrap().is_leaf());
        let leaves = tree.collect_leaves();
        assert_eq!(leaves.len(), 2);
    }

    #[test]
    fn split_leaf_horizontal() {
        let mut tree = LayoutTree::new(PanelType::SceneEditor);
        let root_id = tree.root_id();
        tree.split(root_id, SplitDirection::Horizontal, 0.3);
        let leaves = tree.collect_leaves();
        assert_eq!(leaves.len(), 2);
        for (_, panel_type, _) in &leaves {
            assert_eq!(*panel_type, PanelType::SceneEditor);
        }
    }

    #[test]
    fn merge_collapses_split() {
        let mut tree = LayoutTree::new(PanelType::Preview);
        let root_id = tree.root_id();
        tree.split(root_id, SplitDirection::Vertical, 0.5);
        tree.merge(root_id, MergeSide::First);
        assert!(tree.node(tree.root_id()).unwrap().is_leaf());
    }

    #[test]
    fn resize_clamps_ratio() {
        let mut tree = LayoutTree::new(PanelType::Preview);
        let root_id = tree.root_id();
        tree.split(root_id, SplitDirection::Vertical, 0.5);
        tree.resize(root_id, 0.95);
        let node = tree.node(root_id).unwrap();
        assert!(node.ratio().unwrap() <= 0.9);
    }

    #[test]
    fn swap_panel_type() {
        let mut tree = LayoutTree::new(PanelType::Preview);
        let root_id = tree.root_id();
        tree.swap_type(root_id, PanelType::AudioMixer);
        assert_eq!(
            tree.node(root_id).unwrap().panel_type(),
            Some(PanelType::AudioMixer)
        );
    }

    #[test]
    fn collect_leaves_with_rects() {
        let mut tree = LayoutTree::new(PanelType::Preview);
        let root_id = tree.root_id();
        tree.split(root_id, SplitDirection::Vertical, 0.3);
        let total_rect =
            egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1000.0, 600.0));
        let leaves = tree.collect_leaves_with_rects(total_rect);
        assert_eq!(leaves.len(), 2);
        let (_, _, rect1, _) = &leaves[0];
        let (_, _, rect2, _) = &leaves[1];
        assert!((rect1.width() - 300.0).abs() < 1.0);
        assert!((rect2.width() - 700.0).abs() < 1.0);
    }

    #[test]
    fn panel_id_auto_increments() {
        let tree = LayoutTree::new(PanelType::Preview);
        let first_id = tree.collect_leaves()[0].0;
        let tree2 = LayoutTree::new(PanelType::SceneEditor);
        let second_id = tree2.collect_leaves()[0].0;
        assert_ne!(first_id, second_id);
    }

    #[test]
    fn default_layout() {
        let tree = LayoutTree::default_layout();
        let leaves = tree.collect_leaves();
        assert_eq!(leaves.len(), 4);
        let types: Vec<PanelType> = leaves.iter().map(|(_, t, _)| *t).collect();
        assert!(types.contains(&PanelType::SceneEditor));
        assert!(types.contains(&PanelType::Preview));
        assert!(types.contains(&PanelType::AudioMixer));
        assert!(types.contains(&PanelType::StreamControls));
    }

    #[test]
    fn remove_leaf_collapses_parent() {
        let mut tree = LayoutTree::new(PanelType::Preview);
        let root_id = tree.root_id();
        tree.split(root_id, SplitDirection::Vertical, 0.5);
        let leaves = tree.collect_leaves();
        assert_eq!(leaves.len(), 2);
        let removed_node = leaves[1].2;
        let removed = tree.remove_leaf(removed_node);
        assert!(removed.is_some());
        assert_eq!(tree.collect_leaves().len(), 1);
    }

    #[test]
    fn remove_last_leaf_returns_none() {
        let mut tree = LayoutTree::new(PanelType::Preview);
        let root_id = tree.root_id();
        assert!(tree.remove_leaf(root_id).is_none());
    }

    #[test]
    fn insert_at_root_splits_existing() {
        let mut tree = LayoutTree::new(PanelType::Preview);
        assert_eq!(tree.collect_leaves().len(), 1);
        tree.insert_at_root(
            PanelType::AudioMixer,
            PanelId::next(),
            SplitDirection::Vertical,
            0.5,
        );
        let leaves = tree.collect_leaves();
        assert_eq!(leaves.len(), 2);
        let types: Vec<PanelType> = leaves.iter().map(|(_, t, _)| *t).collect();
        assert!(types.contains(&PanelType::Preview));
        assert!(types.contains(&PanelType::AudioMixer));
    }
}
