// src/renderer/effect_pipeline.rs

//! GPU pipeline manager for per-source shader effects.
//!
//! Compiles effect WGSL shaders on demand, manages per-source ping-pong temp
//! textures, and runs multi-pass effect chains. Follows the same lazy-compile
//! pattern as `TransitionPipeline`.

use std::collections::HashMap;

use bytemuck::{Pod, Zeroable};
use egui_wgpu::wgpu;
use wgpu::Device;

use crate::effect_registry::EffectRegistry;
use crate::scene::SourceId;

/// Uniform buffer for effect shaders. 48 bytes, 16-byte aligned.
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct EffectUniforms {
    /// Elapsed time in seconds (for animated effects).
    pub time: f32,
    /// Alignment padding.
    pub _pad: f32,
    /// Input texture dimensions (width, height).
    pub resolution: [f32; 2],
    /// Named shader parameters (up to 8 floats).
    pub params: [f32; 8],
}

/// An effect to apply, as resolved by the compositor before rendering.
pub struct ResolvedEffect {
    /// Effect ID matching a key in `EffectRegistry`.
    pub effect_id: String,
    /// Parameter values to pass to the shader.
    pub params: [f32; 8],
}

/// Ping-pong texture pair for a single source's effect chain.
struct TempTextures {
    textures: [wgpu::Texture; 2],
    views: [wgpu::TextureView; 2],
    bind_groups: [wgpu::BindGroup; 2],
    size: (u32, u32),
}

/// Fullscreen-quad vertex shader prepended to every effect fragment shader.
const VERTEX_PREAMBLE: &str = r#"
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VertexOutput {
    var out: VertexOutput;
    let x = f32((vi & 1u) * 2u) - 1.0;
    let y = 1.0 - f32((vi >> 1u) * 2u);
    out.position = vec4<f32>(x, y, 0.0, 1.0);
    out.uv = vec2<f32>(f32(vi & 1u), f32(vi >> 1u));
    return out;
}
"#;

/// GPU pipeline that compiles effect shaders, manages ping-pong temp textures,
/// and runs per-source effect chains.
pub struct EffectPipeline {
    /// Lazy-compiled render pipelines keyed by effect ID.
    compiled: HashMap<String, wgpu::RenderPipeline>,
    pipeline_layout: wgpu::PipelineLayout,
    texture_bind_group_layout: wgpu::BindGroupLayout,
    #[allow(dead_code)]
    uniform_bind_group_layout: wgpu::BindGroupLayout,
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    sampler: wgpu::Sampler,
    target_format: wgpu::TextureFormat,
    /// Per-source ping-pong texture pairs.
    temp_textures: HashMap<SourceId, TempTextures>,
}

impl EffectPipeline {
    /// Create the effect pipeline with shared layouts, uniform buffer, and sampler.
    pub fn new(device: &Device, target_format: wgpu::TextureFormat) -> Self {
        // Texture + sampler bind group layout (group 0).
        let texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("effect_texture_bgl"),
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

        // Uniform bind group layout (group 1).
        let uniform_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("effect_uniform_bgl"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("effect_uniform_buffer"),
            size: std::mem::size_of::<EffectUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("effect_uniform_bind_group"),
            layout: &uniform_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("effect_pipeline_layout"),
            bind_group_layouts: &[&texture_bind_group_layout, &uniform_bind_group_layout],
            push_constant_ranges: &[],
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("effect_sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        Self {
            compiled: HashMap::new(),
            pipeline_layout,
            texture_bind_group_layout,
            uniform_bind_group_layout,
            uniform_buffer,
            uniform_bind_group,
            sampler,
            target_format,
            temp_textures: HashMap::new(),
        }
    }

    /// Compile a shader and store its pipeline. Returns true on success.
    ///
    /// The vertex preamble (fullscreen quad) is prepended automatically —
    /// effect shaders only need to define `fs_main`.
    pub fn compile_shader(&mut self, device: &Device, id: &str, wgsl_source: &str) -> bool {
        let full_source = format!("{VERTEX_PREAMBLE}\n{wgsl_source}");

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(&format!("effect_{id}_shader")),
            source: wgpu::ShaderSource::Wgsl(full_source.into()),
        });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(&format!("effect_{id}_pipeline")),
            layout: Some(&self.pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: self.target_format,
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

        self.compiled.insert(id.to_string(), render_pipeline);
        true
    }

    /// Get or lazily compile an effect pipeline by ID.
    ///
    /// Returns `None` if the effect is not in the registry or fails to compile.
    pub fn get_or_compile(
        &mut self,
        device: &Device,
        id: &str,
        registry: &EffectRegistry,
    ) -> Option<&wgpu::RenderPipeline> {
        if !self.compiled.contains_key(id) {
            if let Some(def) = registry.get(id) {
                if !def.shader_source.is_empty() {
                    let success = self.compile_shader(device, id, &def.shader_source);
                    if !success {
                        log::warn!("Failed to compile effect shader '{id}'");
                        return None;
                    }
                } else {
                    log::warn!("Effect '{id}' has empty shader source");
                    return None;
                }
            } else {
                log::warn!("Effect '{id}' not found in registry");
                return None;
            }
        }

        self.compiled.get(id)
    }

    /// Clear all compiled shader pipelines.
    /// Called when the effect registry is rescanned (user modified/added shaders).
    pub fn invalidate_user_shaders(&mut self) {
        self.compiled.clear();
    }

    /// Ensure a ping-pong texture pair exists for the given source at the given size.
    /// Creates or recreates if the size has changed.
    pub fn ensure_temp_textures(&mut self, device: &Device, source_id: SourceId, size: (u32, u32)) {
        let needs_create = match self.temp_textures.get(&source_id) {
            Some(tt) => tt.size != size,
            None => true,
        };

        if needs_create {
            let tt = self.create_temp_texture_pair(device, size);
            self.temp_textures.insert(source_id, tt);
        }
    }

    /// Create a bind group for an arbitrary texture view using this pipeline's layout.
    pub fn create_bind_group_for_view(
        &self,
        device: &Device,
        view: &wgpu::TextureView,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("effect_source_bind_group"),
            layout: &self.texture_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        })
    }

    /// Remove temp textures for a source that no longer exists.
    pub fn remove_temp_textures(&mut self, source_id: SourceId) {
        self.temp_textures.remove(&source_id);
    }

    /// Run an effect chain on a source texture, returning a reference to the
    /// bind group of the post-effect result texture.
    ///
    /// Returns `None` if there are no effects to apply (caller should use the
    /// original source bind group). Returns `Some(&BindGroup)` pointing to
    /// whichever ping-pong texture holds the final result.
    #[allow(clippy::too_many_arguments)]
    pub fn apply_chain(
        &mut self,
        device: &Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        source_id: SourceId,
        source_bind_group: &wgpu::BindGroup,
        source_size: (u32, u32),
        effects: &[ResolvedEffect],
        time: f32,
        registry: &EffectRegistry,
    ) -> Option<usize> {
        if effects.is_empty() {
            return None;
        }

        // Expand the effect list: "blur" becomes two passes (horizontal, vertical).
        // Use owned Strings so we don't borrow `effects` across the mutable phase.
        let mut passes: Vec<(String, [f32; 8])> = Vec::new();
        for effect in effects {
            if effect.effect_id == "blur" {
                let mut h_params = effect.params;
                h_params[7] = 0.0;
                passes.push((effect.effect_id.clone(), h_params));
                let mut v_params = effect.params;
                v_params[7] = 1.0;
                passes.push((effect.effect_id.clone(), v_params));
            } else {
                passes.push((effect.effect_id.clone(), effect.params));
            }
        }

        if passes.is_empty() {
            return None;
        }

        // Pre-compile all needed shaders (mutable borrow phase).
        for (effect_id, _) in &passes {
            self.get_or_compile(device, effect_id, registry);
        }

        // Ensure temp textures exist at the right size (mutable borrow phase).
        self.ensure_temp_textures(device, source_id, source_size);

        // Run each pass, ping-ponging between temp textures A (0) and B (1).
        // First pass reads from source_bind_group, writes to A.
        // Pass 2: read A, write B. Pass 3: read B, write A. Etc.
        let mut last_output_index: usize = 0;

        for (pass_idx, (effect_id, params)) in passes.iter().enumerate() {
            let pipeline = match self.compiled.get(effect_id.as_str()) {
                Some(p) => p,
                None => continue,
            };

            // Upload uniforms for this pass.
            let uniforms = EffectUniforms {
                time,
                _pad: 0.0,
                resolution: [source_size.0 as f32, source_size.1 as f32],
                params: *params,
            };
            queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));

            // Determine input bind group and output texture view.
            let write_index = if pass_idx % 2 == 0 { 0 } else { 1 };
            last_output_index = write_index;

            let tt = self.temp_textures.get(&source_id).expect(
                "temp textures must exist after ensure_temp_textures",
            );
            let target_view = &tt.views[write_index];

            let input_bind_group: &wgpu::BindGroup = if pass_idx == 0 {
                source_bind_group
            } else {
                let read_index = 1 - write_index;
                &tt.bind_groups[read_index]
            };

            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("effect_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target_view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, input_bind_group, &[]);
            pass.set_bind_group(1, &self.uniform_bind_group, &[]);
            pass.draw(0..4, 0..1);
        }

        Some(last_output_index)
    }

    /// After `apply_chain` returns `Some(index)`, call this to get the bind group
    /// for the resulting texture.
    pub fn result_bind_group(&self, source_id: SourceId, index: usize) -> Option<&wgpu::BindGroup> {
        self.temp_textures
            .get(&source_id)
            .map(|tt| &tt.bind_groups[index])
    }

    /// After `apply_chain` returns `Some(index)`, call this to get the texture view
    /// for the resulting texture. Used by the compositor to create a bind group with
    /// its own layout.
    pub fn result_texture_view(&self, source_id: SourceId, index: usize) -> Option<&wgpu::TextureView> {
        self.temp_textures
            .get(&source_id)
            .map(|tt| &tt.views[index])
    }

    // ---- Private helpers ---------------------------------------------------

    fn create_temp_texture_pair(&self, device: &Device, size: (u32, u32)) -> TempTextures {
        let desc = wgpu::TextureDescriptor {
            label: Some("effect_temp_texture"),
            size: wgpu::Extent3d {
                width: size.0,
                height: size.1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.target_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        };

        let tex_a = device.create_texture(&desc);
        let tex_b = device.create_texture(&desc);
        let view_a = tex_a.create_view(&Default::default());
        let view_b = tex_b.create_view(&Default::default());

        let bg_a = self.create_bind_group_for_view(device, &view_a);
        let bg_b = self.create_bind_group_for_view(device, &view_b);

        TempTextures {
            textures: [tex_a, tex_b],
            views: [view_a, view_b],
            bind_groups: [bg_a, bg_b],
            size,
        }
    }
}
