// @name: Vignette
// @author: Lodestone
// @description: Darkens edges with configurable intensity and softness
// @param: intensity 0.5 0.0 1.0
// @param: softness 0.5 0.0 1.0

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
    let softness = u.params_a.y;

    let color = textureSampleLevel(t_input, s_input, in.uv, 0.0);

    // Signed distance from center, ranging 0..~0.7 at corners
    let d = distance(in.uv, vec2(0.5, 0.5));

    // Remap: vignette starts at (1 - softness) radius and fully dark at 0.7
    let inner = 0.5 * (1.0 - softness);
    let outer = 0.7;
    let vignette = 1.0 - intensity * smoothstep(inner, outer, d);

    return vec4(color.rgb * vignette, color.a);
}
