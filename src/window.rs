use anyhow::Result;
use egui_wgpu::wgpu;
use egui_wgpu::wgpu::{Surface, SurfaceConfiguration};
use winit::window::Window;

use crate::renderer::SharedGpuState;
use crate::state::AppState;
use crate::ui::layout::{LayoutTree, PanelId, PanelType};

pub struct DetachRequest {
    pub panel_type: PanelType,
    pub panel_id: PanelId,
}

pub struct WindowState {
    pub window: &'static Window,
    pub surface: Surface<'static>,
    pub surface_config: SurfaceConfiguration,
    pub egui_renderer: egui_wgpu::Renderer,
    pub egui_state: egui_winit::State,
    pub egui_ctx: egui::Context,
    pub layout: LayoutTree,
    #[allow(dead_code)]
    pub is_main: bool,
}

impl WindowState {
    pub fn new(
        window: &'static Window,
        gpu: &SharedGpuState,
        layout: LayoutTree,
        is_main: bool,
    ) -> Result<Self> {
        let surface = gpu.instance.create_surface(window)?;

        let size = window.inner_size();
        // Query capabilities using a temporary adapter — we reuse the same
        // device/queue from SharedGpuState but need surface capabilities for
        // configuration. Since we already picked the format during GPU init,
        // we can trust that format and just need alpha_mode.
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

        let egui_renderer = egui_wgpu::Renderer::new(
            &gpu.device,
            gpu.format,
            egui_wgpu::RendererOptions::default(),
        );

        let egui_ctx = egui::Context::default();
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
        })
    }

    /// Pick a default panel type for a newly-split panel that differs from
    /// the original. Prefers Preview; falls back to the first dockable type
    /// that isn't the original.
    fn pick_new_panel_type(original: PanelType) -> PanelType {
        const DOCKABLE: &[PanelType] = &[
            PanelType::Preview,
            PanelType::SceneEditor,
            PanelType::AudioMixer,
            PanelType::StreamControls,
        ];
        DOCKABLE
            .iter()
            .copied()
            .find(|&t| t != original)
            .unwrap_or(PanelType::Preview)
    }

    pub fn resize(&mut self, gpu: &SharedGpuState, width: u32, height: u32) {
        if width > 0 && height > 0 {
            self.surface_config.width = width;
            self.surface_config.height = height;
            self.surface.configure(&gpu.device, &self.surface_config);
        }
    }

    pub fn render(
        &mut self,
        gpu: &SharedGpuState,
        state: &mut AppState,
    ) -> Result<Vec<DetachRequest>> {
        let raw_input = self.egui_state.take_egui_input(self.window);

        let layout = &self.layout;
        let mut pending_actions = Vec::new();
        let full_output = self.egui_ctx.run(raw_input, |ctx| {
            // Render top menu bar first; it reserves space and returns the remaining rect.
            let (menu_actions, available_rect) =
                crate::ui::layout::render::render_menu_bar(ctx, layout);
            let mut actions = menu_actions;
            actions.extend(crate::ui::layout::render::render_layout(
                ctx,
                layout,
                state,
                available_rect,
            ));
            pending_actions = actions;
        });

        // Apply layout actions after the egui frame
        let mut detach_requests = Vec::new();
        for action in pending_actions {
            use crate::ui::layout::render::LayoutAction;
            match action {
                LayoutAction::Resize { node_id, new_ratio } => {
                    self.layout.resize(node_id, new_ratio);
                }
                LayoutAction::SwapType { node_id, new_type } => {
                    self.layout.swap_type(node_id, new_type);
                }
                LayoutAction::Close { node_id } => {
                    // remove_leaf finds the parent, replaces parent with sibling
                    self.layout.remove_leaf(node_id);
                }
                LayoutAction::Duplicate { node_id } => {
                    self.layout
                        .split(node_id, crate::ui::layout::SplitDirection::Vertical, 0.5);
                }
                LayoutAction::Split { node_id, direction } => {
                    // Determine the original panel type before splitting.
                    let original_type = self
                        .layout
                        .node(node_id)
                        .and_then(|n| n.panel_type());

                    self.layout.split(node_id, direction, 0.5);

                    // After split, the node_id is now a Split node whose second
                    // child is the new panel. Change it to a different type so
                    // the user doesn't get a duplicate.
                    if let Some(original) = original_type {
                        if let Some(crate::ui::layout::LayoutNode::Split { second, .. }) =
                            self.layout.node(node_id).cloned()
                        {
                            let new_type = Self::pick_new_panel_type(original);
                            self.layout.swap_type(second, new_type);
                        }
                    }
                }
                LayoutAction::Merge { node_id, keep } => {
                    self.layout.merge(node_id, keep);
                }
                LayoutAction::Detach { node_id } => {
                    if let Some((panel_type, panel_id)) = self.layout.remove_leaf(node_id) {
                        detach_requests.push(DetachRequest {
                            panel_type,
                            panel_id,
                        });
                    }
                }
                LayoutAction::SplitWithType {
                    node_id,
                    direction,
                    new_type,
                } => {
                    self.layout.split(node_id, direction, 0.5);
                    // After split, set the new (second) child to the requested type.
                    if let Some(crate::ui::layout::LayoutNode::Split { second, .. }) =
                        self.layout.node(node_id).cloned()
                    {
                        self.layout.swap_type(second, new_type);
                    }
                }
                LayoutAction::ResetLayout => {
                    self.layout = crate::ui::layout::LayoutTree::default_layout();
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

        // Pass 2: Preview texture (fullscreen, behind everything)
        {
            let mut preview_pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("preview_pass"),
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
            gpu.preview_renderer.render(&mut preview_pass);
        }

        // Pass 3: egui overlay
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

        Ok(detach_requests)
    }
}
