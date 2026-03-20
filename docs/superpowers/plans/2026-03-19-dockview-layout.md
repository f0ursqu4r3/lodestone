# Dockview-Style Layout System Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the current Blender-style binary split layout with a dockview.dev-style tabbed group system with drag-to-dock and floating groups.

**Architecture:** Tree leaves become `GroupId` references into a `HashMap<GroupId, Group>`, where each `Group` holds `Vec<TabEntry>` (tabbed panels). Drag-and-drop uses a 5-zone overlay (left/right/top/bottom/center) to split groups or merge tabs. Floating groups render as `egui::Window` overlays above the grid. The panel draw contract (`draw(ui, state, panel_id)`) is unchanged.

**Tech Stack:** Rust, egui, wgpu (rendering unchanged), serde/toml (serialization)

**Spec:** `docs/superpowers/specs/2026-03-19-dockview-layout-design.md`

---

## File Structure

### Files to rewrite (full replacement)

| File | Responsibility |
|------|---------------|
| `src/ui/layout/tree.rs` | `GroupId`, `Group`, `TabEntry`, `DockLayout`, `SplitNode`, `SplitTree`, `FloatingGroup`, `DragState`, `DropZone` — all data types and layout operations |
| `src/ui/layout/render.rs` | Tab bar rendering, group content, drop zone overlays, ghost drag label, menu bar, divider rendering |
| `src/ui/layout/interactions.rs` | Drag-and-drop state machine, drop zone hit testing, divider collection |
| `src/ui/layout/serialize.rs` | Serialization/deserialization for `DockLayout` (grid + floating + groups) |

### Files to modify

| File | Changes |
|------|---------|
| `src/ui/layout/mod.rs` | Update re-exports for new types |
| `src/window.rs` | Replace `LayoutTree` with `DockLayout`, update action handling |
| `src/main.rs` | Update imports, `load_layout`, `save_layout`, `reset_layout`, detach/reattach logic |

### Files unchanged

All panel content files (`scene_editor.rs`, `audio_mixer.rs`, `stream_controls.rs`, `settings_panel.rs`, `preview_panel.rs`), `src/ui/mod.rs` (`draw_panel` dispatch), `src/state.rs`, `src/obs/`, `src/renderer/`, `src/settings.rs`, `src/mock_driver.rs`.

---

## Task 1: Data Model — Core Types

**Files:**
- Rewrite: `src/ui/layout/tree.rs`

This task builds ALL data types and layout operations. The existing `PanelType`, `PanelId`, and `SplitDirection` types are preserved. `LayoutNode`, `LayoutTree`, `NodeId` are replaced with the new model.

- [ ] **Step 1: Write tests for GroupId, Group, TabEntry**

Add to the bottom of `src/ui/layout/tree.rs` (after clearing the old code):

```rust
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
        assert_eq!(group.active_tab, 1); // newly added tab becomes active
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
        // Can't remove the last tab
        assert!(group.remove_tab(0).is_none());
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib ui::layout::tree::tests -- --nocapture 2>&1 | head -30`
Expected: compilation errors — types don't exist yet.

- [ ] **Step 3: Write GroupId, Group, TabEntry types**

Replace the entire contents of `src/ui/layout/tree.rs` with:

```rust
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// PanelType (unchanged)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum PanelType {
    Preview,
    SceneEditor,
    AudioMixer,
    StreamControls,
    Settings,
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
    pub fn is_dockable(&self) -> bool {
        !matches!(self, Self::Settings)
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

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub struct GroupId(pub u64);

impl GroupId {
    pub fn next() -> Self {
        Self(GROUP_ID_COUNTER.fetch_add(1, Ordering::Relaxed))
    }

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
    pub fn add_tab_entry(&mut self, entry: TabEntry) {
        self.tabs.push(entry);
        self.active_tab = self.tabs.len() - 1;
    }

    /// Insert a tab at a specific index.
    pub fn insert_tab(&mut self, index: usize, entry: TabEntry) {
        let index = index.min(self.tabs.len());
        self.tabs.insert(index, entry);
        self.active_tab = index;
    }

    /// Remove a tab by index. Returns None if it's the last tab.
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
    Leaf { group_id: GroupId },
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
pub struct FloatingGroup {
    pub group_id: GroupId,
    pub pos: egui::Pos2,
    pub size: egui::Vec2,
}

// ---------------------------------------------------------------------------
// DropZone — where a dragged tab can be dropped
// ---------------------------------------------------------------------------

/// Where a dragged tab can be dropped relative to a target group.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
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
pub struct DragState {
    pub panel_id: PanelId,
    pub panel_type: PanelType,
    pub source_group: GroupId,
    pub tab_index: usize,
}

// ---------------------------------------------------------------------------
// DockLayout — the top-level layout state per window
// ---------------------------------------------------------------------------

/// Top-level layout state per window. Contains the split tree, groups, floating groups, and drag state.
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

impl DockLayout {
    // --- Node allocation ---

    fn alloc_node_id(&mut self) -> NodeId {
        let id = NodeId(self.next_node_id);
        self.next_node_id += 1;
        id
    }

    // --- Accessors ---

    pub fn root_id(&self) -> NodeId {
        self.root
    }

    pub fn node(&self, id: NodeId) -> Option<&SplitNode> {
        self.nodes.get(&id)
    }

    pub fn nodes(&self) -> &HashMap<NodeId, SplitNode> {
        &self.nodes
    }

    // --- Construction ---

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

        // Create groups
        let scene_group = Group::new(PanelType::SceneEditor);
        let scene_gid = scene_group.id;
        layout.groups.insert(scene_gid, scene_group);

        let preview_group = Group::new(PanelType::Preview);
        let preview_gid = preview_group.id;
        layout.groups.insert(preview_gid, preview_group);

        // AudioMixer + StreamControls share a group as tabs
        let mut right_group = Group::new(PanelType::AudioMixer);
        let right_gid = right_group.id;
        right_group.add_tab(PanelType::StreamControls);
        right_group.active_tab = 0; // AudioMixer is active by default
        layout.groups.insert(right_gid, right_group);

        // Build tree: Split(Vertical, 0.2) -> SceneEditor | Split(Horizontal, 0.75) -> Preview | RightGroup
        let scene_node = layout.alloc_node_id();
        layout.nodes.insert(scene_node, SplitNode::Leaf { group_id: scene_gid });

        let preview_node = layout.alloc_node_id();
        layout.nodes.insert(preview_node, SplitNode::Leaf { group_id: preview_gid });

        let right_node = layout.alloc_node_id();
        layout.nodes.insert(right_node, SplitNode::Leaf { group_id: right_gid });

        let right_split = layout.alloc_node_id();
        layout.nodes.insert(right_split, SplitNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.75,
            first: preview_node,
            second: right_node,
        });

        let root = layout.alloc_node_id();
        layout.nodes.insert(root, SplitNode::Split {
            direction: SplitDirection::Vertical,
            ratio: 0.2,
            first: scene_node,
            second: right_split,
        });
        layout.root = root;

        layout
    }

    // --- Split tree operations ---

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
    /// Returns the new group's GroupId.
    pub fn split_group(
        &mut self,
        target_group: GroupId,
        direction: SplitDirection,
        new_panel_type: PanelType,
        new_first: bool, // true = new group goes first (left/top)
    ) -> Option<GroupId> {
        let node_id = self.find_node_for_group(target_group)?;

        let new_group = Group::new(new_panel_type);
        let new_gid = new_group.id;
        self.groups.insert(new_gid, new_group);

        let existing_child = self.alloc_node_id();
        let new_child = self.alloc_node_id();

        self.nodes.insert(existing_child, SplitNode::Leaf { group_id: target_group });
        self.nodes.insert(new_child, SplitNode::Leaf { group_id: new_gid });

        let (first, second) = if new_first {
            (new_child, existing_child)
        } else {
            (existing_child, new_child)
        };

        self.nodes.insert(node_id, SplitNode::Split {
            direction,
            ratio: 0.5,
            first,
            second,
        });

        Some(new_gid)
    }

    /// Split a group's node by placing a specific TabEntry in the new group.
    /// Used for drop-to-split during drag-and-drop.
    pub fn split_group_with_tab(
        &mut self,
        target_group: GroupId,
        direction: SplitDirection,
        tab_entry: TabEntry,
        new_first: bool,
    ) -> Option<GroupId> {
        let node_id = self.find_node_for_group(target_group)?;

        let new_group = Group::new_with_ids(GroupId::next(), tab_entry.panel_id, tab_entry.panel_type);
        let new_gid = new_group.id;
        self.groups.insert(new_gid, new_group);

        let existing_child = self.alloc_node_id();
        let new_child = self.alloc_node_id();

        self.nodes.insert(existing_child, SplitNode::Leaf { group_id: target_group });
        self.nodes.insert(new_child, SplitNode::Leaf { group_id: new_gid });

        let (first, second) = if new_first {
            (new_child, existing_child)
        } else {
            (existing_child, new_child)
        };

        self.nodes.insert(node_id, SplitNode::Split {
            direction,
            ratio: 0.5,
            first,
            second,
        });

        Some(new_gid)
    }

    /// Remove a group from the grid. If it was the only child, returns false.
    /// Collapses the parent split, promoting the sibling.
    pub fn remove_group_from_grid(&mut self, group_id: GroupId) -> bool {
        let node_id = match self.find_node_for_group(group_id) {
            Some(id) => id,
            None => return false,
        };

        // Can't remove if it's the root leaf
        if node_id == self.root {
            return false;
        }

        let parent_id = match self.find_parent(node_id) {
            Some(id) => id,
            None => return false,
        };

        // Get sibling
        let sibling_id = match self.nodes.get(&parent_id) {
            Some(SplitNode::Split { first, second, .. }) => {
                if *first == node_id { *second } else { *first }
            }
            _ => return false,
        };

        // Remove the leaf node
        self.nodes.remove(&node_id);

        // Replace parent with sibling's content
        if let Some(sibling_node) = self.nodes.remove(&sibling_id) {
            self.nodes.insert(parent_id, sibling_node);
        }

        // Remove the group itself
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
        self.nodes.insert(new_leaf_id, SplitNode::Leaf { group_id: new_gid });

        self.nodes.insert(old_root, SplitNode::Split {
            direction,
            ratio,
            first: old_root_new_id,
            second: new_leaf_id,
        });
    }

    // --- Collect helpers ---

    /// Collect all grid groups with their computed screen rects.
    pub fn collect_groups_with_rects(
        &self,
        rect: egui::Rect,
    ) -> Vec<(GroupId, egui::Rect)> {
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
            Some(SplitNode::Split { direction, ratio, first, second }) => {
                let (first_rect, second_rect) = split_rect(rect, *direction, *ratio);
                self.collect_groups_recursive(*first, first_rect, result);
                self.collect_groups_recursive(*second, second_rect, result);
            }
            None => {}
        }
    }

    /// Collect all panels across all groups (grid + floating) as (PanelId, PanelType).
    pub fn collect_all_panels(&self) -> Vec<(PanelId, PanelType)> {
        let mut result = Vec::new();
        for group in self.groups.values() {
            for tab in &group.tabs {
                result.push((tab.panel_id, tab.panel_type));
            }
        }
        result
    }

    // --- Floating group operations ---

    /// Create a floating group from a tab entry.
    pub fn add_floating_group(&mut self, entry: TabEntry, pos: egui::Pos2) -> GroupId {
        let group = Group::new_with_ids(GroupId::next(), entry.panel_id, entry.panel_type);
        let gid = group.id;
        self.groups.insert(gid, group);
        self.floating.push(FloatingGroup {
            group_id: gid,
            pos,
            size: egui::vec2(400.0, 300.0),
        });
        gid
    }

    /// Remove a floating group entry (does NOT remove the group from self.groups).
    pub fn remove_floating(&mut self, group_id: GroupId) {
        self.floating.retain(|f| f.group_id != group_id);
    }

    /// Check if a group is floating.
    pub fn is_floating(&self, group_id: GroupId) -> bool {
        self.floating.iter().any(|f| f.group_id == group_id)
    }

    /// Remove a tab from its source group, cleaning up empty groups.
    /// Returns the removed TabEntry if successful.
    pub fn take_tab(&mut self, group_id: GroupId, tab_index: usize) -> Option<TabEntry> {
        let group = self.groups.get_mut(&group_id)?;
        if group.tabs.len() <= 1 {
            // Last tab — remove the entire group
            let entry = group.tabs[0].clone();
            if self.is_floating(group_id) {
                self.remove_floating(group_id);
                self.groups.remove(&group_id);
            } else {
                // Grid group — collapse parent split
                self.remove_group_from_grid(group_id);
            }
            Some(entry)
        } else {
            group.remove_tab(tab_index)
        }
    }
}

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
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib ui::layout::tree::tests -- --nocapture`
Expected: all 5 tests pass.

- [ ] **Step 5: Write tests for DockLayout operations**

Append to the `#[cfg(test)] mod tests` block:

```rust
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
        let new_gid = layout.split_group(gid, SplitDirection::Vertical, PanelType::AudioMixer, false);
        assert!(new_gid.is_some());
        assert_eq!(layout.groups.len(), 2);
        let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1000.0, 600.0));
        assert_eq!(layout.collect_groups_with_rects(rect).len(), 2);
    }

    #[test]
    fn remove_group_collapses_parent() {
        let mut layout = DockLayout::new_single(PanelType::Preview);
        let original_gid = layout.groups.keys().next().copied().unwrap();
        let new_gid = layout.split_group(original_gid, SplitDirection::Vertical, PanelType::AudioMixer, false).unwrap();
        assert!(layout.remove_group_from_grid(new_gid));
        assert_eq!(layout.groups.len(), 1);
        // Root should be a leaf again
        assert!(matches!(layout.node(layout.root_id()), Some(SplitNode::Leaf { .. })));
    }

    #[test]
    fn cannot_remove_root_leaf_group() {
        let mut layout = DockLayout::new_single(PanelType::Preview);
        let gid = layout.groups.keys().next().copied().unwrap();
        assert!(!layout.remove_group_from_grid(gid));
    }

    #[test]
    fn floating_group_lifecycle() {
        let mut layout = DockLayout::new_single(PanelType::Preview);
        let entry = TabEntry { panel_id: PanelId::next(), panel_type: PanelType::AudioMixer };
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
        layout.groups.get_mut(&gid).unwrap().add_tab(PanelType::AudioMixer);
        assert_eq!(layout.groups[&gid].tabs.len(), 2);
        let taken = layout.take_tab(gid, 1);
        assert!(taken.is_some());
        assert_eq!(taken.unwrap().panel_type, PanelType::AudioMixer);
        assert_eq!(layout.groups[&gid].tabs.len(), 1);
    }

    #[test]
    fn take_last_tab_removes_floating_group() {
        let mut layout = DockLayout::new_single(PanelType::Preview);
        let entry = TabEntry { panel_id: PanelId::next(), panel_type: PanelType::AudioMixer };
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
        let new_gid = layout.split_group(original_gid, SplitDirection::Vertical, PanelType::AudioMixer, false).unwrap();
        assert_eq!(layout.groups.len(), 2);
        let taken = layout.take_tab(new_gid, 0);
        assert!(taken.is_some());
        assert_eq!(layout.groups.len(), 1);
        // Root should be a leaf again after collapsing
        assert!(matches!(layout.node(layout.root_id()), Some(SplitNode::Leaf { .. })));
    }

    #[test]
    fn insert_at_root_adds_group() {
        let mut layout = DockLayout::new_single(PanelType::Preview);
        assert_eq!(layout.groups.len(), 1);
        layout.insert_at_root(PanelType::AudioMixer, PanelId::next(), SplitDirection::Vertical, 0.5);
        assert_eq!(layout.groups.len(), 2);
        let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1000.0, 600.0));
        assert_eq!(layout.collect_groups_with_rects(rect).len(), 2);
    }
```

- [ ] **Step 6: Run all tree tests**

Run: `cargo test --lib ui::layout::tree::tests -- --nocapture`
Expected: all tests pass.

- [ ] **Step 7: Commit**

```bash
git add src/ui/layout/tree.rs
git commit -m "feat: replace LayoutTree with DockLayout data model (groups, tabs, floating)"
```

---

## Task 2: Interactions — Dividers and Drop Zone Hit Testing

**Files:**
- Rewrite: `src/ui/layout/interactions.rs`

- [ ] **Step 1: Write tests for drop zone hit testing**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hit_test_center() {
        let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(500.0, 400.0));
        let center = egui::pos2(250.0, 200.0);
        assert_eq!(hit_test_drop_zone(rect, center), DropZone::Center);
    }

    #[test]
    fn hit_test_left() {
        let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(500.0, 400.0));
        let left = egui::pos2(30.0, 200.0);
        assert_eq!(hit_test_drop_zone(rect, left), DropZone::Left);
    }

    #[test]
    fn hit_test_right() {
        let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(500.0, 400.0));
        let right = egui::pos2(480.0, 200.0);
        assert_eq!(hit_test_drop_zone(rect, right), DropZone::Right);
    }

    #[test]
    fn hit_test_top() {
        let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(500.0, 400.0));
        let top = egui::pos2(250.0, 30.0);
        assert_eq!(hit_test_drop_zone(rect, top), DropZone::Top);
    }

    #[test]
    fn hit_test_bottom() {
        let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(500.0, 400.0));
        let bottom = egui::pos2(250.0, 380.0);
        assert_eq!(hit_test_drop_zone(rect, bottom), DropZone::Bottom);
    }

    #[test]
    fn divider_collection() {
        let layout = DockLayout::new_single(PanelType::Preview);
        let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1000.0, 600.0));
        // Single group = no dividers
        let dividers = collect_dividers(&layout, rect);
        assert_eq!(dividers.len(), 0);
    }

    #[test]
    fn divider_after_split() {
        let mut layout = DockLayout::new_single(PanelType::Preview);
        let gid = layout.groups.keys().next().copied().unwrap();
        layout.split_group(gid, SplitDirection::Vertical, PanelType::AudioMixer, false);
        let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1000.0, 600.0));
        let dividers = collect_dividers(&layout, rect);
        assert_eq!(dividers.len(), 1);
        assert_eq!(dividers[0].direction, SplitDirection::Vertical);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib ui::layout::interactions::tests -- --nocapture 2>&1 | head -20`
Expected: compilation errors.

- [ ] **Step 3: Write interactions module**

Replace `src/ui/layout/interactions.rs`:

```rust
use super::tree::{DockLayout, DropZone, GroupId, NodeId, SplitDirection, SplitNode, split_rect};
use crate::ui::layout::PanelType;

/// A rectangle representing a draggable divider between two split children.
pub struct DividerRect {
    pub rect: egui::Rect,
    pub node_id: NodeId,
    pub direction: SplitDirection,
    pub parent_rect: egui::Rect,
}

/// Collect all divider rects from the split tree.
pub fn collect_dividers(layout: &DockLayout, total_rect: egui::Rect) -> Vec<DividerRect> {
    let mut dividers = Vec::new();
    collect_dividers_recursive(layout, layout.root_id(), total_rect, &mut dividers);
    dividers
}

fn collect_dividers_recursive(
    layout: &DockLayout,
    node_id: NodeId,
    rect: egui::Rect,
    dividers: &mut Vec<DividerRect>,
) {
    let node = match layout.node(node_id) {
        Some(n) => n,
        None => return,
    };

    if let SplitNode::Split {
        direction,
        ratio,
        first,
        second,
    } = node
    {
        let direction = *direction;
        let ratio = *ratio;
        let first = *first;
        let second = *second;

        let divider_thickness = 4.0;
        let half = divider_thickness / 2.0;

        let divider_rect = match direction {
            SplitDirection::Vertical => {
                let split_x = rect.min.x + rect.width() * ratio;
                egui::Rect::from_min_size(
                    egui::pos2(split_x - half, rect.min.y),
                    egui::vec2(divider_thickness, rect.height()),
                )
            }
            SplitDirection::Horizontal => {
                let split_y = rect.min.y + rect.height() * ratio;
                egui::Rect::from_min_size(
                    egui::pos2(rect.min.x, split_y - half),
                    egui::vec2(rect.width(), divider_thickness),
                )
            }
        };

        dividers.push(DividerRect {
            rect: divider_rect,
            node_id,
            direction,
            parent_rect: rect,
        });

        let (first_rect, second_rect) = split_rect(rect, direction, ratio);
        collect_dividers_recursive(layout, first, first_rect, dividers);
        collect_dividers_recursive(layout, second, second_rect, dividers);
    }
}

/// Determine which drop zone a point falls in within a group rect.
/// Edge zones are 20% of the dimension; center is the remaining 60%.
pub fn hit_test_drop_zone(group_rect: egui::Rect, pos: egui::Pos2) -> DropZone {
    let rel_x = (pos.x - group_rect.min.x) / group_rect.width();
    let rel_y = (pos.y - group_rect.min.y) / group_rect.height();

    if rel_x < 0.2 {
        DropZone::Left
    } else if rel_x > 0.8 {
        DropZone::Right
    } else if rel_y < 0.2 {
        DropZone::Top
    } else if rel_y > 0.8 {
        DropZone::Bottom
    } else {
        DropZone::Center
    }
}

/// Get the highlight rect for a drop zone overlay.
pub fn drop_zone_highlight_rect(group_rect: egui::Rect, zone: DropZone) -> egui::Rect {
    match zone {
        DropZone::Left => egui::Rect::from_min_max(
            group_rect.min,
            egui::pos2(group_rect.min.x + group_rect.width() * 0.5, group_rect.max.y),
        ),
        DropZone::Right => egui::Rect::from_min_max(
            egui::pos2(group_rect.min.x + group_rect.width() * 0.5, group_rect.min.y),
            group_rect.max,
        ),
        DropZone::Top => egui::Rect::from_min_max(
            group_rect.min,
            egui::pos2(group_rect.max.x, group_rect.min.y + group_rect.height() * 0.5),
        ),
        DropZone::Bottom => egui::Rect::from_min_max(
            egui::pos2(group_rect.min.x, group_rect.min.y + group_rect.height() * 0.5),
            group_rect.max,
        ),
        DropZone::Center | DropZone::TabBar { .. } => group_rect,
    }
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test --lib ui::layout::interactions::tests -- --nocapture`
Expected: all 7 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/ui/layout/interactions.rs
git commit -m "feat: add drop zone hit testing and divider collection for DockLayout"
```

---

## Task 3: Serialization

**Files:**
- Rewrite: `src/ui/layout/serialize.rs`

- [ ] **Step 1: Write serialization tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_group_roundtrip() {
        let layout = DockLayout::new_single(PanelType::Preview);
        let toml_str = serialize_full_layout(&layout).unwrap();
        let restored = deserialize_full_layout(&toml_str).unwrap();
        assert_eq!(restored.groups.len(), 1);
        let panels = restored.collect_all_panels();
        assert_eq!(panels.len(), 1);
        assert_eq!(panels[0].1, PanelType::Preview);
    }

    #[test]
    fn default_layout_roundtrip() {
        let layout = DockLayout::default_layout();
        let toml_str = serialize_full_layout(&layout).unwrap();
        let restored = deserialize_full_layout(&toml_str).unwrap();
        assert_eq!(restored.groups.len(), 3);
        assert_eq!(restored.collect_all_panels().len(), 4);
    }

    #[test]
    fn floating_groups_roundtrip() {
        let mut layout = DockLayout::new_single(PanelType::Preview);
        let entry = TabEntry { panel_id: PanelId::next(), panel_type: PanelType::AudioMixer };
        layout.add_floating_group(entry, egui::pos2(100.0, 200.0));
        let toml_str = serialize_full_layout(&layout).unwrap();
        let restored = deserialize_full_layout(&toml_str).unwrap();
        assert_eq!(restored.floating.len(), 1);
        assert_eq!(restored.groups.len(), 2);
    }

    #[test]
    fn panel_ids_preserved() {
        let layout = DockLayout::new_single(PanelType::Preview);
        let original_id = layout.collect_all_panels()[0].0;
        let toml_str = serialize_full_layout(&layout).unwrap();
        let restored = deserialize_full_layout(&toml_str).unwrap();
        let restored_id = restored.collect_all_panels()[0].0;
        assert_eq!(original_id, restored_id);
    }

    #[test]
    fn invalid_toml_returns_error() {
        assert!(deserialize_full_layout("not valid toml {{{}}}").is_err());
    }

    #[test]
    fn detached_entries_roundtrip() {
        let layout = DockLayout::default_layout();
        let detached = vec![DetachedEntry {
            panel: PanelType::StreamControls,
            id: 99,
            group_id: 50,
            x: 100, y: 100, width: 400, height: 300,
        }];
        let toml_str = serialize_with_detached(&layout, &detached).unwrap();
        let (restored, restored_detached) = deserialize_with_detached(&toml_str).unwrap();
        assert_eq!(restored.groups.len(), 3);
        assert_eq!(restored_detached.len(), 1);
        assert_eq!(restored_detached[0].group_id, 50);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib ui::layout::serialize::tests -- --nocapture 2>&1 | head -20`

- [ ] **Step 3: Write serialization module**

Replace `src/ui/layout/serialize.rs`:

```rust
use std::collections::HashMap;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::tree::{
    DockLayout, FloatingGroup, Group, GroupId, NodeId, PanelId, PanelType, SplitDirection,
    SplitNode, TabEntry,
};

// ---------------------------------------------------------------------------
// Serialized types
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize)]
struct SavedDockLayout {
    tree: SerializedNode,
    groups: Vec<SerializedGroup>,
    #[serde(default)]
    floating: Vec<SerializedFloating>,
    #[serde(default)]
    detached: Vec<DetachedEntry>,
}

#[derive(Serialize, Deserialize)]
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

#[derive(Serialize, Deserialize)]
struct SerializedGroup {
    id: u64,
    tabs: Vec<SerializedTab>,
    active_tab: usize,
}

#[derive(Serialize, Deserialize)]
struct SerializedTab {
    panel_id: u64,
    panel_type: PanelType,
}

#[derive(Serialize, Deserialize)]
struct SerializedFloating {
    group_id: u64,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

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
// Serialize
// ---------------------------------------------------------------------------

fn build_serialized_node(layout: &DockLayout, node_id: NodeId) -> Result<SerializedNode> {
    let node = layout
        .node(node_id)
        .ok_or_else(|| anyhow::anyhow!("node {node_id:?} missing from layout"))?;
    match node {
        SplitNode::Leaf { group_id } => Ok(SerializedNode::Leaf {
            group_id: group_id.0,
        }),
        SplitNode::Split {
            direction,
            ratio,
            first,
            second,
        } => Ok(SerializedNode::Split {
            direction: *direction,
            ratio: *ratio,
            first: Box::new(build_serialized_node(layout, *first)?),
            second: Box::new(build_serialized_node(layout, *second)?),
        }),
    }
}

fn serialize_groups(layout: &DockLayout) -> Vec<SerializedGroup> {
    layout
        .groups
        .iter()
        .map(|(_, group)| SerializedGroup {
            id: group.id.0,
            tabs: group
                .tabs
                .iter()
                .map(|t| SerializedTab {
                    panel_id: t.panel_id.0,
                    panel_type: t.panel_type,
                })
                .collect(),
            active_tab: group.active_tab,
        })
        .collect()
}

fn serialize_floating(layout: &DockLayout) -> Vec<SerializedFloating> {
    layout
        .floating
        .iter()
        .map(|f| SerializedFloating {
            group_id: f.group_id.0,
            x: f.pos.x,
            y: f.pos.y,
            width: f.size.x,
            height: f.size.y,
        })
        .collect()
}

pub fn serialize_full_layout(layout: &DockLayout) -> Result<String> {
    let saved = SavedDockLayout {
        tree: build_serialized_node(layout, layout.root_id())?,
        groups: serialize_groups(layout),
        floating: serialize_floating(layout),
        detached: Vec::new(),
    };
    Ok(toml::to_string_pretty(&saved)?)
}

pub fn serialize_with_detached(
    layout: &DockLayout,
    detached: &[DetachedEntry],
) -> Result<String> {
    let saved = SavedDockLayout {
        tree: build_serialized_node(layout, layout.root_id())?,
        groups: serialize_groups(layout),
        floating: serialize_floating(layout),
        detached: detached.to_vec(),
    };
    Ok(toml::to_string_pretty(&saved)?)
}

// ---------------------------------------------------------------------------
// Deserialize
// ---------------------------------------------------------------------------

struct RebuildState {
    nodes: HashMap<NodeId, SplitNode>,
    next_node_id: u64,
}

impl RebuildState {
    fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            next_node_id: 0,
        }
    }

    fn alloc_node_id(&mut self) -> NodeId {
        let id = NodeId(self.next_node_id);
        self.next_node_id += 1;
        id
    }

    fn insert_serialized(&mut self, snode: SerializedNode) -> NodeId {
        match snode {
            SerializedNode::Leaf { group_id } => {
                let node_id = self.alloc_node_id();
                self.nodes.insert(
                    node_id,
                    SplitNode::Leaf {
                        group_id: GroupId(group_id),
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
                    SplitNode::Split {
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

fn deserialize_groups(
    serialized: Vec<SerializedGroup>,
) -> (HashMap<GroupId, Group>, u64, u64) {
    let mut groups = HashMap::new();
    let mut max_group_id: u64 = 0;
    let mut max_panel_id: u64 = 0;

    for sg in serialized {
        if sg.id > max_group_id {
            max_group_id = sg.id;
        }
        let tabs: Vec<TabEntry> = sg
            .tabs
            .iter()
            .map(|t| {
                if t.panel_id > max_panel_id {
                    max_panel_id = t.panel_id;
                }
                TabEntry {
                    panel_id: PanelId(t.panel_id),
                    panel_type: t.panel_type,
                }
            })
            .collect();
        let group = Group {
            id: GroupId(sg.id),
            active_tab: sg.active_tab.min(tabs.len().saturating_sub(1)),
            tabs,
        };
        groups.insert(group.id, group);
    }

    (groups, max_group_id, max_panel_id)
}

fn deserialize_floating(serialized: Vec<SerializedFloating>) -> Vec<FloatingGroup> {
    serialized
        .into_iter()
        .map(|f| FloatingGroup {
            group_id: GroupId(f.group_id),
            pos: egui::pos2(f.x, f.y),
            size: egui::vec2(f.width, f.height),
        })
        .collect()
}

pub fn deserialize_full_layout(toml_str: &str) -> Result<DockLayout> {
    let saved: SavedDockLayout = toml::from_str(toml_str)?;

    let mut state = RebuildState::new();
    let root_id = state.insert_serialized(saved.tree);

    let (groups, max_group_id, max_panel_id) = deserialize_groups(saved.groups);
    let floating = deserialize_floating(saved.floating);

    // Advance counters to prevent collisions
    PanelId::set_counter(max_panel_id + 1);
    GroupId::set_counter(max_group_id + 1);

    Ok(DockLayout::from_parts(
        state.nodes,
        root_id,
        state.next_node_id,
        groups,
        floating,
    ))
}

pub fn deserialize_with_detached(
    toml_str: &str,
) -> Result<(DockLayout, Vec<DetachedEntry>)> {
    let saved: SavedDockLayout = toml::from_str(toml_str)?;

    let mut state = RebuildState::new();
    let root_id = state.insert_serialized(saved.tree);

    let (groups, max_group_id, max_panel_id) = deserialize_groups(saved.groups);
    let floating = deserialize_floating(saved.floating);

    PanelId::set_counter(max_panel_id + 1);
    GroupId::set_counter(max_group_id + 1);

    Ok((
        DockLayout::from_parts(state.nodes, root_id, state.next_node_id, groups, floating),
        saved.detached,
    ))
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test --lib ui::layout::serialize::tests -- --nocapture`
Expected: all 6 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/ui/layout/serialize.rs
git commit -m "feat: add serialization for DockLayout with groups, tabs, and floating"
```

---

## Task 4: Rendering and Module Re-exports

**Files:**
- Rewrite: `src/ui/layout/render.rs`
- Modify: `src/ui/layout/mod.rs`

This is the largest task. It replaces the entire render module with group-based tab bar rendering, drop zone overlays, ghost drag labels, and divider interaction. The mod.rs re-exports are updated simultaneously so everything compiles together.

- [ ] **Step 1: Write the render module**

Replace `src/ui/layout/render.rs` with the complete implementation:

```rust
use super::interactions::{collect_dividers, drop_zone_highlight_rect, hit_test_drop_zone};
use super::tree::{
    DockLayout, DragState, DropZone, FloatingGroup, Group, GroupId, PanelType, SplitDirection,
    TabEntry,
};

/// Actions returned from rendering, processed by WindowState after the egui frame.
pub enum LayoutAction {
    Resize {
        node_id: super::NodeId,
        new_ratio: f32,
    },
    Close {
        group_id: GroupId,
        tab_index: usize,
    },
    CloseOthers {
        group_id: GroupId,
        tab_index: usize,
    },
    SetActiveTab {
        group_id: GroupId,
        tab_index: usize,
    },
    DetachToFloat {
        group_id: GroupId,
        tab_index: usize,
    },
    DetachToWindow {
        group_id: GroupId,
        tab_index: usize,
    },
    StartDrag {
        group_id: GroupId,
        tab_index: usize,
    },
    DropOnZone {
        target_group: GroupId,
        zone: DropZone,
    },
    DropOnEmpty {
        pos: egui::Pos2,
    },
    CancelDrag,
    AddPanel {
        target_group: GroupId,
        panel_type: PanelType,
    },
    AddPanelAtRoot {
        panel_type: PanelType,
    },
    ResetLayout,
}

/// All dockable panel types for menus.
pub const DOCKABLE_TYPES: &[PanelType] = &[
    PanelType::Preview,
    PanelType::SceneEditor,
    PanelType::AudioMixer,
    PanelType::StreamControls,
];

// ---------------------------------------------------------------------------
// Colors (dark theme from spec)
// ---------------------------------------------------------------------------

const TAB_BAR_BG: egui::Color32 = egui::Color32::from_rgb(0x1e, 0x1e, 0x2e);
const TAB_ACTIVE_BG: egui::Color32 = egui::Color32::from_rgb(0x2a, 0x2a, 0x3e);
const TAB_HOVER_BG: egui::Color32 = egui::Color32::from_rgb(0x2e, 0x2e, 0x3e);
const TAB_ACCENT: egui::Color32 = egui::Color32::from_rgb(0x7c, 0x6c, 0xf0);
const CONTENT_BG: egui::Color32 = egui::Color32::from_rgb(0x18, 0x18, 0x25);
const DROP_ZONE_TINT: egui::Color32 = egui::Color32::from_rgba_premultiplied(0x7c, 0x6c, 0xf0, 0x40);
const TEXT_DIM: egui::Color32 = egui::Color32::from_gray(0xa0);
const TEXT_BRIGHT: egui::Color32 = egui::Color32::from_gray(0xe0);
const DIVIDER_COLOR: egui::Color32 = egui::Color32::from_gray(60);

// Tab bar height
const TAB_BAR_HEIGHT: f32 = 28.0;

// ---------------------------------------------------------------------------
// Menu bar
// ---------------------------------------------------------------------------

pub fn render_menu_bar(
    ctx: &egui::Context,
    _layout: &DockLayout,
) -> (Vec<LayoutAction>, egui::Rect) {
    let mut actions = Vec::new();

    egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("View", |ui| {
                ui.menu_button("Add Panel", |ui| {
                    for &pt in DOCKABLE_TYPES {
                        if ui.button(pt.display_name()).clicked() {
                            actions.push(LayoutAction::AddPanelAtRoot {
                                panel_type: pt,
                            });
                            ui.close();
                        }
                    }
                });
                if ui.button("Reset Layout").clicked() {
                    actions.push(LayoutAction::ResetLayout);
                    ui.close();
                }
            });
        });
    });

    let available_rect = ctx.available_rect();
    (actions, available_rect)
}

// ---------------------------------------------------------------------------
// Main layout rendering
// ---------------------------------------------------------------------------

pub fn render_layout(
    ctx: &egui::Context,
    layout: &DockLayout,
    state: &mut crate::state::AppState,
    available_rect: egui::Rect,
) -> Vec<LayoutAction> {
    let mut actions = Vec::new();

    // Collect grid groups with rects
    let grid_groups = layout.collect_groups_with_rects(available_rect);

    // Render each grid group
    for (group_id, rect) in &grid_groups {
        if let Some(group) = layout.groups.get(group_id) {
            let group_actions = render_group(ctx, group, *rect, state, layout.drag.as_ref(), false);
            actions.extend(group_actions);
        }
    }

    // Render floating groups
    for floating in &layout.floating {
        if let Some(group) = layout.groups.get(&floating.group_id) {
            render_floating_group(ctx, group, floating, state, &mut actions, layout.drag.as_ref());
        }
    }

    // Render dividers
    render_dividers(ctx, layout, available_rect, &mut actions);

    // Render drag ghost and drop zones if dragging
    if let Some(drag) = &layout.drag {
        render_drag_ghost(ctx, drag);
        render_drop_zones(ctx, layout, &grid_groups, &mut actions, available_rect);
    }

    actions
}

// ---------------------------------------------------------------------------
// Group rendering (shared between grid and floating)
// ---------------------------------------------------------------------------

fn render_group(
    ctx: &egui::Context,
    group: &Group,
    rect: egui::Rect,
    state: &mut crate::state::AppState,
    drag: Option<&DragState>,
    is_floating: bool,
) -> Vec<LayoutAction> {
    let mut actions = Vec::new();
    let group_id = group.id;
    let is_dragging = drag.is_some();

    // Only draw via Area for grid groups (floating groups use egui::Window)
    if is_floating {
        return actions;
    }

    egui::Area::new(egui::Id::new(("group", group_id.0)))
        .fixed_pos(rect.min)
        .sense(egui::Sense::hover())
        .show(ctx, |ui| {
            ui.set_min_size(rect.size());
            ui.set_max_size(rect.size());

            // Content background
            ui.painter().rect_filled(rect, 0.0, CONTENT_BG);

            // Tab bar
            let tab_bar_rect = egui::Rect::from_min_size(
                rect.min,
                egui::vec2(rect.width(), TAB_BAR_HEIGHT),
            );
            render_tab_bar(ui, group, tab_bar_rect, &mut actions, is_dragging);

            // Content area (below tab bar)
            let content_rect = egui::Rect::from_min_max(
                egui::pos2(rect.min.x, rect.min.y + TAB_BAR_HEIGHT),
                rect.max,
            );

            // Draw active panel content
            let active = group.active_tab_entry();
            let mut content_ui = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(content_rect),
            );
            crate::ui::draw_panel(active.panel_type, &mut content_ui, state, active.panel_id);
        });

    actions
}

fn render_tab_bar(
    ui: &mut egui::Ui,
    group: &Group,
    tab_bar_rect: egui::Rect,
    actions: &mut Vec<LayoutAction>,
    is_dragging: bool,
) {
    let group_id = group.id;
    let painter = ui.painter();

    // Tab bar background
    painter.rect_filled(tab_bar_rect, 0.0, TAB_BAR_BG);

    // Bottom border
    painter.line_segment(
        [
            egui::pos2(tab_bar_rect.min.x, tab_bar_rect.max.y),
            egui::pos2(tab_bar_rect.max.x, tab_bar_rect.max.y),
        ],
        egui::Stroke::new(1.0, egui::Color32::from_gray(40)),
    );

    // Render each tab
    let tab_count = group.tabs.len();
    let tab_width = if tab_count == 1 {
        tab_bar_rect.width()
    } else {
        (tab_bar_rect.width() / tab_count as f32).min(150.0)
    };

    for (i, tab) in group.tabs.iter().enumerate() {
        let is_active = i == group.active_tab;
        let tab_rect = egui::Rect::from_min_size(
            egui::pos2(tab_bar_rect.min.x + i as f32 * tab_width, tab_bar_rect.min.y),
            egui::vec2(tab_width, TAB_BAR_HEIGHT),
        );

        let tab_id = egui::Id::new(("tab", group_id.0, i));
        let response = ui.interact(tab_rect, tab_id, egui::Sense::click_and_drag());

        // Background
        let bg_color = if is_active {
            TAB_ACTIVE_BG
        } else if response.hovered() {
            TAB_HOVER_BG
        } else {
            TAB_BAR_BG
        };
        painter.rect_filled(tab_rect, 0.0, bg_color);

        // Active tab accent (bottom line)
        if is_active {
            painter.line_segment(
                [
                    egui::pos2(tab_rect.min.x, tab_rect.max.y - 2.0),
                    egui::pos2(tab_rect.max.x, tab_rect.max.y - 2.0),
                ],
                egui::Stroke::new(2.0, TAB_ACCENT),
            );
        }

        // Tab label
        let text_color = if is_active { TEXT_BRIGHT } else { TEXT_DIM };
        let label_rect = tab_rect.shrink2(egui::vec2(8.0, 0.0));
        painter.text(
            egui::pos2(label_rect.min.x, label_rect.center().y),
            egui::Align2::LEFT_CENTER,
            tab.panel_type.display_name(),
            egui::FontId::proportional(12.0),
            text_color,
        );

        // Close button (hover only, if more than 1 tab or more than 1 group)
        if response.hovered() {
            let close_rect = egui::Rect::from_min_size(
                egui::pos2(tab_rect.max.x - 20.0, tab_rect.min.y + 4.0),
                egui::vec2(16.0, 20.0),
            );
            let close_id = egui::Id::new(("tab_close", group_id.0, i));
            let close_response = ui.interact(close_rect, close_id, egui::Sense::click());

            let close_color = if close_response.hovered() {
                egui::Color32::WHITE
            } else {
                TEXT_DIM
            };
            painter.text(
                close_rect.center(),
                egui::Align2::CENTER_CENTER,
                "\u{00d7}",
                egui::FontId::proportional(14.0),
                close_color,
            );

            if close_response.clicked() {
                actions.push(LayoutAction::Close {
                    group_id,
                    tab_index: i,
                });
            }
        }

        // Click to activate
        if response.clicked() {
            actions.push(LayoutAction::SetActiveTab {
                group_id,
                tab_index: i,
            });
        }

        // Drag to start drag
        if response.drag_started() && !is_dragging {
            actions.push(LayoutAction::StartDrag {
                group_id,
                tab_index: i,
            });
        }

        // Context menu
        response.context_menu(|ui| {
            ui.menu_button("Add", |ui| {
                for &pt in DOCKABLE_TYPES {
                    if ui.button(pt.display_name()).clicked() {
                        actions.push(LayoutAction::AddPanel {
                            target_group: group_id,
                            panel_type: pt,
                        });
                        ui.close();
                    }
                }
            });
            if ui.button("Detach").clicked() {
                actions.push(LayoutAction::DetachToFloat {
                    group_id,
                    tab_index: i,
                });
                ui.close();
            }
            if ui.button("Pop Out").clicked() {
                actions.push(LayoutAction::DetachToWindow {
                    group_id,
                    tab_index: i,
                });
                ui.close();
            }
            if tab_count > 1 {
                if ui.button("Close Others").clicked() {
                    actions.push(LayoutAction::CloseOthers {
                        group_id,
                        tab_index: i,
                    });
                    ui.close();
                }
            }
            if ui.button("Close").clicked() {
                actions.push(LayoutAction::Close {
                    group_id,
                    tab_index: i,
                });
                ui.close();
            }
        });
    }
}

// ---------------------------------------------------------------------------
// Floating group rendering
// ---------------------------------------------------------------------------

fn render_floating_group(
    ctx: &egui::Context,
    group: &Group,
    floating: &FloatingGroup,
    state: &mut crate::state::AppState,
    actions: &mut Vec<LayoutAction>,
    drag: Option<&DragState>,
) {
    let group_id = group.id;
    let active_name = group.active_tab_entry().panel_type.display_name();
    let is_dragging = drag.is_some();

    let mut open = true;
    egui::Window::new(active_name)
        .id(egui::Id::new(("floating_group", group_id.0)))
        .default_pos(floating.pos)
        .default_size(floating.size)
        .min_size(egui::vec2(200.0, 150.0))
        .open(&mut open)
        .show(ctx, |ui| {
            // Tab bar within the floating window
            let available = ui.available_rect_before_wrap();
            let tab_bar_rect = egui::Rect::from_min_size(
                available.min,
                egui::vec2(available.width(), TAB_BAR_HEIGHT),
            );
            render_tab_bar(ui, group, tab_bar_rect, actions, is_dragging);
            ui.add_space(TAB_BAR_HEIGHT);

            // Content
            let active = group.active_tab_entry();
            crate::ui::draw_panel(active.panel_type, ui, state, active.panel_id);
        });

    if !open {
        // Close all tabs in the floating group
        for i in (0..group.tabs.len()).rev() {
            actions.push(LayoutAction::Close {
                group_id,
                tab_index: i,
            });
        }
    }
}

// ---------------------------------------------------------------------------
// Divider rendering (same concept as before)
// ---------------------------------------------------------------------------

fn render_dividers(
    ctx: &egui::Context,
    layout: &DockLayout,
    available_rect: egui::Rect,
    actions: &mut Vec<LayoutAction>,
) {
    let dividers = collect_dividers(layout, available_rect);
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("layout_dividers"),
    ));

    for divider in &dividers {
        // Paint 1px line
        match divider.direction {
            SplitDirection::Vertical => {
                let cx = divider.rect.center().x;
                painter.line_segment(
                    [egui::pos2(cx, divider.rect.min.y), egui::pos2(cx, divider.rect.max.y)],
                    egui::Stroke::new(1.0, DIVIDER_COLOR),
                );
            }
            SplitDirection::Horizontal => {
                let cy = divider.rect.center().y;
                painter.line_segment(
                    [egui::pos2(divider.rect.min.x, cy), egui::pos2(divider.rect.max.x, cy)],
                    egui::Stroke::new(1.0, DIVIDER_COLOR),
                );
            }
        }

        // Drag interaction
        let divider_id = egui::Id::new(("divider", divider.node_id.0));
        let hit_rect = divider.rect;

        let area_response = egui::Area::new(divider_id)
            .fixed_pos(hit_rect.min)
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                ui.allocate_rect(
                    egui::Rect::from_min_size(hit_rect.min, hit_rect.size()),
                    egui::Sense::drag(),
                )
            });

        let response = area_response.inner;

        if response.hovered() || response.dragged() {
            match divider.direction {
                SplitDirection::Vertical => ctx.set_cursor_icon(egui::CursorIcon::ResizeHorizontal),
                SplitDirection::Horizontal => ctx.set_cursor_icon(egui::CursorIcon::ResizeVertical),
            }
        }

        if response.dragged()
            && let Some(pointer_pos) = ctx.pointer_interact_pos()
        {
            let new_ratio = match divider.direction {
                SplitDirection::Vertical => {
                    (pointer_pos.x - divider.parent_rect.min.x) / divider.parent_rect.width()
                }
                SplitDirection::Horizontal => {
                    (pointer_pos.y - divider.parent_rect.min.y) / divider.parent_rect.height()
                }
            };
            actions.push(LayoutAction::Resize {
                node_id: divider.node_id,
                new_ratio: new_ratio.clamp(0.1, 0.9),
            });
        }
    }
}

// ---------------------------------------------------------------------------
// Drag ghost
// ---------------------------------------------------------------------------

fn render_drag_ghost(ctx: &egui::Context, drag: &DragState) {
    if let Some(pos) = ctx.pointer_interact_pos() {
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Tooltip,
            egui::Id::new("drag_ghost"),
        ));
        let text = drag.panel_type.display_name();
        let font = egui::FontId::proportional(13.0);
        let galley = painter.layout_no_wrap(text.to_string(), font, egui::Color32::WHITE);
        let text_size = galley.size();
        let rect = egui::Rect::from_min_size(
            egui::pos2(pos.x + 12.0, pos.y + 12.0),
            text_size + egui::vec2(16.0, 8.0),
        );
        painter.rect_filled(rect, 4.0, egui::Color32::from_rgba_premultiplied(0x1e, 0x1e, 0x2e, 0xd0));
        painter.galley(
            egui::pos2(rect.min.x + 8.0, rect.min.y + 4.0),
            galley,
            egui::Color32::WHITE,
        );
    }
}

// ---------------------------------------------------------------------------
// Drop zone overlays
// ---------------------------------------------------------------------------

fn render_drop_zones(
    ctx: &egui::Context,
    layout: &DockLayout,
    grid_groups: &[(GroupId, egui::Rect)],
    actions: &mut Vec<LayoutAction>,
    available_rect: egui::Rect,
) {
    let pointer_pos = match ctx.pointer_interact_pos() {
        Some(p) => p,
        None => return,
    };

    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("drop_zones"),
    ));

    // Check if pointer is over any grid group
    let mut over_group = None;
    for (group_id, rect) in grid_groups {
        if rect.contains(pointer_pos) {
            // Don't show drop zone on the source group if it only has 1 tab
            if let Some(drag) = &layout.drag {
                if *group_id == drag.source_group {
                    if let Some(g) = layout.groups.get(group_id) {
                        if g.tabs.len() <= 1 {
                            continue;
                        }
                    }
                }
            }
            over_group = Some((*group_id, *rect));
            break;
        }
    }

    // Also check floating groups
    if over_group.is_none() {
        for floating in &layout.floating {
            // Floating group rects are managed by egui::Window — approximate
            let approx_rect = egui::Rect::from_min_size(floating.pos, floating.size);
            if approx_rect.contains(pointer_pos) {
                if let Some(drag) = &layout.drag {
                    if floating.group_id == drag.source_group {
                        if let Some(g) = layout.groups.get(&floating.group_id) {
                            if g.tabs.len() <= 1 {
                                continue;
                            }
                        }
                    }
                }
                over_group = Some((floating.group_id, approx_rect));
                break;
            }
        }
    }

    if let Some((target_group, rect)) = over_group {
        let zone = hit_test_drop_zone(rect, pointer_pos);
        let highlight = drop_zone_highlight_rect(rect, zone);
        painter.rect_filled(highlight, 0.0, DROP_ZONE_TINT);

        // Check for drop (mouse released while dragging)
        if ctx.input(|i| i.pointer.any_released()) {
            actions.push(LayoutAction::DropOnZone {
                target_group,
                zone,
            });
        }
    } else if ctx.input(|i| i.pointer.any_released()) {
        // Dropped outside any group → create floating
        if available_rect.contains(pointer_pos) {
            actions.push(LayoutAction::DropOnEmpty { pos: pointer_pos });
        } else {
            actions.push(LayoutAction::CancelDrag);
        }
    }
}
```

- [ ] **Step 2: Update mod.rs re-exports**

Replace `src/ui/layout/mod.rs`:

```rust
pub mod interactions;
pub mod render;
pub mod serialize;
pub mod tree;

pub use serialize::{DetachedEntry, deserialize_full_layout, deserialize_with_detached, serialize_full_layout, serialize_with_detached};
pub use tree::{
    DockLayout, DragState, DropZone, FloatingGroup, Group, GroupId, NodeId, PanelId, PanelType,
    SplitDirection, SplitNode, TabEntry, split_rect,
};
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check 2>&1 | head -40`
Expected: May have errors in window.rs/main.rs that reference old types — that's expected, those are updated in Task 5. The layout module itself should compile cleanly.

- [ ] **Step 4: Commit**

```bash
git add src/ui/layout/render.rs src/ui/layout/mod.rs
git commit -m "feat: add group/tab bar rendering, drop zone overlays, and drag ghost"
```

---

## Task 5: Integration — window.rs and main.rs

**Files:**
- Modify: `src/window.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Update window.rs**

Replace the entire file to use `DockLayout` instead of `LayoutTree`:

```rust
use anyhow::Result;
use egui_wgpu::wgpu;
use egui_wgpu::wgpu::{Surface, SurfaceConfiguration};
use winit::window::Window;

use crate::renderer::SharedGpuState;
use crate::state::AppState;
use crate::ui::layout::{
    DockLayout, DropZone, GroupId, PanelId, PanelType, SplitDirection, TabEntry,
};

pub struct DetachRequest {
    pub panel_type: PanelType,
    pub panel_id: PanelId,
    pub group_id: GroupId,
}

pub struct WindowState {
    pub window: &'static Window,
    pub surface: Surface<'static>,
    pub surface_config: SurfaceConfiguration,
    pub egui_renderer: egui_wgpu::Renderer,
    pub egui_state: egui_winit::State,
    pub egui_ctx: egui::Context,
    pub layout: DockLayout,
    #[allow(dead_code)]
    pub is_main: bool,
}

impl WindowState {
    pub fn new(
        window: &'static Window,
        gpu: &SharedGpuState,
        layout: DockLayout,
        is_main: bool,
    ) -> Result<Self> {
        let surface = gpu.instance.create_surface(window)?;

        let size = window.inner_size();
        let surface_config = SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: gpu.format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&gpu.device, &surface_config);

        let egui_renderer = egui_wgpu::Renderer::new(
            &gpu.device,
            gpu.format,
            egui_wgpu::RendererOptions::default(),
        );

        let egui_ctx = egui::Context::default();
        let max_tex = gpu.device.limits().max_texture_dimension_2d as usize;
        let egui_state = egui_winit::State::new(
            egui_ctx.clone(),
            egui::ViewportId::ROOT,
            window,
            Some(window.scale_factor() as f32),
            None,
            Some(max_tex),
        );

        Ok(Self {
            window,
            surface,
            surface_config,
            egui_renderer,
            egui_state,
            egui_ctx,
            layout,
            is_main,
        })
    }

    pub fn resize(&mut self, gpu: &SharedGpuState, width: u32, height: u32) {
        if width > 0 && height > 0 {
            self.surface_config.width = width;
            self.surface_config.height = height;
            self.surface.configure(&gpu.device, &self.surface_config);
        }
    }

    pub fn render(
        &mut self,
        gpu: &SharedGpuState,
        state: &mut AppState,
    ) -> Result<Vec<DetachRequest>> {
        let raw_input = self.egui_state.take_egui_input(self.window);

        let layout = &self.layout;
        let mut pending_actions = Vec::new();
        let full_output = self.egui_ctx.run(raw_input, |ctx| {
            let (menu_actions, available_rect) =
                crate::ui::layout::render::render_menu_bar(ctx, layout);
            let mut actions = menu_actions;
            actions.extend(crate::ui::layout::render::render_layout(
                ctx,
                layout,
                state,
                available_rect,
            ));
            pending_actions = actions;
        });

        // Apply layout actions after the egui frame
        let mut detach_requests = Vec::new();
        for action in pending_actions {
            use crate::ui::layout::render::LayoutAction;
            match action {
                LayoutAction::Resize { node_id, new_ratio } => {
                    self.layout.resize(node_id, new_ratio);
                }
                LayoutAction::SetActiveTab {
                    group_id,
                    tab_index,
                } => {
                    if let Some(group) = self.layout.groups.get_mut(&group_id) {
                        group.active_tab = tab_index.min(group.tabs.len().saturating_sub(1));
                    }
                }
                LayoutAction::Close {
                    group_id,
                    tab_index,
                } => {
                    self.close_tab(group_id, tab_index);
                }
                LayoutAction::CloseOthers {
                    group_id,
                    tab_index,
                } => {
                    if let Some(group) = self.layout.groups.get_mut(&group_id) {
                        if tab_index < group.tabs.len() {
                            let kept = group.tabs[tab_index].clone();
                            group.tabs = vec![kept];
                            group.active_tab = 0;
                        }
                    }
                }
                LayoutAction::DetachToFloat {
                    group_id,
                    tab_index,
                } => {
                    if let Some(entry) = self.layout.take_tab(group_id, tab_index) {
                        self.layout
                            .add_floating_group(entry, egui::pos2(200.0, 200.0));
                    }
                }
                LayoutAction::DetachToWindow {
                    group_id,
                    tab_index,
                } => {
                    if let Some(entry) = self.layout.take_tab(group_id, tab_index) {
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
                    if let Some(group) = self.layout.groups.get(&group_id) {
                        if let Some(tab) = group.tabs.get(tab_index) {
                            self.layout.drag = Some(crate::ui::layout::DragState {
                                panel_id: tab.panel_id,
                                panel_type: tab.panel_type,
                                source_group: group_id,
                                tab_index,
                            });
                        }
                    }
                }
                LayoutAction::DropOnZone {
                    target_group,
                    zone,
                } => {
                    if let Some(drag) = self.layout.drag.take() {
                        self.handle_drop(drag, target_group, zone);
                    }
                }
                LayoutAction::DropOnEmpty { pos } => {
                    if let Some(drag) = self.layout.drag.take() {
                        if let Some(entry) =
                            self.layout.take_tab(drag.source_group, drag.tab_index)
                        {
                            self.layout.add_floating_group(entry, pos);
                        }
                    }
                }
                LayoutAction::CancelDrag => {
                    self.layout.drag = None;
                }
                LayoutAction::AddPanel {
                    target_group,
                    panel_type,
                } => {
                    if let Some(group) = self.layout.groups.get_mut(&target_group) {
                        group.add_tab(panel_type);
                    }
                }
                LayoutAction::AddPanelAtRoot { panel_type } => {
                    self.layout.insert_at_root(
                        panel_type,
                        PanelId::next(),
                        SplitDirection::Vertical,
                        0.5,
                    );
                }
                LayoutAction::ResetLayout => {
                    self.layout = DockLayout::default_layout();
                }
            }
        }

        let pixels_per_point = full_output.pixels_per_point;
        let paint_jobs = self
            .egui_ctx
            .tessellate(full_output.shapes, pixels_per_point);

        // --- GPU render ---
        let output = self
            .surface
            .get_current_texture()
            .map_err(|e| anyhow::anyhow!("Failed to get surface texture: {e}"))?;
        let view = output.texture.create_view(&Default::default());
        let mut encoder = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("window_render_encoder"),
            });

        let screen_descriptor = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [self.surface_config.width, self.surface_config.height],
            pixels_per_point,
        };

        for (id, image_delta) in &full_output.textures_delta.set {
            self.egui_renderer
                .update_texture(&gpu.device, &gpu.queue, *id, image_delta);
        }

        let user_cmd_bufs = self.egui_renderer.update_buffers(
            &gpu.device,
            &gpu.queue,
            &mut encoder,
            &paint_jobs,
            &screen_descriptor,
        );

        // Pass 1: Clear
        {
            let _clear_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("clear_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.08,
                            g: 0.08,
                            b: 0.10,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
        }

        // Pass 2: Preview texture
        {
            let mut preview_pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("preview_pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                })
                .forget_lifetime();
            gpu.preview_renderer.render(&mut preview_pass);
        }

        // Pass 3: egui overlay
        {
            let mut render_pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("egui_pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                })
                .forget_lifetime();
            self.egui_renderer
                .render(&mut render_pass, &paint_jobs, &screen_descriptor);
            gpu.text_renderer.render()?;
        }

        for id in &full_output.textures_delta.free {
            self.egui_renderer.free_texture(id);
        }

        let mut cmds: Vec<wgpu::CommandBuffer> = user_cmd_bufs;
        cmds.push(encoder.finish());
        gpu.queue.submit(cmds);
        output.present();

        self.egui_state
            .handle_platform_output(self.window, full_output.platform_output);

        Ok(detach_requests)
    }

    /// Close a tab. If it's the last tab in a grid group, remove the group.
    fn close_tab(&mut self, group_id: GroupId, tab_index: usize) {
        let is_floating = self.layout.is_floating(group_id);
        let group = match self.layout.groups.get_mut(&group_id) {
            Some(g) => g,
            None => return,
        };

        if group.tabs.len() > 1 {
            group.remove_tab(tab_index);
        } else if is_floating {
            // Remove the floating group entirely
            self.layout.remove_floating(group_id);
            self.layout.groups.remove(&group_id);
        } else {
            // Last tab in a grid group — remove group from grid
            self.layout.remove_group_from_grid(group_id);
        }
    }

    /// Handle a drop action.
    fn handle_drop(&mut self, drag: crate::ui::layout::DragState, target_group: GroupId, zone: DropZone) {
        // Take the tab from the source
        let entry = match self.layout.take_tab(drag.source_group, drag.tab_index) {
            Some(e) => e,
            None => return,
        };

        match zone {
            DropZone::Center => {
                // Add as new tab
                if let Some(group) = self.layout.groups.get_mut(&target_group) {
                    group.add_tab_entry(entry);
                }
            }
            DropZone::TabBar { index } => {
                // Insert at specific position
                if let Some(group) = self.layout.groups.get_mut(&target_group) {
                    group.insert_tab(index, entry);
                }
            }
            DropZone::Left => {
                self.layout.split_group_with_tab(
                    target_group,
                    SplitDirection::Vertical,
                    entry,
                    true,
                );
            }
            DropZone::Right => {
                self.layout.split_group_with_tab(
                    target_group,
                    SplitDirection::Vertical,
                    entry,
                    false,
                );
            }
            DropZone::Top => {
                self.layout.split_group_with_tab(
                    target_group,
                    SplitDirection::Horizontal,
                    entry,
                    true,
                );
            }
            DropZone::Bottom => {
                self.layout.split_group_with_tab(
                    target_group,
                    SplitDirection::Horizontal,
                    entry,
                    false,
                );
            }
        }
    }
}
```

- [ ] **Step 2: Update main.rs**

Update `src/main.rs` — change imports and `load_layout`/`save_layout`/`reset_layout` to use `DockLayout`:

Key changes:
1. Import `DockLayout` instead of `LayoutTree`
2. `load_layout()` returns `DockLayout`
3. `save_layout()` serializes `DockLayout`
4. `reset_layout()` uses `DockLayout::default_layout()`
5. Detach/reattach uses new group-based APIs
6. `DetachRequest` now carries `GroupId`

```rust
mod mock_driver;
mod obs;
mod renderer;
mod settings;
mod state;
mod ui;
mod window;

use anyhow::Result;
use obs::ObsEngine;
use obs::mock::MockObsEngine;
use renderer::SharedGpuState;
use state::AppState;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use ui::layout::{
    DockLayout, DetachedEntry, GroupId, PanelId, SplitDirection,
    deserialize_full_layout, deserialize_with_detached, serialize_full_layout, serialize_with_detached,
};
use window::{DetachRequest, WindowState};
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalSize, PhysicalSize},
    event::{KeyEvent, WindowEvent},
    event_loop::EventLoop,
    keyboard::{KeyCode, ModifiersState, PhysicalKey},
    window::{Window, WindowAttributes, WindowId},
};

struct AppManager {
    gpu: Option<SharedGpuState>,
    windows: HashMap<WindowId, WindowState>,
    main_window_id: Option<WindowId>,
    state: Arc<Mutex<AppState>>,
    runtime: tokio::runtime::Runtime,
    #[allow(dead_code)]
    engine: MockObsEngine,
    pending_detaches: Vec<DetachRequest>,
    modifiers: ModifiersState,
}

impl AppManager {
    fn new() -> Self {
        let runtime = tokio::runtime::Runtime::new().expect("create tokio runtime");
        let engine = MockObsEngine::new();

        let scenes = engine.scenes();
        let active_scene_id = engine.active_scene_id();
        let initial_state = AppState {
            scenes,
            active_scene_id,
            ..AppState::default()
        };

        Self {
            gpu: None,
            windows: HashMap::new(),
            main_window_id: None,
            state: Arc::new(Mutex::new(initial_state)),
            runtime,
            engine,
            pending_detaches: Vec::new(),
            modifiers: ModifiersState::empty(),
        }
    }

    /// Load saved layout from disk. Falls back to default.
    /// Note: detached window entries are loaded but not restored on startup
    /// (restoring OS windows requires the event loop, handled separately if needed).
    fn load_layout() -> DockLayout {
        let path = settings::config_dir().join("layout.toml");
        if path.exists()
            && let Ok(contents) = std::fs::read_to_string(&path)
        {
            match deserialize_with_detached(&contents) {
                Ok((layout, _detached)) => {
                    log::info!("Loaded layout from {}", path.display());
                    return layout;
                }
                Err(e) => {
                    log::warn!("Failed to parse layout.toml, using default: {e}");
                }
            }
        }
        DockLayout::default_layout()
    }

    fn save_layout(&self) {
        let Some(main_id) = self.main_window_id else {
            return;
        };
        let Some(main_win) = self.windows.get(&main_id) else {
            return;
        };

        let detached: Vec<DetachedEntry> = self
            .windows
            .iter()
            .filter(|(id, _)| **id != main_id)
            .flat_map(|(_, win)| {
                let panels = win.layout.collect_all_panels();
                let pos = win.window.outer_position().unwrap_or_default();
                let size = win.window.inner_size();
                // Each detached window has one group at its root
                let root_group_id = match win.layout.node(win.layout.root_id()) {
                    Some(ui::layout::SplitNode::Leaf { group_id }) => group_id.0,
                    _ => 0,
                };
                panels
                    .into_iter()
                    .map(move |(panel_id, panel_type)| DetachedEntry {
                        panel: panel_type,
                        id: panel_id.0,
                        group_id: root_group_id,
                        x: pos.x,
                        y: pos.y,
                        width: size.width,
                        height: size.height,
                    })
            })
            .collect();

        match serialize_with_detached(&main_win.layout, &detached) {
            Ok(toml_str) => {
                let path = settings::config_dir().join("layout.toml");
                if let Some(parent) = path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                if let Err(e) = std::fs::write(&path, toml_str) {
                    log::warn!("Failed to save layout: {e}");
                }
            }
            Err(e) => {
                log::warn!("Failed to serialize layout: {e}");
            }
        }
    }

    fn reset_layout(&mut self) {
        if let Some(main_id) = self.main_window_id {
            let detached_ids: Vec<WindowId> = self
                .windows
                .keys()
                .filter(|id| **id != main_id)
                .copied()
                .collect();
            for id in detached_ids {
                self.windows.remove(&id);
            }

            if let Some(main_win) = self.windows.get_mut(&main_id) {
                main_win.layout = DockLayout::default_layout();
            }
        }
        self.save_layout();
        log::info!("Layout reset to default");
    }
}

impl ApplicationHandler for AppManager {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        if self.main_window_id.is_some() {
            return;
        }

        let attrs = WindowAttributes::default()
            .with_title("Lodestone")
            .with_inner_size(LogicalSize::new(1280.0, 720.0))
            .with_min_inner_size(LogicalSize::new(960.0, 540.0));
        let window = event_loop.create_window(attrs).expect("create window");
        let window: &'static Window = Box::leak(Box::new(window));
        let window_id = window.id();

        let gpu =
            pollster::block_on(SharedGpuState::new(window)).expect("initialize shared GPU state");

        let layout = Self::load_layout();
        let win_state =
            WindowState::new(window, &gpu, layout, true).expect("create main window state");

        self.gpu = Some(gpu);
        self.main_window_id = Some(window_id);
        self.windows.insert(window_id, win_state);

        self.runtime
            .spawn(mock_driver::run_mock_driver(self.state.clone()));

        log::info!("Window and renderer initialized");
    }

    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        if let Some(win) = self.windows.get_mut(&window_id) {
            let _ = win.egui_state.on_window_event(win.window, &event);
        }

        match &event {
            WindowEvent::ModifiersChanged(mods) => {
                self.modifiers = mods.state();
            }
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        physical_key: PhysicalKey::Code(key_code),
                        state,
                        ..
                    },
                ..
            } if state.is_pressed() => {
                let ctrl = self.modifiers.control_key();
                let shift = self.modifiers.shift_key();
                if ctrl && shift && *key_code == KeyCode::KeyR {
                    self.reset_layout();
                    return;
                }
            }
            _ => {}
        }

        match event {
            WindowEvent::CloseRequested => {
                if Some(window_id) == self.main_window_id {
                    event_loop.exit();
                } else {
                    // Reattach panels from detached window to main
                    if let Some(detached_win) = self.windows.remove(&window_id)
                        && let Some(main_id) = self.main_window_id
                        && let Some(main_win) = self.windows.get_mut(&main_id)
                    {
                        let panels = detached_win.layout.collect_all_panels();
                        for (panel_id, panel_type) in panels {
                            main_win.layout.insert_at_root(
                                panel_type,
                                panel_id,
                                SplitDirection::Vertical,
                                0.5,
                            );
                        }
                        let win_ptr = detached_win.window as *const Window as *mut Window;
                        unsafe {
                            drop(Box::from_raw(win_ptr));
                        }
                    }
                    self.save_layout();
                }
            }
            WindowEvent::Resized(PhysicalSize { width, height }) => {
                if let (Some(gpu), Some(win)) = (&self.gpu, self.windows.get_mut(&window_id)) {
                    win.resize(gpu, width, height);
                }
            }
            WindowEvent::RedrawRequested => {
                if let Some(gpu) = &self.gpu
                    && let Some(win) = self.windows.get_mut(&window_id)
                {
                    let mut app_state = self.state.lock().unwrap();
                    let layout_changed = match win.render(gpu, &mut app_state) {
                        Ok(detach_requests) => {
                            let changed = !detach_requests.is_empty();
                            self.pending_detaches.extend(detach_requests);
                            changed
                        }
                        Err(e) => {
                            log::error!("Render error: {e}");
                            false
                        }
                    };
                    drop(app_state);
                    if layout_changed {
                        self.save_layout();
                    }
                }
                if let Some(win) = self.windows.get(&window_id) {
                    win.window.request_redraw();
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        if let Some(gpu) = &self.gpu {
            for detach in self.pending_detaches.drain(..) {
                let attrs = WindowAttributes::default()
                    .with_title(detach.panel_type.display_name())
                    .with_inner_size(LogicalSize::new(400.0, 300.0));
                let window = event_loop
                    .create_window(attrs)
                    .expect("create detached window");
                let window: &'static Window = Box::leak(Box::new(window));

                let layout = DockLayout::new_with_ids(
                    detach.group_id,
                    detach.panel_id,
                    detach.panel_type,
                );
                let win_state =
                    WindowState::new(window, gpu, layout, false).expect("init detached window");
                self.windows.insert(window.id(), win_state);
            }
        }

        for win in self.windows.values() {
            win.window.request_redraw();
        }
    }
}

fn main() -> Result<()> {
    env_logger::init();
    log::info!("Lodestone starting");
    let event_loop = EventLoop::new()?;
    let mut app = AppManager::new();
    event_loop.run_app(&mut app)?;
    Ok(())
}
```

- [ ] **Step 3: Verify full compilation**

Run: `cargo check 2>&1`
Expected: clean compilation, zero errors.

- [ ] **Step 4: Run full test suite**

Run: `cargo test 2>&1`
Expected: all tests pass.

- [ ] **Step 5: Run clippy**

Run: `cargo clippy 2>&1`
Expected: zero warnings.

- [ ] **Step 6: Commit**

```bash
git add src/window.rs src/main.rs
git commit -m "feat: integrate DockLayout into window rendering and app management"
```

---

## Task 6: Cleanup and Final Verification

**Files:**
- All modified files

- [ ] **Step 1: Delete saved layout to force default**

The old serialized layout format is incompatible. Delete `~/.config/lodestone/layout.toml` if it exists so the app starts with the new default layout.

```bash
rm -f ~/.config/lodestone/layout.toml
```

- [ ] **Step 2: Run full test suite**

Run: `cargo test 2>&1`
Expected: all tests pass.

- [ ] **Step 3: Run clippy**

Run: `cargo clippy 2>&1`
Expected: zero warnings.

- [ ] **Step 4: Run formatting check**

Run: `cargo fmt --check 2>&1`
Expected: no formatting issues (or run `cargo fmt` to fix).

- [ ] **Step 5: Manual smoke test**

Run: `cargo run`
Verify:
- App launches with default layout (3 groups, 4 panels)
- Tab bar visible on each group with panel names
- AudioMixer and StreamControls share a group as tabs
- Clicking tabs switches active panel
- Right-click tab shows context menu with Add, Detach, Pop Out, Close
- Dragging a tab shows ghost label
- Dropping on a group zone splits correctly
- Dropping in center adds as tab
- Dropping outside creates floating group
- Floating groups render as windows with tab bars
- Divider dragging works
- View → Add Panel creates new group at root
- View → Reset Layout restores default
- Ctrl+Shift+R resets layout
- Closing app and reopening restores layout

- [ ] **Step 6: Commit final state**

```bash
git add -A
git commit -m "feat: complete dockview-style layout system with tabs, drag-to-dock, and floating groups"
```
