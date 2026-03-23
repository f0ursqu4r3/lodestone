//! Sources panel — displays the source list for the active scene.
//!
//! Each source is shown as a row with an icon, name, and visibility toggle.
//! Supports selection, reordering, add, and remove.

use crate::gstreamer::{CaptureSourceConfig, GstCommand};
use crate::scene::{Source, SourceId, SourceProperties, SourceType, Transform};
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
            // Remove selected source
            let has_selection = state.selected_source_id.is_some();
            ui.add_enabled_ui(has_selection, |ui| {
                if ui
                    .button(egui_phosphor::regular::MINUS)
                    .on_hover_text("Remove source")
                    .clicked()
                    && let Some(src_id) = state.selected_source_id
                {
                    remove_source(state, &cmd_tx, active_id, src_id);
                }
            });

            // Add source popup
            let add_btn = ui.button(egui_phosphor::regular::PLUS).on_hover_text("Add source");
            let popup_id = ui.make_persistent_id("add_source_popup");
            if add_btn.clicked() {
                #[allow(deprecated)]
                ui.memory_mut(|mem| mem.toggle_popup(popup_id));
            }
            let mut add_source: Option<SourceType> = None;
            #[allow(deprecated)]
            egui::popup_below_widget(ui, popup_id, &add_btn, egui::PopupCloseBehavior::CloseOnClick, |ui| {
                ui.set_min_width(120.0);
                if ui.button("Display").clicked() {
                    add_source = Some(SourceType::Display);
                }
                if ui.button("Image").clicked() {
                    add_source = Some(SourceType::Image);
                }
            });
            if let Some(source_type) = add_source {
                match source_type {
                    SourceType::Display => add_display_source(state, &cmd_tx, active_id),
                    SourceType::Image => add_image_source(state, &cmd_tx, active_id),
                    _ => {}
                }
            }
        });
    });

    ui.add_space(4.0);

    // ── Source list ──
    let source_ids: Vec<SourceId> = state
        .scenes
        .iter()
        .find(|s| s.id == active_id)
        .map(|s| s.sources.clone())
        .unwrap_or_default();

    if source_ids.is_empty() {
        ui.add_space(16.0);
        ui.centered_and_justified(|ui| {
            ui.colored_label(TEXT_MUTED, "No sources. Click + to add one.");
        });
        return;
    }

    let mut move_up: Option<SourceId> = None;
    let mut move_down: Option<SourceId> = None;
    let source_count = source_ids.len();
    let selected_bg = accent_dim(DEFAULT_ACCENT);

    egui::ScrollArea::vertical().show(ui, |ui| {
        for (idx, &src_id) in source_ids.iter().enumerate() {
            let source = state.sources.iter().find(|s| s.id == src_id);
            let Some(source) = source else { continue };

            let is_selected = state.selected_source_id == Some(src_id);
            let is_visible = source.visible;
            let source_name = source.name.clone();
            let source_type = source.source_type.clone();

            // Row opacity: dim hidden sources to 40%.
            let row_opacity = if is_visible { 1.0 } else { 0.4 };

            ui.push_id(src_id.0, |ui| {
                // Allocate a row area.
                let row_height = 28.0;
                let available_width = ui.available_width();
                let (row_rect, row_response) =
                    ui.allocate_exact_size(vec2(available_width, row_height), Sense::click());

                // Selection highlight background.
                if is_selected {
                    ui.painter()
                        .rect_filled(row_rect, CornerRadius::same(RADIUS_SM as u8), selected_bg);
                }

                // Handle click for selection.
                if row_response.clicked() {
                    state.selected_source_id = Some(src_id);
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
                let icon_text = source_icon(&source_type);
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
                    &source_name,
                    egui::FontId::proportional(11.0),
                    name_color,
                );

                // ── Right-aligned controls ──
                let right_x = row_rect.right() - 4.0;

                // Visibility toggle eye icon.
                let eye_text = if is_visible {
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

                // Handle eye click (toggle visibility).
                if eye_hovered
                    && row_response.clicked()
                    && let Some(source) = state.sources.iter_mut().find(|s| s.id == src_id)
                {
                    source.visible = !source.visible;
                    state.scenes_dirty = true;
                    state.scenes_last_changed = std::time::Instant::now();
                }

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
        // Only show reorder if the selected source belongs to this scene.
        if let Some(idx) = source_ids.iter().position(|&id| id == selected_id) {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.add_enabled_ui(idx > 0, |ui| {
                    if ui.button(egui_phosphor::regular::ARROW_UP).on_hover_text("Move up").clicked() {
                        move_up = Some(selected_id);
                    }
                });
                ui.add_enabled_ui(idx + 1 < source_count, |ui| {
                    if ui.button(egui_phosphor::regular::ARROW_DOWN).on_hover_text("Move down").clicked() {
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
}

/// Add a new display source to the active scene.
fn add_display_source(
    state: &mut AppState,
    cmd_tx: &Option<tokio::sync::mpsc::Sender<GstCommand>>,
    active_id: crate::scene::SceneId,
) {
    let new_src_id = SourceId(state.next_source_id);
    state.next_source_id += 1;
    let source_count = state
        .scenes
        .iter()
        .find(|s| s.id == active_id)
        .map(|s| s.sources.len())
        .unwrap_or(0);
    let new_source = Source {
        id: new_src_id,
        name: format!("Display {}", source_count + 1),
        source_type: SourceType::Display,
        properties: SourceProperties::Display { screen_index: 0 },
        transform: Transform::new(0.0, 0.0, 1920.0, 1080.0),
        opacity: 1.0,
        visible: true,
        muted: false,
        volume: 1.0,
    };
    state.sources.push(new_source);
    if let Some(scene) = state.scenes.iter_mut().find(|s| s.id == active_id) {
        scene.sources.push(new_src_id);
    }
    if let Some(tx) = cmd_tx {
        let _ = tx.try_send(GstCommand::AddCaptureSource {
            source_id: new_src_id,
            config: CaptureSourceConfig::Screen { screen_index: 0 },
        });
    }
    state.selected_source_id = Some(new_src_id);
    state.capture_active = true;
    state.scenes_dirty = true;
    state.scenes_last_changed = std::time::Instant::now();
}

/// Add a new image source to the active scene.
fn add_image_source(
    state: &mut AppState,
    _cmd_tx: &Option<tokio::sync::mpsc::Sender<GstCommand>>,
    active_id: crate::scene::SceneId,
) {
    let new_src_id = SourceId(state.next_source_id);
    state.next_source_id += 1;
    let source = Source {
        id: new_src_id,
        name: "Image".to_string(),
        source_type: SourceType::Image,
        properties: SourceProperties::Image {
            path: String::new(),
        },
        transform: Transform::new(0.0, 0.0, 1920.0, 1080.0),
        opacity: 1.0,
        visible: true,
        muted: false,
        volume: 1.0,
    };
    state.sources.push(source);
    if let Some(scene) = state.scenes.iter_mut().find(|s| s.id == active_id) {
        scene.sources.push(new_src_id);
    }
    state.selected_source_id = Some(new_src_id);
    state.scenes_dirty = true;
    state.scenes_last_changed = std::time::Instant::now();
}

/// Remove a source from the active scene and clean up state.
fn remove_source(
    state: &mut AppState,
    cmd_tx: &Option<tokio::sync::mpsc::Sender<GstCommand>>,
    active_id: crate::scene::SceneId,
    src_id: SourceId,
) {
    // Remove from scene.
    if let Some(scene) = state.scenes.iter_mut().find(|s| s.id == active_id) {
        scene.sources.retain(|&id| id != src_id);
    }
    // Check source type before removing — only capture-based sources need a GstCommand.
    let has_capture_pipeline = state
        .sources
        .iter()
        .find(|s| s.id == src_id)
        .map(|s| matches!(s.source_type, SourceType::Display | SourceType::Window | SourceType::Camera))
        .unwrap_or(false);
    // Remove from global sources list.
    state.sources.retain(|s| s.id != src_id);
    // Send GstCommand only for sources with capture pipelines.
    if has_capture_pipeline
        && let Some(tx) = cmd_tx
    {
        let _ = tx.try_send(GstCommand::RemoveCaptureSource { source_id: src_id });
    }
    // Clear selection if we just deleted the selected source.
    if state.selected_source_id == Some(src_id) {
        state.selected_source_id = None;
    }
    // Update capture_active.
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
