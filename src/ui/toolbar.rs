//! Fixed toolbar rendered at the top of the main window.
//!
//! Contains the app logo, scene quick-switcher, live stats, stream/record
//! controls, and a settings button.

use egui::{self, Color32, RichText, Sense, Vec2};

use crate::gstreamer::EncoderConfig;
use crate::renderer::compositor::parse_resolution;
use crate::ui::scenes_panel::trigger_scene_transition;
use crate::scene::SceneId;
use crate::state::{AppState, RecordingStatus, StreamStatus};
use crate::ui::theme::{BTN_PADDING, BTN_PILL_PADDING, active_theme};

/// Draw the toolbar. Returns `true` if the settings button was clicked.
pub fn draw(ctx: &egui::Context, state: &mut AppState) -> bool {
    let theme = active_theme(ctx);
    let mut settings_clicked = false;

    egui::TopBottomPanel::top("toolbar")
        .exact_height(theme.toolbar_height)
        .frame(
            egui::Frame::new()
                .fill(theme.bg_surface)
                .stroke(egui::Stroke::new(1.0, theme.border))
                .inner_margin(egui::Margin::symmetric(12, 0)),
        )
        .show(ctx, |ui| {
            ui.horizontal_centered(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                ui.spacing_mut().button_padding = BTN_PADDING;

                // ── App logo ──
                ui.label(
                    RichText::new("Lodestone")
                        .size(13.0)
                        .strong()
                        .color(theme.text_primary),
                );

                divider(ui);

                // ── Scene quick-switcher ──
                draw_scene_switcher(ui, state);

                divider(ui);

                // ── Spacer to push controls to the right ──
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;

                    // ── Settings gear ──
                    let gear_resp = ui.add(
                        egui::Button::new(
                            RichText::new(egui_phosphor::regular::GEAR)
                                .size(16.0)
                                .color(theme.text_secondary),
                        )
                        .frame(false),
                    );
                    if gear_resp.clicked() {
                        settings_clicked = true;
                    }
                    gear_resp.on_hover_text("Settings");

                    divider(ui);

                    // ── Virtual Camera button ──
                    draw_virtual_camera_button(ui, state);

                    // ── Record button ──
                    draw_record_button(ui, state);

                    // ── Go Live button ──
                    draw_go_live_button(ui, state);

                    divider(ui);

                    // ── Stream stats (only when live) ──
                    draw_stream_stats(ui, state);
                });
            });
        });

    settings_clicked
}

/// Draw a vertical divider line.
fn divider(ui: &mut egui::Ui) {
    let theme = active_theme(ui.ctx());
    let height = 20.0;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(1.0, height), Sense::hover());
    ui.painter().line_segment(
        [rect.center_top(), rect.center_bottom()],
        (1.0, theme.border),
    );
}

/// Scene quick-switcher: shows only pinned scenes as pill-style buttons.
/// The live (program) scene shows a red dot. When the active scene differs
/// from the program scene, a "Transition" button appears.
fn draw_scene_switcher(ui: &mut egui::Ui, state: &mut AppState) {
    let theme = active_theme(ui.ctx());
    ui.spacing_mut().button_padding = BTN_PILL_PADDING;
    let active_id = state.active_scene_id;
    let program_id = state.program_scene_id;

    // Collect pinned scenes to avoid borrow issues.
    let pinned_scenes: Vec<(SceneId, String)> = state
        .scenes
        .iter()
        .filter(|s| s.pinned)
        .map(|s| (s.id, s.name.clone()))
        .collect();

    if pinned_scenes.is_empty() {
        ui.label(
            RichText::new("Pin scenes to show here")
                .size(10.0)
                .color(theme.text_muted),
        );
        return;
    }

    let mut new_active = active_id;

    for (id, name) in &pinned_scenes {
        let is_active = active_id == Some(*id);
        let is_live = program_id == Some(*id);
        let fill = if is_active {
            theme.bg_elevated
        } else {
            theme.bg_base
        };
        let text_color = if is_active {
            theme.text_primary
        } else {
            theme.text_secondary
        };

        let btn = egui::Button::new(RichText::new(name).size(11.0).color(text_color))
            .fill(fill)
            .corner_radius(theme.radius_lg)
            .min_size(Vec2::new(0.0, 24.0));

        let response = ui.add(btn);

        // Red dot for the live (program) scene.
        if is_live {
            let dot_center = egui::pos2(
                response.rect.left() + 8.0,
                response.rect.center().y,
            );
            ui.painter()
                .circle_filled(dot_center, 3.0, theme.danger);
        }

        if response.clicked() {
            new_active = Some(*id);
        }
    }

    // Transition button — shown when editing scene differs from live scene.
    let can_transition = active_id != program_id
        && active_id.is_some()
        && state.active_transition.is_none();
    if can_transition {
        let btn = egui::Button::new(
            RichText::new(format!("{} Go Live", egui_phosphor::regular::ARROW_RIGHT))
                .size(11.0)
                .strong()
                .color(Color32::WHITE),
        )
        .fill(theme.danger)
        .corner_radius(theme.radius_sm)
        .min_size(Vec2::new(0.0, 24.0));

        if ui.add(btn).clicked() {
            trigger_scene_transition(state);
        }
    }

    if new_active != active_id {
        state.active_scene_id = new_active;
        state.deselect_all();
        state.mark_dirty();
    }
}

/// Stream stats: green dot, uptime, bitrate, dropped frames.
fn draw_stream_stats(ui: &mut egui::Ui, state: &AppState) {
    let theme = active_theme(ui.ctx());
    if let StreamStatus::Live {
        uptime_secs,
        bitrate_kbps,
        dropped_frames,
    } = &state.stream_status
    {
        let total = *uptime_secs as u64;
        let hours = total / 3600;
        let minutes = (total % 3600) / 60;
        let seconds = total % 60;

        ui.label(
            RichText::new(format!("Dropped: {dropped_frames}"))
                .size(11.0)
                .color(theme.text_secondary),
        );
        ui.label(
            RichText::new(format!("{bitrate_kbps:.0} kbps"))
                .size(11.0)
                .color(theme.text_secondary),
        );
        ui.label(
            RichText::new(format!("{hours:02}:{minutes:02}:{seconds:02}"))
                .size(11.0)
                .color(theme.text_primary),
        );

        // Green dot
        let (dot_rect, _) = ui.allocate_exact_size(Vec2::splat(8.0), Sense::hover());
        ui.painter()
            .circle_filled(dot_rect.center(), 4.0, theme.success);
    }
}

/// Go Live / Stop Stream button.
fn draw_go_live_button(ui: &mut egui::Ui, state: &mut AppState) {
    let theme = active_theme(ui.ctx());
    let is_live = state.stream_status.is_live();

    let (label, fill, text_color) = if is_live {
        // Pulsing red fill when live.
        let t = ui.input(|i| i.time);
        let pulse = (t * 2.0).sin() * 0.15 + 0.85;
        let r = (theme.danger.r() as f64 * pulse) as u8;
        let g = (theme.danger.g() as f64 * pulse) as u8;
        let b = (theme.danger.b() as f64 * pulse) as u8;
        let pulsed = Color32::from_rgb(r, g, b);
        ("LIVE", pulsed, Color32::WHITE)
    } else {
        ("Go Live", theme.bg_elevated, theme.text_secondary)
    };

    let stroke_color = if is_live { theme.danger } else { theme.border };

    let btn = egui::Button::new(RichText::new(label).size(11.0).strong().color(text_color))
        .fill(fill)
        .stroke(egui::Stroke::new(1.0, stroke_color))
        .corner_radius(theme.radius_sm)
        .min_size(Vec2::new(64.0, 26.0));

    if ui.add(btn).clicked()
        && let Some(ref tx) = state.command_tx
    {
        if is_live {
            let _ = tx.try_send(crate::gstreamer::GstCommand::StopStream);
            state.stream_status = StreamStatus::Offline;
        } else if let Some(error_msg) = validate_stream_settings(state) {
            state
                .active_errors
                .push(crate::gstreamer::GstError::EncodeFailure { message: error_msg });
        } else {
            let _ = tx.try_send(crate::gstreamer::GstCommand::StartStream {
                destination: state.settings.stream.destination.clone(),
                stream_key: state.settings.stream.stream_key.clone(),
                encoder_config: stream_encoder_config(state),
            });
            state.stream_status = StreamStatus::Live {
                uptime_secs: 0.0,
                bitrate_kbps: 0.0,
                dropped_frames: 0,
            };
        }
    }

    // Request repaint for pulse animation when live.
    if is_live {
        ui.ctx().request_repaint();
    }
}

/// Virtual Camera toggle button.
fn draw_virtual_camera_button(ui: &mut egui::Ui, state: &mut AppState) {
    let theme = active_theme(ui.ctx());
    let is_active = state.virtual_camera_active;

    let icon = egui_phosphor::regular::WEBCAM;
    let (label, fill, text_color) = if is_active {
        (format!("{icon} V-Cam"), theme.success, Color32::WHITE)
    } else {
        (format!("{icon} V-Cam"), theme.bg_elevated, theme.text_secondary)
    };

    let stroke_color = if is_active { theme.success } else { theme.border };

    let btn = egui::Button::new(RichText::new(label).size(11.0).strong().color(text_color))
        .fill(fill)
        .stroke(egui::Stroke::new(1.0, stroke_color))
        .corner_radius(theme.radius_sm)
        .min_size(Vec2::new(64.0, 26.0));

    if ui.add(btn).clicked()
        && let Some(ref tx) = state.command_tx
    {
        if is_active {
            let _ = tx.try_send(crate::gstreamer::GstCommand::StopVirtualCamera);
            state.virtual_camera_active = false;
        } else {
            let _ = tx.try_send(crate::gstreamer::GstCommand::StartVirtualCamera);
            state.virtual_camera_active = true;
        }
    }
}

/// Record / Stop Recording button.
fn draw_record_button(ui: &mut egui::Ui, state: &mut AppState) {
    let theme = active_theme(ui.ctx());
    let is_recording = matches!(state.recording_status, RecordingStatus::Recording { .. });

    let label = if is_recording {
        if let Some(started) = state.recording_started_at {
            let elapsed = started.elapsed().as_secs();
            let h = elapsed / 3600;
            let m = (elapsed % 3600) / 60;
            let s = elapsed % 60;
            format!("REC {:02}:{:02}:{:02}", h, m, s)
        } else {
            "REC".to_string()
        }
    } else {
        "Record".to_string()
    };

    let (fill, text_color) = if is_recording {
        (theme.danger, Color32::WHITE)
    } else {
        (theme.bg_elevated, theme.text_secondary)
    };

    let stroke_color = if is_recording { theme.danger } else { theme.border };

    let btn = egui::Button::new(RichText::new(&label).size(11.0).strong().color(text_color))
        .fill(fill)
        .stroke(egui::Stroke::new(1.0, stroke_color))
        .corner_radius(theme.radius_sm)
        .min_size(Vec2::new(64.0, 26.0));

    if ui.add(btn).clicked()
        && let Some(ref tx) = state.command_tx
    {
        if is_recording {
            let _ = tx.try_send(crate::gstreamer::GstCommand::StopRecording);
            state.recording_status = RecordingStatus::Idle;
            state.recording_started_at = None;
        } else {
            state.recording_counter += 1;
            let scene_name = "Main"; // TODO: get active scene name
            let filename = crate::settings::RecordSettings::expand_template(
                &state.settings.record.filename_template,
                scene_name,
                state.recording_counter,
            );
            let ext = match state.settings.record.format {
                crate::gstreamer::RecordingFormat::Mkv => "mkv",
                crate::gstreamer::RecordingFormat::Mp4 => "mp4",
            };
            let folder = if state.settings.record.output_folder.exists() {
                state.settings.record.output_folder.clone()
            } else {
                dirs::video_dir()
                    .or_else(dirs::home_dir)
                    .unwrap_or_else(|| std::path::PathBuf::from("."))
            };
            let path = folder.join(format!("{filename}.{ext}"));

            let _ = tx.try_send(crate::gstreamer::GstCommand::StartRecording {
                path: path.clone(),
                format: state.settings.record.format,
                encoder_config: record_encoder_config(state),
            });
            state.recording_status = RecordingStatus::Recording { path };
            state.recording_started_at = Some(std::time::Instant::now());
        }
    }

    // Request repaint for timer when recording.
    if is_recording {
        ui.ctx().request_repaint();
    }
}

/// Validate stream settings before starting. Returns error message if invalid.
fn validate_stream_settings(state: &AppState) -> Option<String> {
    match &state.settings.stream.destination {
        crate::gstreamer::StreamDestination::Twitch
        | crate::gstreamer::StreamDestination::YouTube => {
            if state.settings.stream.stream_key.trim().is_empty() {
                return Some("Stream key is required".to_string());
            }
        }
        crate::gstreamer::StreamDestination::CustomRtmp { url } => {
            if url.trim().is_empty() {
                return Some("RTMP URL is required".to_string());
            }
            if !url.starts_with("rtmp://") && !url.starts_with("rtmps://") {
                return Some("RTMP URL must start with rtmp:// or rtmps://".to_string());
            }
        }
    }
    None
}

/// Build an [`EncoderConfig`] for streaming from the current app settings.
fn stream_encoder_config(state: &AppState) -> EncoderConfig {
    let (width, height) = parse_resolution(&state.settings.video.output_resolution);
    let bitrate = if state.settings.stream.quality_preset == crate::gstreamer::QualityPreset::Custom
    {
        state.settings.stream.bitrate_kbps
    } else {
        state.settings.stream.quality_preset.bitrate_kbps()
    };
    EncoderConfig {
        width,
        height,
        fps: state.settings.stream.fps,
        bitrate_kbps: bitrate,
        encoder_type: state.settings.stream.encoder,
    }
}

/// Build an [`EncoderConfig`] for recording from the current app settings.
fn record_encoder_config(state: &AppState) -> EncoderConfig {
    let (width, height) = parse_resolution(&state.settings.video.output_resolution);
    let bitrate = if state.settings.record.quality_preset == crate::gstreamer::QualityPreset::Custom
    {
        state.settings.record.bitrate_kbps
    } else {
        state.settings.record.quality_preset.bitrate_kbps()
    };
    EncoderConfig {
        width,
        height,
        fps: state.settings.record.fps,
        bitrate_kbps: bitrate,
        encoder_type: state.settings.record.encoder,
    }
}
