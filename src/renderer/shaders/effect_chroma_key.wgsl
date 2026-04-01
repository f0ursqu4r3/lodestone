// @name: Chroma Key
// @author: Lodestone
// @description: Removes a key color (green screen)
// @param: key_r 0.0 0.0 1.0
// @param: key_g 1.0 0.0 1.0
// @param: key_b 0.0 0.0 1.0
// @param: threshold 0.3 0.0 1.0
// @param: smoothness 0.1 0.0 0.5

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
    let key = vec3(u.params_a.x, u.params_a.y, u.params_a.z);
    let threshold = u.params_a.w;
    let smooth_width = u.params_b.x;
    let color = textureSample(t_input, s_input, in.uv);
    let diff = distance(color.rgb, key);
    let alpha = smoothstep(threshold, threshold + smooth_width, diff);
    return vec4(color.rgb, color.a * alpha);
}
