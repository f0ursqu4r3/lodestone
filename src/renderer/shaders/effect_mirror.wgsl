// @name: Mirror
// @author: Lodestone
// @description: Horizontal and/or vertical mirror
// @param: horizontal 1.0 0.0 1.0
// @param: vertical 0.0 0.0 1.0

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
    let do_h = u.params_a.x > 0.5;
    let do_v = u.params_a.y > 0.5;

    var uv = in.uv;
    if do_h {
        uv.x = 1.0 - uv.x;
    }
    if do_v {
        uv.y = 1.0 - uv.y;
    }

    return textureSampleLevel(t_input, s_input, uv, 0.0);
}
