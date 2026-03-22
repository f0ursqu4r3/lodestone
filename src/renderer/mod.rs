pub mod pipelines;
pub mod preview;
pub mod text;

use anyhow::Result;
// Use wgpu re-exported from egui_wgpu (wgpu 27) so that we can share
// Device/Queue/Surface with the egui renderer without version conflicts.
use egui_wgpu::wgpu;
use egui_wgpu::wgpu::{Device, Queue, TextureFormat};
use std::sync::Arc;
use winit::window::Window;

use pipelines::WidgetPipeline;
use preview::PreviewRenderer;
use text::GlyphonRenderer;

use crate::gstreamer::RgbaFrame;

// ---------------------------------------------------------------------------
// SharedGpuState — owns GPU device/queue and shared pipelines
// ---------------------------------------------------------------------------

pub struct SharedGpuState {
    pub instance: wgpu::Instance,
    pub device: Arc<Device>,
    pub queue: Arc<Queue>,
    pub format: TextureFormat,
    pub preview_renderer: PreviewRenderer,
    #[allow(dead_code)]
    pub widget_pipeline: WidgetPipeline,
    pub text_renderer: GlyphonRenderer,
}

impl SharedGpuState {
    /// Create shared GPU state. Needs an initial window to select an adapter
    /// compatible with the platform's surface type.
    pub async fn new(window: &Window) -> Result<Self> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());

        // We only need a temporary surface to pick the right adapter; it is
        // dropped immediately after capability query.
        let tmp_surface = instance.create_surface(window)?;

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&tmp_surface),
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

        let surface_caps = tmp_surface.get_capabilities(&adapter);
        let format = surface_caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(surface_caps.formats[0]);

        let device = Arc::new(device);
        let queue = Arc::new(queue);

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
            instance,
            device,
            queue,
            format,
            preview_renderer,
            widget_pipeline,
            text_renderer,
        })
    }
}
