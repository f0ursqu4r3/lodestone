//! Serialization and deserialization of [`DockLayout`] to/from TOML.
//!
//! The TOML format stores the split tree recursively, a flat list of groups
//! (with their tabs), floating group metadata, and optional detached window
//! entries for backwards compatibility.

use std::collections::HashMap;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::tree::{
    DockLayout, FloatingGroup, Group, GroupId, NodeId, PanelId, PanelType, SplitDirection,
    SplitNode, TabEntry,
};

// ---------------------------------------------------------------------------
// Serializable intermediate types
// ---------------------------------------------------------------------------

/// A serializable representation of a split-tree node.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type")]
enum SerializedNode {
    #[serde(rename = "leaf")]
    Leaf { group_id: u64 },
    #[serde(rename = "split")]
    Split {
        direction: SplitDirection,
        ratio: f32,
        first: Box<SerializedNode>,
        second: Box<SerializedNode>,
    },
}

/// Used for serialization — writes the PanelType enum directly.
#[derive(Serialize, Debug, Clone)]
struct SerializedTab {
    panel_id: u64,
    panel_type: PanelType,
}

/// A serializable group (used for serialization only).
#[derive(Serialize, Debug, Clone)]
struct SerializedGroup {
    id: u64,
    active_tab: usize,
    tabs: Vec<SerializedTab>,
}

/// Used for deserialization — tolerates unknown panel types.
#[derive(Deserialize, Debug, Clone)]
struct DeserializedTab {
    panel_id: u64,
    panel_type: toml::Value,
}

/// Deserialization counterpart of [`SerializedGroup`].
#[derive(Deserialize, Debug, Clone)]
struct DeserializedGroup {
    id: u64,
    active_tab: usize,
    tabs: Vec<DeserializedTab>,
}

/// A serializable floating group entry.
#[derive(Serialize, Deserialize, Debug, Clone)]
struct SerializedFloating {
    group_id: u64,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

/// Top-level TOML document structure (serialization only).
#[derive(Serialize, Debug, Clone)]
struct SerializedLayout {
    tree: SerializedNode,
    #[serde(default)]
    groups: Vec<SerializedGroup>,
    #[serde(default)]
    floating: Vec<SerializedFloating>,
    #[serde(default)]
    detached: Vec<DetachedEntry>,
}

/// Top-level TOML document structure (deserialization — tolerates unknown panel types).
#[derive(Deserialize, Debug, Clone)]
struct DeserializedLayout {
    tree: SerializedNode,
    #[serde(default)]
    groups: Vec<DeserializedGroup>,
    #[serde(default)]
    floating: Vec<SerializedFloating>,
    #[serde(default)]
    detached: Vec<DetachedEntry>,
}

/// Entry describing a detached (OS-level) window for backwards compatibility.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DetachedEntry {
    pub panel: PanelType,
    pub id: u64,
    #[serde(default)]
    pub group_id: u64,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

// ---------------------------------------------------------------------------
// Serialization helpers
// ---------------------------------------------------------------------------

/// Recursively build a [`SerializedNode`] from the layout's node map.
fn build_serialized_node(
    node_id: NodeId,
    nodes: &HashMap<NodeId, SplitNode>,
) -> Result<SerializedNode> {
    let node = nodes
        .get(&node_id)
        .with_context(|| format!("missing node {:?} in split tree", node_id))?;

    match node {
        SplitNode::Leaf { group_id } => Ok(SerializedNode::Leaf {
            group_id: group_id.0,
        }),
        SplitNode::Split {
            direction,
            ratio,
            first,
            second,
        } => {
            let first_ser = build_serialized_node(*first, nodes)?;
            let second_ser = build_serialized_node(*second, nodes)?;
            Ok(SerializedNode::Split {
                direction: *direction,
                ratio: *ratio,
                first: Box::new(first_ser),
                second: Box::new(second_ser),
            })
        }
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Serialize a [`DockLayout`] and detached entries to a TOML string.
pub fn serialize_full_layout(layout: &DockLayout, detached: &[DetachedEntry]) -> Result<String> {
    let tree = build_serialized_node(layout.root_id(), layout.nodes())?;

    let groups: Vec<SerializedGroup> = layout
        .groups
        .values()
        .map(|g| SerializedGroup {
            id: g.id.0,
            active_tab: g.active_tab,
            tabs: g
                .tabs
                .iter()
                .map(|t| SerializedTab {
                    panel_id: t.panel_id.0,
                    panel_type: t.panel_type,
                })
                .collect(),
        })
        .collect();

    let floating: Vec<SerializedFloating> = layout
        .floating
        .iter()
        .map(|f| SerializedFloating {
            group_id: f.group_id.0,
            x: f.pos.x,
            y: f.pos.y,
            width: f.size.x,
            height: f.size.y,
        })
        .collect();

    let doc = SerializedLayout {
        tree,
        groups,
        floating,
        detached: detached.to_vec(),
    };

    toml::to_string_pretty(&doc).context("failed to serialize layout to TOML")
}

/// Alias for [`serialize_full_layout`].
#[allow(dead_code)]
pub fn serialize_with_detached(layout: &DockLayout, detached: &[DetachedEntry]) -> Result<String> {
    serialize_full_layout(layout, detached)
}

/// Deserialize a [`DockLayout`] and detached entries from a TOML string.
///
/// Unknown panel types (e.g. a removed variant like `"Settings"`) are silently
/// dropped. If all tabs in a group are unknown the entire group is dropped, and
/// any leaf nodes referencing that group are replaced with a default Preview panel.
pub fn deserialize_full_layout(toml_str: &str) -> Result<(DockLayout, Vec<DetachedEntry>)> {
    let doc: DeserializedLayout =
        toml::from_str(toml_str).context("failed to parse layout TOML")?;

    // Rebuild groups, filtering out tabs with unknown panel types.
    let mut groups: HashMap<GroupId, Group> = HashMap::new();
    let mut max_group_id: u64 = 0;
    let mut max_panel_id: u64 = 0;

    for sg in &doc.groups {
        max_group_id = max_group_id.max(sg.id);
        let tabs: Vec<TabEntry> = sg
            .tabs
            .iter()
            .filter_map(|t| {
                let panel_type: PanelType = t.panel_type.clone().try_into().ok()?;
                max_panel_id = max_panel_id.max(t.panel_id);
                Some(TabEntry {
                    panel_id: PanelId(t.panel_id),
                    panel_type,
                })
            })
            .collect();

        if tabs.is_empty() {
            log::warn!(
                "Dropping group {} — all tabs had unknown panel types",
                sg.id
            );
            continue;
        }

        let active_tab = sg.active_tab.min(tabs.len().saturating_sub(1));
        let group = Group {
            id: GroupId(sg.id),
            tabs,
            active_tab,
        };
        groups.insert(group.id, group);
    }

    // Also scan detached entries for max IDs.
    for d in &doc.detached {
        max_panel_id = max_panel_id.max(d.id);
        max_group_id = max_group_id.max(d.group_id);
    }

    // Rebuild the split tree from the recursive serialized node.
    let mut nodes: HashMap<NodeId, SplitNode> = HashMap::new();
    let mut next_node_id: u64 = 0;

    let root_id = rebuild_node(&doc.tree, &mut nodes, &mut next_node_id)?;

    // Rebuild floating groups.
    let floating: Vec<FloatingGroup> = doc
        .floating
        .iter()
        .map(|f| FloatingGroup {
            group_id: GroupId(f.group_id),
            pos: egui::pos2(f.x, f.y),
            size: egui::vec2(f.width, f.height),
        })
        .collect();

    // Advance counters so new IDs won't collide with deserialized ones.
    PanelId::set_counter(max_panel_id + 1);
    GroupId::set_counter(max_group_id + 1);

    // Repair leaf nodes that reference dropped groups.
    repair_orphaned_leaves(&mut nodes, &mut groups);

    // Filter out floating groups referencing dropped groups
    let floating: Vec<FloatingGroup> = floating
        .into_iter()
        .filter(|f| groups.contains_key(&f.group_id))
        .collect();

    let layout = DockLayout::from_parts(nodes, root_id, next_node_id, groups, floating);

    Ok((layout, doc.detached))
}

/// Alias for [`deserialize_full_layout`].
#[allow(dead_code)]
pub fn deserialize_with_detached(toml_str: &str) -> Result<(DockLayout, Vec<DetachedEntry>)> {
    deserialize_full_layout(toml_str)
}

/// Replace leaf nodes whose group was dropped (e.g. because all tabs had
/// unknown panel types) with a new default Preview group.
fn repair_orphaned_leaves(
    nodes: &mut HashMap<NodeId, SplitNode>,
    groups: &mut HashMap<GroupId, Group>,
) {
    let node_ids: Vec<NodeId> = nodes.keys().copied().collect();
    for node_id in node_ids {
        if let Some(SplitNode::Leaf { group_id }) = nodes.get(&node_id)
            && !groups.contains_key(group_id)
        {
            let new_group = Group::new(PanelType::Preview);
            let new_gid = new_group.id;
            groups.insert(new_gid, new_group);
            nodes.insert(node_id, SplitNode::Leaf { group_id: new_gid });
            log::warn!(
                "Replaced orphaned leaf node {:?} with default Preview group",
                node_id
            );
        }
    }
}

/// Recursively rebuild split-tree nodes from a [`SerializedNode`], assigning
/// incremental [`NodeId`]s.
fn rebuild_node(
    snode: &SerializedNode,
    nodes: &mut HashMap<NodeId, SplitNode>,
    next_id: &mut u64,
) -> Result<NodeId> {
    let id = NodeId(*next_id);
    *next_id += 1;

    match snode {
        SerializedNode::Leaf { group_id } => {
            nodes.insert(
                id,
                SplitNode::Leaf {
                    group_id: GroupId(*group_id),
                },
            );
        }
        SerializedNode::Split {
            direction,
            ratio,
            first,
            second,
        } => {
            // Children must be built first so they get their own IDs.
            let first_id = rebuild_node(first, nodes, next_id)?;
            let second_id = rebuild_node(second, nodes, next_id)?;
            nodes.insert(
                id,
                SplitNode::Split {
                    direction: *direction,
                    ratio: *ratio,
                    first: first_id,
                    second: second_id,
                },
            );
        }
    }

    Ok(id)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::layout::tree::DockLayout;

    #[test]
    fn roundtrip_single_group() {
        let layout = DockLayout::new_single(PanelType::Preview);
        let toml_str = serialize_full_layout(&layout, &[]).unwrap();
        let (restored, detached) = deserialize_full_layout(&toml_str).unwrap();

        assert!(detached.is_empty());
        assert_eq!(restored.groups.len(), 1);

        let group = restored.groups.values().next().unwrap();
        assert_eq!(group.tabs.len(), 1);
        assert_eq!(group.tabs[0].panel_type, PanelType::Preview);

        // Root should be a leaf.
        assert!(matches!(
            restored.node(restored.root_id()),
            Some(SplitNode::Leaf { .. })
        ));
    }

    #[test]
    fn roundtrip_default_layout() {
        let layout = DockLayout::default_layout();
        let toml_str = serialize_full_layout(&layout, &[]).unwrap();
        let (restored, _) = deserialize_full_layout(&toml_str).unwrap();

        assert_eq!(restored.groups.len(), 3);
        let all_panels = restored.collect_all_panels();
        assert_eq!(all_panels.len(), 4);

        let types: Vec<PanelType> = all_panels.iter().map(|(_, t)| *t).collect();
        assert!(types.contains(&PanelType::SceneEditor));
        assert!(types.contains(&PanelType::Preview));
        assert!(types.contains(&PanelType::AudioMixer));
        assert!(types.contains(&PanelType::StreamControls));
    }

    #[test]
    fn roundtrip_floating_groups() {
        let mut layout = DockLayout::new_single(PanelType::Preview);
        let entry = TabEntry {
            panel_id: PanelId::next(),
            panel_type: PanelType::AudioMixer,
        };
        layout.add_floating_group(entry, egui::pos2(100.0, 200.0));

        let toml_str = serialize_full_layout(&layout, &[]).unwrap();
        let (restored, _) = deserialize_full_layout(&toml_str).unwrap();

        assert_eq!(restored.floating.len(), 1);
        let fg = &restored.floating[0];
        assert!((fg.pos.x - 100.0).abs() < 0.01);
        assert!((fg.pos.y - 200.0).abs() < 0.01);
        assert!((fg.size.x - 400.0).abs() < 0.01);
        assert!((fg.size.y - 300.0).abs() < 0.01);

        // The floating group should exist in the groups map.
        assert!(restored.groups.contains_key(&fg.group_id));
    }

    #[test]
    fn panel_id_preservation() {
        let layout = DockLayout::default_layout();
        let original_ids: Vec<u64> = layout
            .collect_all_panels()
            .iter()
            .map(|(id, _)| id.0)
            .collect();

        let toml_str = serialize_full_layout(&layout, &[]).unwrap();
        let (restored, _) = deserialize_full_layout(&toml_str).unwrap();

        let restored_ids: Vec<u64> = restored
            .collect_all_panels()
            .iter()
            .map(|(id, _)| id.0)
            .collect();

        // All original IDs should be present in the restored layout.
        for id in &original_ids {
            assert!(
                restored_ids.contains(id),
                "panel id {id} missing after roundtrip"
            );
        }
    }

    #[test]
    fn roundtrip_detached_entries() {
        let layout = DockLayout::new_single(PanelType::Preview);
        let detached = vec![DetachedEntry {
            panel: PanelType::StreamControls,
            id: 99,
            group_id: 50,
            x: 100,
            y: 200,
            width: 400,
            height: 300,
        }];

        let toml_str = serialize_full_layout(&layout, &detached).unwrap();
        let (_, restored_detached) = deserialize_full_layout(&toml_str).unwrap();

        assert_eq!(restored_detached.len(), 1);
        let d = &restored_detached[0];
        assert_eq!(d.id, 99);
        assert_eq!(d.group_id, 50);
        assert!(matches!(d.panel, PanelType::StreamControls));
        assert_eq!(d.x, 100);
        assert_eq!(d.y, 200);
        assert_eq!(d.width, 400);
        assert_eq!(d.height, 300);
    }

    #[test]
    fn detached_entry_group_id_default() {
        // group_id should default to 0 when missing from TOML (backwards compat).
        let toml_str = r#"
[tree]
type = "leaf"
group_id = 1

[[groups]]
id = 1
active_tab = 0
[[groups.tabs]]
panel_id = 1
panel_type = "Preview"

[[detached]]
panel = "StreamControls"
id = 99
x = 0
y = 0
width = 400
height = 300
"#;
        let (_, detached) = deserialize_full_layout(toml_str).unwrap();
        assert_eq!(detached.len(), 1);
        assert_eq!(detached[0].group_id, 0);
    }

    #[test]
    fn invalid_toml_returns_error() {
        let result = deserialize_full_layout("this is not valid toml {{{{");
        assert!(result.is_err());
    }

    #[test]
    fn unknown_panel_type_drops_gracefully() {
        let toml_str = r#"
[tree]
type = "leaf"
group_id = 1

[[groups]]
id = 1
active_tab = 0
[[groups.tabs]]
panel_id = 1
panel_type = "Settings"
[[groups.tabs]]
panel_id = 2
panel_type = "Preview"
"#;
        let result = deserialize_full_layout(toml_str);
        assert!(result.is_ok());
        let (layout, _) = result.unwrap();
        let all_panels = layout.collect_all_panels();
        assert_eq!(all_panels.len(), 1);
        assert_eq!(all_panels[0].1, PanelType::Preview);
    }

    #[test]
    fn all_unknown_tabs_group_gets_repaired() {
        let toml_str = r#"
[tree]
type = "split"
direction = "Vertical"
ratio = 0.5

[tree.first]
type = "leaf"
group_id = 1

[tree.second]
type = "leaf"
group_id = 2

[[groups]]
id = 1
active_tab = 0
[[groups.tabs]]
panel_id = 1
panel_type = "Preview"

[[groups]]
id = 2
active_tab = 0
[[groups.tabs]]
panel_id = 2
panel_type = "Settings"
"#;
        let result = deserialize_full_layout(toml_str);
        assert!(result.is_ok());
        let (layout, _) = result.unwrap();
        // Group 2's Settings tab is unknown, so it gets replaced with a default Preview group
        assert_eq!(layout.groups.len(), 2);
        let all_panels = layout.collect_all_panels();
        assert!(all_panels.iter().all(|(_, t)| *t == PanelType::Preview));
    }

    #[test]
    fn alias_functions_work() {
        let layout = DockLayout::new_single(PanelType::Preview);
        let toml_str = serialize_with_detached(&layout, &[]).unwrap();
        let (restored, _) = deserialize_with_detached(&toml_str).unwrap();
        assert_eq!(restored.groups.len(), 1);
    }
}
