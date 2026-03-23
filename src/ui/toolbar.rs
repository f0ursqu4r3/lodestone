//! Fixed toolbar rendered at the top of the main window.
//!
//! Contains the app logo, scene quick-switcher, live stats, stream/record
//! controls, and a settings button.

use egui::{self, Color32, RichText, Sense, Vec2};

use crate::scene::{Scene, SceneId};
use crate::state::{AppState, RecordingStatus, StreamStatus};
use crate::ui::theme::{
    BG_BASE, BG_ELEVATED, BG_SURFACE, BORDER, BTN_PADDING, BTN_PILL_PADDING, GREEN_ONLINE,
    RADIUS_LG, RADIUS_SM, RED_LIVE, TEXT_MUTED, TEXT_PRIMARY, TEXT_SECONDARY, TOOLBAR_HEIGHT,
};

/// Draw the toolbar. Returns `true` if the settings button was clicked.
pub fn draw(ctx: &egui::Context, state: &mut AppState) -> bool {
    let mut settings_clicked = false;

    egui::TopBottomPanel::top("toolbar")
        .exact_height(TOOLBAR_HEIGHT)
        .frame(
            egui::Frame::new()
                .fill(BG_SURFACE)
                .stroke(egui::Stroke::new(1.0, BORDER))
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
                        .color(TEXT_PRIMARY),
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
                            RichText::new(egui_phosphor::regular::GEAR).size(16.0).color(TEXT_SECONDARY),
                        )
                        .frame(false),
                    );
                    if gear_resp.clicked() {
                        settings_clicked = true;
                    }
                    gear_resp.on_hover_text("Settings");

                    divider(ui);

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
    let height = 20.0;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(1.0, height), Sense::hover());
    ui.painter()
        .line_segment([rect.center_top(), rect.center_bottom()], (1.0, BORDER));
}

/// Scene quick-switcher: horizontal pill-style buttons.
fn draw_scene_switcher(ui: &mut egui::Ui, state: &mut AppState) {
    ui.spacing_mut().button_padding = BTN_PILL_PADDING;
    let active_id = state.active_scene_id;

    // Collect scene info to avoid borrow issues.
    let scene_info: Vec<(SceneId, String)> = state
        .scenes
        .iter()
        .map(|s| (s.id, s.name.clone()))
        .collect();

    let mut new_active = active_id;

    for (id, name) in &scene_info {
        let is_active = active_id == Some(*id);
        let fill = if is_active { BG_ELEVATED } else { BG_BASE };
        let text_color = if is_active {
            TEXT_PRIMARY
        } else {
            TEXT_SECONDARY
        };

        let btn = egui::Button::new(RichText::new(name).size(11.0).color(text_color))
            .fill(fill)
            .corner_radius(RADIUS_LG)
            .min_size(Vec2::new(0.0, 24.0));

        if ui.add(btn).clicked() {
            new_active = Some(*id);
        }
    }

    // "+" button to create a new scene.
    let add_btn = egui::Button::new(RichText::new(egui_phosphor::regular::PLUS).size(12.0).color(TEXT_MUTED))
        .fill(BG_BASE)
        .corner_radius(RADIUS_LG)
        .min_size(Vec2::new(24.0, 24.0));

    if ui.add(add_btn).clicked() {
        let new_id = SceneId(state.next_scene_id);
        state.next_scene_id += 1;
        let scene_num = state.scenes.len() + 1;
        state.scenes.push(Scene {
            id: new_id,
            name: format!("Scene {scene_num}"),
            sources: Vec::new(),
        });
        new_active = Some(new_id);
        state.scenes_dirty = true;
        state.scenes_last_changed = std::time::Instant::now();
    }

    if new_active != active_id {
        state.active_scene_id = new_active;
        state.scenes_dirty = true;
        state.scenes_last_changed = std::time::Instant::now();
    }
}

/// Stream stats: green dot, uptime, bitrate, dropped frames.
fn draw_stream_stats(ui: &mut egui::Ui, state: &AppState) {
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
                .color(TEXT_SECONDARY),
        );
        ui.label(
            RichText::new(format!("{bitrate_kbps:.0} kbps"))
                .size(11.0)
                .color(TEXT_SECONDARY),
        );
        ui.label(
            RichText::new(format!("{hours:02}:{minutes:02}:{seconds:02}"))
                .size(11.0)
                .color(TEXT_PRIMARY),
        );

        // Green dot
        let (dot_rect, _) = ui.allocate_exact_size(Vec2::splat(8.0), Sense::hover());
        ui.painter()
            .circle_filled(dot_rect.center(), 4.0, GREEN_ONLINE);
    }
}

/// Go Live / Stop Stream button.
fn draw_go_live_button(ui: &mut egui::Ui, state: &mut AppState) {
    let is_live = state.stream_status.is_live();

    let (label, fill, text_color) = if is_live {
        // Pulsing red fill when live.
        let t = ui.input(|i| i.time);
        let pulse = (t * 2.0).sin() * 0.15 + 0.85;
        let r = (RED_LIVE.r() as f64 * pulse) as u8;
        let g = (RED_LIVE.g() as f64 * pulse) as u8;
        let b = (RED_LIVE.b() as f64 * pulse) as u8;
        let pulsed = Color32::from_rgb(r, g, b);
        ("LIVE", pulsed, Color32::WHITE)
    } else {
        ("Go Live", Color32::TRANSPARENT, RED_LIVE)
    };

    let btn = egui::Button::new(RichText::new(label).size(11.0).strong().color(text_color))
        .fill(fill)
        .stroke(egui::Stroke::new(1.0, RED_LIVE))
        .corner_radius(RADIUS_SM)
        .min_size(Vec2::new(64.0, 26.0));

    if ui.add(btn).clicked()
        && let Some(ref tx) = state.command_tx
    {
        if is_live {
            let _ = tx.try_send(crate::gstreamer::GstCommand::StopStream);
            state.stream_status = StreamStatus::Offline;
        } else {
            let config = crate::gstreamer::StreamConfig {
                destination: crate::gstreamer::StreamDestination::Twitch,
                stream_key: state.settings.stream.stream_key.clone(),
            };
            let _ = tx.try_send(crate::gstreamer::GstCommand::StartStream(config));
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

/// Record / Stop Recording button.
fn draw_record_button(ui: &mut egui::Ui, state: &mut AppState) {
    let is_recording = matches!(state.recording_status, RecordingStatus::Recording { .. });

    let rec_color = Color32::from_rgb(0xCC, 0x33, 0x33);

    let (label, fill, text_color) = if is_recording {
        ("REC", rec_color, Color32::WHITE)
    } else {
        ("Record", Color32::TRANSPARENT, rec_color)
    };

    let btn = egui::Button::new(RichText::new(label).size(11.0).strong().color(text_color))
        .fill(fill)
        .stroke(egui::Stroke::new(1.0, rec_color))
        .corner_radius(RADIUS_SM)
        .min_size(Vec2::new(64.0, 26.0));

    if ui.add(btn).clicked()
        && let Some(ref tx) = state.command_tx
    {
        if is_recording {
            let _ = tx.try_send(crate::gstreamer::GstCommand::StopRecording);
            state.recording_status = RecordingStatus::Idle;
        } else {
            let video_dir = dirs::video_dir()
                .or_else(dirs::home_dir)
                .unwrap_or_else(|| std::path::PathBuf::from("."));
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let filename = format!("lodestone-{timestamp}.mkv");
            let path = video_dir.join(filename);
            let _ = tx.try_send(crate::gstreamer::GstCommand::StartRecording {
                path: path.clone(),
                format: crate::gstreamer::RecordingFormat::Mkv,
            });
            state.recording_status = RecordingStatus::Recording { path };
        }
    }
}
