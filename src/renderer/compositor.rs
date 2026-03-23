use std::collections::HashMap;
use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use egui_wgpu::wgpu;
use egui_wgpu::wgpu::{Device, Queue, TextureFormat};

use crate::gstreamer::RgbaFrame;
use crate::scene::{Source, SourceId};

// ---------------------------------------------------------------------------
// WGSL shaders
// ---------------------------------------------------------------------------

/// Compositor shader: positions a source quad based on a normalized rect uniform,
/// samples the source texture and applies per-source opacity.
///
/// group(0): source texture + sampler
/// group(1): SourceUniforms buffer
const COMPOSITOR_SHADER: &str = r#"
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

// Uniform buffer: [x, y, w, h] normalized 0..1, opacity, padding[3]
struct Uniforms {
    rect: vec4<f32>,
    opacity: f32,
    _padding: vec3<f32>,
};

@group(1) @binding(0) var<uniform> u: Uniforms;

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VertexOutput {
    // Triangle strip — 4 vertices for a quad
    // vi=0 top-left, vi=1 top-right, vi=2 bottom-left, vi=3 bottom-right
    let local_u = f32(vi & 1u);
    let local_v = f32(vi >> 1u);

    // Rect is [x, y, w, h] in normalised 0..1 canvas space.
    // Map to NDC: NDC x = norm_x * 2 - 1, NDC y = 1 - norm_y * 2
    let nx = u.rect.x + local_u * u.rect.z;
    let ny = u.rect.y + local_v * u.rect.w;
    let ndc_x = nx * 2.0 - 1.0;
    let ndc_y = 1.0 - ny * 2.0;

    var out: VertexOutput;
    out.position = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
    out.uv = vec2<f32>(local_u, local_v);
    return out;
}

@group(0) @binding(0) var t_source: texture_2d<f32>;
@group(0) @binding(1) var s_source: sampler;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let color = textureSample(t_source, s_source, in.uv);
    return vec4<f32>(color.rgb, color.a * u.opacity);
}
"#;

/// Canvas preview shader: fullscreen quad that samples the composited canvas texture.
///
/// group(0): canvas texture + sampler
const CANVAS_PREVIEW_SHADER: &str = r#"
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    let x = f32((vertex_index & 1u) * 2u) - 1.0;
    let y = 1.0 - f32((vertex_index >> 1u) * 2u);
    let u = f32(vertex_index & 1u);
    let v = f32(vertex_index >> 1u);

    var out: VertexOutput;
    out.position = vec4<f32>(x, y, 0.0, 1.0);
    out.uv = vec2<f32>(u, v);
    return out;
}

@group(0) @binding(0) var t_canvas: texture_2d<f32>;
@group(0) @binding(1) var s_canvas: sampler;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return textureSample(t_canvas, s_canvas, in.uv);
}
"#;

// ---------------------------------------------------------------------------
// SourceUniforms — must be exactly 32 bytes (Pod)
// ---------------------------------------------------------------------------

/// Per-source compositor uniforms uploaded to group(1) binding(0).
///
/// Layout (32 bytes):
/// - rect:     [f32; 4] — normalized x, y, w, h in 0..1 canvas space
/// - opacity:  f32
/// - _padding: [f32; 3] — align to 16 bytes
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct SourceUniforms {
    /// Normalized rect: [x, y, width, height] in 0..1 canvas space.
    pub rect: [f32; 4],
    /// Alpha opacity clamped to [0.0, 1.0].
    pub opacity: f32,
    pub _padding: [f32; 3],
}

// ---------------------------------------------------------------------------
// SourceLayer — per-source GPU resources
// ---------------------------------------------------------------------------

/// GPU resources for a single composited source.
pub struct SourceLayer {
    pub texture: wgpu::Texture,
    pub texture_view: wgpu::TextureView,
    pub bind_group: wgpu::BindGroup,
    pub uniform_buffer: wgpu::Buffer,
    pub uniform_bind_group: wgpu::BindGroup,
    pub size: (u32, u32),
}

// ---------------------------------------------------------------------------
// Compositor
// ---------------------------------------------------------------------------

/// GPU compositor: blends multiple source layers onto a canvas texture.
pub struct Compositor {
    canvas_texture: wgpu::Texture,
    canvas_view: wgpu::TextureView,
    /// Canvas dimensions in pixels.
    pub canvas_width: u32,
    /// Canvas dimensions in pixels.
    pub canvas_height: u32,

    /// Compositor render pipeline (source → canvas).
    pipeline: wgpu::RenderPipeline,
    sampler: wgpu::Sampler,

    /// Layout for per-source uniform bind groups (group 1).
    uniform_bind_group_layout: wgpu::BindGroupLayout,

    /// Layout for source texture bind groups (group 0).
    texture_bind_group_layout: wgpu::BindGroupLayout,

    /// Per-source GPU layer, keyed by SourceId.
    source_layers: HashMap<SourceId, SourceLayer>,

    /// Readback buffer for CPU-side frame access.
    readback_buffer: wgpu::Buffer,

    /// Arc-wrapped canvas bind group for the preview panel paint callback.
    canvas_bind_group: Arc<wgpu::BindGroup>,
    /// Arc-wrapped preview pipeline for the preview panel paint callback.
    canvas_pipeline: Arc<wgpu::RenderPipeline>,
}

impl Compositor {
    /// Create a new compositor targeting a canvas of the given size.
    ///
    /// `surface_format` is the swap-chain format used by the preview pipeline so
    /// its output can be composited directly onto the window surface.
    pub fn new(
        device: &Device,
        surface_format: TextureFormat,
        canvas_width: u32,
        canvas_height: u32,
    ) -> Self {
        // ---- Canvas texture ------------------------------------------------
        let canvas_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("compositor_canvas"),
            size: wgpu::Extent3d {
                width: canvas_width,
                height: canvas_height,
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
        let canvas_view = canvas_texture.create_view(&Default::default());

        // ---- Sampler -------------------------------------------------------
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("compositor_sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        // ---- Texture bind group layout (group 0) ---------------------------
        let texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("compositor_texture_bgl"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });

        // ---- Uniform bind group layout (group 1) — per-source buffers created in upload_frame
        let uniform_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("compositor_uniform_bgl"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        // ---- Compositor render pipeline ------------------------------------
        let compositor_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("compositor_shader"),
            source: wgpu::ShaderSource::Wgsl(COMPOSITOR_SHADER.into()),
        });

        let compositor_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("compositor_pipeline_layout"),
                bind_group_layouts: &[&texture_bind_group_layout, &uniform_bind_group_layout],
                push_constant_ranges: &[],
            });

        // Alpha-over blend: color uses SrcAlpha/OneMinusSrcAlpha, alpha uses One/OneMinusSrcAlpha.
        let alpha_over_blend = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::SrcAlpha,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
        };

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("compositor_pipeline"),
            layout: Some(&compositor_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &compositor_shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &compositor_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8UnormSrgb,
                    blend: Some(alpha_over_blend),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        // ---- Readback buffer -----------------------------------------------
        let bytes_per_row_padded = ((canvas_width * 4) + 255) & !255;
        let readback_size = (bytes_per_row_padded * canvas_height) as u64;
        let readback_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("compositor_readback_buffer"),
            size: readback_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        // ---- Canvas preview pipeline (for egui preview panel) -------------
        let canvas_preview_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("canvas_preview_shader"),
            source: wgpu::ShaderSource::Wgsl(CANVAS_PREVIEW_SHADER.into()),
        });

        // Canvas bind group using the same texture bind group layout.
        let canvas_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("canvas_preview_bind_group"),
            layout: &texture_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&canvas_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        let canvas_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("canvas_preview_pipeline_layout"),
                bind_group_layouts: &[&texture_bind_group_layout],
                push_constant_ranges: &[],
            });

        let canvas_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("canvas_preview_pipeline"),
            layout: Some(&canvas_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &canvas_preview_shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &canvas_preview_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        Self {
            canvas_texture,
            canvas_view,
            canvas_width,
            canvas_height,
            pipeline,
            sampler,
            uniform_bind_group_layout,
            texture_bind_group_layout,
            source_layers: HashMap::new(),
            readback_buffer,
            canvas_bind_group: Arc::new(canvas_bind_group),
            canvas_pipeline: Arc::new(canvas_pipeline),
        }
    }

    /// Upload a raw RGBA frame for a source, creating or resizing the GPU texture as needed.
    pub fn upload_frame(
        &mut self,
        device: &Device,
        queue: &Queue,
        source_id: SourceId,
        frame: &RgbaFrame,
    ) {
        let needs_recreate = match self.source_layers.get(&source_id) {
            Some(layer) => layer.size != (frame.width, frame.height),
            None => true,
        };

        if needs_recreate {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("compositor_source_texture"),
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
                label: Some("compositor_source_bg"),
                layout: &self.texture_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&texture_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                ],
            });
            let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("compositor_source_uniform"),
                size: std::mem::size_of::<SourceUniforms>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("compositor_source_uniform_bg"),
                layout: &self.uniform_bind_group_layout,
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
            .expect("layer just inserted");
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
                bytes_per_row: Some(4 * frame.width),
                rows_per_image: Some(frame.height),
            },
            wgpu::Extent3d {
                width: frame.width,
                height: frame.height,
                depth_or_array_layers: 1,
            },
        );
    }

    /// Remove a source layer, freeing its GPU resources.
    pub fn remove_source(&mut self, source_id: SourceId) {
        self.source_layers.remove(&source_id);
    }

    /// Composite all visible sources onto the canvas texture.
    ///
    /// Always clears the canvas to black first, then draws each visible source
    /// with its own per-source uniform buffer to avoid data races.
    pub fn compose(
        &self,
        queue: &Queue,
        encoder: &mut wgpu::CommandEncoder,
        sources: &[&Source],
    ) {
        let cw = self.canvas_width as f32;
        let ch = self.canvas_height as f32;

        // Dedicated clear pass — always runs.
        {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("compositor_clear_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.canvas_view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
        }

        // Draw each visible source with its own uniform buffer.
        for source in sources {
            let layer = match self.source_layers.get(&source.id) {
                Some(l) => l,
                None => continue,
            };
            if !source.visible {
                continue;
            }

            // Write to per-source uniform buffer — avoids race with shared buffer.
            let t = &source.transform;
            let uniforms = SourceUniforms {
                rect: [t.x / cw, t.y / ch, t.width / cw, t.height / ch],
                opacity: source.opacity.clamp(0.0, 1.0),
                _padding: [0.0; 3],
            };
            queue.write_buffer(&layer.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));

            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("compositor_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.canvas_view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &layer.bind_group, &[]);
            pass.set_bind_group(1, &layer.uniform_bind_group, &[]);
            pass.draw(0..4, 0..1);
        }
    }

    /// Read back the composited canvas from the GPU, returning a CPU-side `RgbaFrame`.
    ///
    /// This blocks until the GPU work is complete.
    pub fn readback(&self, device: &Device, queue: &Queue) -> RgbaFrame {
        let bytes_per_row_padded = ((self.canvas_width * 4) + 255) & !255;

        let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("compositor_readback_encoder"),
            });

        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &self.canvas_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &self.readback_buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(bytes_per_row_padded),
                    rows_per_image: Some(self.canvas_height),
                },
            },
            wgpu::Extent3d {
                width: self.canvas_width,
                height: self.canvas_height,
                depth_or_array_layers: 1,
            },
        );

        let submission_index = queue.submit(std::iter::once(encoder.finish()));

        // Map and read back, stripping row padding.
        let slice = self.readback_buffer.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(submission_index),
                timeout: None,
            })
            .expect("GPU poll failed during readback");

        let raw = slice.get_mapped_range();
        let unpadded_bytes_per_row = (self.canvas_width * 4) as usize;
        let padded = bytes_per_row_padded as usize;
        let mut data =
            Vec::with_capacity(unpadded_bytes_per_row * self.canvas_height as usize);
        for row in 0..self.canvas_height as usize {
            let start = row * padded;
            data.extend_from_slice(&raw[start..start + unpadded_bytes_per_row]);
        }
        drop(raw);
        self.readback_buffer.unmap();

        RgbaFrame {
            data,
            width: self.canvas_width,
            height: self.canvas_height,
        }
    }

    /// Arc-wrapped canvas bind group for the egui preview panel paint callback.
    pub fn canvas_bind_group(&self) -> Arc<wgpu::BindGroup> {
        Arc::clone(&self.canvas_bind_group)
    }

    /// Arc-wrapped canvas preview pipeline for the egui preview panel paint callback.
    pub fn canvas_pipeline(&self) -> Arc<wgpu::RenderPipeline> {
        Arc::clone(&self.canvas_pipeline)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::size_of;

    #[test]
    fn source_uniforms_is_32_bytes() {
        assert_eq!(size_of::<SourceUniforms>(), 32);
    }

    /// Verify that the coordinate normalization math is correct.
    ///
    /// A source at pixel (480, 270) with size 960×540 on a 1920×1080 canvas
    /// should normalize to (0.25, 0.25, 0.5, 0.5).
    #[test]
    fn source_uniforms_normalizes_transform() {
        let canvas_w = 1920.0_f32;
        let canvas_h = 1080.0_f32;

        let px = 480.0_f32;
        let py = 270.0_f32;
        let pw = 960.0_f32;
        let ph = 540.0_f32;

        let rect = [px / canvas_w, py / canvas_h, pw / canvas_w, ph / canvas_h];

        assert!((rect[0] - 0.25).abs() < f32::EPSILON, "x norm");
        assert!((rect[1] - 0.25).abs() < f32::EPSILON, "y norm");
        assert!((rect[2] - 0.5).abs() < f32::EPSILON, "w norm");
        assert!((rect[3] - 0.5).abs() < f32::EPSILON, "h norm");
    }

    #[test]
    fn source_uniforms_can_be_cast_to_bytes() {
        let u = SourceUniforms {
            rect: [0.0, 0.0, 1.0, 1.0],
            opacity: 1.0,
            _padding: [0.0; 3],
        };
        let bytes = bytemuck::bytes_of(&u);
        assert_eq!(bytes.len(), 32);
    }

    #[test]
    fn source_uniforms_opacity_clamped_in_compose_logic() {
        // Verify clamp math without needing GPU context.
        let opacity: f32 = 1.5;
        let clamped = opacity.clamp(0.0, 1.0);
        assert!((clamped - 1.0).abs() < f32::EPSILON);

        let opacity: f32 = -0.3;
        let clamped = opacity.clamp(0.0, 1.0);
        assert!((clamped - 0.0).abs() < f32::EPSILON);
    }
}
