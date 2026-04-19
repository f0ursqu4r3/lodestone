use crate::gstreamer::{AudioLevels, GstCommand};
use crate::scene::{AudioInput, SourceId, SourceProperties, SourceType};
use crate::state::AppState;
use crate::ui::layout::PanelId;
use crate::ui::theme::{Theme, active_theme};

struct AudioStripData {
    source_id: SourceId,
    name: String,
    input_summary: String,
    detail_summary: String,
    volume: f32,
    muted: bool,
    volume_overridden: bool,
    muted_overridden: bool,
    levels: Option<AudioLevels>,
}

/// Draw the audio panel for the current scene.
pub fn draw(ui: &mut egui::Ui, state: &mut AppState, _panel_id: PanelId) {
    let theme = active_theme(ui.ctx());
    let panel_rect = ui.available_rect_before_wrap();
    ui.painter().rect_filled(panel_rect, 0.0, theme.bg_panel);

    let editing_id = egui::Id::new("audio_panel_editing");
    let was_editing: bool = ui.memory(|m| m.data.get_temp(editing_id).unwrap_or(false));
    if was_editing {
        state.begin_continuous_edit();
    }

    let scene_name = state
        .active_scene()
        .map(|scene| scene.name.clone())
        .unwrap_or_else(|| "No Scene".to_string());
    let strips = collect_scene_audio_strips(state);
    let mut changed = false;

    ui.vertical(|ui| {
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
        ui.add_space(10.0);

        if strips.is_empty() {
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new("No audio sources in this scene")
                    .size(11.0)
                    .color(theme.text_muted),
            );
            ui.label(
                egui::RichText::new("Add an Audio source to the scene to mix it here.")
                    .size(10.0)
                    .color(theme.text_secondary),
            );
            return;
        }

        egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for strip in &strips {
                        changed |= draw_audio_strip(ui, state, strip, &theme);
                        ui.add_space(8.0);
                    }
                });
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

fn collect_scene_audio_strips(state: &AppState) -> Vec<AudioStripData> {
    let Some(scene) = state.active_scene() else {
        return Vec::new();
    };

    scene
        .sources
        .iter()
        .filter_map(|scene_source| {
            let source = state.library.iter().find(|source| {
                source.id == scene_source.source_id
                    && matches!(source.source_type, SourceType::Audio)
            })?;
            let (input_summary, detail_summary) = match &source.properties {
                SourceProperties::Audio { input } => describe_audio_input(input),
                _ => return None,
            };
            Some(AudioStripData {
                source_id: scene_source.source_id,
                name: source.name.clone(),
                input_summary,
                detail_summary,
                volume: scene_source.resolve_volume(source),
                muted: scene_source.resolve_muted(source),
                volume_overridden: scene_source.is_volume_overridden(),
                muted_overridden: scene_source.is_muted_overridden(),
                levels: state
                    .audio_levels
                    .source_levels
                    .get(&scene_source.source_id)
                    .cloned(),
            })
        })
        .collect()
}

fn draw_audio_strip(
    ui: &mut egui::Ui,
    state: &mut AppState,
    strip: &AudioStripData,
    theme: &Theme,
) -> bool {
    let Some(lib_idx) = state.library.iter().position(|source| source.id == strip.source_id) else {
        return false;
    };

    let mut volume = strip.volume;
    let mut muted = strip.muted;
    let mut changed = false;

    egui::Frame::new()
        .fill(theme.bg_elevated)
        .stroke(egui::Stroke::new(1.0, theme.border))
        .corner_radius(egui::CornerRadius::same(theme.radius_md as u8))
        .inner_margin(egui::Margin::same(10))
        .show(ui, |ui| {
            ui.set_min_height(84.0);
            ui.horizontal(|ui| {
                ui.set_width(ui.available_width());

                ui.vertical(|ui| {
                    ui.set_min_width(170.0);
                    ui.label(
                        egui::RichText::new(&strip.name)
                            .size(12.0)
                            .color(theme.text_primary),
                    );
                    ui.label(
                        egui::RichText::new(&strip.input_summary)
                            .size(10.0)
                            .color(theme.text_secondary),
                    );
                    ui.label(
                        egui::RichText::new(&strip.detail_summary)
                            .size(10.0)
                            .color(theme.text_muted),
                    );
                    if strip.volume_overridden || strip.muted_overridden {
                        ui.add_space(4.0);
                        ui.label(
                            egui::RichText::new("SCENE OVERRIDE")
                                .size(9.0)
                                .color(theme.accent),
                        );
                    }
                });

                ui.add_space(10.0);

                ui.vertical(|ui| {
                    ui.set_min_width(190.0);
                    draw_vu_meter(ui, theme, strip.levels.as_ref(), muted);
                    ui.add_space(4.0);
                    let peak_db = strip
                        .levels
                        .as_ref()
                        .map(|levels| format!("{:.0} dB", levels.peak_db))
                        .unwrap_or_else(|| "-inf dB".to_string());
                    ui.label(
                        egui::RichText::new(peak_db)
                            .size(10.0)
                            .color(theme.text_muted),
                    );
                });

                ui.add_space(12.0);

                ui.vertical(|ui| {
                    ui.set_min_width(150.0);
                    ui.label(
                        egui::RichText::new("Volume")
                            .size(10.0)
                            .color(theme.text_secondary),
                    );
                    let response = ui.add_sized(
                        [150.0, 18.0],
                        egui::Slider::new(&mut volume, 0.0..=2.0).show_value(false),
                    );
                    if response.drag_started() {
                        state.begin_continuous_edit();
                    }
                    if response.changed() {
                        apply_volume_override(state, strip.source_id, volume);
                        changed = true;
                    }
                    ui.label(
                        egui::RichText::new(format!("{:.0}%", volume * 100.0))
                            .size(10.0)
                            .color(theme.text_muted),
                    );
                });

                ui.add_space(12.0);

                ui.vertical(|ui| {
                    let mute_fill = if muted {
                        theme.danger
                    } else {
                        theme.bg_panel
                    };
                    let mute_text = if muted { "Muted" } else { "Mute" };
                    if ui
                        .add_sized(
                            [72.0, 28.0],
                            egui::Button::new(
                                egui::RichText::new(mute_text).color(if muted {
                                    theme.bg_base
                                } else {
                                    theme.text_primary
                                }),
                            )
                            .fill(mute_fill)
                            .stroke(egui::Stroke::new(1.0, theme.border)),
                        )
                        .clicked()
                    {
                        muted = !muted;
                        apply_mute_override(state, strip.source_id, muted);
                        changed = true;
                    }

                    ui.add_space(6.0);

                    ui.horizontal(|ui| {
                        if strip.volume_overridden
                            && ui
                                .small_button(egui_phosphor::regular::ARROW_COUNTER_CLOCKWISE)
                                .on_hover_text("Reset to library volume")
                                .clicked()
                        {
                            let library_volume = state.library[lib_idx].volume;
                            reset_volume_override(state, strip.source_id, library_volume);
                            changed = true;
                        }

                        if strip.muted_overridden
                            && ui
                                .small_button(egui_phosphor::regular::ARROW_COUNTER_CLOCKWISE)
                                .on_hover_text("Reset to library mute state")
                                .clicked()
                        {
                            let library_muted = state.library[lib_idx].muted;
                            reset_mute_override(state, strip.source_id, library_muted);
                            changed = true;
                        }
                    });
                });
            });
        });

    changed
}

fn draw_vu_meter(
    ui: &mut egui::Ui,
    theme: &Theme,
    levels: Option<&AudioLevels>,
    muted: bool,
) {
    let desired_size = egui::vec2(190.0, 22.0);
    let (rect, _) = ui.allocate_exact_size(desired_size, egui::Sense::hover());
    let painter = ui.painter_at(rect);

    painter.rect_filled(rect, 4.0, theme.bg_panel);
    painter.rect_stroke(
        rect,
        4.0,
        egui::Stroke::new(1.0, theme.border_subtle),
        egui::StrokeKind::Inside,
    );

    let rms = levels.map(|level| db_to_meter(level.rms_db)).unwrap_or(0.0);
    let peak = levels.map(|level| db_to_meter(level.peak_db)).unwrap_or(0.0);
    let segments = 20;
    let gap = 2.0;
    let segment_width = ((rect.width() - gap * (segments as f32 - 1.0)) / segments as f32).max(1.0);

    for idx in 0..segments {
        let x = rect.left() + idx as f32 * (segment_width + gap);
        let seg_rect = egui::Rect::from_min_size(
            egui::pos2(x, rect.top() + 3.0),
            egui::vec2(segment_width, rect.height() - 6.0),
        );
        let fill_threshold = (idx + 1) as f32 / segments as f32;
        let color = meter_color(theme, fill_threshold);
        let fill = if muted || fill_threshold > rms {
            theme.border_subtle
        } else {
            color
        };
        painter.rect_filled(seg_rect, 2.0, fill);
    }

    if !muted && peak > 0.0 {
        let marker_x = rect.left() + peak * rect.width();
        painter.line_segment(
            [
                egui::pos2(marker_x, rect.top() + 2.0),
                egui::pos2(marker_x, rect.bottom() - 2.0),
            ],
            egui::Stroke::new(1.0, theme.text_primary),
        );
    }
}

fn meter_color(theme: &Theme, level: f32) -> egui::Color32 {
    if level >= 0.9 {
        theme.danger
    } else if level >= 0.7 {
        theme.warning
    } else {
        theme.success
    }
}

fn db_to_meter(db: f32) -> f32 {
    ((db + 60.0) / 60.0).clamp(0.0, 1.0)
}

fn apply_volume_override(state: &mut AppState, source_id: SourceId, volume: f32) {
    if let Some(scene) = state.active_scene_mut()
        && let Some(scene_source) = scene.find_source_mut(source_id)
    {
        scene_source.overrides.volume = Some(volume);
    }
    if let Some(ref tx) = state.command_tx {
        let _ = tx.try_send(GstCommand::SetSourceVolume { source_id, volume });
    }
}

fn reset_volume_override(state: &mut AppState, source_id: SourceId, library_volume: f32) {
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
}

fn apply_mute_override(state: &mut AppState, source_id: SourceId, muted: bool) {
    if let Some(scene) = state.active_scene_mut()
        && let Some(scene_source) = scene.find_source_mut(source_id)
    {
        scene_source.overrides.muted = Some(muted);
    }
    if let Some(ref tx) = state.command_tx {
        let _ = tx.try_send(GstCommand::SetSourceMuted { source_id, muted });
    }
}

fn reset_mute_override(state: &mut AppState, source_id: SourceId, library_muted: bool) {
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
            ("Audio File".to_string(), format!("{filename} - {behavior}"))
        }
    }
}
