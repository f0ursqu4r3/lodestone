// src/renderer/shaders/transition_fade.wgsl
//
// Crossfade transition: linearly blends two scene canvases by progress.
// Standard transition interface — all transition shaders receive:
//   - t_from / s_from: outgoing scene texture + sampler (group 0)
//   - t_to / s_to:     incoming scene texture + sampler (group 1)
//   - uniforms.progress: 0.0 (fully "from") to 1.0 (fully "to")
//   - uniforms.time:     elapsed seconds since transition start

struct TransitionUniforms {
    progress: f32,
    time: f32,
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
    let from_color = textureSample(t_from, s_from, in.uv);
    let to_color = textureSample(t_to, s_to, in.uv);
    return mix(from_color, to_color, uniforms.progress);
}
