use std::time::Instant;

use egui::{
    Align, Color32, CornerRadius, CursorIcon, Id, Layout, Rect, Response, Sense, Stroke,
    StrokeKind, Ui, Vec2, Widget,
};

use crate::gstreamer::StreamDestination;
use crate::settings::{
    AdvancedSettings, AppearanceSettings, AudioSettings, GeneralSettings, HotkeySettings,
    StreamSettings, VideoSettings,
};
use crate::state::AppState;

// ── Catppuccin Mocha palette ──────────────────────────────────────────────────

const ACCENT: Color32 = Color32::from_rgb(0x7c, 0x6c, 0xf0);
const TEXT: Color32 = Color32::from_rgb(0xcd, 0xd6, 0xf4);
const SUBTEXT: Color32 = Color32::from_rgb(0xa6, 0xad, 0xc8);
const MUTED: Color32 = Color32::from_rgb(0x6c, 0x70, 0x86);
const SURFACE: Color32 = Color32::from_rgb(0x31, 0x32, 0x44);
const SECTION_HEADER: Color32 = Color32::from_rgb(0x58, 0x5b, 0x70);
const SIDEBAR_BG: Color32 = Color32::from_rgb(0x18, 0x18, 0x25);
const CONTENT_BG: Color32 = Color32::from_rgb(0x1e, 0x1e, 0x2e);

// ── Category enum ─────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum SettingsCategory {
    General,
    Appearance,
    Hotkeys,
    StreamOutput,
    Audio,
    Video,
    Advanced,
}

impl SettingsCategory {
    fn label(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Appearance => "Appearance",
            Self::Hotkeys => "Hotkeys",
            Self::StreamOutput => "Stream / Output",
            Self::Audio => "Audio",
            Self::Video => "Video",
            Self::Advanced => "Advanced",
        }
    }
}

// ── Sidebar grouping ─────────────────────────────────────────────────────────

struct SidebarGroup {
    title: &'static str,
    items: &'static [SettingsCategory],
}

const SIDEBAR_GROUPS: &[SidebarGroup] = &[
    SidebarGroup {
        title: "APPLICATION",
        items: &[
            SettingsCategory::General,
            SettingsCategory::Appearance,
            SettingsCategory::Hotkeys,
        ],
    },
    SidebarGroup {
        title: "OUTPUT",
        items: &[
            SettingsCategory::StreamOutput,
            SettingsCategory::Audio,
            SettingsCategory::Video,
        ],
    },
    SidebarGroup {
        title: "SYSTEM",
        items: &[SettingsCategory::Advanced],
    },
];

// ── Public entry point (native window) ────────────────────────────────────────

/// Render settings UI directly into a native window's egui context.
/// Called from `WindowState::render_settings()`.
pub fn render_native(ctx: &egui::Context, state: &mut AppState) {
    let settings_id = Id::new("settings_active_category");
    let mut active = ctx
        .data_mut(|d| d.get_temp::<SettingsCategory>(settings_id))
        .unwrap_or(SettingsCategory::General);

    // Sidebar panel
    egui::SidePanel::left("settings_sidebar")
        .exact_width(190.0)
        .resizable(false)
        .frame(egui::Frame::NONE.fill(SIDEBAR_BG))
        .show(ctx, |ui| {
            ui.add_space(12.0);
            render_sidebar(ui, &mut active);
        });

    // Content panel
    egui::CentralPanel::default()
        .frame(
            egui::Frame::NONE
                .fill(CONTENT_BG)
                .inner_margin(egui::Margin::same(24)),
        )
        .show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.style_mut().spacing.item_spacing.y = 8.0;

                    let changed = render_content_direct(ui, active, state);

                    if changed {
                        state.settings_dirty = true;
                        state.settings_last_changed = Instant::now();
                    }

                    ui.add_space(24.0);
                });
        });

    ctx.data_mut(|d| d.insert_temp(settings_id, active));
}

// ── Sidebar ───────────────────────────────────────────────────────────────────

fn render_sidebar(ui: &mut Ui, active: &mut SettingsCategory) {
    ui.add_space(16.0);

    for group in SIDEBAR_GROUPS {
        // Group header
        ui.horizontal(|ui| {
            ui.add_space(16.0);
            ui.label(
                egui::RichText::new(group.title)
                    .size(10.0)
                    .color(MUTED)
                    .strong(),
            );
        });
        ui.add_space(4.0);

        for &cat in group.items {
            let is_active = *active == cat;
            let (rect, response) =
                ui.allocate_exact_size(Vec2::new(ui.available_width(), 28.0), Sense::click());

            if response.clicked() {
                *active = cat;
            }

            let hovered = response.hovered();

            // Background on hover or active
            if is_active {
                ui.painter()
                    .rect_filled(rect, CornerRadius::same(4), SURFACE);
                // Accent bar on the left edge
                ui.painter().rect_filled(
                    Rect::from_min_size(rect.min, Vec2::new(3.0, rect.height())),
                    CornerRadius::same(2),
                    ACCENT,
                );
            } else if hovered {
                ui.painter().rect_filled(
                    rect,
                    CornerRadius::same(4),
                    Color32::from_rgba_premultiplied(0x31, 0x32, 0x44, 0x80),
                );
            }

            if hovered {
                ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
            }

            // Label
            let text_color = if is_active { TEXT } else { SUBTEXT };
            let galley = ui.painter().layout_no_wrap(
                cat.label().to_string(),
                egui::FontId::proportional(13.0),
                text_color,
            );
            let text_pos = egui::pos2(rect.min.x + 20.0, rect.center().y - galley.size().y / 2.0);
            ui.painter().galley(text_pos, galley, text_color);
        }

        ui.add_space(12.0);
    }
}

// ── Content dispatch ──────────────────────────────────────────────────────────

/// Render the content area for the active category, taking `&mut AppState` directly.
fn render_content_direct(ui: &mut Ui, category: SettingsCategory, state: &mut AppState) -> bool {
    // Section title
    ui.horizontal(|ui| {
        ui.add_space(24.0);
        ui.label(
            egui::RichText::new(category.label())
                .size(20.0)
                .color(TEXT)
                .strong(),
        );
    });
    ui.add_space(8.0);

    ui.horizontal(|ui| {
        ui.add_space(24.0);
        ui.with_layout(Layout::top_down(Align::Min), |ui| {
            ui.set_width(ui.available_width() - 24.0);
            match category {
                SettingsCategory::General => draw_general(ui, &mut state.settings.general),
                SettingsCategory::StreamOutput => draw_stream(ui, &mut state.settings.stream),
                SettingsCategory::Audio => draw_audio(ui, &mut state.settings.audio),
                SettingsCategory::Video => draw_video(ui, &mut state.settings.video),
                SettingsCategory::Hotkeys => draw_hotkeys(ui, &mut state.settings.hotkeys),
                SettingsCategory::Appearance => draw_appearance(ui, &mut state.settings.appearance),
                SettingsCategory::Advanced => draw_advanced(ui, &mut state.settings.advanced),
            }
        })
        .inner
    })
    .inner
}

// ── Section helper ────────────────────────────────────────────────────────────

fn section_header(ui: &mut Ui, label: &str) {
    ui.add_space(12.0);
    ui.label(
        egui::RichText::new(label)
            .size(11.0)
            .color(SECTION_HEADER)
            .strong(),
    );
    ui.add_space(4.0);
}

fn labeled_row(ui: &mut Ui, label: &str) {
    ui.label(egui::RichText::new(label).size(13.0).color(TEXT));
}

// ── Category: General ─────────────────────────────────────────────────────────

fn draw_general(ui: &mut Ui, settings: &mut GeneralSettings) -> bool {
    let mut changed = false;

    section_header(ui, "STARTUP");

    changed |= draw_toggle(ui, "Launch on startup", &mut settings.launch_on_startup);
    changed |= draw_toggle(
        ui,
        "Check for updates automatically",
        &mut settings.check_for_updates,
    );

    section_header(ui, "BEHAVIOR");

    changed |= draw_toggle(
        ui,
        "Confirm close while streaming",
        &mut settings.confirm_close_while_streaming,
    );

    section_header(ui, "LANGUAGE");

    ui.horizontal(|ui| {
        labeled_row(ui, "Language");
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let combo = egui::ComboBox::from_id_salt("language_combo")
                .selected_text(&settings.language)
                .show_ui(ui, |ui| {
                    let mut c = false;
                    for lang in &["en-US", "en-GB", "es", "fr", "de", "ja", "ko", "zh-CN"] {
                        c |= ui
                            .selectable_value(&mut settings.language, lang.to_string(), *lang)
                            .changed();
                    }
                    c
                });
            if let Some(inner) = combo.inner {
                changed |= inner;
            }
        });
    });

    changed
}

// ── Category: Stream / Output ─────────────────────────────────────────────────

fn draw_stream(ui: &mut Ui, settings: &mut StreamSettings) -> bool {
    let mut changed = false;

    section_header(ui, "DESTINATION");

    ui.horizontal(|ui| {
        labeled_row(ui, "Service");
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let current_label = match &settings.destination {
                StreamDestination::Twitch => "Twitch",
                StreamDestination::YouTube => "YouTube",
                StreamDestination::CustomRtmp { .. } => "Custom RTMP",
            };
            let combo = egui::ComboBox::from_id_salt("stream_dest")
                .selected_text(current_label)
                .show_ui(ui, |ui| {
                    let mut c = false;
                    c |= ui
                        .selectable_value(
                            &mut settings.destination,
                            StreamDestination::Twitch,
                            "Twitch",
                        )
                        .changed();
                    c |= ui
                        .selectable_value(
                            &mut settings.destination,
                            StreamDestination::YouTube,
                            "YouTube",
                        )
                        .changed();
                    if ui
                        .selectable_label(
                            matches!(settings.destination, StreamDestination::CustomRtmp { .. }),
                            "Custom RTMP",
                        )
                        .clicked()
                        && !matches!(settings.destination, StreamDestination::CustomRtmp { .. })
                    {
                        settings.destination = StreamDestination::CustomRtmp { url: String::new() };
                        c = true;
                    }
                    c
                });
            if let Some(inner) = combo.inner {
                changed |= inner;
            }
        });
    });

    // Show custom RTMP URL field if applicable
    if let StreamDestination::CustomRtmp { url } = &mut settings.destination {
        ui.horizontal(|ui| {
            labeled_row(ui, "RTMP URL");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .add(egui::TextEdit::singleline(url).desired_width(250.0))
                    .changed()
                {
                    changed = true;
                }
            });
        });
    }

    ui.horizontal(|ui| {
        labeled_row(ui, "Stream Key");
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui
                .add(
                    egui::TextEdit::singleline(&mut settings.stream_key)
                        .password(true)
                        .desired_width(250.0),
                )
                .changed()
            {
                changed = true;
            }
        });
    });

    section_header(ui, "ENCODER");

    ui.horizontal(|ui| {
        labeled_row(ui, "Encoder");
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let combo = egui::ComboBox::from_id_salt("encoder_combo")
                .selected_text(&settings.encoder)
                .show_ui(ui, |ui| {
                    let mut c = false;
                    for enc in &["x264", "nvenc", "amf", "qsv"] {
                        c |= ui
                            .selectable_value(&mut settings.encoder, enc.to_string(), *enc)
                            .changed();
                    }
                    c
                });
            if let Some(inner) = combo.inner {
                changed |= inner;
            }
        });
    });

    ui.horizontal(|ui| {
        labeled_row(ui, "Bitrate (kbps)");
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            changed |= ui
                .add(egui::DragValue::new(&mut settings.bitrate_kbps).range(500..=50000))
                .changed();
        });
    });

    section_header(ui, "RESOLUTION");

    ui.horizontal(|ui| {
        labeled_row(ui, "Width");
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            changed |= ui
                .add(egui::DragValue::new(&mut settings.width).range(320..=7680))
                .changed();
        });
    });

    ui.horizontal(|ui| {
        labeled_row(ui, "Height");
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            changed |= ui
                .add(egui::DragValue::new(&mut settings.height).range(240..=4320))
                .changed();
        });
    });

    ui.horizontal(|ui| {
        labeled_row(ui, "FPS");
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let combo = egui::ComboBox::from_id_salt("stream_fps")
                .selected_text(format!("{}", settings.fps))
                .show_ui(ui, |ui| {
                    let mut c = false;
                    for fps in &[24u32, 30, 48, 60, 120, 144] {
                        c |= ui
                            .selectable_value(&mut settings.fps, *fps, format!("{fps}"))
                            .changed();
                    }
                    c
                });
            if let Some(inner) = combo.inner {
                changed |= inner;
            }
        });
    });

    changed
}

// ── Category: Audio ───────────────────────────────────────────────────────────

fn draw_audio(ui: &mut Ui, settings: &mut AudioSettings) -> bool {
    let mut changed = false;

    section_header(ui, "DEVICES");

    ui.horizontal(|ui| {
        labeled_row(ui, "Input Device");
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let combo = egui::ComboBox::from_id_salt("audio_input")
                .selected_text(&settings.input_device)
                .show_ui(ui, |ui| {
                    let mut c = false;
                    for dev in &["Default", "Built-in Microphone", "USB Audio"] {
                        c |= ui
                            .selectable_value(&mut settings.input_device, dev.to_string(), *dev)
                            .changed();
                    }
                    c
                });
            if let Some(inner) = combo.inner {
                changed |= inner;
            }
        });
    });

    ui.horizontal(|ui| {
        labeled_row(ui, "Output Device");
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let combo = egui::ComboBox::from_id_salt("audio_output")
                .selected_text(&settings.output_device)
                .show_ui(ui, |ui| {
                    let mut c = false;
                    for dev in &["Default", "Built-in Speakers", "USB Audio"] {
                        c |= ui
                            .selectable_value(&mut settings.output_device, dev.to_string(), *dev)
                            .changed();
                    }
                    c
                });
            if let Some(inner) = combo.inner {
                changed |= inner;
            }
        });
    });

    section_header(ui, "FORMAT");

    ui.horizontal(|ui| {
        labeled_row(ui, "Sample Rate");
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let combo = egui::ComboBox::from_id_salt("sample_rate")
                .selected_text(format!("{} Hz", settings.sample_rate))
                .show_ui(ui, |ui| {
                    let mut c = false;
                    for rate in &[44100u32, 48000, 96000] {
                        c |= ui
                            .selectable_value(
                                &mut settings.sample_rate,
                                *rate,
                                format!("{rate} Hz"),
                            )
                            .changed();
                    }
                    c
                });
            if let Some(inner) = combo.inner {
                changed |= inner;
            }
        });
    });

    ui.horizontal(|ui| {
        labeled_row(ui, "Monitoring");
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let combo = egui::ComboBox::from_id_salt("monitoring")
                .selected_text(&settings.monitoring)
                .show_ui(ui, |ui| {
                    let mut c = false;
                    for mode in &["off", "monitor only", "monitor and output"] {
                        c |= ui
                            .selectable_value(&mut settings.monitoring, mode.to_string(), *mode)
                            .changed();
                    }
                    c
                });
            if let Some(inner) = combo.inner {
                changed |= inner;
            }
        });
    });

    changed
}

// ── Category: Video ───────────────────────────────────────────────────────────

fn draw_video(ui: &mut Ui, settings: &mut VideoSettings) -> bool {
    let mut changed = false;

    section_header(ui, "RESOLUTION");

    ui.horizontal(|ui| {
        labeled_row(ui, "Base (Canvas) Resolution");
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let combo = egui::ComboBox::from_id_salt("base_res")
                .selected_text(&settings.base_resolution)
                .show_ui(ui, |ui| {
                    let mut c = false;
                    for res in &["1920x1080", "2560x1440", "3840x2160"] {
                        c |= ui
                            .selectable_value(&mut settings.base_resolution, res.to_string(), *res)
                            .changed();
                    }
                    c
                });
            if let Some(inner) = combo.inner {
                changed |= inner;
            }
        });
    });

    ui.horizontal(|ui| {
        labeled_row(ui, "Output (Scaled) Resolution");
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let combo = egui::ComboBox::from_id_salt("output_res")
                .selected_text(&settings.output_resolution)
                .show_ui(ui, |ui| {
                    let mut c = false;
                    for res in &["1280x720", "1920x1080", "2560x1440", "3840x2160"] {
                        c |= ui
                            .selectable_value(
                                &mut settings.output_resolution,
                                res.to_string(),
                                *res,
                            )
                            .changed();
                    }
                    c
                });
            if let Some(inner) = combo.inner {
                changed |= inner;
            }
        });
    });

    section_header(ui, "FRAME RATE");

    ui.horizontal(|ui| {
        labeled_row(ui, "FPS");
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let combo = egui::ComboBox::from_id_salt("video_fps")
                .selected_text(format!("{}", settings.fps))
                .show_ui(ui, |ui| {
                    let mut c = false;
                    for fps in &[24u32, 30, 48, 60, 120, 144] {
                        c |= ui
                            .selectable_value(&mut settings.fps, *fps, format!("{fps}"))
                            .changed();
                    }
                    c
                });
            if let Some(inner) = combo.inner {
                changed |= inner;
            }
        });
    });

    section_header(ui, "COLOR");

    ui.horizontal(|ui| {
        labeled_row(ui, "Color Space");
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let combo = egui::ComboBox::from_id_salt("color_space")
                .selected_text(&settings.color_space)
                .show_ui(ui, |ui| {
                    let mut c = false;
                    for cs in &["sRGB", "Rec. 709", "Rec. 2100 (PQ)"] {
                        c |= ui
                            .selectable_value(&mut settings.color_space, cs.to_string(), *cs)
                            .changed();
                    }
                    c
                });
            if let Some(inner) = combo.inner {
                changed |= inner;
            }
        });
    });

    changed
}

// ── Category: Hotkeys ─────────────────────────────────────────────────────────

fn draw_hotkeys(ui: &mut Ui, settings: &mut HotkeySettings) -> bool {
    let mut changed = false;

    section_header(ui, "BINDINGS");

    // Default hotkey actions to display even when the map is empty
    let default_actions = [
        ("start_stream", "Start Streaming"),
        ("stop_stream", "Stop Streaming"),
        ("start_recording", "Start Recording"),
        ("stop_recording", "Stop Recording"),
        ("toggle_mute_mic", "Toggle Mute Mic"),
        ("toggle_mute_desktop", "Toggle Mute Desktop"),
    ];

    for (key, label) in &default_actions {
        ui.horizontal(|ui| {
            labeled_row(ui, label);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let current = settings.bindings.get(*key).cloned().unwrap_or_default();
                let mut binding = current;
                if ui
                    .add(
                        egui::TextEdit::singleline(&mut binding)
                            .desired_width(150.0)
                            .hint_text("Not set"),
                    )
                    .changed()
                {
                    settings.bindings.insert(key.to_string(), binding);
                    changed = true;
                }
            });
        });
    }

    changed
}

// ── Category: Appearance ──────────────────────────────────────────────────────

fn draw_appearance(ui: &mut Ui, settings: &mut AppearanceSettings) -> bool {
    let mut changed = false;

    section_header(ui, "THEME");

    ui.horizontal(|ui| {
        labeled_row(ui, "Theme");
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let combo = egui::ComboBox::from_id_salt("theme_combo")
                .selected_text(&settings.theme)
                .show_ui(ui, |ui| {
                    let mut c = false;
                    for t in &["dark", "light"] {
                        c |= ui
                            .selectable_value(&mut settings.theme, t.to_string(), *t)
                            .changed();
                    }
                    c
                });
            if let Some(inner) = combo.inner {
                changed |= inner;
            }
        });
    });

    section_header(ui, "FONT");

    ui.horizontal(|ui| {
        labeled_row(ui, "Font Size");
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            changed |= ui
                .add(
                    egui::DragValue::new(&mut settings.font_size)
                        .range(8.0..=24.0)
                        .speed(0.25)
                        .suffix(" px"),
                )
                .changed();
        });
    });

    section_header(ui, "ACCENT COLOR");

    ui.horizontal(|ui| {
        labeled_row(ui, "Accent");
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            // Parse hex to egui Color32 for the color picker
            let color = parse_hex_color(&settings.accent_color);
            let mut rgb = [
                color.r() as f32 / 255.0,
                color.g() as f32 / 255.0,
                color.b() as f32 / 255.0,
            ];
            if ui.color_edit_button_rgb(&mut rgb).changed() {
                let r = (rgb[0] * 255.0) as u8;
                let g = (rgb[1] * 255.0) as u8;
                let b = (rgb[2] * 255.0) as u8;
                settings.accent_color = format!("#{r:02x}{g:02x}{b:02x}");
                changed = true;
            }
        });
    });

    changed
}

fn parse_hex_color(hex: &str) -> Color32 {
    let hex = hex.trim_start_matches('#');
    if hex.len() >= 6
        && let (Ok(r), Ok(g), Ok(b)) = (
            u8::from_str_radix(&hex[0..2], 16),
            u8::from_str_radix(&hex[2..4], 16),
            u8::from_str_radix(&hex[4..6], 16),
        )
    {
        return Color32::from_rgb(r, g, b);
    }
    ACCENT
}

// ── Category: Advanced ────────────────────────────────────────────────────────

fn draw_advanced(ui: &mut Ui, settings: &mut AdvancedSettings) -> bool {
    let mut changed = false;

    section_header(ui, "PERFORMANCE");

    ui.horizontal(|ui| {
        labeled_row(ui, "Process Priority");
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let combo = egui::ComboBox::from_id_salt("priority_combo")
                .selected_text(&settings.process_priority)
                .show_ui(ui, |ui| {
                    let mut c = false;
                    for p in &["low", "normal", "high", "realtime"] {
                        c |= ui
                            .selectable_value(&mut settings.process_priority, p.to_string(), *p)
                            .changed();
                    }
                    c
                });
            if let Some(inner) = combo.inner {
                changed |= inner;
            }
        });
    });

    section_header(ui, "NETWORK");

    ui.horizontal(|ui| {
        labeled_row(ui, "Network Buffer Size");
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            changed |= ui
                .add(
                    egui::DragValue::new(&mut settings.network_buffer_size_kb)
                        .range(256..=16384)
                        .suffix(" KB"),
                )
                .changed();
        });
    });

    changed
}

// ── Toggle helper ─────────────────────────────────────────────────────────────

fn draw_toggle(ui: &mut Ui, label: &str, value: &mut bool) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        labeled_row(ui, label);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui.add(toggle_switch(value)).changed() {
                changed = true;
            }
        });
    });
    changed
}

/// A custom iOS-style toggle switch widget with smooth animation.
fn toggle_switch(on: &mut bool) -> impl Widget + '_ {
    move |ui: &mut Ui| -> Response {
        let desired_size = Vec2::new(36.0, 20.0);
        let (rect, mut response) = ui.allocate_exact_size(desired_size, Sense::click());

        if response.clicked() {
            *on = !*on;
            response.mark_changed();
        }

        if ui.is_rect_visible(rect) {
            // Animate the toggle position
            let anim_id = response.id.with("toggle_anim");
            let t = ui.ctx().animate_bool_with_time(anim_id, *on, 0.15);

            let bg_color = if *on { ACCENT } else { SURFACE };

            let knob_radius = 7.0;
            let knob_x = egui::lerp(
                rect.left() + knob_radius + 3.0..=rect.right() - knob_radius - 3.0,
                t,
            );
            let knob_center = egui::pos2(knob_x, rect.center().y);

            // Track background
            ui.painter()
                .rect_filled(rect, CornerRadius::same(10), bg_color);

            // Track border
            if !*on {
                ui.painter().rect_stroke(
                    rect,
                    CornerRadius::same(10),
                    Stroke::new(1.0, MUTED),
                    StrokeKind::Outside,
                );
            }

            // Knob
            ui.painter()
                .circle_filled(knob_center, knob_radius, Color32::WHITE);
        }

        response
    }
}
