# Shader Effects System Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Per-source shader effect chain system with user-loadable WGSL effects, multi-pass GPU pipeline, configurable parameters, and inline Properties panel editor.

**Architecture:** Multi-pass rendering — each effect in a source's chain runs its own GPU render pass via ping-pong temp textures. Effects are standalone WGSL files with header metadata (same pattern as transitions). EffectRegistry scans a directory; EffectPipeline lazy-compiles and caches pipelines; the compositor integrates the chain before its existing alpha-over blend pass.

**Tech Stack:** Rust, wgpu, WGSL shaders, egui (UI), serde/toml (persistence)

---

### Task 1: Data Model — EffectInstance and scene integration

**Files:**
- Modify: `src/scene.rs`

- [ ] **Step 1: Add EffectInstance struct**

Add after the existing `SourceOverrides` struct (~line 78):

```rust
/// A single effect instance applied to a source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffectInstance {
    /// Effect ID matching the registry (e.g. "circle_crop").
    pub effect_id: String,
    /// Parameter values keyed by name. Missing keys use the effect's default.
    #[serde(default)]
    pub params: std::collections::HashMap<String, f32>,
    /// Whether this effect is active. Disabled effects remain in the chain but are skipped.
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}
```

- [ ] **Step 2: Add effects field to LibrarySource**

In the `LibrarySource` struct, add after `folder`:

```rust
    /// Ordered chain of shader effects applied to this source.
    #[serde(default)]
    pub effects: Vec<EffectInstance>,
```

- [ ] **Step 3: Add effects override to SourceOverrides**

In the `SourceOverrides` struct, add after `locked`:

```rust
    /// Per-scene effect chain override. Replaces the entire library chain when set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effects: Option<Vec<EffectInstance>>,
```

- [ ] **Step 4: Add resolve_effects method to SceneSource**

Add alongside the existing `resolve_*` methods:

```rust
    /// Resolve the effect chain: use scene override if set, otherwise library defaults.
    pub fn resolve_effects(&self, lib: &LibrarySource) -> Vec<EffectInstance> {
        self.overrides
            .effects
            .clone()
            .unwrap_or_else(|| lib.effects.clone())
    }

    /// Returns true if the scene overrides the effect chain.
    pub fn is_effects_overridden(&self) -> bool {
        self.overrides.effects.is_some()
    }
```

- [ ] **Step 5: Add default effects to LibrarySource Default**

In any `Default` or initialization for LibrarySource, ensure `effects: Vec::new()` is included. Check existing constructors — the `new_*` factory methods in scene.rs or wherever LibrarySource is created — and add `effects: Vec::new()`.

- [ ] **Step 6: Write tests**

```rust
#[cfg(test)]
mod effect_tests {
    use super::*;

    #[test]
    fn effect_instance_default_enabled() {
        let effect = EffectInstance {
            effect_id: "circle_crop".to_string(),
            params: std::collections::HashMap::new(),
            enabled: true,
        };
        assert!(effect.enabled);
        assert!(effect.params.is_empty());
    }

    #[test]
    fn resolve_effects_uses_library_default() {
        let lib = LibrarySource {
            // ... set up with effects: vec![EffectInstance { effect_id: "blur".into(), .. }]
            ..Default::default() // or minimal construction
        };
        let ss = SceneSource {
            source_id: SourceId(1),
            overrides: SourceOverrides::default(),
        };
        let resolved = ss.resolve_effects(&lib);
        assert_eq!(resolved.len(), lib.effects.len());
    }

    #[test]
    fn resolve_effects_uses_override_when_set() {
        let lib = LibrarySource { ..Default::default() };
        let override_chain = vec![EffectInstance {
            effect_id: "chroma_key".to_string(),
            params: std::collections::HashMap::new(),
            enabled: true,
        }];
        let ss = SceneSource {
            source_id: SourceId(1),
            overrides: SourceOverrides {
                effects: Some(override_chain.clone()),
                ..Default::default()
            },
        };
        let resolved = ss.resolve_effects(&lib);
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].effect_id, "chroma_key");
    }

    #[test]
    fn effect_instance_roundtrip_toml() {
        let effect = EffectInstance {
            effect_id: "circle_crop".to_string(),
            params: [("radius".to_string(), 0.4), ("feather".to_string(), 0.02)]
                .into_iter()
                .collect(),
            enabled: true,
        };
        let toml_str = toml::to_string(&effect).unwrap();
        let restored: EffectInstance = toml::from_str(&toml_str).unwrap();
        assert_eq!(restored.effect_id, "circle_crop");
        assert!((restored.params["radius"] - 0.4).abs() < f32::EPSILON);
    }
}
```

- [ ] **Step 7: Verify**

Run: `cargo test`
Expected: All existing tests pass + new effect tests pass.

- [ ] **Step 8: Commit**

```bash
git add src/scene.rs
git commit -m "feat: add EffectInstance data model and scene integration"
```

---

### Task 2: EffectRegistry — header parser and directory scanner

**Files:**
- Create: `src/effect_registry.rs`
- Modify: `src/main.rs` (add `mod effect_registry;`)

- [ ] **Step 1: Create the EffectRegistry module**

Create `src/effect_registry.rs`. Follow the pattern from `src/transition_registry.rs` but with float params instead of color params:

```rust
//! Registry of available shader effects.
//!
//! Scans a directory of `.wgsl` files, parses `@name`, `@param` headers,
//! and provides lookup by effect ID. Same pattern as `TransitionRegistry`.

use std::collections::HashMap;
use std::path::Path;

/// Definition of a single parameter exposed by an effect shader.
#[derive(Debug, Clone)]
pub struct ParamDef {
    pub name: String,
    pub default: f32,
    pub min: f32,
    pub max: f32,
}

/// A registered effect shader with metadata parsed from its header.
#[derive(Debug, Clone)]
pub struct EffectDef {
    pub id: String,
    pub name: String,
    pub author: String,
    pub description: String,
    pub params: Vec<ParamDef>,
    pub shader_source: String,
    pub is_builtin: bool,
}

/// Registry of available effect shaders, scanned from a directory.
pub struct EffectRegistry {
    effects: Vec<EffectDef>,
    fingerprint: u64,
}

impl EffectRegistry {
    /// Create an empty registry.
    pub fn empty() -> Self {
        Self {
            effects: Vec::new(),
            fingerprint: 0,
        }
    }

    /// Scan a directory for `.wgsl` effect shaders and build the registry.
    pub fn scan(dir: &Path) -> Self {
        let mut effects = Vec::new();

        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return Self::empty(),
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map_or(true, |e| e != "wgsl") {
                continue;
            }
            let id = path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let source = match std::fs::read_to_string(&path) {
                Ok(s) => s,
                Err(_) => continue,
            };

            let (name, author, description, params) = parse_header(&source);

            effects.push(EffectDef {
                id,
                name,
                author,
                description,
                params,
                shader_source: source,
                is_builtin: false,
            });
        }

        effects.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        let fingerprint = compute_fingerprint(&effects);

        Self {
            effects,
            fingerprint,
        }
    }

    /// Rescan the directory. Returns `true` if anything changed.
    pub fn rescan(&mut self, dir: &Path) -> bool {
        let fresh = Self::scan(dir);
        if fresh.fingerprint != self.fingerprint {
            *self = fresh;
            true
        } else {
            false
        }
    }

    /// All registered effects.
    pub fn all(&self) -> &[EffectDef] {
        &self.effects
    }

    /// Look up an effect by ID.
    pub fn get(&self, id: &str) -> Option<&EffectDef> {
        self.effects.iter().find(|e| e.id == id)
    }
}

fn compute_fingerprint(effects: &[EffectDef]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for e in effects {
        e.id.hash(&mut hasher);
        e.shader_source.len().hash(&mut hasher);
        e.shader_source.hash(&mut hasher);
    }
    hasher.finish()
}

/// Parse effect shader header comments.
///
/// Recognizes:
/// - `// @name: Display Name`
/// - `// @author: Author`
/// - `// @description: Description text`
/// - `// @param: name default min max`
fn parse_header(source: &str) -> (String, String, String, Vec<ParamDef>) {
    let mut name = String::new();
    let mut author = String::new();
    let mut description = String::new();
    let mut params = Vec::new();

    for line in source.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with("//") {
            break;
        }
        let comment = trimmed.trim_start_matches("//").trim();

        if let Some(val) = comment.strip_prefix("@name:") {
            name = val.trim().to_string();
        } else if let Some(val) = comment.strip_prefix("@author:") {
            author = val.trim().to_string();
        } else if let Some(val) = comment.strip_prefix("@description:") {
            description = val.trim().to_string();
        } else if let Some(val) = comment.strip_prefix("@param:") {
            let parts: Vec<&str> = val.trim().split_whitespace().collect();
            if parts.len() >= 4 {
                if let (Ok(default), Ok(min), Ok(max)) = (
                    parts[1].parse::<f32>(),
                    parts[2].parse::<f32>(),
                    parts[3].parse::<f32>(),
                ) {
                    params.push(ParamDef {
                        name: parts[0].to_string(),
                        default,
                        min,
                        max,
                    });
                }
            }
        }
    }

    if name.is_empty() {
        // Fallback: won't happen for well-formed files, but defensive.
        name = "Unknown Effect".to_string();
    }

    (name, author, description, params)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_header_basic() {
        let source = r#"// @name: Circle Crop
// @author: Lodestone
// @description: Crops to a circle
// @param: radius 0.4 0.0 1.0
// @param: feather 0.02 0.0 0.2

@fragment
fn fs_main() {}"#;
        let (name, author, desc, params) = parse_header(source);
        assert_eq!(name, "Circle Crop");
        assert_eq!(author, "Lodestone");
        assert_eq!(desc, "Crops to a circle");
        assert_eq!(params.len(), 2);
        assert_eq!(params[0].name, "radius");
        assert!((params[0].default - 0.4).abs() < f32::EPSILON);
        assert!((params[0].min - 0.0).abs() < f32::EPSILON);
        assert!((params[0].max - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn parse_header_no_params() {
        let source = "// @name: Simple\n@fragment\nfn fs_main() {}";
        let (name, _, _, params) = parse_header(source);
        assert_eq!(name, "Simple");
        assert!(params.is_empty());
    }

    #[test]
    fn empty_registry() {
        let reg = EffectRegistry::empty();
        assert!(reg.all().is_empty());
        assert!(reg.get("anything").is_none());
    }
}
```

- [ ] **Step 2: Register the module**

In `src/main.rs`, add `mod effect_registry;` alongside the existing `mod transition_registry;`.

- [ ] **Step 3: Verify**

Run: `cargo test effect_registry`
Expected: 3 tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/effect_registry.rs src/main.rs
git commit -m "feat: add EffectRegistry with header parser and directory scanner"
```

---

### Task 3: Built-in effect shaders (6 WGSL files)

**Files:**
- Create: `src/renderer/shaders/effect_circle_crop.wgsl`
- Create: `src/renderer/shaders/effect_rounded_corners.wgsl`
- Create: `src/renderer/shaders/effect_gradient_fade.wgsl`
- Create: `src/renderer/shaders/effect_color_correction.wgsl`
- Create: `src/renderer/shaders/effect_chroma_key.wgsl`
- Create: `src/renderer/shaders/effect_blur.wgsl`
- Modify: `src/settings.rs` (add `effects_dir()` and `seed_builtin_effects()`)

All effect shaders share the same bindings and vertex shader. The vertex shader is prepended by the engine at compile time (Task 4). Each file only needs the Uniforms struct declaration, bindings, and `fs_main`.

- [ ] **Step 1: Add effects_dir() and seed_builtin_effects() to settings.rs**

Add after `transitions_dir()`:

```rust
pub fn effects_dir() -> PathBuf {
    config_dir().join("effects")
}

/// Write built-in effect shaders to the effects directory (if not already present).
pub fn seed_builtin_effects() {
    let dir = effects_dir();
    if let Err(e) = std::fs::create_dir_all(&dir) {
        log::warn!("Failed to create effects directory: {e}");
        return;
    }

    let builtins: &[(&str, &str)] = &[
        ("circle_crop.wgsl", include_str!("renderer/shaders/effect_circle_crop.wgsl")),
        ("rounded_corners.wgsl", include_str!("renderer/shaders/effect_rounded_corners.wgsl")),
        ("gradient_fade.wgsl", include_str!("renderer/shaders/effect_gradient_fade.wgsl")),
        ("color_correction.wgsl", include_str!("renderer/shaders/effect_color_correction.wgsl")),
        ("chroma_key.wgsl", include_str!("renderer/shaders/effect_chroma_key.wgsl")),
        ("blur.wgsl", include_str!("renderer/shaders/effect_blur.wgsl")),
    ];

    for (filename, content) in builtins {
        let path = dir.join(filename);
        if !path.exists() {
            if let Err(e) = std::fs::write(&path, content) {
                log::warn!("Failed to write built-in effect {filename}: {e}");
            }
        }
    }
}
```

- [ ] **Step 2: Create circle_crop.wgsl**

```wgsl
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
```

- [ ] **Step 3: Create rounded_corners.wgsl**

```wgsl
// @name: Rounded Corners
// @author: Lodestone
// @description: Rounds the corners of the source
// @param: radius 0.05 0.0 0.5
// @param: feather 0.005 0.0 0.05

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
    let radius = u.params[0];
    let feather = u.params[1];

    let color = textureSample(t_input, s_input, in.uv);

    // Distance from nearest corner in UV space
    let half = vec2(0.5);
    let p = abs(in.uv - half) - half + vec2(radius);
    let d = length(max(p, vec2(0.0))) - radius;
    let alpha = 1.0 - smoothstep(-feather, feather, d);

    return vec4(color.rgb, color.a * alpha);
}
```

- [ ] **Step 4: Create gradient_fade.wgsl**

```wgsl
// @name: Gradient Fade
// @author: Lodestone
// @description: Fades source alpha along a direction
// @param: angle 0.0 0.0 360.0
// @param: start 0.3 0.0 1.0
// @param: end 0.7 0.0 1.0

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
    let angle_deg = u.params[0];
    let fade_start = u.params[1];
    let fade_end = u.params[2];

    let angle = radians(angle_deg);
    let dir = vec2(cos(angle), sin(angle));
    let t = dot(in.uv - vec2(0.5), dir) + 0.5;
    let alpha = smoothstep(fade_start, fade_end, t);

    let color = textureSample(t_input, s_input, in.uv);
    return vec4(color.rgb, color.a * alpha);
}
```

- [ ] **Step 5: Create color_correction.wgsl**

```wgsl
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
    params: array<f32, 8>,
}

@group(0) @binding(0) var t_input: texture_2d<f32>;
@group(0) @binding(1) var s_input: sampler;
@group(1) @binding(0) var<uniform> u: Uniforms;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let brightness = u.params[0];
    let contrast = u.params[1];
    let saturation = u.params[2];

    var color = textureSample(t_input, s_input, in.uv);

    // Brightness
    color = vec4(color.rgb + vec3(brightness), color.a);

    // Contrast (around 0.5 midpoint)
    color = vec4((color.rgb - vec3(0.5)) * contrast + vec3(0.5), color.a);

    // Saturation
    let luma = dot(color.rgb, vec3(0.299, 0.587, 0.114));
    color = vec4(mix(vec3(luma), color.rgb, saturation), color.a);

    return color;
}
```

- [ ] **Step 6: Create chroma_key.wgsl**

```wgsl
// @name: Chroma Key
// @author: Lodestone
// @description: Removes a key color (green screen)
// @param: key_r 0.0 0.0 1.0
// @param: key_g 1.0 0.0 1.0
// @param: key_b 0.0 0.0 1.0
// @param: threshold 0.3 0.0 1.0
// @param: smoothness 0.1 0.0 0.5

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
    let key = vec3(u.params[0], u.params[1], u.params[2]);
    let threshold = u.params[3];
    let smooth_width = u.params[4];

    let color = textureSample(t_input, s_input, in.uv);
    let diff = distance(color.rgb, key);
    let alpha = smoothstep(threshold, threshold + smooth_width, diff);

    return vec4(color.rgb, color.a * alpha);
}
```

- [ ] **Step 7: Create blur.wgsl**

Blur is a single-pass separable approximation. The EffectPipeline will run it twice (horizontal then vertical) using the `direction` param.

```wgsl
// @name: Blur
// @author: Lodestone
// @description: Gaussian blur (run as two passes: H then V)
// @param: radius 5.0 0.0 50.0
// @param: direction 0.0 0.0 1.0

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
    let radius = u.params[0];
    let direction = u.params[1]; // 0.0 = horizontal, 1.0 = vertical

    let texel = vec2(1.0 / u.resolution.x, 1.0 / u.resolution.y);
    let dir = select(vec2(texel.x, 0.0), vec2(0.0, texel.y), direction > 0.5);

    let steps = i32(clamp(radius, 1.0, 50.0));
    var color = vec4(0.0);
    var total_weight = 0.0;

    for (var i = -steps; i <= steps; i = i + 1) {
        let offset = dir * f32(i);
        let sigma = radius * 0.33333;
        let w = exp(-0.5 * f32(i * i) / (sigma * sigma + 0.0001));
        color += textureSample(t_input, s_input, in.uv + offset) * w;
        total_weight += w;
    }

    return color / total_weight;
}
```

- [ ] **Step 8: Verify**

Run: `cargo build`
Expected: Compiles (shaders are just string assets via `include_str!`).

- [ ] **Step 9: Commit**

```bash
git add src/renderer/shaders/effect_*.wgsl src/settings.rs
git commit -m "feat: add 6 built-in effect shaders and seed function"
```

---

### Task 4: EffectPipeline — GPU pipeline manager

**Files:**
- Create: `src/renderer/effect_pipeline.rs`
- Modify: `src/renderer/mod.rs` (add to SharedGpuState)

- [ ] **Step 1: Create effect_pipeline.rs**

```rust
//! GPU pipeline for applying shader effect chains to source textures.
//!
//! Each effect runs as a render pass: input texture → fragment shader → output texture.
//! Effects in a chain use ping-pong temp textures (A→B, B→A).

use std::collections::HashMap;
use bytemuck::{Pod, Zeroable};
use wgpu;

use crate::effect_registry::EffectRegistry;
use crate::scene::SourceId;

/// Uniform buffer layout for effect shaders (48 bytes, std140).
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct EffectUniforms {
    pub time: f32,
    pub _pad: f32,
    pub resolution: [f32; 2],
    pub params: [f32; 8],
}

/// Resolved effect ready for rendering: effect ID + populated param values.
pub struct ResolvedEffect {
    pub effect_id: String,
    pub params: [f32; 8],
}

/// Ping-pong texture pair for a source's effect chain.
pub struct TempTextures {
    pub texture_a: wgpu::Texture,
    pub view_a: wgpu::TextureView,
    pub texture_b: wgpu::Texture,
    pub view_b: wgpu::TextureView,
    pub bind_group_a: wgpu::BindGroup,
    pub bind_group_b: wgpu::BindGroup,
    pub size: (u32, u32),
}

pub struct EffectPipeline {
    compiled: HashMap<String, wgpu::RenderPipeline>,
    pipeline_layout: wgpu::PipelineLayout,
    texture_bind_group_layout: wgpu::BindGroupLayout,
    uniform_bind_group_layout: wgpu::BindGroupLayout,
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    sampler: wgpu::Sampler,
    target_format: wgpu::TextureFormat,
    /// Fullscreen quad vertex shader prepended to every effect fragment shader.
    vertex_preamble: String,
    /// Per-source temp textures for effect chain processing.
    pub temp_textures: HashMap<SourceId, TempTextures>,
}
```

The `new()` constructor, `compile_shader()`, `get_or_compile()`, `invalidate_user_shaders()`, `ensure_temp_textures()`, and `apply_chain()` methods follow. This is a large file — see full implementation details below.

- [ ] **Step 2: Implement new() constructor**

Creates bind group layouts, uniform buffer, sampler, and pipeline layout. Same pattern as `TransitionPipeline::new()` but with 2 bind groups (texture+sampler, uniforms) instead of 3.

```rust
impl EffectPipeline {
    pub fn new(
        device: &wgpu::Device,
        target_format: wgpu::TextureFormat,
    ) -> Self {
        // Texture + sampler bind group layout
        let texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("effect_texture_bgl"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });

        // Uniform bind group layout
        let uniform_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("effect_uniform_bgl"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("effect_uniform_buffer"),
            size: std::mem::size_of::<EffectUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("effect_uniform_bg"),
            layout: &uniform_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("effect_pipeline_layout"),
            bind_group_layouts: &[&texture_bind_group_layout, &uniform_bind_group_layout],
            push_constant_ranges: &[],
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("effect_sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let vertex_preamble = r#"
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VertexOutput {
    var out: VertexOutput;
    let x = f32((vi & 1u) * 2u) - 1.0;
    let y = 1.0 - f32((vi >> 1u) * 2u);
    out.position = vec4<f32>(x, y, 0.0, 1.0);
    out.uv = vec2<f32>(f32(vi & 1u), f32(vi >> 1u));
    return out;
}
"#
        .to_string();

        Self {
            compiled: HashMap::new(),
            pipeline_layout,
            texture_bind_group_layout,
            uniform_bind_group_layout,
            uniform_buffer,
            uniform_bind_group,
            sampler,
            target_format,
            vertex_preamble,
            temp_textures: HashMap::new(),
        }
    }
}
```

- [ ] **Step 3: Implement compile_shader, get_or_compile, invalidate_user_shaders**

```rust
    /// Compile an effect shader and cache the pipeline.
    pub fn compile_shader(
        &mut self,
        device: &wgpu::Device,
        id: &str,
        wgsl_source: &str,
    ) -> bool {
        let full_source = format!("{}\n{}", self.vertex_preamble, wgsl_source);

        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(id),
            source: wgpu::ShaderSource::Wgsl(full_source.into()),
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(id),
            layout: Some(&self.pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: self.target_format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                strip_index_format: None,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: Default::default(),
            multiview: None,
            cache: None,
        });

        self.compiled.insert(id.to_string(), pipeline);
        true
    }

    /// Get a compiled pipeline, compiling on first use. Returns None if shader not in registry.
    pub fn get_or_compile(
        &mut self,
        device: &wgpu::Device,
        id: &str,
        registry: &EffectRegistry,
    ) -> Option<&wgpu::RenderPipeline> {
        if !self.compiled.contains_key(id) {
            if let Some(def) = registry.get(id) {
                if !self.compile_shader(device, id, &def.shader_source) {
                    log::warn!("Failed to compile effect shader: {id}");
                    return None;
                }
            } else {
                return None;
            }
        }
        self.compiled.get(id)
    }

    /// Clear all compiled shaders (call when registry changes).
    pub fn invalidate_user_shaders(&mut self) {
        self.compiled.clear();
    }
```

- [ ] **Step 4: Implement ensure_temp_textures and create_bind_group_for_view**

```rust
    /// Ensure ping-pong temp textures exist for a source at the given size.
    pub fn ensure_temp_textures(
        &mut self,
        device: &wgpu::Device,
        source_id: SourceId,
        size: (u32, u32),
    ) {
        let needs_create = self
            .temp_textures
            .get(&source_id)
            .map_or(true, |t| t.size != size);

        if needs_create {
            let create_tex = |label: &str| -> (wgpu::Texture, wgpu::TextureView) {
                let tex = device.create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d {
                        width: size.0,
                        height: size.1,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: self.target_format,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                        | wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                });
                let view = tex.create_view(&Default::default());
                (tex, view)
            };

            let (texture_a, view_a) = create_tex("effect_temp_a");
            let (texture_b, view_b) = create_tex("effect_temp_b");

            let bind_group_a = self.create_bind_group_for_view(device, &view_a);
            let bind_group_b = self.create_bind_group_for_view(device, &view_b);

            self.temp_textures.insert(
                source_id,
                TempTextures {
                    texture_a,
                    view_a,
                    texture_b,
                    view_b,
                    bind_group_a,
                    bind_group_b,
                    size,
                },
            );
        }
    }

    /// Create a texture bind group for a given texture view + the shared sampler.
    pub fn create_bind_group_for_view(
        &self,
        device: &wgpu::Device,
        view: &wgpu::TextureView,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("effect_tex_bg"),
            layout: &self.texture_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        })
    }

    /// Remove temp textures for a source (e.g. when effects are cleared).
    pub fn remove_temp_textures(&mut self, source_id: SourceId) {
        self.temp_textures.remove(&source_id);
    }
```

- [ ] **Step 5: Implement apply_chain**

```rust
    /// Run the full effect chain for a source. Returns the texture view to use
    /// for compositing (either the final temp texture, or the original source
    /// texture if no effects are active).
    ///
    /// `source_bind_group` is the bind group for the original source texture + sampler.
    pub fn apply_chain<'a>(
        &'a mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        source_id: SourceId,
        source_bind_group: &wgpu::BindGroup,
        source_size: (u32, u32),
        effects: &[ResolvedEffect],
        time: f32,
        registry: &EffectRegistry,
    ) -> Option<&'a wgpu::BindGroup> {
        if effects.is_empty() {
            return None; // No effects — caller uses original source texture
        }

        self.ensure_temp_textures(device, source_id, source_size);
        let temps = self.temp_textures.get(&source_id)?;

        // Expand blur into two passes (horizontal + vertical)
        let mut expanded_passes: Vec<(&str, [f32; 8])> = Vec::new();
        for effect in effects {
            if effect.effect_id == "blur" {
                // Horizontal pass
                let mut h_params = effect.params;
                h_params[1] = 0.0; // direction = horizontal
                expanded_passes.push(("blur", h_params));
                // Vertical pass
                let mut v_params = effect.params;
                v_params[1] = 1.0; // direction = vertical
                expanded_passes.push(("blur", v_params));
            } else {
                expanded_passes.push((&effect.effect_id, effect.params));
            }
        }

        let mut read_from_a = false; // false = read source, true = read A
        let mut last_wrote_a = false;

        for (pass_idx, (effect_id, params)) in expanded_passes.iter().enumerate() {
            let Some(pipeline) = self.get_or_compile(device, effect_id, registry) else {
                continue; // Skip unknown effects
            };

            // Write uniforms
            let uniforms = EffectUniforms {
                time,
                _pad: 0.0,
                resolution: [source_size.0 as f32, source_size.1 as f32],
                params: *params,
            };
            queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));

            // Determine input bind group and output target
            let (input_bg, target_view, writes_to_a) = if pass_idx == 0 {
                // First pass reads from source
                (source_bind_group, &temps.view_a, true)
            } else if last_wrote_a {
                // Read from A, write to B
                (&temps.bind_group_a, &temps.view_b, false)
            } else {
                // Read from B, write to A
                (&temps.bind_group_b, &temps.view_a, true)
            };

            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("effect_pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: target_view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    ..Default::default()
                });

                pass.set_pipeline(pipeline);
                pass.set_bind_group(0, input_bg, &[]);
                pass.set_bind_group(1, &self.uniform_bind_group, &[]);
                pass.draw(0..4, 0..1);
            }

            last_wrote_a = writes_to_a;
        }

        // Return the bind group for the texture that was last written to
        if last_wrote_a {
            Some(&self.temp_textures.get(&source_id)?.bind_group_a)
        } else {
            Some(&self.temp_textures.get(&source_id)?.bind_group_b)
        }
    }
```

- [ ] **Step 6: Add to SharedGpuState**

In `src/renderer/mod.rs`, add `pub mod effect_pipeline;` and add the field to `SharedGpuState`:

```rust
pub effect_pipeline: effect_pipeline::EffectPipeline,
```

Initialize in the constructor:

```rust
effect_pipeline: effect_pipeline::EffectPipeline::new(
    &device,
    wgpu::TextureFormat::Rgba8UnormSrgb,
),
```

- [ ] **Step 7: Verify**

Run: `cargo build`
Expected: Compiles.

- [ ] **Step 8: Commit**

```bash
git add src/renderer/effect_pipeline.rs src/renderer/mod.rs
git commit -m "feat: add EffectPipeline GPU manager with multi-pass chain support"
```

---

### Task 5: Compositor integration — run effects before compositing

**Files:**
- Modify: `src/renderer/compositor.rs`

- [ ] **Step 1: Modify compose_to to accept EffectPipeline and registry**

Update the `compose_to` method signature to optionally run effects. In the per-source loop, before rendering the quad, check if the source has effects. If so, run `effect_pipeline.apply_chain()` and use the result bind group instead of the source's original bind group.

The key change in the source rendering loop inside `compose_to`:

```rust
// For each visible source:
let bind_group_to_use = if !source.effects.is_empty() {
    // Run effect chain, get post-effect texture bind group
    effect_pipeline.apply_chain(
        device, queue, encoder, source.id,
        &layer.bind_group, layer.size,
        &source.effects, time, registry,
    ).unwrap_or(&layer.bind_group)
} else {
    &layer.bind_group
};
// Then use bind_group_to_use instead of layer.bind_group in the render pass
```

This requires adding `effects: Vec<ResolvedEffect>` to the `ResolvedSource` struct (or equivalent) that `compose_to` receives. The `ResolvedSource` is constructed in `main.rs` when resolving the active scene — add effect resolution there.

- [ ] **Step 2: Add effects field to ResolvedSource**

Find where `ResolvedSource` is defined (likely in `compositor.rs` or the types used by `compose_to`) and add:

```rust
pub effects: Vec<crate::renderer::effect_pipeline::ResolvedEffect>,
```

- [ ] **Step 3: Resolve effects in main.rs**

Where sources are resolved for the active scene (building the `ResolvedSource` array), add effect resolution:

```rust
// For each scene source:
let effect_chain = scene_source.resolve_effects(&lib_source);
let resolved_effects: Vec<ResolvedEffect> = effect_chain
    .iter()
    .filter(|e| e.enabled)
    .filter_map(|e| {
        let def = app_state.effect_registry.get(&e.effect_id)?;
        let mut params = [0.0f32; 8];
        for (i, param_def) in def.params.iter().enumerate().take(8) {
            params[i] = e.params
                .get(&param_def.name)
                .copied()
                .unwrap_or(param_def.default);
        }
        Some(ResolvedEffect {
            effect_id: e.effect_id.clone(),
            params,
        })
    })
    .collect();
```

- [ ] **Step 4: Wire up EffectRegistry in AppState and main.rs initialization**

In `src/state.rs`, add to `AppState`:
```rust
pub effect_registry: crate::effect_registry::EffectRegistry,
pub effect_registry_changed: bool,
pub last_effect_scan: std::time::Instant,
```

In `main.rs` initialization, alongside the transition registry:
```rust
settings::seed_builtin_effects();
let effect_registry = crate::effect_registry::EffectRegistry::scan(&settings::effects_dir());
```

Add the rescan block (same pattern as transitions):
```rust
if app_state.last_effect_scan.elapsed() >= std::time::Duration::from_secs(2) {
    app_state.last_effect_scan = std::time::Instant::now();
    if app_state.effect_registry.rescan(&crate::settings::effects_dir()) {
        app_state.effect_registry_changed = true;
    }
}
```

And GPU invalidation when changed:
```rust
if effect_registry_changed {
    gpu.effect_pipeline.invalidate_user_shaders();
}
```

- [ ] **Step 5: Verify**

Run: `cargo build && cargo run`
Expected: App runs. Sources without effects render as before. No visual changes yet (no effects assigned).

- [ ] **Step 6: Commit**

```bash
git add src/renderer/compositor.rs src/state.rs src/main.rs
git commit -m "feat: integrate effect chain into compositor pipeline"
```

---

### Task 6: Properties panel — effect chain editor UI

**Files:**
- Modify: `src/ui/properties_panel.rs`

- [ ] **Step 1: Add draw_effects_section function**

Add a new function that renders the EFFECTS section in the Properties panel. Position it after `draw_opacity_section` and before `draw_source_properties`.

The section includes:
- Section header with override dot + "EFFECTS" label + "+ Add" button
- For each effect in the chain: a collapsible card with toggle, name, expand/collapse, remove
- When expanded: sliders for each parameter from the EffectRegistry

Key UI elements:
- Use `egui::CollapsingHeader` or manual expand/collapse state
- Use the existing `toggle_switch` widget for enable/disable
- Use `egui::Slider` for each parameter with the ParamDef's min/max/default
- Store expand state in egui temp data
- Add effect via popup menu listing all effects from the registry

- [ ] **Step 2: Wire into the main draw function**

In the `draw` function, add between opacity and source properties:

```rust
ui.add_space(12.0);
changed |= draw_effects_section(ui, state, selected_id, lib_idx, in_active_scene);
```

- [ ] **Step 3: Implement the add-effect popup**

"+ Add" button opens a popup listing all effects from `state.effect_registry.all()`. Clicking one appends a new `EffectInstance` with default params to the source's effect chain.

- [ ] **Step 4: Implement drag-to-reorder**

Use the same drag-to-reorder pattern from `sources_panel.rs` — `egui::DragAndDrop` payload with an effect index, animated Y offsets.

- [ ] **Step 5: Verify**

Run: `cargo run`
Expected: EFFECTS section appears in Properties panel. Can add effects, toggle them, adjust parameters, reorder, and remove. Effects render in real-time in the preview.

- [ ] **Step 6: Commit**

```bash
git add src/ui/properties_panel.rs
git commit -m "feat: add effect chain editor to Properties panel"
```

---

### Task 7: End-to-end testing and polish

**Files:**
- Modify: various (bug fixes found during testing)

- [ ] **Step 1: Test the full flow**

1. Launch app
2. Add a display source to a scene
3. Open Properties panel
4. Add "Circle Crop" effect — verify circle mask appears in preview
5. Adjust radius/feather sliders — verify real-time update
6. Add "Color Correction" effect — verify it chains (circle crop + color adjust)
7. Reorder effects — verify rendering order changes
8. Disable an effect via toggle — verify it's bypassed
9. Remove an effect — verify it disappears
10. Add "Blur" effect — verify blur renders (two-pass)
11. Add "Chroma Key" with a camera — verify green screen removal
12. Restart app — verify effect chain persists in TOML
13. Drop a custom `.wgsl` file in the effects directory — verify it appears in the add menu within 2 seconds

- [ ] **Step 2: Test scene overrides**

1. Set effects on a source in the library
2. Override effects in a specific scene
3. Verify override dot appears
4. Reset override — verify library defaults restored

- [ ] **Step 3: Fix any issues found**

Address bugs, visual glitches, or performance issues discovered during testing.

- [ ] **Step 4: Final commit**

```bash
git add -A
git commit -m "fix: shader effects end-to-end polish and bug fixes"
```
