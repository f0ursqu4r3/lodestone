// src/renderer/transition.rs

use bytemuck::{Pod, Zeroable};
use egui_wgpu::wgpu;
use wgpu::Device;

const TRANSITION_FADE_SHADER: &str = include_str!("shaders/transition_fade.wgsl");

/// Uniform buffer for transition shaders. 8 bytes padded to 16 for alignment.
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct TransitionUniforms {
    pub progress: f32,
    pub time: f32,
    pub _padding: [f32; 2],
}

/// GPU resources for the transition blend pass.
///
/// Takes two canvas textures (from + to) and blends them via a fullscreen quad
/// using a configurable shader. The result is written to whichever render target
/// the caller provides (the primary canvas view or the output texture view).
pub struct TransitionPipeline {
    pipeline: wgpu::RenderPipeline,
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
}

impl TransitionPipeline {
    pub fn new(
        device: &Device,
        texture_bind_group_layout: &wgpu::BindGroupLayout,
        target_format: wgpu::TextureFormat,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("transition_fade_shader"),
            source: wgpu::ShaderSource::Wgsl(TRANSITION_FADE_SHADER.into()),
        });

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

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("transition_pipeline"),
            layout: Some(&pipeline_layout),
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
                    format: target_format,
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
            pipeline,
            uniform_buffer,
            uniform_bind_group,
        }
    }

    /// Run the transition blend pass, writing the result to `target_view`.
    ///
    /// `from_bind_group` and `to_bind_group` are texture+sampler bind groups
    /// for the outgoing and incoming scene canvases.
    #[allow(clippy::too_many_arguments)]
    pub fn blend(
        &self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        from_bind_group: &wgpu::BindGroup,
        to_bind_group: &wgpu::BindGroup,
        target_view: &wgpu::TextureView,
        progress: f32,
        time: f32,
    ) {
        let uniforms = TransitionUniforms {
            progress,
            time,
            _padding: [0.0; 2],
        };
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));

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

        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, from_bind_group, &[]);
        pass.set_bind_group(1, to_bind_group, &[]);
        pass.set_bind_group(2, &self.uniform_bind_group, &[]);
        pass.draw(0..4, 0..1);
    }
}
