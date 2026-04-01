# Shader Effects System — Design Spec

**Date:** 2026-04-01
**Scope:** Per-source shader effect chain — user-loadable WGSL effects with configurable parameters, multi-pass GPU pipeline, inline Properties panel editor, and 6 built-in effects.

---

## 1. Overview

A unified per-source shader effect system that supports both alpha masks (circle crop, rounded corners, gradient fade) and color/effect filters (color correction, chroma key, blur). Effects are chainable — each source can have an ordered list of effects, processed sequentially via multi-pass rendering.

Effects are standalone WGSL shader files with header metadata declaring name, description, and configurable parameters. Both built-in effects (shipped with the app) and user-loaded effects (dropped into a directory) are supported, with live-reload for development.

---

## 2. Render Pipeline

### Multi-pass effect chain

Sources with effects get additional render passes before the existing compositor pass:

```
source_texture
  → Effect Pass 1 (e.g. Circle Crop)   → temp_texture_A
  → Effect Pass 2 (e.g. Color Correct) → temp_texture_B
  → Compositor Pass (existing, unchanged — alpha-over blend onto canvas)
```

Sources without effects skip directly to the compositor (zero overhead).

### Ping-pong temp textures

Each source that has active effects uses two temp textures (A and B) at the source's texture resolution. Effects alternate between reading from A and writing to B, then reading from B and writing to A. The final output becomes the input to the compositor pass.

Temp textures are lazily allocated when a source first has effects, and freed when all effects are removed. They resize when the source texture resolution changes.

### Integration with compositor

The compositor's `compose_to()` method currently iterates visible sources and renders each one. For sources with effects:

1. Run the effect chain on the source's texture, producing a final post-effect texture
2. Pass that texture (instead of the raw source texture) to the existing compositor pass
3. The compositor pass applies transform, opacity, and alpha-over blend as before — unchanged

The compositor shader (`compositor.wgsl`) is not modified. Effects run before it.

---

## 3. Effect WGSL Shader Format

### Header metadata

```wgsl
// @name: Circle Crop
// @author: Lodestone
// @description: Crops source to a circle with soft edge
// @param: center_x 0.5 0.0 1.0
// @param: center_y 0.5 0.0 1.0
// @param: radius 0.4 0.0 1.0
// @param: feather 0.02 0.0 0.2
```

- `@name:` — display name in UI (required)
- `@author:` — optional credit
- `@description:` — tooltip text (optional)
- `@param:` — `name default min max` — one line per parameter, up to 8. Name is used as the UI label and the key in the params HashMap. Default/min/max are f32 values.

### Bindings

```wgsl
@group(0) @binding(0) var t_input: texture_2d<f32>;
@group(0) @binding(1) var s_input: sampler;
@group(1) @binding(0) var<uniform> u: Uniforms;
```

- **group(0):** Input texture + linear sampler. For the first effect in the chain, this is the source texture. For subsequent effects, this is the previous effect's output.
- **group(1):** Uniform buffer with time and parameters.

### Uniform struct

```wgsl
struct Uniforms {
    time: f32,             // Elapsed seconds since app start (for animated effects)
    _pad: f32,             // Alignment
    resolution: vec2<f32>, // Input texture resolution (width, height)
    params: array<f32, 8>, // Named parameters mapped by declaration order
}
```

Total size: 48 bytes (std140 aligned).

Parameters are mapped by their declaration order in the header: the first `@param:` maps to `u.params[0]`, the second to `u.params[1]`, etc.

### Vertex shader

The vertex shader is provided by the engine — effect authors only write `fs_main`. The engine prepends a standard fullscreen-quad vertex shader to each effect shader:

```wgsl
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
```

---

## 4. Key Components

### EffectRegistry

Scans the effects directory (`config_dir/effects/`) for `.wgsl` files. Parses header comments to extract name, description, and parameter definitions. Seeds built-in effects on startup. Supports periodic rescan for live-reload (same 2-second poll as transitions).

Same pattern as `TransitionRegistry` in `src/transition_registry.rs`.

```rust
pub struct EffectDef {
    pub id: String,           // filename stem (e.g. "circle_crop")
    pub name: String,         // @name value
    pub author: Option<String>,
    pub description: Option<String>,
    pub params: Vec<ParamDef>,
    pub shader_source: String, // full WGSL content
    pub is_builtin: bool,
}

pub struct ParamDef {
    pub name: String,
    pub default: f32,
    pub min: f32,
    pub max: f32,
}

pub struct EffectRegistry {
    effects: HashMap<String, EffectDef>,
}
```

### EffectPipeline

GPU pipeline manager living on `SharedGpuState`. Lazy-compiles WGSL shaders into `wgpu::RenderPipeline` instances on first use. Caches compiled pipelines. Manages ping-pong temp textures per source. Provides `apply_chain()` method that runs the full effect chain for a source.

```rust
pub struct EffectPipeline {
    compiled: HashMap<String, wgpu::RenderPipeline>,
    pipeline_layout: wgpu::PipelineLayout,
    sampler: wgpu::Sampler,
    target_format: wgpu::TextureFormat, // Rgba8UnormSrgb
}
```

The `apply_chain()` method:

```rust
pub fn apply_chain(
    &mut self,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    encoder: &mut wgpu::CommandEncoder,
    source_texture_view: &wgpu::TextureView,
    source_size: (u32, u32),
    effects: &[ResolvedEffect],  // effect_id + param values
    time: f32,
    registry: &EffectRegistry,
    temp_textures: &mut TempTextures,
) -> &wgpu::TextureView  // returns final output texture view
```

### TempTextures

Per-source pair of ping-pong textures. Stored alongside `SourceLayer` in the compositor, lazily allocated.

```rust
pub struct TempTextures {
    texture_a: wgpu::Texture,
    view_a: wgpu::TextureView,
    texture_b: wgpu::Texture,
    view_b: wgpu::TextureView,
    size: (u32, u32),
}
```

### EffectInstance (data model)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffectInstance {
    pub effect_id: String,
    #[serde(default)]
    pub params: HashMap<String, f32>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}
```

Added to `LibrarySource`:
```rust
#[serde(default)]
pub effects: Vec<EffectInstance>,
```

Added to `SourceOverrides`:
```rust
#[serde(default, skip_serializing_if = "Option::is_none")]
pub effects: Option<Vec<EffectInstance>>,
```

Resolution: scene override replaces the entire chain. `resolve_effects()` returns the override chain if present, otherwise the library chain.

---

## 5. Built-in Effects (6)

### Alpha/Shape masks

**circle_crop** — Crops source to a circle. Params: center_x (0.5), center_y (0.5), radius (0.4), feather (0.02).

**rounded_corners** — Rounds corners of the source rect. Params: radius (0.05), feather (0.005).

**gradient_fade** — Fades the source alpha along a direction. Params: direction (0.0 = left→right, 0.25 = top→bottom, 0.5 = right→left, 0.75 = bottom→top), start (0.0), end (1.0).

### Color/Effect filters

**color_correction** — Adjusts brightness, contrast, saturation. Params: brightness (0.0, -1.0, 1.0), contrast (1.0, 0.0, 3.0), saturation (1.0, 0.0, 3.0).

**chroma_key** — Removes a color (green screen). Params: key_r (0.0), key_g (1.0), key_b (0.0), threshold (0.3), smoothness (0.1).

**blur** — Gaussian-approximation blur via two-pass (horizontal then vertical). Params: radius (5.0, 0.0, 50.0). Note: blur requires two internal passes (H then V) within a single "effect" — the EffectPipeline handles this specially by running the blur shader twice with a direction uniform.

---

## 6. Properties Panel UI

The effect chain appears as an "EFFECTS" section in the Properties panel, positioned after OPACITY and before SOURCE.

### Effect cards

Each effect is a card with:
- **Drag handle** (left) — grip dots for reorder
- **Toggle switch** — enables/disables the effect (bypass)
- **Name label** — from `@name` in shader header
- **Expand/collapse arrow** — toggles parameter visibility
- **Remove button** (✕) — removes effect from chain

### Parameters

When expanded, each `@param` renders as a labeled slider with the param name, a drag value input, and the current value. Min/max from the header constrain the slider range.

### Accordion behavior

Only one effect is expanded at a time. Expanding one collapses the previously expanded one.

### Add effect

"+ Add" button at the section header opens a popup listing all effects from the EffectRegistry. Clicking one appends it to the chain with default parameter values.

### Reorder

Drag-to-reorder within the effect list changes the processing order. Same animated reorder pattern as the source list in sources_panel.rs.

### Override indicator

Override dot on the "EFFECTS" section header when the scene has an effects override. Right-click to reset to library defaults (same pattern as transform/opacity override dots).

---

## 7. Directory Structure & Live Reload

Effects directory: `config_dir/effects/` (alongside `config_dir/transitions/`).

Built-in effects are seeded to this directory on first launch (same as transitions). User effects are `.wgsl` files placed in this directory.

Live reload: EffectRegistry rescans every 2 seconds (same poll as transitions). When a shader file changes, the compiled pipeline for that effect is invalidated and recompiled on next use.

Helper function: `settings::effects_dir()` returns the effects directory path.

---

## 8. Files Changed / Created

| File | Change |
|------|--------|
| `src/scene.rs` | Add `EffectInstance`, `effects` field on `LibrarySource` and `SourceOverrides`, `resolve_effects()` |
| `src/effect_registry.rs` | New — `EffectRegistry`, `EffectDef`, `ParamDef`, header parser, directory scanner |
| `src/renderer/effect_pipeline.rs` | New — `EffectPipeline`, `TempTextures`, shader compilation, `apply_chain()` |
| `src/renderer/shaders/effect_*.wgsl` | New — 6 built-in effect shaders |
| `src/renderer/compositor.rs` | Integrate effect chain into `compose_to()` — run effects before compositor pass |
| `src/renderer/mod.rs` | Add `EffectPipeline` to `SharedGpuState` |
| `src/ui/properties_panel.rs` | Add EFFECTS section with card UI, sliders, drag-to-reorder, add/remove |
| `src/settings.rs` | Add `effects_dir()` helper |
| `src/state.rs` | Add `EffectRegistry` to `AppState`, `effect_registry_changed` flag |
| `src/main.rs` | Initialize EffectRegistry, periodic rescan, seed built-ins, invalidate on change |
