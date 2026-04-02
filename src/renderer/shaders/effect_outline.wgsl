// @name: Outline
// @author: Lodestone
// @description: Edge detection outline (Sobel)
// @param: thickness 1.0 0.5 5.0
// @param: intensity 1.0 0.0 2.0

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

fn luminance(c: vec3<f32>) -> f32 {
    return dot(c, vec3(0.2126, 0.7152, 0.0722));
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let thickness  = u.params_a.x;
    let intensity  = u.params_a.y;

    let texel = thickness / u.resolution;

    // 3x3 neighbourhood luminance samples
    let tl = luminance(textureSampleLevel(t_input, s_input, in.uv + vec2(-texel.x,  texel.y), 0.0).rgb);
    let tc = luminance(textureSampleLevel(t_input, s_input, in.uv + vec2( 0.0,      texel.y), 0.0).rgb);
    let tr = luminance(textureSampleLevel(t_input, s_input, in.uv + vec2( texel.x,  texel.y), 0.0).rgb);
    let ml = luminance(textureSampleLevel(t_input, s_input, in.uv + vec2(-texel.x,  0.0    ), 0.0).rgb);
    let mr = luminance(textureSampleLevel(t_input, s_input, in.uv + vec2( texel.x,  0.0    ), 0.0).rgb);
    let bl = luminance(textureSampleLevel(t_input, s_input, in.uv + vec2(-texel.x, -texel.y), 0.0).rgb);
    let bc = luminance(textureSampleLevel(t_input, s_input, in.uv + vec2( 0.0,     -texel.y), 0.0).rgb);
    let br = luminance(textureSampleLevel(t_input, s_input, in.uv + vec2( texel.x, -texel.y), 0.0).rgb);

    // Sobel kernels
    let gx = -tl - 2.0 * ml - bl + tr + 2.0 * mr + br;
    let gy = -tl - 2.0 * tc - tr + bl + 2.0 * bc + br;
    let edge = clamp(sqrt(gx * gx + gy * gy) * intensity, 0.0, 1.0);

    let base = textureSampleLevel(t_input, s_input, in.uv, 0.0);

    // Overlay edge on top of original image
    return vec4(mix(base.rgb, vec3(0.0), edge), base.a);
}
