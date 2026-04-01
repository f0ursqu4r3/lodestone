// src/renderer/transition.rs

use std::collections::HashMap;

use bytemuck::{Pod, Zeroable};
use egui_wgpu::wgpu;
use wgpu::Device;

const TRANSITION_FADE_SHADER: &str = include_str!("shaders/transition_fade.wgsl");

/// Uniform buffer for transition shaders. 48 bytes, 16-byte aligned.
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct TransitionUniforms {
    pub progress: f32,
    pub time: f32,
    pub _pad: [f32; 2],
    pub color: [f32; 4],
    pub from_color: [f32; 4],
    pub to_color: [f32; 4],
}

/// GPU resources for the transition blend pass.
///
/// Supports multiple transition shaders via lazy compilation. All shaders share
/// the same bind group layout (from texture, to texture, uniforms) and pipeline
/// layout. Shader modules are compiled on first use.
pub struct TransitionPipeline {
    compiled: HashMap<String, wgpu::RenderPipeline>,
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    pipeline_layout: wgpu::PipelineLayout,
    target_format: wgpu::TextureFormat,
}

impl TransitionPipeline {
    pub fn new(
        device: &Device,
        texture_bind_group_layout: &wgpu::BindGroupLayout,
        target_format: wgpu::TextureFormat,
    ) -> Self {
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("transition_uniform_buffer"),
            size: std::mem::size_of::<TransitionUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let uniform_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("transition_uniform_bgl"),
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

        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("transition_uniform_bind_group"),
            layout: &uniform_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("transition_pipeline_layout"),
            bind_group_layouts: &[
                texture_bind_group_layout,  // group 0: from texture + sampler
                texture_bind_group_layout,  // group 1: to texture + sampler
                &uniform_bind_group_layout, // group 2: uniforms
            ],
            push_constant_ranges: &[],
        });

        let mut pipeline = Self {
            compiled: HashMap::new(),
            uniform_buffer,
            uniform_bind_group,
            pipeline_layout,
            target_format,
        };

        // Pre-compile the built-in fade shader so it's always available as fallback.
        pipeline.compile_shader(
            device,
            crate::transition::TRANSITION_FADE,
            TRANSITION_FADE_SHADER,
        );

        pipeline
    }

    /// Compile a shader and store its pipeline. Returns true on success.
    fn compile_shader(&mut self, device: &Device, id: &str, wgsl_source: &str) -> bool {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(&format!("transition_{id}_shader")),
            source: wgpu::ShaderSource::Wgsl(wgsl_source.into()),
        });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(&format!("transition_{id}_pipeline")),
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

    /// Get or lazily compile a transition pipeline by ID.
    /// Falls back to "fade" if the shader fails to compile or isn't found in the registry.
    pub fn get_or_compile(
        &mut self,
        device: &Device,
        id: &str,
        registry: &crate::transition_registry::TransitionRegistry,
    ) -> &wgpu::RenderPipeline {
        if !self.compiled.contains_key(id) {
            if let Some(def) = registry.get(id) {
                if !def.shader_source.is_empty() {
                    let success = self.compile_shader(device, id, &def.shader_source);
                    if !success {
                        log::warn!(
                            "Failed to compile transition shader '{id}', falling back to fade"
                        );
                    }
                }
            } else {
                log::warn!("Transition '{id}' not in registry, falling back to fade");
            }
        }

        self.compiled
            .get(id)
            .or_else(|| self.compiled.get(crate::transition::TRANSITION_FADE))
            .expect("fade pipeline must always be compiled")
    }

    /// Run the transition blend pass, writing the result to `target_view`.
    #[allow(clippy::too_many_arguments)]
    pub fn blend(
        &mut self,
        device: &Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        from_bind_group: &wgpu::BindGroup,
        to_bind_group: &wgpu::BindGroup,
        target_view: &wgpu::TextureView,
        transition_id: &str,
        progress: f32,
        time: f32,
        colors: &crate::transition::TransitionColors,
        registry: &crate::transition_registry::TransitionRegistry,
    ) {
        let uniforms = TransitionUniforms {
            progress,
            time,
            _pad: [0.0; 2],
            color: colors.color,
            from_color: colors.from_color,
            to_color: colors.to_color,
        };
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));

        // Get the pipeline (compile if needed, fallback to fade).
        let pipeline = self.get_or_compile(device, transition_id, registry);

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("transition_blend_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target_view,
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

        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, from_bind_group, &[]);
        pass.set_bind_group(1, to_bind_group, &[]);
        pass.set_bind_group(2, &self.uniform_bind_group, &[]);
        pass.draw(0..4, 0..1);
    }
}
