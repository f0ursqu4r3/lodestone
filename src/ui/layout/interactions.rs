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

/// Tab bar height used for drop zone detection.
const DROP_TAB_BAR_HEIGHT: f32 = 28.0;
/// Add button width reserved in the tab bar.
const DROP_ADD_BUTTON_WIDTH: f32 = 28.0;
/// Dock grip width reserved in the tab bar.
const DROP_DOCK_GRIP_WIDTH: f32 = 28.0;
/// Maximum individual tab width.
const DROP_MAX_TAB_WIDTH: f32 = 160.0;

/// Determine which drop zone a point falls in within a group rect.
/// The group rect includes the tab bar at the top. Dropping on the tab bar
/// adds as a tab at a computed index. Edge zones (20%) apply to the content
/// area below the tab bar.
pub fn hit_test_drop_zone(group_rect: egui::Rect, pos: egui::Pos2, tab_count: usize) -> DropZone {
    // Check if pointer is in the tab bar area
    if pos.y < group_rect.min.y + DROP_TAB_BAR_HEIGHT {
        let available = group_rect.width() - DROP_ADD_BUTTON_WIDTH - DROP_DOCK_GRIP_WIDTH;
        let tab_width = if tab_count > 0 {
            (available / tab_count as f32).min(DROP_MAX_TAB_WIDTH)
        } else {
            DROP_MAX_TAB_WIDTH
        };
        let rel_x = pos.x - group_rect.min.x;
        // Compute insertion index: which gap between tabs the pointer is closest to
        let index = ((rel_x + tab_width * 0.5) / tab_width).floor().max(0.0) as usize;
        let index = index.min(tab_count);
        return DropZone::TabBar { index };
    }

    // Content area is below the tab bar
    let content_top = group_rect.min.y + DROP_TAB_BAR_HEIGHT;
    let content_height = group_rect.max.y - content_top;
    if content_height <= 0.0 {
        return DropZone::Center;
    }

    let rel_x = (pos.x - group_rect.min.x) / group_rect.width();
    let rel_y = (pos.y - content_top) / content_height;

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
/// Edge zones are constrained to the content area (below the tab bar).
/// TabBar zone renders a vertical insertion line at the computed index.
pub fn drop_zone_highlight_rect(
    group_rect: egui::Rect,
    zone: DropZone,
    tab_count: usize,
) -> egui::Rect {
    let content_top = group_rect.min.y + DROP_TAB_BAR_HEIGHT;
    let content_rect =
        egui::Rect::from_min_max(egui::pos2(group_rect.min.x, content_top), group_rect.max);

    match zone {
        DropZone::TabBar { index } => {
            // Vertical insertion line between tabs
            let available = group_rect.width() - DROP_ADD_BUTTON_WIDTH - DROP_DOCK_GRIP_WIDTH;
            let tab_width = if tab_count > 0 {
                (available / tab_count as f32).min(DROP_MAX_TAB_WIDTH)
            } else {
                DROP_MAX_TAB_WIDTH
            };
            let line_x = group_rect.min.x + index as f32 * tab_width;
            let line_width = 2.0;
            egui::Rect::from_min_size(
                egui::pos2(line_x - line_width * 0.5, group_rect.min.y),
                egui::vec2(line_width, DROP_TAB_BAR_HEIGHT),
            )
        }
        DropZone::Center => group_rect,
        DropZone::Left => egui::Rect::from_min_max(
            content_rect.min,
            egui::pos2(
                content_rect.min.x + content_rect.width() * 0.5,
                content_rect.max.y,
            ),
        ),
        DropZone::Right => egui::Rect::from_min_max(
            egui::pos2(
                content_rect.min.x + content_rect.width() * 0.5,
                content_rect.min.y,
            ),
            content_rect.max,
        ),
        DropZone::Top => egui::Rect::from_min_max(
            content_rect.min,
            egui::pos2(
                content_rect.max.x,
                content_rect.min.y + content_rect.height() * 0.5,
            ),
        ),
        DropZone::Bottom => egui::Rect::from_min_max(
            egui::pos2(
                content_rect.min.x,
                content_rect.min.y + content_rect.height() * 0.5,
            ),
            content_rect.max,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::layout::tree::{DockLayout, PanelType};

    #[test]
    fn hit_test_center() {
        let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(500.0, 400.0));
        assert_eq!(
            hit_test_drop_zone(rect, egui::pos2(250.0, 220.0), 2),
            DropZone::Center
        );
    }

    #[test]
    fn hit_test_left() {
        let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(500.0, 400.0));
        assert_eq!(
            hit_test_drop_zone(rect, egui::pos2(30.0, 220.0), 2),
            DropZone::Left
        );
    }

    #[test]
    fn hit_test_right() {
        let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(500.0, 400.0));
        assert_eq!(
            hit_test_drop_zone(rect, egui::pos2(480.0, 220.0), 2),
            DropZone::Right
        );
    }

    #[test]
    fn hit_test_top() {
        // Top zone is in the content area (below tab bar), not the tab bar itself
        let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(500.0, 400.0));
        assert_eq!(
            hit_test_drop_zone(rect, egui::pos2(250.0, 40.0), 2),
            DropZone::Top
        );
    }

    #[test]
    fn hit_test_bottom() {
        let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(500.0, 400.0));
        assert_eq!(
            hit_test_drop_zone(rect, egui::pos2(250.0, 380.0), 2),
            DropZone::Bottom
        );
    }

    #[test]
    fn hit_test_tab_bar_first() {
        // Pointer near the left edge of tab bar → index 0
        let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(500.0, 400.0));
        assert_eq!(
            hit_test_drop_zone(rect, egui::pos2(10.0, 14.0), 2),
            DropZone::TabBar { index: 0 }
        );
    }

    #[test]
    fn hit_test_tab_bar_between() {
        // With 2 tabs in a 500px rect: available = 500-28-28 = 444, tab_width = 160 (capped)
        // Pointer at x=170 → between tab 0 and tab 1 → index 1
        let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(500.0, 400.0));
        assert_eq!(
            hit_test_drop_zone(rect, egui::pos2(170.0, 14.0), 2),
            DropZone::TabBar { index: 1 }
        );
    }

    #[test]
    fn hit_test_tab_bar_end() {
        // Pointer past all tabs → clamped to tab_count
        let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(500.0, 400.0));
        assert_eq!(
            hit_test_drop_zone(rect, egui::pos2(400.0, 14.0), 2),
            DropZone::TabBar { index: 2 }
        );
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
