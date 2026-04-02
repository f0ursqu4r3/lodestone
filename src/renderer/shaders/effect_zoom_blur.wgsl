// @name: Zoom Blur
// @author: Lodestone
// @description: Radial zoom blur from a configurable center point
// @param: amount 0.1 0.0 0.5
// @param: center_x 0.5 0.0 1.0
// @param: center_y 0.5 0.0 1.0

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

const SAMPLES: i32 = 8;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let amount   = u.params_a.x;
    let center   = vec2(u.params_a.y, u.params_a.z);

    let dir = in.uv - center;

    var color = vec4(0.0);
    for (var i = 0; i < SAMPLES; i++) {
        let t = f32(i) / f32(SAMPLES - 1);
        let scale = 1.0 - amount * t;
        let sample_uv = center + dir * scale;
        color += textureSampleLevel(t_input, s_input, sample_uv, 0.0);
    }

    return color / f32(SAMPLES);
}
