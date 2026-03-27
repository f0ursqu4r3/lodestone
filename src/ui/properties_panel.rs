//! Properties panel — context-sensitive property editor for the selected source.
//!
//! Works in two modes:
//! - **Library mode** — source is not in the active scene: edits library defaults directly
//! - **Scene mode** — source is in the active scene: edits scene overrides, shows override dots

use crate::gstreamer::types::RgbaFrame;
use crate::gstreamer::{CaptureSourceConfig, GstCommand, GstError};
use crate::scene::{
    AudioInput, ColorFill, GradientStop, SourceId, SourceProperties, SourceType, TextAlignment,
    WindowCaptureMode,
};
use crate::state::AppState;
use crate::ui::layout::tree::PanelId;
use crate::ui::theme::active_theme;

/// Draw the properties panel. Shows an empty-state message when no source is
/// selected, or transform / opacity / source-specific controls when one is.
pub fn draw(ui: &mut egui::Ui, state: &mut AppState, _id: PanelId) {
    let theme = active_theme(ui.ctx());
    // Determine which source is selected: prefer scene selection, fall back to library.
    let (selected_id, from_library_selection) = if let Some(id) = state.selected_source_id() {
        (id, false)
    } else if let Some(id) = state.selected_library_source_id {
        (id, true)
    } else {
        // Empty state: centered muted label.
        ui.vertical_centered(|ui| {
            ui.add_space(ui.available_height() / 3.0);
            ui.label(
                egui::RichText::new("Select a source to view properties")
                    .color(theme.text_muted)
                    .size(11.0),
            );
        });
        return;
    };

    // Find the library source index.
    let Some(lib_idx) = state.library.iter().position(|s| s.id == selected_id) else {
        ui.label(
            egui::RichText::new("Source not found")
                .color(theme.text_muted)
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
                .color(theme.accent)
                .size(9.0),
        );
    } else {
        ui.label(
            egui::RichText::new("LIBRARY DEFAULTS")
                .color(theme.text_muted)
                .size(9.0),
        );
    }

    ui.add_space(8.0);

    // Track continuous edits (drag fields, sliders) so the undo system
    // captures one snapshot per gesture rather than one per frame.
    let editing_id = egui::Id::new("props_editing");
    let was_editing: bool = ui.memory(|m| m.data.get_temp(editing_id).unwrap_or(false));
    let any_drag_active = ui.ctx().is_using_pointer();

    if was_editing {
        state.begin_continuous_edit();
    }

    let mut changed = false;

    changed |= draw_transform_section(ui, state, selected_id, lib_idx, in_active_scene);

    ui.add_space(12.0);

    changed |= draw_opacity_section(ui, state, selected_id, lib_idx, in_active_scene);

    ui.add_space(12.0);

    changed |= draw_source_properties(ui, state, selected_id, lib_idx);

    if changed {
        state.mark_dirty();
    }

    let still_editing = changed || (was_editing && any_drag_active);
    if was_editing && !still_editing {
        state.end_continuous_edit();
    }
    ui.memory_mut(|m| m.data.insert_temp(editing_id, still_editing));
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
    let theme = active_theme(ui.ctx());
    let mut changed = false;

    // Read native size for the reset button.
    let native_size = state.library[lib_idx].native_size;
    let aspect_locked = state.library[lib_idx].aspect_ratio_locked;

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
            theme.text_primary
        } else {
            theme.text_muted
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

        // W / H row with aspect-ratio lock + reset size button
        let prev_w = transform.width;
        let prev_h = transform.height;
        ui.horizontal(|ui| {
            transform_changed |= drag_field_colored(ui, "W", &mut transform.width, text_color);
            ui.add_space(4.0);
            if aspect_lock_button(ui, aspect_locked) {
                state.library[lib_idx].aspect_ratio_locked = !aspect_locked;
                changed = true;
            }
            ui.add_space(4.0);
            transform_changed |= drag_field_colored(ui, "H", &mut transform.height, text_color);
            ui.add_space(4.0);
            if ui
                .add(
                    egui::Button::new(
                        egui::RichText::new(egui_phosphor::regular::ARROW_COUNTER_CLOCKWISE)
                            .size(12.0)
                            .color(theme.text_secondary),
                    )
                    .frame(false),
                )
                .on_hover_text("Reset to native size")
                .clicked()
            {
                transform.width = native_size.0;
                transform.height = native_size.1;
                transform_changed = true;
            }
        });

        // Enforce aspect ratio after drag.
        if transform_changed && aspect_locked {
            enforce_aspect_ratio(&mut transform.width, &mut transform.height, prev_w, prev_h);
        }

        ui.add_space(2.0);

        // Rotation row
        ui.horizontal(|ui| {
            let mut rotation = transform.rotation;
            let response = ui.add(
                egui::DragValue::new(&mut rotation)
                    .speed(1.0)
                    .suffix("°")
                    .range(0.0..=360.0)
                    .update_while_editing(false),
            );
            ui.label(egui::RichText::new("Rotation").color(text_color).size(10.0));
            if response.changed() {
                transform.rotation = rotation.rem_euclid(360.0);
                transform_changed = true;
            }
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

        // X / Y row
        {
            let source = &mut state.library[lib_idx];
            ui.horizontal(|ui| {
                changed |= drag_field(ui, "X", &mut source.transform.x);
                ui.add_space(8.0);
                changed |= drag_field(ui, "Y", &mut source.transform.y);
            });
        }

        ui.add_space(2.0);

        // W / H row with aspect-ratio lock + reset size button
        let prev_w = state.library[lib_idx].transform.width;
        let prev_h = state.library[lib_idx].transform.height;
        let mut lock_toggled = false;
        {
            let source = &mut state.library[lib_idx];
            ui.horizontal(|ui| {
                changed |= drag_field(ui, "W", &mut source.transform.width);
                ui.add_space(4.0);
                lock_toggled = aspect_lock_button(ui, aspect_locked);
                ui.add_space(4.0);
                changed |= drag_field(ui, "H", &mut source.transform.height);
                ui.add_space(4.0);
                if ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new(egui_phosphor::regular::ARROW_COUNTER_CLOCKWISE)
                                .size(12.0)
                                .color(theme.text_secondary),
                        )
                        .frame(false),
                    )
                    .on_hover_text("Reset to native size")
                    .clicked()
                {
                    source.transform.width = native_size.0;
                    source.transform.height = native_size.1;
                    changed = true;
                }
            });
        }

        if lock_toggled {
            state.library[lib_idx].aspect_ratio_locked = !aspect_locked;
        }

        ui.add_space(2.0);

        // Rotation row
        {
            let source = &mut state.library[lib_idx];
            ui.horizontal(|ui| {
                let mut rotation = source.transform.rotation;
                let response = ui.add(
                    egui::DragValue::new(&mut rotation)
                        .speed(1.0)
                        .suffix("°")
                        .range(0.0..=360.0)
                        .update_while_editing(false),
                );
                ui.label(
                    egui::RichText::new("Rotation")
                        .color(theme.text_muted)
                        .size(10.0),
                );
                if response.changed() {
                    source.transform.rotation = rotation.rem_euclid(360.0);
                    changed = true;
                }
            });
        }

        // Enforce aspect ratio after drag.
        if changed && aspect_locked {
            let source = &mut state.library[lib_idx];
            enforce_aspect_ratio(
                &mut source.transform.width,
                &mut source.transform.height,
                prev_w,
                prev_h,
            );
        }
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
    let theme = active_theme(ui.ctx());
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
            theme.text_primary
        } else {
            theme.text_muted
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
                    .color(theme.text_secondary)
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
    let theme = active_theme(ui.ctx());
    let mut changed = false;

    let source_type = state.library[lib_idx].source_type.clone();
    let cmd_tx_for_display = state.command_tx.clone();
    let exclude_self = state.settings.general.exclude_self_from_capture;
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
                    let new_idx = *screen_index;
                    // Update native_size and transform to match the new display.
                    if let Some(display) = state
                        .available_displays
                        .iter()
                        .find(|d| d.index == new_idx as usize)
                    {
                        source.native_size = (display.width as f32, display.height as f32);
                        source.transform.width = display.width as f32;
                        source.transform.height = display.height as f32;
                    }

                    // Stop old capture, start new one with the new monitor.
                    if let Some(ref tx) = cmd_tx_for_display {
                        let _ = tx.try_send(GstCommand::RemoveCaptureSource {
                            source_id: selected_id,
                        });
                        let _ = tx.try_send(GstCommand::AddCaptureSource {
                            source_id: selected_id,
                            config: CaptureSourceConfig::Screen {
                                screen_index: new_idx,
                                exclude_self,
                            },
                        });
                    }
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

            if state.available_apps.is_empty() {
                state.available_apps = crate::gstreamer::devices::enumerate_applications();
            }

            // Consume window picker result if available.
            if let Some(result) = state.window_picker_result.take() {
                let source = &mut state.library[lib_idx];
                if let SourceProperties::Window { ref mut mode, .. } = source.properties {
                    let new_mode = WindowCaptureMode::Application {
                        bundle_id: result.bundle_id,
                        app_name: result.app_name,
                        pinned_title: if result.window_title.is_empty() {
                            None
                        } else {
                            Some(result.window_title)
                        },
                    };
                    *mode = new_mode.clone();
                    changed = true;
                    // Restart capture with the new mode.
                    if let Some(ref tx) = state.command_tx {
                        let _ = tx.try_send(GstCommand::RemoveCaptureSource {
                            source_id: selected_id,
                        });
                        let _ = tx.try_send(GstCommand::AddCaptureSource {
                            source_id: selected_id,
                            config: CaptureSourceConfig::Window { mode: new_mode },
                        });
                    }
                    // Refresh app list so the newly selected app shows up.
                    state.available_apps = crate::gstreamer::devices::enumerate_applications();
                }
            }

            let apps = state.available_apps.clone();
            let cmd_tx = state.command_tx.clone();

            let source = &mut state.library[lib_idx];
            let SourceProperties::Window {
                ref mut mode,
                ref current_window_id,
            } = source.properties
            else {
                return changed;
            };

            let prev_mode = mode.clone();

            // Mode selector
            let is_fullscreen_mode = matches!(mode, WindowCaptureMode::AnyFullscreen);
            let mode_label = if is_fullscreen_mode {
                "Any Fullscreen Application"
            } else {
                "Specific Application"
            };

            egui::ComboBox::from_id_salt(egui::Id::new("props_window_mode").with(selected_id.0))
                .selected_text(mode_label)
                .width(ui.available_width())
                .show_ui(ui, |ui| {
                    if ui
                        .selectable_label(!is_fullscreen_mode, "Specific Application")
                        .clicked()
                        && is_fullscreen_mode
                    {
                        *mode = WindowCaptureMode::Application {
                            bundle_id: String::new(),
                            app_name: String::new(),
                            pinned_title: None,
                        };
                    }
                    if ui
                        .selectable_label(is_fullscreen_mode, "Any Fullscreen Application")
                        .clicked()
                        && !is_fullscreen_mode
                    {
                        *mode = WindowCaptureMode::AnyFullscreen;
                    }
                });

            ui.add_space(4.0);

            // App selector (only in Application mode)
            if let WindowCaptureMode::Application {
                bundle_id,
                app_name,
                pinned_title,
            } = mode
            {
                let selected_app_label = if app_name.is_empty() {
                    "Select an application...".to_string()
                } else {
                    app_name.clone()
                };

                ui.horizontal(|ui| {
                    egui::ComboBox::from_id_salt(
                        egui::Id::new("props_window_app").with(selected_id.0),
                    )
                    .selected_text(&selected_app_label)
                    .width(ui.available_width() - 32.0)
                    .show_ui(ui, |ui| {
                        for app in &apps {
                            if ui
                                .selectable_label(*bundle_id == app.bundle_id, &app.name)
                                .clicked()
                            {
                                *bundle_id = app.bundle_id.clone();
                                *app_name = app.name.clone();
                                *pinned_title = None;
                            }
                        }
                    });

                    // Window picker (dropper) button
                    if ui
                        .button(
                            egui::RichText::new(egui_phosphor::regular::CROSSHAIR)
                                .size(14.0)
                                .color(theme.text_secondary),
                        )
                        .on_hover_text("Pick a window from screen")
                        .clicked()
                    {
                        state.window_picker_active = true;
                    }

                    // Refresh button
                    if ui
                        .button(
                            egui::RichText::new(egui_phosphor::regular::ARROW_CLOCKWISE)
                                .size(14.0)
                                .color(theme.text_secondary),
                        )
                        .on_hover_text("Refresh application list")
                        .clicked()
                    {
                        state.available_apps = crate::gstreamer::devices::enumerate_applications();
                    }
                });

                // Pin-to-window toggle (when app has multiple windows)
                if !bundle_id.is_empty()
                    && let Some(app) = apps.iter().find(|a| a.bundle_id == *bundle_id)
                    && app.windows.len() > 1
                {
                    ui.add_space(4.0);
                    let mut is_pinned = pinned_title.is_some();
                    if ui
                        .checkbox(&mut is_pinned, "Pin to specific window")
                        .changed()
                    {
                        if is_pinned {
                            *pinned_title = app.windows.first().map(|w| w.title.clone());
                        } else {
                            *pinned_title = None;
                        }
                    }

                    if let Some(title) = pinned_title {
                        egui::ComboBox::from_id_salt(
                            egui::Id::new("props_window_pin").with(selected_id.0),
                        )
                        .selected_text(title.as_str())
                        .width(ui.available_width())
                        .show_ui(ui, |ui| {
                            for win in &app.windows {
                                if ui
                                    .selectable_label(*title == win.title, &win.title)
                                    .clicked()
                                {
                                    *title = win.title.clone();
                                }
                            }
                        });
                    }
                }
            }

            // Status display
            ui.add_space(4.0);
            let status = if current_window_id.is_some() {
                match mode {
                    WindowCaptureMode::Application { app_name, .. } => {
                        format!("Capturing: {app_name}")
                    }
                    WindowCaptureMode::AnyFullscreen => "Capturing fullscreen app".to_string(),
                }
            } else {
                match mode {
                    WindowCaptureMode::Application { app_name, .. } if !app_name.is_empty() => {
                        format!("Waiting for {}...", app_name)
                    }
                    WindowCaptureMode::AnyFullscreen => "No fullscreen application".to_string(),
                    _ => "Select an application".to_string(),
                }
            };
            ui.label(egui::RichText::new(&status).size(11.0).color(
                if current_window_id.is_some() {
                    theme.text_secondary
                } else {
                    theme.text_muted
                },
            ));

            // Trigger capture restart if mode changed
            if *mode != prev_mode {
                if let Some(ref tx) = cmd_tx {
                    let _ = tx.try_send(GstCommand::RemoveCaptureSource {
                        source_id: selected_id,
                    });
                    let _ = tx.try_send(GstCommand::AddCaptureSource {
                        source_id: selected_id,
                        config: CaptureSourceConfig::Window { mode: mode.clone() },
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
                // Update native_size and transform to match the new camera.
                let new_idx = *device_index;
                if let Some(cam) = cameras.iter().find(|c| c.device_index == new_idx) {
                    source.native_size = (cam.resolution.0 as f32, cam.resolution.1 as f32);
                    source.transform.width = cam.resolution.0 as f32;
                    source.transform.height = cam.resolution.1 as f32;
                }

                // Stop old capture, start new one.
                if let Some(ref tx) = cmd_tx {
                    let _ = tx.try_send(GstCommand::RemoveCaptureSource {
                        source_id: selected_id,
                    });
                    let _ = tx.try_send(GstCommand::AddCaptureSource {
                        source_id: selected_id,
                        config: CaptureSourceConfig::Camera {
                            device_index: new_idx,
                        },
                    });
                }
                changed = true;
            }
        }
        SourceType::Text => {
            section_label(ui, "TEXT");
            ui.add_space(4.0);

            let cmd_tx = state.command_tx.clone();
            let src_id = selected_id;
            let source = &mut state.library[lib_idx];
            if let SourceProperties::Text {
                ref mut content,
                ref mut font_family,
                ref mut font_size,
                ref mut font_color,
                ref mut background_color,
                ref mut bold,
                ref mut italic,
                ref mut alignment,
                ref mut outline,
                ref mut padding,
                ref mut wrap_width,
            } = source.properties
            {
                // Multiline text input.
                let te = egui::TextEdit::multiline(content)
                    .hint_text("Enter text...")
                    .desired_rows(3)
                    .desired_width(ui.available_width() - 8.0);
                if ui.add(te).changed() {
                    changed = true;
                }
                ui.add_space(4.0);

                // Font family dropdown.
                let families = [
                    ("bundled:sans", "Sans"),
                    ("bundled:serif", "Serif"),
                    ("bundled:mono", "Mono"),
                    ("bundled:display", "Display"),
                ];
                let current_label = families
                    .iter()
                    .find(|(k, _)| *k == font_family.as_str())
                    .map(|(_, v)| *v)
                    .unwrap_or("Sans");
                egui::ComboBox::from_id_salt(
                    egui::Id::new("props_font_family").with(selected_id.0),
                )
                .selected_text(current_label)
                .width(ui.available_width() - 8.0)
                .show_ui(ui, |ui| {
                    for (key, label) in &families {
                        if ui.selectable_label(*font_family == *key, *label).clicked() {
                            *font_family = key.to_string();
                            changed = true;
                        }
                    }
                });
                ui.add_space(4.0);

                // Font size slider.
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Size")
                            .color(theme.text_secondary)
                            .size(10.0),
                    );
                    if ui
                        .add(egui::Slider::new(font_size, 8.0..=200.0).suffix(" pt"))
                        .changed()
                    {
                        changed = true;
                    }
                });
                ui.add_space(2.0);

                // Bold / Italic toggles.
                ui.horizontal(|ui| {
                    if ui
                        .selectable_label(*bold, egui::RichText::new("B").strong())
                        .clicked()
                    {
                        *bold = !*bold;
                        changed = true;
                    }
                    if ui
                        .selectable_label(*italic, egui::RichText::new("I").italics())
                        .clicked()
                    {
                        *italic = !*italic;
                        changed = true;
                    }
                });
                ui.add_space(2.0);

                // Text color.
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Text Color")
                            .color(theme.text_secondary)
                            .size(10.0),
                    );
                    if ui.color_edit_button_rgba_unmultiplied(font_color).changed() {
                        changed = true;
                    }
                });
                ui.add_space(2.0);

                // Background color.
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Background")
                            .color(theme.text_secondary)
                            .size(10.0),
                    );
                    if ui
                        .color_edit_button_rgba_unmultiplied(background_color)
                        .changed()
                    {
                        changed = true;
                    }
                });
                ui.add_space(2.0);

                // Alignment buttons.
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Align")
                            .color(theme.text_secondary)
                            .size(10.0),
                    );
                    if ui
                        .selectable_label(
                            *alignment == TextAlignment::Left,
                            egui_phosphor::regular::TEXT_ALIGN_LEFT,
                        )
                        .clicked()
                    {
                        *alignment = TextAlignment::Left;
                        changed = true;
                    }
                    if ui
                        .selectable_label(
                            *alignment == TextAlignment::Center,
                            egui_phosphor::regular::TEXT_ALIGN_CENTER,
                        )
                        .clicked()
                    {
                        *alignment = TextAlignment::Center;
                        changed = true;
                    }
                    if ui
                        .selectable_label(
                            *alignment == TextAlignment::Right,
                            egui_phosphor::regular::TEXT_ALIGN_RIGHT,
                        )
                        .clicked()
                    {
                        *alignment = TextAlignment::Right;
                        changed = true;
                    }
                });
                ui.add_space(2.0);

                // Outline.
                let mut has_outline = outline.is_some();
                if ui.checkbox(&mut has_outline, "Outline").changed() {
                    if has_outline {
                        *outline = Some(crate::scene::TextOutline {
                            color: [0.0, 0.0, 0.0, 1.0],
                            width: 2.0,
                        });
                    } else {
                        *outline = None;
                    }
                    changed = true;
                }
                if let Some(ol) = outline {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("Outline Color")
                                .color(theme.text_secondary)
                                .size(10.0),
                        );
                        if ui
                            .color_edit_button_rgba_unmultiplied(&mut ol.color)
                            .changed()
                        {
                            changed = true;
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("Width")
                                .color(theme.text_secondary)
                                .size(10.0),
                        );
                        if ui
                            .add(egui::Slider::new(&mut ol.width, 0.5..=20.0).suffix(" px"))
                            .changed()
                        {
                            changed = true;
                        }
                    });
                }
                ui.add_space(2.0);

                // Padding.
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Padding")
                            .color(theme.text_secondary)
                            .size(10.0),
                    );
                    if ui
                        .add(egui::Slider::new(padding, 0.0..=100.0).suffix(" px"))
                        .changed()
                    {
                        changed = true;
                    }
                });
                ui.add_space(2.0);

                // Word wrap.
                let mut has_wrap = wrap_width.is_some();
                if ui.checkbox(&mut has_wrap, "Word Wrap").changed() {
                    if has_wrap {
                        *wrap_width = Some(400.0);
                    } else {
                        *wrap_width = None;
                    }
                    changed = true;
                }
                if let Some(ww) = wrap_width {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("Wrap Width")
                                .color(theme.text_secondary)
                                .size(10.0),
                        );
                        if ui
                            .add(egui::Slider::new(ww, 50.0..=3840.0).suffix(" px"))
                            .changed()
                        {
                            changed = true;
                        }
                    });
                }

                // Re-render on change.
                if changed {
                    let props = state.library[lib_idx].properties.clone();
                    if let Some(frame) = crate::text_source::render_text_source(&props) {
                        let source = &mut state.library[lib_idx];
                        source.native_size = (frame.width as f32, frame.height as f32);
                        source.transform.width = frame.width as f32;
                        source.transform.height = frame.height as f32;
                        if let Some(ref tx) = cmd_tx {
                            let _ = tx.try_send(GstCommand::LoadImageFrame {
                                source_id: src_id,
                                frame,
                            });
                        }
                    }
                }
            }
        }
        SourceType::Color => {
            section_label(ui, "COLOR");
            ui.add_space(4.0);

            let cmd_tx = state.command_tx.clone();
            let src_id = selected_id;
            let source = &mut state.library[lib_idx];
            if let SourceProperties::Color { ref mut fill } = source.properties {
                // Fill type selector.
                let fill_type = match fill {
                    ColorFill::Solid { .. } => 0,
                    ColorFill::LinearGradient { .. } => 1,
                    ColorFill::RadialGradient { .. } => 2,
                };
                ui.horizontal(|ui| {
                    if ui.selectable_label(fill_type == 0, "Solid").clicked() && fill_type != 0 {
                        *fill = ColorFill::Solid {
                            color: [1.0, 1.0, 1.0, 1.0],
                        };
                        changed = true;
                    }
                    if ui.selectable_label(fill_type == 1, "Linear").clicked() && fill_type != 1 {
                        *fill = ColorFill::LinearGradient {
                            angle: 0.0,
                            stops: vec![
                                GradientStop {
                                    position: 0.0,
                                    color: [0.0, 0.0, 0.0, 1.0],
                                },
                                GradientStop {
                                    position: 1.0,
                                    color: [1.0, 1.0, 1.0, 1.0],
                                },
                            ],
                        };
                        changed = true;
                    }
                    if ui.selectable_label(fill_type == 2, "Radial").clicked() && fill_type != 2 {
                        *fill = ColorFill::RadialGradient {
                            center: (0.5, 0.5),
                            radius: 0.5,
                            stops: vec![
                                GradientStop {
                                    position: 0.0,
                                    color: [1.0, 1.0, 1.0, 1.0],
                                },
                                GradientStop {
                                    position: 1.0,
                                    color: [0.0, 0.0, 0.0, 1.0],
                                },
                            ],
                        };
                        changed = true;
                    }
                });
                ui.add_space(4.0);

                match fill {
                    ColorFill::Solid { color } => {
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new("Color")
                                    .color(theme.text_secondary)
                                    .size(10.0),
                            );
                            if ui.color_edit_button_rgba_unmultiplied(color).changed() {
                                changed = true;
                            }
                        });
                    }
                    ColorFill::LinearGradient { angle, stops } => {
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new("Angle")
                                    .color(theme.text_secondary)
                                    .size(10.0),
                            );
                            if ui
                                .add(egui::Slider::new(angle, 0.0..=360.0).suffix("°"))
                                .changed()
                            {
                                changed = true;
                            }
                        });
                        ui.add_space(2.0);
                        changed |= draw_gradient_stops(ui, stops, selected_id);
                    }
                    ColorFill::RadialGradient {
                        center,
                        radius,
                        stops,
                    } => {
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new("Center X")
                                    .color(theme.text_secondary)
                                    .size(10.0),
                            );
                            if ui
                                .add(egui::Slider::new(&mut center.0, 0.0..=1.0))
                                .changed()
                            {
                                changed = true;
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new("Center Y")
                                    .color(theme.text_secondary)
                                    .size(10.0),
                            );
                            if ui
                                .add(egui::Slider::new(&mut center.1, 0.0..=1.0))
                                .changed()
                            {
                                changed = true;
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new("Radius")
                                    .color(theme.text_secondary)
                                    .size(10.0),
                            );
                            if ui.add(egui::Slider::new(radius, 0.01..=2.0)).changed() {
                                changed = true;
                            }
                        });
                        ui.add_space(2.0);
                        changed |= draw_gradient_stops(ui, stops, selected_id);
                    }
                }

                // Re-render on change.
                if changed {
                    let w = state.library[lib_idx].transform.width as u32;
                    let h = state.library[lib_idx].transform.height as u32;
                    let fill_clone = if let SourceProperties::Color { ref fill } =
                        state.library[lib_idx].properties
                    {
                        fill.clone()
                    } else {
                        ColorFill::Solid {
                            color: [1.0, 1.0, 1.0, 1.0],
                        }
                    };
                    let frame = crate::color_source::render_color_source(&fill_clone, w, h);
                    if let Some(ref tx) = cmd_tx {
                        let _ = tx.try_send(GstCommand::LoadImageFrame {
                            source_id: src_id,
                            frame,
                        });
                    }
                }
            }
        }
        SourceType::Audio => {
            section_label(ui, "AUDIO");
            ui.add_space(4.0);

            // Cache audio devices (same pattern as Window/Camera panels).
            if state.available_audio_devices.is_empty() {
                state.available_audio_devices =
                    crate::gstreamer::devices::enumerate_audio_input_devices().unwrap_or_default();
            }
            let audio_devices = state.available_audio_devices.clone();
            let cmd_tx = state.command_tx.clone();
            let src_id = selected_id;
            let source = &mut state.library[lib_idx];
            if let SourceProperties::Audio { ref mut input } = source.properties {
                // Input type toggle.
                let is_device = matches!(input, AudioInput::Device { .. });
                ui.horizontal(|ui| {
                    if ui.selectable_label(is_device, "Device").clicked() && !is_device {
                        *input = AudioInput::Device {
                            device_uid: String::new(),
                            device_name: String::new(),
                        };
                        changed = true;
                    }
                    if ui.selectable_label(!is_device, "File").clicked() && is_device {
                        *input = AudioInput::File {
                            path: String::new(),
                            looping: false,
                        };
                        changed = true;
                    }
                });
                ui.add_space(4.0);

                match input {
                    AudioInput::Device {
                        device_uid,
                        device_name,
                    } => {
                        let current_label = if device_name.is_empty() {
                            "Select device...".to_string()
                        } else {
                            device_name.clone()
                        };
                        let prev_uid = device_uid.clone();

                        ui.horizontal(|ui| {
                            egui::ComboBox::from_id_salt(
                                egui::Id::new("props_audio_device").with(selected_id.0),
                            )
                            .selected_text(&current_label)
                            .width(ui.available_width() - 40.0)
                            .show_ui(ui, |ui| {
                                for dev in &audio_devices {
                                    if ui
                                        .selectable_label(*device_uid == dev.uid, &dev.name)
                                        .clicked()
                                    {
                                        *device_uid = dev.uid.clone();
                                        *device_name = dev.name.clone();
                                    }
                                }
                            });
                            if ui
                                .button(egui_phosphor::regular::ARROWS_CLOCKWISE)
                                .on_hover_text("Refresh devices")
                                .clicked()
                            {
                                state.available_audio_devices =
                                    crate::gstreamer::devices::enumerate_audio_input_devices()
                                        .unwrap_or_default();
                            }
                        });

                        if *device_uid != prev_uid {
                            // Restart capture with new device.
                            if let Some(ref tx) = cmd_tx {
                                let _ = tx.try_send(GstCommand::RemoveCaptureSource {
                                    source_id: src_id,
                                });
                                if !device_uid.is_empty() {
                                    let _ = tx.try_send(GstCommand::AddCaptureSource {
                                        source_id: src_id,
                                        config: CaptureSourceConfig::AudioDevice {
                                            device_uid: device_uid.clone(),
                                        },
                                    });
                                }
                            }
                            changed = true;
                        }
                    }
                    AudioInput::File { path, looping } => {
                        let prev_path = path.clone();
                        ui.horizontal(|ui| {
                            let te = egui::TextEdit::singleline(path)
                                .hint_text("Select audio file...")
                                .desired_width(ui.available_width() - 40.0);
                            if ui.add(te).changed() {
                                changed = true;
                            }
                            if ui
                                .button(egui_phosphor::regular::FOLDER)
                                .on_hover_text("Browse for audio file")
                                .clicked()
                                && let Some(picked) = rfd::FileDialog::new()
                                    .add_filter(
                                        "Audio",
                                        &["mp3", "wav", "ogg", "flac", "aac", "m4a"],
                                    )
                                    .pick_file()
                            {
                                *path = picked.to_string_lossy().to_string();
                                changed = true;
                            }
                        });
                        if ui.checkbox(looping, "Loop").changed() {
                            changed = true;
                        }

                        if changed && *path != prev_path && !path.is_empty() {
                            // Restart capture with new file.
                            if let Some(ref tx) = cmd_tx {
                                let _ = tx.try_send(GstCommand::RemoveCaptureSource {
                                    source_id: src_id,
                                });
                                let _ = tx.try_send(GstCommand::AddCaptureSource {
                                    source_id: src_id,
                                    config: CaptureSourceConfig::AudioFile {
                                        path: path.clone(),
                                        looping: *looping,
                                    },
                                });
                            }
                        }
                    }
                }
            }

            ui.add_space(8.0);

            // Volume and mute controls (always shown, use library-level fields).
            let source = &mut state.library[lib_idx];
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Volume")
                        .color(theme.text_secondary)
                        .size(10.0),
                );
                let prev_vol = source.volume;
                if ui
                    .add(egui::Slider::new(&mut source.volume, 0.0..=2.0).suffix("x"))
                    .changed()
                    && (source.volume - prev_vol).abs() > f32::EPSILON
                {
                    if let Some(ref tx) = cmd_tx {
                        let _ = tx.try_send(GstCommand::SetSourceVolume {
                            source_id: src_id,
                            volume: source.volume,
                        });
                    }
                    changed = true;
                }
            });

            let prev_muted = source.muted;
            if ui.checkbox(&mut source.muted, "Mute").changed() && source.muted != prev_muted {
                if let Some(ref tx) = cmd_tx {
                    let _ = tx.try_send(GstCommand::SetSourceMuted {
                        source_id: src_id,
                        muted: source.muted,
                    });
                }
                changed = true;
            }
        }
        SourceType::Browser => {
            section_label(ui, "BROWSER");
            ui.add_space(4.0);

            let cmd_tx = state.command_tx.clone();
            let src_id = selected_id;
            let source = &mut state.library[lib_idx];
            if let SourceProperties::Browser {
                ref mut url,
                ref mut width,
                ref mut height,
            } = source.properties
            {
                // URL input.
                let te = egui::TextEdit::singleline(url)
                    .hint_text("https://example.com")
                    .desired_width(ui.available_width() - 8.0);
                if ui.add(te).changed() {
                    changed = true;
                }
                ui.add_space(4.0);

                // Width / Height.
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("W")
                            .color(theme.text_secondary)
                            .size(10.0),
                    );
                    if ui
                        .add(egui::DragValue::new(width).range(100..=3840).speed(1.0))
                        .changed()
                    {
                        changed = true;
                    }
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new("H")
                            .color(theme.text_secondary)
                            .size(10.0),
                    );
                    if ui
                        .add(egui::DragValue::new(height).range(100..=2160).speed(1.0))
                        .changed()
                    {
                        changed = true;
                    }
                });
                ui.add_space(8.0);

                ui.label(
                    egui::RichText::new("Browser rendering engine not yet available.")
                        .color(theme.text_muted)
                        .size(10.0),
                );

                // Generate placeholder on change.
                if changed {
                    let frame = generate_browser_placeholder(*width, *height);
                    let source = &mut state.library[lib_idx];
                    source.native_size = (frame.width as f32, frame.height as f32);
                    source.transform.width = frame.width as f32;
                    source.transform.height = frame.height as f32;
                    if let Some(ref tx) = cmd_tx {
                        let _ = tx.try_send(GstCommand::LoadImageFrame {
                            source_id: src_id,
                            frame,
                        });
                    }
                }
            }
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
            .circle_filled(rect.center(), size / 2.0, active_theme(ui.ctx()).accent);
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

/// Render a section heading in the style: 9px uppercase text_muted with letter spacing.
fn section_label(ui: &mut egui::Ui, text: &str) {
    let theme = active_theme(ui.ctx());
    ui.label(egui::RichText::new(text).color(theme.text_muted).size(9.0));
}

/// Render a labeled `DragValue` field and return whether the value changed.
fn drag_field(ui: &mut egui::Ui, label: &str, value: &mut f32) -> bool {
    let theme = active_theme(ui.ctx());
    ui.label(
        egui::RichText::new(label)
            .color(theme.text_muted)
            .size(10.0),
    );
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

/// Draw the aspect-ratio lock toggle between W and H. Returns `true` if clicked.
fn aspect_lock_button(ui: &mut egui::Ui, locked: bool) -> bool {
    let theme = active_theme(ui.ctx());
    let icon = if locked {
        egui_phosphor::regular::LOCK_SIMPLE
    } else {
        egui_phosphor::regular::LOCK_SIMPLE_OPEN
    };
    let color = if locked {
        theme.text_primary
    } else {
        theme.text_muted
    };
    ui.add(egui::Button::new(egui::RichText::new(icon).size(12.0).color(color)).frame(false))
        .on_hover_text(if locked {
            "Unlock aspect ratio"
        } else {
            "Lock aspect ratio"
        })
        .clicked()
}

/// Adjust width or height to preserve aspect ratio after one of them changed.
///
/// Compares current values against `prev_w`/`prev_h` to decide which axis
/// was edited, then scales the other axis proportionally.
fn enforce_aspect_ratio(w: &mut f32, h: &mut f32, prev_w: f32, prev_h: f32) {
    if prev_w.abs() < f32::EPSILON || prev_h.abs() < f32::EPSILON {
        return;
    }
    let ratio = prev_w / prev_h;
    let w_changed = (*w - prev_w).abs() > f32::EPSILON;
    let h_changed = (*h - prev_h).abs() > f32::EPSILON;
    if w_changed && !h_changed {
        *h = *w / ratio;
    } else if h_changed {
        *w = *h * ratio;
    }
}

/// Draw gradient stop editor UI. Returns `true` if any stop was modified.
fn draw_gradient_stops(
    ui: &mut egui::Ui,
    stops: &mut Vec<GradientStop>,
    _source_id: SourceId,
) -> bool {
    let theme = active_theme(ui.ctx());
    let mut changed = false;
    let mut remove_idx: Option<usize> = None;

    ui.label(
        egui::RichText::new("Gradient Stops")
            .color(theme.text_secondary)
            .size(10.0),
    );
    ui.add_space(2.0);

    let stop_count = stops.len();
    for (i, stop) in stops.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(format!("#{}", i + 1))
                    .color(theme.text_muted)
                    .size(9.0),
            );
            if ui
                .color_edit_button_rgba_unmultiplied(&mut stop.color)
                .changed()
            {
                changed = true;
            }
            if ui
                .add(egui::Slider::new(&mut stop.position, 0.0..=1.0))
                .changed()
            {
                changed = true;
            }
            // Only allow removal if more than 2 stops.
            if stop_count > 2
                && ui
                    .small_button(egui_phosphor::regular::X)
                    .on_hover_text("Remove stop")
                    .clicked()
            {
                remove_idx = Some(i);
            }
        });
    }

    if let Some(idx) = remove_idx {
        stops.remove(idx);
        changed = true;
    }

    if ui
        .small_button(
            egui::RichText::new(format!("{} Add Stop", egui_phosphor::regular::PLUS)).size(10.0),
        )
        .clicked()
    {
        stops.push(GradientStop {
            position: 0.5,
            color: [0.5, 0.5, 0.5, 1.0],
        });
        changed = true;
    }

    changed
}

/// Generate a placeholder frame for a browser source.
///
/// Fills with a dark background (#1a1a2e) at the given dimensions.
pub fn generate_browser_placeholder(width: u32, height: u32) -> RgbaFrame {
    let w = width.max(1) as usize;
    let h = height.max(1) as usize;
    let pixel: [u8; 4] = [0x1a, 0x1a, 0x2e, 0xff];
    let mut data = vec![0u8; w * h * 4];
    for chunk in data.chunks_exact_mut(4) {
        chunk.copy_from_slice(&pixel);
    }
    RgbaFrame {
        data,
        width: w as u32,
        height: h as u32,
    }
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
