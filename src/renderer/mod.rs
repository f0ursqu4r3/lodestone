pub mod compositor;
pub mod pipelines;
pub mod secondary_canvas;
pub mod text;
pub mod transition;

use anyhow::Result;
// Use wgpu re-exported from egui_wgpu (wgpu 27) so that we can share
// Device/Queue/Surface with the egui renderer without version conflicts.
use egui_wgpu::wgpu;
use egui_wgpu::wgpu::{Device, Queue, TextureFormat};
use std::sync::Arc;
use winit::window::Window;

use compositor::Compositor;
use pipelines::WidgetPipeline;
use text::GlyphonRenderer;

// ---------------------------------------------------------------------------
// SharedGpuState — owns GPU device/queue and shared pipelines
// ---------------------------------------------------------------------------

pub struct SharedGpuState {
    pub instance: wgpu::Instance,
    pub device: Arc<Device>,
    pub queue: Arc<Queue>,
    pub format: TextureFormat,
    pub compositor: Compositor,
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

        // Default canvas size: 1920x1080 for both base and output resolution.
        // Task 4 will wire these to user settings.
        let compositor = Compositor::new(&device, format, (1920, 1080), (1920, 1080));

        Ok(Self {
            instance,
            device,
            queue,
            format,
            compositor,
            widget_pipeline,
            text_renderer,
        })
    }
}
