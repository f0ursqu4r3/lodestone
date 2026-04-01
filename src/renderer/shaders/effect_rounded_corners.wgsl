// @name: Rounded Corners
// @author: Lodestone
// @description: Rounds the corners of the source
// @param: radius 0.05 0.0 0.5
// @param: feather 0.005 0.0 0.05

struct Uniforms {
    time: f32,
    _pad: f32,
    resolution: vec2<f32>,
    params: array<f32, 8>,
}

@group(0) @binding(0) var t_input: texture_2d<f32>;
@group(0) @binding(1) var s_input: sampler;
@group(1) @binding(0) var<uniform> u: Uniforms;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let radius = u.params[0];
    let feather = u.params[1];
    let color = textureSample(t_input, s_input, in.uv);
    let half = vec2(0.5);
    let p = abs(in.uv - half) - half + vec2(radius);
    let d = length(max(p, vec2(0.0))) - radius;
    let alpha = 1.0 - smoothstep(-feather, feather, d);
    return vec4(color.rgb, color.a * alpha);
}
