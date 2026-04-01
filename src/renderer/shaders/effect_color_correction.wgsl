// @name: Color Correction
// @author: Lodestone
// @description: Adjusts brightness, contrast, and saturation
// @param: brightness 0.0 -1.0 1.0
// @param: contrast 1.0 0.0 3.0
// @param: saturation 1.0 0.0 3.0

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
    let brightness = u.params_a.x;
    let contrast = u.params_a.y;
    let saturation = u.params_a.z;
    var color = textureSample(t_input, s_input, in.uv);
    color = vec4(color.rgb + vec3(brightness), color.a);
    color = vec4((color.rgb - vec3(0.5)) * contrast + vec3(0.5), color.a);
    let luma = dot(color.rgb, vec3(0.299, 0.587, 0.114));
    color = vec4(mix(vec3(luma), color.rgb, saturation), color.a);
    return color;
}
