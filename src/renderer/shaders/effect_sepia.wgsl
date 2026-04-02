// @name: Sepia
// @author: Lodestone
// @description: Warm sepia tone
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

    let r = dot(c.rgb, vec3(0.393, 0.769, 0.189));
    let g = dot(c.rgb, vec3(0.349, 0.686, 0.168));
    let b = dot(c.rgb, vec3(0.272, 0.534, 0.131));
    let sepia = vec3(r, g, b);

    return vec4(mix(c.rgb, sepia, intensity), c.a);
}
