// SDF widget pipeline — renders rounded rectangles with borders, drop shadows,
// and fill colors using a signed distance field shader.
//
// TODO: Backdrop blur pipeline is deferred. Panels use solid semi-transparent
// backgrounds for now. A blur pipeline can be added later by sampling the
// previous frame's texture and applying a Gaussian or Kawase blur kernel.

use egui_wgpu::wgpu;

/// WGSL SDF shader for rounded rectangle rendering.
///
/// The vertex shader generates a fullscreen-ish quad from `vertex_index` (0..3
/// triangle strip) that covers the widget rect plus shadow padding.  The
/// fragment shader evaluates a rounded-rect SDF and composites shadow, fill,
/// and border layers with anti-aliased edges.
const SDF_SHADER_SRC: &str = r#"
struct WidgetParams {
    rect: vec4<f32>,          // x, y, width, height (pixels)
    color: vec4<f32>,         // fill RGBA
    border_color: vec4<f32>,
    corner_radius: f32,
    border_width: f32,
    shadow_offset: vec2<f32>,
    shadow_blur: f32,
    _pad0: vec3<f32>,
    shadow_color: vec4<f32>,
    viewport_size: vec2<f32>,
    _pad1: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> params: WidgetParams;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) pixel_pos: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    // Expand the quad to cover the widget rect + shadow padding.
    let pad = params.shadow_blur + max(abs(params.shadow_offset.x), abs(params.shadow_offset.y));
    let rect_min = vec2<f32>(params.rect.x - pad, params.rect.y - pad);
    let rect_max = vec2<f32>(params.rect.x + params.rect.z + pad, params.rect.y + params.rect.w + pad);

    // Triangle strip: 0=TL, 1=TR, 2=BL, 3=BR
    var pos: vec2<f32>;
    switch vertex_index {
        case 0u: { pos = vec2<f32>(rect_min.x, rect_min.y); }
        case 1u: { pos = vec2<f32>(rect_max.x, rect_min.y); }
        case 2u: { pos = vec2<f32>(rect_min.x, rect_max.y); }
        case 3u: { pos = vec2<f32>(rect_max.x, rect_max.y); }
        default: { pos = vec2<f32>(0.0, 0.0); }
    }

    var out: VertexOutput;
    // Convert pixel coordinates to clip space: [0, width] -> [-1, 1], [0, height] -> [1, -1]
    let ndc = vec2<f32>(
        (pos.x / params.viewport_size.x) * 2.0 - 1.0,
        1.0 - (pos.y / params.viewport_size.y) * 2.0,
    );
    out.clip_position = vec4<f32>(ndc, 0.0, 1.0);
    out.pixel_pos = pos;
    return out;
}

// Signed distance function for a rounded rectangle.
// `p` is the point relative to the rect center, `half_size` is half the rect
// dimensions, and `r` is the corner radius.
fn rounded_rect_sdf(p: vec2<f32>, half_size: vec2<f32>, r: f32) -> f32 {
    let q = abs(p) - half_size + vec2<f32>(r, r);
    return length(max(q, vec2<f32>(0.0, 0.0))) + min(max(q.x, q.y), 0.0) - r;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let rect_center = vec2<f32>(
        params.rect.x + params.rect.z * 0.5,
        params.rect.y + params.rect.w * 0.5,
    );
    let half_size = vec2<f32>(params.rect.z * 0.5, params.rect.w * 0.5);
    let r = params.corner_radius;

    // --- Shadow layer ---
    let shadow_p = in.pixel_pos - rect_center - params.shadow_offset;
    let shadow_dist = rounded_rect_sdf(shadow_p, half_size, r);
    // Smooth falloff for the shadow using the blur radius.
    let shadow_alpha = params.shadow_color.a * (1.0 - smoothstep(-params.shadow_blur, params.shadow_blur * 0.5, shadow_dist));
    var result = vec4<f32>(params.shadow_color.rgb, shadow_alpha);

    // --- Fill layer ---
    let fill_p = in.pixel_pos - rect_center;
    let fill_dist = rounded_rect_sdf(fill_p, half_size, r);
    let fill_alpha = params.color.a * (1.0 - smoothstep(-1.0, 1.0, fill_dist));
    // Alpha-blend fill on top of shadow.
    result = vec4<f32>(
        mix(result.rgb, params.color.rgb, fill_alpha),
        result.a * (1.0 - fill_alpha) + fill_alpha,
    );

    // --- Border layer ---
    let border_dist = abs(fill_dist) - params.border_width * 0.5;
    let border_alpha = params.border_color.a * (1.0 - smoothstep(-1.0, 1.0, border_dist));
    // Alpha-blend border on top.
    result = vec4<f32>(
        mix(result.rgb, params.border_color.rgb, border_alpha),
        result.a * (1.0 - border_alpha) + border_alpha,
    );

    // Discard fully transparent pixels.
    if result.a < 0.001 {
        discard;
    }

    return result;
}
"#;

/// Uniform data for a single SDF widget draw call.
///
/// Layout matches the WGSL `WidgetParams` struct under WGSL uniform address
/// space rules (equivalent to std140).  `vec3<f32>` in WGSL has alignment 16
/// and size 12, which means `_pad0` in the WGSL struct starts at offset 80
/// (not 68).  The bytes from offset 68 through 79 are implicit WGSL padding,
/// and bytes 92–95 are implicit padding before `shadow_color`.  Both gaps are
/// collapsed here into a single `_pad0: [f32; 7]` field (28 bytes) that
/// bridges from `shadow_blur` (ends at 68) to `shadow_color` (starts at 96).
///
/// | Offset | Field            | Size | Notes                          |
/// |--------|-----------------|------|--------------------------------|
/// |   0    | rect            |  16  | vec4, align 16                 |
/// |  16    | color           |  16  | vec4, align 16                 |
/// |  32    | border_color    |  16  | vec4, align 16                 |
/// |  48    | corner_radius   |   4  | f32,  align 4                  |
/// |  52    | border_width    |   4  | f32,  align 4                  |
/// |  56    | shadow_offset   |   8  | vec2, align 8                  |
/// |  64    | shadow_blur     |   4  | f32,  align 4                  |
/// |  68    | _pad0           |  28  | WGSL vec3 align-16 + tail pad  |
/// |  96    | shadow_color    |  16  | vec4, align 16                 |
/// | 112    | viewport_size   |   8  | vec2, align 8                  |
/// | 120    | _pad1           |   8  | pad struct to multiple of 16   |
/// | 128    | (total)         |      |                                |
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct WidgetParams {
    pub rect: [f32; 4],
    pub color: [f32; 4],
    pub border_color: [f32; 4],
    pub corner_radius: f32,
    pub border_width: f32,
    pub shadow_offset: [f32; 2],
    pub shadow_blur: f32,
    pub _pad0: [f32; 7],
    pub shadow_color: [f32; 4],
    pub viewport_size: [f32; 2],
    pub _pad1: [f32; 2],
}

pub struct WidgetPipeline {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    uniform_buffer: wgpu::Buffer,
}

impl WidgetPipeline {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("sdf_widget_shader"),
            source: wgpu::ShaderSource::Wgsl(SDF_SHADER_SRC.into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("widget_bind_group_layout"),
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

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("widget_pipeline_layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("sdf_widget_pipeline"),
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
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("widget_uniform_buffer"),
            size: std::mem::size_of::<WidgetParams>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            pipeline,
            bind_group_layout,
            uniform_buffer,
        }
    }

    /// Draw a single SDF widget (rounded rect with border and shadow).
    ///
    /// The render pass must already be active.  This method writes the params
    /// to the uniform buffer, creates a bind group, sets the pipeline, and
    /// issues a draw call for 4 vertices (triangle strip quad).
    pub fn draw_widget(
        &self,
        render_pass: &mut wgpu::RenderPass<'_>,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        params: &WidgetParams,
    ) {
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(params));

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("widget_bind_group"),
            layout: &self.bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: self.uniform_buffer.as_entire_binding(),
            }],
        });

        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &bind_group, &[]);
        render_pass.draw(0..4, 0..1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widget_params_size_alignment() {
        // The struct must be exactly 128 bytes to match the WGSL uniform struct.
        // wgpu validates this at runtime: buffer size must equal the shader's
        // expected uniform size.
        let size = std::mem::size_of::<WidgetParams>();
        assert_eq!(
            size,
            128,
            "WidgetParams size ({size}) must be exactly 128 bytes to match the WGSL struct"
        );
    }

    #[test]
    fn widget_params_default_values() {
        let params = WidgetParams {
            rect: [20.0, 20.0, 220.0, 400.0],
            color: [0.12, 0.12, 0.14, 0.85],
            border_color: [0.3, 0.3, 0.35, 0.5],
            corner_radius: 12.0,
            border_width: 1.0,
            shadow_offset: [4.0, 4.0],
            shadow_blur: 16.0,
            _pad0: [0.0; 7],
            shadow_color: [0.0, 0.0, 0.0, 0.4],
            viewport_size: [1280.0, 720.0],
            _pad1: [0.0, 0.0],
        };
        assert_eq!(params.rect[2], 220.0);
        assert_eq!(params.corner_radius, 12.0);
        assert_eq!(params.border_width, 1.0);
        assert_eq!(params.shadow_blur, 16.0);
    }

    #[test]
    fn widget_params_bytemuck_roundtrip() {
        let params = WidgetParams {
            rect: [10.0, 20.0, 300.0, 200.0],
            color: [1.0, 0.0, 0.0, 1.0],
            border_color: [0.0, 1.0, 0.0, 1.0],
            corner_radius: 8.0,
            border_width: 2.0,
            shadow_offset: [2.0, 2.0],
            shadow_blur: 8.0,
            _pad0: [0.0; 7],
            shadow_color: [0.0, 0.0, 0.0, 0.5],
            viewport_size: [1920.0, 1080.0],
            _pad1: [0.0, 0.0],
        };
        let bytes = bytemuck::bytes_of(&params);
        let restored: &WidgetParams = bytemuck::from_bytes(bytes);
        assert_eq!(restored.rect, params.rect);
        assert_eq!(restored.color, params.color);
        assert_eq!(restored.corner_radius, params.corner_radius);
        assert_eq!(restored.viewport_size, params.viewport_size);
    }
}
