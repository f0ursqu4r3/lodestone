// @name: Dissolve
// @author: Lodestone
// @description: Noise-based pixel dissolve with organic, grainy texture

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

// Hash-based pseudo-random noise (no texture needed).
// Returns 0..1 for a given 2D coordinate.
fn hash(p: vec2<f32>) -> f32 {
    var p3 = fract(vec3<f32>(p.x, p.y, p.x) * 0.1031);
    p3 = p3 + vec3<f32>(dot(p3, vec3<f32>(p3.y + 33.33, p3.z + 33.33, p3.x + 33.33)));
    return fract((p3.x + p3.y) * p3.z);
}

// Multi-octave noise for organic-looking dissolve pattern
fn dissolve_noise(uv: vec2<f32>) -> f32 {
    let scale1 = uv * 80.0;
    let scale2 = uv * 40.0;
    let scale3 = uv * 160.0;

    // Blend multiple frequencies: coarse shapes + fine grain
    let n = hash(floor(scale1)) * 0.5
          + hash(floor(scale2)) * 0.35
          + hash(floor(scale3)) * 0.15;

    return n;
}

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

    let noise = dissolve_noise(in.uv);

    // Each pixel dissolves when progress passes its noise threshold.
    // Soft edge for slight antialiasing at the dissolve boundary.
    let edge = 0.04;
    let blend = smoothstep(uniforms.progress - edge, uniforms.progress + edge, noise);

    return mix(dst, src, blend);
}
