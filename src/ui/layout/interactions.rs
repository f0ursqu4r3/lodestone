use super::tree::{DockLayout, DropZone, NodeId, SplitDirection, SplitNode, split_rect};

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::layout::tree::{DockLayout, PanelType};

    #[test]
    fn hit_test_center() {
        let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(500.0, 400.0));
        assert_eq!(hit_test_drop_zone(rect, egui::pos2(250.0, 200.0)), DropZone::Center);
    }

    #[test]
    fn hit_test_left() {
        let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(500.0, 400.0));
        assert_eq!(hit_test_drop_zone(rect, egui::pos2(30.0, 200.0)), DropZone::Left);
    }

    #[test]
    fn hit_test_right() {
        let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(500.0, 400.0));
        assert_eq!(hit_test_drop_zone(rect, egui::pos2(480.0, 200.0)), DropZone::Right);
    }

    #[test]
    fn hit_test_top() {
        let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(500.0, 400.0));
        assert_eq!(hit_test_drop_zone(rect, egui::pos2(250.0, 30.0)), DropZone::Top);
    }

    #[test]
    fn hit_test_bottom() {
        let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(500.0, 400.0));
        assert_eq!(hit_test_drop_zone(rect, egui::pos2(250.0, 380.0)), DropZone::Bottom);
    }

    #[test]
    fn divider_collection_single_group() {
        let layout = DockLayout::new_single(PanelType::Preview);
        let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1000.0, 600.0));
        assert_eq!(collect_dividers(&layout, rect).len(), 0);
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
