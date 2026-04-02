// @name: Blur
// @author: Lodestone
// @description: Gaussian blur (run as two passes: H then V)
// @param: radius 5.0 0.0 50.0

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
    let radius = u.params_a.x;
    let direction = u.params_a.y;
    let texel = 1.0 / u.resolution;
    let dir = select(vec2(texel.x, 0.0), vec2(0.0, texel.y), direction > 0.5);

    // Fixed 9-tap Gaussian kernel, step size scales with radius.
    let step = dir * max(radius / 4.0, 1.0);
    let w0 = 0.2270270;
    let w1 = 0.1945946;
    let w2 = 0.1216216;
    let w3 = 0.0540540;
    let w4 = 0.0162162;

    var color = textureSampleLevel(t_input, s_input, in.uv, 0.0) * w0;
    color += textureSampleLevel(t_input, s_input, in.uv + step * 1.0, 0.0) * w1;
    color += textureSampleLevel(t_input, s_input, in.uv - step * 1.0, 0.0) * w1;
    color += textureSampleLevel(t_input, s_input, in.uv + step * 2.0, 0.0) * w2;
    color += textureSampleLevel(t_input, s_input, in.uv - step * 2.0, 0.0) * w2;
    color += textureSampleLevel(t_input, s_input, in.uv + step * 3.0, 0.0) * w3;
    color += textureSampleLevel(t_input, s_input, in.uv - step * 3.0, 0.0) * w3;
    color += textureSampleLevel(t_input, s_input, in.uv + step * 4.0, 0.0) * w4;
    color += textureSampleLevel(t_input, s_input, in.uv - step * 4.0, 0.0) * w4;

    return color;
}
