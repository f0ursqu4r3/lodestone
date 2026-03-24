use crate::gstreamer::{AudioSourceKind, GstCommand};
use crate::state::AppState;
use crate::ui::layout::PanelId;
use crate::ui::theme::{
    BG_BASE, BG_PANEL, BORDER, RADIUS_MD, RED_LIVE, TEXT_MUTED, VU_GREEN, VU_RED, VU_YELLOW,
};
use egui::StrokeKind;

/// Draw the audio mixer panel with per-source VU meters, faders, and mute controls.
pub fn draw(ui: &mut egui::Ui, state: &mut AppState, _panel_id: PanelId) {
    // Panel background
    let panel_rect = ui.available_rect_before_wrap();
    ui.painter().rect_filled(panel_rect, 0.0, BG_PANEL);

    // Clone levels and device info before the closure to avoid borrow conflicts.
    let mic_levels = state.audio_levels.mic.clone();
    let system_levels = state.audio_levels.system.clone();
    let has_loopback = state.available_audio_devices.iter().any(|d| d.is_loopback);

    ui.with_layout(egui::Layout::left_to_right(egui::Align::Min), |ui| {
        ui.spacing_mut().item_spacing.x = 8.0;

        // Mic channel
        draw_channel_strip(ui, state, "MIC", AudioSourceKind::Mic, mic_levels.as_ref());

        // System channel — only show if a loopback device is available
        if has_loopback {
            draw_channel_strip(
                ui,
                state,
                "SYSTEM",
                AudioSourceKind::System,
                system_levels.as_ref(),
            );
        } else {
            ui.vertical(|ui| {
                ui.label(egui::RichText::new("SYSTEM").size(9.0).color(TEXT_MUTED));
                ui.add_space(10.0);
                ui.label(
                    egui::RichText::new("Install\nBlackHole\nfor system\naudio")
                        .color(TEXT_MUTED)
                        .size(9.0),
                );
            });
        }
    });
}

fn draw_channel_strip(
    ui: &mut egui::Ui,
    state: &AppState,
    name: &str,
    kind: AudioSourceKind,
    levels: Option<&crate::gstreamer::AudioLevels>,
) {
    let current_db = levels.map(|l| l.rms_db).unwrap_or(-60.0);
    let peak_db = levels.map(|l| l.peak_db).unwrap_or(-60.0);

    ui.vertical(|ui| {
        // Source label: 9px uppercase TEXT_MUTED, centered
        ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
            ui.set_width(40.0);
            ui.label(egui::RichText::new(name).size(9.0).color(TEXT_MUTED));

            ui.add_space(4.0);

            // VU meter: 8px wide, fills available height minus label/dB/mute overhead.
            let vu_width = 8.0_f32;
            let fixed_overhead = 4.0 + 14.0 + 4.0 + 16.0; // spacer + dB + spacer + mute
            let vu_height = (ui.available_height() - fixed_overhead).max(20.0);
            let fill_frac = ((current_db + 60.0) / 60.0).clamp(0.0, 1.0);
            let filled_height = vu_height * fill_frac;

            let (rect, _) =
                ui.allocate_exact_size(egui::vec2(vu_width, vu_height), egui::Sense::hover());

            // Background: BG_BASE with 4px corner radius
            ui.painter().rect(
                rect,
                4.0,
                BG_BASE,
                egui::Stroke::new(1.0, BORDER),
                StrokeKind::Outside,
            );

            // Peak glow: subtle red glow behind meter when signal > -6 dB
            if peak_db > -6.0 {
                let glow_rect = rect.expand(2.0);
                let glow_color = VU_RED.gamma_multiply(0.3);
                ui.painter().rect_filled(glow_rect, RADIUS_MD, glow_color);
                // Re-draw background on top of glow
                ui.painter().rect(
                    rect,
                    4.0,
                    BG_BASE,
                    egui::Stroke::new(1.0, BORDER),
                    StrokeKind::Outside,
                );
            }

            // Filled portion with gradient coloring
            if filled_height > 0.0 {
                let fill_rect = egui::Rect::from_min_max(
                    egui::pos2(rect.min.x, rect.max.y - filled_height),
                    rect.max,
                );

                // Draw individual pixel rows for the gradient effect
                let fill_top = fill_rect.min.y;
                let fill_bottom = fill_rect.max.y;

                let steps = (filled_height as i32).max(1);
                for i in 0..steps {
                    let y_bottom = fill_bottom - i as f32;
                    let y_top = (y_bottom - 1.0).max(fill_top);
                    if y_top >= y_bottom {
                        continue;
                    }

                    // What dB does this row represent?
                    let row_frac = (i as f32 + 0.5) / vu_height;
                    let row_db = -60.0 + row_frac * 60.0;

                    let color = if row_db > -6.0 {
                        // Red zone: lerp yellow -> red from -6 to 0 dB
                        let t = ((row_db + 6.0) / 6.0).clamp(0.0, 1.0);
                        lerp_color(VU_YELLOW, VU_RED, t)
                    } else if row_db > -18.0 {
                        // Yellow zone: lerp green -> yellow from -18 to -6 dB
                        let t = ((row_db + 18.0) / 12.0).clamp(0.0, 1.0);
                        lerp_color(VU_GREEN, VU_YELLOW, t)
                    } else {
                        VU_GREEN
                    };

                    let row_rect = egui::Rect::from_min_max(
                        egui::pos2(rect.min.x + 1.0, y_top),
                        egui::pos2(rect.max.x - 1.0, y_bottom),
                    );
                    ui.painter().rect_filled(row_rect, 0.0, color);
                }
            }

            ui.add_space(4.0);

            // dB readout: 9px TEXT_MUTED, monospace, below meter
            let db_text = if current_db <= -60.0 {
                "-inf".to_string()
            } else {
                format!("{:.0}", current_db)
            };
            ui.label(
                egui::RichText::new(db_text)
                    .size(9.0)
                    .color(TEXT_MUTED)
                    .monospace(),
            );

            ui.add_space(4.0);

            // Mute toggle: 20x16px, 1px BORDER, 2px corner radius
            let mute_id = egui::Id::new(("audio_mute", name));
            let mut muted: bool = ui.memory(|m| m.data.get_temp(mute_id).unwrap_or(false));

            let mute_size = egui::vec2(20.0, 16.0);
            let (mute_rect, mute_response) =
                ui.allocate_exact_size(mute_size, egui::Sense::click());

            if mute_response.clicked() {
                muted = !muted;
                if let Some(ref tx) = state.command_tx {
                    let _ = tx.try_send(GstCommand::SetAudioMuted {
                        source: kind,
                        muted,
                    });
                }
            }
            ui.memory_mut(|m| m.data.insert_temp(mute_id, muted));

            // Draw mute button
            let (bg, text_color) = if muted {
                (RED_LIVE, egui::Color32::WHITE)
            } else {
                (BG_BASE, TEXT_MUTED)
            };
            ui.painter().rect(
                mute_rect,
                2.0,
                bg,
                egui::Stroke::new(1.0, BORDER),
                StrokeKind::Outside,
            );
            ui.painter().text(
                mute_rect.center(),
                egui::Align2::CENTER_CENTER,
                "M",
                egui::FontId::new(8.0, egui::FontFamily::Proportional),
                text_color,
            );
        });
    });
}

/// Linearly interpolate between two colors.
fn lerp_color(a: egui::Color32, b: egui::Color32, t: f32) -> egui::Color32 {
    let t = t.clamp(0.0, 1.0);
    let inv = 1.0 - t;
    egui::Color32::from_rgb(
        (a.r() as f32 * inv + b.r() as f32 * t) as u8,
        (a.g() as f32 * inv + b.g() as f32 * t) as u8,
        (a.b() as f32 * inv + b.b() as f32 * t) as u8,
    )
}
