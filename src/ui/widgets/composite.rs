//! Composite widgets combining multiple controls into reusable units.

use egui::Ui;

#[allow(unused_imports)]
use crate::ui::theme::active_theme;
use crate::gstreamer::QualityPreset;

/// Encoder selection dropdown showing available encoders with "Recommended" tag.
/// Returns `true` if the selection changed.
pub fn encoder_dropdown(
    ui: &mut Ui,
    selected: &mut crate::gstreamer::EncoderType,
    available: &[crate::gstreamer::AvailableEncoder],
) -> bool {
    let mut changed = false;
    let current_label = available
        .iter()
        .find(|e| e.encoder_type == *selected)
        .map(|e| {
            let name = e.encoder_type.display_name();
            if e.is_recommended {
                format!("{name} — Recommended")
            } else {
                name.to_string()
            }
        })
        .unwrap_or_else(|| selected.display_name().to_string());

    egui::ComboBox::from_label("")
        .selected_text(&current_label)
        .show_ui(ui, |ui| {
            for enc in available {
                let label = if enc.is_recommended {
                    format!("{} — Recommended", enc.encoder_type.display_name())
                } else {
                    enc.encoder_type.display_name().to_string()
                };
                if ui
                    .selectable_value(selected, enc.encoder_type, &label)
                    .changed()
                {
                    changed = true;
                }
            }
        });
    changed
}

/// Quality preset toggle row + conditional bitrate input.
/// Returns `true` if any value changed.
pub fn quality_presets(
    ui: &mut Ui,
    preset: &mut crate::gstreamer::QualityPreset,
    custom_bitrate: &mut u32,
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        for &p in QualityPreset::all() {
            let label = match p {
                QualityPreset::Low => "Low",
                QualityPreset::Medium => "Medium",
                QualityPreset::High => "High",
                QualityPreset::Custom => "Custom",
            };
            if ui.selectable_label(*preset == p, label).clicked() {
                *preset = p;
                changed = true;
            }
        }
    });
    if *preset == QualityPreset::Custom {
        if ui
            .add(
                egui::DragValue::new(custom_bitrate)
                    .range(500..=50000)
                    .suffix(" kbps"),
            )
            .changed()
        {
            changed = true;
        }
    } else {
        ui.label(
            egui::RichText::new(format!("{} kbps", preset.bitrate_kbps()))
                .weak()
                .size(11.0),
        );
    }
    changed
}

/// FPS toggle row (24 / 30 / 60). Returns `true` if the value changed.
pub fn fps_toggles(ui: &mut Ui, fps: &mut u32) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        for &f in &[24u32, 30, 60] {
            if ui.selectable_label(*fps == f, f.to_string()).clicked() {
                *fps = f;
                changed = true;
            }
        }
    });
    changed
}
