// @name: Pixelate
// @author: Lodestone
// @description: Pixelation / mosaic effect
// @param: pixel_size 8.0 1.0 100.0

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
    let pixel_size = max(u.params_a.x, 1.0);

    // Snap UV to the center of the nearest pixel block
    let block = pixel_size / u.resolution;
    let snapped = (floor(in.uv / block) + 0.5) * block;

    return textureSampleLevel(t_input, s_input, snapped, 0.0);
}
