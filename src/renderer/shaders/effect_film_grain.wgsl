// @name: Film Grain
// @author: Lodestone
// @description: Animated noise grain overlay
// @param: intensity 0.15 0.0 1.0
// @param: speed 1.0 0.0 5.0

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

// Pseudo-random hash from a 2D seed
fn hash(p: vec2<f32>) -> f32 {
    let q = fract(p * vec2(127.1, 311.7));
    let r = q + dot(q, q.yx + 19.19);
    return fract((r.x + r.y) * r.x);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let intensity = u.params_a.x;
    let speed     = u.params_a.y;

    let color = textureSampleLevel(t_input, s_input, in.uv, 0.0);

    // Advance the seed each frame using time and speed
    let frame_seed = floor(u.time * speed * 24.0);
    let noise = hash(in.uv * u.resolution + vec2(frame_seed, frame_seed * 1.3));

    // Center grain around 0 and scale by intensity
    let grain = (noise - 0.5) * intensity;

    return vec4(clamp(color.rgb + grain, vec3(0.0), vec3(1.0)), color.a);
}
