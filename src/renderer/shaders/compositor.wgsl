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
