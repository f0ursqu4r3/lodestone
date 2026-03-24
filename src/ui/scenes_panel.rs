//! Scenes panel — displays a 2-column thumbnail grid of scenes.
//!
//! Each scene is shown as a 16:9 thumbnail with a label beneath it.
//! The active scene is highlighted with a `TEXT_PRIMARY` border.
//! An "Add" card with a dashed border creates new scenes.

use crate::gstreamer::{CaptureSourceConfig, GstCommand};
use crate::scene::{Scene, SceneId, SourceId};
use crate::state::AppState;
use crate::ui::layout::tree::PanelId;
use crate::ui::theme::{BG_ELEVATED, BORDER, RADIUS_SM, TEXT_MUTED, TEXT_PRIMARY, TEXT_SECONDARY};
use egui::{CornerRadius, Pos2, Rect, Sense, Stroke, vec2};

/// Draw the scenes panel — a 2-column grid of scene thumbnails.
pub fn draw(ui: &mut egui::Ui, state: &mut AppState, _id: PanelId) {
    let cmd_tx = state.command_tx.clone();

    // Snapshot scene data to avoid borrow conflicts during iteration.
    let scenes: Vec<(SceneId, String)> = state
        .scenes
        .iter()
        .map(|s| (s.id, s.name.clone()))
        .collect();
    let active_id = state.active_scene_id;

    let available_width = ui.available_width();
    let spacing = 6.0;
    let padding = 4.0;
    let col_width = ((available_width - spacing - padding * 2.0) / 2.0).max(40.0);
    let thumb_height = col_width * 9.0 / 16.0;
    let label_height = 14.0;
    let cell_height = thumb_height + label_height + 4.0; // 4px gap between thumb and label

    let mut switch_to: Option<SceneId> = None;
    let mut delete_scene: Option<SceneId> = None;

    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.add_space(padding);

        // Iterate scenes + the "Add" card at the end.
        let total_cells = scenes.len() + 1;
        let rows = total_cells.div_ceil(2);

        for row in 0..rows {
            ui.horizontal(|ui| {
                ui.add_space(padding);

                for col in 0..2 {
                    let cell_idx = row * 2 + col;
                    if cell_idx >= total_cells {
                        break;
                    }

                    if col > 0 {
                        ui.add_space(spacing);
                    }

                    let (rect, response) =
                        ui.allocate_exact_size(vec2(col_width, cell_height), Sense::click());

                    let thumb_rect = Rect::from_min_size(rect.min, vec2(col_width, thumb_height));
                    let label_pos = Pos2::new(
                        rect.min.x + col_width / 2.0,
                        thumb_rect.max.y + 2.0 + label_height / 2.0,
                    );

                    let painter = ui.painter_at(rect);

                    if cell_idx < scenes.len() {
                        // ── Scene thumbnail ──
                        let (scene_id, scene_name) = &scenes[cell_idx];
                        let is_active = active_id == Some(*scene_id);
                        let is_hovered = response.hovered();

                        // Thumbnail background.
                        painter.rect_filled(
                            thumb_rect,
                            CornerRadius::same(RADIUS_SM as u8),
                            BG_ELEVATED,
                        );

                        // Border: active = TEXT_PRIMARY, hovered = TEXT_MUTED, default = BORDER.
                        let border_color = if is_active {
                            TEXT_PRIMARY
                        } else if is_hovered {
                            TEXT_MUTED
                        } else {
                            BORDER
                        };
                        painter.rect_stroke(
                            thumb_rect,
                            CornerRadius::same(RADIUS_SM as u8),
                            Stroke::new(1.0, border_color),
                            egui::StrokeKind::Outside,
                        );

                        // Label below thumbnail.
                        let label_color = if is_active {
                            TEXT_PRIMARY
                        } else {
                            TEXT_SECONDARY
                        };
                        painter.text(
                            label_pos,
                            egui::Align2::CENTER_CENTER,
                            scene_name,
                            egui::FontId::proportional(9.0),
                            label_color,
                        );

                        // Click to switch active scene.
                        if response.clicked() && !is_active {
                            switch_to = Some(*scene_id);
                        }

                        // Context menu for delete.
                        response.context_menu(|ui| {
                            if ui.button("Delete").clicked() {
                                delete_scene = Some(*scene_id);
                                ui.close();
                            }
                        });
                    } else {
                        // ── "Add Scene" card ──
                        draw_add_card(&painter, thumb_rect, label_pos, response.hovered());

                        if response.clicked() {
                            let new_id = SceneId(state.next_scene_id);
                            state.next_scene_id += 1;
                            state.scenes.push(Scene {
                                id: new_id,
                                name: format!("Scene {}", state.scenes.len() + 1),
                                sources: Vec::new(),
                            });
                            state.active_scene_id = Some(new_id);
                            state.scenes_dirty = true;
                            state.scenes_last_changed = std::time::Instant::now();
                        }
                    }
                }
            });

            ui.add_space(spacing);
        }
    });

    // ── Apply scene switch ──
    if let Some(new_id) = switch_to {
        let old_scene = state
            .active_scene_id
            .and_then(|id| state.scenes.iter().find(|s| s.id == id))
            .cloned();
        let new_scene = state.scenes.iter().find(|s| s.id == new_id).cloned();

        state.active_scene_id = Some(new_id);
        state.selected_source_id = None;

        apply_scene_diff(
            &cmd_tx,
            &state.sources,
            old_scene.as_ref(),
            new_scene.as_ref(),
        );

        if let Some(ref scene) = new_scene {
            state.capture_active = !scene.sources.is_empty();
        }
        state.scenes_dirty = true;
        state.scenes_last_changed = std::time::Instant::now();
    }

    // ── Apply scene delete ──
    if let Some(del_id) = delete_scene {
        delete_scene_by_id(state, &cmd_tx, del_id);
    }
}

/// Draw the dashed-border "Add" card with a "+" icon and "Add" label.
fn draw_add_card(painter: &egui::Painter, thumb_rect: Rect, label_pos: Pos2, hovered: bool) {
    let border_color = if hovered { TEXT_MUTED } else { BORDER };

    // Draw dashed border as short segments along the rectangle edges.
    let dash_len = 4.0;
    let gap_len = 3.0;
    let stroke = Stroke::new(1.0, border_color);
    let corners = [
        thumb_rect.left_top(),
        thumb_rect.right_top(),
        thumb_rect.right_bottom(),
        thumb_rect.left_bottom(),
    ];
    for i in 0..4 {
        let start = corners[i];
        let end = corners[(i + 1) % 4];
        let dir = (end - start).normalized();
        let total = start.distance(end);
        let mut d = 0.0;
        while d < total {
            let seg_end = (d + dash_len).min(total);
            painter.line_segment([start + dir * d, start + dir * seg_end], stroke);
            d = seg_end + gap_len;
        }
    }

    // "+" icon in center of thumbnail.
    let icon_color = if hovered { TEXT_MUTED } else { BORDER };
    painter.text(
        thumb_rect.center(),
        egui::Align2::CENTER_CENTER,
        egui_phosphor::regular::PLUS,
        egui::FontId::proportional(18.0),
        icon_color,
    );

    // "Add" label.
    painter.text(
        label_pos,
        egui::Align2::CENTER_CENTER,
        "Add",
        egui::FontId::proportional(9.0),
        TEXT_MUTED,
    );
}

/// Send `AddCaptureSource` / `RemoveCaptureSource` commands for the delta between two scenes.
fn apply_scene_diff(
    cmd_tx: &Option<tokio::sync::mpsc::Sender<GstCommand>>,
    sources: &[crate::scene::LibrarySource],
    old_scene: Option<&Scene>,
    new_scene: Option<&Scene>,
) {
    let Some(tx) = cmd_tx else { return };

    let old_ids: std::collections::HashSet<SourceId> = old_scene
        .map(|s| s.source_ids().into_iter().collect())
        .unwrap_or_default();
    let new_ids: std::collections::HashSet<SourceId> = new_scene
        .map(|s| s.source_ids().into_iter().collect())
        .unwrap_or_default();

    for &src_id in old_ids.difference(&new_ids) {
        let _ = tx.try_send(GstCommand::RemoveCaptureSource { source_id: src_id });
    }

    for &src_id in new_ids.difference(&old_ids) {
        if let Some(source) = sources.iter().find(|s| s.id == src_id) {
            match &source.properties {
                crate::scene::SourceProperties::Display { screen_index } => {
                    let _ = tx.try_send(GstCommand::AddCaptureSource {
                        source_id: src_id,
                        config: CaptureSourceConfig::Screen {
                            screen_index: *screen_index,
                        },
                    });
                }
                crate::scene::SourceProperties::Window { window_id, .. } => {
                    let _ = tx.try_send(GstCommand::AddCaptureSource {
                        source_id: src_id,
                        config: CaptureSourceConfig::Window {
                            window_id: *window_id,
                        },
                    });
                }
                crate::scene::SourceProperties::Camera { device_index, .. } => {
                    let _ = tx.try_send(GstCommand::AddCaptureSource {
                        source_id: src_id,
                        config: CaptureSourceConfig::Camera {
                            device_index: *device_index,
                        },
                    });
                }
                crate::scene::SourceProperties::Image { .. } => {
                    // Image sources don't use a capture pipeline.
                }
            }
        }
    }
}

/// Delete a scene by ID, cleaning up its sources and selecting a fallback.
fn delete_scene_by_id(
    state: &mut AppState,
    cmd_tx: &Option<tokio::sync::mpsc::Sender<GstCommand>>,
    scene_id: SceneId,
) {
    // If this is the last scene, create a new default first.
    if state.scenes.len() <= 1 {
        let new_id = SceneId(state.next_scene_id);
        state.next_scene_id += 1;
        state.scenes.push(Scene {
            id: new_id,
            name: "Scene 1".to_string(),
            sources: Vec::new(),
        });
    }

    // Remove sources belonging to the deleted scene.
    if let Some(scene) = state.scenes.iter().find(|s| s.id == scene_id) {
        let src_ids: Vec<SourceId> = scene.source_ids();
        for &src_id in &src_ids {
            if let Some(tx) = cmd_tx {
                let _ = tx.try_send(GstCommand::RemoveCaptureSource { source_id: src_id });
            }
        }
        state.sources.retain(|s| !src_ids.contains(&s.id));
    }

    // Remove the scene itself.
    state.scenes.retain(|s| s.id != scene_id);

    // Select the first remaining scene, clear source selection.
    state.selected_source_id = None;
    let first_scene = state.scenes.first().cloned();
    if let Some(ref scene) = first_scene {
        state.active_scene_id = Some(scene.id);
        send_capture_for_scene(cmd_tx, &state.sources, scene);
        state.capture_active = !scene.sources.is_empty();
    } else {
        state.active_scene_id = None;
        state.capture_active = false;
    }

    state.scenes_dirty = true;
    state.scenes_last_changed = std::time::Instant::now();
}

/// Start capture for all sources in a scene, or `StopCapture` if it has none.
fn send_capture_for_scene(
    cmd_tx: &Option<tokio::sync::mpsc::Sender<GstCommand>>,
    sources: &[crate::scene::LibrarySource],
    scene: &Scene,
) {
    let Some(tx) = cmd_tx else { return };
    let mut any_started = false;
    for src_id in scene.source_ids() {
        if let Some(source) = sources.iter().find(|s| s.id == src_id) {
            match &source.properties {
                crate::scene::SourceProperties::Display { screen_index } => {
                    let _ = tx.try_send(GstCommand::AddCaptureSource {
                        source_id: src_id,
                        config: CaptureSourceConfig::Screen {
                            screen_index: *screen_index,
                        },
                    });
                    any_started = true;
                }
                crate::scene::SourceProperties::Window { window_id, .. } => {
                    let _ = tx.try_send(GstCommand::AddCaptureSource {
                        source_id: src_id,
                        config: CaptureSourceConfig::Window {
                            window_id: *window_id,
                        },
                    });
                    any_started = true;
                }
                crate::scene::SourceProperties::Camera { device_index, .. } => {
                    let _ = tx.try_send(GstCommand::AddCaptureSource {
                        source_id: src_id,
                        config: CaptureSourceConfig::Camera {
                            device_index: *device_index,
                        },
                    });
                    any_started = true;
                }
                crate::scene::SourceProperties::Image { .. } => {
                    // Image sources don't use a capture pipeline.
                }
            }
        }
    }
    if !any_started {
        let _ = tx.try_send(GstCommand::StopCapture);
    }
}
