// @name: Invert
// @author: Lodestone
// @description: Color inversion
// @param: intensity 1.0 0.0 1.0

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
    let intensity = u.params_a.x;

    let c = textureSampleLevel(t_input, s_input, in.uv, 0.0);
    let inverted = vec3(1.0) - c.rgb;

    return vec4(mix(c.rgb, inverted, intensity), c.a);
}
