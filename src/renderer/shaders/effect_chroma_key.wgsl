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
    params: array<f32, 8>,
}

@group(0) @binding(0) var t_input: texture_2d<f32>;
@group(0) @binding(1) var s_input: sampler;
@group(1) @binding(0) var<uniform> u: Uniforms;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let key = vec3(u.params[0], u.params[1], u.params[2]);
    let threshold = u.params[3];
    let smooth_width = u.params[4];
    let color = textureSample(t_input, s_input, in.uv);
    let diff = distance(color.rgb, key);
    let alpha = smoothstep(threshold, threshold + smooth_width, diff);
    return vec4(color.rgb, color.a * alpha);
}
