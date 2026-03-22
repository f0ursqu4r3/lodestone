use crate::scene::SourceId;
use crate::state::AppState;
use crate::ui::layout::PanelId;

pub fn draw(ui: &mut egui::Ui, state: &mut AppState, panel_id: PanelId) {
    // ---- Scenes section ----
    ui.horizontal(|ui| {
        ui.heading("Scenes");
        if ui.button("+").clicked() {
            let new_id =
                crate::scene::SceneId(state.scenes.iter().map(|s| s.id.0).max().unwrap_or(0) + 1);
            state.scenes.push(crate::scene::Scene {
                id: new_id,
                name: format!("Scene {}", state.scenes.len() + 1),
                sources: Vec::new(),
            });
        }
    });

    let mut new_active: Option<crate::scene::SceneId> = state.active_scene_id;
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
            let new_src_id = SourceId(state.sources.iter().map(|s| s.id.0).max().unwrap_or(0) + 1);
            let new_source = crate::scene::Source {
                id: new_src_id,
                name: format!("Source {}", state.sources.len() + 1),
                source_type: crate::scene::SourceType::Display,
                properties: crate::scene::SourceProperties::default(),
                transform: crate::scene::Transform::new(0.0, 0.0, 1920.0, 1080.0),
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
        let mem = ui.memory(|m| {
            m.data
                .get_temp::<u64>(egui::Id::new(("selected_source_id", panel_id.0)))
        });
        mem.map(SourceId)
    };

    let mut new_selected = selected_source_id;

    for src_id in &scene_source_ids {
        if let Some(source) = state.sources.iter().find(|s| s.id == *src_id) {
            let type_label = match source.source_type {
                crate::scene::SourceType::Display => "Display",
                crate::scene::SourceType::Window => "Window",
                crate::scene::SourceType::Camera => "Camera",
                crate::scene::SourceType::Audio => "Audio",
                crate::scene::SourceType::Image => "Image",
                crate::scene::SourceType::Browser => "Browser",
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
                    .insert_temp(egui::Id::new(("selected_source_id", panel_id.0)), sid.0);
            } else {
                m.data
                    .remove::<u64>(egui::Id::new(("selected_source_id", panel_id.0)));
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
        egui::Grid::new(egui::Id::new(("transform_grid", panel_id.0)))
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
}
