//! Sources panel — scene composition tool.
//!
//! Lists the sources in the active scene (picked from the global library).
//! Each source is shown as a row with an icon, name, and visibility toggle.
//! Supports selection, reordering, add-from-library, and remove-from-scene.

use crate::gstreamer::{CaptureSourceConfig, GstCommand};
use crate::scene::{SceneId, SceneSource, SourceId, SourceOverrides, SourceProperties, SourceType};
use crate::state::AppState;
use crate::ui::draw_helpers::{draw_selection_highlight, source_icon, with_opacity};
use crate::ui::layout::tree::PanelId;
use crate::ui::theme::active_theme;
use egui::{Color32, CornerRadius, Rect, Sense, Stroke, vec2};

/// Payload type for drag-to-reorder within the source list.
/// Distinct from `SourceId` which is used for library-to-scene drag.
#[derive(Clone, Copy)]
struct ReorderPayload {
    source_id: SourceId,
}

/// Snapshot of per-source display data, collected before rendering to avoid borrow conflicts.
struct SourceRow {
    id: SourceId,
    name: String,
    source_type: SourceType,
    visible: bool,
    locked: bool,
}

/// Draw the sources panel for the currently active scene.
pub fn draw(ui: &mut egui::Ui, state: &mut AppState, _id: PanelId) {
    let theme = active_theme(ui.ctx());
    // Capture the full panel rect before any content is drawn, so the drop zone
    // covers the entire panel even when content is small.
    let full_panel_rect = ui.max_rect();
    let cmd_tx = state.command_tx.clone();

    let Some(active_id) = state.active_scene_id else {
        ui.centered_and_justified(|ui| {
            ui.colored_label(theme.text_muted, "No active scene");
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
        ui.colored_label(theme.text_primary, &scene_name);

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // Remove selected source from scene (small icon button)
            let has_selection = state.selected_source_id().is_some();
            ui.add_enabled_ui(has_selection, |ui| {
                if ui
                    .add(egui::Button::new(
                        egui::RichText::new(egui_phosphor::regular::MINUS).size(10.0),
                    ))
                    .on_hover_text("Remove from scene")
                    .clicked()
                    && let Some(src_id) = state.selected_source_id()
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

            if let Some((src_id, props)) =
                draw_add_from_library_popup(ui, state, active_id, popup_id, &add_response)
            {
                if let Some(scene) = state.scenes.iter_mut().find(|s| s.id == active_id) {
                    scene.sources.push(SceneSource {
                        source_id: src_id,
                        overrides: SourceOverrides::default(),
                    });
                }
                start_capture_from_properties(state, &cmd_tx, src_id, &props);
                state.select_source(src_id);
                state.mark_dirty();
            }
        });
    });

    ui.add_space(4.0);

    // ── Source list ──
    // Iterate the active scene's SceneSource entries and look up LibrarySource for each.
    let scene_sources: Vec<(SourceId, bool, bool)> = state
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
                    let locked = ss.resolve_locked();
                    (ss.source_id, visible, locked)
                })
                .collect()
        })
        .unwrap_or_default();

    if scene_sources.is_empty() {
        ui.add_space(16.0);
        ui.centered_and_justified(|ui| {
            ui.colored_label(theme.text_muted, "No sources. Click + to add one.");
        });
    }

    let source_count = scene_sources.len();
    let selected_bg = theme.accent_dim;

    // Reverse the list so top-most source (highest z-order) appears at the top.
    let rows: Vec<SourceRow> = scene_sources
        .iter()
        .rev()
        .filter_map(|(src_id, resolved_visible, resolved_locked)| {
            let lib = state.library.iter().find(|l| l.id == *src_id)?;
            Some(SourceRow {
                id: *src_id,
                name: lib.name.clone(),
                source_type: lib.source_type.clone(),
                visible: *resolved_visible,
                locked: *resolved_locked,
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
                    let dragged_display_idx = rows.iter().position(|r| r.id == did).unwrap_or(0);
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
            let is_selected = state.selected_source_id() == Some(row.id);
            let is_being_dragged = dragged_id == Some(row.id);
            let y_offset = offsets.get(&row.id.0).copied().unwrap_or(0.0);

            ui.push_id(row.id.0, |ui| {
                draw_source_row(
                    ui,
                    state,
                    row,
                    active_id,
                    available_width,
                    row_height,
                    y_offset,
                    is_selected,
                    is_being_dragged,
                    pointer_pos,
                    selected_bg,
                    idx,
                    source_count,
                );
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
    if let Some((dragged_id, display_insert_idx)) = reorder_drop {
        let mut did_reorder = false;
        if let Some(scene) = state.scenes.iter_mut().find(|s| s.id == active_id) {
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
                    did_reorder = true;
                }
            }
        }
        if did_reorder {
            state.mark_dirty();
        }
    }

    // ── Drop zone: accept SourceId dragged from library panel ──
    // Use the full panel rect captured at the start of draw (before content
    // layout shrank it). Expand by PANEL_PADDING to recover the wrapper bounds.
    let pad = theme.panel_padding;
    let panel_rect = full_panel_rect.expand(pad);
    let has_drag_payload = egui::DragAndDrop::has_payload_of_type::<SourceId>(ui.ctx());
    let pointer_in_panel = ui
        .input(|i| i.pointer.hover_pos())
        .is_some_and(|p| panel_rect.contains(p));
    let pointer_released = ui.input(|i| i.pointer.any_released());

    // Show visual hint when a library source is being dragged over the panel.
    // Use a foreground layer painter so the highlight is not clipped by the
    // padded UI region — the panel_rect extends beyond the current clip.
    if has_drag_payload && pointer_in_panel {
        let highlight_rect = panel_rect.shrink(2.0);
        let painter = ui.ctx().layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            ui.id().with("drop_highlight"),
        ));
        painter.rect_stroke(
            highlight_rect,
            CornerRadius::same(theme.radius_sm as u8),
            Stroke::new(1.0, state.accent_color),
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
            state.select_source(src_id);
            state.mark_dirty();
        }
    }
}

/// Draw the "add source from library" popup.
///
/// Returns `Some((source_id, properties))` if the user selected a source to add.
/// The caller is responsible for mutating scene and app state.
#[allow(deprecated)]
fn draw_add_from_library_popup(
    ui: &mut egui::Ui,
    state: &AppState,
    scene_id: SceneId,
    popup_id: egui::Id,
    anchor: &egui::Response,
) -> Option<(SourceId, SourceProperties)> {
    use crate::ui::widgets::menu::{menu_item_icon, styled_menu};
    let theme = active_theme(ui.ctx());

    // Snapshot library entries not already in the scene.
    let scene_source_ids: Vec<SourceId> = state
        .scenes
        .iter()
        .find(|s| s.id == scene_id)
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

    let mut selected: Option<(SourceId, SourceProperties)> = None;

    egui::popup_below_widget(
        ui,
        popup_id,
        anchor,
        egui::PopupCloseBehavior::CloseOnClickOutside,
        |ui: &mut egui::Ui| {
            styled_menu(ui, |ui| {
                if available_sources.is_empty() {
                    ui.label(
                        egui::RichText::new("All sources added")
                            .color(theme.text_muted)
                            .size(11.0),
                    );
                } else {
                    for (src_id, name, src_type, props) in &available_sources {
                        if menu_item_icon(ui, source_icon(src_type), name) {
                            selected = Some((*src_id, props.clone()));
                            ui.memory_mut(|m| m.close_popup(popup_id));
                        }
                    }
                }
            });
        },
    );

    selected
}

/// Draw a single source row with selection highlight, flash, drag handle, icon, name,
/// visibility eye, context menu, and separator.
#[allow(clippy::too_many_arguments)]
fn draw_source_row(
    ui: &mut egui::Ui,
    state: &mut AppState,
    row: &SourceRow,
    active_id: SceneId,
    available_width: f32,
    row_height: f32,
    y_offset: f32,
    is_selected: bool,
    is_being_dragged: bool,
    pointer_pos: Option<egui::Pos2>,
    selected_bg: Color32,
    idx: usize,
    source_count: usize,
) {
    let theme = active_theme(ui.ctx());
    let (row_rect, row_response) =
        ui.allocate_exact_size(vec2(available_width, row_height), Sense::click_and_drag());

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

    let row_opacity = if row.visible { 1.0 } else { 0.4 };
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
            let acc = state.accent_color;
            let flash_color = Color32::from_rgba_premultiplied(
                (acc.r() as f32 * alpha) as u8,
                (acc.g() as f32 * alpha) as u8,
                (acc.b() as f32 * alpha) as u8,
                (255.0 * alpha) as u8,
            );
            ui.painter()
                .rect_filled(paint_rect, CornerRadius::same(theme.radius_sm as u8), flash_color);
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
        state.select_source(row.id);
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
    painter.rect_filled(icon_rect, CornerRadius::same(theme.radius_sm as u8), theme.bg_elevated);
    painter.text(
        icon_rect.center(),
        egui::Align2::CENTER_CENTER,
        source_icon(&row.source_type),
        egui::FontId::proportional(10.0),
        with_opacity(theme.text_primary, effective_opacity),
    );
    cursor_x += icon_size + 6.0;

    // Name.
    let name_galley = painter.text(
        egui::pos2(cursor_x, center_y),
        egui::Align2::LEFT_CENTER,
        &row.name,
        egui::FontId::proportional(11.0),
        with_opacity(theme.text_primary, effective_opacity),
    );

    // Audio indicator: small speaker icon after the name for audio sources.
    if matches!(row.source_type, SourceType::Audio) {
        let indicator_x = cursor_x + name_galley.width() + 4.0;
        painter.text(
            egui::pos2(indicator_x, center_y),
            egui::Align2::LEFT_CENTER,
            egui_phosphor::regular::SPEAKER_HIGH,
            egui::FontId::proportional(9.0),
            with_opacity(theme.text_muted, effective_opacity),
        );
    }

    // Eye and lock icons (right-aligned, lock then eye from right).
    let right_x = paint_rect.right() - 4.0;

    // Eye icon — only shown when source is hidden (or hovered for toggle).
    let eye_rect =
        Rect::from_center_size(egui::pos2(right_x - 8.0, center_y), vec2(16.0, row_height));
    let eye_hovered = ui.rect_contains_pointer(eye_rect);
    if !row.visible || eye_hovered {
        let eye_text = if row.visible {
            egui_phosphor::regular::EYE
        } else {
            egui_phosphor::regular::EYE_SLASH
        };
        let eye_color = if eye_hovered {
            with_opacity(theme.text_primary, effective_opacity)
        } else {
            // Hidden sources get a prominent icon so the state is obvious
            with_opacity(theme.text_secondary, effective_opacity)
        };
        painter.text(
            eye_rect.center(),
            egui::Align2::CENTER_CENTER,
            eye_text,
            egui::FontId::proportional(11.0),
            eye_color,
        );
    }

    // Lock icon (to the left of the eye icon).
    let lock_text = if row.locked {
        egui_phosphor::regular::LOCK_SIMPLE
    } else {
        egui_phosphor::regular::LOCK_SIMPLE_OPEN
    };
    let lock_rect = Rect::from_center_size(
        egui::pos2(right_x - 8.0 - 18.0, center_y),
        vec2(16.0, row_height),
    );
    let lock_hovered = ui.rect_contains_pointer(lock_rect);
    let lock_color = if row.locked {
        // Locked: brighter to draw attention
        if lock_hovered {
            with_opacity(theme.text_primary, effective_opacity)
        } else {
            with_opacity(theme.text_primary, 0.7 * effective_opacity)
        }
    } else {
        // Unlocked: dimmer (only visible on hover)
        if lock_hovered {
            with_opacity(theme.text_muted, 0.8 * effective_opacity)
        } else {
            with_opacity(theme.text_muted, 0.2 * effective_opacity)
        }
    };
    painter.text(
        lock_rect.center(),
        egui::Align2::CENTER_CENTER,
        lock_text,
        egui::FontId::proportional(11.0),
        lock_color,
    );

    // Eye click.
    if eye_hovered && row_response.clicked() {
        let current_visible = row.visible;
        if let Some(scene) = state.scenes.iter_mut().find(|s| s.id == active_id)
            && let Some(scene_src) = scene.sources.iter_mut().find(|ss| ss.source_id == row.id)
        {
            scene_src.overrides.visible = Some(!current_visible);
        }
        state.mark_dirty();
    }

    // Lock click.
    if lock_hovered && row_response.clicked() {
        let current_locked = row.locked;
        if let Some(scene) = state.scenes.iter_mut().find(|s| s.id == active_id)
            && let Some(scene_src) = scene.sources.iter_mut().find(|ss| ss.source_id == row.id)
        {
            scene_src.overrides.locked = Some(!current_locked);
        }
        state.mark_dirty();
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
        let sep_y = paint_rect.bottom();
        painter.line_segment(
            [
                egui::pos2(paint_rect.left(), sep_y),
                egui::pos2(paint_rect.right(), sep_y),
            ],
            Stroke::new(1.0, theme.border),
        );
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
                    exclude_self: state.settings.general.exclude_self_from_capture,
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
        SourceProperties::Audio { input } => {
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
            let _ = tx.try_send(GstCommand::AddCaptureSource { source_id, config });
        }
        // Text, Color, Browser, Image: no capture pipeline
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
        SourceType::Display | SourceType::Window | SourceType::Camera | SourceType::Audio
    ) && let Some(tx) = cmd_tx
    {
        let _ = tx.try_send(GstCommand::RemoveCaptureSource { source_id });
    }
}

/// Remove a source from the active scene only (does NOT delete from library).
pub(crate) fn remove_source_from_scene(
    state: &mut AppState,
    cmd_tx: &Option<tokio::sync::mpsc::Sender<GstCommand>>,
    active_id: SceneId,
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
    if state.selected_source_id() == Some(src_id) {
        state.deselect_all();
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
    state.mark_dirty();
}
