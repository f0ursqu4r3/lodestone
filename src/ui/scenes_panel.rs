//! Scenes panel — displays a 2-column thumbnail grid of scenes.
//!
//! Each scene is shown as a 16:9 thumbnail with a label beneath it.
//! The active scene is highlighted with a `TEXT_PRIMARY` border.
//! An "Add" card with a dashed border creates new scenes.

use crate::gstreamer::{CaptureSourceConfig, GstCommand};
use crate::scene::{Scene, SceneId, SourceId};
use crate::state::AppState;
use crate::ui::layout::tree::PanelId;
use crate::ui::theme::active_theme;
use egui::{CornerRadius, Pos2, Rect, Sense, Stroke, vec2};

/// A deferred action produced by a scene card interaction.
enum SceneAction {
    /// Switch the active scene to the given ID.
    Switch(SceneId),
    /// Delete the scene with the given ID.
    Delete(SceneId),
}

/// Draw the scenes panel — a 2-column grid of scene thumbnails.
pub fn draw(ui: &mut egui::Ui, state: &mut AppState, _id: PanelId) {
    let theme = active_theme(ui.ctx());
    let cmd_tx = state.command_tx.clone();

    // Snapshot scene data to avoid borrow conflicts during iteration.
    let scenes: Vec<(SceneId, String, bool)> = state
        .scenes
        .iter()
        .map(|s| (s.id, s.name.clone(), s.pinned))
        .collect();
    let active_id = state.active_scene_id;

    let available_width = ui.available_width();
    let spacing = 6.0;
    let padding = 4.0;
    let col_width = ((available_width - spacing - padding * 2.0) / 2.0).max(40.0);
    let thumb_height = col_width * 9.0 / 16.0;
    let label_height = 14.0;
    let cell_height = thumb_height + label_height + 4.0; // 4px gap between thumb and label

    let mut pending_action: Option<SceneAction> = None;

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

                    if cell_idx < scenes.len() {
                        let (scene_id, scene_name, is_pinned) = &scenes[cell_idx];
                        let is_active = active_id == Some(*scene_id);

                        if let Some(action) = draw_scene_card(
                            ui,
                            state,
                            *scene_id,
                            scene_name,
                            *is_pinned,
                            thumb_rect,
                            label_pos,
                            label_height,
                            col_width,
                            response,
                            is_active,
                            &theme,
                        ) {
                            pending_action = Some(action);
                        }
                    } else {
                        // ── "Add Scene" card ──
                        let painter = ui.painter_at(rect);
                        draw_add_card(&painter, thumb_rect, label_pos, response.hovered(), &theme);

                        if response.clicked() {
                            let new_id = SceneId(state.next_scene_id);
                            state.next_scene_id += 1;
                            state.scenes.push(Scene {
                                id: new_id,
                                name: format!("Scene {}", state.scenes.len() + 1),
                                sources: Vec::new(),
                                pinned: false,
                            });
                            state.active_scene_id = Some(new_id);
                            state.mark_dirty();
                        }
                    }
                }
            });

            ui.add_space(spacing);
        }
    });

    // ── Apply deferred action ──
    match pending_action {
        Some(SceneAction::Switch(new_id)) => {
            let old_scene = state
                .active_scene_id
                .and_then(|id| state.scenes.iter().find(|s| s.id == id))
                .cloned();
            let new_scene = state.scenes.iter().find(|s| s.id == new_id).cloned();

            state.active_scene_id = Some(new_id);
            state.deselect_all();

            apply_scene_diff(
                &cmd_tx,
                &state.library,
                old_scene.as_ref(),
                new_scene.as_ref(),
                state.settings.general.exclude_self_from_capture,
            );

            if let Some(ref scene) = new_scene {
                state.capture_active = !scene.sources.is_empty();
            }
            state.mark_dirty();
        }
        Some(SceneAction::Delete(del_id)) => {
            delete_scene_by_id(state, &cmd_tx, del_id);
        }
        None => {}
    }
}

/// Draw a single scene card in the grid.
/// Returns an optional deferred action (switch scene, delete, rename, etc.).
#[allow(clippy::too_many_arguments)]
fn draw_scene_card(
    ui: &mut egui::Ui,
    state: &mut AppState,
    scene_id: SceneId,
    scene_name: &str,
    is_pinned: bool,
    thumb_rect: Rect,
    label_pos: Pos2,
    label_height: f32,
    col_width: f32,
    response: egui::Response,
    is_active: bool,
    theme: &crate::ui::theme::Theme,
) -> Option<SceneAction> {
    let is_hovered = response.hovered();
    let painter = ui.painter_at(thumb_rect.expand2(egui::vec2(0.0, label_height + 4.0)));

    // Thumbnail background.
    painter.rect_filled(
        thumb_rect,
        CornerRadius::same(theme.radius_sm as u8),
        theme.bg_elevated,
    );

    // Border: active = text_primary, hovered = text_muted, default = border.
    let border_color = if is_active {
        theme.text_primary
    } else if is_hovered {
        theme.text_muted
    } else {
        theme.border
    };
    painter.rect_stroke(
        thumb_rect,
        CornerRadius::same(theme.radius_sm as u8),
        Stroke::new(1.0, border_color),
        egui::StrokeKind::Outside,
    );

    // Draw miniature source rectangles inside the thumbnail.
    if let Some(scene) = state.scenes.iter().find(|s| s.id == scene_id) {
        let canvas_w = 1920.0_f32;
        let canvas_h = 1080.0_f32;
        let scale_x = thumb_rect.width() / canvas_w;
        let scale_y = thumb_rect.height() / canvas_h;

        for scene_src in &scene.sources {
            if let Some(lib_src) = state.library.iter().find(|s| s.id == scene_src.source_id) {
                let visible = scene_src.resolve_visible(lib_src);
                if !visible {
                    continue;
                }
                let t = scene_src.resolve_transform(lib_src);
                let mini_rect = egui::Rect::from_min_size(
                    egui::pos2(
                        thumb_rect.left() + t.x * scale_x,
                        thumb_rect.top() + t.y * scale_y,
                    ),
                    egui::vec2(t.width * scale_x, t.height * scale_y),
                );
                // Clamp to thumbnail bounds
                let clamped = mini_rect.intersect(thumb_rect);
                if clamped.width() > 0.5 && clamped.height() > 0.5 {
                    let fill = egui::Color32::from_rgba_premultiplied(
                        theme.text_muted.r(),
                        theme.text_muted.g(),
                        theme.text_muted.b(),
                        30,
                    );
                    painter.rect_filled(clamped, 1.0, fill);
                    painter.rect_stroke(
                        clamped,
                        1.0,
                        egui::Stroke::new(0.5, theme.text_muted),
                        egui::StrokeKind::Outside,
                    );
                }
            }
        }
    }

    // Pin indicator (top-right of thumbnail).
    if is_pinned {
        painter.text(
            egui::pos2(thumb_rect.right() - 6.0, thumb_rect.top() + 8.0),
            egui::Align2::RIGHT_CENTER,
            egui_phosphor::regular::PUSH_PIN,
            egui::FontId::proportional(10.0),
            theme.text_muted,
        );
    }

    let is_renaming = state.renaming_scene_id == Some(scene_id);

    // Label below thumbnail: inline TextEdit when renaming.
    if is_renaming {
        let label_rect =
            egui::Rect::from_center_size(label_pos, egui::vec2(col_width, label_height));
        let mut child_ui = ui.new_child(egui::UiBuilder::new().max_rect(label_rect).layout(
            egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
        ));
        let te = egui::TextEdit::singleline(&mut state.rename_buffer)
            .desired_width(col_width - 4.0)
            .font(egui::FontId::proportional(9.0))
            .horizontal_align(egui::Align::Center);
        let te_response = child_ui.add(te);
        // Focus once on first frame.
        let gen_id = egui::Id::new("scene_rename_gen");
        let focused_gen_id = egui::Id::new(("scene_rename_fg", scene_id.0));
        let current_gen: u64 = ui.data(|d| d.get_temp(gen_id).unwrap_or(0));
        let focused_gen: u64 = ui.data(|d| d.get_temp(focused_gen_id).unwrap_or(0));
        if focused_gen != current_gen {
            te_response.request_focus();
            ui.data_mut(|d| d.insert_temp(focused_gen_id, current_gen));
        }
        let confirmed = te_response.lost_focus() && !ui.input(|i| i.key_pressed(egui::Key::Escape));
        let cancelled = ui.input(|i| i.key_pressed(egui::Key::Escape));
        if confirmed {
            let new_name = state.rename_buffer.trim().to_string();
            if !new_name.is_empty() {
                if let Some(scene) = state.scenes.iter_mut().find(|s| s.id == scene_id) {
                    scene.name = new_name;
                }
                state.mark_dirty();
            }
            state.renaming_scene_id = None;
        } else if cancelled {
            state.renaming_scene_id = None;
        }
    } else {
        let label_color = if is_active {
            theme.text_primary
        } else {
            theme.text_secondary
        };
        painter.text(
            label_pos,
            egui::Align2::CENTER_CENTER,
            scene_name,
            egui::FontId::proportional(9.0),
            label_color,
        );
    }

    let mut action: Option<SceneAction> = None;

    // Click to switch active scene.
    if response.clicked() && !is_active && !is_renaming {
        action = Some(SceneAction::Switch(scene_id));
    }

    // Double-click to rename.
    if response.double_clicked() {
        state.renaming_scene_id = Some(scene_id);
        state.rename_buffer = scene_name.to_owned();
        let gen_id = egui::Id::new("scene_rename_gen");
        ui.data_mut(|d| {
            let g: u64 = d.get_temp(gen_id).unwrap_or(0);
            d.insert_temp(gen_id, g + 1);
        });
    }

    // Context menu.
    response.context_menu(|ui| {
        // Pin / Unpin
        let pin_label = if is_pinned {
            "Unpin from toolbar"
        } else {
            "Pin to toolbar"
        };
        if ui.button(pin_label).clicked() {
            if let Some(scene) = state.scenes.iter_mut().find(|s| s.id == scene_id) {
                scene.pinned = !scene.pinned;
            }
            state.mark_dirty();
            ui.close();
        }
        if ui.button("Rename").clicked() {
            state.renaming_scene_id = Some(scene_id);
            state.rename_buffer = scene_name.to_owned();
            let gen_id = egui::Id::new("scene_rename_gen");
            ui.data_mut(|d| {
                let g: u64 = d.get_temp(gen_id).unwrap_or(0);
                d.insert_temp(gen_id, g + 1);
            });
            ui.close();
        }
        if ui.button("Delete").clicked() {
            // Context menu closures can't return values; we rely on the outer
            // `pending_action` being set via a workaround. Since closures here
            // don't have access to the outer `action`, delete is handled by
            // storing intent in egui temp storage and reading it back.
            ui.data_mut(|d| {
                d.insert_temp::<bool>(egui::Id::new(("scene_delete", scene_id.0)), true)
            });
            ui.close();
        }
    });

    // Check for a delete that was set via temp storage from the context menu.
    let delete_requested: bool = ui.data_mut(|d| {
        let key = egui::Id::new(("scene_delete", scene_id.0));
        let v = d.get_temp(key).unwrap_or(false);
        if v {
            d.insert_temp(key, false);
        }
        v
    });
    if delete_requested {
        action = Some(SceneAction::Delete(scene_id));
    }

    action
}

/// Draw the dashed-border "Add" card with a "+" icon and "Add" label.
fn draw_add_card(
    painter: &egui::Painter,
    thumb_rect: Rect,
    label_pos: Pos2,
    hovered: bool,
    theme: &crate::ui::theme::Theme,
) {
    let border_color = if hovered {
        theme.text_muted
    } else {
        theme.border
    };

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
    let icon_color = if hovered {
        theme.text_muted
    } else {
        theme.border
    };
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
        theme.text_muted,
    );
}

/// Send `AddCaptureSource` / `RemoveCaptureSource` commands for the delta between two scenes.
fn apply_scene_diff(
    cmd_tx: &Option<tokio::sync::mpsc::Sender<GstCommand>>,
    library: &[crate::scene::LibrarySource],
    old_scene: Option<&Scene>,
    new_scene: Option<&Scene>,
    exclude_self: bool,
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
        if let Some(source) = library.iter().find(|s| s.id == src_id) {
            match &source.properties {
                crate::scene::SourceProperties::Display { screen_index } => {
                    let _ = tx.try_send(GstCommand::AddCaptureSource {
                        source_id: src_id,
                        config: CaptureSourceConfig::Screen {
                            screen_index: *screen_index,
                            exclude_self,
                        },
                    });
                }
                crate::scene::SourceProperties::Window { mode, .. } => {
                    let _ = tx.try_send(GstCommand::AddCaptureSource {
                        source_id: src_id,
                        config: CaptureSourceConfig::Window { mode: mode.clone() },
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
                crate::scene::SourceProperties::Audio { input } => {
                    let config = match input {
                        crate::scene::AudioInput::Device { device_uid, .. } => {
                            CaptureSourceConfig::AudioDevice {
                                device_uid: device_uid.clone(),
                            }
                        }
                        crate::scene::AudioInput::File { path, looping } => {
                            CaptureSourceConfig::AudioFile {
                                path: path.clone(),
                                looping: *looping,
                            }
                        }
                    };
                    let _ = tx.try_send(GstCommand::AddCaptureSource {
                        source_id: src_id,
                        config,
                    });
                }
                crate::scene::SourceProperties::Image { .. } => {
                    // Image sources don't use a capture pipeline.
                }
                // Text, Color, Browser: no capture pipeline
                _ => {}
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
            pinned: false,
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
    }

    // Remove the scene itself.
    state.scenes.retain(|s| s.id != scene_id);

    // Select the first remaining scene, clear source selection.
    state.deselect_all();
    let first_scene = state.scenes.first().cloned();
    if let Some(ref scene) = first_scene {
        state.active_scene_id = Some(scene.id);
        send_capture_for_scene(
            cmd_tx,
            &state.library,
            scene,
            state.settings.general.exclude_self_from_capture,
        );
        state.capture_active = !scene.sources.is_empty();
    } else {
        state.active_scene_id = None;
        state.capture_active = false;
    }

    state.mark_dirty();
}

/// Start capture for all sources in a scene, or `StopCapture` if it has none.
pub(crate) fn send_capture_for_scene(
    cmd_tx: &Option<tokio::sync::mpsc::Sender<GstCommand>>,
    library: &[crate::scene::LibrarySource],
    scene: &Scene,
    exclude_self: bool,
) {
    let Some(tx) = cmd_tx else { return };
    let mut any_started = false;
    for src_id in scene.source_ids() {
        if let Some(source) = library.iter().find(|s| s.id == src_id) {
            match &source.properties {
                crate::scene::SourceProperties::Display { screen_index } => {
                    let _ = tx.try_send(GstCommand::AddCaptureSource {
                        source_id: src_id,
                        config: CaptureSourceConfig::Screen {
                            screen_index: *screen_index,
                            exclude_self,
                        },
                    });
                    any_started = true;
                }
                crate::scene::SourceProperties::Window { mode, .. } => {
                    let _ = tx.try_send(GstCommand::AddCaptureSource {
                        source_id: src_id,
                        config: CaptureSourceConfig::Window { mode: mode.clone() },
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
                crate::scene::SourceProperties::Audio { input } => {
                    let config = match input {
                        crate::scene::AudioInput::Device { device_uid, .. } => {
                            CaptureSourceConfig::AudioDevice {
                                device_uid: device_uid.clone(),
                            }
                        }
                        crate::scene::AudioInput::File { path, looping } => {
                            CaptureSourceConfig::AudioFile {
                                path: path.clone(),
                                looping: *looping,
                            }
                        }
                    };
                    let _ = tx.try_send(GstCommand::AddCaptureSource {
                        source_id: src_id,
                        config,
                    });
                    any_started = true;
                }
                crate::scene::SourceProperties::Image { .. } => {
                    // Image sources don't use a capture pipeline.
                }
                // Text, Color, Browser: no capture pipeline
                _ => {}
            }
        }
    }
    if !any_started {
        let _ = tx.try_send(GstCommand::StopCapture);
    }
}
