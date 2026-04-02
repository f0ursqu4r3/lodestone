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
    let texel = vec2(1.0 / u.resolution.x, 1.0 / u.resolution.y);
    let dir = select(vec2(texel.x, 0.0), vec2(0.0, texel.y), direction > 0.5);
    let steps = i32(clamp(radius, 1.0, 50.0));
    var color = vec4(0.0);
    var total_weight = 0.0;
    let sigma = radius * 0.33333;
    for (var i = -steps; i <= steps; i = i + 1) {
        let offset = dir * f32(i);
        let w = exp(-0.5 * f32(i * i) / (sigma * sigma + 0.0001));
        // Use textureSampleLevel with explicit LOD 0 — textureSample uses
        // implicit derivatives which produce undefined results in a loop
        // with dynamic UV offsets.
        color += textureSampleLevel(t_input, s_input, in.uv + offset, 0.0) * w;
        total_weight += w;
    }
    return color / total_weight;
}
