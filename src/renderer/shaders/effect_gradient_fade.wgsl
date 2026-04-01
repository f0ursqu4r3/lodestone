// @name: Gradient Fade
// @author: Lodestone
// @description: Fades source alpha along a direction
// @param: angle 0.0 0.0 360.0
// @param: start 0.3 0.0 1.0
// @param: end 0.7 0.0 1.0

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

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let angle_deg = u.params_a.x;
    let fade_start = u.params_a.y;
    let fade_end = u.params_a.z;
    let angle = radians(angle_deg);
    let dir = vec2(cos(angle), sin(angle));
    let t = dot(in.uv - vec2(0.5), dir) + 0.5;
    let alpha = smoothstep(fade_start, fade_end, t);
    let color = textureSample(t_input, s_input, in.uv);
    return vec4(color.rgb, color.a * alpha);
}
