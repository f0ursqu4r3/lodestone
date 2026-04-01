// @name: Slide
// @author: Lodestone
// @description: Incoming scene slides in from the right, pushing the outgoing scene left

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
    // Ease-out cubic for natural deceleration
    let t = 1.0 - pow(1.0 - uniforms.progress, 3.0);

    // Outgoing scene slides left: sample with positive UV offset
    let from_uv = vec2<f32>(in.uv.x + t, in.uv.y);
    // Incoming scene slides in from the right: sample with negative offset
    let to_uv = vec2<f32>(in.uv.x - 1.0 + t, in.uv.y);

    // Show whichever scene occupies this pixel
    if to_uv.x >= 0.0 {
        return textureSample(t_to, s_to, to_uv);
    } else {
        return textureSample(t_from, s_from, from_uv);
    }
}
