pub mod pipelines;
pub mod preview;
pub mod text;

use anyhow::Result;
// Use wgpu re-exported from egui_wgpu (wgpu 27) so that we can share
// Device/Queue/Surface with the egui renderer without version conflicts.
use egui_wgpu::wgpu;
use egui_wgpu::wgpu::{Device, Queue, Surface, SurfaceConfiguration, TextureFormat};
use winit::window::Window;

use pipelines::{WidgetParams, WidgetPipeline};
use preview::PreviewRenderer;
use text::{GlyphonRenderer, TextSection};

use crate::obs::RgbaFrame;

pub struct Renderer {
    pub device: Device,
    pub queue: Queue,
    pub surface: Surface<'static>,
    pub surface_config: SurfaceConfiguration,
    #[allow(dead_code)]
    pub format: TextureFormat,
    egui_renderer: egui_wgpu::Renderer,
    text_renderer: GlyphonRenderer,
    widget_pipeline: WidgetPipeline,
    preview_renderer: PreviewRenderer,
}

impl Renderer {
    pub async fn new(window: &'static Window) -> Result<Self> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let surface = instance.create_surface(window)?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .map_err(|e| anyhow::anyhow!("no suitable GPU adapter found: {e}"))?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("lodestone_device"),
                ..Default::default()
            })
            .await?;
        let size = window.inner_size();
        let surface_caps = surface.get_capabilities(&adapter);
        let format = surface_caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(surface_caps.formats[0]);
        let surface_config = SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &surface_config);

        let egui_renderer =
            egui_wgpu::Renderer::new(&device, format, egui_wgpu::RendererOptions::default());

        let text_renderer = GlyphonRenderer::new();
        let widget_pipeline = WidgetPipeline::new(&device, format);

        // Default preview size: 1920x1080
        let preview_width: u32 = 1920;
        let preview_height: u32 = 1080;
        let preview_renderer = PreviewRenderer::new(&device, format, preview_width, preview_height);

        // Upload a solid dark gray test frame
        let test_frame = RgbaFrame {
            data: vec![30u8, 30, 30, 255]
                .into_iter()
                .cycle()
                .take((preview_width * preview_height * 4) as usize)
                .collect(),
            width: preview_width,
            height: preview_height,
        };
        preview_renderer.upload_frame(&queue, &test_frame);

        Ok(Self {
            device,
            queue,
            surface,
            surface_config,
            format,
            egui_renderer,
            text_renderer,
            widget_pipeline,
            preview_renderer,
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width > 0 && height > 0 {
            self.surface_config.width = width;
            self.surface_config.height = height;
            self.surface.configure(&self.device, &self.surface_config);
        }
    }

    pub fn render(&mut self) -> Result<()> {
        // Prepare test label for text rendering
        let test_label = TextSection {
            text: "Lodestone".to_string(),
            position: [20.0, 20.0],
            size: 24.0,
            color: [255, 255, 255, 255],
        };
        self.text_renderer.prepare(&[test_label])?;

        let output = self
            .surface
            .get_current_texture()
            .map_err(|e| anyhow::anyhow!("Failed to get surface texture: {e}"))?;
        let view = output.texture.create_view(&Default::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("render_encoder"),
            });
        {
            let _render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
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
            // Text rendering (currently a no-op stub)
            self.text_renderer.render()?;
        }
        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();
        Ok(())
    }

    /// Render the frame with egui overlay. Takes tessellated paint jobs and texture delta.
    pub fn render_with_egui(
        &mut self,
        paint_jobs: &[egui::epaint::ClippedPrimitive],
        textures_delta: &egui::TexturesDelta,
        pixels_per_point: f32,
    ) -> Result<()> {
        // Prepare test label for text rendering
        let test_label = TextSection {
            text: "Lodestone".to_string(),
            position: [20.0, 20.0],
            size: 24.0,
            color: [255, 255, 255, 255],
        };
        self.text_renderer.prepare(&[test_label])?;

        let output = self
            .surface
            .get_current_texture()
            .map_err(|e| anyhow::anyhow!("Failed to get surface texture: {e}"))?;
        let view = output.texture.create_view(&Default::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("egui_render_encoder"),
            });

        let screen_descriptor = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [self.surface_config.width, self.surface_config.height],
            pixels_per_point,
        };

        // Upload texture updates
        for (id, image_delta) in &textures_delta.set {
            self.egui_renderer
                .update_texture(&self.device, &self.queue, *id, image_delta);
        }

        // Update vertex/index buffers
        let user_cmd_bufs = self.egui_renderer.update_buffers(
            &self.device,
            &self.queue,
            &mut encoder,
            paint_jobs,
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
            self.preview_renderer.render(&mut preview_pass);
        }

        // Pass 3: SDF widget rendering (behind egui)
        {
            let mut widget_pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("widget_pass"),
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

            // Test widget: a dark semi-transparent panel
            let test_widget = WidgetParams {
                rect: [20.0, 20.0, 220.0, 400.0],
                color: [0.12, 0.12, 0.14, 0.85],
                border_color: [0.3, 0.3, 0.35, 0.5],
                corner_radius: 12.0,
                border_width: 1.0,
                shadow_offset: [4.0, 4.0],
                shadow_blur: 16.0,
                _pad0: [0.0; 3],
                shadow_color: [0.0, 0.0, 0.0, 0.4],
                viewport_size: [
                    self.surface_config.width as f32,
                    self.surface_config.height as f32,
                ],
                _pad1: [0.0, 0.0],
            };
            self.widget_pipeline.draw_widget(
                &mut widget_pass,
                &self.device,
                &self.queue,
                &test_widget,
            );
        }

        // Pass 4: egui overlay
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
                .render(&mut render_pass, paint_jobs, &screen_descriptor);
            // Text rendering on top of everything (currently a no-op stub)
            self.text_renderer.render()?;
        }

        // Free released textures
        for id in &textures_delta.free {
            self.egui_renderer.free_texture(id);
        }

        let mut cmds: Vec<wgpu::CommandBuffer> = user_cmd_bufs;
        cmds.push(encoder.finish());
        self.queue.submit(cmds);
        output.present();
        Ok(())
    }
}
