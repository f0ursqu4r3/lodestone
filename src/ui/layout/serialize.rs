use std::collections::HashMap;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::tree::{LayoutNode, LayoutTree, NodeId, PanelId, PanelType, SplitDirection};

// ---------------------------------------------------------------------------
// SavedLayout — full multi-window layout wrapper
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize)]
pub struct SavedLayout {
    pub(crate) layout: SerializedNode,
    #[serde(default)]
    pub detached: Vec<DetachedEntry>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DetachedEntry {
    pub panel: PanelType,
    pub id: u64,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

// ---------------------------------------------------------------------------
// SerializedNode — recursive serde representation
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
pub(crate) enum SerializedNode {
    #[serde(rename = "leaf")]
    Leaf { panel: PanelType, id: u64 },
    #[serde(rename = "split")]
    Split {
        direction: SplitDirection,
        ratio: f32,
        first: Box<SerializedNode>,
        second: Box<SerializedNode>,
    },
}

#[derive(Serialize, Deserialize)]
#[allow(dead_code)]
struct SerializedTree {
    root: SerializedNode,
}

// ---------------------------------------------------------------------------
// Tree → SerializedNode
// ---------------------------------------------------------------------------

fn build_serialized_node(tree: &LayoutTree, node_id: NodeId) -> SerializedNode {
    match tree.node(node_id).expect("node missing from tree") {
        LayoutNode::Leaf {
            panel_type,
            panel_id,
        } => SerializedNode::Leaf {
            panel: *panel_type,
            id: panel_id.0,
        },
        LayoutNode::Split {
            direction,
            ratio,
            first,
            second,
        } => SerializedNode::Split {
            direction: *direction,
            ratio: *ratio,
            first: Box::new(build_serialized_node(tree, *first)),
            second: Box::new(build_serialized_node(tree, *second)),
        },
    }
}

#[allow(dead_code)]
pub fn serialize_layout(tree: &LayoutTree) -> Result<String> {
    let root = build_serialized_node(tree, tree.root_id());
    let toml_str = toml::to_string_pretty(&SerializedTree { root })?;
    Ok(toml_str)
}

// ---------------------------------------------------------------------------
// SerializedNode → LayoutTree
// ---------------------------------------------------------------------------

struct RebuildState {
    nodes: HashMap<NodeId, LayoutNode>,
    next_node_id: u64,
    max_panel_id: u64,
}

impl RebuildState {
    fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            next_node_id: 0,
            max_panel_id: 0,
        }
    }

    fn alloc_node_id(&mut self) -> NodeId {
        let id = NodeId(self.next_node_id);
        self.next_node_id += 1;
        id
    }

    fn insert_serialized(&mut self, snode: SerializedNode) -> NodeId {
        match snode {
            SerializedNode::Leaf { panel, id } => {
                let node_id = self.alloc_node_id();
                if id > self.max_panel_id {
                    self.max_panel_id = id;
                }
                self.nodes.insert(
                    node_id,
                    LayoutNode::Leaf {
                        panel_type: panel,
                        panel_id: PanelId(id),
                    },
                );
                node_id
            }
            SerializedNode::Split {
                direction,
                ratio,
                first,
                second,
            } => {
                let first_id = self.insert_serialized(*first);
                let second_id = self.insert_serialized(*second);
                let node_id = self.alloc_node_id();
                self.nodes.insert(
                    node_id,
                    LayoutNode::Split {
                        direction,
                        ratio,
                        first: first_id,
                        second: second_id,
                    },
                );
                node_id
            }
        }
    }
}

#[allow(dead_code)]
pub fn deserialize_layout(toml_str: &str) -> Result<LayoutTree> {
    let serialized: SerializedTree = toml::from_str(toml_str)?;

    let mut state = RebuildState::new();
    let root_id = state.insert_serialized(serialized.root);

    // Advance the global PanelId counter so new panels won't collide.
    PanelId::set_counter(state.max_panel_id + 1);

    let tree = LayoutTree::from_parts(state.nodes, root_id, state.next_node_id);
    Ok(tree)
}

// ---------------------------------------------------------------------------
// Full layout (main tree + detached windows)
// ---------------------------------------------------------------------------

pub fn serialize_full_layout(tree: &LayoutTree, detached: &[DetachedEntry]) -> Result<String> {
    let layout = build_serialized_node(tree, tree.root_id());
    let saved = SavedLayout {
        layout,
        detached: detached.to_vec(),
    };
    let toml_str = toml::to_string_pretty(&saved)?;
    Ok(toml_str)
}

pub fn deserialize_full_layout(toml_str: &str) -> Result<(LayoutTree, Vec<DetachedEntry>)> {
    let saved: SavedLayout = toml::from_str(toml_str)?;

    let mut state = RebuildState::new();
    let root_id = state.insert_serialized(saved.layout);

    PanelId::set_counter(state.max_panel_id + 1);

    let tree = LayoutTree::from_parts(state.nodes, root_id, state.next_node_id);
    Ok((tree, saved.detached))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_leaf_roundtrip() {
        let tree = LayoutTree::new(PanelType::Preview);
        let toml_str = serialize_layout(&tree).unwrap();
        let restored = deserialize_layout(&toml_str).unwrap();
        let leaves = restored.collect_leaves();
        assert_eq!(leaves.len(), 1);
        assert_eq!(leaves[0].1, PanelType::Preview);
    }

    #[test]
    fn split_tree_roundtrip() {
        let mut tree = LayoutTree::new(PanelType::Preview);
        let root = tree.root_id();
        tree.split(root, SplitDirection::Vertical, 0.3);
        let toml_str = serialize_layout(&tree).unwrap();
        let restored = deserialize_layout(&toml_str).unwrap();
        let leaves = restored.collect_leaves();
        assert_eq!(leaves.len(), 2);
    }

    #[test]
    fn default_layout_roundtrip() {
        let tree = LayoutTree::default_layout();
        let toml_str = serialize_layout(&tree).unwrap();
        let restored = deserialize_layout(&toml_str).unwrap();
        assert_eq!(restored.collect_leaves().len(), 4);
    }

    #[test]
    fn panel_ids_preserved() {
        let tree = LayoutTree::new(PanelType::SceneEditor);
        let original_id = tree.collect_leaves()[0].0;
        let toml_str = serialize_layout(&tree).unwrap();
        let restored = deserialize_layout(&toml_str).unwrap();
        let restored_id = restored.collect_leaves()[0].0;
        assert_eq!(original_id, restored_id);
    }

    #[test]
    fn invalid_toml_returns_error() {
        assert!(deserialize_layout("not valid toml {{{}}}").is_err());
    }

    #[test]
    fn full_layout_save_load_roundtrip() {
        let tree = LayoutTree::default_layout();
        let detached = vec![DetachedEntry {
            panel: PanelType::StreamControls,
            id: 99,
            x: 100,
            y: 100,
            width: 400,
            height: 300,
        }];
        let toml_str = serialize_full_layout(&tree, &detached).unwrap();
        let (restored_tree, restored_detached) = deserialize_full_layout(&toml_str).unwrap();
        assert_eq!(restored_tree.collect_leaves().len(), 4);
        assert_eq!(restored_detached.len(), 1);
    }
}
