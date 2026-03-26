use egui::Ui;

use crate::settings::RecordSettings;

pub(super) fn draw(
    ui: &mut Ui,
    settings: &mut RecordSettings,
    available_encoders: &[crate::gstreamer::AvailableEncoder],
) -> bool {
    let mut changed = false;

    // Format toggle (MKV / MP4)
    ui.label("Format");
    ui.horizontal(|ui| {
        if ui
            .selectable_label(
                matches!(settings.format, crate::gstreamer::RecordingFormat::Mkv),
                "MKV",
            )
            .clicked()
        {
            settings.format = crate::gstreamer::RecordingFormat::Mkv;
            changed = true;
        }
        if ui
            .selectable_label(
                matches!(settings.format, crate::gstreamer::RecordingFormat::Mp4),
                "MP4",
            )
            .clicked()
        {
            settings.format = crate::gstreamer::RecordingFormat::Mp4;
            changed = true;
        }
    });
    ui.label(egui::RichText::new("MKV is crash-safe").weak().size(11.0));

    ui.add_space(8.0);

    // Output folder
    ui.label("Output Folder");
    let folder_str = settings.output_folder.display().to_string();
    ui.horizontal(|ui| {
        ui.label(&folder_str);
        if ui.button("Browse").clicked()
            && let Some(path) = rfd::FileDialog::new()
                .set_directory(&settings.output_folder)
                .pick_folder()
        {
            settings.output_folder = path;
            changed = true;
        }
    });

    ui.add_space(8.0);

    // Filename template
    ui.label("Filename Template");
    if ui
        .text_edit_singleline(&mut settings.filename_template)
        .changed()
    {
        changed = true;
    }
    let preview = RecordSettings::expand_template(&settings.filename_template, "Main", 1);
    let ext = match settings.format {
        crate::gstreamer::RecordingFormat::Mkv => "mkv",
        crate::gstreamer::RecordingFormat::Mp4 => "mp4",
    };
    ui.label(
        egui::RichText::new(format!("Preview: {preview}.{ext}"))
            .weak()
            .size(11.0),
    );

    ui.separator();

    // Encoder, quality, FPS — reuse shared helpers from stream.rs
    ui.label("Encoder");
    if super::stream::draw_encoder_dropdown(ui, &mut settings.encoder, available_encoders) {
        changed = true;
    }

    ui.add_space(8.0);
    ui.label("Quality");
    if super::stream::draw_quality_presets(
        ui,
        &mut settings.quality_preset,
        &mut settings.bitrate_kbps,
    ) {
        changed = true;
    }

    ui.add_space(8.0);
    ui.label("FPS");
    if super::stream::draw_fps_toggles(ui, &mut settings.fps) {
        changed = true;
    }

    changed
}
