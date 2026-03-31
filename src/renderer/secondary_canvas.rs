// src/renderer/secondary_canvas.rs

use std::collections::HashMap;
use std::sync::Arc;

use egui_wgpu::wgpu;
use egui_wgpu::wgpu::Device;

use crate::gstreamer::RgbaFrame;
use crate::renderer::compositor::SourceLayer;
use crate::scene::SourceId;

/// On-demand second canvas for Studio Mode and transitions.
///
/// Owns its own texture, view, bind group, and per-source GPU layers so it can
/// be composited independently of the primary canvas.
pub struct SecondaryCanvas {
    /// The GPU texture backing this canvas.
    pub texture: wgpu::Texture,
    /// View into the canvas texture for use as a render target.
    pub view: wgpu::TextureView,
    /// Bind group for sampling this canvas in the transition pipeline or preview panel.
    pub bind_group: Arc<wgpu::BindGroup>,
    /// Per-source GPU resources for compositing onto this canvas.
    pub source_layers: HashMap<SourceId, SourceLayer>,
}

impl SecondaryCanvas {
    /// Allocate a secondary canvas matching the primary canvas dimensions.
    pub fn new(
        device: &Device,
        width: u32,
        height: u32,
        texture_bind_group_layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
    ) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("secondary_canvas"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("secondary_canvas_bind_group"),
            layout: texture_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        });

        Self {
            texture,
            view,
            bind_group: Arc::new(bind_group),
            source_layers: HashMap::new(),
        }
    }

    /// Upload a frame for a source on the secondary canvas.
    ///
    /// Creates or resizes the source layer GPU texture as needed.
    pub fn upload_frame(
        &mut self,
        device: &Device,
        queue: &wgpu::Queue,
        source_id: SourceId,
        frame: &RgbaFrame,
        texture_bind_group_layout: &wgpu::BindGroupLayout,
        uniform_bind_group_layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
    ) {
        let needs_create = match self.source_layers.get(&source_id) {
            None => true,
            Some(layer) => layer.size != (frame.width, frame.height),
        };

        if needs_create {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("secondary_source_texture"),
                size: wgpu::Extent3d {
                    width: frame.width,
                    height: frame.height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let texture_view = texture.create_view(&Default::default());

            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("secondary_source_bind_group"),
                layout: texture_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&texture_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(sampler),
                    },
                ],
            });

            let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("secondary_source_uniform_buffer"),
                size: 48, // SourceUniforms is 48 bytes
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });

            let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("secondary_source_uniform_bind_group"),
                layout: uniform_bind_group_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                }],
            });

            self.source_layers.insert(
                source_id,
                SourceLayer {
                    texture,
                    texture_view,
                    bind_group,
                    uniform_buffer,
                    uniform_bind_group,
                    size: (frame.width, frame.height),
                },
            );
        }

        let layer = self
            .source_layers
            .get(&source_id)
            .expect("layer just inserted or already existed");
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &layer.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &frame.data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(frame.width * 4),
                rows_per_image: Some(frame.height),
            },
            wgpu::Extent3d {
                width: frame.width,
                height: frame.height,
                depth_or_array_layers: 1,
            },
        );
    }
}
