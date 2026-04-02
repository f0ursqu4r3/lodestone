# Shader Authoring Guide

This guide covers writing custom effect shaders and transition shaders for Lodestone. Both systems use WGSL and follow the same header-metadata convention.

---

## 1. Overview

**Effects** are per-source filters applied before compositing. They run in a chain — each effect reads the previous effect's output and writes to a temp texture. The final output goes to the compositor. Examples: color correction, blur, circle crop, chroma key.

**Transitions** are full-screen shaders that blend two scene renders during a scene switch. They receive the outgoing scene ("from") and incoming scene ("to") as separate textures and output the blended frame. Examples: fade, wipe, slide, dissolve.

### Where shader files live

| Type | Directory |
|------|-----------|
| Effects | `config_dir/effects/*.wgsl` |
| Transitions | `config_dir/transitions/*.wgsl` |

On macOS, `config_dir` is typically `~/Library/Application Support/lodestone/`.

Both registries are rescanned every 2 seconds. Save a file and it reloads automatically — no restart needed.

---

## 2. Writing Effect Shaders

### File format

An effect shader is a plain `.wgsl` file. The filename stem becomes the effect's ID (e.g. `my_effect.wgsl` → ID `"my_effect"`).

### Header

The header is a block of `// @key: value` comment lines at the top of the file. Parsing stops at the first non-comment, non-blank line.

```wgsl
// @name: My Effect
// @author: Your Name
// @description: One-line description shown as a tooltip
// @param: intensity 1.0 0.0 2.0
// @param: threshold 0.5 0.0 1.0
```

| Tag | Required | Format |
|-----|----------|--------|
| `@name` | Yes | Display name in the UI |
| `@author` | No | Credit string |
| `@description` | No | Tooltip text |
| `@param` | No | `name default min max` — up to 8 params |

If `@name` is omitted, the file stem is title-cased (`my_effect` → `"My Effect"`).

### Uniform struct

Every effect shader must declare this struct exactly:

```wgsl
struct Uniforms {
    time: f32,             // Elapsed seconds since app start
    _pad: f32,             // Alignment padding — do not use
    resolution: vec2<f32>, // Input texture size in pixels (width, height)
    params_a: vec4<f32>,   // @param slots 0–3 (x, y, z, w)
    params_b: vec4<f32>,   // @param slots 4–7 (x, y, z, w)
}
```

Total size: 48 bytes (std140 aligned).

### Parameter mapping

`@param` declarations map to uniform slots in declaration order:

| Declaration order | Uniform field |
|-------------------|---------------|
| 1st `@param` | `params_a.x` |
| 2nd `@param` | `params_a.y` |
| 3rd `@param` | `params_a.z` |
| 4th `@param` | `params_a.w` |
| 5th `@param` | `params_b.x` |
| 6th `@param` | `params_b.y` |
| 7th `@param` | `params_b.z` |
| 8th `@param` | `params_b.w` |

### Bindings

```wgsl
@group(0) @binding(0) var t_input: texture_2d<f32>;
@group(0) @binding(1) var s_input: sampler;
@group(1) @binding(0) var<uniform> u: Uniforms;
```

`t_input` is the input texture — either the raw source texture (first effect in chain) or the previous effect's output (subsequent effects). The sampler is a linear clamp sampler.

### Vertex shader

The engine prepends a fullscreen-quad vertex shader automatically. You only write `fs_main`. The prepended vertex shader outputs:

```wgsl
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,  // 0..1, top-left origin
}
```

Your fragment entry point signature must be:

```wgsl
@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32>
```

### Template

```wgsl
// @name: My Effect
// @author: Your Name
// @description: What it does
// @param: my_param 1.0 0.0 2.0

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
    let my_param = u.params_a.x;
    let color = textureSampleLevel(t_input, s_input, in.uv, 0.0);
    // ... your effect logic ...
    return color;
}
```

### Complete example: Vignette

```wgsl
// @name: Vignette
// @author: Your Name
// @description: Darkens edges with configurable intensity and softness
// @param: intensity 0.5 0.0 1.0
// @param: softness 0.5 0.0 1.0

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
    let intensity = u.params_a.x;  // 1st @param
    let softness  = u.params_a.y;  // 2nd @param

    let color = textureSampleLevel(t_input, s_input, in.uv, 0.0);

    // Distance from center. Corners are ~0.7 away.
    let d = distance(in.uv, vec2(0.5, 0.5));

    // Vignette starts at (1 - softness) * 0.5 radius and maxes at the corner.
    let inner = 0.5 * (1.0 - softness);
    let outer = 0.7;
    let vignette = 1.0 - intensity * smoothstep(inner, outer, d);

    // Multiply RGB, preserve alpha.
    return vec4(color.rgb * vignette, color.a);
}
```

The `intensity` and `softness` sliders appear in the Properties panel automatically. Dragging `intensity` toward 1.0 deepens the darkening; `softness` controls how gradually it falls off from center.

---

## 3. Writing Transition Shaders

### File format

A transition shader is a `.wgsl` file placed in `config_dir/transitions/`. The filename stem becomes the transition's ID (e.g. `my_transition.wgsl` → ID `"my_transition"`).

### Header

```wgsl
// @name: My Transition
// @author: Your Name
// @description: One-line description
// @params: color, from_color, to_color
```

| Tag | Format |
|-----|--------|
| `@name` | Display name |
| `@author` | Credit string |
| `@description` | Tooltip text |
| `@params` | Comma-separated list of color uniforms to expose in the UI |

The `@params` tag is optional. Valid values: `color`, `from_color`, `to_color`. These control which color pickers appear in the transition settings UI.

- `color` — a single accent/dip color (used by Dip to Color)
- `from_color` — a color associated with the outgoing scene
- `to_color` — a color associated with the incoming scene

### Uniform struct

```wgsl
struct TransitionUniforms {
    progress: f32,       // 0.0 = transition start, 1.0 = transition complete
    time: f32,           // Elapsed seconds since app start (for animated effects)
    _pad0: f32,          // Alignment padding
    _pad1: f32,          // Alignment padding
    color: vec4<f32>,    // User-configured accent color (used if @params: color)
    from_color: vec4<f32>, // User-configured from-scene color
    to_color: vec4<f32>,   // User-configured to-scene color
};
```

### Bindings

```wgsl
@group(0) @binding(0) var t_from: texture_2d<f32>;
@group(0) @binding(1) var s_from: sampler;

@group(1) @binding(0) var t_to: texture_2d<f32>;
@group(1) @binding(1) var s_to: sampler;

@group(2) @binding(0) var<uniform> uniforms: TransitionUniforms;
```

`t_from` is a render of the outgoing scene. `t_to` is a render of the incoming scene. Both are the full canvas resolution.

### The src/dst naming convention

Within `fs_main`, name your sampled colors `src` (from) and `dst` (to). The names `from` and `to` are reserved keywords in WGSL — using them as variable names will cause a compile error.

```wgsl
let src = textureSample(t_from, s_from, in.uv);  // outgoing scene
let dst = textureSample(t_to, s_to, in.uv);       // incoming scene
```

### Vertex shader

Unlike effects, transitions include their own vertex shader. You must write both `vs_main` and `fs_main`. The vertex shader is the same fullscreen-quad boilerplate for every transition — just copy it verbatim:

```wgsl
@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VertexOutput {
    let x = f32((vi & 1u) * 2u) - 1.0;
    let y = 1.0 - f32((vi >> 1u) * 2u);
    let u = f32(vi & 1u);
    let v = f32(vi >> 1u);

    var out: VertexOutput;
    out.position = vec4<f32>(x, y, 0.0, 1.0);
    out.uv = vec2<f32>(u, v);
    return out;
}
```

### Template

```wgsl
// @name: My Transition
// @author: Your Name
// @description: What it does

struct TransitionUniforms {
    progress: f32,
    time: f32,
    _pad0: f32,
    _pad1: f32,
    color: vec4<f32>,
    from_color: vec4<f32>,
    to_color: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@group(0) @binding(0) var t_from: texture_2d<f32>;
@group(0) @binding(1) var s_from: sampler;

@group(1) @binding(0) var t_to: texture_2d<f32>;
@group(1) @binding(1) var s_to: sampler;

@group(2) @binding(0) var<uniform> uniforms: TransitionUniforms;

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VertexOutput {
    let x = f32((vi & 1u) * 2u) - 1.0;
    let y = 1.0 - f32((vi >> 1u) * 2u);
    let u = f32(vi & 1u);
    let v = f32(vi >> 1u);

    var out: VertexOutput;
    out.position = vec4<f32>(x, y, 0.0, 1.0);
    out.uv = vec2<f32>(u, v);
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let src = textureSample(t_from, s_from, in.uv);
    let dst = textureSample(t_to, s_to, in.uv);

    // ... your blend logic using uniforms.progress (0..1) ...

    return mix(src, dst, uniforms.progress);
}
```

### Complete example: Iris wipe

A circular reveal that expands from the center, with a soft edge:

```wgsl
// @name: Iris Wipe
// @author: Your Name
// @description: Circular reveal expanding from the center

struct TransitionUniforms {
    progress: f32,
    time: f32,
    _pad0: f32,
    _pad1: f32,
    color: vec4<f32>,
    from_color: vec4<f32>,
    to_color: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@group(0) @binding(0) var t_from: texture_2d<f32>;
@group(0) @binding(1) var s_from: sampler;
@group(1) @binding(0) var t_to: texture_2d<f32>;
@group(1) @binding(1) var s_to: sampler;
@group(2) @binding(0) var<uniform> uniforms: TransitionUniforms;

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VertexOutput {
    let x = f32((vi & 1u) * 2u) - 1.0;
    let y = 1.0 - f32((vi >> 1u) * 2u);
    var out: VertexOutput;
    out.position = vec4<f32>(x, y, 0.0, 1.0);
    out.uv = vec2<f32>(f32(vi & 1u), f32(vi >> 1u));
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let src = textureSample(t_from, s_from, in.uv);
    let dst = textureSample(t_to, s_to, in.uv);

    // Distance from center. Farthest corner is ~0.707 away.
    let dist = distance(in.uv, vec2(0.5, 0.5));

    // Expand radius from 0 to 0.75 (covers corners) as progress goes 0..1.
    let radius = uniforms.progress * 0.75;
    let edge = 0.02;  // soft edge width

    // Inside the circle: show incoming (dst). Outside: show outgoing (src).
    let blend = smoothstep(radius - edge, radius + edge, dist);
    return mix(dst, src, blend);
}
```

`progress` drives `radius`. At `progress = 0` the incoming scene is invisible; at `progress = 1` it fully covers the frame. `smoothstep` gives a 2% antialiased edge.

---

## 4. Tips & Best Practices

### Use textureSampleLevel for multi-sample effects

Effects that sample the input texture multiple times (blur, pixelate, UV distortion) must use `textureSampleLevel` instead of `textureSample`:

```wgsl
// Wrong — implicit LOD is undefined in a loop or when used multiple times
let c = textureSample(t_input, s_input, uv);

// Correct — explicit LOD 0 forces the base mip level
let c = textureSampleLevel(t_input, s_input, uv, 0.0);
```

Effects that only sample once at `in.uv` can use `textureSample` (the compiler is happy with a single implicit-gradient sample). When in doubt, use `textureSampleLevel`.

### Alpha handling in effects

Effects run before the compositor's alpha-over blend. Preserve the input alpha unless your effect intentionally modifies it (masking effects like Circle Crop and Rounded Corners multiply the alpha to create the shape).

```wgsl
// Correct: preserve alpha when doing a color-only operation
return vec4(modified_rgb, color.a);

// Correct: multiply alpha for a mask effect
return vec4(color.rgb, color.a * mask);

// Wrong: hardcoding alpha to 1.0 — kills transparency
return vec4(modified_rgb, 1.0);
```

### sRGB and intermediate texture format

Effect temp textures are `Rgba8Unorm` (linear, not sRGB). Math on `color.rgb` values operates in linear space, which is correct for most operations. If you need perceptual gamma (e.g. for mixing colors that look right to the eye), apply a manual gamma correction:

```wgsl
let linear = pow(srgb_value, vec3(2.2));   // sRGB → linear
let srgb   = pow(linear_value, vec3(1.0 / 2.2)); // linear → sRGB
```

### Performance

- Every texture sample has a cost. Minimize them where possible.
- The blur effect uses 9 samples per pass (18 total for H+V). That's the practical ceiling for a smooth result at 60fps.
- Avoid branching on per-pixel noise or hash functions in tight loops — the GPU handles branching poorly when neighboring pixels diverge.
- Avoid `textureSample` inside loops or conditionals — use `textureSampleLevel`.

### Easing transitions

Raw `progress` is linear. For more natural-feeling motion, apply an easing function:

```wgsl
// Ease-out cubic: fast start, deceleration at the end
let t = 1.0 - pow(1.0 - uniforms.progress, 3.0);

// Ease-in-out: smooth acceleration and deceleration
let t = smoothstep(0.0, 1.0, uniforms.progress);
```

### Live reload workflow

1. Place your `.wgsl` file in the transitions or effects directory.
2. Switch to Lodestone — within 2 seconds the shader appears in the UI.
3. Edit the file in your editor, save. The pipeline is recompiled on next use automatically.
4. No restart required at any point.

### Compile errors and fallback behavior

If a shader fails to compile (WGSL syntax error, missing binding, etc.), the effect or transition is silently skipped for that frame. The error is logged to the application log. The previously compiled pipeline is not used — the effect simply produces no output until the shader compiles successfully.

To debug: check `~/Library/Logs/lodestone/` (macOS) or the terminal if running from a debug build (`cargo run`). The wgpu error message includes the line number and description.

---

## 5. Built-in Effects Reference

All 12 built-in effects ship in `config_dir/effects/`. They can be used as-is or as reference implementations.

### Alpha/Shape masks

**Circle Crop** (`effect_circle_crop.wgsl`)
Crops the source to a circle with a soft feathered edge.
| Param | Default | Range | Description |
|-------|---------|-------|-------------|
| center_x | 0.5 | 0.0–1.0 | Circle center X in UV space |
| center_y | 0.5 | 0.0–1.0 | Circle center Y in UV space |
| radius | 0.4 | 0.0–1.0 | Circle radius in UV units |
| feather | 0.02 | 0.0–0.2 | Edge softness |

**Rounded Corners** (`effect_rounded_corners.wgsl`)
Rounds the corners of the source rectangle.
| Param | Default | Range | Description |
|-------|---------|-------|-------------|
| radius | 0.05 | 0.0–0.5 | Corner radius in UV units |
| feather | 0.005 | 0.0–0.05 | Edge softness |

**Gradient Fade** (`effect_gradient_fade.wgsl`)
Fades the source alpha along a direction.
| Param | Default | Range | Description |
|-------|---------|-------|-------------|
| angle | 0.0 | 0.0–360.0 | Fade direction in degrees |
| start | 0.3 | 0.0–1.0 | UV position where fade begins |
| end | 0.7 | 0.0–1.0 | UV position where fade completes |

### Color/Effect filters

**Color Correction** (`effect_color_correction.wgsl`)
Adjusts brightness, contrast, and saturation.
| Param | Default | Range | Description |
|-------|---------|-------|-------------|
| brightness | 0.0 | -1.0–1.0 | Additive brightness offset |
| contrast | 1.0 | 0.0–3.0 | Contrast multiplier around 0.5 midpoint |
| saturation | 1.0 | 0.0–3.0 | Saturation multiplier (0 = grayscale) |

**Chroma Key** (`effect_chroma_key.wgsl`)
Removes a key color (green screen). Sets alpha to 0 near the key color.
| Param | Default | Range | Description |
|-------|---------|-------|-------------|
| key_r | 0.0 | 0.0–1.0 | Key color red |
| key_g | 1.0 | 0.0–1.0 | Key color green |
| key_b | 0.0 | 0.0–1.0 | Key color blue |
| threshold | 0.3 | 0.0–1.0 | Distance from key color to start removing |
| smoothness | 0.1 | 0.0–0.5 | Width of the smooth transition |

**Blur** (`effect_blur.wgsl`)
Gaussian blur approximated with a 9-tap kernel. Internally runs as two passes (horizontal, then vertical). Only one param is exposed.
| Param | Default | Range | Description |
|-------|---------|-------|-------------|
| radius | 5.0 | 0.0–50.0 | Blur radius in pixels |

**Vignette** (`effect_vignette.wgsl`)
Darkens the edges of the source.
| Param | Default | Range | Description |
|-------|---------|-------|-------------|
| intensity | 0.5 | 0.0–1.0 | Darkness at the corners |
| softness | 0.5 | 0.0–1.0 | How gradually the vignette falls off |

**Sepia** (`effect_sepia.wgsl`)
Warm sepia tone.
| Param | Default | Range | Description |
|-------|---------|-------|-------------|
| intensity | 1.0 | 0.0–1.0 | Blend between original and full sepia |

**Invert** (`effect_invert.wgsl`)
Inverts RGB channel values.
| Param | Default | Range | Description |
|-------|---------|-------|-------------|
| intensity | 1.0 | 0.0–1.0 | Blend between original and fully inverted |

**Mirror** (`effect_mirror.wgsl`)
Flips the source horizontally and/or vertically.
| Param | Default | Range | Description |
|-------|---------|-------|-------------|
| horizontal | 1.0 | 0.0–1.0 | > 0.5 enables horizontal flip |
| vertical | 0.0 | 0.0–1.0 | > 0.5 enables vertical flip |

**Pixelate** (`effect_pixelate.wgsl`)
Mosaic / pixelation effect.
| Param | Default | Range | Description |
|-------|---------|-------|-------------|
| pixel_size | 8.0 | 1.0–100.0 | Block size in pixels |

**Scanlines** (`effect_scanlines.wgsl`)
CRT-style horizontal scanlines. Supports scrolling for animated scanline movement.
| Param | Default | Range | Description |
|-------|---------|-------|-------------|
| density | 200.0 | 10.0–1000.0 | Number of scanlines across the frame |
| opacity | 0.3 | 0.0–1.0 | Darkness of the dark scanlines |
| speed | 0.0 | 0.0–5.0 | Scroll speed (uses `time` uniform); 0 = static |

---

## 6. Built-in Transitions Reference

Transitions live in `config_dir/transitions/`. "Cut" is a special built-in with no shader — it is always present and switches scenes instantly.

**Cut** — Instant scene switch. No parameters.

**Fade** (`transition_fade.wgsl`)
Linear crossfade between outgoing and incoming scene. No user-configurable parameters.

**Wipe** (`transition_wipe.wgsl`)
Hard-edge horizontal wipe from left to right with a 2% antialiased edge. No user-configurable parameters.

**Slide** (`transition_slide.wgsl`)
Incoming scene slides in from the right, pushing the outgoing scene left. Uses an ease-out cubic curve for natural deceleration. No user-configurable parameters.

**Dissolve** (`transition_dissolve.wgsl`)
Noise-based pixel dissolve with organic, multi-octave grain texture. Uses a hash function (no noise texture required). No user-configurable parameters.

**Radial Wipe** (`transition_radial_wipe.wgsl`)
Circular reveal expanding from the center with a soft edge. Aspect-ratio corrected for 16:9. No user-configurable parameters.

**Dip to Color** (`transition_dip_to_color.wgsl`)
Fades to a solid color at the midpoint, then reveals the incoming scene. Exposes a `color` picker in the UI.
| UI control | Description |
|------------|-------------|
| Color | The dip color (defaults to black) |

Declared with `// @params: color` in the header, which tells the UI to render a color picker for `uniforms.color`.
