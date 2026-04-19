use crate::gstreamer::{AudioLevels, GstCommand};
use crate::scene::{AudioInput, SourceId, SourceProperties, SourceType};
use crate::state::AppState;
use crate::ui::layout::PanelId;
use crate::ui::theme::{Theme, active_theme};

const ROW_PADDING_X: f32 = 12.0;
const ROW_SPACING: f32 = 12.0;
const METER_HEIGHT: f32 = 10.0;
const SCALE_HEIGHT: f32 = 11.0;
const FADER_HEIGHT: f32 = 14.0;
const BUTTON_SIZE: egui::Vec2 = egui::vec2(24.0, 20.0);

struct AudioStripData {
    source_id: SourceId,
    name: String,
    #[allow(dead_code)]
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

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.add_space(10.0);
            draw_panel_header(ui, &theme, &scene_name);
            ui.add_space(12.0);

            if strips.is_empty() {
                draw_empty_state(ui, &theme);
                return;
            }

            let mut first = true;
            for strip in &strips {
                if !first {
                    ui.add_space(ROW_SPACING);
                }
                first = false;
                changed |= draw_audio_strip(ui, state, strip, &theme);
            }
            ui.add_space(10.0);
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

fn draw_panel_header(ui: &mut egui::Ui, theme: &Theme, scene_name: &str) {
    ui.horizontal(|ui| {
        ui.add_space(ROW_PADDING_X);
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
        });
    });
}

fn draw_empty_state(ui: &mut egui::Ui, theme: &Theme) {
    ui.horizontal(|ui| {
        ui.add_space(ROW_PADDING_X);
        ui.vertical(|ui| {
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
        });
    });
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
            let (_input_summary, detail_summary) = match &source.properties {
                SourceProperties::Audio { input } => describe_audio_input(input),
                _ => return None,
            };
            Some(AudioStripData {
                source_id: scene_source.source_id,
                name: source.name.clone(),
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
    let Some(lib_idx) = state
        .library
        .iter()
        .position(|source| source.id == strip.source_id)
    else {
        return false;
    };

    let mut volume = strip.volume;
    let mut muted = strip.muted;
    let mut changed = false;
    let overridden = strip.volume_overridden || strip.muted_overridden;

    ui.horizontal(|ui| {
        ui.add_space(ROW_PADDING_X);
        ui.vertical(|ui| {
            let content_width = (ui.available_width() - ROW_PADDING_X).max(80.0);
            ui.set_min_width(content_width);
            ui.set_max_width(content_width);

            // Header: name + optional SCENE tag on the left; dB readout,
            // reset (if overridden), mute on the right.
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(&strip.name)
                        .size(12.0)
                        .color(theme.text_primary),
                );
                if overridden {
                    ui.label(
                        egui::RichText::new("SCENE")
                            .size(9.0)
                            .color(theme.accent),
                    );
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let speaker_icon = if muted {
                        egui_phosphor::regular::SPEAKER_NONE
                    } else {
                        egui_phosphor::regular::SPEAKER_HIGH
                    };
                    let mute_response = ui
                        .add_sized(
                            BUTTON_SIZE,
                            egui::Button::new(
                                egui::RichText::new(speaker_icon).size(13.0).color(
                                    if muted {
                                        theme.bg_base
                                    } else {
                                        theme.text_primary
                                    },
                                ),
                            )
                            .fill(if muted { theme.danger } else { theme.bg_panel })
                            .stroke(egui::Stroke::new(1.0, theme.border)),
                        )
                        .on_hover_text(if muted { "Unmute" } else { "Mute" });
                    if mute_response.clicked() {
                        muted = !muted;
                        apply_mute_override(state, strip.source_id, muted);
                        changed = true;
                    }

                    if overridden {
                        ui.add_space(4.0);
                        let reset_response = ui
                            .add_sized(
                                BUTTON_SIZE,
                                egui::Button::new(
                                    egui::RichText::new(
                                        egui_phosphor::regular::ARROW_COUNTER_CLOCKWISE,
                                    )
                                    .size(12.0)
                                    .color(theme.text_primary),
                                )
                                .fill(theme.border_subtle)
                                .stroke(egui::Stroke::new(1.0, theme.border)),
                            )
                            .on_hover_text("Reset scene overrides");
                        if reset_response.clicked() {
                            if strip.volume_overridden {
                                let library_volume = state.library[lib_idx].volume;
                                reset_volume_override(state, strip.source_id, library_volume);
                            }
                            if strip.muted_overridden {
                                let library_muted = state.library[lib_idx].muted;
                                reset_mute_override(state, strip.source_id, library_muted);
                            }
                            changed = true;
                        }
                    }

                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new(format_level_readout(strip.levels.as_ref(), muted))
                            .size(11.0)
                            .monospace()
                            .color(if muted {
                                theme.text_muted
                            } else {
                                theme.text_primary
                            }),
                    );
                });
            });

            ui.add_space(4.0);

            // Meter, scale, fader take the full content width. Reserve a
            // small inset on each end so the fader thumb doesn't get clipped
            // by the track edges and scale labels at 0/-60 don't get cut off.
            let track_inset = 6.0;
            let track_width = (ui.available_width() - track_inset * 2.0).max(40.0);

            draw_horizontal_vu_meter(
                ui,
                theme,
                strip.levels.as_ref(),
                muted,
                egui::vec2(track_width, METER_HEIGHT),
                track_inset,
            );
            draw_horizontal_db_scale(
                ui,
                theme,
                muted,
                egui::vec2(track_width, SCALE_HEIGHT),
                track_inset,
            );
            let fader_response = draw_horizontal_fader(
                ui,
                theme,
                &mut volume,
                muted,
                egui::vec2(track_width, FADER_HEIGHT),
                track_inset,
            );
            if fader_response.drag_started() {
                state.begin_continuous_edit();
            }
            if fader_response.changed() {
                apply_volume_override(state, strip.source_id, volume);
                changed = true;
            }
        });
        ui.add_space(ROW_PADDING_X);
    });

    changed
}

fn draw_horizontal_vu_meter(
    ui: &mut egui::Ui,
    theme: &Theme,
    levels: Option<&AudioLevels>,
    muted: bool,
    desired_size: egui::Vec2,
    inset: f32,
) {
    let (row_rect, _) = ui.allocate_exact_size(
        egui::vec2(desired_size.x + inset * 2.0, desired_size.y),
        egui::Sense::hover(),
    );
    let rect = egui::Rect::from_min_size(
        egui::pos2(row_rect.left() + inset, row_rect.top()),
        desired_size,
    );
    let painter = ui.painter_at(row_rect);

    painter.rect_filled(rect, 2.0, theme.bg_panel);
    painter.rect_stroke(
        rect,
        2.0,
        egui::Stroke::new(1.0, theme.border_subtle),
        egui::StrokeKind::Inside,
    );

    let rms = levels.map(|level| db_to_meter(level.rms_db)).unwrap_or(0.0);
    let peak = levels
        .map(|level| db_to_meter(level.peak_db))
        .unwrap_or(0.0);

    // Use one segment per ~4px so the meter stays detailed at any width.
    let segments = (rect.width() / 4.0).round().max(12.0) as usize;
    let gap = 1.0;
    let total_gap = gap * (segments as f32 - 1.0);
    let segment_width = ((rect.width() - 2.0 - total_gap) / segments as f32).max(1.0);

    for idx in 0..segments {
        let x = rect.left() + 1.0 + idx as f32 * (segment_width + gap);
        let seg_rect = egui::Rect::from_min_size(
            egui::pos2(x, rect.top() + 1.0),
            egui::vec2(segment_width, rect.height() - 2.0),
        );
        let fill_threshold = (idx + 1) as f32 / segments as f32;
        let db_level = -60.0 + fill_threshold * 60.0;
        let color = meter_color(theme, db_level);
        let fill = if muted || fill_threshold > rms {
            theme.border_subtle
        } else {
            color
        };
        painter.rect_filled(seg_rect, 1.0, fill);
    }

    if !muted && peak > 0.0 {
        let marker_x = rect.left() + 1.0 + peak * (rect.width() - 2.0);
        painter.line_segment(
            [
                egui::pos2(marker_x, rect.top() - 1.0),
                egui::pos2(marker_x, rect.bottom() + 1.0),
            ],
            egui::Stroke::new(1.0, theme.text_primary.gamma_multiply(0.85)),
        );
    }
}

fn draw_horizontal_db_scale(
    ui: &mut egui::Ui,
    theme: &Theme,
    muted: bool,
    desired_size: egui::Vec2,
    inset: f32,
) {
    let (row_rect, _) = ui.allocate_exact_size(
        egui::vec2(desired_size.x + inset * 2.0, desired_size.y),
        egui::Sense::hover(),
    );
    let rect = egui::Rect::from_min_size(
        egui::pos2(row_rect.left() + inset, row_rect.top()),
        desired_size,
    );
    let painter = ui.painter_at(row_rect);
    let color = if muted {
        theme.text_muted.gamma_multiply(0.7)
    } else {
        theme.text_muted
    };

    // Pick label density based on available width so we don't overcrowd.
    let labels: &[i32] = if rect.width() < 220.0 {
        &[-60, -40, -20, 0]
    } else if rect.width() < 360.0 {
        &[-60, -50, -40, -30, -20, -10, 0]
    } else {
        &[-60, -50, -40, -30, -20, -15, -10, -5, 0]
    };

    for &db in labels {
        let t = db_to_meter(db as f32);
        let x = rect.left() + t * rect.width();
        painter.text(
            egui::pos2(x, rect.center().y),
            egui::Align2::CENTER_CENTER,
            db.to_string(),
            egui::FontId::monospace(8.0),
            color,
        );
    }
}

fn draw_horizontal_fader(
    ui: &mut egui::Ui,
    theme: &Theme,
    volume: &mut f32,
    muted: bool,
    desired_size: egui::Vec2,
    inset: f32,
) -> egui::Response {
    let (row_rect, mut response) = ui.allocate_exact_size(
        egui::vec2(desired_size.x + inset * 2.0, desired_size.y),
        egui::Sense::click_and_drag(),
    );
    let rect = egui::Rect::from_min_size(
        egui::pos2(row_rect.left() + inset, row_rect.top()),
        desired_size,
    );

    if (response.dragged() || response.clicked())
        && let Some(pointer) = response.interact_pointer_pos()
    {
        let t = ((pointer.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
        let new_volume = t * 2.0;
        if (new_volume - *volume).abs() > f32::EPSILON {
            *volume = new_volume;
            response.mark_changed();
        }
    }

    let track_rect = egui::Rect::from_center_size(rect.center(), egui::vec2(rect.width(), 4.0));
    let normalized = (*volume / 2.0).clamp(0.0, 1.0);
    let thumb_center_x = rect.left() + normalized * rect.width();
    let thumb_rect = egui::Rect::from_center_size(
        egui::pos2(thumb_center_x, rect.center().y),
        egui::vec2(10.0, rect.height() - 2.0),
    );

    let painter = ui.painter_at(row_rect);
    painter.rect_filled(track_rect, 2.0, theme.bg_panel);
    painter.rect_stroke(
        track_rect,
        2.0,
        egui::Stroke::new(1.0, theme.border),
        egui::StrokeKind::Inside,
    );

    let active_rect = egui::Rect::from_min_max(
        track_rect.min,
        egui::pos2(thumb_center_x, track_rect.bottom()),
    );
    painter.rect_filled(
        active_rect,
        2.0,
        if muted {
            theme.border_subtle
        } else {
            theme.accent.gamma_multiply(0.35)
        },
    );

    painter.rect_filled(
        thumb_rect,
        2.0,
        if response.dragged() || response.hovered() {
            theme.text_primary
        } else {
            theme.bg_surface
        },
    );
    painter.rect_stroke(
        thumb_rect,
        2.0,
        egui::Stroke::new(1.0, theme.border),
        egui::StrokeKind::Inside,
    );

    response
}

fn meter_color(theme: &Theme, db: f32) -> egui::Color32 {
    if db >= -6.0 {
        theme.danger
    } else if db >= -18.0 {
        theme.warning
    } else {
        theme.success
    }
}

fn db_to_meter(db: f32) -> f32 {
    ((db + 60.0) / 60.0).clamp(0.0, 1.0)
}

fn format_level_readout(levels: Option<&AudioLevels>, muted: bool) -> String {
    if muted {
        return "Muted".to_string();
    }
    let Some(levels) = levels else {
        return "-inf dB".to_string();
    };
    if levels.rms_db <= -60.0 {
        "-inf dB".to_string()
    } else {
        format!("{:.1} dB", levels.rms_db)
    }
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
