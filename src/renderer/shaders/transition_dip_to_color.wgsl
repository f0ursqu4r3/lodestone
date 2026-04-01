// @name: Dip to Color
// @author: Lodestone
// @description: Fades to a solid color at the midpoint, then reveals the incoming scene
// @params: color

struct TransitionUniforms {
    progress: f32,
    time: f32,
    _pad0: f32,
    _pad1: f32,
    color: vec4<f32>,
    from_color: vec4<f32>,
    to_color: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@group(0) @binding(0) var t_from: texture_2d<f32>;
@group(0) @binding(1) var s_from: sampler;

@group(1) @binding(0) var t_to: texture_2d<f32>;
@group(1) @binding(1) var s_to: sampler;

@group(2) @binding(0) var<uniform> uniforms: TransitionUniforms;

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VertexOutput {
    let x = f32((vi & 1u) * 2u) - 1.0;
    let y = 1.0 - f32((vi >> 1u) * 2u);
    let u = f32(vi & 1u);
    let v = f32(vi >> 1u);

    var out: VertexOutput;
    out.position = vec4<f32>(x, y, 0.0, 1.0);
    out.uv = vec2<f32>(u, v);
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let src = textureSample(t_from, s_from, in.uv);
    let dst = textureSample(t_to, s_to, in.uv);
    let p = uniforms.progress;

    // First half: fade from scene → solid color
    // Second half: fade from solid color → to scene
    if p < 0.5 {
        let t = p * 2.0; // 0..1 over first half
        return mix(src, uniforms.color, t);
    } else {
        let t = (p - 0.5) * 2.0; // 0..1 over second half
        return mix(uniforms.color, dst, t);
    }
}
