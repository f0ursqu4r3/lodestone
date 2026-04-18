mod color_source;
mod effect_registry;
mod gstreamer;
mod image_source;
mod renderer;
mod scene;
mod settings;
mod state;
#[cfg(target_os = "macos")]
mod system_extension;
mod text_source;
mod transition;
mod transition_registry;
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
use ui::live_panel::LiveResources;
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

/// Runtime state for an active GIF animation.
struct AnimationState {
    frames: Vec<crate::gstreamer::RgbaFrame>,
    delays: Vec<std::time::Duration>,
    current_frame: usize,
    frame_started_at: std::time::Instant,
    loop_mode: crate::scene::LoopMode,
    loops_completed: u32,
    finished: bool,
}

/// Resolve sources for any scene by ID, applying per-scene overrides.
///
/// Returns an empty vec if the scene is not found.
fn resolve_scene_sources(
    state: &crate::state::AppState,
    scene_id: crate::scene::SceneId,
) -> Vec<crate::renderer::compositor::ResolvedSource> {
    state
        .scenes
        .iter()
        .find(|s| s.id == scene_id)
        .map(|scene| {
            scene
                .sources
                .iter()
                .filter_map(|scene_src| {
                    state
                        .library
                        .iter()
                        .find(|s| s.id == scene_src.source_id)
                        .map(|lib| {
                            // Resolve effect chain and convert to ResolvedEffect.
                            let effect_chain = scene_src.effect_chain(lib);
                            let resolved_effects: Vec<
                                crate::renderer::effect_pipeline::ResolvedEffect,
                            > = effect_chain
                                .iter()
                                .filter(|e| e.enabled)
                                .filter_map(|e| {
                                    let def = state.effect_registry.get(&e.effect_id)?;
                                    let mut params = [0.0f32; 8];
                                    for (i, param_def) in def.params.iter().enumerate().take(8) {
                                        params[i] = e
                                            .params
                                            .get(&param_def.name)
                                            .copied()
                                            .unwrap_or(param_def.default);
                                    }
                                    Some(crate::renderer::effect_pipeline::ResolvedEffect {
                                        effect_id: e.effect_id.clone(),
                                        params,
                                    })
                                })
                                .collect();

                            let maintain_aspect_ratio = matches!(
                                lib.properties,
                                crate::scene::SourceProperties::Window {
                                    maintain_aspect_ratio: true,
                                    ..
                                }
                            );
                            crate::renderer::compositor::ResolvedSource {
                                id: lib.id,
                                transform: scene_src.resolve_transform(lib),
                                opacity: scene_src.resolve_opacity(lib),
                                visible: scene_src.resolve_visible(lib),
                                effects: resolved_effects,
                                maintain_aspect_ratio,
                            }
                        })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn event_needs_undo_snapshot(event: &WindowEvent) -> bool {
    matches!(
        event,
        WindowEvent::KeyboardInput {
            event: KeyEvent {
                state: winit::event::ElementState::Pressed,
                ..
            },
            ..
        } | WindowEvent::MouseInput { .. }
            | WindowEvent::MouseWheel { .. }
            | WindowEvent::Touch { .. }
            | WindowEvent::Ime(_)
            | WindowEvent::DroppedFile(_)
    )
}

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
    open_effects_folder: MenuId,
    open_transitions_folder: MenuId,
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
        let open_effects_folder = MenuItem::new("Open Effects Folder", true, None);
        let open_transitions_folder = MenuItem::new("Open Transitions Folder", true, None);
        file_menu.append(&open_effects_folder).ok();
        file_menu.append(&open_transitions_folder).ok();
        file_menu.append(&PredefinedMenuItem::separator()).ok();
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
            open_effects_folder: open_effects_folder.id().clone(),
            open_transitions_folder: open_transitions_folder.id().clone(),
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
    gif_animations: std::collections::HashMap<crate::scene::SourceId, AnimationState>,
    /// Tracks the last serialized layout to detect changes for debounced saving.
    last_saved_layout: Option<String>,
    /// Throttles readback to the recording/streaming FPS instead of display refresh rate.
    last_readback_at: std::time::Instant,
    /// App start time for effect animations (avoids f32 precision loss from epoch time).
    start_time: std::time::Instant,
    /// Whether global hotkeys have been registered (Windows).
    #[cfg(target_os = "windows")]
    global_hotkeys_registered: bool,
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
        let available_apps = crate::gstreamer::devices::enumerate_applications();
        log::info!("Found {} application(s)", available_apps.len());

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
            #[cfg(target_os = "windows")]
            {
                let displays = crate::gstreamer::devices::enumerate_displays();
                log::info!("Found {} display(s)", displays.len());
                displays
            }
            #[cfg(not(any(target_os = "macos", target_os = "windows")))]
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
        let saved_settings =
            settings::AppSettings::load_or_detect(&settings::settings_path(), detected_resolution);

        // Seed built-in transition shaders on first launch.
        settings::seed_builtin_transitions();

        // Seed built-in effect shaders on first launch.
        settings::seed_builtin_effects();

        // Scan transitions directory and populate the registry.
        let transition_registry =
            crate::transition_registry::TransitionRegistry::scan(&settings::transitions_dir());

        // Scan effects directory and populate the registry.
        let effect_registry =
            crate::effect_registry::EffectRegistry::scan(&settings::effects_dir());

        let initial_state = AppState {
            scenes: collection.scenes,
            library: collection.library,
            active_scene_id: collection.active_scene_id,
            next_scene_id: collection.next_scene_id,
            next_source_id: collection.next_source_id,
            command_tx: Some(main_channels.command_tx.clone()),
            available_cameras,
            available_apps,
            available_displays,
            detected_resolution,
            settings: saved_settings,
            transition_registry,
            effect_registry,
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
            gif_animations: std::collections::HashMap::new(),
            last_saved_layout: None,
            last_readback_at: std::time::Instant::now(),
            start_time: std::time::Instant::now(),
            #[cfg(target_os = "windows")]
            global_hotkeys_registered: false,
        }
    }

    /// Register system-wide global hotkeys on Windows using RegisterHotKey.
    #[cfg(target_os = "windows")]
    fn register_global_hotkeys(&mut self) {
        if self.global_hotkeys_registered {
            return;
        }
        use std::ffi::c_void;
        type HWND = *mut c_void;

        #[allow(non_snake_case)]
        unsafe extern "system" {
            fn RegisterHotKey(hWnd: HWND, id: i32, fsModifiers: u32, vk: u32) -> i32;
        }

        const MOD_CONTROL: u32 = 0x0002;
        const MOD_SHIFT: u32 = 0x0004;
        const MOD_ALT: u32 = 0x0001;
        const MOD_NOREPEAT: u32 = 0x4000;

        let app_state = self.state.lock().unwrap();
        if let Some(binding) = app_state.settings.hotkeys.get("capture_foreground_window") {
            let mut mods = MOD_NOREPEAT;
            if binding.ctrl {
                mods |= MOD_CONTROL;
            }
            if binding.shift {
                mods |= MOD_SHIFT;
            }
            if binding.alt {
                mods |= MOD_ALT;
            }
            // Map key name to Windows virtual key code.
            if let Some(vk) = key_name_to_vk(&binding.key) {
                let ok = unsafe { RegisterHotKey(std::ptr::null_mut(), 1, mods, vk) };
                if ok != 0 {
                    log::info!("Global hotkey registered: {} (id=1)", binding.display());
                } else {
                    log::warn!(
                        "Failed to register global hotkey: {} (may be in use by another app)",
                        binding.display()
                    );
                }
            }
        }
        self.global_hotkeys_registered = true;
    }

    /// Poll for global hotkey events on Windows (WM_HOTKEY messages).
    #[cfg(target_os = "windows")]
    fn poll_global_hotkeys(&self) {
        use std::ffi::c_void;
        type HWND = *mut c_void;

        const WM_HOTKEY: u32 = 0x0312;

        #[repr(C)]
        struct MSG {
            hwnd: HWND,
            message: u32,
            w_param: usize,
            l_param: isize,
            time: u32,
            pt_x: i32,
            pt_y: i32,
        }

        #[allow(non_snake_case)]
        unsafe extern "system" {
            fn PeekMessageW(
                lpMsg: *mut MSG,
                hWnd: HWND,
                wMsgFilterMin: u32,
                wMsgFilterMax: u32,
                wRemoveMsg: u32,
            ) -> i32;
        }

        const PM_REMOVE: u32 = 0x0001;

        let mut msg: MSG = unsafe { std::mem::zeroed() };
        while unsafe {
            PeekMessageW(
                &mut msg,
                std::ptr::null_mut(),
                WM_HOTKEY,
                WM_HOTKEY,
                PM_REMOVE,
            )
        } != 0
        {
            let hotkey_id = msg.w_param;
            if hotkey_id == 1 {
                // Capture foreground window
                let app_state = self.state.lock().unwrap();
                if let Some(ref tx) = app_state.command_tx {
                    log::info!("Global hotkey triggered: capture foreground window");
                    let _ = tx.try_send(gstreamer::GstCommand::CaptureForegroundWindow);
                }
            }
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
    fn save_layout(&mut self) {
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
                if let Err(e) = std::fs::write(&path, &toml_str) {
                    log::warn!("Failed to save layout: {e}");
                }
                self.last_saved_layout = Some(toml_str);
            }
            Err(e) => {
                log::warn!("Failed to serialize layout: {e}");
            }
        }
    }

    /// Save layout only if it has changed since the last save.
    fn save_layout_if_changed(&mut self) {
        let Some(main_id) = self.main_window_id else {
            return;
        };
        let Some(main_win) = self.windows.get(&main_id) else {
            return;
        };
        let detached = Vec::new();
        let Ok(toml_str) = serialize_full_layout(&main_win.layout, &detached) else {
            return;
        };
        let changed = self
            .last_saved_layout
            .as_ref()
            .map_or(true, |prev| *prev != toml_str);
        if changed {
            let path = settings::config_dir().join("layout.toml");
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Err(e) = std::fs::write(&path, &toml_str) {
                log::warn!("Failed to save layout: {e}");
            }
            self.last_saved_layout = Some(toml_str);
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
                self.reconcile_captures(&mut app_state);
            }
            return;
        }

        if *id == native_menu.reset_layout {
            self.reset_layout();
            return;
        }

        if *id == native_menu.open_effects_folder {
            let dir = settings::effects_dir();
            let _ = std::fs::create_dir_all(&dir);
            let _ = std::process::Command::new("open").arg(&dir).spawn();
            return;
        }

        if *id == native_menu.open_transitions_folder {
            let dir = settings::transitions_dir();
            let _ = std::fs::create_dir_all(&dir);
            let _ = std::process::Command::new("open").arg(&dir).spawn();
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

    /// Refresh display capture exclusion after a window is created or destroyed.
    ///
    /// On macOS, updates ScreenCaptureKit content filters via the GStreamer thread.
    /// On Windows, sets `WDA_EXCLUDEFROMCAPTURE` display affinity on all app windows.
    fn refresh_display_exclusion(&self) {
        let state = self.state.lock().unwrap();
        let exclude = state.settings.general.exclude_self_from_capture;

        #[cfg(target_os = "macos")]
        if exclude {
            if let Some(tx) = &state.command_tx {
                let _ = tx
                    .try_send(gstreamer::GstCommand::UpdateDisplayExclusion { exclude_self: true });
            }
        }

        // On Windows, tell the OS to exclude our windows from screen capture.
        #[cfg(target_os = "windows")]
        {
            drop(state); // release lock before iterating windows
            self.set_window_display_affinity(exclude);
        }
    }

    /// Set or clear `WDA_EXCLUDEFROMCAPTURE` on all Lodestone windows (Windows only).
    ///
    /// When enabled, the OS excludes these windows from DXGI Desktop Duplication and
    /// other screen capture APIs — they appear as black regions in the capture output.
    #[cfg(target_os = "windows")]
    fn set_window_display_affinity(&self, exclude: bool) {
        use raw_window_handle::HasWindowHandle;

        // WDA_NONE = 0, WDA_EXCLUDEFROMCAPTURE = 0x11
        const WDA_NONE: u32 = 0x00;
        const WDA_EXCLUDEFROMCAPTURE: u32 = 0x11;

        unsafe extern "system" {
            fn SetWindowDisplayAffinity(hwnd: isize, dw_affinity: u32) -> i32;
        }

        let affinity = if exclude {
            WDA_EXCLUDEFROMCAPTURE
        } else {
            WDA_NONE
        };

        for win_state in self.windows.values() {
            if let Ok(handle) = win_state.window.window_handle() {
                if let raw_window_handle::RawWindowHandle::Win32(h) = handle.as_raw() {
                    let hwnd = h.hwnd.get() as isize;
                    let result = unsafe { SetWindowDisplayAffinity(hwnd, affinity) };
                    if result == 0 {
                        log::warn!("SetWindowDisplayAffinity failed for hwnd {hwnd:#x}");
                    } else {
                        log::debug!("SetWindowDisplayAffinity({hwnd:#x}, {affinity:#x}) succeeded");
                    }
                }
            }
        }
    }

    /// Reconcile GStreamer captures after undo/redo by stopping all captures
    /// and restarting those in the active scene.
    fn reconcile_captures(&self, app_state: &mut AppState) {
        let cmd_tx = &app_state.command_tx;
        if let Some(tx) = cmd_tx {
            let _ = tx.try_send(crate::gstreamer::GstCommand::StopCapture);
        }
        if let Some(scene) = app_state.active_scene() {
            let scene = scene.clone();
            let capture_size = crate::renderer::compositor::parse_resolution(
                &app_state.settings.video.base_resolution,
            );
            let anims = crate::ui::scenes_panel::send_capture_for_scene(
                cmd_tx,
                &app_state.library,
                &app_state.available_cameras,
                &scene,
                app_state.settings.general.exclude_self_from_capture,
                capture_size,
                app_state.settings.video.fps,
            );
            app_state.pending_gif_animations.extend(anims);
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
        let live_resources = LiveResources {
            pipeline: gpu.compositor.canvas_pipeline(),
            bind_group: gpu.compositor.canvas_bind_group(),
        };

        // Try to load saved layout; fall back to default.
        let layout = Self::load_layout();
        let win_state = WindowState::new(
            window,
            &gpu,
            layout,
            true,
            Some(preview_resources),
            Some(live_resources),
        )
        .expect("create main window state");

        self.gpu = Some(gpu);
        self.main_window_id = Some(window_id);
        self.windows.insert(window_id, win_state);

        // Attach native menu bar — must happen after window is fully initialized.
        // On Windows, we render the menu bar in egui to match the app theme,
        // so we skip attaching the native menu to the HWND.
        let native_menu = NativeMenu::build();
        #[cfg(target_os = "macos")]
        {
            native_menu.menu.init_for_nsapp();
        }
        self.native_menu = Some(native_menu);

        // Store monitor count
        {
            let monitor_count = event_loop.available_monitors().count().max(1);
            let mut state = self.state.lock().unwrap();
            state.monitor_count = monitor_count;
        }

        // Send initial capture commands for all sources in the active scene
        let mut startup_gif_animations = Vec::new();
        {
            let state = self.state.lock().unwrap();
            if let Some(scene_id) = state.active_scene_id
                && let Some(scene) = state.scenes.iter().find(|s| s.id == scene_id)
            {
                let capture_size = crate::renderer::compositor::parse_resolution(
                    &state.settings.video.base_resolution,
                );
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
                                            capture_size,
                                        },
                                        fps: state.settings.video.fps,
                                    });
                                }
                            }
                            crate::scene::SourceProperties::Window { mode, .. } => {
                                if let Some(ref tx) = state.command_tx {
                                    let _ = tx.try_send(gstreamer::GstCommand::AddCaptureSource {
                                        source_id: src_id,
                                        config: gstreamer::CaptureSourceConfig::Window {
                                            mode: mode.clone(),
                                            capture_size,
                                        },
                                        fps: state.settings.video.fps,
                                    });
                                }
                            }
                            crate::scene::SourceProperties::Camera {
                                device_index,
                                device_name,
                                device_uid,
                            } => {
                                if let Some(ref tx) = state.command_tx {
                                    let idx = gstreamer::resolve_camera_index(
                                        &state.available_cameras,
                                        device_uid,
                                        device_name,
                                        *device_index,
                                    );
                                    let _ = tx.try_send(gstreamer::GstCommand::AddCaptureSource {
                                        source_id: src_id,
                                        config: gstreamer::CaptureSourceConfig::Camera {
                                            device_index: idx,
                                        },
                                        fps: state.settings.video.fps,
                                    });
                                }
                            }
                            crate::scene::SourceProperties::Image { path, loop_mode } => {
                                if let Some(ref tx) = state.command_tx {
                                    if !path.is_empty() {
                                        let mut pending_anims = Vec::new();
                                        crate::ui::scenes_panel::load_image_for_source(
                                            tx,
                                            src_id,
                                            path,
                                            *loop_mode,
                                            &mut pending_anims,
                                        );
                                        // Can't push to state here (immutable borrow),
                                        // store for later.
                                        startup_gif_animations.extend(pending_anims);
                                    }
                                }
                            }
                            crate::scene::SourceProperties::GameCapture {
                                process_name, ..
                            } => {
                                if let Some(ref tx) = state.command_tx {
                                    if !process_name.is_empty() {
                                        let windows = gstreamer::devices::enumerate_windows();
                                        if let Some(win) = windows.iter().find(|w| {
                                            w.process_name.eq_ignore_ascii_case(process_name)
                                        }) {
                                            let _ = tx.try_send(
                                                gstreamer::GstCommand::AddCaptureSource {
                                                    source_id: src_id,
                                                    config:
                                                        gstreamer::CaptureSourceConfig::GameCapture {
                                                            process_id: win.process_id,
                                                            hwnd: win.native_handle,
                                                            process_name: process_name.clone(),
                                                        },
                                                    fps: state.settings.video.fps,
                                                },
                                            );
                                        }
                                    }
                                }
                            }
                            _ => {
                                // Text, Color, Audio, Browser sources don't use a capture pipeline yet.
                            }
                        }
                    }
                }
            }
        }
        // Push any GIF animations collected during startup image loading.
        if !startup_gif_animations.is_empty() {
            let mut state = self.state.lock().unwrap();
            state.pending_gif_animations.extend(startup_gif_animations);
        }

        // Apply initial display exclusion if the setting was persisted.
        #[cfg(target_os = "windows")]
        {
            let state = self.state.lock().unwrap();
            if state.settings.general.exclude_self_from_capture {
                drop(state);
                self.set_window_display_affinity(true);
            }
        }

        // Register global hotkeys for actions that need to work when the app is not focused.
        #[cfg(target_os = "windows")]
        self.register_global_hotkeys();

        // Request an immediate redraw so the toolbar and UI paint on first frame.
        if let Some(win) = self.main_window_id.and_then(|id| self.windows.get(&id)) {
            win.window.request_redraw();
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
            if event_needs_undo_snapshot(&event) {
                win.note_input_for_undo_snapshot();
            }
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
                        self.reconcile_captures(&mut app_state);
                    }
                    return;
                }

                // ── Configurable hotkeys (from settings) ───────────────
                {
                    let mut app_state = self.state.lock().unwrap();
                    let hotkeys = &app_state.settings.hotkeys;
                    let mods = &self.modifiers;

                    // Capture Foreground Window
                    if let Some(binding) = hotkeys.get("capture_foreground_window")
                        && binding.matches(key_code, mods)
                    {
                        if let Some(ref tx) = app_state.command_tx {
                            let _ = tx.try_send(gstreamer::GstCommand::CaptureForegroundWindow);
                        }
                        return;
                    }

                    // Start Streaming
                    if let Some(binding) = hotkeys.get("start_streaming")
                        && binding.matches(key_code, mods)
                        && !app_state.stream_status.is_live()
                    {
                        if let Some(ref tx) = app_state.command_tx {
                            if crate::ui::toolbar::validate_stream_settings(&app_state).is_none() {
                                let _ = tx.try_send(gstreamer::GstCommand::StartStream {
                                    destination: app_state.settings.stream.destination.clone(),
                                    stream_key: app_state.settings.stream.stream_key.clone(),
                                    encoder_config: crate::ui::toolbar::stream_encoder_config(
                                        &app_state,
                                    ),
                                });
                            }
                        }
                        return;
                    }

                    // Stop Streaming
                    if let Some(binding) = hotkeys.get("stop_streaming")
                        && binding.matches(key_code, mods)
                        && app_state.stream_status.is_live()
                    {
                        if let Some(ref tx) = app_state.command_tx {
                            let _ = tx.try_send(gstreamer::GstCommand::StopStream);
                        }
                        return;
                    }

                    // Start Recording
                    if let Some(binding) = hotkeys.get("start_recording")
                        && binding.matches(key_code, mods)
                        && matches!(
                            app_state.recording_status,
                            crate::state::RecordingStatus::Idle
                        )
                    {
                        if let Some(ref tx) = app_state.command_tx {
                            let counter = app_state.recording_counter + 1;
                            let scene_name = "Main";
                            let filename = crate::settings::RecordSettings::expand_template(
                                &app_state.settings.record.filename_template,
                                scene_name,
                                counter,
                            );
                            let ext = match app_state.settings.record.format {
                                gstreamer::RecordingFormat::Mkv => "mkv",
                                gstreamer::RecordingFormat::Mp4 => "mp4",
                            };
                            let folder = if app_state.settings.record.output_folder.exists() {
                                app_state.settings.record.output_folder.clone()
                            } else {
                                dirs::video_dir()
                                    .or_else(dirs::home_dir)
                                    .unwrap_or_else(|| std::path::PathBuf::from("."))
                            };
                            let path = folder.join(format!("{filename}.{ext}"));
                            let _ = tx.try_send(gstreamer::GstCommand::StartRecording {
                                path: path.clone(),
                                format: app_state.settings.record.format,
                                encoder_config: crate::ui::toolbar::record_encoder_config(
                                    &app_state,
                                ),
                            });
                            app_state.recording_counter = counter;
                        }
                        return;
                    }

                    // Stop Recording
                    if let Some(binding) = hotkeys.get("stop_recording")
                        && binding.matches(key_code, mods)
                        && matches!(
                            app_state.recording_status,
                            crate::state::RecordingStatus::Recording { .. }
                        )
                    {
                        if let Some(ref tx) = app_state.command_tx {
                            let _ = tx.try_send(gstreamer::GstCommand::StopRecording);
                        }
                        return;
                    }

                    // Toggle Mute Mic
                    if let Some(binding) = hotkeys.get("toggle_mute_mic")
                        && binding.matches(key_code, mods)
                    {
                        if let Some(ref tx) = app_state.command_tx {
                            // Toggle: we don't have "is_muted" state for global mic,
                            // so this is a best-effort toggle.
                            let _ = tx.try_send(gstreamer::GstCommand::SetAudioMuted {
                                source: gstreamer::AudioSourceKind::Mic,
                                muted: true, // TODO: proper toggle with tracked state
                            });
                        }
                        return;
                    }

                    // Toggle Mute Desktop
                    if let Some(binding) = hotkeys.get("toggle_mute_desktop")
                        && binding.matches(key_code, mods)
                    {
                        if let Some(ref tx) = app_state.command_tx {
                            let _ = tx.try_send(gstreamer::GstCommand::SetAudioMuted {
                                source: gstreamer::AudioSourceKind::System,
                                muted: true, // TODO: proper toggle with tracked state
                            });
                        }
                        return;
                    }
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

                // ── Scene transition hotkeys ────────────────────────────────
                // All of these are skipped when egui has keyboard focus
                // (e.g. a text field is active).

                // Space: Quick cut — instantly make active_scene_id the new program scene.
                // Cancels any in-flight transition.
                if *key_code == KeyCode::Space {
                    let egui_wants_input = self
                        .windows
                        .get(&window_id)
                        .map(|w| w.egui_ctx.wants_keyboard_input())
                        .unwrap_or(false);
                    if !egui_wants_input {
                        let mut app_state = self.state.lock().unwrap();
                        if let Some(new_program_id) = app_state.active_scene_id
                            && app_state.program_scene_id != Some(new_program_id)
                        {
                            let old_program_id = app_state.program_scene_id;
                            let exclude_self = app_state.settings.general.exclude_self_from_capture;
                            let cmd_tx = app_state.command_tx.clone();

                            // Cancel any in-flight transition.
                            app_state.active_transition = None;

                            let old_scene = old_program_id
                                .and_then(|id| app_state.scenes.iter().find(|s| s.id == id))
                                .cloned();
                            let new_scene = app_state
                                .scenes
                                .iter()
                                .find(|s| s.id == new_program_id)
                                .cloned();

                            app_state.program_scene_id = Some(new_program_id);

                            // Stop sources that were exclusive to the old program scene
                            // (not needed by the current active/editing scene).
                            let capture_size = crate::renderer::compositor::parse_resolution(
                                &app_state.settings.video.base_resolution,
                            );
                            let no_keep = std::collections::HashSet::new();
                            let anims = crate::ui::scenes_panel::apply_scene_diff(
                                &cmd_tx,
                                &app_state.library,
                                &app_state.available_cameras,
                                old_scene.as_ref(),
                                new_scene.as_ref(),
                                exclude_self,
                                capture_size,
                                app_state.settings.video.fps,
                                &no_keep,
                            );
                            app_state.pending_gif_animations.extend(anims);
                            if let Some(ref scene) = new_scene {
                                app_state.capture_active = !scene.sources.is_empty();
                            }
                            app_state.mark_dirty();
                        }
                        return;
                    }
                }

                // Enter: Trigger transition — active_scene_id → program using configured
                // transition type/duration.
                if *key_code == KeyCode::Enter || *key_code == KeyCode::NumpadEnter {
                    let egui_wants_input = self
                        .windows
                        .get(&window_id)
                        .map(|w| w.egui_ctx.wants_keyboard_input())
                        .unwrap_or(false);
                    if !egui_wants_input {
                        let mut app_state = self.state.lock().unwrap();
                        let can_transition = app_state.program_scene_id
                            != app_state.active_scene_id
                            && app_state.active_scene_id.is_some()
                            && app_state.active_transition.is_none();
                        if can_transition {
                            let new_program_id = app_state.active_scene_id.unwrap();
                            let from_id = app_state.program_scene_id;
                            let exclude_self = app_state.settings.general.exclude_self_from_capture;
                            let cmd_tx = app_state.command_tx.clone();
                            let capture_size = crate::renderer::compositor::parse_resolution(
                                &app_state.settings.video.base_resolution,
                            );

                            let target_scene =
                                app_state.scenes.iter().find(|s| s.id == new_program_id);
                            let resolved = target_scene
                                .map(|s| {
                                    crate::transition::resolve_transition(
                                        &app_state.settings.transitions,
                                        &s.transition_override,
                                    )
                                })
                                .unwrap_or_else(|| crate::transition::ResolvedTransition {
                                    transition: crate::transition::TRANSITION_FADE.to_string(),
                                    duration: std::time::Duration::from_millis(300),
                                    colors: crate::transition::TransitionColors::default(),
                                    params: std::collections::HashMap::new(),
                                });

                            if resolved.transition == crate::transition::TRANSITION_CUT {
                                let old_scene = from_id
                                    .and_then(|id| app_state.scenes.iter().find(|s| s.id == id))
                                    .cloned();
                                let new_scene = app_state
                                    .scenes
                                    .iter()
                                    .find(|s| s.id == new_program_id)
                                    .cloned();

                                app_state.program_scene_id = Some(new_program_id);

                                let no_keep = std::collections::HashSet::new();
                                let anims = crate::ui::scenes_panel::apply_scene_diff(
                                    &cmd_tx,
                                    &app_state.library,
                                    &app_state.available_cameras,
                                    old_scene.as_ref(),
                                    new_scene.as_ref(),
                                    exclude_self,
                                    capture_size,
                                    app_state.settings.video.fps,
                                    &no_keep,
                                );
                                app_state.pending_gif_animations.extend(anims);
                                if let Some(ref scene) = new_scene {
                                    app_state.capture_active = !scene.sources.is_empty();
                                }
                                app_state.mark_dirty();
                            } else {
                                let from_scene_id = from_id;
                                let old_scene = from_scene_id
                                    .and_then(|id| app_state.scenes.iter().find(|s| s.id == id))
                                    .cloned();
                                let new_scene = app_state
                                    .scenes
                                    .iter()
                                    .find(|s| s.id == new_program_id)
                                    .cloned();

                                if let Some(ref new_s) = new_scene {
                                    for &src_id in &new_s.source_ids() {
                                        let already_running = old_scene
                                            .as_ref()
                                            .map(|s| s.source_ids().contains(&src_id))
                                            .unwrap_or(false);
                                        if !already_running {
                                            let anims =
                                                crate::ui::scenes_panel::start_capture_source(
                                                    &cmd_tx,
                                                    &app_state.library,
                                                    &app_state.available_cameras,
                                                    src_id,
                                                    exclude_self,
                                                    capture_size,
                                                    app_state.settings.video.fps,
                                                );
                                            app_state.pending_gif_animations.extend(anims);
                                        }
                                    }
                                }

                                let transition_from = from_scene_id.unwrap_or(new_program_id);
                                app_state.active_transition =
                                    Some(crate::transition::TransitionState {
                                        from_scene: transition_from,
                                        to_scene: new_program_id,
                                        transition: resolved.transition,
                                        started_at: std::time::Instant::now(),
                                        duration: resolved.duration,
                                        colors: resolved.colors,
                                        params: resolved.params,
                                    });
                                app_state.program_scene_id = Some(new_program_id);
                                app_state.mark_dirty();
                            }
                            return;
                        }
                    }
                }

                // 1-9: Select scene by index for editing (sets active_scene_id).
                // Does not trigger a transition — use Enter/Space for that.
                let digit_opt: Option<usize> = match key_code {
                    KeyCode::Digit1 if !self.modifiers.super_key() && !ctrl => Some(1),
                    KeyCode::Digit2 if !self.modifiers.super_key() && !ctrl => Some(2),
                    KeyCode::Digit3 if !self.modifiers.super_key() && !ctrl => Some(3),
                    KeyCode::Digit4 if !self.modifiers.super_key() && !ctrl => Some(4),
                    KeyCode::Digit5 if !self.modifiers.super_key() && !ctrl => Some(5),
                    KeyCode::Digit6 if !self.modifiers.super_key() && !ctrl => Some(6),
                    KeyCode::Digit7 if !self.modifiers.super_key() && !ctrl => Some(7),
                    KeyCode::Digit8 if !self.modifiers.super_key() && !ctrl => Some(8),
                    KeyCode::Digit9 if !self.modifiers.super_key() && !ctrl => Some(9),
                    _ => None,
                };
                if let Some(digit) = digit_opt {
                    let egui_wants_input = self
                        .windows
                        .get(&window_id)
                        .map(|w| w.egui_ctx.wants_keyboard_input())
                        .unwrap_or(false);
                    if !egui_wants_input {
                        let mut app_state = self.state.lock().unwrap();
                        let scene_index = digit - 1; // 0-based
                        let target_id = app_state.scenes.get(scene_index).map(|s| s.id);

                        if let Some(new_id) = target_id
                            && app_state.active_scene_id != Some(new_id)
                        {
                            let old_active = app_state.active_scene_id;
                            let program_id = app_state.program_scene_id;
                            let exclude_self = app_state.settings.general.exclude_self_from_capture;
                            let cmd_tx = app_state.command_tx.clone();
                            let capture_size = crate::renderer::compositor::parse_resolution(
                                &app_state.settings.video.base_resolution,
                            );

                            let old_scene = old_active
                                .and_then(|id| app_state.scenes.iter().find(|s| s.id == id))
                                .cloned();
                            let new_scene =
                                app_state.scenes.iter().find(|s| s.id == new_id).cloned();
                            let program_scene = program_id
                                .and_then(|id| app_state.scenes.iter().find(|s| s.id == id))
                                .cloned();

                            app_state.active_scene_id = Some(new_id);
                            app_state.deselect_all();

                            // Start sources for the new editing scene not already running.
                            if let Some(ref new_s) = new_scene {
                                for &src_id in &new_s.source_ids() {
                                    let already_running = old_scene
                                        .as_ref()
                                        .map(|s| s.source_ids().contains(&src_id))
                                        .unwrap_or(false)
                                        || program_scene
                                            .as_ref()
                                            .map(|s| s.source_ids().contains(&src_id))
                                            .unwrap_or(false);
                                    if !already_running {
                                        let anims = crate::ui::scenes_panel::start_capture_source(
                                            &cmd_tx,
                                            &app_state.library,
                                            &app_state.available_cameras,
                                            src_id,
                                            exclude_self,
                                            capture_size,
                                            app_state.settings.video.fps,
                                        );
                                        app_state.pending_gif_animations.extend(anims);
                                    }
                                }
                            }
                            // Stop sources that were only in old_scene (not in new or program).
                            if let Some(ref old_s) = old_scene {
                                let new_src_ids: std::collections::HashSet<_> = new_scene
                                    .as_ref()
                                    .map(|s| s.source_ids().into_iter().collect())
                                    .unwrap_or_default();
                                let prog_src_ids: std::collections::HashSet<_> = program_scene
                                    .as_ref()
                                    .map(|s| s.source_ids().into_iter().collect())
                                    .unwrap_or_default();
                                if let Some(ref tx) = cmd_tx {
                                    for &src_id in &old_s.source_ids() {
                                        if !new_src_ids.contains(&src_id)
                                            && !prog_src_ids.contains(&src_id)
                                        {
                                            let _ = tx.try_send(
                                                crate::gstreamer::GstCommand::RemoveCaptureSource {
                                                    source_id: src_id,
                                                },
                                            );
                                        }
                                    }
                                }
                            }

                            if let Some(ref scene) = new_scene {
                                app_state.capture_active = !scene.sources.is_empty();
                            }
                            app_state.mark_dirty();
                        }
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

                        // Handle egui menu bar actions (Windows — no native menu).
                        {
                            let mut app_state = self.state.lock().unwrap();
                            let do_undo = app_state.menu_undo;
                            let do_redo = app_state.menu_redo;
                            let open_effects = app_state.menu_open_effects_folder;
                            let open_transitions = app_state.menu_open_transitions_folder;
                            app_state.menu_undo = false;
                            app_state.menu_redo = false;
                            app_state.menu_open_effects_folder = false;
                            app_state.menu_open_transitions_folder = false;

                            if do_undo || do_redo {
                                let restored = if do_redo {
                                    app_state.redo()
                                } else {
                                    app_state.undo()
                                };
                                if restored {
                                    self.reconcile_captures(&mut app_state);
                                }
                            }
                            drop(app_state);

                            if open_effects {
                                let dir = settings::effects_dir();
                                let _ = std::fs::create_dir_all(&dir);
                                #[cfg(target_os = "windows")]
                                {
                                    let _ =
                                        std::process::Command::new("explorer").arg(&dir).spawn();
                                }
                                #[cfg(not(target_os = "windows"))]
                                {
                                    let _ = std::process::Command::new("open").arg(&dir).spawn();
                                }
                            }
                            if open_transitions {
                                let dir = settings::transitions_dir();
                                let _ = std::fs::create_dir_all(&dir);
                                #[cfg(target_os = "windows")]
                                {
                                    let _ =
                                        std::process::Command::new("explorer").arg(&dir).spawn();
                                }
                                #[cfg(not(target_os = "windows"))]
                                {
                                    let _ = std::process::Command::new("open").arg(&dir).spawn();
                                }
                            }
                        }

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

                            // Debounced layout persistence — compare with last saved.
                            self.save_layout_if_changed();
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
                    && Some(window_id) != self.settings_window_id
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
        // Poll global hotkeys (system-wide, works even when app is not focused).
        #[cfg(target_os = "windows")]
        self.poll_global_hotkeys();

        // Collect any completed async readback from the previous frame and
        // forward it to the GStreamer encode thread (non-blocking).
        if let Some(ref mut gpu) = self.gpu
            && let Some(frame) = gpu.compositor.try_finish_readback(&gpu.device)
            && let Some(ref channels) = self.gst_channels
        {
            let _ = channels.composited_frame_tx.try_send(frame);
        }

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
            // Update native_size on sources when frame dimensions change.
            {
                let mut app_state = self.state.lock().expect("lock AppState");
                for (source_id, frame) in &drained_frames {
                    let new_size = (frame.width as f32, frame.height as f32);
                    if let Some(s) = app_state.library.iter_mut().find(|s| s.id == *source_id)
                        && s.native_size != new_size
                    {
                        let old_native = s.native_size;
                        s.native_size = new_size;
                        // Update transform dimensions if they still match the old
                        // native_size (user hasn't manually resized).
                        let matches_old = (s.transform.width - old_native.0).abs() < 1.0
                            && (s.transform.height - old_native.1).abs() < 1.0;
                        let was_default = old_native == (1920.0, 1080.0);
                        if matches_old || was_default {
                            s.transform.width = new_size.0;
                            s.transform.height = new_size.1;
                        }
                    }
                }
            }
            for (source_id, frame) in drained_frames {
                gpu.compositor
                    .upload_frame(&gpu.device, &gpu.queue, source_id, &frame);
                if let Some(ref mut secondary) = gpu.secondary_canvas {
                    secondary.upload_frame(
                        &gpu.device,
                        &gpu.queue,
                        source_id,
                        &frame,
                        gpu.compositor.texture_bind_group_layout(),
                        gpu.compositor.uniform_bind_group_layout(),
                        gpu.compositor.compositor_sampler(),
                    );
                }
            }
        }

        // Consume pending GIF animations and loop mode updates from UI.
        {
            let mut app_state = self.state.lock().expect("lock AppState");
            for (source_id, animation, loop_mode) in app_state.pending_gif_animations.drain(..) {
                self.gif_animations.insert(
                    source_id,
                    AnimationState {
                        frames: animation.frames,
                        delays: animation.delays,
                        current_frame: 0,
                        frame_started_at: std::time::Instant::now(),
                        loop_mode,
                        loops_completed: 0,
                        finished: false,
                    },
                );
            }
            for (source_id, new_mode) in app_state.pending_loop_mode_updates.drain(..) {
                if let Some(anim) = self.gif_animations.get_mut(&source_id) {
                    anim.loop_mode = new_mode;
                    anim.finished = false;
                    anim.loops_completed = 0;
                }
            }
        }

        // Remove animations for deleted sources.
        {
            let app_state = self.state.lock().expect("lock AppState");
            self.gif_animations
                .retain(|source_id, _| app_state.library.iter().any(|s| s.id == *source_id));
        }

        // Advance GIF animations.
        let mut gif_uploads: Vec<(crate::scene::SourceId, usize)> = Vec::new();
        let mut any_gif_active = false;

        for (source_id, anim) in self.gif_animations.iter_mut() {
            if anim.finished {
                continue;
            }
            any_gif_active = true;
            if anim.frame_started_at.elapsed() >= anim.delays[anim.current_frame] {
                anim.current_frame += 1;
                if anim.current_frame >= anim.frames.len() {
                    anim.loops_completed += 1;
                    match anim.loop_mode {
                        crate::scene::LoopMode::Infinite => anim.current_frame = 0,
                        crate::scene::LoopMode::Once => {
                            anim.current_frame = anim.frames.len() - 1;
                            anim.finished = true;
                            continue;
                        }
                        crate::scene::LoopMode::Count(n) => {
                            if anim.loops_completed >= n {
                                anim.current_frame = anim.frames.len() - 1;
                                anim.finished = true;
                                continue;
                            }
                            anim.current_frame = 0;
                        }
                    }
                }
                anim.frame_started_at = std::time::Instant::now();
                gif_uploads.push((*source_id, anim.current_frame));
            }
        }

        // Upload GIF frames to compositor.
        if !gif_uploads.is_empty()
            && let Some(ref mut gpu) = self.gpu
        {
            for (source_id, frame_idx) in &gif_uploads {
                if let Some(anim) = self.gif_animations.get(source_id) {
                    let frame = &anim.frames[*frame_idx];
                    gpu.compositor
                        .upload_frame(&gpu.device, &gpu.queue, *source_id, frame);
                    if let Some(ref mut secondary) = gpu.secondary_canvas {
                        secondary.upload_frame(
                            &gpu.device,
                            &gpu.queue,
                            *source_id,
                            frame,
                            gpu.compositor.texture_bind_group_layout(),
                            gpu.compositor.uniform_bind_group_layout(),
                            gpu.compositor.compositor_sampler(),
                        );
                    }
                }
            }
        }

        if any_gif_active
            && let Some(main_id) = self.main_window_id
            && let Some(win) = self.windows.get(&main_id)
        {
            win.window.request_redraw();
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
                // Update preview and live resources — the canvas bind group changed.
                if let Some(main_id) = self.main_window_id
                    && let Some(win) = self.windows.get_mut(&main_id)
                {
                    let new_resources = PreviewResources {
                        pipeline: gpu.compositor.canvas_pipeline(),
                        bind_group: gpu.compositor.canvas_bind_group(),
                    };
                    win.egui_renderer.callback_resources.insert(new_resources);
                    let new_live = LiveResources {
                        pipeline: gpu.compositor.canvas_pipeline(),
                        bind_group: gpu.compositor.canvas_bind_group(),
                    };
                    win.egui_renderer.callback_resources.insert(new_live);
                }
            }
        }

        // Compose scene sources onto canvases, with optional transition blending.
        //
        // Primary canvas  → active_scene_id  (Preview/editor panel)
        // Secondary canvas → program_scene_id (Live panel + encoding) when it differs
        if let Some(ref mut gpu) = self.gpu {
            // Read scene IDs, transition state, and encoding status from AppState.
            let (
                active_id,
                program_id,
                transition_info,
                is_encoding,
                encode_fps,
                transition_registry,
                registry_changed,
                effect_registry,
                effect_registry_changed,
            ) = {
                let mut app_state = self.state.lock().expect("lock AppState");

                // Initialize program_scene_id from active_scene_id on startup.
                if app_state.program_scene_id.is_none() && app_state.active_scene_id.is_some() {
                    app_state.program_scene_id = app_state.active_scene_id;
                }

                // Periodically rescan the transitions directory for new/changed shaders.
                if app_state.last_transition_scan.elapsed() >= std::time::Duration::from_secs(2) {
                    app_state.last_transition_scan = std::time::Instant::now();
                    if app_state
                        .transition_registry
                        .rescan(&crate::settings::transitions_dir())
                    {
                        app_state.transition_registry_changed = true;
                        log::info!(
                            "Transition registry updated — {} transitions available",
                            app_state.transition_registry.all().len()
                        );
                    }
                }

                // Periodically rescan the effects directory for new/changed shaders.
                if app_state.last_effect_scan.elapsed() >= std::time::Duration::from_secs(2) {
                    app_state.last_effect_scan = std::time::Instant::now();
                    if app_state
                        .effect_registry
                        .rescan(&crate::settings::effects_dir())
                    {
                        app_state.effect_registry_changed = true;
                        log::info!(
                            "Effect registry updated — {} effects available",
                            app_state.effect_registry.all().len()
                        );
                    }
                }

                let active = app_state.active_scene_id;
                let program = app_state.program_scene_id;
                let trans = app_state.active_transition.as_ref().map(|t| {
                    (
                        t.from_scene,
                        t.to_scene,
                        t.transition.clone(),
                        t.progress(),
                        t.is_complete(),
                        t.colors,
                        t.params.clone(),
                    )
                });
                let transition_registry = app_state.transition_registry.clone();
                let registry_changed = app_state.transition_registry_changed;
                if registry_changed {
                    app_state.transition_registry_changed = false;
                }
                let effect_registry = app_state.effect_registry.clone();
                let effect_registry_changed = app_state.effect_registry_changed;
                if effect_registry_changed {
                    app_state.effect_registry_changed = false;
                }
                let encoding = app_state.stream_status.is_live()
                    || matches!(
                        app_state.recording_status,
                        crate::state::RecordingStatus::Recording { .. }
                    )
                    || app_state.virtual_camera_active;
                // Use the highest FPS across all encode targets so no target is starved.
                let encode_fps = app_state
                    .settings
                    .video
                    .fps
                    .max(app_state.settings.stream.fps)
                    .max(app_state.settings.record.fps);
                (
                    active,
                    program,
                    trans,
                    encoding,
                    encode_fps,
                    transition_registry,
                    registry_changed,
                    effect_registry,
                    effect_registry_changed,
                )
            };
            // Invalidate compiled shader pipelines when the registry changed.
            if registry_changed {
                log::info!("Invalidating transition pipelines — registry changed");
                gpu.transition_pipeline.invalidate_user_shaders();
            }
            if effect_registry_changed {
                log::info!(
                    "Invalidating effect pipelines — registry changed ({} effects)",
                    effect_registry.all().len()
                );
                gpu.effect_pipeline.invalidate_user_shaders();
            }

            let mut did_transition_blend = false;

            if let Some(active_scene_id) = active_id {
                let program_scene_id = program_id.unwrap_or(active_scene_id);
                let scenes_differ = active_scene_id != program_scene_id;

                // Resolve sources for the active scene (always needed for Preview).
                // Also resolve program scene sources when they differ.
                // During a transition, resolve both the from and to scenes.
                let (
                    active_sources,
                    program_sources,
                    transition_from_sources,
                    transition_to_sources,
                ) = {
                    let app_state = self.state.lock().expect("lock AppState");
                    let active = resolve_scene_sources(&app_state, active_scene_id);
                    let program = if scenes_differ {
                        Some(resolve_scene_sources(&app_state, program_scene_id))
                    } else {
                        None
                    };
                    let (trans_from, trans_to) =
                        if let Some((from, to, _, _, _, _, _)) = transition_info {
                            // During transition, we need both from and to scenes composed.
                            // The from scene is the current program_scene_id.
                            // The to scene may or may not be active_scene_id.
                            let from_sources = if from == active_scene_id {
                                None // reuse active_sources from primary canvas
                            } else {
                                Some(resolve_scene_sources(&app_state, from))
                            };
                            let to_sources = if to == active_scene_id {
                                None // reuse active_sources from primary canvas
                            } else {
                                Some(resolve_scene_sources(&app_state, to))
                            };
                            (from_sources, to_sources)
                        } else {
                            (None, None)
                        };
                    (active, program, trans_from, trans_to)
                };

                // Allocate secondary canvas when program differs from active, or
                // during a transition that needs a separate compose target.
                let need_secondary = scenes_differ || transition_info.is_some();
                if need_secondary && gpu.secondary_canvas.is_none() {
                    gpu.secondary_canvas =
                        Some(crate::renderer::secondary_canvas::SecondaryCanvas::new(
                            &gpu.device,
                            gpu.compositor.canvas_width,
                            gpu.compositor.canvas_height,
                            gpu.compositor.texture_bind_group_layout(),
                            gpu.compositor.compositor_sampler(),
                        ));
                } else if !need_secondary {
                    // Deallocate when no longer needed.
                    gpu.secondary_canvas = None;
                }

                let mut encoder =
                    gpu.device
                        .create_command_encoder(&egui_wgpu::wgpu::CommandEncoderDescriptor {
                            label: Some("compositor_encoder"),
                        });

                // Time value for animated effects (seconds since app start).
                // Using Instant instead of epoch avoids f32 precision loss —
                // epoch seconds (~1.7e9) leave only ~128s of fractional precision.
                let effect_time = self.start_time.elapsed().as_secs_f32();

                // Always compose active_scene_id onto primary canvas (Preview).
                gpu.compositor.compose(
                    &gpu.device,
                    &gpu.queue,
                    &mut encoder,
                    &active_sources,
                    Some(&mut gpu.effect_pipeline),
                    effect_time,
                    Some(&effect_registry),
                );

                // Track whether we need to force readback from the output texture.
                let mut force_output_readback = false;

                if let Some((from, to, ref transition_id, progress, _, colors, ref params)) =
                    transition_info
                {
                    // --- Transition in progress ---
                    // We need the from-scene and to-scene composed onto separate
                    // targets so the blend pass can sample both.
                    //
                    // Strategy:
                    //   - primary canvas has active_scene_id (may be to_scene)
                    //   - secondary canvas gets the "other" scene
                    //   - blend pass writes to the output texture

                    // Determine which bind groups represent from and to for blending.
                    // If from == active, primary canvas already has it.
                    // If to == active, primary canvas already has it.
                    // Otherwise, compose the missing scene(s) onto secondary canvas.

                    // We'll compose the from scene onto secondary if it's not already on primary.
                    if let Some(ref from_sources) = transition_from_sources
                        && let Some(ref secondary) = gpu.secondary_canvas
                    {
                        gpu.compositor.compose_to(
                            &gpu.device,
                            &gpu.queue,
                            &mut encoder,
                            &secondary.view,
                            gpu.compositor.source_layers(),
                            from_sources,
                            Some(&mut gpu.effect_pipeline),
                            effect_time,
                            Some(&effect_registry),
                        );
                    }

                    // If to_scene also needs separate compose and from != to on secondary,
                    // we'd need a third canvas. For simplicity, handle the common case:
                    // from = old program, to = active (already on primary).
                    // The rare case where neither from nor to is active would skip the blend.

                    if transition_id != crate::transition::TRANSITION_CUT {
                        // Determine from/to bind groups for the blend pass.
                        let from_bg = if from == active_scene_id {
                            // from is on primary canvas
                            Some(gpu.compositor.output_bind_group())
                        } else if transition_from_sources.is_some() {
                            // from was composed onto secondary canvas
                            gpu.secondary_canvas.as_ref().map(|s| s.bind_group.as_ref())
                        } else {
                            None
                        };

                        let to_bg = if to == active_scene_id {
                            // to is on primary canvas
                            Some(gpu.compositor.output_bind_group())
                        } else if let Some(ref to_src) = transition_to_sources {
                            // Need to compose to_scene onto secondary. But secondary
                            // may already have from_scene. In the common case (to == active),
                            // this branch is not reached. For the uncommon case, compose
                            // to_scene onto secondary (overwriting from, which we already
                            // sampled via bind group).
                            if let Some(ref secondary) = gpu.secondary_canvas {
                                gpu.compositor.compose_to(
                                    &gpu.device,
                                    &gpu.queue,
                                    &mut encoder,
                                    &secondary.view,
                                    gpu.compositor.source_layers(),
                                    to_src,
                                    Some(&mut gpu.effect_pipeline),
                                    effect_time,
                                    Some(&effect_registry),
                                );
                            }
                            gpu.secondary_canvas.as_ref().map(|s| s.bind_group.as_ref())
                        } else {
                            None
                        };

                        if let (Some(from_bind_group), Some(to_bind_group)) = (from_bg, to_bg) {
                            let time = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_secs_f32();
                            gpu.transition_pipeline.blend(
                                &gpu.device,
                                &gpu.queue,
                                &mut encoder,
                                from_bind_group,
                                to_bind_group,
                                gpu.compositor.output_texture_view(),
                                transition_id,
                                progress,
                                time,
                                &colors,
                                params,
                                &transition_registry,
                            );
                            did_transition_blend = true;
                            force_output_readback = true;
                        }
                    }
                } else if scenes_differ {
                    // --- No transition, but program differs from active ---
                    // Compose program scene onto secondary canvas for Live panel + encoding.
                    if let Some(ref prog_sources) = program_sources
                        && let Some(ref secondary) = gpu.secondary_canvas
                    {
                        gpu.compositor.compose_to(
                            &gpu.device,
                            &gpu.queue,
                            &mut encoder,
                            &secondary.view,
                            gpu.compositor.source_layers(),
                            prog_sources,
                            Some(&mut gpu.effect_pipeline),
                            effect_time,
                            Some(&effect_registry),
                        );
                    }

                    // When encoding, blit secondary canvas to output texture for readback.
                    if is_encoding && let Some(ref secondary) = gpu.secondary_canvas {
                        gpu.compositor
                            .scale_from_bind_group(&mut encoder, &secondary.bind_group);
                        force_output_readback = true;
                    }
                }

                // Scale primary canvas to output when encoding the same scene
                // (skip if blend or secondary blit already wrote to output).
                if is_encoding && !did_transition_blend && !force_output_readback {
                    gpu.compositor.scale_to_output(&mut encoder);
                }

                gpu.queue.submit(std::iter::once(encoder.finish()));

                // Start async readback at the target encode FPS, not the display
                // refresh rate. Without throttling, the encoder receives frames at
                // 60-120fps but the video is configured for 30fps, causing
                // choppiness and duplicate frame issues.
                if is_encoding {
                    let target_interval =
                        std::time::Duration::from_secs_f64(1.0 / encode_fps.max(1) as f64);
                    if self.last_readback_at.elapsed() >= target_interval {
                        self.last_readback_at = std::time::Instant::now();
                        gpu.compositor.start_readback(
                            &gpu.device,
                            &gpu.queue,
                            force_output_readback,
                        );
                    }
                }

                // Complete transition if done.
                if let Some((from_scene, to_scene, _, _, is_complete, _, _)) = transition_info
                    && is_complete
                {
                    let mut app_state = self.state.lock().expect("lock AppState");
                    app_state.program_scene_id = Some(to_scene);
                    app_state.active_transition = None;

                    // If program now matches active, deallocate secondary canvas.
                    if app_state.program_scene_id == app_state.active_scene_id {
                        gpu.secondary_canvas = None;
                    }

                    // Send RemoveCaptureSource for sources exclusive to the old
                    // program scene that aren't needed by active_scene_id either.
                    let new_source_ids: Vec<crate::scene::SourceId> = app_state
                        .scenes
                        .iter()
                        .find(|s| s.id == to_scene)
                        .map(|s| s.source_ids())
                        .unwrap_or_default();
                    let active_source_ids: Vec<crate::scene::SourceId> = app_state
                        .active_scene_id
                        .and_then(|aid| app_state.scenes.iter().find(|s| s.id == aid))
                        .map(|s| s.source_ids())
                        .unwrap_or_default();
                    let old_source_ids: Vec<crate::scene::SourceId> = app_state
                        .scenes
                        .iter()
                        .find(|s| s.id == from_scene)
                        .map(|s| s.source_ids())
                        .unwrap_or_default();
                    if let Some(ref channels) = self.gst_channels {
                        for old_id in &old_source_ids {
                            if !new_source_ids.contains(old_id)
                                && !active_source_ids.contains(old_id)
                            {
                                let _ = channels.command_tx.try_send(
                                    crate::gstreamer::GstCommand::RemoveCaptureSource {
                                        source_id: *old_id,
                                    },
                                );
                            }
                        }
                    }
                }
            }

            // Update preview and live resources on all windows.
            // Preview always shows the primary canvas (active_scene_id).
            // Live shows:
            //   - output texture during a transition blend (the blended result)
            //   - secondary canvas when program differs from active (no transition)
            //   - primary canvas when they're the same
            {
                let live_bind_group = if did_transition_blend {
                    gpu.compositor.output_preview_bind_group()
                } else if let Some(ref secondary) = gpu.secondary_canvas {
                    Arc::clone(&secondary.bind_group)
                } else {
                    gpu.compositor.canvas_bind_group()
                };
                for win in self.windows.values_mut() {
                    let new_preview = PreviewResources {
                        pipeline: gpu.compositor.canvas_pipeline(),
                        bind_group: gpu.compositor.canvas_bind_group(),
                    };
                    win.egui_renderer.callback_resources.insert(new_preview);

                    let new_live = LiveResources {
                        pipeline: gpu.compositor.canvas_pipeline(),
                        bind_group: Arc::clone(&live_bind_group),
                    };
                    win.egui_renderer.callback_resources.insert(new_live);
                }
            }

            // Request continuous redraws while a transition is in progress.
            if transition_info.is_some()
                && let Some(main_id) = self.main_window_id
                && let Some(win) = self.windows.get(&main_id)
            {
                win.window.request_redraw();
            }
        }

        if let Some(ref mut channels) = self.gst_channels {
            // Poll backend runtime state for stream/record/vcam lifecycle changes.
            if channels.runtime_state_rx.has_changed().unwrap_or(false) {
                let runtime = channels.runtime_state_rx.borrow_and_update().clone();
                let mut app_state = self.state.lock().expect("lock AppState");

                if runtime.stream_active {
                    if !app_state.stream_status.is_live() {
                        app_state.stream_status = crate::state::StreamStatus::Live {
                            uptime_secs: 0.0,
                            bitrate_kbps: 0.0,
                            dropped_frames: 0,
                        };
                    }
                } else {
                    app_state.stream_status = crate::state::StreamStatus::Offline;
                }

                match runtime.recording_path {
                    Some(path) => {
                        let path_changed = !matches!(
                            &app_state.recording_status,
                            crate::state::RecordingStatus::Recording { path: current }
                                if current == &path
                        );
                        app_state.recording_status =
                            crate::state::RecordingStatus::Recording { path };
                        if path_changed {
                            app_state.recording_started_at = Some(std::time::Instant::now());
                        }
                    }
                    None => {
                        app_state.recording_status = crate::state::RecordingStatus::Idle;
                        app_state.recording_started_at = None;
                    }
                }

                app_state.virtual_camera_active = runtime.virtual_camera_active;
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

            // Poll encoder list (populated once at startup)
            if channels.encoders_rx.has_changed().unwrap_or(false) {
                let encoders = channels.encoders_rx.borrow().clone();
                let mut app_state = self.state.lock().expect("lock AppState");
                app_state.available_encoders = encoders;
            }
        }

        // Check if display exclusion setting changed (Windows: update window affinity).
        #[cfg(target_os = "windows")]
        {
            let mut app_state = self.state.lock().expect("lock AppState");
            if app_state.display_exclusion_changed {
                app_state.display_exclusion_changed = false;
                let exclude = app_state.settings.general.exclude_self_from_capture;
                drop(app_state);
                self.set_window_display_affinity(exclude);
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
                let win_state = WindowState::new(window, gpu, layout, false, None, None)
                    .expect("init settings window");
                let window_id = window.id();
                self.windows.insert(window_id, win_state);
                self.settings_window_id = Some(window_id);
                self.refresh_display_exclusion();
            }
        }

        // Window picker: non-blocking overlay lifecycle.
        {
            let app_state = self.state.lock().expect("lock AppState");
            let should_start = app_state.window_picker_active && !app_state.window_picker_running;
            let is_running = app_state.window_picker_running;
            drop(app_state);

            if should_start {
                crate::ui::window_picker::start_window_picker();
                let mut app_state = self.state.lock().expect("lock AppState");
                app_state.window_picker_running = true;
                app_state.window_picker_active = false;
            } else if is_running {
                // Poll for result each frame.
                if let Some(result) = crate::ui::window_picker::poll_window_picker() {
                    crate::ui::window_picker::stop_window_picker();
                    let mut app_state = self.state.lock().expect("lock AppState");
                    app_state.window_picker_running = false;
                    app_state.window_picker_result = result;
                }
                // Request redraw to keep polling.
                for win in self.windows.values() {
                    win.window.request_redraw();
                }
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
                let live_bind_group = if let Some(ref secondary) = gpu.secondary_canvas {
                    Arc::clone(&secondary.bind_group)
                } else {
                    gpu.compositor.canvas_bind_group()
                };
                let live_resources = LiveResources {
                    pipeline: gpu.compositor.canvas_pipeline(),
                    bind_group: live_bind_group,
                };
                let win_state = WindowState::new(
                    window,
                    gpu,
                    layout,
                    false,
                    Some(preview_resources),
                    Some(live_resources),
                )
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
            transition_override: Default::default(),
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
            transition_override: Default::default(),
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
            transition_override: Default::default(),
        };
        let new = Scene {
            id: SceneId(2),
            name: "B".into(),
            sources: vec![SceneSource::new(SourceId(2)), SceneSource::new(SourceId(3))],
            pinned: false,
            transition_override: Default::default(),
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
            transition_override: Default::default(),
        };
        let (to_add, to_remove) = diff_scene_sources(Some(&scene), Some(&scene));
        assert!(to_add.is_empty());
        assert!(to_remove.is_empty());
    }
}

/// Map a key name (from HotkeyBinding) to a Windows virtual key code.
#[cfg(target_os = "windows")]
fn key_name_to_vk(name: &str) -> Option<u32> {
    match name.to_uppercase().as_str() {
        "A" => Some(0x41),
        "B" => Some(0x42),
        "C" => Some(0x43),
        "D" => Some(0x44),
        "E" => Some(0x45),
        "F" => Some(0x46),
        "G" => Some(0x47),
        "H" => Some(0x48),
        "I" => Some(0x49),
        "J" => Some(0x4A),
        "K" => Some(0x4B),
        "L" => Some(0x4C),
        "M" => Some(0x4D),
        "N" => Some(0x4E),
        "O" => Some(0x4F),
        "P" => Some(0x50),
        "Q" => Some(0x51),
        "R" => Some(0x52),
        "S" => Some(0x53),
        "T" => Some(0x54),
        "U" => Some(0x55),
        "V" => Some(0x56),
        "W" => Some(0x57),
        "X" => Some(0x58),
        "Y" => Some(0x59),
        "Z" => Some(0x5A),
        "0" => Some(0x30),
        "1" => Some(0x31),
        "2" => Some(0x32),
        "3" => Some(0x33),
        "4" => Some(0x34),
        "5" => Some(0x35),
        "6" => Some(0x36),
        "7" => Some(0x37),
        "8" => Some(0x38),
        "9" => Some(0x39),
        "F1" => Some(0x70),
        "F2" => Some(0x71),
        "F3" => Some(0x72),
        "F4" => Some(0x73),
        "F5" => Some(0x74),
        "F6" => Some(0x75),
        "F7" => Some(0x76),
        "F8" => Some(0x77),
        "F9" => Some(0x78),
        "F10" => Some(0x79),
        "F11" => Some(0x7A),
        "F12" => Some(0x7B),
        "SPACE" => Some(0x20),
        "ENTER" => Some(0x0D),
        "ESCAPE" => Some(0x1B),
        "BACKSPACE" => Some(0x08),
        "DELETE" => Some(0x2E),
        "TAB" => Some(0x09),
        "UP" => Some(0x26),
        "DOWN" => Some(0x28),
        "LEFT" => Some(0x25),
        "RIGHT" => Some(0x27),
        "HOME" => Some(0x24),
        "END" => Some(0x23),
        "PAGEUP" => Some(0x21),
        "PAGEDOWN" => Some(0x22),
        "INSERT" => Some(0x2D),
        "[" => Some(0xDB),
        "]" => Some(0xDD),
        "," => Some(0xBC),
        "." => Some(0xBE),
        "/" => Some(0xBF),
        "\\" => Some(0xDC),
        ";" => Some(0xBA),
        "'" => Some(0xDE),
        "`" => Some(0xC0),
        "-" => Some(0xBD),
        "=" => Some(0xBB),
        _ => None,
    }
}

fn main() -> Result<()> {
    env_logger::init();
    log::info!("Lodestone starting");

    #[cfg(target_os = "macos")]
    system_extension::activate_camera_extension();

    text_source::init_font_system();
    let event_loop = EventLoop::new()?;
    let mut app = AppManager::new();
    event_loop.run_app(&mut app)?;
    Ok(())
}
