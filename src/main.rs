mod gstreamer;
mod renderer;
mod scene;
mod settings;
mod state;
mod ui;
mod window;

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
    add_preview: MenuId,
    add_scene_editor: MenuId,
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

        // Edit menu
        let edit_menu = Submenu::new("Edit", true);
        edit_menu.append(&PredefinedMenuItem::undo(None)).ok();
        edit_menu.append(&PredefinedMenuItem::redo(None)).ok();
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
        let add_scene_editor = MenuItem::new("Scene Editor", true, None);
        let add_audio_mixer = MenuItem::new("Audio Mixer", true, None);
        let add_stream_controls = MenuItem::new("Stream Controls", true, None);

        add_panel_menu.append(&add_preview).ok();
        add_panel_menu.append(&add_scene_editor).ok();
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
            add_preview: add_preview.id().clone(),
            add_scene_editor: add_scene_editor.id().clone(),
            add_audio_mixer: add_audio_mixer.id().clone(),
            add_stream_controls: add_stream_controls.id().clone(),
            reset_layout: reset_layout.id().clone(),
        }
    }

    /// Map a menu event ID to a panel type for the "Add Panel" action.
    fn panel_type_for_id(&self, id: &MenuId) -> Option<PanelType> {
        if *id == self.add_preview {
            Some(PanelType::Preview)
        } else if *id == self.add_scene_editor {
            Some(PanelType::SceneEditor)
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

        use crate::scene::SceneCollection;
        let collection = SceneCollection::load_from(&settings::scenes_path());
        let initial_state = AppState {
            scenes: collection.scenes,
            sources: collection.sources,
            active_scene_id: collection.active_scene_id,
            next_scene_id: collection.next_scene_id,
            next_source_id: collection.next_source_id,
            command_tx: Some(main_channels.command_tx.clone()),
            ..AppState::default()
        };

        Self {
            gpu: None,
            windows: HashMap::new(),
            main_window_id: None,
            state: Arc::new(Mutex::new(initial_state)),
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
    fn load_layout() -> DockLayout {
        let path = settings::config_dir().join("layout.toml");
        if path.exists()
            && let Ok(contents) = std::fs::read_to_string(&path)
        {
            match deserialize_full_layout(&contents) {
                Ok((layout, _detached)) => {
                    log::info!("Loaded layout from {}", path.display());
                    return layout;
                }
                Err(e) => {
                    log::warn!("Failed to parse layout.toml, using default: {e}");
                }
            }
        }
        DockLayout::default_layout()
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
        let gpu =
            pollster::block_on(SharedGpuState::new(window)).expect("initialize shared GPU state");

        // Set preview dimensions on AppState
        {
            let mut app_state = self.state.lock().expect("lock AppState");
            app_state.preview_width = gpu.preview_renderer.width;
            app_state.preview_height = gpu.preview_renderer.height;
        }

        let preview_resources = PreviewResources {
            pipeline: gpu.preview_renderer.pipeline(),
            bind_group: gpu.preview_renderer.bind_group(),
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

        // Send initial capture command based on active scene
        {
            let state = self.state.lock().unwrap();
            if let Some(scene_id) = state.active_scene_id {
                if let Some(scene) = state.scenes.iter().find(|s| s.id == scene_id) {
                    if let Some(&src_id) = scene.sources.first() {
                        if let Some(source) = state.sources.iter().find(|s| s.id == src_id) {
                            let crate::scene::SourceProperties::Display { screen_index } =
                                source.properties;
                            if let Some(ref tx) = state.command_tx {
                                let _ = tx.try_send(gstreamer::GstCommand::SetCaptureSource(
                                    gstreamer::CaptureSourceConfig::Screen { screen_index },
                                ));
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
            }
            _ => {}
        }

        match event {
            WindowEvent::CloseRequested => {
                if Some(window_id) == self.main_window_id {
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
                        let layout_changed = match win.render(gpu, &mut app_state) {
                            Ok(detach_requests) => {
                                let changed = !detach_requests.is_empty();
                                self.pending_detaches.extend(detach_requests);
                                changed
                            }
                            Err(e) => {
                                log::error!("Render error: {e}");
                                false
                            }
                        };
                        drop(app_state);
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
                                    sources: app_state.sources.clone(),
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
        // Poll GStreamer frame channel and upload to preview texture
        if let Some(ref mut channels) = self.gst_channels {
            while let Ok(frame) = channels.frame_rx.try_recv() {
                if let Some(ref gpu) = self.gpu {
                    gpu.preview_renderer.upload_frame(&gpu.queue, &frame);
                }
            }

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
        }

        // Upload blank preview when capture is stopped
        {
            let state = self.state.lock().unwrap();
            if !state.capture_active {
                drop(state);
                if let Some(ref gpu) = self.gpu {
                    let w = gpu.preview_renderer.width;
                    let h = gpu.preview_renderer.height;
                    let size = (w * h * 4) as usize;
                    let mut data = vec![0u8; size];
                    for pixel in data.chunks_exact_mut(4) {
                        pixel[0] = 30;
                        pixel[1] = 30;
                        pixel[2] = 30;
                        pixel[3] = 255;
                    }
                    let blank = gstreamer::RgbaFrame {
                        data,
                        width: w,
                        height: h,
                    };
                    gpu.preview_renderer.upload_frame(&gpu.queue, &blank);
                }
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
            }
        }

        // Create windows for any pending detach requests.
        if let Some(gpu) = &self.gpu {
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
                    pipeline: gpu.preview_renderer.pipeline(),
                    bind_group: gpu.preview_renderer.bind_group(),
                };
                let win_state =
                    WindowState::new(window, gpu, layout, false, Some(preview_resources))
                        .expect("init detached window");
                self.windows.insert(window.id(), win_state);
            }
        }

        // Request redraw for all windows so detached windows also animate.
        for win in self.windows.values() {
            win.window.request_redraw();
        }
    }
}

fn main() -> Result<()> {
    env_logger::init();
    log::info!("Lodestone starting");
    let event_loop = EventLoop::new()?;
    let mut app = AppManager::new();
    event_loop.run_app(&mut app)?;
    Ok(())
}
