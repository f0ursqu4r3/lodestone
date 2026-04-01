// @name: Circle Crop
// @author: Lodestone
// @description: Crops source to a circle with soft edge
// @param: center_x 0.5 0.0 1.0
// @param: center_y 0.5 0.0 1.0
// @param: radius 0.4 0.0 1.0
// @param: feather 0.02 0.0 0.2

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
    let center_x = u.params[0];
    let center_y = u.params[1];
    let radius = u.params[2];
    let feather = u.params[3];
    let color = textureSample(t_input, s_input, in.uv);
    let dist = length(in.uv - vec2(center_x, center_y));
    let alpha = 1.0 - smoothstep(radius - feather, radius + feather, dist);
    return vec4(color.rgb, color.a * alpha);
}
