use crate::gstreamer::{CaptureSourceConfig, GstCommand};
use crate::scene::{Scene, SceneId, Source, SourceId, SourceProperties, SourceType, Transform};
use crate::state::AppState;
use crate::ui::layout::PanelId;

/// Send `AddCaptureSource` / `RemoveCaptureSource` commands for the delta between two scenes.
///
/// Sources shared between `old_scene` and `new_scene` are untouched.  Sources only in
/// `new_scene` get `AddCaptureSource`; sources only in `old_scene` get `RemoveCaptureSource`.
fn apply_scene_diff(
    cmd_tx: &Option<tokio::sync::mpsc::Sender<GstCommand>>,
    sources: &[Source],
    old_scene: Option<&Scene>,
    new_scene: Option<&Scene>,
) {
    let Some(tx) = cmd_tx else { return };

    let old_ids: std::collections::HashSet<SourceId> = old_scene
        .map(|s| s.sources.iter().copied().collect())
        .unwrap_or_default();
    let new_ids: std::collections::HashSet<SourceId> = new_scene
        .map(|s| s.sources.iter().copied().collect())
        .unwrap_or_default();

    // Remove sources that are no longer in the active scene.
    for &src_id in old_ids.difference(&new_ids) {
        let _ = tx.try_send(GstCommand::RemoveCaptureSource { source_id: src_id });
    }

    // Add sources that are new in the active scene.
    for &src_id in new_ids.difference(&old_ids) {
        if let Some(source) = sources.iter().find(|s| s.id == src_id) {
            let SourceProperties::Display { screen_index } = source.properties;
            let _ = tx.try_send(GstCommand::AddCaptureSource {
                source_id: src_id,
                config: CaptureSourceConfig::Screen { screen_index },
            });
        }
    }
}

/// Send the appropriate capture command for a scene's source, or `StopCapture`
/// if the scene has no display source.
///
/// Used for initial scene setup (first run / scene delete fallback).
/// Starts capture for all sources in the given scene.
fn send_capture_for_scene(
    cmd_tx: &Option<tokio::sync::mpsc::Sender<GstCommand>>,
    sources: &[Source],
    scene: &Scene,
) {
    let Some(tx) = cmd_tx else { return };
    let mut any_started = false;
    for &src_id in &scene.sources {
        if let Some(source) = sources.iter().find(|s| s.id == src_id) {
            let SourceProperties::Display { screen_index } = source.properties;
            let _ = tx.try_send(GstCommand::AddCaptureSource {
                source_id: src_id,
                config: CaptureSourceConfig::Screen { screen_index },
            });
            any_started = true;
        }
    }
    if !any_started {
        let _ = tx.try_send(GstCommand::StopCapture);
    }
}

/// Draw the scene editor panel (scene list, sources, and per-source properties).
pub fn draw(ui: &mut egui::Ui, state: &mut AppState, _panel_id: PanelId) {
    // Clone command_tx early so we can use it without borrowing state.
    let cmd_tx = state.command_tx.clone();

    // ---- Scenes section ----
    ui.horizontal(|ui| {
        ui.heading("Scenes");

        // Add scene
        if ui.button("+").clicked() {
            let new_id = SceneId(state.next_scene_id);
            state.next_scene_id += 1;
            state.scenes.push(Scene {
                id: new_id,
                name: format!("Scene {}", state.scenes.len() + 1),
                sources: Vec::new(),
            });
            state.scenes_dirty = true;
            state.scenes_last_changed = std::time::Instant::now();
        }

        // Delete active scene
        if ui.button("−").clicked()
            && let Some(active_id) = state.active_scene_id
        {
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
            if let Some(scene) = state.scenes.iter().find(|s| s.id == active_id) {
                let src_ids: Vec<SourceId> = scene.sources.clone();
                // Send RemoveCaptureSource for each source being deleted.
                for &src_id in &src_ids {
                    if let Some(ref tx) = cmd_tx {
                        let _ = tx.try_send(GstCommand::RemoveCaptureSource { source_id: src_id });
                    }
                }
                state.sources.retain(|s| !src_ids.contains(&s.id));
            }

            // Remove the scene itself.
            state.scenes.retain(|s| s.id != active_id);

            // Select the first remaining scene.
            let first_scene = state.scenes.first().cloned();
            if let Some(ref scene) = first_scene {
                state.active_scene_id = Some(scene.id);
                send_capture_for_scene(&cmd_tx, &state.sources, scene);
                state.capture_active = !scene.sources.is_empty();
            } else {
                state.active_scene_id = None;
                state.capture_active = false;
            }

            state.scenes_dirty = true;
            state.scenes_last_changed = std::time::Instant::now();
        }
    });

    // Scene list – clicking switches the active scene.
    let mut switch_to: Option<SceneId> = None;
    for scene in &state.scenes {
        let is_selected = state.active_scene_id == Some(scene.id);
        if ui.selectable_label(is_selected, &scene.name).clicked() && !is_selected {
            switch_to = Some(scene.id);
        }
    }

    if let Some(new_id) = switch_to {
        let old_scene = state
            .active_scene_id
            .and_then(|id| state.scenes.iter().find(|s| s.id == id))
            .cloned();
        let new_scene = state.scenes.iter().find(|s| s.id == new_id).cloned();

        state.active_scene_id = Some(new_id);

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

    ui.separator();

    // ---- Sources section (all sources in the active scene) ----
    ui.heading("Sources");

    let Some(active_id) = state.active_scene_id else {
        return;
    };

    // "Add Display Source" button – always visible, adds to the scene's existing sources.
    if ui.button("Add Display Source").clicked() {
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
        if let Some(ref tx) = cmd_tx {
            let _ = tx.try_send(GstCommand::AddCaptureSource {
                source_id: new_src_id,
                config: CaptureSourceConfig::Screen { screen_index: 0 },
            });
        }
        state.capture_active = true;
        state.scenes_dirty = true;
        state.scenes_last_changed = std::time::Instant::now();
    }

    // Collect source IDs in the active scene — clone to avoid borrow conflicts.
    let source_ids: Vec<SourceId> = state
        .scenes
        .iter()
        .find(|s| s.id == active_id)
        .map(|s| s.sources.clone())
        .unwrap_or_default();

    if source_ids.is_empty() {
        return;
    }

    ui.separator();

    // Track any mutations requested during the loop.
    let mut delete_source: Option<SourceId> = None;
    let mut move_up_source: Option<SourceId> = None;
    let mut move_down_source: Option<SourceId> = None;
    let mut scenes_changed = false;

    let source_count = source_ids.len();

    for (idx, &src_id) in source_ids.iter().enumerate() {
        // ---- Per-source row ----
        ui.push_id(src_id.0, |ui| {
            // Header row: visibility checkbox + name edit + move + delete
            ui.horizontal(|ui| {
                // Visibility toggle
                if let Some(source) = state.sources.iter_mut().find(|s| s.id == src_id)
                    && ui.checkbox(&mut source.visible, "").changed()
                {
                    scenes_changed = true;
                }

                // Name edit
                if let Some(source) = state.sources.iter_mut().find(|s| s.id == src_id) {
                    ui.add(egui::TextEdit::singleline(&mut source.name).desired_width(100.0));
                }

                // Move Up (disabled for first item)
                ui.add_enabled_ui(idx > 0, |ui| {
                    if ui.button("▲").clicked() {
                        move_up_source = Some(src_id);
                    }
                });

                // Move Down (disabled for last item)
                ui.add_enabled_ui(idx + 1 < source_count, |ui| {
                    if ui.button("▼").clicked() {
                        move_down_source = Some(src_id);
                    }
                });

                // Delete
                if ui.button("✕").clicked() {
                    delete_source = Some(src_id);
                }
            });

            // Opacity slider
            if let Some(source) = state.sources.iter_mut().find(|s| s.id == src_id) {
                ui.horizontal(|ui| {
                    ui.label("Opacity");
                    if ui
                        .add(egui::Slider::new(&mut source.opacity, 0.0..=1.0))
                        .changed()
                    {
                        scenes_changed = true;
                    }
                });
            }

            // Monitor selector
            {
                let monitor_count = state.monitor_count;
                if let Some(source) = state.sources.iter_mut().find(|s| s.id == src_id) {
                    let SourceProperties::Display {
                        ref mut screen_index,
                    } = source.properties;
                    let prev_index = *screen_index;
                    let selected_label = format!("Monitor {}", *screen_index);
                    egui::ComboBox::from_id_salt(egui::Id::new("monitor_combo").with(src_id.0))
                        .selected_text(&selected_label)
                        .show_ui(ui, |ui| {
                            for i in 0..monitor_count as u32 {
                                let label = format!("Monitor {i}");
                                ui.selectable_value(screen_index, i, label);
                            }
                        });

                    if *screen_index != prev_index {
                        let new_index = *screen_index;
                        if let Some(ref tx) = cmd_tx {
                            let _ = tx.try_send(GstCommand::AddCaptureSource {
                                source_id: src_id,
                                config: CaptureSourceConfig::Screen {
                                    screen_index: new_index,
                                },
                            });
                        }
                        scenes_changed = true;
                    }
                }
            }

            // Transform grid
            ui.label("Transform");
            if let Some(source) = state.sources.iter_mut().find(|s| s.id == src_id) {
                let mut any_changed = false;
                egui::Grid::new(egui::Id::new("source_transform_grid").with(src_id.0))
                    .num_columns(2)
                    .show(ui, |ui| {
                        ui.label("X");
                        if ui
                            .add(egui::DragValue::new(&mut source.transform.x).speed(1.0))
                            .changed()
                        {
                            any_changed = true;
                        }
                        ui.end_row();
                        ui.label("Y");
                        if ui
                            .add(egui::DragValue::new(&mut source.transform.y).speed(1.0))
                            .changed()
                        {
                            any_changed = true;
                        }
                        ui.end_row();
                        ui.label("Width");
                        if ui
                            .add(egui::DragValue::new(&mut source.transform.width).speed(1.0))
                            .changed()
                        {
                            any_changed = true;
                        }
                        ui.end_row();
                        ui.label("Height");
                        if ui
                            .add(egui::DragValue::new(&mut source.transform.height).speed(1.0))
                            .changed()
                        {
                            any_changed = true;
                        }
                        ui.end_row();
                    });
                if any_changed {
                    scenes_changed = true;
                }
            }

            // Visual separator between sources
            if idx + 1 < source_count {
                ui.separator();
            }
        });
    }

    // Apply reorder mutations.
    if let Some(src_id) = move_up_source {
        if let Some(scene) = state.scenes.iter_mut().find(|s| s.id == active_id) {
            scene.move_source_up(src_id);
        }
        scenes_changed = true;
    }
    if let Some(src_id) = move_down_source {
        if let Some(scene) = state.scenes.iter_mut().find(|s| s.id == active_id) {
            scene.move_source_down(src_id);
        }
        scenes_changed = true;
    }

    // Apply delete mutation.
    if let Some(src_id) = delete_source {
        if let Some(scene) = state.scenes.iter_mut().find(|s| s.id == active_id) {
            scene.sources.retain(|&id| id != src_id);
        }
        state.sources.retain(|s| s.id != src_id);
        if let Some(ref tx) = cmd_tx {
            let _ = tx.try_send(GstCommand::RemoveCaptureSource { source_id: src_id });
        }
        // Update capture_active based on whether any sources remain.
        let has_sources = state
            .scenes
            .iter()
            .find(|s| s.id == active_id)
            .map(|s| !s.sources.is_empty())
            .unwrap_or(false);
        if !has_sources && let Some(ref tx) = cmd_tx {
            let _ = tx.try_send(GstCommand::StopCapture);
        }
        state.capture_active = has_sources;
        scenes_changed = true;
    }

    if scenes_changed {
        state.scenes_dirty = true;
        state.scenes_last_changed = std::time::Instant::now();
    }
}
