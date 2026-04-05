use anyhow::Result;
use egui_wgpu::wgpu;
use egui_wgpu::wgpu::{Surface, SurfaceConfiguration};
use winit::window::Window;

use crate::renderer::SharedGpuState;
use crate::state::AppState;
use crate::ui::layout::tree::{DockLayout, GroupId, PanelId, PanelType};
use crate::ui::live_panel::LiveResources;
use crate::ui::preview_panel::PreviewResources;
use crate::window_actions::apply_layout_actions;

/// A request to create a new OS-level window for a detached panel.
pub struct DetachRequest {
    pub panel_type: PanelType,
    pub panel_id: PanelId,
    pub group_id: GroupId,
}

/// Per-window state including surface, egui context, and layout.
pub struct WindowState {
    pub window: &'static Window,
    pub surface: Surface<'static>,
    pub surface_config: SurfaceConfiguration,
    pub egui_renderer: egui_wgpu::Renderer,
    pub egui_state: egui_winit::State,
    pub egui_ctx: egui::Context,
    pub layout: DockLayout,
    pub is_main: bool,
    /// Set to true when user requests reattaching this window's panels to main.
    pub reattach_pending: bool,
    /// Font families that were successfully loaded and registered with egui.
    pub loaded_fonts: Vec<String>,
    /// Base font definitions (with all loaded fonts, before any family reordering).
    base_font_defs: egui::FontDefinitions,
    /// Whether egui has completed its first `run()`. Font queries panic before this.
    first_frame_done: bool,
    /// Last title bar color applied via DWM, to avoid redundant API calls.
    #[cfg(target_os = "windows")]
    last_titlebar_color: Option<egui::Color32>,
}

impl WindowState {
    pub fn new(
        window: &'static Window,
        gpu: &SharedGpuState,
        layout: DockLayout,
        is_main: bool,
        preview_resources: Option<PreviewResources>,
        live_resources: Option<LiveResources>,
    ) -> Result<Self> {
        let surface = gpu.instance.create_surface(window)?;

        let size = window.inner_size();
        let surface_config = SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: gpu.format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&gpu.device, &surface_config);

        let mut egui_renderer = egui_wgpu::Renderer::new(
            &gpu.device,
            gpu.format,
            egui_wgpu::RendererOptions::default(),
        );

        if let Some(resources) = preview_resources {
            egui_renderer.callback_resources.insert(resources);
        }
        if let Some(resources) = live_resources {
            egui_renderer.callback_resources.insert(resources);
        }

        let egui_ctx = egui::Context::default();
        egui_ctx.set_visuals(egui::Visuals::dark());
        egui_ctx.style_mut(|style| {
            style.spacing.button_padding = crate::ui::theme::BTN_PADDING;
        });
        // Register Phosphor icon font so icon constants render as glyphs.
        let mut fonts = egui::FontDefinitions::default();
        egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);

        // Register system fonts for font family switching.
        #[cfg(target_os = "macos")]
        let system_font_names: &[&str] = &["SF Pro", "Helvetica Neue", "Menlo", "Monaco"];
        #[cfg(target_os = "windows")]
        let system_font_names: &[&str] = &["segoeui", "consola", "cascadiamono", "arial"];
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        let system_font_names: &[&str] = &["DejaVuSans", "LiberationMono"];
        let mut loaded_fonts = vec!["Default".to_string()];
        for name in system_font_names {
            if let Some(data) = Self::load_system_font(name) {
                fonts.font_data.insert(
                    name.to_string(),
                    std::sync::Arc::new(egui::FontData::from_owned(data)),
                );
                fonts.families.insert(
                    egui::FontFamily::Name((*name).into()),
                    vec![name.to_string(), "Hack".to_string()],
                );
                loaded_fonts.push(name.to_string());
            }
        }

        let base_font_defs = fonts.clone();
        egui_ctx.set_fonts(fonts);
        let max_tex = gpu.device.limits().max_texture_dimension_2d as usize;
        let egui_state = egui_winit::State::new(
            egui_ctx.clone(),
            egui::ViewportId::ROOT,
            window,
            Some(window.scale_factor() as f32),
            None,
            Some(max_tex),
        );

        Ok(Self {
            window,
            surface,
            surface_config,
            egui_renderer,
            egui_state,
            egui_ctx,
            layout,
            is_main,
            reattach_pending: false,
            loaded_fonts,
            base_font_defs,
            first_frame_done: false,
            #[cfg(target_os = "windows")]
            last_titlebar_color: None,
        })
    }

    pub fn resize(&mut self, gpu: &SharedGpuState, width: u32, height: u32) {
        if width > 0 && height > 0 {
            self.surface_config.width = width;
            self.surface_config.height = height;
            self.surface.configure(&gpu.device, &self.surface_config);
        }
    }

    /// Render the window contents. Returns detach requests and whether the
    /// settings button was clicked.
    pub fn render(
        &mut self,
        gpu: &SharedGpuState,
        state: &mut AppState,
    ) -> Result<(Vec<DetachRequest>, bool)> {
        // Resolve active theme from settings, applying any per-user accent override.
        let mut theme = crate::ui::theme::Theme::builtin(state.settings.appearance.theme);
        if let Some(ref hex) = state.settings.appearance.accent_color {
            let accent = crate::ui::theme::parse_hex_color(hex);
            theme.accent = accent;
            theme.accent_hover = accent;
            theme.accent_dim = crate::ui::theme::accent_dim(accent);
        }
        // Store resolved Theme (with accent override applied) in context data.
        state.accent_color = theme.accent;
        self.egui_ctx.data_mut(|d| {
            d.insert_temp(egui::Id::new("active_theme"), theme.clone());
            d.insert_temp(egui::Id::new("accent_color"), theme.accent);
        });
        // Set egui dark/light visuals based on theme brightness.
        let luminance =
            theme.bg_base.r() as u16 + theme.bg_base.g() as u16 + theme.bg_base.b() as u16;
        if luminance < 384 {
            self.egui_ctx.set_visuals(egui::Visuals::dark());
        } else {
            self.egui_ctx.set_visuals(egui::Visuals::light());
        }

        // Sync OS title bar color with the active theme.
        #[cfg(target_os = "windows")]
        self.sync_titlebar_color(theme.bg_base);

        // Apply UI scale — scales everything (text, spacing, widgets) uniformly.
        self.egui_ctx
            .set_zoom_factor(state.settings.appearance.font_scale.zoom_factor());

        // Apply font family by rebuilding font definitions with the selected font first
        // in the Proportional family list. This preserves Phosphor icon fallback.
        // Guard: egui panics on fonts() before the first Context::run(). On Windows,
        // muda menu init can trigger a render before the first frame.
        if self.first_frame_done {
            let family = &state.settings.appearance.font_family;
            if family != "Default" && self.loaded_fonts.contains(family) {
                let mut font_defs = self.base_font_defs.clone();
                if let Some(list) = font_defs.families.get_mut(&egui::FontFamily::Proportional) {
                    // Insert selected font at front (before default + phosphor)
                    if !list.first().is_some_and(|f| f == family) {
                        list.retain(|n| n != family);
                        list.insert(0, family.clone());
                        self.egui_ctx.set_fonts(font_defs);
                    }
                }
            } else {
                // Reset to base (default font first)
                let current_first = self.egui_ctx.fonts(|f| {
                    f.definitions()
                        .families
                        .get(&egui::FontFamily::Proportional)
                        .and_then(|l| l.first().cloned())
                });
                let base_first = self
                    .base_font_defs
                    .families
                    .get(&egui::FontFamily::Proportional)
                    .and_then(|l| l.first().cloned());
                if current_first != base_first {
                    self.egui_ctx.set_fonts(self.base_font_defs.clone());
                }
            }
        }

        // Sync loaded fonts to AppState so the appearance settings dropdown shows valid options.
        if state.system_fonts != self.loaded_fonts {
            state.system_fonts = self.loaded_fonts.clone();
        }

        // Capture pre-frame undo snapshot before any UI mutations.
        if self.is_main {
            state.begin_frame_for_undo();
        }

        let raw_input = self.egui_state.take_egui_input(self.window);

        let layout = &self.layout;
        let is_main = self.is_main;
        let mut pending_actions = Vec::new();
        let mut open_settings = false;
        let full_output = self.egui_ctx.run(raw_input, |ctx| {
            // Detached single-panel windows skip the menu bar and toolbar.
            let available_rect = if is_main {
                let (menu_actions, _rect) = crate::ui::layout::render::render_menu_bar(ctx, layout, state);
                pending_actions = menu_actions;
                // Draw the toolbar (always visible on the main window).
                open_settings = crate::ui::toolbar::draw(ctx, state);
                ctx.available_rect()
            } else {
                ctx.available_rect()
            };
            pending_actions.extend(crate::ui::layout::render::render_layout(
                ctx,
                layout,
                state,
                available_rect,
                is_main,
            ));
        });
        self.first_frame_done = true;

        // Apply layout actions after the egui frame
        let detach_requests = apply_layout_actions(self, pending_actions);

        let pixels_per_point = full_output.pixels_per_point;
        let paint_jobs = self
            .egui_ctx
            .tessellate(full_output.shapes, pixels_per_point);

        // --- GPU render ---
        let output = self
            .surface
            .get_current_texture()
            .map_err(|e| anyhow::anyhow!("Failed to get surface texture: {e}"))?;
        let view = output.texture.create_view(&Default::default());
        let mut encoder = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("window_render_encoder"),
            });

        let screen_descriptor = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [self.surface_config.width, self.surface_config.height],
            pixels_per_point,
        };

        // Upload texture updates
        for (id, image_delta) in &full_output.textures_delta.set {
            self.egui_renderer
                .update_texture(&gpu.device, &gpu.queue, *id, image_delta);
        }

        // Update vertex/index buffers
        let user_cmd_bufs = self.egui_renderer.update_buffers(
            &gpu.device,
            &gpu.queue,
            &mut encoder,
            &paint_jobs,
            &screen_descriptor,
        );

        // Pass 1: Clear
        {
            let _clear_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("clear_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.08,
                            g: 0.08,
                            b: 0.10,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
        }

        // Pass 2: egui (includes preview via paint callbacks)
        {
            let mut render_pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("egui_pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                })
                .forget_lifetime();
            self.egui_renderer
                .render(&mut render_pass, &paint_jobs, &screen_descriptor);
            gpu.text_renderer.render()?;
        }

        // Free released textures
        for id in &full_output.textures_delta.free {
            self.egui_renderer.free_texture(id);
        }

        let mut cmds: Vec<wgpu::CommandBuffer> = user_cmd_bufs;
        cmds.push(encoder.finish());
        gpu.queue.submit(cmds);
        output.present();

        self.egui_state
            .handle_platform_output(self.window, full_output.platform_output);

        Ok((detach_requests, open_settings))
    }

    /// Render the settings window content.
    pub fn render_settings(&mut self, gpu: &SharedGpuState, state: &mut AppState) -> Result<()> {
        // Resolve active theme from settings, applying any per-user accent override.
        let mut theme = crate::ui::theme::Theme::builtin(state.settings.appearance.theme);
        if let Some(ref hex) = state.settings.appearance.accent_color {
            let accent = crate::ui::theme::parse_hex_color(hex);
            theme.accent = accent;
            theme.accent_hover = accent;
            theme.accent_dim = crate::ui::theme::accent_dim(accent);
        }
        // Store resolved Theme (with accent override applied) in context data.
        state.accent_color = theme.accent;
        self.egui_ctx.data_mut(|d| {
            d.insert_temp(egui::Id::new("active_theme"), theme.clone());
            d.insert_temp(egui::Id::new("accent_color"), theme.accent);
        });
        // Set egui dark/light visuals based on theme brightness.
        let luminance =
            theme.bg_base.r() as u16 + theme.bg_base.g() as u16 + theme.bg_base.b() as u16;
        if luminance < 384 {
            self.egui_ctx.set_visuals(egui::Visuals::dark());
        } else {
            self.egui_ctx.set_visuals(egui::Visuals::light());
        }

        // Sync OS title bar color with the active theme.
        #[cfg(target_os = "windows")]
        self.sync_titlebar_color(theme.bg_base);

        // Apply UI scale — scales everything (text, spacing, widgets) uniformly.
        self.egui_ctx
            .set_zoom_factor(state.settings.appearance.font_scale.zoom_factor());

        // Apply font family by rebuilding font definitions with the selected font first
        // in the Proportional family list. This preserves Phosphor icon fallback.
        // Guard: egui panics on fonts() before the first Context::run(). On Windows,
        // muda menu init can trigger a render before the first frame.
        if self.first_frame_done {
            let family = &state.settings.appearance.font_family;
            if family != "Default" && self.loaded_fonts.contains(family) {
                let mut font_defs = self.base_font_defs.clone();
                if let Some(list) = font_defs.families.get_mut(&egui::FontFamily::Proportional) {
                    // Insert selected font at front (before default + phosphor)
                    if !list.first().is_some_and(|f| f == family) {
                        list.retain(|n| n != family);
                        list.insert(0, family.clone());
                        self.egui_ctx.set_fonts(font_defs);
                    }
                }
            } else {
                // Reset to base (default font first)
                let current_first = self.egui_ctx.fonts(|f| {
                    f.definitions()
                        .families
                        .get(&egui::FontFamily::Proportional)
                        .and_then(|l| l.first().cloned())
                });
                let base_first = self
                    .base_font_defs
                    .families
                    .get(&egui::FontFamily::Proportional)
                    .and_then(|l| l.first().cloned());
                if current_first != base_first {
                    self.egui_ctx.set_fonts(self.base_font_defs.clone());
                }
            }
        }

        // Sync loaded fonts to AppState so the appearance settings dropdown shows valid options.
        if state.system_fonts != self.loaded_fonts {
            state.system_fonts = self.loaded_fonts.clone();
        }

        let raw_input = self.egui_state.take_egui_input(self.window);

        let full_output = self.egui_ctx.run(raw_input, |ctx| {
            crate::ui::settings::render_native(ctx, state);
        });

        let pixels_per_point = full_output.pixels_per_point;
        let paint_jobs = self
            .egui_ctx
            .tessellate(full_output.shapes, pixels_per_point);

        let output = self
            .surface
            .get_current_texture()
            .map_err(|e| anyhow::anyhow!("Failed to get surface texture: {e}"))?;
        let view = output.texture.create_view(&Default::default());
        let mut encoder = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("settings_render_encoder"),
            });

        let screen_descriptor = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [self.surface_config.width, self.surface_config.height],
            pixels_per_point,
        };

        for (id, image_delta) in &full_output.textures_delta.set {
            self.egui_renderer
                .update_texture(&gpu.device, &gpu.queue, *id, image_delta);
        }

        let user_cmd_bufs = self.egui_renderer.update_buffers(
            &gpu.device,
            &gpu.queue,
            &mut encoder,
            &paint_jobs,
            &screen_descriptor,
        );

        // Clear pass
        {
            let _clear_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("settings_clear_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.08,
                            g: 0.08,
                            b: 0.10,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
        }

        // egui pass
        {
            let mut render_pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("settings_egui_pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                })
                .forget_lifetime();
            self.egui_renderer
                .render(&mut render_pass, &paint_jobs, &screen_descriptor);
            gpu.text_renderer.render()?;
        }

        for id in &full_output.textures_delta.free {
            self.egui_renderer.free_texture(id);
        }

        let mut cmds: Vec<wgpu::CommandBuffer> = user_cmd_bufs;
        cmds.push(encoder.finish());
        gpu.queue.submit(cmds);
        output.present();

        self.egui_state
            .handle_platform_output(self.window, full_output.platform_output);

        Ok(())
    }

    /// Update the Windows title bar caption color to match the given theme background.
    ///
    /// Uses `DwmSetWindowAttribute` with `DWMWA_CAPTION_COLOR` (35) and
    /// `DWMWA_USE_IMMERSIVE_DARK_MODE` (20). Only issues API calls when the
    /// color actually changes.
    #[cfg(target_os = "windows")]
    fn sync_titlebar_color(&mut self, bg: egui::Color32) {
        if self.last_titlebar_color == Some(bg) {
            return;
        }
        self.last_titlebar_color = Some(bg);

        use raw_window_handle::HasWindowHandle;

        // DWM constants
        const DWMWA_USE_IMMERSIVE_DARK_MODE: u32 = 20;
        const DWMWA_CAPTION_COLOR: u32 = 35;

        unsafe extern "system" {
            fn DwmSetWindowAttribute(
                hwnd: isize,
                dw_attribute: u32,
                pv_attribute: *const std::ffi::c_void,
                cb_attribute: u32,
            ) -> i32;
        }

        let hwnd = match self.window.window_handle() {
            Ok(handle) => match handle.as_raw() {
                raw_window_handle::RawWindowHandle::Win32(h) => h.hwnd.get() as isize,
                _ => return,
            },
            Err(_) => return,
        };

        // COLORREF is 0x00BBGGRR
        let colorref: u32 = (bg.r() as u32) | ((bg.g() as u32) << 8) | ((bg.b() as u32) << 16);

        unsafe {
            DwmSetWindowAttribute(
                hwnd,
                DWMWA_CAPTION_COLOR,
                &colorref as *const u32 as *const std::ffi::c_void,
                std::mem::size_of::<u32>() as u32,
            );
        }

        // Use dark title bar text/buttons for light themes, immersive dark mode for dark themes.
        let luminance = bg.r() as u16 + bg.g() as u16 + bg.b() as u16;
        let use_dark: u32 = if luminance < 384 { 1 } else { 0 };
        unsafe {
            DwmSetWindowAttribute(
                hwnd,
                DWMWA_USE_IMMERSIVE_DARK_MODE,
                &use_dark as *const u32 as *const std::ffi::c_void,
                std::mem::size_of::<u32>() as u32,
            );
        }
    }

    /// Close a tab. If it's the last tab in a floating group, remove the floating group.
    /// If it's the last tab in a grid group (non-root), collapse the parent split.
    pub(crate) fn apply_close(&mut self, group_id: GroupId, tab_index: usize) {
        let group = match self.layout.groups.get(&group_id) {
            Some(g) => g,
            None => return,
        };

        if group.tabs.len() <= 1 {
            // Last tab — remove the entire group
            if self.layout.is_floating(group_id) {
                self.layout.remove_floating(group_id);
                self.layout.groups.remove(&group_id);
            } else {
                self.layout.remove_group_from_grid(group_id);
            }
        } else if let Some(group) = self.layout.groups.get_mut(&group_id) {
            group.remove_tab(tab_index);
        }
    }

    /// Attempt to load a system font by name from common system font paths.
    fn load_system_font(name: &str) -> Option<Vec<u8>> {
        let mut candidates: Vec<std::path::PathBuf> = Vec::new();

        #[cfg(target_os = "macos")]
        {
            candidates.extend([
                format!("/System/Library/Fonts/{name}.ttf").into(),
                format!("/System/Library/Fonts/{name}.otf").into(),
                format!("/Library/Fonts/{name}.ttf").into(),
                format!("/Library/Fonts/{name}.otf").into(),
                // SF Pro and Helvetica Neue use .ttc (TrueType Collection)
                format!("/System/Library/Fonts/{name}.ttc").into(),
                // Some fonts use spaces in filenames
                format!("/System/Library/Fonts/{}.ttf", name.replace(' ', "")).into(),
                format!("/System/Library/Fonts/{}.ttc", name.replace(' ', "")).into(),
            ]);
            if let Some(h) = dirs::home_dir() {
                candidates.push(h.join(format!("Library/Fonts/{name}.ttf")));
            }
        }

        #[cfg(target_os = "windows")]
        {
            // Windows system fonts directory
            let win_dir =
                std::env::var("WINDIR").unwrap_or_else(|_| r"C:\Windows".to_string());
            let fonts_dir = std::path::PathBuf::from(&win_dir).join("Fonts");
            candidates.extend([
                fonts_dir.join(format!("{name}.ttf")),
                fonts_dir.join(format!("{name}.otf")),
                fonts_dir.join(format!("{name}.ttc")),
                // Windows font files sometimes use no-space names
                fonts_dir.join(format!("{}.ttf", name.replace(' ', ""))),
                fonts_dir.join(format!("{}.ttc", name.replace(' ', ""))),
            ]);
            // Per-user fonts (Windows 10 1809+)
            if let Some(local) = dirs::data_local_dir() {
                let user_fonts = local.join("Microsoft").join("Windows").join("Fonts");
                candidates.push(user_fonts.join(format!("{name}.ttf")));
                candidates.push(user_fonts.join(format!("{name}.otf")));
            }
        }

        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            // Linux / other: common font directories
            candidates.extend([
                format!("/usr/share/fonts/truetype/{name}.ttf").into(),
                format!("/usr/share/fonts/{name}.ttf").into(),
            ]);
        }
        for path in candidates {
            if let Ok(data) = std::fs::read(&path) {
                log::info!("Loaded system font '{}' from {}", name, path.display());
                return Some(data);
            }
        }
        log::debug!("System font '{}' not found", name);
        None
    }
}
