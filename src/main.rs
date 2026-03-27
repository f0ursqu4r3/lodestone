mod color_source;
mod gstreamer;
mod image_source;
mod renderer;
mod scene;
mod settings;
mod state;
mod text_source;
mod ui;
mod window;
mod window_actions;

use anyhow::Result;
use muda::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem, Submenu};
use renderer::SharedGpuState;
use state::AppState;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
#[allow(unused_imports)]
use ui::layout::{
    DockLayout, PanelType, SplitDirection, deserialize_full_layout, serialize_full_layout,
};
use ui::preview_panel::PreviewResources;
use window::{DetachRequest, WindowState};
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalSize, PhysicalSize},
    event::{KeyEvent, WindowEvent},
    event_loop::EventLoop,
    keyboard::{KeyCode, ModifiersState, PhysicalKey},
    window::{Window, WindowAttributes, WindowId},
};

/// Holds the native menu bar and IDs for each menu item.
struct NativeMenu {
    #[allow(dead_code)]
    menu: Menu,
    undo: MenuId,
    redo: MenuId,
    add_preview: MenuId,
    add_library: MenuId,
    add_audio_mixer: MenuId,
    add_stream_controls: MenuId,
    reset_layout: MenuId,
}

impl NativeMenu {
    fn build() -> Self {
        let menu = Menu::new();

        // App menu (macOS: first submenu becomes the application menu)
        let app_menu = Submenu::new("Lodestone", true);
        app_menu.append(&PredefinedMenuItem::about(None, None)).ok();
        app_menu.append(&PredefinedMenuItem::separator()).ok();
        app_menu.append(&PredefinedMenuItem::services(None)).ok();
        app_menu.append(&PredefinedMenuItem::separator()).ok();
        app_menu.append(&PredefinedMenuItem::hide(None)).ok();
        app_menu.append(&PredefinedMenuItem::hide_others(None)).ok();
        app_menu.append(&PredefinedMenuItem::show_all(None)).ok();
        app_menu.append(&PredefinedMenuItem::separator()).ok();
        app_menu.append(&PredefinedMenuItem::quit(None)).ok();
        menu.append(&app_menu).ok();

        // File menu
        let file_menu = Submenu::new("File", true);
        file_menu
            .append(&PredefinedMenuItem::close_window(None))
            .ok();
        menu.append(&file_menu).ok();

        // Edit menu — use custom items for Undo/Redo so we receive menu events
        // (PredefinedMenuItem::undo/redo are handled by the macOS responder chain
        // and never reach our event handler).
        let edit_menu = Submenu::new("Edit", true);
        let undo_item = MenuItem::new(
            "Undo",
            true,
            Some(muda::accelerator::Accelerator::new(
                Some(muda::accelerator::Modifiers::SUPER),
                muda::accelerator::Code::KeyZ,
            )),
        );
        let redo_item = MenuItem::new(
            "Redo",
            true,
            Some(muda::accelerator::Accelerator::new(
                Some(muda::accelerator::Modifiers::SUPER | muda::accelerator::Modifiers::SHIFT),
                muda::accelerator::Code::KeyZ,
            )),
        );
        edit_menu.append(&undo_item).ok();
        edit_menu.append(&redo_item).ok();
        edit_menu.append(&PredefinedMenuItem::separator()).ok();
        edit_menu.append(&PredefinedMenuItem::cut(None)).ok();
        edit_menu.append(&PredefinedMenuItem::copy(None)).ok();
        edit_menu.append(&PredefinedMenuItem::paste(None)).ok();
        edit_menu.append(&PredefinedMenuItem::select_all(None)).ok();
        menu.append(&edit_menu).ok();

        // View menu
        let view_menu = Submenu::new("View", true);
        let add_panel_menu = Submenu::new("Add Panel", true);

        let add_preview = MenuItem::new("Preview", true, None);
        let add_library = MenuItem::new("Library", true, None);
        let add_audio_mixer = MenuItem::new("Audio Mixer", true, None);
        let add_stream_controls = MenuItem::new("Stream Controls", true, None);

        add_panel_menu.append(&add_preview).ok();
        add_panel_menu.append(&add_library).ok();
        add_panel_menu.append(&add_audio_mixer).ok();
        add_panel_menu.append(&add_stream_controls).ok();

        let reset_layout = MenuItem::new("Reset Layout", true, None);

        view_menu.append(&add_panel_menu).ok();
        view_menu.append(&reset_layout).ok();
        view_menu.append(&PredefinedMenuItem::separator()).ok();
        view_menu.append(&PredefinedMenuItem::fullscreen(None)).ok();

        menu.append(&view_menu).ok();

        // Window menu
        let window_menu = Submenu::new("Window", true);
        window_menu.append(&PredefinedMenuItem::minimize(None)).ok();
        window_menu.append(&PredefinedMenuItem::maximize(None)).ok();
        menu.append(&window_menu).ok();

        Self {
            menu,
            undo: undo_item.id().clone(),
            redo: redo_item.id().clone(),
            add_preview: add_preview.id().clone(),
            add_library: add_library.id().clone(),
            add_audio_mixer: add_audio_mixer.id().clone(),
            add_stream_controls: add_stream_controls.id().clone(),
            reset_layout: reset_layout.id().clone(),
        }
    }

    /// Map a menu event ID to a panel type for the "Add Panel" action.
    fn panel_type_for_id(&self, id: &MenuId) -> Option<PanelType> {
        if *id == self.add_preview {
            Some(PanelType::Preview)
        } else if *id == self.add_library {
            Some(PanelType::Library)
        } else if *id == self.add_audio_mixer {
            Some(PanelType::AudioMixer)
        } else if *id == self.add_stream_controls {
            Some(PanelType::StreamControls)
        } else {
            None
        }
    }
}

struct AppManager {
    gpu: Option<SharedGpuState>,
    windows: HashMap<WindowId, WindowState>,
    main_window_id: Option<WindowId>,
    state: Arc<Mutex<AppState>>,
    #[allow(dead_code)]
    runtime: tokio::runtime::Runtime,
    gst_channels: Option<gstreamer::GstChannels>,
    #[allow(dead_code)]
    gst_thread: Option<std::thread::JoinHandle<()>>,
    pending_detaches: Vec<DetachRequest>,
    modifiers: ModifiersState,
    native_menu: Option<NativeMenu>,
    focused_window_id: Option<WindowId>,
    settings_window_id: Option<WindowId>,
    pending_settings_window: bool,
}

impl AppManager {
    fn new() -> Self {
        let runtime = tokio::runtime::Runtime::new().expect("create tokio runtime");

        // Create GStreamer channels and spawn the GStreamer thread.
        let (main_channels, thread_channels) = gstreamer::create_channels();
        let gst_handle = gstreamer::spawn_gstreamer_thread(thread_channels);

        // Enumerate cameras on the main thread (gstreamer::init is idempotent).
        if let Err(e) = ::gstreamer::init() {
            log::error!("Failed to initialize GStreamer on main thread: {e}");
        }
        let available_cameras = match crate::gstreamer::devices::enumerate_cameras() {
            Ok(cams) => {
                log::info!("Found {} camera(s)", cams.len());
                cams
            }
            Err(e) => {
                log::warn!("Failed to enumerate cameras: {e}");
                Vec::new()
            }
        };
        let available_windows = crate::gstreamer::devices::enumerate_windows();
        log::info!("Found {} window(s)", available_windows.len());

        // Enumerate displays for resolution detection.
        let available_displays = {
            #[cfg(target_os = "macos")]
            {
                match crate::gstreamer::screencapturekit::enumerate_displays() {
                    Ok(displays) => {
                        log::info!("Found {} display(s)", displays.len());
                        displays
                    }
                    Err(e) => {
                        log::warn!("Failed to enumerate displays: {e}");
                        Vec::new()
                    }
                }
            }
            #[cfg(not(target_os = "macos"))]
            {
                Vec::new()
            }
        };
        let detected_resolution = available_displays.first().map(|d| (d.width, d.height));

        use crate::scene::SceneCollection;
        let scenes_path = settings::scenes_path();
        let collection = SceneCollection::load_from(&scenes_path);
        // Save default scenes on first run so the file exists
        if !scenes_path.exists() {
            let _ = collection.save_to(&scenes_path);
        }
        // Load persisted settings (stream key, encoder, resolution, etc.).
        // On first launch, use detected monitor resolution for defaults.
        let saved_settings = settings::AppSettings::load_or_detect(
            &settings::settings_path(),
            detected_resolution,
        );

        let initial_state = AppState {
            scenes: collection.scenes,
            library: collection.library,
            active_scene_id: collection.active_scene_id,
            next_scene_id: collection.next_scene_id,
            next_source_id: collection.next_source_id,
            command_tx: Some(main_channels.command_tx.clone()),
            available_cameras,
            available_windows,
            available_displays,
            detected_resolution,
            settings: saved_settings,
            ..AppState::default()
        };

        let state = Arc::new(Mutex::new(initial_state));

        // system_fonts is populated by WindowState on first render (from actually-loaded fonts).

        Self {
            gpu: None,
            windows: HashMap::new(),
            main_window_id: None,
            state,
            runtime,
            gst_channels: Some(main_channels),
            gst_thread: Some(gst_handle),
            pending_detaches: Vec::new(),
            modifiers: ModifiersState::empty(),
            native_menu: None,
            focused_window_id: None,
            settings_window_id: None,
            pending_settings_window: false,
        }
    }

    /// Try to load a saved layout from disk. Falls back to default.
    ///
    /// If the saved layout is missing essential panels (e.g. Library was added
    /// in a newer version), the missing panels are injected and the layout is
    /// re-saved immediately.
    fn load_layout() -> DockLayout {
        let path = settings::config_dir().join("layout.toml");
        if path.exists()
            && let Ok(contents) = std::fs::read_to_string(&path)
        {
            match deserialize_full_layout(&contents) {
                Ok((mut layout, _detached)) => {
                    log::info!("Loaded layout from {}", path.display());
                    let modified = Self::ensure_essential_panels(&mut layout);
                    if modified {
                        // Re-save so the injected panels persist.
                        if let Ok(toml_str) = serialize_full_layout(&layout, &[]) {
                            let _ = std::fs::write(&path, toml_str);
                            log::info!("Re-saved layout after injecting missing panels");
                        }
                    }
                    return layout;
                }
                Err(e) => {
                    log::warn!("Failed to parse layout.toml, using default: {e}");
                }
            }
        }
        DockLayout::default_layout()
    }

    /// Ensure essential panels exist in the layout, injecting missing ones
    /// as tabs in appropriate existing groups. Returns `true` if the layout was modified.
    fn ensure_essential_panels(layout: &mut DockLayout) -> bool {
        let all_panels = layout.collect_all_panels();
        let has = |pt: PanelType| all_panels.iter().any(|(_, t)| *t == pt);
        let mut modified = false;

        // Library panel: add as a tab alongside Sources if missing.
        if !has(PanelType::Library) {
            let target_group = layout
                .groups
                .iter()
                .find(|(_, g)| g.tabs.iter().any(|t| t.panel_type == PanelType::Sources))
                .map(|(gid, _)| *gid);

            if let Some(gid) = target_group {
                if let Some(group) = layout.groups.get_mut(&gid) {
                    group.add_tab(PanelType::Library);
                    log::info!("Injected Library panel into Sources group");
                    modified = true;
                }
            } else if let Some(group) = layout.groups.values_mut().next() {
                group.add_tab(PanelType::Library);
                log::info!("Injected Library panel into first available group");
                modified = true;
            }
        }

        modified
    }

    /// Save the current main window layout to disk.
    fn save_layout(&self) {
        let Some(main_id) = self.main_window_id else {
            return;
        };
        let Some(main_win) = self.windows.get(&main_id) else {
            return;
        };

        // Collect detached window info (stub — no detached windows in new model yet).
        let detached = Vec::new();

        match serialize_full_layout(&main_win.layout, &detached) {
            Ok(toml_str) => {
                let path = settings::config_dir().join("layout.toml");
                if let Some(parent) = path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                if let Err(e) = std::fs::write(&path, toml_str) {
                    log::warn!("Failed to save layout: {e}");
                }
            }
            Err(e) => {
                log::warn!("Failed to serialize layout: {e}");
            }
        }
    }

    /// Handle a native menu event by mapping the menu item ID to a layout action.
    fn handle_menu_event(&mut self, id: &MenuId) {
        let Some(native_menu) = &self.native_menu else {
            return;
        };

        // Undo / Redo via Edit menu
        if *id == native_menu.undo || *id == native_menu.redo {
            let is_redo = *id == native_menu.redo;
            let mut app_state = self.state.lock().unwrap();
            let restored = if is_redo {
                app_state.redo()
            } else {
                app_state.undo()
            };
            if restored {
                self.reconcile_captures(&app_state);
            }
            return;
        }

        if *id == native_menu.reset_layout {
            self.reset_layout();
            return;
        }

        if let Some(panel_type) = native_menu.panel_type_for_id(id) {
            // Add panel to the currently focused window, falling back to main
            let target_id = match self.focused_window_id.or(self.main_window_id) {
                Some(id) => id,
                None => return,
            };
            if let Some(win) = self.windows.get_mut(&target_id) {
                // Find the first group and add as a tab, or insert at root
                let first_group = win.layout.groups.keys().next().copied();
                if let Some(gid) = first_group {
                    if let Some(group) = win.layout.groups.get_mut(&gid) {
                        group.add_tab(panel_type);
                    }
                } else {
                    win.layout.insert_at_root(
                        panel_type,
                        ui::layout::PanelId::next(),
                        SplitDirection::Vertical,
                        0.8,
                    );
                }
                self.save_layout();
            }
        }
    }

    /// Reset the main window layout to default and close all detached windows.
    fn reset_layout(&mut self) {
        // Close all detached windows by collecting their IDs.
        if let Some(main_id) = self.main_window_id {
            let settings_id = self.settings_window_id;
            let detached_ids: Vec<WindowId> = self
                .windows
                .keys()
                .filter(|id| **id != main_id && Some(**id) != settings_id)
                .copied()
                .collect();
            for id in detached_ids {
                self.windows.remove(&id);
            }

            // Reset the main layout.
            if let Some(main_win) = self.windows.get_mut(&main_id) {
                main_win.layout = DockLayout::default_layout();
            }
        }
        self.save_layout();
        log::info!("Layout reset to default");
    }

    /// Refresh SCK display capture exclusion filter after a window is created or destroyed.
    fn refresh_display_exclusion(&self) {
        let state = self.state.lock().unwrap();
        if state.settings.general.exclude_self_from_capture
            && let Some(tx) = &state.command_tx
        {
            let _ =
                tx.try_send(gstreamer::GstCommand::UpdateDisplayExclusion { exclude_self: true });
        }
    }

    /// Reconcile GStreamer captures after undo/redo by stopping all captures
    /// and restarting those in the active scene.
    fn reconcile_captures(&self, app_state: &AppState) {
        let cmd_tx = &app_state.command_tx;
        if let Some(tx) = cmd_tx {
            let _ = tx.try_send(crate::gstreamer::GstCommand::StopCapture);
        }
        if let Some(scene) = app_state.active_scene() {
            let scene = scene.clone();
            crate::ui::scenes_panel::send_capture_for_scene(
                cmd_tx,
                &app_state.library,
                &scene,
                app_state.settings.general.exclude_self_from_capture,
            );
        }
    }

    /// Close the settings window if it's open.
    fn close_settings_window(&mut self) {
        if let Some(settings_id) = self.settings_window_id.take()
            && let Some(win) = self.windows.remove(&settings_id)
        {
            let win_ptr = win.window as *const Window as *mut Window;
            // SAFETY: the pointer was created via Box::leak(Box::new(window))
            // in `about_to_wait`, and we are the sole owner.
            unsafe {
                drop(Box::from_raw(win_ptr));
            }
            self.refresh_display_exclusion();
        }
    }
}

impl ApplicationHandler for AppManager {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        if self.main_window_id.is_some() {
            return;
        }

        let attrs = WindowAttributes::default()
            .with_title("Lodestone")
            .with_inner_size(LogicalSize::new(1280.0, 720.0))
            .with_min_inner_size(LogicalSize::new(960.0, 540.0));
        let window = event_loop.create_window(attrs).expect("create window");
        let window: &'static Window = Box::leak(Box::new(window));
        let window_id = window.id();

        // Create shared GPU state from the main window
        let mut gpu =
            pollster::block_on(SharedGpuState::new(window)).expect("initialize shared GPU state");

        // Resize compositor to match saved resolution settings (GPU init uses 1920x1080 default).
        {
            let app_state = self.state.lock().expect("lock AppState");
            let base = crate::renderer::compositor::parse_resolution(
                &app_state.settings.video.base_resolution,
            );
            let output = crate::renderer::compositor::parse_resolution(
                &app_state.settings.video.output_resolution,
            );
            if base != (1920, 1080) || output != (1920, 1080) {
                gpu.compositor.resize(&gpu.device, base, output);
            }
        }

        let preview_resources = PreviewResources {
            pipeline: gpu.compositor.canvas_pipeline(),
            bind_group: gpu.compositor.canvas_bind_group(),
        };

        // Try to load saved layout; fall back to default.
        let layout = Self::load_layout();
        let win_state = WindowState::new(window, &gpu, layout, true, Some(preview_resources))
            .expect("create main window state");

        self.gpu = Some(gpu);
        self.main_window_id = Some(window_id);
        self.windows.insert(window_id, win_state);

        // Attach native menu bar — must happen after window is fully initialized.
        let native_menu = NativeMenu::build();
        #[cfg(target_os = "macos")]
        {
            native_menu.menu.init_for_nsapp();
        }
        #[cfg(target_os = "windows")]
        {
            let _ = unsafe { native_menu.menu.init_for_hwnd(window.rwh().hwnd) };
        }
        self.native_menu = Some(native_menu);

        // Store monitor count
        {
            let monitor_count = event_loop.available_monitors().count().max(1);
            let mut state = self.state.lock().unwrap();
            state.monitor_count = monitor_count;
        }

        // Send initial capture commands for all sources in the active scene
        {
            let state = self.state.lock().unwrap();
            if let Some(scene_id) = state.active_scene_id
                && let Some(scene) = state.scenes.iter().find(|s| s.id == scene_id)
            {
                for src_id in scene.source_ids() {
                    if let Some(source) = state.library.iter().find(|s| s.id == src_id) {
                        match &source.properties {
                            crate::scene::SourceProperties::Display { screen_index } => {
                                if let Some(ref tx) = state.command_tx {
                                    let _ = tx.try_send(gstreamer::GstCommand::AddCaptureSource {
                                        source_id: src_id,
                                        config: gstreamer::CaptureSourceConfig::Screen {
                                            screen_index: *screen_index,
                                            exclude_self: state
                                                .settings
                                                .general
                                                .exclude_self_from_capture,
                                        },
                                    });
                                }
                            }
                            crate::scene::SourceProperties::Window { mode, .. } => {
                                if let Some(ref tx) = state.command_tx {
                                    let _ = tx.try_send(gstreamer::GstCommand::AddCaptureSource {
                                        source_id: src_id,
                                        config: gstreamer::CaptureSourceConfig::Window {
                                            mode: mode.clone(),
                                        },
                                    });
                                }
                            }
                            crate::scene::SourceProperties::Camera { device_index, .. } => {
                                if let Some(ref tx) = state.command_tx {
                                    let _ = tx.try_send(gstreamer::GstCommand::AddCaptureSource {
                                        source_id: src_id,
                                        config: gstreamer::CaptureSourceConfig::Camera {
                                            device_index: *device_index,
                                        },
                                    });
                                }
                            }
                            crate::scene::SourceProperties::Image { .. } => {
                                // Image sources don't use a capture pipeline;
                                // frames are loaded via LoadImageFrame.
                            }
                            _ => {
                                // Text, Color, Audio, Browser sources don't use a capture pipeline yet.
                            }
                        }
                    }
                }
            }
        }

        log::info!("Window and renderer initialized");
    }

    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        // Route event to the correct window
        if let Some(win) = self.windows.get_mut(&window_id) {
            let _ = win.egui_state.on_window_event(win.window, &event);
        }

        match &event {
            WindowEvent::Focused(true) => {
                self.focused_window_id = Some(window_id);
            }
            WindowEvent::ModifiersChanged(mods) => {
                self.modifiers = mods.state();
            }
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        physical_key: PhysicalKey::Code(key_code),
                        state,
                        ..
                    },
                ..
            } if state.is_pressed() => {
                let ctrl = self.modifiers.control_key();
                let shift = self.modifiers.shift_key();
                if ctrl && shift && *key_code == KeyCode::KeyR {
                    self.reset_layout();
                    return;
                }
                if self.modifiers.super_key() && *key_code == KeyCode::Comma {
                    if self.settings_window_id.is_some() {
                        self.close_settings_window();
                    } else {
                        // Request settings window creation (deferred to about_to_wait)
                        self.pending_settings_window = true;
                    }
                    return;
                }
                // Escape closes the settings window when it's focused
                if *key_code == KeyCode::Escape && Some(window_id) == self.settings_window_id {
                    self.close_settings_window();
                    return;
                }

                // Undo / Redo (Cmd+Z / Cmd+Shift+Z) — fallback for non-macOS
                // or if the menu accelerator doesn't fire.
                if self.modifiers.super_key() && *key_code == KeyCode::KeyZ {
                    let mut app_state = self.state.lock().unwrap();
                    let restored = if shift {
                        app_state.redo()
                    } else {
                        app_state.undo()
                    };
                    if restored {
                        self.reconcile_captures(&app_state);
                    }
                    return;
                }

                // Cmd+0: Fit to panel (reset zoom/pan)
                if self.modifiers.super_key() && *key_code == KeyCode::Digit0 {
                    let mut app_state = self.state.lock().unwrap();
                    app_state.reset_preview_zoom = true;
                    return;
                }

                // Cmd+1: 100% zoom
                if self.modifiers.super_key() && *key_code == KeyCode::Digit1 {
                    let mut app_state = self.state.lock().unwrap();
                    app_state.set_preview_zoom_100 = true;
                    return;
                }

                // Arrow keys: nudge selected sources
                if matches!(
                    key_code,
                    KeyCode::ArrowUp
                        | KeyCode::ArrowDown
                        | KeyCode::ArrowLeft
                        | KeyCode::ArrowRight
                ) {
                    let egui_wants_input = self
                        .windows
                        .get(&window_id)
                        .map(|w| w.egui_ctx.wants_keyboard_input())
                        .unwrap_or(false);
                    if !egui_wants_input {
                        let mut app_state = self.state.lock().unwrap();
                        if app_state.selected_source_ids.is_empty() {
                            return;
                        }
                        if app_state.renaming_source_id.is_some()
                            || app_state.renaming_scene_id.is_some()
                        {
                            return;
                        }

                        let step = if shift { 10.0 } else { 1.0 };
                        let (dx, dy) = match key_code {
                            KeyCode::ArrowUp => (0.0, -step),
                            KeyCode::ArrowDown => (0.0, step),
                            KeyCode::ArrowLeft => (-step, 0.0),
                            KeyCode::ArrowRight => (step, 0.0),
                            _ => unreachable!(),
                        };

                        // Batch undo: only push snapshot if last nudge was >500ms ago
                        let now = std::time::Instant::now();
                        let batch = app_state
                            .last_nudge_time
                            .map(|t| now.duration_since(t).as_millis() < 500)
                            .unwrap_or(false);
                        if !batch {
                            app_state.mark_dirty();
                        }
                        app_state.last_nudge_time = Some(now);

                        // Apply delta to all selected sources.
                        // Snap is intentionally skipped here: the snap computation
                        // requires canvas_size, grid_size, and other-source transforms
                        // that are only available at preview-panel render time. Nudge
                        // moves 1px or 10px, making snap during pixel-perfect
                        // adjustments more annoying than helpful.
                        let ids = app_state.selected_source_ids.clone();
                        for id in ids {
                            let lib_transform =
                                app_state.find_library_source(id).map(|ls| ls.transform);
                            if let Some(scene) = app_state.active_scene_mut()
                                && let Some(ss) = scene.find_source_mut(id)
                            {
                                let mut t = match ss.overrides.transform {
                                    Some(t) => t,
                                    None => lib_transform.unwrap_or_default(),
                                };
                                t.x += dx;
                                t.y += dy;
                                ss.overrides.transform = Some(t);
                            }
                        }
                        app_state.scenes_dirty = true;
                        app_state.scenes_last_changed = std::time::Instant::now();
                        return;
                    }
                }

                // Cmd+A: Select all unlocked sources in the active scene
                if self.modifiers.super_key() && *key_code == KeyCode::KeyA {
                    let egui_wants_input = self
                        .windows
                        .get(&window_id)
                        .map(|w| w.egui_ctx.wants_keyboard_input())
                        .unwrap_or(false);
                    if !egui_wants_input {
                        let mut app_state = self.state.lock().unwrap();
                        if let Some(scene) = app_state.active_scene() {
                            let ids: Vec<crate::scene::SourceId> = scene
                                .sources
                                .iter()
                                .filter(|ss| !ss.resolve_locked())
                                .map(|ss| ss.source_id)
                                .collect();
                            app_state.selected_source_ids = ids;
                            app_state.primary_selected_id =
                                app_state.selected_source_ids.last().copied();
                        }
                        return;
                    }
                }

                // Cmd+C: Copy selected sources to clipboard
                if self.modifiers.super_key() && !shift && *key_code == KeyCode::KeyC {
                    let egui_wants_input = self
                        .windows
                        .get(&window_id)
                        .map(|w| w.egui_ctx.wants_keyboard_input())
                        .unwrap_or(false);
                    if !egui_wants_input {
                        let mut app_state = self.state.lock().unwrap();
                        app_state.clipboard.clear();
                        let sel_ids = app_state.selected_source_ids.clone();
                        if let Some(scene) = app_state.active_scene().cloned() {
                            for &id in &sel_ids {
                                if let Some(ss) = scene.find_source(id) {
                                    app_state.clipboard.push(crate::state::ClipboardEntry {
                                        library_source_id: ss.source_id,
                                        overrides_snapshot: ss.overrides.clone(),
                                    });
                                }
                            }
                        }
                        return;
                    }
                }

                // Cmd+V: Paste as reference (reuse existing library source)
                if self.modifiers.super_key() && !shift && *key_code == KeyCode::KeyV {
                    let egui_wants_input = self
                        .windows
                        .get(&window_id)
                        .map(|w| w.egui_ctx.wants_keyboard_input())
                        .unwrap_or(false);
                    if !egui_wants_input {
                        let mut app_state = self.state.lock().unwrap();
                        if app_state.clipboard.is_empty() {
                            return;
                        }
                        let entries = app_state.clipboard.clone();
                        let mut new_ids = Vec::new();
                        for entry in &entries {
                            let mut overrides = entry.overrides_snapshot.clone();
                            if let Some(ref mut t) = overrides.transform {
                                t.x += 20.0;
                                t.y += 20.0;
                            }
                            let ss = crate::scene::SceneSource {
                                source_id: entry.library_source_id,
                                overrides,
                            };
                            if let Some(scene) = app_state.active_scene_mut() {
                                scene.sources.push(ss);
                                new_ids.push(entry.library_source_id);
                            }
                        }
                        app_state.selected_source_ids = new_ids;
                        app_state.primary_selected_id =
                            app_state.selected_source_ids.last().copied();
                        app_state.mark_dirty();
                        return;
                    }
                }

                // Cmd+Shift+V: Paste as clone (create new library sources)
                if self.modifiers.super_key() && shift && *key_code == KeyCode::KeyV {
                    let egui_wants_input = self
                        .windows
                        .get(&window_id)
                        .map(|w| w.egui_ctx.wants_keyboard_input())
                        .unwrap_or(false);
                    if !egui_wants_input {
                        let mut app_state = self.state.lock().unwrap();
                        if app_state.clipboard.is_empty() {
                            return;
                        }
                        let entries = app_state.clipboard.clone();
                        let mut new_ids = Vec::new();
                        for entry in &entries {
                            if let Some(lib) = app_state
                                .find_library_source(entry.library_source_id)
                                .cloned()
                            {
                                let new_id = crate::scene::SourceId(app_state.next_source_id);
                                app_state.next_source_id += 1;
                                let mut new_lib = lib;
                                new_lib.id = new_id;
                                new_lib.name = format!("{} (Copy)", new_lib.name);
                                app_state.library.push(new_lib);

                                let mut overrides = entry.overrides_snapshot.clone();
                                if let Some(ref mut t) = overrides.transform {
                                    t.x += 20.0;
                                    t.y += 20.0;
                                }
                                let ss = crate::scene::SceneSource {
                                    source_id: new_id,
                                    overrides,
                                };
                                if let Some(scene) = app_state.active_scene_mut() {
                                    scene.sources.push(ss);
                                    new_ids.push(new_id);
                                }
                            }
                        }
                        app_state.selected_source_ids = new_ids;
                        app_state.primary_selected_id =
                            app_state.selected_source_ids.last().copied();
                        app_state.mark_dirty();
                        return;
                    }
                }

                // Cmd+D: Duplicate selected sources in current scene (clone)
                if self.modifiers.super_key() && *key_code == KeyCode::KeyD {
                    let egui_wants_input = self
                        .windows
                        .get(&window_id)
                        .map(|w| w.egui_ctx.wants_keyboard_input())
                        .unwrap_or(false);
                    if !egui_wants_input {
                        let mut app_state = self.state.lock().unwrap();
                        let ids = app_state.selected_source_ids.clone();
                        if ids.is_empty() {
                            return;
                        }
                        let scene_clone = app_state.active_scene().cloned();
                        let mut new_ids = Vec::new();
                        if let Some(scene_data) = scene_clone {
                            for id in &ids {
                                if let Some(ss) = scene_data.find_source(*id)
                                    && let Some(lib) =
                                        app_state.find_library_source(ss.source_id).cloned()
                                {
                                    let new_id = crate::scene::SourceId(app_state.next_source_id);
                                    app_state.next_source_id += 1;
                                    let mut new_lib = lib;
                                    new_lib.id = new_id;
                                    new_lib.name = format!("{} (Copy)", new_lib.name);
                                    app_state.library.push(new_lib);

                                    let mut overrides = ss.overrides.clone();
                                    if let Some(ref mut t) = overrides.transform {
                                        t.x += 20.0;
                                        t.y += 20.0;
                                    }
                                    let new_ss = crate::scene::SceneSource {
                                        source_id: new_id,
                                        overrides,
                                    };
                                    if let Some(scene) = app_state.active_scene_mut() {
                                        scene.sources.push(new_ss);
                                        new_ids.push(new_id);
                                    }
                                }
                            }
                        }
                        app_state.selected_source_ids = new_ids;
                        app_state.primary_selected_id =
                            app_state.selected_source_ids.last().copied();
                        app_state.mark_dirty();
                        return;
                    }
                }

                // Cmd+]: Bring forward / Cmd+Shift+]: Bring to front
                if self.modifiers.super_key() && *key_code == KeyCode::BracketRight {
                    let mut app_state = self.state.lock().unwrap();
                    let ids = app_state.selected_source_ids.clone();
                    if ids.is_empty() {
                        return;
                    }
                    if let Some(scene) = app_state.active_scene_mut() {
                        if shift {
                            // Bring to front — move each selected to end
                            for id in &ids {
                                if let Some(pos) =
                                    scene.sources.iter().position(|s| s.source_id == *id)
                                {
                                    let s = scene.sources.remove(pos);
                                    scene.sources.push(s);
                                }
                            }
                        } else {
                            for id in &ids {
                                scene.move_source_up(*id);
                            }
                        }
                    }
                    app_state.mark_dirty();
                    return;
                }

                // Cmd+[: Send backward / Cmd+Shift+[: Send to back
                if self.modifiers.super_key() && *key_code == KeyCode::BracketLeft {
                    let mut app_state = self.state.lock().unwrap();
                    let ids = app_state.selected_source_ids.clone();
                    if ids.is_empty() {
                        return;
                    }
                    if let Some(scene) = app_state.active_scene_mut() {
                        if shift {
                            // Send to back — move each to front (in reverse to preserve order)
                            for id in ids.iter().rev() {
                                if let Some(pos) =
                                    scene.sources.iter().position(|s| s.source_id == *id)
                                {
                                    let s = scene.sources.remove(pos);
                                    scene.sources.insert(0, s);
                                }
                            }
                        } else {
                            for id in &ids {
                                scene.move_source_down(*id);
                            }
                        }
                    }
                    app_state.mark_dirty();
                    return;
                }

                // Cmd+L: Toggle lock on selected sources
                if self.modifiers.super_key() && *key_code == KeyCode::KeyL {
                    let egui_wants_input = self
                        .windows
                        .get(&window_id)
                        .map(|w| w.egui_ctx.wants_keyboard_input())
                        .unwrap_or(false);
                    if !egui_wants_input {
                        let mut app_state = self.state.lock().unwrap();
                        let ids = app_state.selected_source_ids.clone();
                        if ids.is_empty() {
                            return;
                        }
                        if let Some(scene) = app_state.active_scene_mut() {
                            for id in ids {
                                if let Some(ss) = scene.find_source_mut(id) {
                                    let currently_locked = ss.resolve_locked();
                                    ss.overrides.locked = Some(!currently_locked);
                                }
                            }
                        }
                        app_state.mark_dirty();
                        return;
                    }
                }

                // DEL / Backspace deletes selected sources.
                // Skip if egui has keyboard focus (text fields, rename, etc.).
                if matches!(key_code, KeyCode::Delete | KeyCode::Backspace) {
                    let egui_wants_input = self
                        .windows
                        .get(&window_id)
                        .map(|w| w.egui_ctx.wants_keyboard_input())
                        .unwrap_or(false);
                    if !egui_wants_input {
                        let mut app_state = self.state.lock().unwrap();
                        // Don't delete while renaming.
                        if app_state.renaming_source_id.is_some()
                            || app_state.renaming_scene_id.is_some()
                        {
                            return;
                        }
                        if !app_state.selected_source_ids.is_empty() {
                            let ids = app_state.selected_source_ids.clone();
                            if let Some(scene_id) = app_state.active_scene_id {
                                let cmd_tx = app_state.command_tx.clone();
                                for id in ids {
                                    crate::ui::sources_panel::remove_source_from_scene(
                                        &mut app_state,
                                        &cmd_tx,
                                        scene_id,
                                        id,
                                    );
                                }
                            }
                            app_state.deselect_all();
                        } else if let Some(src_id) = app_state.selected_library_source_id {
                            // Library selection → cascade delete.
                            crate::ui::library_panel::delete_source_cascade(&mut app_state, src_id);
                        }
                        return;
                    }
                }
            }
            _ => {}
        }

        match event {
            WindowEvent::CloseRequested => {
                if Some(window_id) == self.main_window_id {
                    // Save layout and scenes before exiting.
                    self.save_layout();
                    {
                        let app_state = self.state.lock().unwrap();
                        if app_state.scenes_dirty {
                            let collection = crate::scene::SceneCollection {
                                scenes: app_state.scenes.clone(),
                                library: app_state.library.clone(),
                                active_scene_id: app_state.active_scene_id,
                                next_scene_id: app_state.next_scene_id,
                                next_source_id: app_state.next_source_id,
                            };
                            if let Err(e) = collection.save_to(&settings::scenes_path()) {
                                log::warn!("Failed to save scenes on exit: {e}");
                            }
                        }
                    }
                    event_loop.exit();
                } else if Some(window_id) == self.settings_window_id {
                    self.close_settings_window();
                } else {
                    // Close the detached window — panels are discarded, not reattached.
                    if let Some(detached_win) = self.windows.remove(&window_id) {
                        // Drop the leaked Window so the OS window actually closes.
                        // SAFETY: the pointer was created via Box::leak(Box::new(window))
                        // in `resumed` / `about_to_wait`, and we are the sole owner.
                        let win_ptr = detached_win.window as *const Window as *mut Window;
                        unsafe {
                            drop(Box::from_raw(win_ptr));
                        }
                        self.refresh_display_exclusion();
                    }
                    self.save_layout();
                }
            }
            WindowEvent::Resized(PhysicalSize { width, height }) => {
                if let (Some(gpu), Some(win)) = (&self.gpu, self.windows.get_mut(&window_id)) {
                    win.resize(gpu, width, height);
                }
            }
            WindowEvent::RedrawRequested => {
                if let Some(gpu) = &self.gpu
                    && let Some(win) = self.windows.get_mut(&window_id)
                {
                    if Some(window_id) == self.settings_window_id {
                        // Settings window render path
                        let mut app_state = self.state.lock().unwrap();
                        if let Err(e) = win.render_settings(gpu, &mut app_state) {
                            log::error!("Settings render error: {e}");
                        }
                        // Debounced settings persistence
                        if app_state.settings_dirty
                            && app_state.settings_last_changed.elapsed()
                                > std::time::Duration::from_millis(500)
                        {
                            let path = settings::settings_path();
                            if let Err(e) = app_state.settings.save_to(&path) {
                                log::warn!("Failed to save settings: {e}");
                            }
                            app_state.settings_dirty = false;
                        }
                        drop(app_state);
                    } else {
                        // Normal render path
                        let mut app_state = self.state.lock().unwrap();
                        let (layout_changed, toolbar_settings) =
                            match win.render(gpu, &mut app_state) {
                                Ok((detach_requests, open_settings)) => {
                                    let changed = !detach_requests.is_empty();
                                    self.pending_detaches.extend(detach_requests);
                                    (changed, open_settings)
                                }
                                Err(e) => {
                                    log::error!("Render error: {e}");
                                    (false, false)
                                }
                            };
                        drop(app_state);
                        if toolbar_settings {
                            if self.settings_window_id.is_some() {
                                self.close_settings_window();
                            } else {
                                self.pending_settings_window = true;
                            }
                        }
                        if layout_changed {
                            self.save_layout();
                        }

                        // Debounced settings persistence (main window)
                        if Some(window_id) == self.main_window_id {
                            let mut app_state = self.state.lock().unwrap();
                            if app_state.settings_dirty
                                && app_state.settings_last_changed.elapsed()
                                    > std::time::Duration::from_millis(500)
                            {
                                let path = settings::settings_path();
                                if let Err(e) = app_state.settings.save_to(&path) {
                                    log::warn!("Failed to save settings: {e}");
                                }
                                app_state.settings_dirty = false;
                            }

                            // Debounced scene persistence
                            if app_state.scenes_dirty
                                && app_state.scenes_last_changed.elapsed()
                                    > std::time::Duration::from_millis(500)
                            {
                                let collection = crate::scene::SceneCollection {
                                    scenes: app_state.scenes.clone(),
                                    library: app_state.library.clone(),
                                    active_scene_id: app_state.active_scene_id,
                                    next_scene_id: app_state.next_scene_id,
                                    next_source_id: app_state.next_source_id,
                                };
                                let path = settings::scenes_path();
                                if let Err(e) = collection.save_to(&path) {
                                    log::warn!("Failed to save scenes: {e}");
                                }
                                app_state.scenes_dirty = false;
                            }
                            drop(app_state);
                        }
                    }

                    // Check for reattach request from detached windows
                    if Some(window_id) != self.main_window_id
                        && self
                            .windows
                            .get(&window_id)
                            .is_some_and(|w| w.reattach_pending)
                    {
                        if let Some(detached_win) = self.windows.remove(&window_id)
                            && let Some(main_id) = self.main_window_id
                            && let Some(main_win) = self.windows.get_mut(&main_id)
                        {
                            let panels = detached_win.layout.collect_all_panels();
                            for (panel_id, panel_type) in panels {
                                main_win.layout.insert_at_root(
                                    panel_type,
                                    panel_id,
                                    SplitDirection::Vertical,
                                    0.5,
                                );
                            }
                            let win_ptr = detached_win.window as *const Window as *mut Window;
                            unsafe {
                                drop(Box::from_raw(win_ptr));
                            }
                        }
                        self.save_layout();
                    }
                }
                // Update detached window title to match the active panel name
                if Some(window_id) != self.main_window_id
                    && let Some(win) = self.windows.get(&window_id)
                {
                    let title = win
                        .layout
                        .groups
                        .values()
                        .next()
                        .map(|g| g.active_tab_entry().panel_type.display_name())
                        .unwrap_or("Lodestone");
                    win.window.set_title(title);
                }
                if let Some(win) = self.windows.get(&window_id) {
                    win.window.request_redraw();
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        // Poll GStreamer latest frames and upload to compositor source layers.
        // Drain the shared frame map into a local vec first so we can release
        // self.gst_channels borrow before mutably borrowing self.gpu.
        let drained_frames: Vec<_> = if let Some(ref channels) = self.gst_channels {
            channels
                .latest_frames
                .lock()
                .expect("lock latest_frames")
                .drain()
                .collect()
        } else {
            Vec::new()
        };
        let had_new_frames = !drained_frames.is_empty();
        if had_new_frames && let Some(ref mut gpu) = self.gpu {
            // Update native_size on sources when we first see their frame dimensions.
            {
                let mut app_state = self.state.lock().expect("lock AppState");
                for (source_id, frame) in &drained_frames {
                    let new_size = (frame.width as f32, frame.height as f32);
                    if let Some(s) = app_state.library.iter_mut().find(|s| s.id == *source_id)
                        && s.native_size != new_size
                    {
                        // Only update native_size from frame data if it was still the
                        // default placeholder. Sources with eagerly-detected resolutions
                        // (display via SCDisplay, camera via device caps) already have
                        // the correct native_size and should not be overwritten by the
                        // capture pipeline's output resolution.
                        let was_default = s.native_size == (1920.0, 1080.0);
                        if was_default {
                            s.native_size = new_size;
                            if new_size != (1920.0, 1080.0) {
                                s.transform.width = new_size.0;
                                s.transform.height = new_size.1;
                            }
                        }
                    }
                }
            }
            for (source_id, frame) in drained_frames {
                gpu.compositor
                    .upload_frame(&gpu.device, &gpu.queue, source_id, &frame);
            }
        }

        // Detect resolution changes from settings and resize compositor.
        if let Some(ref mut gpu) = self.gpu {
            let app_state = self.state.lock().expect("lock AppState");
            let new_base = crate::renderer::compositor::parse_resolution(
                &app_state.settings.video.base_resolution,
            );
            let new_output = crate::renderer::compositor::parse_resolution(
                &app_state.settings.video.output_resolution,
            );
            if new_base != (gpu.compositor.canvas_width, gpu.compositor.canvas_height)
                || new_output != (gpu.compositor.output_width, gpu.compositor.output_height)
            {
                gpu.compositor.resize(&gpu.device, new_base, new_output);
                // Update preview resources — the canvas bind group changed.
                if let Some(main_id) = self.main_window_id
                    && let Some(win) = self.windows.get_mut(&main_id)
                {
                    let new_resources = PreviewResources {
                        pipeline: gpu.compositor.canvas_pipeline(),
                        bind_group: gpu.compositor.canvas_bind_group(),
                    };
                    win.egui_renderer.callback_resources.insert(new_resources);
                }
            }
        }

        // Compose active scene sources onto the canvas.
        // upload_frame() (mut borrow) is finished above; compose() uses &self — no overlap.
        if let Some(ref gpu) = self.gpu {
            let app_state = self.state.lock().expect("lock AppState");
            if let Some(active_scene_id) = app_state.active_scene_id {
                // Resolve sources: apply per-scene overrides from SceneSource onto LibrarySource.
                let resolved_sources: Vec<crate::renderer::compositor::ResolvedSource> = app_state
                    .scenes
                    .iter()
                    .find(|s| s.id == active_scene_id)
                    .map(|scene| {
                        scene
                            .sources
                            .iter()
                            .filter_map(|scene_src| {
                                app_state
                                    .library
                                    .iter()
                                    .find(|s| s.id == scene_src.source_id)
                                    .map(|lib| crate::renderer::compositor::ResolvedSource {
                                        id: lib.id,
                                        transform: scene_src.resolve_transform(lib),
                                        opacity: scene_src.resolve_opacity(lib),
                                        visible: scene_src.resolve_visible(lib),
                                    })
                            })
                            .collect()
                    })
                    .unwrap_or_default();

                let mut encoder =
                    gpu.device
                        .create_command_encoder(&egui_wgpu::wgpu::CommandEncoderDescriptor {
                            label: Some("compositor_encoder"),
                        });
                gpu.compositor
                    .compose(&gpu.queue, &mut encoder, &resolved_sources);

                // Scale to output resolution when encoding.
                let is_encoding = app_state.stream_status.is_live()
                    || matches!(
                        app_state.recording_status,
                        crate::state::RecordingStatus::Recording { .. }
                    )
                    || app_state.virtual_camera_active;
                if is_encoding {
                    gpu.compositor.scale_to_output(&mut encoder);
                }

                gpu.queue.submit(std::iter::once(encoder.finish()));

                // Readback for encoding if streaming or recording.
                if is_encoding {
                    drop(app_state); // release lock before blocking readback
                    let frame = gpu.compositor.readback(&gpu.device, &gpu.queue);
                    if let Some(ref channels) = self.gst_channels {
                        let _ = channels.composited_frame_tx.try_send(frame);
                    }
                }
            }
        }

        if let Some(ref mut channels) = self.gst_channels {
            // Poll GStreamer error channel
            while let Ok(err) = channels.error_rx.try_recv() {
                log::error!("GStreamer error: {err}");
                let mut app_state = self.state.lock().expect("lock AppState");
                app_state.active_errors.push(err);
            }

            // Poll audio levels
            if channels.audio_level_rx.has_changed().unwrap_or(false) {
                let levels = channels.audio_level_rx.borrow_and_update().clone();
                let mut app_state = self.state.lock().expect("lock AppState");
                app_state.audio_levels = levels;
            }

            // Poll device list
            if channels.devices_rx.has_changed().unwrap_or(false) {
                let devices = channels.devices_rx.borrow_and_update().clone();
                let mut app_state = self.state.lock().expect("lock AppState");
                app_state.available_audio_devices = devices;
            }

            // Poll encoder list (populated once at startup)
            if channels.encoders_rx.has_changed().unwrap_or(false) {
                let encoders = channels.encoders_rx.borrow().clone();
                let mut app_state = self.state.lock().expect("lock AppState");
                app_state.available_encoders = encoders;
            }
        }

        // Process native menu events
        if let Ok(event) = MenuEvent::receiver().try_recv() {
            self.handle_menu_event(&event.id);
        }

        // Create settings window if requested
        if self.pending_settings_window {
            self.pending_settings_window = false;
            if let Some(gpu) = &self.gpu {
                let app_state = self.state.lock().unwrap();
                let win_w = app_state.settings.settings_window.width;
                let win_h = app_state.settings.settings_window.height;
                drop(app_state);

                let attrs = WindowAttributes::default()
                    .with_title("Settings")
                    .with_inner_size(LogicalSize::new(win_w as f64, win_h as f64))
                    .with_min_inner_size(LogicalSize::new(500.0, 400.0))
                    .with_maximized(false);
                let window = event_loop
                    .create_window(attrs)
                    .expect("create settings window");
                let window: &'static Window = Box::leak(Box::new(window));

                // Settings window doesn't need a layout or preview — use a dummy single-panel layout
                let layout = DockLayout::new_single(PanelType::Preview);
                let win_state = WindowState::new(window, gpu, layout, false, None)
                    .expect("init settings window");
                let window_id = window.id();
                self.windows.insert(window_id, win_state);
                self.settings_window_id = Some(window_id);
                self.refresh_display_exclusion();
            }
        }

        // Create windows for any pending detach requests.
        if let Some(gpu) = &self.gpu {
            let had_detaches = !self.pending_detaches.is_empty();
            for detach in self.pending_detaches.drain(..) {
                let attrs = WindowAttributes::default()
                    .with_title(detach.panel_type.display_name())
                    .with_inner_size(LogicalSize::new(400.0, 300.0))
                    .with_min_inner_size(LogicalSize::new(200.0, 150.0));
                let window = event_loop
                    .create_window(attrs)
                    .expect("create detached window");
                let window: &'static Window = Box::leak(Box::new(window));

                let layout =
                    DockLayout::new_with_ids(detach.group_id, detach.panel_id, detach.panel_type);
                let preview_resources = PreviewResources {
                    pipeline: gpu.compositor.canvas_pipeline(),
                    bind_group: gpu.compositor.canvas_bind_group(),
                };
                let win_state =
                    WindowState::new(window, gpu, layout, false, Some(preview_resources))
                        .expect("init detached window");
                self.windows.insert(window.id(), win_state);
            }
            if had_detaches {
                self.refresh_display_exclusion();
            }
        }

        // Request redraws only when new content arrived — avoids a tight busy
        // loop that pegs the CPU and starves macOS window management (Lasso jank).
        if had_new_frames {
            // New capture frames: redraw to display them.
            for win in self.windows.values() {
                win.window.request_redraw();
            }
        }
        // When no new frames arrive, winit still delivers redraws triggered by:
        // - Input events (mouse, keyboard, resize)
        // - egui's request_repaint() (animations, hover effects, toolbar pulse)
    }
}

/// Compute the diff between two scenes' source lists.
///
/// Returns `(to_add, to_remove)` — source IDs that are in `new_scene` but not `old_scene`,
/// and source IDs that are in `old_scene` but not `new_scene`, respectively.
/// Sources present in both scenes are not included in either list.
#[allow(dead_code)]
fn diff_scene_sources(
    old_scene: Option<&crate::scene::Scene>,
    new_scene: Option<&crate::scene::Scene>,
) -> (Vec<crate::scene::SourceId>, Vec<crate::scene::SourceId>) {
    let old_ids: std::collections::HashSet<_> = old_scene
        .map(|s| s.source_ids().into_iter().collect())
        .unwrap_or_default();
    let new_ids: std::collections::HashSet<_> = new_scene
        .map(|s| s.source_ids().into_iter().collect())
        .unwrap_or_default();

    let to_add: Vec<_> = new_ids.difference(&old_ids).copied().collect();
    let to_remove: Vec<_> = old_ids.difference(&new_ids).copied().collect();
    (to_add, to_remove)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{Scene, SceneId, SceneSource, SourceId};

    #[test]
    fn diff_empty_to_scene() {
        let scene = Scene {
            id: SceneId(1),
            name: "S".into(),
            sources: vec![SceneSource::new(SourceId(1)), SceneSource::new(SourceId(2))],
            pinned: false,
        };
        let (to_add, to_remove) = diff_scene_sources(None, Some(&scene));
        assert_eq!(to_add.len(), 2);
        assert!(to_remove.is_empty());
    }

    #[test]
    fn diff_scene_to_empty() {
        let scene = Scene {
            id: SceneId(1),
            name: "S".into(),
            sources: vec![SceneSource::new(SourceId(1))],
            pinned: false,
        };
        let (to_add, to_remove) = diff_scene_sources(Some(&scene), None);
        assert!(to_add.is_empty());
        assert_eq!(to_remove.len(), 1);
    }

    #[test]
    fn diff_shared_sources_not_touched() {
        let old = Scene {
            id: SceneId(1),
            name: "A".into(),
            sources: vec![SceneSource::new(SourceId(1)), SceneSource::new(SourceId(2))],
            pinned: false,
        };
        let new = Scene {
            id: SceneId(2),
            name: "B".into(),
            sources: vec![SceneSource::new(SourceId(2)), SceneSource::new(SourceId(3))],
            pinned: false,
        };
        let (to_add, to_remove) = diff_scene_sources(Some(&old), Some(&new));
        assert!(to_add.contains(&SourceId(3)));
        assert!(!to_add.contains(&SourceId(2)));
        assert!(to_remove.contains(&SourceId(1)));
        assert!(!to_remove.contains(&SourceId(2)));
    }

    #[test]
    fn diff_identical_scenes() {
        let scene = Scene {
            id: SceneId(1),
            name: "A".into(),
            sources: vec![SceneSource::new(SourceId(1))],
            pinned: false,
        };
        let (to_add, to_remove) = diff_scene_sources(Some(&scene), Some(&scene));
        assert!(to_add.is_empty());
        assert!(to_remove.is_empty());
    }
}

fn main() -> Result<()> {
    env_logger::init();
    log::info!("Lodestone starting");
    text_source::init_font_system();
    let event_loop = EventLoop::new()?;
    let mut app = AppManager::new();
    event_loop.run_app(&mut app)?;
    Ok(())
}
