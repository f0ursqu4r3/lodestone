# User-Loadable Transition Shaders

## Summary

Replace the compile-time-embedded fade shader with a runtime-loaded transition system. Transitions are `.wgsl` files stored in `<config_dir>/lodestone/transitions/`. Users can add custom transitions by dropping shader files into this directory. Each shader follows a standard uniform interface and declares metadata via comment headers.

## Current State

- Single `transition_fade.wgsl` embedded via `include_str!()` in `renderer/transition.rs`
- `TransitionType` is a two-variant enum: `Cut`, `Fade`
- `TransitionSettings` stores `default_type: TransitionType` and `default_duration_ms: u32`
- `SceneTransitionOverride` allows per-scene type and duration overrides
- `TransitionPipeline` owns a single `wgpu::RenderPipeline` built at init time
- UI in `scenes_panel.rs` uses a segmented Fade/Cut toggle and duration field

## Design

### Transition Directory

All transitions live in `<config_dir>/lodestone/transitions/`. On first launch (directory doesn't exist), the app seeds it with built-in shaders (`fade.wgsl`, `cut` is handled in code as a zero-duration instant switch — no shader needed).

Users add custom transitions by placing `.wgsl` files in this directory. The app scans the directory at startup and provides a rescan/reload action in the UI.

Path helper in `settings.rs`:

```rust
pub fn transitions_dir() -> PathBuf {
    config_dir().join("transitions")
}
```

### Shader Interface Contract

Every transition shader receives the same bind group layout:

| Group | Binding | Type | Description |
|-------|---------|------|-------------|
| 0 | 0 | `texture_2d<f32>` | Outgoing scene texture (`t_from`) |
| 0 | 1 | `sampler` | Outgoing scene sampler (`s_from`) |
| 1 | 0 | `texture_2d<f32>` | Incoming scene texture (`t_to`) |
| 1 | 1 | `sampler` | Incoming scene sampler (`s_to`) |
| 2 | 0 | `uniform` | `TransitionUniforms` struct |

The uniform struct is extended:

```wgsl
struct TransitionUniforms {
    progress: f32,       // 0.0 = fully "from", 1.0 = fully "to"
    time: f32,           // seconds since transition start
    _pad0: f32,
    _pad1: f32,
    color: vec4<f32>,    // user-picked accent color
    from_color: vec4<f32>, // e.g. dip-to-black: this is black
    to_color: vec4<f32>,   // e.g. fade-from-white: this is white
};
```

Rust-side struct (48 bytes, 16-byte aligned):

```rust
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct TransitionUniforms {
    pub progress: f32,
    pub time: f32,
    pub _pad: [f32; 2],
    pub color: [f32; 4],
    pub from_color: [f32; 4],
    pub to_color: [f32; 4],
}
```

Entry points: `vs_main` (vertex), `fs_main` (fragment). The vertex shader is always a fullscreen triangle-strip quad — shaders may copy the standard one or provide their own.

### Comment Header Metadata

Metadata is parsed from `// @key: value` lines at the top of the file. Parsing stops at the first non-comment, non-blank line.

| Key | Required | Description |
|-----|----------|-------------|
| `@name` | No | Display name in UI. Falls back to file stem, title-cased (`radial_wipe` → `Radial Wipe`) |
| `@author` | No | Creator attribution. Falls back to empty string |
| `@description` | No | Tooltip/description text |
| `@params` | No | Comma-separated list of color uniforms to expose in UI: `color`, `from_color`, `to_color`. No `@params` line = no color pickers shown |

Example header:

```wgsl
// @name: Dip to Color
// @author: Lodestone
// @description: Fades to a solid color at the midpoint, then reveals the incoming scene
// @params: color
```

### Transition Registry

New module: `src/transition_registry.rs`

```rust
pub struct TransitionDef {
    pub id: String,           // file stem, e.g. "fade", "dip_to_color"
    pub name: String,         // from @name or title-cased file stem
    pub author: String,       // from @author or ""
    pub description: String,  // from @description or ""
    pub params: Vec<TransitionParam>, // parsed from @params
    pub shader_source: String, // raw .wgsl content
}

pub enum TransitionParam {
    Color,
    FromColor,
    ToColor,
}

pub struct TransitionRegistry {
    transitions: Vec<TransitionDef>,
}
```

The registry:
- Scans the transitions directory and parses each `.wgsl` file
- Skips files that fail to parse headers (logs a warning)
- Provides lookup by ID (file stem)
- Always includes a synthetic "Cut" entry (no shader, handled as instant switch in code)
- Provides `rescan()` to reload from disk

Shader compilation into `wgpu::ShaderModule` happens lazily in the renderer when a transition is first used, not at scan time. This keeps the registry independent of GPU state. Invalid WGSL that fails compilation logs a warning and falls back to the built-in fade.

### TransitionType Replacement

The current `TransitionType` enum (`Cut`, `Fade`) is replaced with a string ID:

```rust
// In transition.rs
pub const TRANSITION_CUT: &str = "cut";
pub const TRANSITION_FADE: &str = "fade";
```

`TransitionSettings` changes:

```rust
pub struct TransitionSettings {
    pub default_transition: String,  // was default_type: TransitionType
    pub default_duration_ms: u32,
    pub default_colors: TransitionColors,  // new
}

pub struct TransitionColors {
    pub color: [f32; 4],
    pub from_color: [f32; 4],
    pub to_color: [f32; 4],
}
```

`SceneTransitionOverride` changes similarly:

```rust
pub struct SceneTransitionOverride {
    pub transition: Option<String>,  // was transition_type: Option<TransitionType>
    pub duration_ms: Option<u32>,
    pub colors: Option<TransitionColors>,
}
```

### TransitionPipeline Changes

`TransitionPipeline` becomes capable of holding multiple compiled pipelines:

```rust
pub struct TransitionPipeline {
    compiled: HashMap<String, wgpu::RenderPipeline>,
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    pipeline_layout: wgpu::PipelineLayout,  // shared, since all shaders use same bind group layout
    target_format: wgpu::TextureFormat,
}
```

- `pipeline_layout` is shared across all transition shaders (same bind groups)
- `uniform_buffer` and `uniform_bind_group` are shared (same struct for all)
- Pipelines are compiled on first use via `get_or_compile(id, registry, device)`
- The `blend()` method takes a transition ID string instead of being hardcoded to fade

### Built-in Shaders Seeded on First Launch

On first launch, `transitions_dir()` doesn't exist. The app creates it and writes the built-in shaders:

- `fade.wgsl` — the existing crossfade (currently in `src/renderer/shaders/`)
- Future built-ins (wipe, slide, etc.) added the same way

The source for built-in shaders remains in `src/renderer/shaders/` and is embedded via `include_str!()` for seeding. After seeding, the runtime loads from disk like any other transition. This means users can modify or replace the built-ins.

### UI Changes

**Transition bar** (`scenes_panel.rs`): The Fade/Cut segmented toggle is replaced with a dropdown populated from the registry. The duration field stays. Below the dropdown, color pickers appear based on the selected transition's `@params`.

**Per-scene override** (context menu): Same dropdown + color pickers, with an "Inherit Default" option.

### Settings Persistence (TOML)

```toml
[transitions]
default_transition = "fade"
default_duration_ms = 300

[transitions.default_colors]
color = [0.0, 0.0, 0.0, 1.0]
from_color = [0.0, 0.0, 0.0, 1.0]
to_color = [0.0, 0.0, 0.0, 1.0]
```

Backwards compatibility: if `default_type = "Fade"` is found (old format), map it to `default_transition = "fade"`. Missing `default_colors` gets black defaults.

### Error Handling

- Missing transitions directory → create and seed built-ins
- Unparseable `.wgsl` file → log warning, skip (don't add to registry)
- WGSL compilation failure at render time → log warning, fall back to fade
- Selected transition ID not in registry (e.g. user deleted the file) → fall back to fade
- Empty transitions directory → seed built-ins again

### File Organization

| File | Change |
|------|--------|
| `src/transition.rs` | Replace `TransitionType` enum with string IDs, add `TransitionColors`, update `TransitionSettings` and `SceneTransitionOverride` |
| `src/transition_registry.rs` | **New** — `TransitionDef`, `TransitionRegistry`, header parser, directory scanner |
| `src/renderer/transition.rs` | Extend `TransitionUniforms`, make `TransitionPipeline` multi-shader with lazy compilation |
| `src/renderer/shaders/transition_fade.wgsl` | Update uniform struct to match extended layout |
| `src/settings.rs` | Add `transitions_dir()` helper |
| `src/ui/scenes_panel.rs` | Replace segmented toggle with dropdown, add color pickers |
| `src/state.rs` | Add `TransitionRegistry` to `AppState` |
| `src/main.rs` | Initialize registry at startup, seed built-ins on first launch |

### Testing

- **Unit tests** for header parser (all combinations: full header, partial, no header, malformed)
- **Unit tests** for title-case fallback from file stem
- **Unit tests** for registry scan (tempdir with valid/invalid `.wgsl` files)
- **Unit tests** for backwards-compatible TOML deserialization (`TransitionType::Fade` → `"fade"`)
- **Unit tests** for `TransitionColors` default values
- **Unit tests** for `resolve_transition` with new string-based IDs and colors
- Existing transition tests updated for new types
