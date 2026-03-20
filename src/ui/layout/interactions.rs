use super::tree::{LayoutNode, LayoutTree, NodeId, SplitDirection};

/// A rectangle representing a draggable divider between two split children.
pub struct DividerRect {
    /// The hit-test rectangle (4px wide/tall strip).
    pub rect: egui::Rect,
    /// The node ID of the split node that owns this divider.
    pub node_id: NodeId,
    /// The split direction (determines cursor and drag axis).
    pub direction: SplitDirection,
    /// The full rect of the parent split node (used to compute new ratio on drag).
    pub parent_rect: egui::Rect,
}

impl DividerRect {
    pub fn contains(&self, pos: egui::Pos2) -> bool {
        self.rect.contains(pos)
    }
}

/// Walk the layout tree and collect divider rects for all split nodes.
pub fn collect_dividers(tree: &LayoutTree, total_rect: egui::Rect) -> Vec<DividerRect> {
    let mut dividers = Vec::new();
    collect_dividers_recursive(tree, tree.root_id(), total_rect, &mut dividers);
    dividers
}

fn collect_dividers_recursive(
    tree: &LayoutTree,
    node_id: NodeId,
    rect: egui::Rect,
    dividers: &mut Vec<DividerRect>,
) {
    let node = match tree.node(node_id) {
        Some(n) => n,
        None => return,
    };

    if let LayoutNode::Split {
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

        // Recurse into children with their sub-rects.
        let (first_rect, second_rect) = split_rect(rect, direction, ratio);
        collect_dividers_recursive(tree, first, first_rect, dividers);
        collect_dividers_recursive(tree, second, second_rect, dividers);
    }
    // Leaf nodes have no dividers.
}

fn split_rect(
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn point_on_vertical_divider() {
        let divider = DividerRect {
            rect: egui::Rect::from_min_size(egui::pos2(300.0, 0.0), egui::vec2(4.0, 600.0)),
            node_id: NodeId(1),
            direction: SplitDirection::Vertical,
            parent_rect: egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1000.0, 600.0)),
        };
        assert!(divider.contains(egui::pos2(302.0, 300.0)));
        assert!(!divider.contains(egui::pos2(100.0, 300.0)));
    }

    #[test]
    fn point_on_horizontal_divider() {
        let divider = DividerRect {
            rect: egui::Rect::from_min_size(egui::pos2(0.0, 450.0), egui::vec2(1000.0, 4.0)),
            node_id: NodeId(2),
            direction: SplitDirection::Horizontal,
            parent_rect: egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1000.0, 600.0)),
        };
        assert!(divider.contains(egui::pos2(500.0, 452.0)));
        assert!(!divider.contains(egui::pos2(500.0, 100.0)));
    }
}
