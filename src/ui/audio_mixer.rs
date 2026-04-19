use crate::gstreamer::{AudioLevels, GstCommand};
use crate::scene::{AudioInput, SourceId, SourceProperties, SourceType};
use crate::state::AppState;
use crate::ui::layout::PanelId;
use crate::ui::theme::{Theme, active_theme};

const STRIP_WIDTH: f32 = 88.0;
const STRIP_HEIGHT_PADDING: f32 = 12.0;
const METER_WIDTH: f32 = 6.0;
const SCALE_WIDTH: f32 = 18.0;
const FADER_WIDTH: f32 = 16.0;

struct AudioStripData {
    source_id: SourceId,
    name: String,
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

        let strip_height = (ui.available_height() - STRIP_HEIGHT_PADDING)
            .max(0.0)
            .min(ui.available_height());

        if strip_height <= 0.0 {
            return;
        }

        egui::ScrollArea::horizontal()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    for strip in &strips {
                        changed |= draw_audio_strip(ui, state, strip, &theme, strip_height);
                    }
                });
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
    strip_height: f32,
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

    ui.allocate_ui_with_layout(
        egui::vec2(STRIP_WIDTH, strip_height),
        egui::Layout::top_down(egui::Align::Center),
        |ui| {
            egui::Frame::new()
                .fill(theme.bg_elevated)
                .stroke(egui::Stroke::new(1.0, theme.border))
                .corner_radius(egui::CornerRadius::same(theme.radius_sm as u8))
                .inner_margin(egui::Margin::same(8))
                .show(ui, |ui| {
                    let meter_cluster_height =
                        (strip_height - 124.0).clamp(32.0, (strip_height - 84.0).max(32.0));
                    ui.set_width(ui.available_width());
                    ui.set_max_height(strip_height - 2.0);
                    ui.vertical_centered(|ui| {
                        ui.label(
                            egui::RichText::new(&strip.name)
                                .size(10.0)
                                .color(theme.text_primary),
                        );
                        ui.label(
                            egui::RichText::new(compact_detail_summary(&strip.detail_summary))
                                .size(8.0)
                                .color(theme.text_secondary),
                        );
                        if strip.volume_overridden || strip.muted_overridden {
                            ui.add_space(1.0);
                            ui.label(egui::RichText::new("SCENE").size(8.0).color(theme.accent));
                        }

                        ui.add_space(6.0);

                        ui.label(
                            egui::RichText::new(format_level_readout(strip.levels.as_ref(), muted))
                                .size(9.0)
                                .color(if muted {
                                    theme.text_muted
                                } else {
                                    theme.text_primary
                                })
                                .monospace(),
                        );

                        ui.add_space(6.0);

                        ui.horizontal_centered(|ui| {
                            draw_vu_meter(
                                ui,
                                theme,
                                strip.levels.as_ref(),
                                muted,
                                egui::vec2(METER_WIDTH, meter_cluster_height),
                            );
                            ui.add_space(3.0);
                            draw_db_scale(
                                ui,
                                theme,
                                muted,
                                egui::vec2(SCALE_WIDTH, meter_cluster_height),
                            );
                            ui.add_space(6.0);
                            let response = draw_volume_fader(
                                ui,
                                theme,
                                &mut volume,
                                muted,
                                egui::vec2(FADER_WIDTH, meter_cluster_height),
                            );
                            if response.drag_started() {
                                state.begin_continuous_edit();
                            }
                            if response.changed() {
                                apply_volume_override(state, strip.source_id, volume);
                                changed = true;
                            }
                        });

                        ui.add_space(8.0);

                        ui.label(
                            egui::RichText::new(format!("{:.0}%", volume * 100.0))
                                .size(8.0)
                                .color(theme.text_muted)
                                .monospace(),
                        );

                        ui.add_space(8.0);

                        ui.horizontal_centered(|ui| {
                            let reset_active = strip.volume_overridden || strip.muted_overridden;
                            let reset_icon =
                                egui::RichText::new(egui_phosphor::regular::DOTS_SIX_VERTICAL)
                                    .size(13.0)
                                    .color(if reset_active {
                                        theme.text_primary
                                    } else {
                                        theme.text_muted
                                    });
                            if ui
                                .add_sized(
                                    [18.0, 18.0],
                                    egui::Button::new(reset_icon)
                                        .fill(if reset_active {
                                            theme.border_subtle
                                        } else {
                                            theme.bg_panel
                                        })
                                        .stroke(egui::Stroke::new(1.0, theme.border)),
                                )
                                .on_hover_text("Reset scene overrides")
                                .clicked()
                            {
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

                            ui.add_space(4.0);

                            let speaker_icon = if muted {
                                egui_phosphor::regular::SPEAKER_NONE
                            } else {
                                egui_phosphor::regular::SPEAKER_HIGH
                            };
                            if ui
                                .add_sized(
                                    [18.0, 18.0],
                                    egui::Button::new(
                                        egui::RichText::new(speaker_icon).size(11.5).color(
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
                                .on_hover_text(if muted { "Unmute" } else { "Mute" })
                                .clicked()
                            {
                                muted = !muted;
                                apply_mute_override(state, strip.source_id, muted);
                                changed = true;
                            }
                        });
                    });
                });
        },
    );

    changed
}

fn draw_vu_meter(
    ui: &mut egui::Ui,
    theme: &Theme,
    levels: Option<&AudioLevels>,
    muted: bool,
    desired_size: egui::Vec2,
) {
    let (rect, _) = ui.allocate_exact_size(desired_size, egui::Sense::hover());
    let painter = ui.painter_at(rect);

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
    let segments = 28;
    let gap = 1.0;
    let segment_height =
        ((rect.height() - gap * (segments as f32 - 1.0)) / segments as f32).max(1.0);

    for idx in 0..segments {
        let y = rect.bottom() - (idx + 1) as f32 * segment_height - idx as f32 * gap;
        let seg_rect = egui::Rect::from_min_size(
            egui::pos2(rect.left() + 1.0, y),
            egui::vec2(rect.width() - 2.0, segment_height),
        );
        let fill_threshold = (idx + 1) as f32 / segments as f32;
        let db_level = -60.0 + fill_threshold * 60.0;
        let color = meter_color(theme, db_level);
        let fill = if muted || fill_threshold > rms {
            theme.border_subtle
        } else {
            color
        };
        painter.rect_filled(seg_rect, 2.0, fill);
    }

    if !muted && peak > 0.0 {
        let marker_y = rect.bottom() - peak * rect.height();
        painter.line_segment(
            [
                egui::pos2(rect.left() - 1.0, marker_y),
                egui::pos2(rect.right() + 1.0, marker_y),
            ],
            egui::Stroke::new(1.0, theme.text_primary.gamma_multiply(0.85)),
        );
    }
}

fn draw_db_scale(ui: &mut egui::Ui, theme: &Theme, muted: bool, desired_size: egui::Vec2) {
    let (rect, _) = ui.allocate_exact_size(desired_size, egui::Sense::hover());
    let painter = ui.painter_at(rect);
    let labels = [0, -5, -10, -15, -20, -25, -30, -35, -40, -45, -50, -55, -60];

    for db in labels {
        let t = db_to_meter(db as f32);
        let y = rect.bottom() - rect.height() * t;
        painter.text(
            egui::pos2(rect.left(), y),
            egui::Align2::LEFT_CENTER,
            db.to_string(),
            egui::FontId::monospace(7.0),
            if muted {
                theme.text_muted.gamma_multiply(0.7)
            } else {
                theme.text_muted
            },
        );
    }
}

fn draw_volume_fader(
    ui: &mut egui::Ui,
    theme: &Theme,
    volume: &mut f32,
    muted: bool,
    desired_size: egui::Vec2,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(desired_size, egui::Sense::click_and_drag());
    if (response.dragged() || response.clicked())
        && let Some(pointer) = response.interact_pointer_pos()
    {
        let t = ((rect.bottom() - pointer.y) / rect.height()).clamp(0.0, 1.0);
        *volume = t * 2.0;
    }

    let track_rect = egui::Rect::from_center_size(rect.center(), egui::vec2(4.0, rect.height()));
    let normalized = (*volume / 2.0).clamp(0.0, 1.0);
    let thumb_center_y = rect.bottom() - normalized * rect.height();
    let thumb_rect = egui::Rect::from_center_size(
        egui::pos2(rect.center().x, thumb_center_y),
        egui::vec2(10.0, 8.0),
    );

    let painter = ui.painter_at(rect);
    painter.rect_filled(track_rect, 3.0, theme.bg_panel);
    painter.rect_stroke(
        track_rect,
        3.0,
        egui::Stroke::new(1.0, theme.border),
        egui::StrokeKind::Inside,
    );

    let active_rect = egui::Rect::from_min_max(
        egui::pos2(track_rect.left(), thumb_center_y),
        track_rect.max,
    );
    painter.rect_filled(
        active_rect,
        3.0,
        if muted {
            theme.border_subtle
        } else {
            theme.accent.gamma_multiply(0.18)
        },
    );

    painter.rect_filled(
        thumb_rect,
        3.0,
        if response.dragged() || response.hovered() {
            theme.text_primary
        } else {
            theme.bg_surface
        },
    );
    painter.rect_stroke(
        thumb_rect,
        3.0,
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

fn compact_detail_summary(detail: &str) -> String {
    let text = detail.trim();
    if text.chars().count() <= 12 {
        text.to_string()
    } else {
        let short: String = text.chars().take(9).collect();
        format!("{short}...")
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
