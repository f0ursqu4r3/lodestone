//! Properties panel — context-sensitive property editor for the selected source.
//!
//! Works in two modes:
//! - **Library mode** — source is not in the active scene: edits library defaults directly
//! - **Scene mode** — source is in the active scene: edits scene overrides, shows override dots

use crate::gstreamer::{CaptureSourceConfig, GstCommand, GstError};
use crate::scene::{SourceId, SourceProperties, SourceType};
use crate::state::AppState;
use crate::ui::layout::tree::PanelId;
use crate::ui::theme::{DEFAULT_ACCENT, TEXT_MUTED, TEXT_PRIMARY, TEXT_SECONDARY};

/// Draw the properties panel. Shows an empty-state message when no source is
/// selected, or transform / opacity / source-specific controls when one is.
pub fn draw(ui: &mut egui::Ui, state: &mut AppState, _id: PanelId) {
    // Determine which source is selected: prefer scene selection, fall back to library.
    let (selected_id, from_library_selection) =
        if let Some(id) = state.selected_source_id {
            (id, false)
        } else if let Some(id) = state.selected_library_source_id {
            (id, true)
        } else {
            // Empty state: centered muted label.
            ui.vertical_centered(|ui| {
                ui.add_space(ui.available_height() / 3.0);
                ui.label(
                    egui::RichText::new("Select a source to view properties")
                        .color(TEXT_MUTED)
                        .size(11.0),
                );
            });
            return;
        };

    // Find the library source index.
    let Some(lib_idx) = state.library.iter().position(|s| s.id == selected_id) else {
        ui.label(
            egui::RichText::new("Source not found")
                .color(TEXT_MUTED)
                .size(11.0),
        );
        return;
    };

    // Determine editing context: show scene overrides only when selected from the scene,
    // never when selected from the library panel.
    let in_active_scene = !from_library_selection
        && state
            .active_scene()
            .map(|s| s.sources.iter().any(|ss| ss.source_id == selected_id))
            .unwrap_or(false);

    // Show mode header.
    if in_active_scene {
        let scene_name = state
            .active_scene()
            .map(|s| s.name.clone())
            .unwrap_or_default();
        ui.label(
            egui::RichText::new(format!("SCENE OVERRIDE — {}", scene_name.to_uppercase()))
                .color(DEFAULT_ACCENT)
                .size(9.0),
        );
    } else {
        ui.label(
            egui::RichText::new("LIBRARY DEFAULTS")
                .color(TEXT_MUTED)
                .size(9.0),
        );
    }

    ui.add_space(8.0);

    let mut changed = false;

    changed |= draw_transform_section(ui, state, selected_id, lib_idx, in_active_scene);

    ui.add_space(12.0);

    changed |= draw_opacity_section(ui, state, selected_id, lib_idx, in_active_scene);

    ui.add_space(12.0);

    changed |= draw_source_properties(ui, state, selected_id, lib_idx);

    // Mark dirty so the scene collection gets persisted.
    if changed {
        state.scenes_dirty = true;
        state.scenes_last_changed = std::time::Instant::now();
    }
}

/// Draw the transform section (position x/y, size w/h).
///
/// In scene mode, shows an override dot and reads/writes scene overrides.
/// In library mode, edits the library source directly.
///
/// Returns `true` if any value changed.
fn draw_transform_section(
    ui: &mut egui::Ui,
    state: &mut AppState,
    selected_id: SourceId,
    lib_idx: usize,
    in_active_scene: bool,
) -> bool {
    let mut changed = false;

    if in_active_scene {
        // Read current values from scene override + library.
        let lib_source = &state.library[lib_idx];
        let scene_source = state
            .active_scene()
            .and_then(|s| s.find_source(selected_id));
        let is_overridden = scene_source
            .map(|ss| ss.is_transform_overridden())
            .unwrap_or(false);
        let mut transform = scene_source
            .map(|ss| ss.resolve_transform(lib_source))
            .unwrap_or(lib_source.transform);

        let text_color = if is_overridden {
            TEXT_PRIMARY
        } else {
            TEXT_MUTED
        };

        ui.horizontal(|ui| {
            let reset = override_dot(ui, is_overridden);
            if reset
                && let Some(scene) = state.active_scene_mut()
                && let Some(ss) = scene.find_source_mut(selected_id)
            {
                ss.overrides.transform = None;
                changed = true;
            }
            section_label(ui, "TRANSFORM");
        });

        ui.add_space(4.0);

        let mut transform_changed = false;

        // X / Y row
        ui.horizontal(|ui| {
            transform_changed |= drag_field_colored(ui, "X", &mut transform.x, text_color);
            ui.add_space(8.0);
            transform_changed |= drag_field_colored(ui, "Y", &mut transform.y, text_color);
        });

        ui.add_space(2.0);

        // W / H row
        ui.horizontal(|ui| {
            transform_changed |= drag_field_colored(ui, "W", &mut transform.width, text_color);
            ui.add_space(8.0);
            transform_changed |= drag_field_colored(ui, "H", &mut transform.height, text_color);
        });

        if transform_changed
            && let Some(scene) = state.active_scene_mut()
            && let Some(ss) = scene.find_source_mut(selected_id)
        {
            ss.overrides.transform = Some(transform);
            changed = true;
        }
    } else {
        // Library mode: edit library source directly, no dots.
        section_label(ui, "TRANSFORM");
        ui.add_space(4.0);

        let source = &mut state.library[lib_idx];

        // X / Y row
        ui.horizontal(|ui| {
            changed |= drag_field(ui, "X", &mut source.transform.x);
            ui.add_space(8.0);
            changed |= drag_field(ui, "Y", &mut source.transform.y);
        });

        ui.add_space(2.0);

        // W / H row
        ui.horizontal(|ui| {
            changed |= drag_field(ui, "W", &mut source.transform.width);
            ui.add_space(8.0);
            changed |= drag_field(ui, "H", &mut source.transform.height);
        });
    }

    changed
}

/// Draw the opacity slider section.
///
/// In scene mode, shows an override dot and reads/writes scene overrides.
/// In library mode, edits the library source directly.
///
/// Returns `true` if any value changed.
fn draw_opacity_section(
    ui: &mut egui::Ui,
    state: &mut AppState,
    selected_id: SourceId,
    lib_idx: usize,
    in_active_scene: bool,
) -> bool {
    let mut changed = false;

    if in_active_scene {
        // Read current values from scene override + library.
        let lib_source = &state.library[lib_idx];
        let scene_source = state
            .active_scene()
            .and_then(|s| s.find_source(selected_id));
        let is_overridden = scene_source
            .map(|ss| ss.is_opacity_overridden())
            .unwrap_or(false);
        let mut opacity = scene_source
            .map(|ss| ss.resolve_opacity(lib_source))
            .unwrap_or(lib_source.opacity);

        let text_color = if is_overridden {
            TEXT_PRIMARY
        } else {
            TEXT_MUTED
        };

        ui.horizontal(|ui| {
            let reset = override_dot(ui, is_overridden);
            if reset
                && let Some(scene) = state.active_scene_mut()
                && let Some(ss) = scene.find_source_mut(selected_id)
            {
                ss.overrides.opacity = None;
                changed = true;
            }
            section_label(ui, "OPACITY");
        });

        ui.add_space(4.0);

        let prev_opacity = opacity;
        ui.horizontal(|ui| {
            let slider = egui::Slider::new(&mut opacity, 0.0..=1.0).show_value(false);
            if ui.add(slider).changed() {
                // Will be handled below.
            }
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(format!("{}%", (opacity * 100.0).round() as u32))
                    .color(text_color)
                    .size(10.0),
            );
        });

        if (opacity - prev_opacity).abs() > f32::EPSILON
            && let Some(scene) = state.active_scene_mut()
            && let Some(ss) = scene.find_source_mut(selected_id)
        {
            ss.overrides.opacity = Some(opacity);
            changed = true;
        }
    } else {
        // Library mode: edit library source directly, no dots.
        section_label(ui, "OPACITY");
        ui.add_space(4.0);

        let source = &mut state.library[lib_idx];
        ui.horizontal(|ui| {
            let slider = egui::Slider::new(&mut source.opacity, 0.0..=1.0).show_value(false);
            if ui.add(slider).changed() {
                changed = true;
            }
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(format!("{}%", (source.opacity * 100.0).round() as u32))
                    .color(TEXT_SECONDARY)
                    .size(10.0),
            );
        });
    }

    changed
}

/// Draw source-type-specific property controls (device config — always edits library directly).
///
/// Dispatches to Display/Image/Window/Camera UI based on the source type.
///
/// Returns `true` if any value changed.
fn draw_source_properties(
    ui: &mut egui::Ui,
    state: &mut AppState,
    selected_id: SourceId,
    lib_idx: usize,
) -> bool {
    let mut changed = false;

    let source_type = state.library[lib_idx].source_type.clone();
    match source_type {
        SourceType::Display => {
            section_label(ui, "SOURCE");
            ui.add_space(4.0);

            let monitor_count = state.monitor_count;
            let source = &mut state.library[lib_idx];
            if let SourceProperties::Display {
                ref mut screen_index,
            } = source.properties
            {
                let prev_index = *screen_index;
                let selected_label = format!("Monitor {}", *screen_index);
                egui::ComboBox::from_id_salt(
                    egui::Id::new("props_monitor_combo").with(selected_id.0),
                )
                .selected_text(&selected_label)
                .width(ui.available_width() - 8.0)
                .show_ui(ui, |ui| {
                    for i in 0..monitor_count as u32 {
                        let label = format!("Monitor {i}");
                        ui.selectable_value(screen_index, i, label);
                    }
                });

                if *screen_index != prev_index {
                    changed = true;
                }
            }
        }
        SourceType::Image => {
            section_label(ui, "SOURCE");
            ui.add_space(4.0);

            // Clone what we need before taking mutable borrows.
            let cmd_tx = state.command_tx.clone();
            let src_id = selected_id;

            let source = &mut state.library[lib_idx];
            if let SourceProperties::Image { ref mut path } = source.properties {
                // Path text input.
                let hint = if path.is_empty() {
                    "Select an image..."
                } else {
                    ""
                };
                ui.horizontal(|ui| {
                    let te = egui::TextEdit::singleline(path)
                        .hint_text(hint)
                        .desired_width(ui.available_width() - 60.0);
                    if ui.add(te).changed() {
                        changed = true;
                    }
                });

                ui.add_space(4.0);

                let current_path = path.clone();

                ui.horizontal(|ui| {
                    // Browse button.
                    if ui
                        .button(egui_phosphor::regular::FOLDER)
                        .on_hover_text("Browse for image")
                        .clicked()
                        && let Some(picked) = rfd::FileDialog::new()
                            .add_filter(
                                "Images",
                                &["png", "jpg", "jpeg", "bmp", "gif", "webp", "tiff", "tif"],
                            )
                            .pick_file()
                    {
                        let picked_str = picked.to_string_lossy().to_string();
                        load_and_send_image(state, lib_idx, src_id, &cmd_tx, picked_str);
                        changed = true;
                    }

                    // Reload button.
                    let has_path = !current_path.is_empty();
                    ui.add_enabled_ui(has_path, |ui| {
                        if ui
                            .button(egui_phosphor::regular::ARROW_CLOCKWISE)
                            .on_hover_text("Reload image")
                            .clicked()
                        {
                            load_and_send_image(
                                state,
                                lib_idx,
                                src_id,
                                &cmd_tx,
                                current_path.clone(),
                            );
                            changed = true;
                        }
                    });
                });
            }
        }
        SourceType::Window => {
            section_label(ui, "SOURCE");
            ui.add_space(4.0);

            // Clone to avoid borrow conflicts.
            let windows = state.available_windows.clone();
            let cmd_tx = state.command_tx.clone();

            let source = &mut state.library[lib_idx];
            let SourceProperties::Window {
                ref mut window_id,
                ref mut window_title,
                ref mut owner_name,
            } = source.properties
            else {
                return changed;
            };

            let prev_window_id = *window_id;
            let selected_label = if owner_name.is_empty() && window_title.is_empty() {
                "Select a window...".to_string()
            } else {
                format!("{owner_name} \u{2014} {window_title}")
            };

            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt(
                    egui::Id::new("props_window_combo").with(selected_id.0),
                )
                .selected_text(&selected_label)
                .width(ui.available_width() - 32.0)
                .show_ui(ui, |ui| {
                    for win in &windows {
                        let label = format!("{} \u{2014} {}", win.owner_name, win.title);
                        if ui
                            .selectable_label(*window_id == win.window_id, &label)
                            .clicked()
                        {
                            *window_id = win.window_id;
                            *window_title = win.title.clone();
                            *owner_name = win.owner_name.clone();
                        }
                    }
                });

                // Refresh button to re-enumerate windows.
                if ui
                    .button(
                        egui::RichText::new(egui_phosphor::regular::ARROW_CLOCKWISE)
                            .size(14.0)
                            .color(TEXT_SECONDARY),
                    )
                    .on_hover_text("Refresh window list")
                    .clicked()
                {
                    state.available_windows = crate::gstreamer::devices::enumerate_windows();
                }
            });

            if *window_id != prev_window_id && *window_id != 0 {
                // Stop old capture, start new one.
                if let Some(ref tx) = cmd_tx {
                    let _ = tx.try_send(GstCommand::RemoveCaptureSource {
                        source_id: selected_id,
                    });
                    let _ = tx.try_send(GstCommand::AddCaptureSource {
                        source_id: selected_id,
                        config: CaptureSourceConfig::Window {
                            window_id: *window_id,
                        },
                    });
                }
                changed = true;
            }
        }
        SourceType::Camera => {
            section_label(ui, "SOURCE");
            ui.add_space(4.0);

            // Clone to avoid borrow conflicts.
            let cameras = state.available_cameras.clone();
            let cmd_tx = state.command_tx.clone();

            let source = &mut state.library[lib_idx];
            let SourceProperties::Camera {
                ref mut device_index,
                ref mut device_name,
            } = source.properties
            else {
                return changed;
            };

            let prev_device_index = *device_index;
            let selected_label = if device_name.is_empty() {
                "Select a camera...".to_string()
            } else {
                device_name.clone()
            };

            egui::ComboBox::from_id_salt(egui::Id::new("props_camera_combo").with(selected_id.0))
                .selected_text(&selected_label)
                .width(ui.available_width() - 8.0)
                .show_ui(ui, |ui| {
                    for cam in &cameras {
                        if ui
                            .selectable_label(*device_index == cam.device_index, &cam.name)
                            .clicked()
                        {
                            *device_index = cam.device_index;
                            *device_name = cam.name.clone();
                        }
                    }
                });

            if *device_index != prev_device_index {
                // Stop old capture, start new one.
                if let Some(ref tx) = cmd_tx {
                    let _ = tx.try_send(GstCommand::RemoveCaptureSource {
                        source_id: selected_id,
                    });
                    let _ = tx.try_send(GstCommand::AddCaptureSource {
                        source_id: selected_id,
                        config: CaptureSourceConfig::Camera {
                            device_index: *device_index,
                        },
                    });
                }
                changed = true;
            }
        }
        _ => {
            // Other source types don't have extra properties yet.
        }
    }

    changed
}

/// Draw a small override indicator dot. Returns `true` if the user right-clicked
/// and chose "Reset to library default".
fn override_dot(ui: &mut egui::Ui, is_overridden: bool) -> bool {
    let size = 6.0;
    let (rect, response) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::click());
    if is_overridden {
        ui.painter()
            .circle_filled(rect.center(), size / 2.0, DEFAULT_ACCENT);
    }
    // Right-click to reset.
    let mut reset = false;
    if is_overridden {
        response.context_menu(|ui| {
            if ui.button("Reset to library default").clicked() {
                reset = true;
                ui.close();
            }
        });
    }
    reset
}

/// Render a section heading in the style: 9px uppercase `TEXT_MUTED` with letter spacing.
fn section_label(ui: &mut egui::Ui, text: &str) {
    ui.label(egui::RichText::new(text).color(TEXT_MUTED).size(9.0));
}

/// Render a labeled `DragValue` field and return whether the value changed.
fn drag_field(ui: &mut egui::Ui, label: &str, value: &mut f32) -> bool {
    ui.label(egui::RichText::new(label).color(TEXT_MUTED).size(10.0));
    ui.add(
        egui::DragValue::new(value)
            .speed(1.0)
            .update_while_editing(false),
    )
    .changed()
}

/// Render a labeled `DragValue` field with a specific label color, and return
/// whether the value changed.
fn drag_field_colored(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut f32,
    label_color: egui::Color32,
) -> bool {
    ui.label(egui::RichText::new(label).color(label_color).size(10.0));
    ui.add(
        egui::DragValue::new(value)
            .speed(1.0)
            .update_while_editing(false),
    )
    .changed()
}

/// Load an image from `path`, update the source properties/transform, and send the frame
/// to the GStreamer thread via `LoadImageFrame`.
fn load_and_send_image(
    state: &mut AppState,
    source_idx: usize,
    source_id: crate::scene::SourceId,
    cmd_tx: &Option<tokio::sync::mpsc::Sender<GstCommand>>,
    path: String,
) {
    match crate::image_source::load_image_source(&path) {
        Ok(frame) => {
            let source = &mut state.library[source_idx];
            // Update the stored path.
            if let SourceProperties::Image { path: ref mut p } = source.properties {
                *p = path;
            }
            // Set transform and native size to the image's dimensions.
            let native = (frame.width as f32, frame.height as f32);
            source.transform.width = native.0;
            source.transform.height = native.1;
            source.native_size = native;
            // Send the frame to GStreamer.
            if let Some(tx) = cmd_tx {
                let _ = tx.try_send(GstCommand::LoadImageFrame { source_id, frame });
            }
        }
        Err(e) => {
            state.active_errors.push(GstError::CaptureFailure {
                message: format!("Failed to load image: {e}"),
            });
        }
    }
}
