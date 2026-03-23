use crate::gstreamer::{CaptureSourceConfig, GstCommand};
use crate::scene::{Scene, SceneId, Source, SourceId, SourceProperties, SourceType, Transform};
use crate::state::AppState;
use crate::ui::layout::PanelId;

/// Send the appropriate capture command for a scene's source, or `StopCapture`
/// if the scene has no display source.
fn send_capture_for_scene(
    cmd_tx: &Option<tokio::sync::mpsc::Sender<GstCommand>>,
    sources: &[Source],
    scene: &Scene,
) {
    let Some(tx) = cmd_tx else { return };
    if let Some(&src_id) = scene.sources.first()
        && let Some(source) = sources.iter().find(|s| s.id == src_id)
    {
        let SourceProperties::Display { screen_index } = source.properties;
        let _ = tx.try_send(GstCommand::SetCaptureSource(CaptureSourceConfig::Screen {
            screen_index,
        }));
        return;
    }
    let _ = tx.try_send(GstCommand::StopCapture);
}

/// Draw the scene editor panel (scene list, source, and source properties).
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
        state.active_scene_id = Some(new_id);
        if let Some(scene) = state.scenes.iter().find(|s| s.id == new_id).cloned() {
            send_capture_for_scene(&cmd_tx, &state.sources, &scene);
            state.capture_active = !scene.sources.is_empty();
        }
        state.scenes_dirty = true;
        state.scenes_last_changed = std::time::Instant::now();
    }

    ui.separator();

    // ---- Sources section (one source per scene) ----
    ui.heading("Sources");

    let Some(active_id) = state.active_scene_id else {
        return;
    };

    // Find the single source id for the active scene (if any).
    let source_id: Option<SourceId> = state
        .scenes
        .iter()
        .find(|s| s.id == active_id)
        .and_then(|s| s.sources.first().copied());

    if source_id.is_none() {
        // No source yet – offer to add one.
        if ui.button("Add Display Source").clicked() {
            let new_src_id = SourceId(state.next_source_id);
            state.next_source_id += 1;
            let new_source = Source {
                id: new_src_id,
                name: "Display".to_string(),
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
                let _ = tx.try_send(GstCommand::SetCaptureSource(CaptureSourceConfig::Screen {
                    screen_index: 0,
                }));
            }
            state.capture_active = true;
            state.scenes_dirty = true;
            state.scenes_last_changed = std::time::Instant::now();
        }
        return;
    }

    let Some(src_id) = source_id else { return };

    // Delete source button
    if ui.button("Delete Source").clicked() {
        // Remove from the scene's source list.
        if let Some(scene) = state.scenes.iter_mut().find(|s| s.id == active_id) {
            scene.sources.retain(|&id| id != src_id);
        }
        // Remove from the global sources list.
        state.sources.retain(|s| s.id != src_id);
        if let Some(ref tx) = cmd_tx {
            let _ = tx.try_send(GstCommand::StopCapture);
        }
        state.capture_active = false;
        state.scenes_dirty = true;
        state.scenes_last_changed = std::time::Instant::now();
        return;
    }

    ui.separator();

    // ---- Source properties ----

    // Name
    {
        let Some(source) = state.sources.iter_mut().find(|s| s.id == src_id) else {
            return;
        };
        ui.label("Name");
        ui.text_edit_singleline(&mut source.name);
    }

    // Visible
    {
        let Some(source) = state.sources.iter_mut().find(|s| s.id == src_id) else {
            return;
        };
        if ui.checkbox(&mut source.visible, "Visible").changed() {
            state.scenes_dirty = true;
            state.scenes_last_changed = std::time::Instant::now();
        }
    }

    // Monitor selector
    {
        let Some(source) = state.sources.iter_mut().find(|s| s.id == src_id) else {
            return;
        };
        let SourceProperties::Display {
            ref mut screen_index,
        } = source.properties;
        let prev_index = *screen_index;
        let monitor_count = state.monitor_count;
        let selected_label = format!("Monitor {}", *screen_index);
        egui::ComboBox::from_label("Monitor")
            .selected_text(&selected_label)
            .show_ui(ui, |ui| {
                for i in 0..monitor_count as u32 {
                    let label = format!("Monitor {i}");
                    ui.selectable_value(screen_index, i, label);
                }
            });

        if *screen_index != prev_index {
            if let Some(ref tx) = cmd_tx {
                let _ = tx.try_send(GstCommand::SetCaptureSource(CaptureSourceConfig::Screen {
                    screen_index: *screen_index,
                }));
            }
            state.scenes_dirty = true;
            state.scenes_last_changed = std::time::Instant::now();
        }
    }

    // Transform grid
    ui.label("Transform");
    {
        let Some(source) = state.sources.iter_mut().find(|s| s.id == src_id) else {
            return;
        };
        let mut any_changed = false;
        egui::Grid::new("source_transform_grid")
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
            state.scenes_dirty = true;
            state.scenes_last_changed = std::time::Instant::now();
        }
    }
}
