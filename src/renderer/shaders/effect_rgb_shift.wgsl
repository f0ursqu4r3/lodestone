// @name: RGB Shift
// @author: Lodestone
// @description: Chromatic aberration — RGB channel offset
// @param: amount 3.0 0.0 20.0
// @param: angle 0.0 0.0 360.0

struct Uniforms {
    time: f32,
    _pad: f32,
    resolution: vec2<f32>,
    params_a: vec4<f32>,
    params_b: vec4<f32>,
}

@group(0) @binding(0) var t_input: texture_2d<f32>;
@group(0) @binding(1) var s_input: sampler;
@group(1) @binding(0) var<uniform> u: Uniforms;

const TAU: f32 = 6.283185307;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let amount = u.params_a.x;
    let angle  = u.params_a.y * (TAU / 360.0);

    let offset = vec2(cos(angle), sin(angle)) * (amount / u.resolution);

    let r = textureSampleLevel(t_input, s_input, in.uv + offset, 0.0).r;
    let g = textureSampleLevel(t_input, s_input, in.uv,          0.0).g;
    let b = textureSampleLevel(t_input, s_input, in.uv - offset, 0.0).b;
    let a = textureSampleLevel(t_input, s_input, in.uv,          0.0).a;

    return vec4(r, g, b, a);
}
