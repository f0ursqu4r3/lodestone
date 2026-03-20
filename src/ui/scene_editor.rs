use crate::obs::SourceId;
use crate::state::AppState;

pub fn draw(ctx: &egui::Context, state: &mut AppState) {
    if !state.ui_state.scene_panel_open {
        return;
    }

    egui::SidePanel::left("scene_editor")
        .exact_width(220.0)
        .show(ctx, |ui| {
            // ---- Scenes section ----
            ui.horizontal(|ui| {
                ui.heading("Scenes");
                if ui.button("⚙").on_hover_text("Open Settings").clicked() {
                    state.ui_state.settings_modal_open = true;
                }
                if ui.button("+").clicked() {
                    let new_id = crate::obs::SceneId(
                        state.scenes.iter().map(|s| s.id.0).max().unwrap_or(0) + 1,
                    );
                    state.scenes.push(crate::obs::Scene {
                        id: new_id,
                        name: format!("Scene {}", state.scenes.len() + 1),
                        sources: Vec::new(),
                    });
                }
            });

            let mut new_active: Option<crate::obs::SceneId> = state.active_scene_id;
            for scene in &state.scenes {
                let is_selected = state.active_scene_id == Some(scene.id);
                if ui.selectable_label(is_selected, &scene.name).clicked() {
                    new_active = Some(scene.id);
                }
            }
            state.active_scene_id = new_active;

            ui.separator();

            // ---- Sources section ----
            ui.horizontal(|ui| {
                ui.heading("Sources");
                if ui.button("+").clicked()
                    && let Some(active_id) = state.active_scene_id
                {
                    let new_src_id =
                        SourceId(state.sources.iter().map(|s| s.id.0).max().unwrap_or(0) + 1);
                    let new_source = crate::obs::Source {
                        id: new_src_id,
                        name: format!("Source {}", state.sources.len() + 1),
                        source_type: crate::obs::SourceType::Display,
                        transform: crate::obs::Transform::new(0.0, 0.0, 1920.0, 1080.0),
                        visible: true,
                        muted: false,
                        volume: 1.0,
                    };
                    state.sources.push(new_source);
                    if let Some(scene) = state.scenes.iter_mut().find(|s| s.id == active_id) {
                        scene.sources.push(new_src_id);
                    }
                }
            });

            // Gather source ids for active scene
            let scene_source_ids: Vec<SourceId> = state
                .active_scene_id
                .and_then(|id| state.scenes.iter().find(|s| s.id == id))
                .map(|s| s.sources.clone())
                .unwrap_or_default();

            // Track selected source via a local egui memory key
            let selected_source_id: Option<SourceId> = {
                let mem =
                    ui.memory(|m| m.data.get_temp::<u64>(egui::Id::new("selected_source_id")));
                mem.map(SourceId)
            };

            let mut new_selected = selected_source_id;

            for src_id in &scene_source_ids {
                if let Some(source) = state.sources.iter().find(|s| s.id == *src_id) {
                    let type_label = match source.source_type {
                        crate::obs::SourceType::Display => "Display",
                        crate::obs::SourceType::Window => "Window",
                        crate::obs::SourceType::Camera => "Camera",
                        crate::obs::SourceType::Audio => "Audio",
                        crate::obs::SourceType::Image => "Image",
                        crate::obs::SourceType::Browser => "Browser",
                    };
                    let label_text = format!("{} ({})", source.name, type_label);
                    let is_sel = selected_source_id == Some(*src_id);
                    if ui.selectable_label(is_sel, label_text).clicked() {
                        new_selected = Some(*src_id);
                    }
                }
            }

            // Persist selection
            if new_selected != selected_source_id {
                ui.memory_mut(|m| {
                    if let Some(sid) = new_selected {
                        m.data
                            .insert_temp(egui::Id::new("selected_source_id"), sid.0);
                    } else {
                        m.data.remove::<u64>(egui::Id::new("selected_source_id"));
                    }
                });
            }

            // Transform controls for the selected source
            if let Some(sel_id) = new_selected
                && scene_source_ids.contains(&sel_id)
                && let Some(source) = state.sources.iter_mut().find(|s| s.id == sel_id)
            {
                ui.separator();
                ui.label("Transform");
                egui::Grid::new("transform_grid")
                    .num_columns(2)
                    .show(ui, |ui| {
                        ui.label("X");
                        ui.add(egui::DragValue::new(&mut source.transform.x).speed(1.0));
                        ui.end_row();
                        ui.label("Y");
                        ui.add(egui::DragValue::new(&mut source.transform.y).speed(1.0));
                        ui.end_row();
                        ui.label("Width");
                        ui.add(egui::DragValue::new(&mut source.transform.width).speed(1.0));
                        ui.end_row();
                        ui.label("Height");
                        ui.add(egui::DragValue::new(&mut source.transform.height).speed(1.0));
                        ui.end_row();
                    });
            }
        });
}
