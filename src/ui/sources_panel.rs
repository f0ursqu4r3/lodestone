//! Sources panel — scene composition tool.
//!
//! Lists the sources in the active scene (picked from the global library).
//! Each source is shown as a row with an icon, name, and visibility toggle.
//! Supports selection, reordering, add-from-library, and remove-from-scene.

use crate::gstreamer::{CaptureSourceConfig, GstCommand};
use crate::scene::{SceneSource, SourceId, SourceOverrides, SourceProperties, SourceType};
use crate::state::AppState;
use crate::ui::draw_helpers::{draw_selection_highlight, source_icon, with_opacity};
use crate::ui::layout::tree::PanelId;
use crate::ui::theme::{
    BG_ELEVATED, BORDER, DEFAULT_ACCENT, RADIUS_SM, TEXT_MUTED, TEXT_PRIMARY, accent_dim,
};
use egui::{Color32, CornerRadius, Rect, Sense, Stroke, vec2};

/// Payload type for drag-to-reorder within the source list.
/// Distinct from `SourceId` which is used for library-to-scene drag.
#[derive(Clone, Copy)]
struct ReorderPayload {
    source_id: SourceId,
}

/// Draw the sources panel for the currently active scene.
pub fn draw(ui: &mut egui::Ui, state: &mut AppState, _id: PanelId) {
    let cmd_tx = state.command_tx.clone();

    let Some(active_id) = state.active_scene_id else {
        ui.centered_and_justified(|ui| {
            ui.colored_label(TEXT_MUTED, "No active scene");
        });
        return;
    };

    // ── Header row: scene name + add/remove buttons ──
    let scene_name = state
        .scenes
        .iter()
        .find(|s| s.id == active_id)
        .map(|s| s.name.clone())
        .unwrap_or_else(|| "Sources".to_string());
    ui.horizontal(|ui| {
        ui.colored_label(TEXT_PRIMARY, &scene_name);

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // Remove selected source from scene (small icon button)
            let has_selection = state.selected_source_id.is_some();
            ui.add_enabled_ui(has_selection, |ui| {
                if ui
                    .add(egui::Button::new(
                        egui::RichText::new(egui_phosphor::regular::MINUS).size(10.0),
                    ))
                    .on_hover_text("Remove from scene")
                    .clicked()
                    && let Some(src_id) = state.selected_source_id
                {
                    remove_source_from_scene(state, &cmd_tx, active_id, src_id);
                }
            });

            // Add source from library (small icon button + popup menu)
            let add_response = ui
                .add(egui::Button::new(
                    egui::RichText::new(egui_phosphor::regular::PLUS).size(10.0),
                ))
                .on_hover_text("Add from library");

            let popup_id = ui.make_persistent_id("add_source_menu");
            if add_response.clicked() {
                #[allow(deprecated)]
                ui.memory_mut(|m: &mut egui::Memory| m.toggle_popup(popup_id));
            }

            // Snapshot library data before the popup to avoid borrow conflicts.
            let scene_source_ids: Vec<SourceId> = state
                .scenes
                .iter()
                .find(|s| s.id == active_id)
                .map(|s| s.source_ids())
                .unwrap_or_default();

            let available_sources: Vec<(SourceId, String, SourceType, SourceProperties)> = state
                .library
                .iter()
                .filter(|lib_src| !scene_source_ids.contains(&lib_src.id))
                .map(|lib_src| {
                    (
                        lib_src.id,
                        lib_src.name.clone(),
                        lib_src.source_type.clone(),
                        lib_src.properties.clone(),
                    )
                })
                .collect();

            #[allow(deprecated)]
            egui::popup_below_widget(
                ui,
                popup_id,
                &add_response,
                egui::PopupCloseBehavior::CloseOnClickOutside,
                |ui: &mut egui::Ui| {
                    use crate::ui::theme::{menu_item_icon, styled_menu};
                    styled_menu(ui, |ui| {
                        if available_sources.is_empty() {
                            ui.label(
                                egui::RichText::new("All sources added")
                                    .color(TEXT_MUTED)
                                    .size(11.0),
                            );
                        } else {
                            // Track which source to add (collected after loop to avoid borrow conflicts).
                            let mut source_to_add: Option<(SourceId, SourceProperties)> = None;

                            for (src_id, name, src_type, props) in &available_sources {
                                if menu_item_icon(ui, source_icon(src_type), name) {
                                    source_to_add = Some((*src_id, props.clone()));
                                    ui.memory_mut(|m| m.close_popup(popup_id));
                                }
                            }

                            // Apply outside the iterator to satisfy the borrow checker.
                            if let Some((src_id, props)) = source_to_add {
                                if let Some(scene) =
                                    state.scenes.iter_mut().find(|s| s.id == active_id)
                                {
                                    scene.sources.push(SceneSource {
                                        source_id: src_id,
                                        overrides: SourceOverrides::default(),
                                    });
                                }
                                // Start capture based on snapshotted properties.
                                start_capture_from_properties(state, &cmd_tx, src_id, &props);
                                state.selected_source_id = Some(src_id);
                                state.scenes_dirty = true;
                                state.scenes_last_changed = std::time::Instant::now();
                            }
                        }
                    });
                },
            );
        });
    });

    ui.add_space(4.0);

    // ── Source list ──
    // Iterate the active scene's SceneSource entries and look up LibrarySource for each.
    let scene_sources: Vec<(SourceId, bool)> = state
        .scenes
        .iter()
        .find(|s| s.id == active_id)
        .map(|scene| {
            scene
                .sources
                .iter()
                .map(|ss| {
                    let lib = state.library.iter().find(|l| l.id == ss.source_id);
                    let visible = lib.map(|l| ss.resolve_visible(l)).unwrap_or(true);
                    (ss.source_id, visible)
                })
                .collect()
        })
        .unwrap_or_default();

    if scene_sources.is_empty() {
        ui.add_space(16.0);
        ui.centered_and_justified(|ui| {
            ui.colored_label(TEXT_MUTED, "No sources. Click + to add one.");
        });
    }

    let source_count = scene_sources.len();
    let selected_bg = accent_dim(DEFAULT_ACCENT);

    // Snapshot display data for each source to avoid borrowing state during rendering.
    struct SourceRow {
        id: SourceId,
        name: String,
        source_type: SourceType,
        visible: bool,
    }

    // Reverse the list so top-most source (highest z-order) appears at the top.
    let rows: Vec<SourceRow> = scene_sources
        .iter()
        .rev()
        .filter_map(|(src_id, resolved_visible)| {
            let lib = state.library.iter().find(|l| l.id == *src_id)?;
            Some(SourceRow {
                id: *src_id,
                name: lib.name.clone(),
                source_type: lib.source_type.clone(),
                visible: *resolved_visible,
            })
        })
        .collect();

    // ── Animated drag-to-reorder state ──
    // Persist per-row Y offsets across frames for smooth animation.
    let offsets_id = ui.make_persistent_id("source_row_offsets");
    let mut offsets: std::collections::HashMap<u64, f32> =
        ui.data(|d| d.get_temp(offsets_id).unwrap_or_default());

    let row_height = 28.0_f32;
    let mut reorder_drop: Option<(SourceId, usize)> = None;

    // Determine drag state: which row is being dragged and where the insertion point is.
    let drag_payload = egui::DragAndDrop::payload::<ReorderPayload>(ui.ctx());
    let pointer_pos = ui.ctx().pointer_interact_pos();
    let dragged_id = drag_payload.as_ref().map(|p| p.source_id);

    // Compute the insertion index based on pointer Y (we need the scroll area top for this).
    egui::ScrollArea::vertical().show(ui, |ui| {
        let list_top = ui.cursor().top();
        let available_width = ui.available_width();

        // Calculate insertion index from pointer position.
        let insert_idx = match (dragged_id, pointer_pos) {
            (Some(_), Some(pos)) => {
                let mut idx = rows.len();
                for (i, _) in rows.iter().enumerate() {
                    let row_center_y = list_top + (i as f32 + 0.5) * row_height;
                    if pos.y < row_center_y {
                        idx = i;
                        break;
                    }
                }
                Some(idx)
            }
            _ => None,
        };

        // Compute target Y offsets for each row.
        let lerp_speed: f32 = 0.2; // 0..1, higher = faster
        let dt: f32 = ui.input(|i| i.stable_dt).min(0.1);
        let lerp_t: f32 = (1.0 - (1.0_f32 - lerp_speed).powf(dt * 60.0)).clamp(0.0, 1.0);
        let mut any_animating = false;

        for (idx, row) in rows.iter().enumerate() {
            let target_offset = match (dragged_id, insert_idx) {
                (Some(did), Some(ins)) if row.id != did => {
                    // Find the dragged row's display index.
                    let dragged_display_idx =
                        rows.iter().position(|r| r.id == did).unwrap_or(0);
                    // Rows between the drag origin and insertion point shift by one row height.
                    if dragged_display_idx < ins {
                        // Dragging down: rows between (drag+1..ins) shift up.
                        if idx > dragged_display_idx && idx < ins {
                            -row_height
                        } else {
                            0.0
                        }
                    } else {
                        // Dragging up: rows between (ins..drag) shift down.
                        if idx >= ins && idx < dragged_display_idx {
                            row_height
                        } else {
                            0.0
                        }
                    }
                }
                _ => 0.0, // No drag active or this is the dragged row — target 0.
            };

            let current = offsets.get(&row.id.0).copied().unwrap_or(0.0);
            let new_offset = current + (target_offset - current) * lerp_t;

            if (new_offset - target_offset).abs() > 0.5 {
                any_animating = true;
            }

            offsets.insert(row.id.0, new_offset);
        }

        if any_animating || dragged_id.is_some() {
            ui.ctx().request_repaint();
        }

        // ── Render rows with animated offsets ──
        for (idx, row) in rows.iter().enumerate() {
            let is_selected = state.selected_source_id == Some(row.id);
            let row_opacity = if row.visible { 1.0 } else { 0.4 };
            let is_being_dragged = dragged_id == Some(row.id);
            let y_offset = offsets.get(&row.id.0).copied().unwrap_or(0.0);

            ui.push_id(row.id.0, |ui| {
                let (row_rect, row_response) = ui
                    .allocate_exact_size(vec2(available_width, row_height), Sense::click_and_drag());

                // The paint rect is shifted by the animation offset.
                let paint_rect = if is_being_dragged {
                    // Dragged row follows the pointer.
                    if let Some(pos) = pointer_pos {
                        Rect::from_min_size(
                            egui::pos2(row_rect.left(), pos.y - row_height / 2.0),
                            row_rect.size(),
                        )
                    } else {
                        row_rect
                    }
                } else {
                    row_rect.translate(vec2(0.0, y_offset))
                };

                let effective_opacity = if is_being_dragged {
                    row_opacity * 0.5
                } else {
                    row_opacity
                };

                // Selection highlight.
                if is_selected && !is_being_dragged {
                    draw_selection_highlight(ui.painter(), paint_rect, selected_bg);
                }

                // Flash highlight.
                if state.flash_source_id == Some(row.id)
                    && let Some(start) = state.flash_start
                {
                    let elapsed = start.elapsed().as_secs_f32();
                    let duration = 0.6;
                    if elapsed < duration {
                        let alpha = (1.0 - elapsed / duration) * 0.4;
                        let flash_color = Color32::from_rgba_premultiplied(
                            (DEFAULT_ACCENT.r() as f32 * alpha) as u8,
                            (DEFAULT_ACCENT.g() as f32 * alpha) as u8,
                            (DEFAULT_ACCENT.b() as f32 * alpha) as u8,
                            (255.0 * alpha) as u8,
                        );
                        ui.painter()
                            .rect_filled(paint_rect, CornerRadius::same(RADIUS_SM as u8), flash_color);
                        ui.ctx().request_repaint();
                    } else {
                        state.flash_source_id = None;
                        state.flash_start = None;
                    }
                }

                // Start drag.
                if row_response.drag_started() {
                    row_response.dnd_set_drag_payload(ReorderPayload { source_id: row.id });
                }

                // Click to select.
                if row_response.clicked() {
                    state.selected_source_id = Some(row.id);
                    state.selected_library_source_id = None;
                }

                // Paint row contents at the animated position.
                // Use painter_at to avoid borrowing ui for the painter's lifetime.
                let painter = ui.painter_at(paint_rect);
                let mut cursor_x = paint_rect.left() + 4.0;
                let center_y = paint_rect.center().y;

                // Icon.
                let icon_size = 16.0;
                let icon_rect = Rect::from_center_size(
                    egui::pos2(cursor_x + icon_size / 2.0, center_y),
                    vec2(icon_size, icon_size),
                );
                painter.rect_filled(icon_rect, CornerRadius::same(RADIUS_SM as u8), BG_ELEVATED);
                painter.text(
                    icon_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    source_icon(&row.source_type),
                    egui::FontId::proportional(10.0),
                    with_opacity(TEXT_PRIMARY, effective_opacity),
                );
                cursor_x += icon_size + 6.0;

                // Name.
                painter.text(
                    egui::pos2(cursor_x, center_y),
                    egui::Align2::LEFT_CENTER,
                    &row.name,
                    egui::FontId::proportional(11.0),
                    with_opacity(TEXT_PRIMARY, effective_opacity),
                );

                // Eye icon.
                let right_x = paint_rect.right() - 4.0;
                let eye_text = if row.visible {
                    egui_phosphor::regular::EYE
                } else {
                    egui_phosphor::regular::EYE_SLASH
                };
                let eye_rect = Rect::from_center_size(
                    egui::pos2(right_x - 8.0, center_y),
                    vec2(16.0, row_height),
                );
                let eye_hovered = ui.rect_contains_pointer(eye_rect);
                let eye_color = if eye_hovered {
                    with_opacity(TEXT_PRIMARY, effective_opacity)
                } else {
                    with_opacity(TEXT_MUTED, 0.5 * effective_opacity)
                };
                painter.text(
                    eye_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    eye_text,
                    egui::FontId::proportional(11.0),
                    eye_color,
                );

                // Eye click.
                if eye_hovered && row_response.clicked() {
                    let current_visible = row.visible;
                    if let Some(scene) = state.scenes.iter_mut().find(|s| s.id == active_id)
                        && let Some(scene_src) =
                            scene.sources.iter_mut().find(|ss| ss.source_id == row.id)
                    {
                        scene_src.overrides.visible = Some(!current_visible);
                    }
                    state.scenes_dirty = true;
                    state.scenes_last_changed = std::time::Instant::now();
                }

                // Context menu.
                crate::ui::transform_handles::show_source_context_menu(
                    ui,
                    &row_response,
                    state,
                    row.id,
                    egui::Vec2::new(1920.0, 1080.0),
                );

                // Separator (skip for dragged row).
                if idx + 1 < source_count && !is_being_dragged {
                    let sep_y = paint_rect.bottom() ;
                    painter.line_segment(
                        [
                            egui::pos2(paint_rect.left(), sep_y),
                            egui::pos2(paint_rect.right(), sep_y),
                        ],
                        Stroke::new(1.0, BORDER),
                    );
                }
            });
        }

        // Detect drop.
        if dragged_id.is_some()
            && ui.input(|i| i.pointer.any_released())
            && let (Some(did), Some(ins)) = (dragged_id, insert_idx)
        {
            reorder_drop = Some((did, ins));
        }
    });

    // Store animated offsets.
    ui.data_mut(|d| d.insert_temp(offsets_id, offsets));

    // ── Apply drag-to-reorder ──
    // Display is reversed (top-most first), so display index 0 = last in data.
    // Convert: data_index = data_len - 1 - display_index
    if let Some((dragged_id, display_insert_idx)) = reorder_drop
        && let Some(scene) = state.scenes.iter_mut().find(|s| s.id == active_id)
    {
            let data_len = scene.sources.len();
            // Find current position in data.
            if let Some(from_data) = scene
                .sources
                .iter()
                .position(|ss| ss.source_id == dragged_id)
            {
                // Convert display insert index to data insert index.
                // Display idx 0 = top of list = data idx (data_len - 1), etc.
                let to_data = data_len.saturating_sub(display_insert_idx);
                // Clamp and skip no-ops.
                let to_data = to_data.min(data_len);
                if from_data != to_data && from_data + 1 != to_data {
                    let entry = scene.sources.remove(from_data);
                    let adjusted = if to_data > from_data {
                        to_data - 1
                    } else {
                        to_data
                    };
                    scene.sources.insert(adjusted, entry);
                    state.scenes_dirty = true;
                    state.scenes_last_changed = std::time::Instant::now();
                }
            }
    }

    // ── Drop zone: accept SourceId dragged from library panel ──
    // The entire panel is a drop target, not just the empty space at the bottom.
    let panel_rect = ui.min_rect();
    let has_drag_payload = egui::DragAndDrop::has_payload_of_type::<SourceId>(ui.ctx());
    let pointer_in_panel = ui
        .input(|i| i.pointer.hover_pos())
        .is_some_and(|p| panel_rect.contains(p));
    let pointer_released = ui.input(|i| i.pointer.any_released());

    // Show visual hint when a library source is being dragged over the panel.
    if has_drag_payload && pointer_in_panel {
        ui.painter().rect_stroke(
            panel_rect,
            CornerRadius::same(RADIUS_SM as u8),
            Stroke::new(1.0, DEFAULT_ACCENT),
            egui::StrokeKind::Inside,
        );
    }

    // Accept the drop when pointer is released over the panel.
    let dropped_payload = if has_drag_payload && pointer_in_panel && pointer_released {
        egui::DragAndDrop::take_payload::<SourceId>(ui.ctx())
    } else {
        None
    };

    if let Some(payload) = dropped_payload {
        let src_id = *payload;
        let already_in_scene = state
            .active_scene()
            .map(|s| s.sources.iter().any(|ss| ss.source_id == src_id))
            .unwrap_or(false);
        if !already_in_scene {
            let props = state
                .library
                .iter()
                .find(|l| l.id == src_id)
                .map(|l| l.properties.clone());
            if let Some(scene) = state.active_scene_mut() {
                scene.sources.push(SceneSource {
                    source_id: src_id,
                    overrides: SourceOverrides::default(),
                });
            }
            if let Some(properties) = props {
                start_capture_from_properties(state, &cmd_tx, src_id, &properties);
            }
            state.selected_source_id = Some(src_id);
            state.scenes_dirty = true;
            state.scenes_last_changed = std::time::Instant::now();
        }
    }
}

/// Start capture from already-snapshotted properties (avoids borrow conflicts).
fn start_capture_from_properties(
    state: &mut AppState,
    cmd_tx: &Option<tokio::sync::mpsc::Sender<GstCommand>>,
    source_id: SourceId,
    properties: &SourceProperties,
) {
    let Some(tx) = cmd_tx else { return };
    match properties {
        SourceProperties::Display { screen_index } => {
            let _ = tx.try_send(GstCommand::AddCaptureSource {
                source_id,
                config: CaptureSourceConfig::Screen {
                    screen_index: *screen_index,
                },
            });
            state.capture_active = true;
        }
        SourceProperties::Window { window_id, .. } if *window_id != 0 => {
            let _ = tx.try_send(GstCommand::AddCaptureSource {
                source_id,
                config: CaptureSourceConfig::Window {
                    window_id: *window_id,
                },
            });
            state.capture_active = true;
        }
        SourceProperties::Camera { device_index, .. } => {
            let _ = tx.try_send(GstCommand::AddCaptureSource {
                source_id,
                config: CaptureSourceConfig::Camera {
                    device_index: *device_index,
                },
            });
            state.capture_active = true;
        }
        _ => {}
    }
}

/// Stop the capture pipeline for a source if it has a capturable type.
fn stop_capture_for_source(
    cmd_tx: &Option<tokio::sync::mpsc::Sender<GstCommand>>,
    source_type: &SourceType,
    source_id: SourceId,
) {
    if matches!(
        source_type,
        SourceType::Display | SourceType::Window | SourceType::Camera
    ) && let Some(tx) = cmd_tx
    {
        let _ = tx.try_send(GstCommand::RemoveCaptureSource { source_id });
    }
}

/// Remove a source from the active scene only (does NOT delete from library).
fn remove_source_from_scene(
    state: &mut AppState,
    cmd_tx: &Option<tokio::sync::mpsc::Sender<GstCommand>>,
    active_id: crate::scene::SceneId,
    src_id: SourceId,
) {
    // Get source type before removing from scene.
    let source_type = state
        .library
        .iter()
        .find(|s| s.id == src_id)
        .map(|s| s.source_type.clone());

    // Remove from scene.
    if let Some(scene) = state.scenes.iter_mut().find(|s| s.id == active_id) {
        scene.sources.retain(|s| s.source_id != src_id);
    }

    // Stop capture if the source is no longer in the active scene.
    if let Some(st) = &source_type {
        stop_capture_for_source(cmd_tx, st, src_id);
    }

    // Clear selection if we just removed the selected source.
    if state.selected_source_id == Some(src_id) {
        state.selected_source_id = None;
    }

    // Update capture_active based on remaining sources.
    let has_sources = state
        .scenes
        .iter()
        .find(|s| s.id == active_id)
        .map(|s| !s.sources.is_empty())
        .unwrap_or(false);
    if !has_sources && let Some(tx) = cmd_tx {
        let _ = tx.try_send(GstCommand::StopCapture);
    }
    state.capture_active = has_sources;
    state.scenes_dirty = true;
    state.scenes_last_changed = std::time::Instant::now();
}

