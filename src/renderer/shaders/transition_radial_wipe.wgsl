// @name: Radial Wipe
// @author: Lodestone
// @description: Circular reveal expanding from the center

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
    let from = textureSample(t_from, s_from, in.uv);
    let to = textureSample(t_to, s_to, in.uv);

    // Distance from center, corrected for 16:9 aspect ratio
    let center = vec2<f32>(0.5, 0.5);
    let aspect = vec2<f32>(16.0 / 9.0, 1.0);
    let dist = length((in.uv - center) * aspect);

    // Max possible distance (corner to center) for normalization
    let max_dist = length(vec2<f32>(0.5, 0.5) * aspect);

    // Radius expands from 0 to max_dist over the transition
    let radius = uniforms.progress * max_dist * 1.1; // slight overshoot so edges fully clear
    let edge = 0.02 * max_dist; // soft edge

    let blend = smoothstep(radius - edge, radius + edge, dist);
    return mix(to, from, blend);
}
