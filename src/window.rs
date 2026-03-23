use anyhow::Result;
use egui_wgpu::wgpu;
use egui_wgpu::wgpu::{Surface, SurfaceConfiguration};
use winit::window::Window;

use crate::renderer::SharedGpuState;
use crate::state::AppState;
use crate::ui::layout::render::LayoutAction;
use crate::ui::layout::tree::{
    DockLayout, DragState, DropZone, GroupId, PanelId, PanelType, SplitDirection,
};
use crate::ui::preview_panel::PreviewResources;

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
}

impl WindowState {
    pub fn new(
        window: &'static Window,
        gpu: &SharedGpuState,
        layout: DockLayout,
        is_main: bool,
        preview_resources: Option<PreviewResources>,
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

        let egui_ctx = egui::Context::default();
        egui_ctx.set_visuals(egui::Visuals::dark());
        egui_ctx.style_mut(|style| {
            style.spacing.button_padding = crate::ui::theme::BTN_PADDING;
        });
        // Register Phosphor icon font so icon constants render as glyphs.
        let mut fonts = egui::FontDefinitions::default();
        egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
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
        let raw_input = self.egui_state.take_egui_input(self.window);

        let layout = &self.layout;
        let is_main = self.is_main;
        let mut pending_actions = Vec::new();
        let mut open_settings = false;
        let full_output = self.egui_ctx.run(raw_input, |ctx| {
            // Detached single-panel windows skip the menu bar and toolbar.
            let available_rect = if is_main {
                let (menu_actions, _rect) = crate::ui::layout::render::render_menu_bar(ctx, layout);
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

        // Apply layout actions after the egui frame
        let mut detach_requests = Vec::new();
        for action in pending_actions {
            match action {
                LayoutAction::Resize { node_id, new_ratio } => {
                    self.layout.resize(node_id, new_ratio);
                }
                LayoutAction::SetActiveTab {
                    group_id,
                    tab_index,
                } => {
                    if let Some(group) = self.layout.groups.get_mut(&group_id)
                        && tab_index < group.tabs.len()
                    {
                        group.active_tab = tab_index;
                    }
                }
                LayoutAction::Close {
                    group_id,
                    tab_index,
                } => {
                    self.apply_close(group_id, tab_index);
                }
                LayoutAction::CloseOthers {
                    group_id,
                    tab_index,
                } => {
                    if let Some(group) = self.layout.groups.get_mut(&group_id)
                        && tab_index < group.tabs.len()
                    {
                        let kept = group.tabs[tab_index].clone();
                        group.tabs = vec![kept];
                        group.active_tab = 0;
                    }
                }
                LayoutAction::DetachToFloat {
                    group_id,
                    tab_index,
                } => {
                    if let Some(entry) = self.layout.take_tab(group_id, tab_index) {
                        self.layout
                            .add_floating_group(entry, egui::pos2(200.0, 200.0));
                    }
                }
                LayoutAction::DetachToWindow {
                    group_id,
                    tab_index,
                } => {
                    if let Some(entry) = self.layout.take_tab(group_id, tab_index) {
                        detach_requests.push(DetachRequest {
                            panel_type: entry.panel_type,
                            panel_id: entry.panel_id,
                            group_id: GroupId::next(),
                        });
                    }
                }
                LayoutAction::StartDrag {
                    group_id,
                    tab_index,
                } => {
                    if let Some(group) = self.layout.groups.get(&group_id)
                        && let Some(tab) = group.tabs.get(tab_index)
                    {
                        self.layout.drag = Some(DragState {
                            panel_id: tab.panel_id,
                            panel_type: tab.panel_type,
                            source_group: group_id,
                            tab_index,
                        });
                    }
                }
                LayoutAction::DropOnZone { target_group, zone } => {
                    if let Some(drag) = self.layout.drag.take()
                        && let Some(entry) = self.layout.take_tab(drag.source_group, drag.tab_index)
                    {
                        // Floating groups can't be split — always add as a tab
                        let is_floating = self.layout.is_floating(target_group);
                        match zone {
                            _ if is_floating => {
                                if let Some(group) = self.layout.groups.get_mut(&target_group) {
                                    group.add_tab_entry(entry);
                                }
                            }
                            DropZone::TabBar { index } => {
                                if let Some(group) = self.layout.groups.get_mut(&target_group) {
                                    // When reordering within the same group, the source
                                    // tab was already removed, shifting indices down.
                                    // Adjust the insertion index to compensate.
                                    let adjusted = if target_group == drag.source_group
                                        && drag.tab_index < index
                                    {
                                        index.saturating_sub(1)
                                    } else {
                                        index
                                    };
                                    group.insert_tab(adjusted, entry);
                                }
                            }
                            DropZone::Center => {
                                if let Some(group) = self.layout.groups.get_mut(&target_group) {
                                    group.add_tab_entry(entry);
                                }
                            }
                            DropZone::Left => {
                                self.layout.split_group_with_tab(
                                    target_group,
                                    SplitDirection::Vertical,
                                    entry,
                                    true,
                                );
                            }
                            DropZone::Right => {
                                self.layout.split_group_with_tab(
                                    target_group,
                                    SplitDirection::Vertical,
                                    entry,
                                    false,
                                );
                            }
                            DropZone::Top => {
                                self.layout.split_group_with_tab(
                                    target_group,
                                    SplitDirection::Horizontal,
                                    entry,
                                    true,
                                );
                            }
                            DropZone::Bottom => {
                                self.layout.split_group_with_tab(
                                    target_group,
                                    SplitDirection::Horizontal,
                                    entry,
                                    false,
                                );
                            }
                        }
                    }
                }
                LayoutAction::DropOnEmpty { pos } => {
                    if let Some(drag) = self.layout.drag.take()
                        && let Some(entry) = self.layout.take_tab(drag.source_group, drag.tab_index)
                    {
                        self.layout.add_floating_group(entry, pos);
                    }
                }
                LayoutAction::CancelDrag => {
                    self.layout.drag = None;
                }
                LayoutAction::AddPanel {
                    target_group,
                    panel_type,
                } => {
                    if let Some(group) = self.layout.groups.get_mut(&target_group) {
                        group.add_tab(panel_type);
                    }
                }
                LayoutAction::AddPanelAtRoot { panel_type } => {
                    self.layout.insert_at_root(
                        panel_type,
                        PanelId::next(),
                        SplitDirection::Vertical,
                        0.8,
                    );
                }
                LayoutAction::ResetLayout => {
                    self.layout = DockLayout::default_layout();
                }
                LayoutAction::ReattachToMain => {
                    self.reattach_pending = true;
                }
                LayoutAction::DockFloatingToGrid { group_id } => {
                    self.layout.insert_floating_into_grid(group_id);
                }
                LayoutAction::CloseFloatingGroup { group_id } => {
                    self.layout.remove_floating(group_id);
                    self.layout.groups.remove(&group_id);
                }
                LayoutAction::DetachGroupToFloat { group_id } => {
                    self.layout
                        .detach_grid_group_to_floating(group_id, egui::pos2(200.0, 200.0));
                }
                LayoutAction::MoveGroupToTarget {
                    source_group,
                    target_group,
                    zone,
                } => {
                    // Skip self-drop
                    if source_group == target_group {
                        continue;
                    }

                    // Take all tabs from the source group
                    let source_tabs = self
                        .layout
                        .groups
                        .get(&source_group)
                        .map(|g| g.tabs.clone())
                        .unwrap_or_default();

                    if source_tabs.is_empty() {
                        continue;
                    }

                    // Remove the source group from wherever it is
                    let was_floating = self.layout.is_floating(source_group);
                    if was_floating {
                        self.layout.remove_floating(source_group);
                        self.layout.groups.remove(&source_group);
                    } else {
                        self.layout.remove_group_from_grid(source_group);
                    }

                    // Add all tabs to the target based on drop zone
                    match zone {
                        DropZone::Center | DropZone::TabBar { .. } => {
                            if let Some(group) = self.layout.groups.get_mut(&target_group) {
                                for tab in source_tabs {
                                    group.add_tab_entry(tab);
                                }
                            }
                        }
                        _ => {
                            if let Some(first_tab) = source_tabs.first() {
                                let direction = match zone {
                                    DropZone::Left | DropZone::Right => SplitDirection::Vertical,
                                    _ => SplitDirection::Horizontal,
                                };
                                let before = matches!(zone, DropZone::Left | DropZone::Top);
                                if let Some(new_gid) = self.layout.split_group_with_tab(
                                    target_group,
                                    direction,
                                    first_tab.clone(),
                                    before,
                                ) && let Some(group) = self.layout.groups.get_mut(&new_gid)
                                {
                                    for tab in &source_tabs[1..] {
                                        group.add_tab_entry(tab.clone());
                                    }
                                }
                            }
                        }
                    }
                }
                LayoutAction::UpdateFloatingGeometry {
                    group_id,
                    pos,
                    size,
                } => {
                    self.layout.update_floating_geometry(group_id, pos, size);
                }
            }
        }

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
        let raw_input = self.egui_state.take_egui_input(self.window);

        let full_output = self.egui_ctx.run(raw_input, |ctx| {
            crate::ui::settings_window::render_native(ctx, state);
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

    /// Close a tab. If it's the last tab in a floating group, remove the floating group.
    /// If it's the last tab in a grid group (non-root), collapse the parent split.
    fn apply_close(&mut self, group_id: GroupId, tab_index: usize) {
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
}
