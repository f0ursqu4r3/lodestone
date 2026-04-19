use crate::gstreamer::GstCommand;
use crate::scene::{AudioInput, SourceId, SourceProperties, SourceType};
use crate::state::AppState;
use crate::ui::layout::PanelId;
use crate::ui::theme::active_theme;

/// Draw the audio panel for the current scene.
///
/// Shows audio sources that are present in the active scene, along with the
/// source input settings and scene-effective mix controls.
pub fn draw(ui: &mut egui::Ui, state: &mut AppState, _panel_id: PanelId) {
    let theme = active_theme(ui.ctx());
    let panel_rect = ui.available_rect_before_wrap();
    ui.painter().rect_filled(panel_rect, 0.0, theme.bg_panel);

    let editing_id = egui::Id::new("audio_panel_editing");
    let was_editing: bool = ui.memory(|m| m.data.get_temp(editing_id).unwrap_or(false));
    if was_editing {
        state.begin_continuous_edit();
    }

    let mut changed = false;

    ui.vertical(|ui| {
        let scene_audio_ids = active_scene_audio_source_ids(state);
        let scene_name = state
            .active_scene()
            .map(|scene| scene.name.clone())
            .unwrap_or_else(|| "No Scene".to_string());

        ui.label(
            egui::RichText::new("CURRENT SCENE")
                .size(9.0)
                .color(theme.text_muted),
        );
        ui.label(
            egui::RichText::new(scene_name)
                .size(13.0)
                .color(theme.text_primary),
        );
        ui.add_space(8.0);

        if scene_audio_ids.is_empty() {
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new("No audio sources in this scene")
                    .size(11.0)
                    .color(theme.text_muted),
            );
            ui.label(
                egui::RichText::new("Add an Audio source to the scene to control it here.")
                    .size(10.0)
                    .color(theme.text_secondary),
            );
        } else {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for source_id in scene_audio_ids {
                        changed |= draw_scene_audio_source(ui, state, source_id);
                        ui.add_space(8.0);
                    }
                });
        }
    });

    if changed {
        state.mark_dirty();
    }

    let still_editing = changed || (was_editing && ui.ctx().is_using_pointer());
    if was_editing && !still_editing {
        state.end_continuous_edit();
    }
    ui.memory_mut(|m| m.data.insert_temp(editing_id, still_editing));
}

fn active_scene_audio_source_ids(state: &AppState) -> Vec<SourceId> {
    let Some(scene) = state.active_scene() else {
        return Vec::new();
    };

    scene
        .sources
        .iter()
        .filter_map(|scene_source| {
            state
                .library
                .iter()
                .find(|source| source.id == scene_source.source_id)
                .filter(|source| matches!(source.source_type, SourceType::Audio))
                .map(|_| scene_source.source_id)
        })
        .collect()
}

fn draw_scene_audio_source(ui: &mut egui::Ui, state: &mut AppState, source_id: SourceId) -> bool {
    let theme = active_theme(ui.ctx());

    let Some(lib_idx) = state.library.iter().position(|source| source.id == source_id) else {
        return false;
    };

    let (name, input_summary, detail_summary, mut volume, mut muted, volume_overridden, muted_overridden) = {
        let source = &state.library[lib_idx];
        let Some(scene_source) = state.active_scene().and_then(|scene| scene.find_source(source_id)) else {
            return false;
        };

        let (input_summary, detail_summary) = match &source.properties {
            SourceProperties::Audio { input } => describe_audio_input(input),
            _ => return false,
        };

        (
            source.name.clone(),
            input_summary,
            detail_summary,
            scene_source.resolve_volume(source),
            scene_source.resolve_muted(source),
            scene_source.is_volume_overridden(),
            scene_source.is_muted_overridden(),
        )
    };

    let mut changed = false;

    egui::Frame::new()
        .fill(theme.bg_elevated)
        .stroke(egui::Stroke::new(1.0, theme.border))
        .corner_radius(egui::CornerRadius::same(theme.radius_md as u8))
        .inner_margin(egui::Margin::same(10))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(&name)
                        .size(12.0)
                        .color(theme.text_primary),
                );
                if volume_overridden || muted_overridden {
                    ui.label(
                        egui::RichText::new("SCENE OVERRIDE")
                            .size(9.0)
                            .color(theme.accent),
                    );
                }
            });

            ui.label(
                egui::RichText::new(input_summary)
                    .size(10.0)
                    .color(theme.text_secondary),
            );
            ui.label(
                egui::RichText::new(detail_summary)
                    .size(10.0)
                    .color(theme.text_muted),
            );

            ui.add_space(8.0);

            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Volume")
                        .size(10.0)
                        .color(theme.text_secondary),
                );

                let response =
                    ui.add(egui::Slider::new(&mut volume, 0.0..=2.0).suffix("x"));
                if response.drag_started() {
                    state.begin_continuous_edit();
                }
                if response.changed() {
                    if let Some(scene) = state.active_scene_mut()
                        && let Some(scene_source) = scene.find_source_mut(source_id)
                    {
                        scene_source.overrides.volume = Some(volume);
                    }
                    if let Some(ref tx) = state.command_tx {
                        let _ = tx.try_send(GstCommand::SetSourceVolume { source_id, volume });
                    }
                    changed = true;
                }

                if volume_overridden
                    && ui
                        .small_button(egui_phosphor::regular::ARROW_COUNTER_CLOCKWISE)
                        .on_hover_text("Reset to library volume")
                        .clicked()
                {
                    let library_volume = state.library[lib_idx].volume;
                    if let Some(scene) = state.active_scene_mut()
                        && let Some(scene_source) = scene.find_source_mut(source_id)
                    {
                        scene_source.overrides.volume = None;
                    }
                    if let Some(ref tx) = state.command_tx {
                        let _ = tx.try_send(GstCommand::SetSourceVolume {
                            source_id,
                            volume: library_volume,
                        });
                    }
                    changed = true;
                }
            });

            let mute_label = if muted { "Muted" } else { "Live" };
            ui.horizontal(|ui| {
                let prev_muted = muted;
                if ui.checkbox(&mut muted, "Mute").changed() && muted != prev_muted {
                    if let Some(scene) = state.active_scene_mut()
                        && let Some(scene_source) = scene.find_source_mut(source_id)
                    {
                        scene_source.overrides.muted = Some(muted);
                    }
                    if let Some(ref tx) = state.command_tx {
                        let _ = tx.try_send(GstCommand::SetSourceMuted { source_id, muted });
                    }
                    changed = true;
                }

                ui.label(
                    egui::RichText::new(mute_label)
                        .size(10.0)
                        .color(if muted {
                            theme.danger
                        } else {
                            theme.text_secondary
                        }),
                );

                if muted_overridden
                    && ui
                        .small_button(egui_phosphor::regular::ARROW_COUNTER_CLOCKWISE)
                        .on_hover_text("Reset to library mute state")
                        .clicked()
                {
                    let library_muted = state.library[lib_idx].muted;
                    if let Some(scene) = state.active_scene_mut()
                        && let Some(scene_source) = scene.find_source_mut(source_id)
                    {
                        scene_source.overrides.muted = None;
                    }
                    if let Some(ref tx) = state.command_tx {
                        let _ = tx.try_send(GstCommand::SetSourceMuted {
                            source_id,
                            muted: library_muted,
                        });
                    }
                    changed = true;
                }
            });
        });

    changed
}

fn describe_audio_input(input: &AudioInput) -> (String, String) {
    match input {
        AudioInput::Device {
            device_uid: _,
            device_name,
        } => {
            let name = if device_name.is_empty() {
                "No device selected".to_string()
            } else {
                device_name.clone()
            };
            ("Device Input".to_string(), name)
        }
        AudioInput::File { path, looping } => {
            let filename = std::path::Path::new(path)
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("No file selected")
                .to_string();
            let behavior = if *looping { "Looping" } else { "Play once" };
            ("Audio File".to_string(), format!("{filename} · {behavior}"))
        }
    }
}
