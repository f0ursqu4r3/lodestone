//! Sources panel — scene composition tool.
//!
//! Lists the sources in the active scene (picked from the global library).
//! Each source is shown as a row with an icon, name, and visibility toggle.
//! Supports selection, reordering, add-from-library, and remove-from-scene.

use crate::gstreamer::{CaptureSourceConfig, GstCommand};
use crate::scene::{SceneSource, SourceId, SourceOverrides, SourceProperties, SourceType};
use crate::state::AppState;
use crate::ui::layout::tree::PanelId;
use crate::ui::theme::{
    BG_ELEVATED, BORDER, DEFAULT_ACCENT, RADIUS_SM, TEXT_MUTED, TEXT_PRIMARY, accent_dim,
};
use egui::{Color32, CornerRadius, Rect, Sense, Stroke, vec2};

/// Return a Phosphor icon for a given source type.
fn source_icon(source_type: &SourceType) -> &'static str {
    match source_type {
        SourceType::Display => egui_phosphor::regular::MONITOR,
        SourceType::Camera => egui_phosphor::regular::VIDEO_CAMERA,
        SourceType::Image => egui_phosphor::regular::IMAGE,
        SourceType::Browser => egui_phosphor::regular::BROWSER,
        SourceType::Audio => egui_phosphor::regular::SPEAKER_HIGH,
        SourceType::Window => egui_phosphor::regular::APP_WINDOW,
    }
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

    // ── Header row: title + add/remove buttons ──
    ui.horizontal(|ui| {
        ui.colored_label(TEXT_PRIMARY, "Sources");

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // Remove selected source from scene
            let has_selection = state.selected_source_id.is_some();
            ui.add_enabled_ui(has_selection, |ui| {
                if ui
                    .button(egui_phosphor::regular::MINUS)
                    .on_hover_text("Remove from scene")
                    .clicked()
                    && let Some(src_id) = state.selected_source_id
                {
                    remove_source_from_scene(state, &cmd_tx, active_id, src_id);
                }
            });

            // Add source from library (popup menu)
            let add_response = ui
                .button(egui_phosphor::regular::PLUS)
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

    let mut move_up: Option<SourceId> = None;
    let mut move_down: Option<SourceId> = None;
    let source_count = scene_sources.len();
    let selected_bg = accent_dim(DEFAULT_ACCENT);

    // Snapshot display data for each source to avoid borrowing state during rendering.
    struct SourceRow {
        id: SourceId,
        name: String,
        source_type: SourceType,
        visible: bool,
    }

    let rows: Vec<SourceRow> = scene_sources
        .iter()
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

    egui::ScrollArea::vertical().show(ui, |ui| {
        for (idx, row) in rows.iter().enumerate() {
            let is_selected = state.selected_source_id == Some(row.id);

            // Row opacity: dim hidden sources to 40%.
            let row_opacity = if row.visible { 1.0 } else { 0.4 };

            ui.push_id(row.id.0, |ui| {
                // Allocate a row area.
                let row_height = 28.0;
                let available_width = ui.available_width();
                let (row_rect, row_response) =
                    ui.allocate_exact_size(vec2(available_width, row_height), Sense::click());

                // Selection highlight background.
                if is_selected {
                    ui.painter().rect_filled(
                        row_rect,
                        CornerRadius::same(RADIUS_SM as u8),
                        selected_bg,
                    );
                }

                // Flash highlight when selected in library.
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
                        ui.painter().rect_filled(
                            row_rect,
                            CornerRadius::same(RADIUS_SM as u8),
                            flash_color,
                        );
                        ui.ctx().request_repaint();
                    } else {
                        // Flash finished — clear it.
                        state.flash_source_id = None;
                        state.flash_start = None;
                    }
                }

                // Handle click for selection (scene selection, clears library selection).
                if row_response.clicked() {
                    state.selected_source_id = Some(row.id);
                    state.selected_library_source_id = None;
                }

                // Paint the row contents.
                let painter = ui.painter_at(row_rect);
                let mut cursor_x = row_rect.left() + 4.0;
                let center_y = row_rect.center().y;

                // ── Icon (16x16, BG_ELEVATED background, 2px border-radius) ──
                let icon_size = 16.0;
                let icon_rect = Rect::from_center_size(
                    egui::pos2(cursor_x + icon_size / 2.0, center_y),
                    vec2(icon_size, icon_size),
                );
                painter.rect_filled(icon_rect, CornerRadius::same(RADIUS_SM as u8), BG_ELEVATED);
                let icon_text = source_icon(&row.source_type);
                let icon_color = with_opacity(TEXT_PRIMARY, row_opacity);
                painter.text(
                    icon_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    icon_text,
                    egui::FontId::proportional(10.0),
                    icon_color,
                );
                cursor_x += icon_size + 6.0;

                // ── Name (TEXT_PRIMARY, 11px) ──
                let name_color = with_opacity(TEXT_PRIMARY, row_opacity);
                painter.text(
                    egui::pos2(cursor_x, center_y),
                    egui::Align2::LEFT_CENTER,
                    &row.name,
                    egui::FontId::proportional(11.0),
                    name_color,
                );

                // ── Right-aligned controls ──
                let right_x = row_rect.right() - 4.0;

                // Visibility toggle eye icon.
                let eye_text = if row.visible {
                    egui_phosphor::regular::EYE
                } else {
                    egui_phosphor::regular::EYE_SLASH
                };
                let eye_width = 16.0;
                let eye_rect = Rect::from_center_size(
                    egui::pos2(right_x - eye_width / 2.0, center_y),
                    vec2(eye_width, row_height),
                );

                let eye_hovered = ui.rect_contains_pointer(eye_rect);
                let eye_color = if eye_hovered {
                    with_opacity(TEXT_PRIMARY, row_opacity)
                } else {
                    with_opacity(TEXT_MUTED, 0.5 * row_opacity)
                };
                painter.text(
                    eye_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    eye_text,
                    egui::FontId::proportional(11.0),
                    eye_color,
                );

                // Handle eye click — toggle per-scene visibility override.
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

                // Source context menu (right-click) for quick transform actions.
                crate::ui::transform_handles::show_source_context_menu(
                    ui,
                    &row_response,
                    state,
                    row.id,
                    egui::Vec2::new(1920.0, 1080.0), // TODO: read from settings
                );

                // Separator line between items.
                if idx + 1 < source_count {
                    let sep_y = row_rect.bottom();
                    painter.line_segment(
                        [
                            egui::pos2(row_rect.left(), sep_y),
                            egui::pos2(row_rect.right(), sep_y),
                        ],
                        Stroke::new(1.0, BORDER),
                    );
                }
            });
        }
    });

    // ── Reorder buttons for selected source ──
    if let Some(selected_id) = state.selected_source_id {
        let source_ids: Vec<SourceId> = rows.iter().map(|r| r.id).collect();
        if let Some(idx) = source_ids.iter().position(|&id| id == selected_id) {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.add_enabled_ui(idx > 0, |ui| {
                    if ui
                        .button(egui_phosphor::regular::ARROW_UP)
                        .on_hover_text("Move up")
                        .clicked()
                    {
                        move_up = Some(selected_id);
                    }
                });
                ui.add_enabled_ui(idx + 1 < source_count, |ui| {
                    if ui
                        .button(egui_phosphor::regular::ARROW_DOWN)
                        .on_hover_text("Move down")
                        .clicked()
                    {
                        move_down = Some(selected_id);
                    }
                });
            });
        }
    }

    // ── Apply reorder mutations ──
    if let Some(src_id) = move_up {
        if let Some(scene) = state.scenes.iter_mut().find(|s| s.id == active_id) {
            scene.move_source_up(src_id);
        }
        state.scenes_dirty = true;
        state.scenes_last_changed = std::time::Instant::now();
    }
    if let Some(src_id) = move_down {
        if let Some(scene) = state.scenes.iter_mut().find(|s| s.id == active_id) {
            scene.move_source_down(src_id);
        }
        state.scenes_dirty = true;
        state.scenes_last_changed = std::time::Instant::now();
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

/// Apply an opacity multiplier to a Color32.
fn with_opacity(color: Color32, opacity: f32) -> Color32 {
    Color32::from_rgba_premultiplied(
        (color.r() as f32 * opacity) as u8,
        (color.g() as f32 * opacity) as u8,
        (color.b() as f32 * opacity) as u8,
        (color.a() as f32 * opacity) as u8,
    )
}
