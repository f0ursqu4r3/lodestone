// @name: Scanlines
// @author: Lodestone
// @description: CRT-style horizontal scanlines
// @param: density 200.0 10.0 1000.0
// @param: opacity 0.3 0.0 1.0
// @param: speed 0.0 0.0 5.0

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
    let density = u.params_a.x;
    let opacity  = u.params_a.y;
    let speed    = u.params_a.z;

    let color = textureSampleLevel(t_input, s_input, in.uv, 0.0);

    // Scrolling scanline pattern
    let scroll = u.time * speed;
    let line = sin((in.uv.y + scroll) * density) * 0.5 + 0.5;
    // line is 0..1; darken on the trough
    let mask = 1.0 - opacity * (1.0 - line);

    return vec4(color.rgb * mask, color.a);
}
