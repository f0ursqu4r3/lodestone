use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use egui_wgpu::wgpu;
use egui_wgpu::wgpu::{Device, Queue, TextureFormat};

use crate::gstreamer::RgbaFrame;
use crate::scene::{SourceId, Transform};

// ---------------------------------------------------------------------------
// ResolvedSource — render-ready source data after applying scene overrides
// ---------------------------------------------------------------------------

/// Resolved source data for rendering. Built from LibrarySource + SceneSource overrides.
///
/// Contains only the fields that the compositor needs to draw a source.
/// Device config, name, source_type etc. are irrelevant for rendering.
pub struct ResolvedSource {
    /// The source's unique identifier (used to look up the GPU layer).
    pub id: SourceId,
    /// Position and size on the canvas, in pixels.
    pub transform: Transform,
    /// Alpha opacity in \[0.0, 1.0\]. Clamped by the compositor.
    pub opacity: f32,
    /// Whether this source should be drawn at all.
    pub visible: bool,
}

// ---------------------------------------------------------------------------
// WGSL shaders
// ---------------------------------------------------------------------------

/// Compositor shader: positions a source quad based on a normalized rect uniform,
/// samples the source texture and applies per-source opacity.
///
/// group(0): source texture + sampler
/// group(1): SourceUniforms buffer
const COMPOSITOR_SHADER: &str = include_str!("shaders/compositor.wgsl");

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
/// Layout (48 bytes, std140-aligned to match WGSL):
/// - rect:     [f32; 4] — normalized x, y, w, h in 0..1 canvas space
/// - opacity:  f32
/// - _pad_align: [f32; 3] — implicit gap so `_padding` starts at offset 32 (vec3 alignment)
/// - _padding: [f32; 3] — matches shader `vec3<f32>` padding field
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct SourceUniforms {
    /// Normalized rect: [x, y, width, height] in 0..1 canvas space.
    pub rect: [f32; 4],
    /// Alpha opacity clamped to [0.0, 1.0].
    pub opacity: f32,
    /// Alignment gap: WGSL vec3<f32> requires 16-byte alignment, so 12 bytes of
    /// padding are needed after the f32 `opacity` field (offset 20 → 32).
    pub _pad_align: [f32; 3],
    pub _padding: [f32; 3],
    /// Struct-level alignment pad: WGSL struct size rounds up to 16-byte multiple (44 → 48).
    pub _pad_end: f32,
}

// ---------------------------------------------------------------------------
// SourceLayer — per-source GPU resources
// ---------------------------------------------------------------------------

/// GPU resources for a single composited source.
#[allow(dead_code)]
pub struct SourceLayer {
    pub texture: wgpu::Texture,
    pub texture_view: wgpu::TextureView,
    pub bind_group: wgpu::BindGroup,
    pub uniform_buffer: wgpu::Buffer,
    pub uniform_bind_group: wgpu::BindGroup,
    pub size: (u32, u32),
}

// ---------------------------------------------------------------------------
// Helper: parse a "WxH" resolution string
// ---------------------------------------------------------------------------

/// Parse a resolution string like `"1920x1080"` into `(width, height)`.
///
/// Returns `(1920, 1080)` if parsing fails.
pub fn parse_resolution(s: &str) -> (u32, u32) {
    let parts: Vec<&str> = s.split('x').collect();
    let w = parts.first().and_then(|s| s.parse().ok()).unwrap_or(1920);
    let h = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(1080);
    (w, h)
}

// ---------------------------------------------------------------------------
// Internal texture / buffer creation helpers
// ---------------------------------------------------------------------------

/// Canvas texture format used throughout the compositor.
const CANVAS_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

/// Create a texture suitable for use as a canvas or output target.
fn create_render_texture(device: &Device, label: &str, width: u32, height: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: CANVAS_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

/// State for an in-flight async readback.
struct ReadbackInflight {
    ready: Arc<AtomicBool>,
    width: u32,
    height: u32,
}

/// Create a readback buffer sized for the given dimensions (with 256-byte row alignment).
fn create_readback_buffer(device: &Device, width: u32, height: u32) -> wgpu::Buffer {
    let bytes_per_row_padded = ((width * 4) + 255) & !255;
    let readback_size = (bytes_per_row_padded * height) as u64;
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("compositor_readback_buffer"),
        size: readback_size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    })
}

/// Create a texture+sampler bind group using the shared texture bind group layout.
fn create_texture_bind_group(
    device: &Device,
    label: &str,
    layout: &wgpu::BindGroupLayout,
    view: &wgpu::TextureView,
    sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(label),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    })
}

// ---------------------------------------------------------------------------
// Compositor
// ---------------------------------------------------------------------------

/// GPU compositor: blends multiple source layers onto a canvas texture.
pub struct Compositor {
    canvas_texture: wgpu::Texture,
    canvas_view: wgpu::TextureView,
    /// Base (canvas) resolution width in pixels.
    pub canvas_width: u32,
    /// Base (canvas) resolution height in pixels.
    pub canvas_height: u32,

    /// Output (scaled) resolution width in pixels.
    pub output_width: u32,
    /// Output (scaled) resolution height in pixels.
    pub output_height: u32,

    /// Output texture at `output_res` dimensions. Used for the scale pass and readback.
    output_texture: wgpu::Texture,
    output_texture_view: wgpu::TextureView,
    /// Bind group for the scale pass: binds `canvas_texture_view` + sampler.
    output_bind_group: wgpu::BindGroup,

    /// Compositor render pipeline (source → canvas).
    pipeline: wgpu::RenderPipeline,
    sampler: wgpu::Sampler,

    /// Scale pass pipeline: fullscreen quad sampling canvas → output at Rgba8UnormSrgb.
    scale_pipeline: wgpu::RenderPipeline,

    /// Layout for per-source uniform bind groups (group 1).
    uniform_bind_group_layout: wgpu::BindGroupLayout,

    /// Layout for source texture bind groups (group 0).
    texture_bind_group_layout: wgpu::BindGroupLayout,

    /// Per-source GPU layer, keyed by SourceId.
    source_layers: HashMap<SourceId, SourceLayer>,

    /// Readback buffer for CPU-side frame access (sized for output resolution).
    readback_buffer: wgpu::Buffer,

    /// Async readback state: dimensions and completion flag for the in-flight map.
    readback_inflight: Option<ReadbackInflight>,

    /// Sampler for the preview panel: nearest-neighbor magnification for pixel-sharp
    /// display when zoomed past 1:1, linear minification for smooth zoom-out.
    preview_sampler: wgpu::Sampler,

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
    ///
    /// `base_res` is the internal canvas resolution (where sources are composited).
    /// `output_res` is the final output resolution (for readback / encoding).
    /// When they differ, a scale pass blits the canvas to the output texture.
    pub fn new(
        device: &Device,
        surface_format: TextureFormat,
        base_res: (u32, u32),
        output_res: (u32, u32),
    ) -> Self {
        let (canvas_width, canvas_height) = base_res;
        let (output_width, output_height) = output_res;

        // ---- Canvas texture ------------------------------------------------
        let canvas_texture =
            create_render_texture(device, "compositor_canvas", canvas_width, canvas_height);
        let canvas_view = canvas_texture.create_view(&Default::default());

        // ---- Output texture ------------------------------------------------
        let output_texture =
            create_render_texture(device, "compositor_output", output_width, output_height);
        let output_texture_view = output_texture.create_view(&Default::default());

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
                    format: CANVAS_FORMAT,
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

        // ---- Readback buffer (sized for output resolution) -----------------
        let readback_buffer = create_readback_buffer(device, output_width, output_height);

        // ---- Canvas preview shader -----------------------------------------
        let canvas_preview_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("canvas_preview_shader"),
            source: wgpu::ShaderSource::Wgsl(CANVAS_PREVIEW_SHADER.into()),
        });

        let canvas_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("canvas_preview_pipeline_layout"),
                bind_group_layouts: &[&texture_bind_group_layout],
                push_constant_ranges: &[],
            });

        // ---- Canvas preview pipeline (targets surface_format for egui panel)
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

        // ---- Scale pass pipeline (targets Rgba8UnormSrgb for output texture)
        // This is a separate pipeline because the preview pipeline targets
        // surface_format which may differ from Rgba8UnormSrgb.
        let scale_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("compositor_scale_pipeline"),
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
                    format: CANVAS_FORMAT,
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

        // ---- Preview sampler (nearest mag for pixel-sharp zoom-in) ----------
        let preview_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("compositor_preview_sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        // ---- Bind groups ---------------------------------------------------
        let canvas_bind_group = create_texture_bind_group(
            device,
            "canvas_preview_bind_group",
            &texture_bind_group_layout,
            &canvas_view,
            &preview_sampler,
        );

        // Scale pass bind group: samples canvas_texture → output_texture.
        let output_bind_group = create_texture_bind_group(
            device,
            "compositor_output_bind_group",
            &texture_bind_group_layout,
            &canvas_view,
            &sampler,
        );

        Self {
            canvas_texture,
            canvas_view,
            canvas_width,
            canvas_height,
            output_width,
            output_height,
            output_texture,
            output_texture_view,
            output_bind_group,
            pipeline,
            sampler,
            scale_pipeline,
            uniform_bind_group_layout,
            texture_bind_group_layout,
            source_layers: HashMap::new(),
            readback_buffer,
            readback_inflight: None,
            preview_sampler,
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
                format: CANVAS_FORMAT,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let texture_view = texture.create_view(&Default::default());
            let bind_group = create_texture_bind_group(
                device,
                "compositor_source_bg",
                &self.texture_bind_group_layout,
                &texture_view,
                &self.sampler,
            );
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
    #[allow(dead_code)]
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
        sources: &[ResolvedSource],
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
                _pad_align: [0.0; 3],
                _padding: [0.0; 3],
                _pad_end: 0.0,
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

    /// Scale the canvas texture to the output texture via a fullscreen quad blit.
    ///
    /// This is a no-op when base and output resolutions are identical.
    pub fn scale_to_output(&self, encoder: &mut wgpu::CommandEncoder) {
        // Skip if resolutions match — readback will use canvas_texture directly.
        if self.canvas_width == self.output_width && self.canvas_height == self.output_height {
            return;
        }

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("compositor_scale_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &self.output_texture_view,
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

        pass.set_pipeline(&self.scale_pipeline);
        pass.set_bind_group(0, &self.output_bind_group, &[]);
        pass.draw(0..4, 0..1);
    }

    /// Begin an async GPU readback of the composited output.
    ///
    /// Submits a texture-to-buffer copy and starts an async buffer map.
    /// Call [`try_finish_readback`] on a subsequent frame to collect the result
    /// without blocking the render loop.
    pub fn start_readback(&mut self, device: &Device, queue: &Queue) {
        // If a previous readback is still in flight, skip — we'll collect it
        // next frame and start a fresh one after that.
        if self.readback_inflight.is_some() {
            return;
        }

        let (read_texture, read_width, read_height) =
            if self.canvas_width == self.output_width && self.canvas_height == self.output_height {
                (&self.canvas_texture, self.canvas_width, self.canvas_height)
            } else {
                (&self.output_texture, self.output_width, self.output_height)
            };

        let bytes_per_row_padded = ((read_width * 4) + 255) & !255;

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("compositor_readback_encoder"),
        });

        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: read_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &self.readback_buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(bytes_per_row_padded),
                    rows_per_image: Some(read_height),
                },
            },
            wgpu::Extent3d {
                width: read_width,
                height: read_height,
                depth_or_array_layers: 1,
            },
        );

        queue.submit(std::iter::once(encoder.finish()));

        let ready = Arc::new(AtomicBool::new(false));
        let ready_clone = Arc::clone(&ready);

        let slice = self.readback_buffer.slice(..);
        slice.map_async(wgpu::MapMode::Read, move |_| {
            ready_clone.store(true, Ordering::Release);
        });

        self.readback_inflight = Some(ReadbackInflight {
            ready,
            width: read_width,
            height: read_height,
        });
    }

    /// Try to collect a previously started async readback.
    ///
    /// Returns `Some(frame)` if the GPU copy has completed, `None` if it is
    /// still in flight (or no readback was started). A non-blocking
    /// `device.poll` drives the map callback without stalling the render loop.
    pub fn try_finish_readback(&mut self, device: &Device) -> Option<RgbaFrame> {
        let inflight = self.readback_inflight.as_ref()?;

        // Drive wgpu callbacks without blocking.
        let _ = device.poll(wgpu::PollType::Poll);

        if !inflight.ready.load(Ordering::Acquire) {
            return None;
        }

        let inflight = self.readback_inflight.take().unwrap();
        let read_width = inflight.width;
        let read_height = inflight.height;
        let bytes_per_row_padded = ((read_width * 4) + 255) & !255;

        let slice = self.readback_buffer.slice(..);
        let raw = slice.get_mapped_range();
        let unpadded_bytes_per_row = (read_width * 4) as usize;
        let padded = bytes_per_row_padded as usize;
        let mut data = Vec::with_capacity(unpadded_bytes_per_row * read_height as usize);
        for row in 0..read_height as usize {
            let start = row * padded;
            data.extend_from_slice(&raw[start..start + unpadded_bytes_per_row]);
        }
        drop(raw);
        self.readback_buffer.unmap();

        Some(RgbaFrame {
            data,
            width: read_width,
            height: read_height,
        })
    }

    /// Recreate GPU resources after a resolution change.
    ///
    /// - If `base_res` changed: recreates canvas texture, canvas bind group, output bind group.
    /// - If `output_res` changed: recreates output texture, readback buffer, output bind group.
    pub fn resize(&mut self, device: &Device, base_res: (u32, u32), output_res: (u32, u32)) {
        let (new_cw, new_ch) = base_res;
        let (new_ow, new_oh) = output_res;

        let canvas_changed = new_cw != self.canvas_width || new_ch != self.canvas_height;
        let output_changed = new_ow != self.output_width || new_oh != self.output_height;

        if !canvas_changed && !output_changed {
            return;
        }

        if canvas_changed {
            self.canvas_width = new_cw;
            self.canvas_height = new_ch;

            self.canvas_texture =
                create_render_texture(device, "compositor_canvas", new_cw, new_ch);
            self.canvas_view = self.canvas_texture.create_view(&Default::default());

            // Recreate the preview bind group (points at canvas_view, uses preview sampler).
            let canvas_bind_group = create_texture_bind_group(
                device,
                "canvas_preview_bind_group",
                &self.texture_bind_group_layout,
                &self.canvas_view,
                &self.preview_sampler,
            );
            self.canvas_bind_group = Arc::new(canvas_bind_group);
        }

        if output_changed {
            self.output_width = new_ow;
            self.output_height = new_oh;

            self.output_texture =
                create_render_texture(device, "compositor_output", new_ow, new_oh);
            self.output_texture_view = self.output_texture.create_view(&Default::default());

            self.readback_buffer = create_readback_buffer(device, new_ow, new_oh);
        }

        // Output bind group always references canvas_view, so recreate if either changed.
        if canvas_changed || output_changed {
            self.output_bind_group = create_texture_bind_group(
                device,
                "compositor_output_bind_group",
                &self.texture_bind_group_layout,
                &self.canvas_view,
                &self.sampler,
            );
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
    fn source_uniforms_is_48_bytes() {
        assert_eq!(size_of::<SourceUniforms>(), 48);
    }

    /// Verify that the coordinate normalization math is correct.
    ///
    /// A source at pixel (480, 270) with size 960x540 on a 1920x1080 canvas
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
            _pad_align: [0.0; 3],
            _padding: [0.0; 3],
            _pad_end: 0.0,
        };
        let bytes = bytemuck::bytes_of(&u);
        assert_eq!(bytes.len(), 48);
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

    #[test]
    fn parse_resolution_valid() {
        assert_eq!(parse_resolution("1920x1080"), (1920, 1080));
        assert_eq!(parse_resolution("1280x720"), (1280, 720));
        assert_eq!(parse_resolution("3840x2160"), (3840, 2160));
    }

    #[test]
    fn parse_resolution_fallback() {
        assert_eq!(parse_resolution(""), (1920, 1080));
        assert_eq!(parse_resolution("badxinput"), (1920, 1080));
        assert_eq!(parse_resolution("1280x"), (1280, 1080));
        assert_eq!(parse_resolution("x720"), (1920, 720));
    }
}
