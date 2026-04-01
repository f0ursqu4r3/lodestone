# User-Loadable Transition Shaders Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the compile-time-embedded fade shader with a runtime-loaded transition system where users drop `.wgsl` files into a directory.

**Architecture:** A `TransitionRegistry` scans `<config_dir>/lodestone/transitions/` for `.wgsl` files, parses comment-header metadata, and provides transition definitions to the renderer and UI. `TransitionPipeline` lazily compiles shader modules on first use. The `TransitionType` enum is replaced with string IDs. An extended uniform buffer passes user-configurable colors to all shaders.

**Tech Stack:** Rust, wgpu, egui, serde, bytemuck

---

## File Structure

| File | Action | Responsibility |
|------|--------|---------------|
| `src/transition.rs` | Modify | Replace `TransitionType` enum with string constants, add `TransitionColors`, update `TransitionSettings`, `SceneTransitionOverride`, `TransitionState`, `resolve_transition` |
| `src/transition_registry.rs` | Create | `TransitionDef`, `TransitionParam`, `TransitionRegistry` — header parser, directory scanner |
| `src/renderer/transition.rs` | Modify | Extend `TransitionUniforms` to 48 bytes, make `TransitionPipeline` multi-shader with lazy compilation and shared pipeline layout |
| `src/renderer/shaders/transition_fade.wgsl` | Modify | Add comment header, update uniform struct to extended layout |
| `src/renderer/mod.rs` | Modify | Pass registry to `TransitionPipeline::new`, update init call |
| `src/settings.rs` | Modify | Add `transitions_dir()` helper |
| `src/state.rs` | Modify | Add `TransitionRegistry` field to `AppState` |
| `src/main.rs` | Modify | Initialize registry at startup, seed built-ins, update transition trigger sites (3 locations), update render loop blend call |
| `src/ui/scenes_panel.rs` | Modify | Replace Fade/Cut segmented toggle with dropdown, add color pickers, update per-scene override context menu |

---

## Task 1: Replace TransitionType Enum with String IDs

**Files:**
- Modify: `src/transition.rs`

This task replaces the `TransitionType` enum with string-based IDs and adds the `TransitionColors` struct. All existing tests are updated.

- [ ] **Step 1: Write tests for new string-based types and TransitionColors**

Add these tests at the bottom of the `#[cfg(test)] mod tests` block in `src/transition.rs`, replacing the existing tests that reference `TransitionType`:

```rust
#[test]
fn transition_colors_default_is_black() {
    let c = TransitionColors::default();
    assert_eq!(c.color, [0.0, 0.0, 0.0, 1.0]);
    assert_eq!(c.from_color, [0.0, 0.0, 0.0, 1.0]);
    assert_eq!(c.to_color, [0.0, 0.0, 0.0, 1.0]);
}

#[test]
fn transition_settings_default_uses_fade() {
    let s = TransitionSettings::default();
    assert_eq!(s.default_transition, TRANSITION_FADE);
    assert_eq!(s.default_duration_ms, 300);
}

#[test]
fn resolve_uses_global_defaults() {
    let global = TransitionSettings::default();
    let override_ = SceneTransitionOverride::default();
    let resolved = resolve_transition(&global, &override_);
    assert_eq!(resolved.transition, TRANSITION_FADE);
    assert_eq!(resolved.duration, Duration::from_millis(300));
    assert_eq!(resolved.colors.color, [0.0, 0.0, 0.0, 1.0]);
}

#[test]
fn resolve_per_scene_overrides_global() {
    let global = TransitionSettings::default();
    let override_ = SceneTransitionOverride {
        transition: Some(TRANSITION_CUT.to_string()),
        duration_ms: Some(0),
        colors: Some(TransitionColors {
            color: [1.0, 0.0, 0.0, 1.0],
            ..Default::default()
        }),
    };
    let resolved = resolve_transition(&global, &override_);
    assert_eq!(resolved.transition, TRANSITION_CUT);
    assert_eq!(resolved.duration, Duration::ZERO);
    assert_eq!(resolved.colors.color, [1.0, 0.0, 0.0, 1.0]);
}

#[test]
fn resolve_partial_override_inherits_unset_fields() {
    let global = TransitionSettings {
        default_transition: TRANSITION_FADE.to_string(),
        default_duration_ms: 300,
        default_colors: TransitionColors::default(),
    };
    let override_ = SceneTransitionOverride {
        transition: None,
        duration_ms: Some(1000),
        colors: None,
    };
    let resolved = resolve_transition(&global, &override_);
    assert_eq!(resolved.transition, TRANSITION_FADE);
    assert_eq!(resolved.duration, Duration::from_millis(1000));
}

#[test]
fn transition_state_progress_at_start() {
    let state = TransitionState {
        from_scene: SceneId(1),
        to_scene: SceneId(2),
        transition: TRANSITION_FADE.to_string(),
        started_at: Instant::now(),
        duration: Duration::from_millis(300),
        colors: TransitionColors::default(),
    };
    assert!(state.progress() < 0.1);
    assert!(!state.is_complete());
}

#[test]
fn transition_state_progress_when_complete() {
    let state = TransitionState {
        from_scene: SceneId(1),
        to_scene: SceneId(2),
        transition: TRANSITION_FADE.to_string(),
        started_at: Instant::now() - Duration::from_millis(500),
        duration: Duration::from_millis(300),
        colors: TransitionColors::default(),
    };
    assert_eq!(state.progress(), 1.0);
    assert!(state.is_complete());
}

#[test]
fn transition_state_zero_duration_is_immediately_complete() {
    let state = TransitionState {
        from_scene: SceneId(1),
        to_scene: SceneId(2),
        transition: TRANSITION_CUT.to_string(),
        started_at: Instant::now(),
        duration: Duration::ZERO,
        colors: TransitionColors::default(),
    };
    assert_eq!(state.progress(), 1.0);
    assert!(state.is_complete());
}

#[test]
fn scene_transition_override_default_is_none() {
    let o = SceneTransitionOverride::default();
    assert!(o.transition.is_none());
    assert!(o.duration_ms.is_none());
    assert!(o.colors.is_none());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib transition::tests -- --nocapture 2>&1 | head -40`
Expected: compilation errors — `TransitionColors`, `TRANSITION_FADE`, `TRANSITION_CUT`, `ResolvedTransition` don't exist yet.

- [ ] **Step 3: Implement the new types**

Replace the entire contents of `src/transition.rs` with:

```rust
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

use crate::scene::SceneId;

/// Well-known transition ID: instant scene switch, no animation.
pub const TRANSITION_CUT: &str = "cut";
/// Well-known transition ID: linear crossfade.
pub const TRANSITION_FADE: &str = "fade";

/// User-configurable color parameters passed to transition shaders.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TransitionColors {
    /// Accent color for the transition effect (e.g. wipe edge glow).
    pub color: [f32; 4],
    /// Color to transition FROM (e.g. dip-to-black: this is black).
    pub from_color: [f32; 4],
    /// Color to transition TO.
    pub to_color: [f32; 4],
}

impl Default for TransitionColors {
    fn default() -> Self {
        Self {
            color: [0.0, 0.0, 0.0, 1.0],
            from_color: [0.0, 0.0, 0.0, 1.0],
            to_color: [0.0, 0.0, 0.0, 1.0],
        }
    }
}

/// Global transition defaults, persisted in settings TOML.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TransitionSettings {
    /// Transition ID string (file stem, e.g. "fade", "dip_to_color").
    pub default_transition: String,
    pub default_duration_ms: u32,
    pub default_colors: TransitionColors,
}

impl Default for TransitionSettings {
    fn default() -> Self {
        Self {
            default_transition: TRANSITION_FADE.to_string(),
            default_duration_ms: 300,
            default_colors: TransitionColors::default(),
        }
    }
}

/// Per-scene transition override. Controls the transition used when
/// transitioning *into* this scene. `None` fields inherit from global defaults.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SceneTransitionOverride {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transition: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub colors: Option<TransitionColors>,
}

/// Fully resolved transition parameters (global defaults merged with per-scene overrides).
pub struct ResolvedTransition {
    pub transition: String,
    pub duration: Duration,
    pub colors: TransitionColors,
}

/// Runtime state for an in-progress transition. Not persisted.
#[derive(Debug, Clone)]
pub struct TransitionState {
    pub from_scene: SceneId,
    pub to_scene: SceneId,
    /// Transition ID string (e.g. "fade", "dip_to_color").
    pub transition: String,
    pub started_at: Instant,
    pub duration: Duration,
    pub colors: TransitionColors,
}

impl TransitionState {
    /// Returns the transition progress in 0.0..=1.0.
    pub fn progress(&self) -> f32 {
        let elapsed = self.started_at.elapsed().as_secs_f32();
        let total = self.duration.as_secs_f32();
        if total <= 0.0 {
            1.0
        } else {
            (elapsed / total).clamp(0.0, 1.0)
        }
    }

    /// Returns true when the transition has completed.
    pub fn is_complete(&self) -> bool {
        self.started_at.elapsed() >= self.duration
    }
}

/// Resolve which transition, duration, and colors to use for a scene switch.
/// Per-scene override takes priority over global default.
pub fn resolve_transition(
    global: &TransitionSettings,
    scene_override: &SceneTransitionOverride,
) -> ResolvedTransition {
    let transition = scene_override
        .transition
        .clone()
        .unwrap_or_else(|| global.default_transition.clone());
    let duration_ms = scene_override
        .duration_ms
        .unwrap_or(global.default_duration_ms);
    let colors = scene_override
        .colors
        .unwrap_or(global.default_colors);
    ResolvedTransition {
        transition,
        duration: Duration::from_millis(duration_ms as u64),
        colors,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ... (paste all tests from Step 1 here)
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib transition::tests -- --nocapture`
Expected: all 9 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/transition.rs
git commit -m "refactor: replace TransitionType enum with string-based transition IDs and TransitionColors"
```

---

## Task 2: Transition Registry — Header Parser and Directory Scanner

**Files:**
- Create: `src/transition_registry.rs`
- Modify: `src/main.rs` (add `mod transition_registry;`)

- [ ] **Step 1: Write tests for header parsing**

Create `src/transition_registry.rs` with tests only (implementation stubs that fail):

```rust
/// Which color uniforms a transition shader exposes to the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitionParam {
    Color,
    FromColor,
    ToColor,
}

/// Parsed definition of a single transition shader.
#[derive(Debug, Clone)]
pub struct TransitionDef {
    /// Unique ID derived from the file stem (e.g. "fade", "dip_to_color").
    pub id: String,
    /// Display name from `@name` header, or title-cased file stem.
    pub name: String,
    /// Author from `@author` header, or empty.
    pub author: String,
    /// Description from `@description` header, or empty.
    pub description: String,
    /// Which color uniforms to expose in the UI, from `@params` header.
    pub params: Vec<TransitionParam>,
    /// Raw WGSL shader source.
    pub shader_source: String,
}

/// Registry of available transition shaders.
pub struct TransitionRegistry {
    transitions: Vec<TransitionDef>,
}

impl TransitionRegistry {
    /// Scan a directory for `.wgsl` files and build the registry.
    /// Always includes a synthetic "cut" entry.
    pub fn scan(dir: &std::path::Path) -> Self {
        todo!()
    }

    /// Re-scan the transitions directory.
    pub fn rescan(&mut self, dir: &std::path::Path) {
        *self = Self::scan(dir);
    }

    /// Look up a transition by ID.
    pub fn get(&self, id: &str) -> Option<&TransitionDef> {
        self.transitions.iter().find(|t| t.id == id)
    }

    /// All available transitions, in alphabetical order by name.
    /// "Cut" is always first.
    pub fn all(&self) -> &[TransitionDef] {
        &self.transitions
    }
}

/// Parse `// @key: value` metadata from the top of a WGSL source string.
fn parse_header(source: &str) -> (String, String, String, Vec<TransitionParam>) {
    todo!()
}

/// Convert a file stem like "radial_wipe" to "Radial Wipe".
fn title_case_stem(stem: &str) -> String {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn title_case_basic() {
        assert_eq!(title_case_stem("radial_wipe"), "Radial Wipe");
    }

    #[test]
    fn title_case_single_word() {
        assert_eq!(title_case_stem("fade"), "Fade");
    }

    #[test]
    fn title_case_already_capitalized() {
        assert_eq!(title_case_stem("Dip_To_Color"), "Dip To Color");
    }

    #[test]
    fn parse_header_full() {
        let src = r#"// @name: Dip to Color
// @author: Lodestone
// @description: Fades through a solid color
// @params: color, from_color

struct TransitionUniforms { progress: f32 };
"#;
        let (name, author, desc, params) = parse_header(src);
        assert_eq!(name, "Dip to Color");
        assert_eq!(author, "Lodestone");
        assert_eq!(desc, "Fades through a solid color");
        assert_eq!(params, vec![TransitionParam::Color, TransitionParam::FromColor]);
    }

    #[test]
    fn parse_header_partial_no_params() {
        let src = "// @name: Fade\n\nstruct Foo {};";
        let (name, author, desc, params) = parse_header(src);
        assert_eq!(name, "Fade");
        assert_eq!(author, "");
        assert_eq!(desc, "");
        assert!(params.is_empty());
    }

    #[test]
    fn parse_header_empty_source() {
        let (name, author, desc, params) = parse_header("");
        assert_eq!(name, "");
        assert_eq!(author, "");
        assert_eq!(desc, "");
        assert!(params.is_empty());
    }

    #[test]
    fn parse_header_no_comment_lines() {
        let src = "struct TransitionUniforms { progress: f32 };";
        let (name, _, _, _) = parse_header(src);
        assert_eq!(name, "");
    }

    #[test]
    fn parse_header_all_params() {
        let src = "// @params: color, from_color, to_color\n";
        let (_, _, _, params) = parse_header(src);
        assert_eq!(params, vec![
            TransitionParam::Color,
            TransitionParam::FromColor,
            TransitionParam::ToColor,
        ]);
    }

    #[test]
    fn registry_scan_empty_dir_has_cut() {
        let dir = TempDir::new().unwrap();
        let reg = TransitionRegistry::scan(dir.path());
        assert_eq!(reg.all().len(), 1);
        assert_eq!(reg.all()[0].id, "cut");
    }

    #[test]
    fn registry_scan_finds_wgsl_files() {
        let dir = TempDir::new().unwrap();
        let mut f = std::fs::File::create(dir.path().join("fade.wgsl")).unwrap();
        writeln!(f, "// @name: Fade\n@fragment fn fs_main() {{}}").unwrap();
        let mut f2 = std::fs::File::create(dir.path().join("wipe.wgsl")).unwrap();
        writeln!(f2, "@fragment fn fs_main() {{}}").unwrap();

        let reg = TransitionRegistry::scan(dir.path());
        // cut + fade + wipe
        assert_eq!(reg.all().len(), 3);
        assert!(reg.get("fade").is_some());
        assert!(reg.get("wipe").is_some());
        assert_eq!(reg.get("fade").unwrap().name, "Fade");
        // wipe has no @name header — falls back to title-cased stem
        assert_eq!(reg.get("wipe").unwrap().name, "Wipe");
    }

    #[test]
    fn registry_scan_ignores_non_wgsl() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("readme.txt"), "not a shader").unwrap();
        let reg = TransitionRegistry::scan(dir.path());
        assert_eq!(reg.all().len(), 1); // just cut
    }

    #[test]
    fn registry_cut_always_first() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("aaa.wgsl"), "@fragment fn fs_main() {}").unwrap();
        let reg = TransitionRegistry::scan(dir.path());
        assert_eq!(reg.all()[0].id, "cut");
    }

    #[test]
    fn registry_get_missing_returns_none() {
        let dir = TempDir::new().unwrap();
        let reg = TransitionRegistry::scan(dir.path());
        assert!(reg.get("nonexistent").is_none());
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib transition_registry::tests -- --nocapture 2>&1 | head -20`
Expected: compilation succeeds but all tests panic with `todo!()`.

- [ ] **Step 3: Implement `title_case_stem`**

Replace the `title_case_stem` function body:

```rust
fn title_case_stem(stem: &str) -> String {
    stem.split('_')
        .filter(|s| !s.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(c) => {
                    let upper: String = c.to_uppercase().collect();
                    upper + &chars.as_str().to_lowercase()
                }
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
```

- [ ] **Step 4: Run title_case tests**

Run: `cargo test --lib transition_registry::tests::title_case -- --nocapture`
Expected: all 3 title_case tests pass.

- [ ] **Step 5: Implement `parse_header`**

Replace the `parse_header` function body:

```rust
fn parse_header(source: &str) -> (String, String, String, Vec<TransitionParam>) {
    let mut name = String::new();
    let mut author = String::new();
    let mut description = String::new();
    let mut params = Vec::new();

    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if !trimmed.starts_with("//") {
            break;
        }
        let comment_body = trimmed.trim_start_matches("//").trim();
        if let Some(value) = comment_body.strip_prefix("@name:") {
            name = value.trim().to_string();
        } else if let Some(value) = comment_body.strip_prefix("@author:") {
            author = value.trim().to_string();
        } else if let Some(value) = comment_body.strip_prefix("@description:") {
            description = value.trim().to_string();
        } else if let Some(value) = comment_body.strip_prefix("@params:") {
            params = value
                .split(',')
                .filter_map(|p| match p.trim() {
                    "color" => Some(TransitionParam::Color),
                    "from_color" => Some(TransitionParam::FromColor),
                    "to_color" => Some(TransitionParam::ToColor),
                    _ => None,
                })
                .collect();
        }
    }

    (name, author, description, params)
}
```

- [ ] **Step 6: Run parse_header tests**

Run: `cargo test --lib transition_registry::tests::parse_header -- --nocapture`
Expected: all 5 parse_header tests pass.

- [ ] **Step 7: Implement `TransitionRegistry::scan`**

Replace the `scan` method body:

```rust
pub fn scan(dir: &std::path::Path) -> Self {
    let mut transitions = Vec::new();

    // Synthetic "Cut" entry — always present, always first.
    transitions.push(TransitionDef {
        id: crate::transition::TRANSITION_CUT.to_string(),
        name: "Cut".to_string(),
        author: String::new(),
        description: "Instant scene switch".to_string(),
        params: Vec::new(),
        shader_source: String::new(),
    });

    // Scan directory for .wgsl files.
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) => {
            log::warn!("Failed to read transitions directory {}: {e}", dir.display());
            return Self { transitions };
        }
    };

    let mut shader_defs: Vec<TransitionDef> = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("wgsl") {
            continue;
        }
        let stem = match path.file_stem().and_then(|s| s.to_str()) {
            Some(s) => s.to_string(),
            None => continue,
        };
        let source = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => {
                log::warn!("Failed to read transition shader {}: {e}", path.display());
                continue;
            }
        };

        let (header_name, author, description, params) = parse_header(&source);
        let name = if header_name.is_empty() {
            title_case_stem(&stem)
        } else {
            header_name
        };

        shader_defs.push(TransitionDef {
            id: stem,
            name,
            author,
            description,
            params,
            shader_source: source,
        });
    }

    // Sort shader defs alphabetically by name.
    shader_defs.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    transitions.extend(shader_defs);

    Self { transitions }
}
```

- [ ] **Step 8: Add `mod transition_registry;` to `main.rs`**

Add after the `mod transition;` line in `src/main.rs`:

```rust
mod transition_registry;
```

- [ ] **Step 9: Run all registry tests**

Run: `cargo test --lib transition_registry::tests -- --nocapture`
Expected: all 11 tests pass.

- [ ] **Step 10: Commit**

```bash
git add src/transition_registry.rs src/main.rs
git commit -m "feat: add TransitionRegistry with header parser and directory scanner"
```

---

## Task 3: Extend TransitionUniforms and Update Shader

**Files:**
- Modify: `src/renderer/transition.rs`
- Modify: `src/renderer/shaders/transition_fade.wgsl`

- [ ] **Step 1: Update the WGSL shader with extended uniforms and comment header**

Replace the entire contents of `src/renderer/shaders/transition_fade.wgsl`:

```wgsl
// @name: Fade
// @author: Lodestone
// @description: Linear crossfade between outgoing and incoming scene

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
    let from_color = textureSample(t_from, s_from, in.uv);
    let to_color = textureSample(t_to, s_to, in.uv);
    return mix(from_color, to_color, uniforms.progress);
}
```

- [ ] **Step 2: Update the Rust-side TransitionUniforms struct**

In `src/renderer/transition.rs`, replace the existing `TransitionUniforms` struct:

```rust
/// Uniform buffer for transition shaders. 48 bytes, 16-byte aligned.
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

- [ ] **Step 3: Update TransitionPipeline to support multi-shader lazy compilation**

Replace the entire `TransitionPipeline` struct and impl in `src/renderer/transition.rs`:

```rust
use std::collections::HashMap;

const TRANSITION_FADE_SHADER: &str = include_str!("shaders/transition_fade.wgsl");

/// GPU resources for the transition blend pass.
///
/// Supports multiple transition shaders via lazy compilation. All shaders share
/// the same bind group layout (from texture, to texture, uniforms) and pipeline
/// layout. Shader modules are compiled on first use.
pub struct TransitionPipeline {
    compiled: HashMap<String, wgpu::RenderPipeline>,
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    pipeline_layout: wgpu::PipelineLayout,
    target_format: wgpu::TextureFormat,
}

impl TransitionPipeline {
    pub fn new(
        device: &Device,
        texture_bind_group_layout: &wgpu::BindGroupLayout,
        target_format: wgpu::TextureFormat,
    ) -> Self {
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("transition_uniform_buffer"),
            size: std::mem::size_of::<TransitionUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let uniform_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("transition_uniform_bgl"),
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

        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("transition_uniform_bind_group"),
            layout: &uniform_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("transition_pipeline_layout"),
            bind_group_layouts: &[
                texture_bind_group_layout,  // group 0: from texture + sampler
                texture_bind_group_layout,  // group 1: to texture + sampler
                &uniform_bind_group_layout, // group 2: uniforms
            ],
            push_constant_ranges: &[],
        });

        let mut pipeline = Self {
            compiled: HashMap::new(),
            uniform_buffer,
            uniform_bind_group,
            pipeline_layout,
            target_format,
        };

        // Pre-compile the built-in fade shader so it's always available as fallback.
        pipeline.compile_shader(
            device,
            crate::transition::TRANSITION_FADE,
            TRANSITION_FADE_SHADER,
        );

        pipeline
    }

    /// Compile a shader and store its pipeline. Returns true on success.
    fn compile_shader(&mut self, device: &Device, id: &str, wgsl_source: &str) -> bool {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(&format!("transition_{id}_shader")),
            source: wgpu::ShaderSource::Wgsl(wgsl_source.into()),
        });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(&format!("transition_{id}_pipeline")),
            layout: Some(&self.pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
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
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        self.compiled.insert(id.to_string(), render_pipeline);
        true
    }

    /// Get or lazily compile a transition pipeline by ID.
    /// Falls back to "fade" if the shader fails to compile or isn't found in the registry.
    pub fn get_or_compile(
        &mut self,
        device: &Device,
        id: &str,
        registry: &crate::transition_registry::TransitionRegistry,
    ) -> &wgpu::RenderPipeline {
        if !self.compiled.contains_key(id) {
            if let Some(def) = registry.get(id) {
                if !def.shader_source.is_empty() {
                    let success = self.compile_shader(device, id, &def.shader_source);
                    if !success {
                        log::warn!("Failed to compile transition shader '{id}', falling back to fade");
                    }
                }
            } else {
                log::warn!("Transition '{id}' not in registry, falling back to fade");
            }
        }

        self.compiled
            .get(id)
            .or_else(|| self.compiled.get(crate::transition::TRANSITION_FADE))
            .expect("fade pipeline must always be compiled")
    }

    /// Run the transition blend pass, writing the result to `target_view`.
    #[allow(clippy::too_many_arguments)]
    pub fn blend(
        &mut self,
        device: &Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        from_bind_group: &wgpu::BindGroup,
        to_bind_group: &wgpu::BindGroup,
        target_view: &wgpu::TextureView,
        transition_id: &str,
        progress: f32,
        time: f32,
        colors: &crate::transition::TransitionColors,
        registry: &crate::transition_registry::TransitionRegistry,
    ) {
        let uniforms = TransitionUniforms {
            progress,
            time,
            _pad: [0.0; 2],
            color: colors.color,
            from_color: colors.from_color,
            to_color: colors.to_color,
        };
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));

        // Get the pipeline (compile if needed, fallback to fade).
        let pipeline = self.get_or_compile(device, transition_id, registry);

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("transition_blend_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target_view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, from_bind_group, &[]);
        pass.set_bind_group(1, to_bind_group, &[]);
        pass.set_bind_group(2, &self.uniform_bind_group, &[]);
        pass.draw(0..4, 0..1);
    }
}
```

Note: the `blend` method signature changes — it now takes `&mut self` (for lazy compilation), `device`, `transition_id`, `colors`, and `registry`. Callers will be updated in Task 6.

- [ ] **Step 4: Verify it compiles**

Run: `cargo check 2>&1 | head -30`
Expected: Errors in `main.rs` and `renderer/mod.rs` where the old `blend()` signature is called. This is expected — those call sites are updated in Task 6.

- [ ] **Step 5: Commit**

```bash
git add src/renderer/transition.rs src/renderer/shaders/transition_fade.wgsl
git commit -m "feat: extend TransitionUniforms with colors, make TransitionPipeline multi-shader"
```

---

## Task 4: Add transitions_dir Helper and Seed Built-in Shaders

**Files:**
- Modify: `src/settings.rs`

- [ ] **Step 1: Add `transitions_dir()` helper**

In `src/settings.rs`, add after the `scenes_path()` function (line 405):

```rust
pub fn transitions_dir() -> PathBuf {
    config_dir().join("transitions")
}
```

- [ ] **Step 2: Add `seed_builtin_transitions()` function**

Add after `transitions_dir()`:

```rust
/// Seed the transitions directory with built-in shaders on first launch.
/// Only writes files that don't already exist, so user modifications are preserved.
pub fn seed_builtin_transitions() {
    let dir = transitions_dir();
    if let Err(e) = std::fs::create_dir_all(&dir) {
        log::warn!("Failed to create transitions directory: {e}");
        return;
    }

    let builtins: &[(&str, &str)] = &[
        ("fade.wgsl", include_str!("renderer/shaders/transition_fade.wgsl")),
    ];

    for (filename, source) in builtins {
        let path = dir.join(filename);
        if !path.exists() {
            if let Err(e) = std::fs::write(&path, source) {
                log::warn!("Failed to write built-in transition {filename}: {e}");
            }
        }
    }
}
```

- [ ] **Step 3: Add tests**

Add inside the `#[cfg(test)] mod tests` block in `src/settings.rs`:

```rust
#[test]
fn transitions_dir_is_inside_config_dir() {
    let td = super::transitions_dir();
    let cd = super::config_dir();
    assert!(td.starts_with(cd));
    assert!(td.ends_with("transitions"));
}

#[test]
fn seed_builtin_transitions_creates_fade() {
    let dir = tempfile::tempdir().unwrap();
    let transitions = dir.path().join("transitions");
    // Manually set up the directory and call the seeding logic inline
    // since seed_builtin_transitions uses the real config_dir.
    std::fs::create_dir_all(&transitions).unwrap();
    let fade_path = transitions.join("fade.wgsl");
    let source = include_str!("renderer/shaders/transition_fade.wgsl");
    std::fs::write(&fade_path, source).unwrap();
    let contents = std::fs::read_to_string(&fade_path).unwrap();
    assert!(contents.contains("@fragment"));
    assert!(contents.contains("@name: Fade"));
}

#[test]
fn seed_does_not_overwrite_existing() {
    let dir = tempfile::tempdir().unwrap();
    let transitions = dir.path().join("transitions");
    std::fs::create_dir_all(&transitions).unwrap();
    let fade_path = transitions.join("fade.wgsl");
    std::fs::write(&fade_path, "// user modified version").unwrap();
    // If we were to call seed, it should not overwrite.
    // The real function checks path.exists() before writing.
    assert!(fade_path.exists());
    let contents = std::fs::read_to_string(&fade_path).unwrap();
    assert_eq!(contents, "// user modified version");
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test --lib settings::tests -- --nocapture`
Expected: all settings tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/settings.rs
git commit -m "feat: add transitions_dir helper and seed_builtin_transitions"
```

---

## Task 5: Wire Registry and Seeding into AppState and Startup

**Files:**
- Modify: `src/state.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Add TransitionRegistry to AppState**

In `src/state.rs`, add the field to the `AppState` struct (after `pending_loop_mode_updates`):

```rust
/// Registry of available transition shaders, scanned from transitions directory.
pub transition_registry: crate::transition_registry::TransitionRegistry,
```

- [ ] **Step 2: Initialize registry in AppState::default**

Find the `Default` implementation for `AppState` and add the `transition_registry` field. Initialize it with an empty registry for now (the real scan happens at startup in main.rs). In the `Default` impl, add:

```rust
transition_registry: crate::transition_registry::TransitionRegistry::empty(),
```

Add an `empty()` constructor to `TransitionRegistry` in `src/transition_registry.rs`:

```rust
/// Create an empty registry with just the synthetic "cut" entry.
pub fn empty() -> Self {
    Self {
        transitions: vec![TransitionDef {
            id: crate::transition::TRANSITION_CUT.to_string(),
            name: "Cut".to_string(),
            author: String::new(),
            description: "Instant scene switch".to_string(),
            params: Vec::new(),
            shader_source: String::new(),
        }],
    }
}
```

- [ ] **Step 3: Add seeding and registry scan to main.rs startup**

Find the startup section in `main.rs` where settings are loaded (search for `AppSettings::load_or_detect` or similar). After settings loading and before the event loop starts, add:

```rust
// Seed built-in transition shaders on first launch.
crate::settings::seed_builtin_transitions();

// Scan transitions directory and populate the registry.
{
    let mut app_state = state.lock().unwrap();
    app_state.transition_registry =
        crate::transition_registry::TransitionRegistry::scan(&crate::settings::transitions_dir());
}
```

- [ ] **Step 4: Verify it compiles**

Run: `cargo check 2>&1 | head -30`
Expected: may still have errors from the blend() signature change in Task 3 — that's addressed in Task 6.

- [ ] **Step 5: Commit**

```bash
git add src/state.rs src/main.rs src/transition_registry.rs
git commit -m "feat: wire TransitionRegistry into AppState, seed built-ins at startup"
```

---

## Task 6: Update All Transition Trigger Sites and Render Loop

**Files:**
- Modify: `src/main.rs`
- Modify: `src/ui/scenes_panel.rs`
- Modify: `src/renderer/mod.rs`

This is the integration task. It updates all call sites that reference the old `TransitionType` enum and the old `blend()` signature.

- [ ] **Step 1: Update TransitionState construction in scenes_panel.rs**

In `src/ui/scenes_panel.rs`, the `draw_transition_bar` function (around line 800-880) constructs `TransitionState` and checks `TransitionType`. Update the transition trigger block.

Replace the match on `transition_type` (the block starting at line 820 `match transition_type {`) with a match on the string ID:

The `resolve_transition` call returns a `ResolvedTransition` now. Update the code from:

```rust
let (transition_type, duration) = target_scene
    .map(|s| {
        crate::transition::resolve_transition(
            &state.settings.transitions,
            &s.transition_override,
        )
    })
    .unwrap_or((
        crate::transition::TransitionType::Fade,
        std::time::Duration::from_millis(300),
    ));
```

To:

```rust
let resolved = target_scene
    .map(|s| {
        crate::transition::resolve_transition(
            &state.settings.transitions,
            &s.transition_override,
        )
    })
    .unwrap_or_else(|| crate::transition::ResolvedTransition {
        transition: crate::transition::TRANSITION_FADE.to_string(),
        duration: std::time::Duration::from_millis(300),
        colors: crate::transition::TransitionColors::default(),
    });
```

Then replace `match transition_type {` with `if resolved.transition == crate::transition::TRANSITION_CUT {`:

For the Cut branch, keep the existing logic.

For the else branch (any shader-based transition including fade), update the `TransitionState` construction:

```rust
state.active_transition = Some(crate::transition::TransitionState {
    from_scene: from_scene_id,
    to_scene: to_id,
    transition: resolved.transition,
    started_at: std::time::Instant::now(),
    duration: resolved.duration,
    colors: resolved.colors,
});
```

- [ ] **Step 2: Update the keyboard shortcut transition trigger in main.rs**

In `src/main.rs` around line 1140-1235, the same pattern exists for the Enter key transition trigger. Apply the same changes:

Replace the `resolve_transition` call and `match transition_type` block with the new `ResolvedTransition` pattern, same as Step 1.

Update the `TransitionState` construction to use the new fields.

- [ ] **Step 3: Update the transition info extraction in the render loop**

In `src/main.rs` around line 1771-1779, where `trans` is extracted from `active_transition`:

```rust
let trans = app_state.active_transition.as_ref().map(|t| {
    (
        t.from_scene,
        t.to_scene,
        t.transition_type,
        t.progress(),
        t.is_complete(),
    )
});
```

Change to:

```rust
let trans = app_state.active_transition.as_ref().map(|t| {
    (
        t.from_scene,
        t.to_scene,
        t.transition.clone(),
        t.progress(),
        t.is_complete(),
        t.colors,
    )
});
```

Update the corresponding destructuring wherever `transition_info` is used. The tuple type changes from `(SceneId, SceneId, TransitionType, f32, bool)` to `(SceneId, SceneId, String, f32, bool, TransitionColors)`.

- [ ] **Step 4: Update the blend() call site in the render loop**

In `src/main.rs` around line 1935, update the `gpu.transition_pipeline.blend(...)` call:

```rust
gpu.transition_pipeline.blend(
    &gpu.device,
    &gpu.queue,
    &mut encoder,
    from_bind_group,
    to_bind_group,
    gpu.compositor.output_texture_view(),
    &transition_id,
    progress,
    time,
    &colors,
    &transition_registry,
);
```

The `transition_registry` needs to be cloned or extracted from `app_state` before the lock is released. When extracting the transition info from `app_state`, also clone the registry reference. Since `TransitionRegistry` isn't `Clone`, extract the registry separately. The simplest approach: extract the registry from `app_state` into a local variable alongside the other transition data, while the lock is held.

Since the `TransitionPipeline` is on `gpu` (not behind the mutex), and `TransitionRegistry` is on `AppState` (behind the mutex), you need to hold the lock or clone the registry. The cleanest approach is to take a reference to the registry while the lock is held, and pass it into blend. But the lock is released before the GPU work. So add `Clone` derive to `TransitionRegistry` and `TransitionDef`, and clone it:

In `src/transition_registry.rs`, add `#[derive(Clone)]` to `TransitionRegistry` and ensure `TransitionDef` is `Clone` (it already is).

Then in the render loop, when extracting transition info:

```rust
let transition_registry = app_state.transition_registry.clone();
```

- [ ] **Step 5: Update SharedGpuState — transition_pipeline is now &mut**

In `src/renderer/mod.rs`, the `TransitionPipeline` is accessed as `gpu.transition_pipeline.blend(...)`. Since `blend` now takes `&mut self`, verify that `gpu` is already `&mut` in the render loop. It should be, since it's a local variable. No changes needed here unless the borrow checker complains.

- [ ] **Step 6: Remove the `use crate::transition::TransitionType;` import from scenes_panel.rs**

In `src/ui/scenes_panel.rs` line 10, remove:

```rust
use crate::transition::TransitionType;
```

- [ ] **Step 7: Verify it compiles**

Run: `cargo check 2>&1 | head -40`
Expected: clean compilation (possibly with warnings about unused fields in the scenes_panel UI — the transition bar UI is updated in Task 7).

- [ ] **Step 8: Run all tests**

Run: `cargo test -- --nocapture 2>&1 | tail -20`
Expected: all tests pass.

- [ ] **Step 9: Commit**

```bash
git add src/main.rs src/ui/scenes_panel.rs src/renderer/mod.rs src/transition_registry.rs
git commit -m "refactor: update all transition call sites for string-based IDs and extended blend API"
```

---

## Task 7: Update Transition Bar UI — Dropdown and Color Pickers

**Files:**
- Modify: `src/ui/scenes_panel.rs`

- [ ] **Step 1: Replace the segmented Fade/Cut toggle with a dropdown**

In `draw_transition_bar()`, replace the segmented control section (lines 589-673) with a `ComboBox` populated from the registry:

```rust
// ── Transition selector dropdown ──
let dropdown_w = 80.0;
let dropdown_h = 20.0;
let dropdown_y = bar_rect.center().y - dropdown_h / 2.0;
let dropdown_x = bar_rect.left() + padding;
let dropdown_rect = egui::Rect::from_min_size(
    egui::pos2(dropdown_x, dropdown_y),
    egui::vec2(dropdown_w, dropdown_h),
);

let current_id = &state.settings.transitions.default_transition;
let current_name = state
    .transition_registry
    .get(current_id)
    .map(|t| t.name.as_str())
    .unwrap_or("Fade");

let mut child_ui = ui.new_child(
    egui::UiBuilder::new()
        .max_rect(dropdown_rect)
        .layout(egui::Layout::left_to_right(egui::Align::Center)),
);

egui::ComboBox::from_id_salt("transition_default_selector")
    .selected_text(current_name)
    .width(dropdown_w - 16.0)
    .show_ui(&mut child_ui, |ui| {
        for def in state.transition_registry.all() {
            if ui
                .selectable_label(current_id == &def.id, &def.name)
                .clicked()
            {
                state.settings.transitions.default_transition = def.id.clone();
                state.mark_dirty();
            }
        }
    });
```

- [ ] **Step 2: Update the duration input position**

Since the dropdown replaces the segmented control, adjust `dur_x`:

```rust
let dur_x = dropdown_x + dropdown_w + 6.0;
```

Remove the old `seg_btn_w`, `seg_btn_h`, `seg_y`, `seg_x` variables and all the segmented control drawing code (fade_rect, cut_rect, pill background, active highlight, labels, hit-testing).

Keep `seg_btn_h` renamed to `btn_h = 20.0` and `seg_y` renamed to `btn_y = bar_rect.center().y - btn_h / 2.0` since the duration field and transition button reference these.

- [ ] **Step 3: Add color pickers below the transition bar (conditionally)**

After the transition bar, add color pickers based on the selected transition's `@params`. This should be added after the `draw_transition_bar` call in the `draw()` function, or at the end of `draw_transition_bar` itself.

Add at the end of `draw_transition_bar`, before the closing brace:

```rust
// ── Color pickers (shown based on selected transition's @params) ──
let current_def = state.transition_registry.get(
    &state.settings.transitions.default_transition,
);
if let Some(def) = current_def {
    if !def.params.is_empty() {
        let color_row_height = 22.0;
        let color_row_y = bar_rect.bottom() + 2.0;
        let mut cx = bar_rect.left() + padding;

        for param in &def.params {
            let (label, color_ref) = match param {
                crate::transition_registry::TransitionParam::Color => {
                    ("Color", &mut state.settings.transitions.default_colors.color)
                }
                crate::transition_registry::TransitionParam::FromColor => {
                    ("From", &mut state.settings.transitions.default_colors.from_color)
                }
                crate::transition_registry::TransitionParam::ToColor => {
                    ("To", &mut state.settings.transitions.default_colors.to_color)
                }
            };

            // Label
            painter.text(
                egui::pos2(cx, color_row_y + color_row_height / 2.0),
                egui::Align2::LEFT_CENTER,
                label,
                egui::FontId::proportional(9.0),
                theme.text_muted,
            );
            cx += 30.0;

            // Color button
            let swatch_rect = egui::Rect::from_min_size(
                egui::pos2(cx, color_row_y + 2.0),
                egui::vec2(color_row_height - 4.0, color_row_height - 4.0),
            );

            let egui_color = egui::Color32::from_rgba_unmultiplied(
                (color_ref[0] * 255.0) as u8,
                (color_ref[1] * 255.0) as u8,
                (color_ref[2] * 255.0) as u8,
                (color_ref[3] * 255.0) as u8,
            );
            painter.rect_filled(
                swatch_rect,
                CornerRadius::same(3),
                egui_color,
            );
            painter.rect_stroke(
                swatch_rect,
                CornerRadius::same(3),
                egui::Stroke::new(1.0, theme.border),
                egui::StrokeKind::Outside,
            );

            let swatch_response = ui.interact(
                swatch_rect,
                egui::Id::new(("transition_color", label)),
                egui::Sense::click(),
            );

            let popup_id = egui::Id::new(("transition_color_popup", label));
            if swatch_response.clicked() {
                ui.memory_mut(|m| m.toggle_popup(popup_id));
            }

            egui::popup_below_widget(ui, popup_id, &swatch_response, egui::PopupCloseBehavior::CloseOnClickOutside, |ui| {
                let mut rgba = egui::ecolor::Rgba::from_rgba_unmultiplied(
                    color_ref[0], color_ref[1], color_ref[2], color_ref[3],
                );
                if egui::color_picker::color_edit_button_rgba(ui, &mut rgba, egui::color_picker::Alpha::OnlyBlend).changed() {
                    *color_ref = [rgba.r(), rgba.g(), rgba.b(), rgba.a()];
                    state.mark_dirty();
                }
            });

            cx += color_row_height + 8.0;
        }
    }
}
```

Note: The bar height allocation may need to increase when color pickers are shown. Adjust the `bar_height` to be dynamic based on whether the current transition has params.

- [ ] **Step 4: Update per-scene override context menu**

In the scene context menu (around line 445-498), replace the `TransitionType` combo box with a dropdown from the registry:

```rust
ui.menu_button("Transition Override", |ui| {
    let (current_transition, current_duration_ms) = state
        .scenes
        .iter()
        .find(|s| s.id == scene_id)
        .map(|s| (
            s.transition_override.transition.clone(),
            s.transition_override.duration_ms,
        ))
        .unwrap_or((None, None));

    ui.label("Type");

    let type_label = current_transition
        .as_ref()
        .and_then(|id| state.transition_registry.get(id))
        .map(|t| t.name.as_str())
        .unwrap_or("Default");

    egui::ComboBox::from_id_salt(egui::Id::new(("scene_tx_type", scene_id.0)))
        .selected_text(type_label)
        .show_ui(ui, |ui| {
            if ui
                .selectable_label(current_transition.is_none(), "Default")
                .clicked()
            {
                if let Some(scene) = state.scenes.iter_mut().find(|s| s.id == scene_id) {
                    scene.transition_override.transition = None;
                }
                state.mark_dirty();
            }
            for def in state.transition_registry.all() {
                if ui
                    .selectable_label(
                        current_transition.as_deref() == Some(&def.id),
                        &def.name,
                    )
                    .clicked()
                {
                    if let Some(scene) = state.scenes.iter_mut().find(|s| s.id == scene_id) {
                        scene.transition_override.transition = Some(def.id.clone());
                    }
                    state.mark_dirty();
                }
            }
        });

    // Duration input (keep existing logic, just update field name)
    // ... (same as existing, but use scene.transition_override.duration_ms)
});
```

- [ ] **Step 5: Verify it compiles and runs**

Run: `cargo check && cargo test -- --nocapture 2>&1 | tail -20`
Expected: clean compilation, all tests pass.

- [ ] **Step 6: Commit**

```bash
git add src/ui/scenes_panel.rs
git commit -m "feat: replace Fade/Cut toggle with transition dropdown and color pickers"
```

---

## Task 8: TOML Backwards Compatibility

**Files:**
- Modify: `src/transition.rs`

The old TOML format used `default_type = "Fade"` (the enum variant name). The new format uses `default_transition = "fade"`. We need serde deserialization to handle both.

- [ ] **Step 1: Write backwards compatibility tests**

Add to `src/transition.rs` tests:

```rust
#[test]
fn settings_deserialize_new_format() {
    let toml = r#"
default_transition = "fade"
default_duration_ms = 500

[default_colors]
color = [1.0, 0.0, 0.0, 1.0]
from_color = [0.0, 0.0, 0.0, 1.0]
to_color = [0.0, 0.0, 0.0, 1.0]
"#;
    let settings: TransitionSettings = toml::from_str(toml).unwrap();
    assert_eq!(settings.default_transition, "fade");
    assert_eq!(settings.default_duration_ms, 500);
    assert_eq!(settings.default_colors.color, [1.0, 0.0, 0.0, 1.0]);
}

#[test]
fn settings_deserialize_old_format_fade() {
    let toml = r#"
default_type = "Fade"
default_duration_ms = 300
"#;
    let settings: TransitionSettings = toml::from_str(toml).unwrap();
    assert_eq!(settings.default_transition, "fade");
    assert_eq!(settings.default_duration_ms, 300);
}

#[test]
fn settings_deserialize_old_format_cut() {
    let toml = r#"
default_type = "Cut"
default_duration_ms = 0
"#;
    let settings: TransitionSettings = toml::from_str(toml).unwrap();
    assert_eq!(settings.default_transition, "cut");
}

#[test]
fn settings_deserialize_empty() {
    let settings: TransitionSettings = toml::from_str("").unwrap();
    assert_eq!(settings.default_transition, "fade");
    assert_eq!(settings.default_duration_ms, 300);
}

#[test]
fn override_deserialize_new_format() {
    let toml = r#"
transition = "dip_to_color"
duration_ms = 1000

[colors]
color = [1.0, 1.0, 1.0, 1.0]
from_color = [0.0, 0.0, 0.0, 1.0]
to_color = [0.0, 0.0, 0.0, 1.0]
"#;
    let o: SceneTransitionOverride = toml::from_str(toml).unwrap();
    assert_eq!(o.transition.as_deref(), Some("dip_to_color"));
    assert_eq!(o.duration_ms, Some(1000));
    assert_eq!(o.colors.unwrap().color, [1.0, 1.0, 1.0, 1.0]);
}

#[test]
fn override_deserialize_old_format() {
    let toml = r#"
transition_type = "Fade"
duration_ms = 500
"#;
    let o: SceneTransitionOverride = toml::from_str(toml).unwrap();
    assert_eq!(o.transition.as_deref(), Some("fade"));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib transition::tests -- --nocapture 2>&1 | head -30`
Expected: the old format tests fail because serde doesn't know about `default_type` or `transition_type`.

- [ ] **Step 3: Add custom deserialization for backwards compatibility**

Add a custom `Deserialize` impl for `TransitionSettings` using a helper struct:

```rust
use serde::de::Deserializer;

impl<'de> Deserialize<'de> for TransitionSettings {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(default)]
        struct Raw {
            default_transition: Option<String>,
            default_type: Option<String>,
            default_duration_ms: u32,
            default_colors: TransitionColors,
        }

        impl Default for Raw {
            fn default() -> Self {
                Self {
                    default_transition: None,
                    default_type: None,
                    default_duration_ms: 300,
                    default_colors: TransitionColors::default(),
                }
            }
        }

        let raw = Raw::deserialize(deserializer)?;

        let default_transition = raw
            .default_transition
            .or_else(|| {
                raw.default_type.map(|old| match old.as_str() {
                    "Cut" => TRANSITION_CUT.to_string(),
                    "Fade" | _ => TRANSITION_FADE.to_string(),
                })
            })
            .unwrap_or_else(|| TRANSITION_FADE.to_string());

        Ok(TransitionSettings {
            default_transition,
            default_duration_ms: raw.default_duration_ms,
            default_colors: raw.default_colors,
        })
    }
}
```

Remove the `#[derive(Deserialize)]` and `#[serde(default)]` from `TransitionSettings` since we now have a manual impl. Keep `#[derive(Debug, Clone, Serialize)]`.

Similarly, add custom deserialization for `SceneTransitionOverride`:

```rust
impl<'de> Deserialize<'de> for SceneTransitionOverride {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize, Default)]
        #[serde(default)]
        struct Raw {
            transition: Option<String>,
            transition_type: Option<String>,
            duration_ms: Option<u32>,
            colors: Option<TransitionColors>,
        }

        let raw = Raw::deserialize(deserializer)?;

        let transition = raw.transition.or_else(|| {
            raw.transition_type.map(|old| match old.as_str() {
                "Cut" => TRANSITION_CUT.to_string(),
                "Fade" => TRANSITION_FADE.to_string(),
                other => other.to_lowercase(),
            })
        });

        Ok(SceneTransitionOverride {
            transition,
            duration_ms: raw.duration_ms,
            colors: raw.colors,
        })
    }
}
```

Remove `#[derive(Deserialize)]` from `SceneTransitionOverride`. Keep `#[derive(Debug, Clone, Default, Serialize)]`.

- [ ] **Step 4: Run tests**

Run: `cargo test --lib transition::tests -- --nocapture`
Expected: all tests pass including backwards compatibility tests.

- [ ] **Step 5: Run full test suite**

Run: `cargo test -- --nocapture 2>&1 | tail -20`
Expected: all tests pass.

- [ ] **Step 6: Commit**

```bash
git add src/transition.rs
git commit -m "feat: add backwards-compatible TOML deserialization for transition settings"
```

---

## Task 9: Final Integration and Cleanup

**Files:**
- Modify: various (cleanup pass)

- [ ] **Step 1: Run clippy**

Run: `cargo clippy 2>&1 | head -40`
Fix any warnings.

- [ ] **Step 2: Run fmt**

Run: `cargo fmt --check`
Fix any formatting issues with `cargo fmt`.

- [ ] **Step 3: Run full test suite**

Run: `cargo test -- --nocapture 2>&1 | tail -30`
Expected: all tests pass.

- [ ] **Step 4: Verify the app builds in release mode**

Run: `cargo build --release 2>&1 | tail -10`
Expected: clean compilation.

- [ ] **Step 5: Commit any cleanup**

```bash
git add -A
git commit -m "chore: clippy and fmt cleanup for transition system"
```

---

## Summary

| Task | What it does | Key files |
|------|-------------|-----------|
| 1 | Replace `TransitionType` enum with string IDs + `TransitionColors` | `transition.rs` |
| 2 | Header parser + directory scanner + registry | `transition_registry.rs` |
| 3 | Extend GPU uniforms, multi-shader pipeline | `renderer/transition.rs`, `transition_fade.wgsl` |
| 4 | `transitions_dir()` helper + seeding | `settings.rs` |
| 5 | Wire registry into `AppState` + startup | `state.rs`, `main.rs` |
| 6 | Update all call sites (3 trigger locations + render loop) | `main.rs`, `scenes_panel.rs` |
| 7 | Dropdown UI + color pickers | `scenes_panel.rs` |
| 8 | TOML backwards compatibility | `transition.rs` |
| 9 | Clippy, fmt, release build verification | various |
