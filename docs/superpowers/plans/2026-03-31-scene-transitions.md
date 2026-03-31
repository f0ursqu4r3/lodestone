# Scene Transitions Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add cut and crossfade transitions between scenes, with Studio Mode (dual preview/program), per-scene overrides, and a shader-ready transition pipeline.

**Architecture:** Dual-canvas lazy activation — a secondary canvas + source layer set is allocated on demand when Studio Mode is toggled on or a fade transition starts. A fullscreen-quad transition pipeline blends the two canvases using a `progress` uniform. The pipeline accepts swappable WGSL shaders for future custom transitions.

**Tech Stack:** Rust, wgpu, egui, WGSL shaders, serde/TOML for settings persistence.

---

## File Structure

| Action | File | Responsibility |
|--------|------|---------------|
| Create | `src/renderer/shaders/transition_fade.wgsl` | Fade crossfade shader (mix two textures by progress) |
| Create | `src/renderer/transition.rs` | TransitionPipeline struct — GPU resources for blending two canvases |
| Create | `src/renderer/secondary_canvas.rs` | SecondaryCanvas struct — on-demand second canvas + source layers |
| Create | `src/transition.rs` | TransitionType, TransitionConfig, TransitionState, SceneTransitionOverride types |
| Modify | `src/renderer/compositor.rs` | Add `compose_to(view, source_layers, ...)` method, expose bind group layouts |
| Modify | `src/state.rs` | Add `studio_mode`, `preview_scene_id`, `active_transition` fields to AppState |
| Modify | `src/scene.rs` | Add `transition_override` field to Scene |
| Modify | `src/settings.rs` | Add `TransitionSettings` to AppSettings |
| Modify | `src/main.rs` | Dual-canvas render loop, transition progress, secondary canvas lifecycle |
| Modify | `src/ui/scenes_panel.rs` | Transition bar UI, Studio Mode badges, deferred source cleanup |
| Modify | `src/ui/preview_panel.rs` | Studio Mode dual-pane layout, transition progress indicator |
| Modify | `src/ui/toolbar.rs` | Studio Mode toggle hotkey display |

---

## Task 1: Transition Types and Configuration

**Files:**
- Create: `src/transition.rs`
- Modify: `src/scene.rs:12-20` (Scene struct)
- Modify: `src/settings.rs:7-30` (AppSettings struct)
- Modify: `src/state.rs:108-184` (AppState struct)

- [ ] **Step 1: Create `src/transition.rs` with core types**

```rust
// src/transition.rs
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

use crate::scene::SceneId;

/// The type of transition effect between scenes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TransitionType {
    /// Instant scene switch, no animation.
    Cut,
    /// Linear crossfade between outgoing and incoming scene.
    #[default]
    Fade,
}

/// Global transition defaults, persisted in settings TOML.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TransitionSettings {
    pub default_type: TransitionType,
    pub default_duration_ms: u32,
}

impl Default for TransitionSettings {
    fn default() -> Self {
        Self {
            default_type: TransitionType::Fade,
            default_duration_ms: 300,
        }
    }
}

/// Per-scene transition override. Controls the transition used when
/// transitioning *into* this scene. `None` fields inherit from global defaults.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SceneTransitionOverride {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transition_type: Option<TransitionType>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u32>,
}

/// Runtime state for an in-progress transition. Not persisted.
#[derive(Debug, Clone)]
pub struct TransitionState {
    pub from_scene: SceneId,
    pub to_scene: SceneId,
    pub transition_type: TransitionType,
    pub started_at: Instant,
    pub duration: Duration,
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

/// Resolve which transition type and duration to use for a scene switch.
/// Per-scene override takes priority over global default.
pub fn resolve_transition(
    global: &TransitionSettings,
    scene_override: &SceneTransitionOverride,
) -> (TransitionType, Duration) {
    let t = scene_override.transition_type.unwrap_or(global.default_type);
    let d = scene_override.duration_ms.unwrap_or(global.default_duration_ms);
    (t, Duration::from_millis(d as u64))
}
```

- [ ] **Step 2: Add `pub mod transition;` to `src/main.rs`**

Add the module declaration near the top of `src/main.rs` alongside the other `mod` declarations:

```rust
pub mod transition;
```

- [ ] **Step 3: Add `transition_override` to Scene struct**

In `src/scene.rs`, add the field to the `Scene` struct (after `pinned` at line 19):

```rust
/// Per-scene transition override (type + duration when transitioning INTO this scene).
#[serde(default)]
pub transition_override: crate::transition::SceneTransitionOverride,
```

- [ ] **Step 4: Add `TransitionSettings` to AppSettings**

In `src/settings.rs`, add a new field to `AppSettings` (after `settings_window` at line 29):

```rust
#[serde(default)]
pub transitions: crate::transition::TransitionSettings,
```

- [ ] **Step 5: Add transition state fields to AppState**

In `src/state.rs`, add three fields to the `AppState` struct (after `window_picker_result` at line 183):

```rust
/// Whether Studio Mode is active (dual preview/program layout).
pub studio_mode: bool,
/// In Studio Mode, the scene loaded in the Preview pane. None = no scene selected.
pub preview_scene_id: Option<SceneId>,
/// In-progress transition state. None = no transition active.
pub active_transition: Option<crate::transition::TransitionState>,
```

Also add defaults in the `Default` impl for `AppState`:

```rust
studio_mode: false,
preview_scene_id: None,
active_transition: None,
```

- [ ] **Step 6: Run `cargo build` to verify compilation**

Run: `cargo build 2>&1 | tail -20`
Expected: Successful compilation (warnings OK, no errors).

- [ ] **Step 7: Run existing tests**

Run: `cargo test 2>&1 | tail -20`
Expected: All existing tests pass.

- [ ] **Step 8: Commit**

```bash
git add src/transition.rs src/scene.rs src/settings.rs src/state.rs src/main.rs
git commit -m "feat: add transition types, config, and state model"
```

---

## Task 2: Fade Transition Shader

**Files:**
- Create: `src/renderer/shaders/transition_fade.wgsl`

- [ ] **Step 1: Create the fade shader**

```wgsl
// src/renderer/shaders/transition_fade.wgsl
//
// Crossfade transition: linearly blends two scene canvases by progress.
// Standard transition interface — all transition shaders receive:
//   - t_from / s_from: outgoing scene texture + sampler (group 0)
//   - t_to / s_to:     incoming scene texture + sampler (group 1)
//   - uniforms.progress: 0.0 (fully "from") to 1.0 (fully "to")
//   - uniforms.time:     elapsed seconds since transition start

struct TransitionUniforms {
    progress: f32,
    time: f32,
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

- [ ] **Step 2: Commit**

```bash
git add src/renderer/shaders/transition_fade.wgsl
git commit -m "feat: add fade transition WGSL shader"
```

---

## Task 3: TransitionPipeline GPU Resources

**Files:**
- Create: `src/renderer/transition.rs`
- Modify: `src/renderer/mod.rs` (add `pub mod transition;`)

- [ ] **Step 1: Create `src/renderer/transition.rs`**

This struct owns the GPU pipeline and uniform buffer for blending two canvas textures.

```rust
// src/renderer/transition.rs

use bytemuck::{Pod, Zeroable};
use wgpu::Device;

const TRANSITION_FADE_SHADER: &str = include_str!("shaders/transition_fade.wgsl");

/// Uniform buffer for transition shaders. 8 bytes padded to 16 for alignment.
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct TransitionUniforms {
    pub progress: f32,
    pub time: f32,
    pub _padding: [f32; 2],
}

/// GPU resources for the transition blend pass.
///
/// Takes two canvas textures (from + to) and blends them via a fullscreen quad
/// using a configurable shader. The result is written to whichever render target
/// the caller provides (the primary canvas view or the output texture view).
pub struct TransitionPipeline {
    pipeline: wgpu::RenderPipeline,
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    /// Layout for texture+sampler bind groups (groups 0 and 1).
    /// Matches the existing `texture_bind_group_layout` in the compositor.
    texture_bind_group_layout: wgpu::BindGroupLayout,
}

impl TransitionPipeline {
    pub fn new(
        device: &Device,
        texture_bind_group_layout: &wgpu::BindGroupLayout,
        target_format: wgpu::TextureFormat,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("transition_fade_shader"),
            source: wgpu::ShaderSource::Wgsl(TRANSITION_FADE_SHADER.into()),
        });

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
                texture_bind_group_layout, // group 0: from texture + sampler
                texture_bind_group_layout, // group 1: to texture + sampler
                &uniform_bind_group_layout, // group 2: uniforms
            ],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("transition_pipeline"),
            layout: Some(&pipeline_layout),
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
                    format: target_format,
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

        // Clone the layout for creating bind groups later.
        let texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("transition_texture_bgl"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
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

        Self {
            pipeline,
            uniform_buffer,
            uniform_bind_group,
            texture_bind_group_layout,
        }
    }

    /// Run the transition blend pass, writing the result to `target_view`.
    ///
    /// `from_bind_group` and `to_bind_group` are texture+sampler bind groups
    /// for the outgoing and incoming scene canvases.
    pub fn blend(
        &self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        from_bind_group: &wgpu::BindGroup,
        to_bind_group: &wgpu::BindGroup,
        target_view: &wgpu::TextureView,
        progress: f32,
        time: f32,
    ) {
        let uniforms = TransitionUniforms {
            progress,
            time,
            _padding: [0.0; 2],
        };
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));

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

        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, from_bind_group, &[]);
        pass.set_bind_group(1, to_bind_group, &[]);
        pass.set_bind_group(2, &self.uniform_bind_group, &[]);
        pass.draw(0..4, 0..1);
    }
}
```

- [ ] **Step 2: Add module declaration**

In `src/renderer/mod.rs`, add:

```rust
pub mod transition;
```

If `src/renderer/mod.rs` doesn't exist and modules are declared in `src/main.rs`, add `pub mod transition;` inside the `renderer` module declaration there instead. Check the actual module structure first.

- [ ] **Step 3: Run `cargo build` to verify compilation**

Run: `cargo build 2>&1 | tail -20`
Expected: Successful compilation. The `TransitionPipeline` compiles but isn't wired up yet.

- [ ] **Step 4: Commit**

```bash
git add src/renderer/transition.rs src/renderer/shaders/transition_fade.wgsl src/renderer/mod.rs
git commit -m "feat: add TransitionPipeline GPU resources and blend pass"
```

---

## Task 4: SecondaryCanvas for Dual-Scene Rendering

**Files:**
- Create: `src/renderer/secondary_canvas.rs`
- Modify: `src/renderer/compositor.rs:629-699` (extract `compose_to` method, expose layouts)
- Modify: `src/renderer/mod.rs` (add module)

- [ ] **Step 1: Expose bind group layouts and compose helper on Compositor**

The secondary canvas needs access to the compositor's `texture_bind_group_layout`, `uniform_bind_group_layout`, `pipeline`, and `sampler` to create its own source layers and compose to its own texture view. Also extract a `compose_to()` method that accepts an arbitrary target view + source layers.

In `src/renderer/compositor.rs`, add these public accessor methods after the existing `canvas_pipeline()` method (around line 899):

```rust
/// Returns a reference to the texture bind group layout for creating new bind groups.
pub fn texture_bind_group_layout(&self) -> &wgpu::BindGroupLayout {
    &self.texture_bind_group_layout
}

/// Returns a reference to the uniform bind group layout for creating per-source uniform bind groups.
pub fn uniform_bind_group_layout(&self) -> &wgpu::BindGroupLayout {
    &self.uniform_bind_group_layout
}

/// Returns a reference to the compositor render pipeline (sources → canvas).
pub fn compositor_pipeline(&self) -> &wgpu::RenderPipeline {
    &self.pipeline
}

/// Returns a reference to the sampler used for source textures.
pub fn compositor_sampler(&self) -> &wgpu::Sampler {
    &self.sampler
}

/// Compose sources onto an arbitrary target view using the given source layers.
///
/// This is the core composition logic extracted for use by both the primary
/// canvas and the secondary canvas.
pub fn compose_to(
    &self,
    queue: &wgpu::Queue,
    encoder: &mut wgpu::CommandEncoder,
    target_view: &wgpu::TextureView,
    source_layers: &std::collections::HashMap<crate::scene::SourceId, SourceLayer>,
    sources: &[ResolvedSource],
) {
    let cw = self.canvas_width as f32;
    let ch = self.canvas_height as f32;

    // Clear pass.
    {
        let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("compositor_clear_pass"),
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
    }

    for source in sources {
        let layer = match source_layers.get(&source.id) {
            Some(l) => l,
            None => continue,
        };
        if !source.visible {
            continue;
        }

        let t = &source.transform;
        let uniforms = SourceUniforms {
            rect: [t.x / cw, t.y / ch, t.width / cw, t.height / ch],
            opacity: source.opacity.clamp(0.0, 1.0),
            _pad_align: [0.0; 3],
            _padding: [0.0; 3],
            _pad_end: 0.0,
        };
        queue.write_buffer(&layer.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("compositor_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target_view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &layer.bind_group, &[]);
        pass.set_bind_group(1, &layer.uniform_bind_group, &[]);
        pass.draw(0..4, 0..1);
    }
}
```

Then refactor the existing `compose()` method to delegate to `compose_to()`:

```rust
pub fn compose(
    &self,
    queue: &wgpu::Queue,
    encoder: &mut wgpu::CommandEncoder,
    sources: &[ResolvedSource],
) {
    self.compose_to(queue, encoder, &self.canvas_view, &self.source_layers, sources);
}
```

- [ ] **Step 2: Also expose the `preview_sampler` and canvas view for the transition pipeline**

Add these accessors to `Compositor`:

```rust
/// Returns a reference to the preview sampler.
pub fn preview_sampler(&self) -> &wgpu::Sampler {
    &self.preview_sampler
}

/// Returns a reference to the primary canvas texture view.
pub fn canvas_view(&self) -> &wgpu::TextureView {
    &self.canvas_view
}
```

- [ ] **Step 3: Create `src/renderer/secondary_canvas.rs`**

```rust
// src/renderer/secondary_canvas.rs

use std::collections::HashMap;
use std::sync::Arc;
use wgpu::Device;

use crate::renderer::compositor::{
    ResolvedSource, SourceLayer,
};
use crate::scene::SourceId;

/// On-demand second canvas for Studio Mode and transitions.
///
/// Owns its own texture, texture view, bind group, and source layers.
/// Created when Studio Mode is toggled on or a fade transition starts.
/// Destroyed when Studio Mode is off and no transition is in progress.
pub struct SecondaryCanvas {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    /// Bind group for sampling this canvas in the transition pipeline or preview panel.
    pub bind_group: Arc<wgpu::BindGroup>,
    /// Per-source GPU resources for compositing onto this canvas.
    pub source_layers: HashMap<SourceId, SourceLayer>,
}

impl SecondaryCanvas {
    /// Allocate a secondary canvas matching the primary canvas dimensions.
    pub fn new(
        device: &Device,
        width: u32,
        height: u32,
        texture_bind_group_layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
    ) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("secondary_canvas"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("secondary_canvas_bind_group"),
            layout: texture_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        });

        Self {
            texture,
            view,
            bind_group: Arc::new(bind_group),
            source_layers: HashMap::new(),
        }
    }

    /// Upload a frame for a source on the secondary canvas.
    /// Creates or resizes the source layer GPU texture as needed.
    ///
    /// This mirrors `Compositor::upload_frame()` but operates on the
    /// secondary canvas's own source layers.
    pub fn upload_frame(
        &mut self,
        device: &Device,
        queue: &wgpu::Queue,
        source_id: SourceId,
        frame: &crate::renderer::compositor::RgbaFrame,
        texture_bind_group_layout: &wgpu::BindGroupLayout,
        uniform_bind_group_layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
    ) {
        let needs_create = match self.source_layers.get(&source_id) {
            None => true,
            Some(layer) => layer.size != (frame.width, frame.height),
        };

        if needs_create {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("secondary_source_texture"),
                size: wgpu::Extent3d {
                    width: frame.width,
                    height: frame.height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let texture_view = texture.create_view(&Default::default());

            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("secondary_source_bind_group"),
                layout: texture_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&texture_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(sampler),
                    },
                ],
            });

            let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("secondary_source_uniform_buffer"),
                size: 48, // SourceUniforms is 48 bytes
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });

            let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("secondary_source_uniform_bind_group"),
                layout: uniform_bind_group_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                }],
            });

            self.source_layers.insert(
                source_id,
                SourceLayer {
                    texture,
                    texture_view,
                    bind_group,
                    uniform_buffer,
                    uniform_bind_group,
                    size: (frame.width, frame.height),
                },
            );
        }

        let layer = self.source_layers.get(&source_id).unwrap();
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &layer.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &frame.data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(frame.width * 4),
                rows_per_image: Some(frame.height),
            },
            wgpu::Extent3d {
                width: frame.width,
                height: frame.height,
                depth_or_array_layers: 1,
            },
        );
    }
}
```

- [ ] **Step 4: Add module declaration**

In `src/renderer/mod.rs`, add:

```rust
pub mod secondary_canvas;
```

- [ ] **Step 5: Run `cargo build` to verify compilation**

Run: `cargo build 2>&1 | tail -20`
Expected: Successful compilation.

- [ ] **Step 6: Commit**

```bash
git add src/renderer/secondary_canvas.rs src/renderer/compositor.rs src/renderer/mod.rs
git commit -m "feat: add SecondaryCanvas and extract compose_to on Compositor"
```

---

## Task 5: Wire Up Transition in the Render Loop

**Files:**
- Modify: `src/main.rs:1243-1380` (about_to_wait render loop)

This is the core integration. The render loop needs to:
1. Upload frames to both primary and secondary canvas source layers
2. Compose both canvases when Studio Mode is on or a transition is active
3. Run the transition blend pass during active transitions
4. Complete transitions when progress reaches 1.0

- [ ] **Step 1: Add TransitionPipeline and SecondaryCanvas to the GPU state**

Find where the `Gpu` struct (or equivalent) is defined in `src/main.rs` — it holds the `compositor`, `device`, `queue`, etc. Add two new fields:

```rust
transition_pipeline: crate::renderer::transition::TransitionPipeline,
secondary_canvas: Option<crate::renderer::secondary_canvas::SecondaryCanvas>,
```

Initialize `transition_pipeline` in the GPU setup code where the `Compositor` is created:

```rust
let transition_pipeline = crate::renderer::transition::TransitionPipeline::new(
    &device,
    compositor.texture_bind_group_layout(),
    wgpu::TextureFormat::Rgba8UnormSrgb, // CANVAS_FORMAT
);
```

Initialize `secondary_canvas` as `None`.

- [ ] **Step 2: Add helper to resolve sources for any scene**

Add a function near the render loop (or as a method on the event handler) that resolves sources for a given scene ID:

```rust
fn resolve_scene_sources(
    state: &crate::state::AppState,
    scene_id: crate::scene::SceneId,
) -> Vec<crate::renderer::compositor::ResolvedSource> {
    state
        .scenes
        .iter()
        .find(|s| s.id == scene_id)
        .map(|scene| {
            scene
                .sources
                .iter()
                .filter_map(|scene_src| {
                    state
                        .library
                        .iter()
                        .find(|s| s.id == scene_src.source_id)
                        .map(|lib| crate::renderer::compositor::ResolvedSource {
                            id: lib.id,
                            transform: scene_src.resolve_transform(lib),
                            opacity: scene_src.resolve_opacity(lib),
                            visible: scene_src.resolve_visible(lib),
                        })
                })
                .collect()
        })
        .unwrap_or_default()
}
```

- [ ] **Step 3: Modify frame upload to feed both canvases**

In the frame upload section (around line 1292), after uploading to the primary compositor, also upload to the secondary canvas if it exists:

```rust
for (source_id, frame) in &drained_frames {
    gpu.compositor.upload_frame(&gpu.device, &gpu.queue, *source_id, frame);
    if let Some(ref mut secondary) = gpu.secondary_canvas {
        secondary.upload_frame(
            &gpu.device,
            &gpu.queue,
            *source_id,
            frame,
            gpu.compositor.texture_bind_group_layout(),
            gpu.compositor.uniform_bind_group_layout(),
            gpu.compositor.compositor_sampler(),
        );
    }
}
```

- [ ] **Step 4: Replace the single-scene compose with dual-canvas logic**

Replace the composition section (lines 1324-1379) with the new dual-canvas render logic:

```rust
// Compose scene(s) and run transition blend if active.
if let Some(ref mut gpu) = self.gpu {
    let app_state = self.state.lock().expect("lock AppState");

    let active_scene_id = app_state.active_scene_id;
    let transition = app_state.active_transition.clone();
    let studio_mode = app_state.studio_mode;
    let preview_scene_id = app_state.preview_scene_id;
    let is_encoding = app_state.stream_status.is_live()
        || matches!(
            app_state.recording_status,
            crate::state::RecordingStatus::Recording { .. }
        )
        || app_state.virtual_camera_active;

    // Determine which scenes to compose.
    let program_scene_id = if let Some(ref t) = transition {
        Some(t.from_scene)
    } else {
        active_scene_id
    };

    let secondary_scene_id = if let Some(ref t) = transition {
        Some(t.to_scene)
    } else if studio_mode {
        preview_scene_id
    } else {
        None
    };

    // Resolve sources while holding the lock.
    let program_sources = program_scene_id
        .map(|id| resolve_scene_sources(&app_state, id))
        .unwrap_or_default();
    let secondary_sources = secondary_scene_id
        .map(|id| resolve_scene_sources(&app_state, id));

    drop(app_state); // Release lock before GPU work.

    // Ensure secondary canvas exists if we need it.
    if secondary_sources.is_some() && gpu.secondary_canvas.is_none() {
        gpu.secondary_canvas = Some(
            crate::renderer::secondary_canvas::SecondaryCanvas::new(
                &gpu.device,
                gpu.compositor.canvas_width,
                gpu.compositor.canvas_height,
                gpu.compositor.texture_bind_group_layout(),
                gpu.compositor.preview_sampler(),
            ),
        );
    }

    let mut encoder =
        gpu.device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("compositor_encoder"),
            });

    // 1. Compose program scene onto primary canvas.
    gpu.compositor.compose(&gpu.queue, &mut encoder, &program_sources);

    // 2. Compose secondary scene onto secondary canvas (if needed).
    if let (Some(ref sec_sources), Some(ref secondary)) =
        (&secondary_sources, &gpu.secondary_canvas)
    {
        gpu.compositor.compose_to(
            &gpu.queue,
            &mut encoder,
            &secondary.view,
            &secondary.source_layers,
            sec_sources,
        );
    }

    // 3. Run transition blend pass if a fade transition is active.
    if let Some(ref t) = transition {
        match t.transition_type {
            crate::transition::TransitionType::Fade => {
                let progress = t.progress();
                let time = t.started_at.elapsed().as_secs_f32();
                if let Some(ref secondary) = gpu.secondary_canvas {
                    // Blend from primary canvas → secondary canvas,
                    // writing result to primary canvas view.
                    // We need a separate output texture for this to avoid
                    // read/write conflict. Use the output_texture as scratch.
                    gpu.transition_pipeline.blend(
                        &gpu.queue,
                        &mut encoder,
                        &gpu.compositor.canvas_bind_group(),
                        &secondary.bind_group,
                        gpu.compositor.canvas_view(),
                        progress,
                        time,
                    );
                }
            }
            crate::transition::TransitionType::Cut => {
                // Cut transitions are handled instantly — no blend pass.
            }
        }
    }

    // 4. Scale to output resolution when encoding.
    if is_encoding {
        gpu.compositor.scale_to_output(&mut encoder);
    }

    gpu.queue.submit(std::iter::once(encoder.finish()));

    if is_encoding {
        gpu.compositor.start_readback(&gpu.device, &gpu.queue);
    }

    // 5. Complete transition if done.
    if let Some(ref t) = transition {
        if t.is_complete() {
            let mut app_state = self.state.lock().expect("lock AppState");
            app_state.active_scene_id = Some(t.to_scene);
            app_state.active_transition = None;

            if !app_state.studio_mode {
                // Deallocate secondary canvas.
                gpu.secondary_canvas = None;
                // TODO: Task 7 will add deferred source cleanup here.
            } else {
                // In Studio Mode, reset preview scene selection.
                app_state.preview_scene_id = None;
            }
        }
    }

    // Request repaint if transition is in progress (drives continuous animation).
    if transition.is_some() {
        if let Some(main_id) = self.main_window_id
            && let Some(win) = self.windows.get(&main_id)
        {
            win.window.request_redraw();
        }
    }
}
```

**Important note about read/write conflict:** The blend pass above writes to the primary canvas view while also reading from it via `canvas_bind_group()`. This is a GPU hazard. To fix this, the transition pipeline should write to a dedicated output texture (e.g., the existing `output_texture`) rather than back to the primary canvas. The implementer should use `output_texture_view` as the blend target, or create a small dedicated "transition output" texture. Adjust the `blend()` call's `target_view` accordingly. The preview panel would then need to sample from this output texture during transitions. Document this decision when implementing.

- [ ] **Step 5: Run `cargo build` to verify compilation**

Run: `cargo build 2>&1 | tail -20`
Expected: Successful compilation. There may be warnings about unused code if Studio Mode UI isn't wired up yet.

- [ ] **Step 6: Commit**

```bash
git add src/main.rs
git commit -m "feat: wire up dual-canvas rendering and transition blend in render loop"
```

---

## Task 6: Scene Switching with Transitions

**Files:**
- Modify: `src/ui/scenes_panel.rs:120-150` (SceneAction::Switch handler)

- [ ] **Step 1: Modify scene switch to start transitions instead of instant switching**

Replace the `SceneAction::Switch` handler (lines 122-143) with transition-aware logic:

```rust
Some(SceneAction::Switch(new_id)) => {
    // Don't switch to the same scene.
    if state.active_scene_id == Some(new_id) {
        // In Studio Mode, clicking the active scene is a no-op.
        // Otherwise, also no-op.
    } else if state.studio_mode {
        // In Studio Mode, clicking a scene sets it as preview.
        state.preview_scene_id = Some(new_id);

        // Start sources for the preview scene (diff against current program + old preview).
        let new_scene = state.scenes.iter().find(|s| s.id == new_id).cloned();
        let old_preview_scene = state
            .preview_scene_id
            .and_then(|id| state.scenes.iter().find(|s| s.id == id))
            .cloned();

        // Start any new sources needed for the preview scene.
        apply_scene_diff(
            &cmd_tx,
            &state.library,
            old_preview_scene.as_ref(),
            new_scene.as_ref(),
            state.settings.general.exclude_self_from_capture,
        );
        state.mark_dirty();
    } else {
        // Normal mode: resolve transition and start it.
        let target_scene = state.scenes.iter().find(|s| s.id == new_id);
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

        match transition_type {
            crate::transition::TransitionType::Cut => {
                // Instant switch — same as before.
                let old_scene = state
                    .active_scene_id
                    .and_then(|id| state.scenes.iter().find(|s| s.id == id))
                    .cloned();
                let new_scene = state.scenes.iter().find(|s| s.id == new_id).cloned();

                state.active_scene_id = Some(new_id);
                state.deselect_all();

                apply_scene_diff(
                    &cmd_tx,
                    &state.library,
                    old_scene.as_ref(),
                    new_scene.as_ref(),
                    state.settings.general.exclude_self_from_capture,
                );

                if let Some(ref scene) = new_scene {
                    state.capture_active = !scene.sources.is_empty();
                }
                state.mark_dirty();
            }
            crate::transition::TransitionType::Fade => {
                if let Some(from_scene_id) = state.active_scene_id {
                    // Start incoming scene sources (don't remove outgoing yet).
                    let old_scene = state.scenes.iter().find(|s| s.id == from_scene_id).cloned();
                    let new_scene = state.scenes.iter().find(|s| s.id == new_id).cloned();

                    // Only ADD new sources — don't remove old ones until transition completes.
                    if let Some(ref new_s) = new_scene {
                        for &src_id in &new_s.source_ids() {
                            let already_running = old_scene
                                .as_ref()
                                .map(|s| s.source_ids().contains(&src_id))
                                .unwrap_or(false);
                            if !already_running {
                                start_capture_source(&cmd_tx, &state.library, src_id,
                                    state.settings.general.exclude_self_from_capture);
                            }
                        }
                    }

                    state.active_transition = Some(crate::transition::TransitionState {
                        from_scene: from_scene_id,
                        to_scene: new_id,
                        transition_type,
                        started_at: std::time::Instant::now(),
                        duration,
                    });
                    state.deselect_all();
                    state.mark_dirty();
                }
            }
        }
    }
}
```

- [ ] **Step 2: Add `start_capture_source` helper function**

Add this helper after `apply_scene_diff()` in `src/ui/scenes_panel.rs`:

```rust
/// Start a single capture source by ID without stopping anything.
fn start_capture_source(
    cmd_tx: &Option<tokio::sync::mpsc::Sender<GstCommand>>,
    library: &[crate::scene::LibrarySource],
    source_id: SourceId,
    exclude_self: bool,
) {
    let Some(tx) = cmd_tx else { return };
    let Some(source) = library.iter().find(|s| s.id == source_id) else { return };

    match &source.properties {
        crate::scene::SourceProperties::Display { screen_index } => {
            let _ = tx.try_send(GstCommand::AddCaptureSource {
                source_id,
                config: CaptureSourceConfig::Screen {
                    screen_index: *screen_index,
                    exclude_self,
                },
            });
        }
        crate::scene::SourceProperties::Window { mode, .. } => {
            let _ = tx.try_send(GstCommand::AddCaptureSource {
                source_id,
                config: CaptureSourceConfig::Window { mode: mode.clone() },
            });
        }
        crate::scene::SourceProperties::Camera { device_index, .. } => {
            let _ = tx.try_send(GstCommand::AddCaptureSource {
                source_id,
                config: CaptureSourceConfig::Camera {
                    device_index: *device_index,
                },
            });
        }
        crate::scene::SourceProperties::Audio { input } => {
            let config = match input {
                crate::scene::AudioInput::Device { device_uid, .. } => {
                    CaptureSourceConfig::AudioDevice {
                        device_uid: device_uid.clone(),
                    }
                }
                crate::scene::AudioInput::File { path, looping } => {
                    CaptureSourceConfig::AudioFile {
                        path: path.clone(),
                        looping: *looping,
                    }
                }
            };
            let _ = tx.try_send(GstCommand::AddCaptureSource {
                source_id,
                config,
            });
        }
        _ => {} // Image, Text, Color, Browser: no capture pipeline.
    }
}
```

- [ ] **Step 3: Add deferred source cleanup on transition completion**

In `src/main.rs`, in the transition completion block (the `if t.is_complete()` section from Task 5), add source cleanup:

```rust
if t.is_complete() {
    let to_scene_id = t.to_scene;
    let from_scene_id = t.from_scene;
    let mut app_state = self.state.lock().expect("lock AppState");
    app_state.active_scene_id = Some(to_scene_id);
    app_state.active_transition = None;

    if !app_state.studio_mode {
        gpu.secondary_canvas = None;

        // Stop sources exclusive to the old scene.
        let old_scene = app_state.scenes.iter().find(|s| s.id == from_scene_id).cloned();
        let new_scene = app_state.scenes.iter().find(|s| s.id == to_scene_id).cloned();
        let new_ids: std::collections::HashSet<crate::scene::SourceId> = new_scene
            .as_ref()
            .map(|s| s.source_ids().into_iter().collect())
            .unwrap_or_default();
        if let Some(ref old_s) = old_scene {
            for &src_id in &old_s.source_ids() {
                if !new_ids.contains(&src_id) {
                    if let Some(ref tx) = app_state.command_tx {
                        let _ = tx.try_send(crate::gstreamer::commands::GstCommand::RemoveCaptureSource {
                            source_id: src_id,
                        });
                    }
                }
            }
        }

        if let Some(ref scene) = new_scene {
            app_state.capture_active = !scene.sources.is_empty();
        }
    } else {
        app_state.preview_scene_id = None;
    }
}
```

- [ ] **Step 4: Run `cargo build` to verify compilation**

Run: `cargo build 2>&1 | tail -20`
Expected: Successful compilation.

- [ ] **Step 5: Commit**

```bash
git add src/ui/scenes_panel.rs src/main.rs
git commit -m "feat: scene switching triggers transitions with deferred source cleanup"
```

---

## Task 7: Transition Bar UI in Scenes Panel

**Files:**
- Modify: `src/ui/scenes_panel.rs` (add transition bar below scene thumbnails)

- [ ] **Step 1: Add transition bar UI**

After the scene grid rendering and before the deferred action handler (around line 118), add the transition controls bar:

```rust
// ── Transition bar ──
ui.add_space(4.0);
{
    let theme = active_theme(&state.settings);
    let bar_rect = ui.available_rect_before_wrap();
    let bar_rect = egui::Rect::from_min_size(
        bar_rect.min,
        egui::vec2(bar_rect.width(), 32.0),
    );
    ui.allocate_rect(bar_rect, egui::Sense::hover());

    let painter = ui.painter_at(bar_rect);
    painter.rect_filled(bar_rect, 0.0, theme.panel_bg());

    let mut cursor_x = bar_rect.min.x + 8.0;
    let center_y = bar_rect.center().y;

    // Type toggle: Fade / Cut
    let is_fade = state.settings.transitions.default_type == crate::transition::TransitionType::Fade;
    for (label, is_selected, tt) in [
        ("Fade", is_fade, crate::transition::TransitionType::Fade),
        ("Cut", !is_fade, crate::transition::TransitionType::Cut),
    ] {
        let text_color = if is_selected { theme.text_primary() } else { theme.text_muted() };
        let btn_rect = egui::Rect::from_center_size(
            egui::pos2(cursor_x + 20.0, center_y),
            egui::vec2(40.0, 22.0),
        );
        let response = ui.allocate_rect(btn_rect, egui::Sense::click());
        if is_selected {
            painter.rect_filled(btn_rect, 3.0, theme.surface_raised());
        }
        painter.text(
            btn_rect.center(),
            egui::Align2::CENTER_CENTER,
            label,
            egui::FontId::proportional(11.0),
            text_color,
        );
        if response.clicked() {
            state.settings.transitions.default_type = tt;
            state.mark_dirty();
        }
        cursor_x += 44.0;
    }

    cursor_x += 8.0;

    // Duration input
    let duration_rect = egui::Rect::from_min_size(
        egui::pos2(cursor_x, center_y - 10.0),
        egui::vec2(42.0, 20.0),
    );
    let mut duration_str = state.settings.transitions.default_duration_ms.to_string();
    let duration_response = ui.put(
        duration_rect,
        egui::TextEdit::singleline(&mut duration_str)
            .desired_width(36.0)
            .font(egui::FontId::proportional(11.0)),
    );
    if duration_response.changed() {
        if let Ok(ms) = duration_str.parse::<u32>() {
            state.settings.transitions.default_duration_ms = ms.clamp(0, 10000);
            state.mark_dirty();
        }
    }
    painter.text(
        egui::pos2(duration_rect.right() + 4.0, center_y),
        egui::Align2::LEFT_CENTER,
        "ms",
        egui::FontId::proportional(11.0),
        theme.text_muted(),
    );

    // Studio Mode toggle (right-aligned)
    let studio_btn_rect = egui::Rect::from_min_size(
        egui::pos2(bar_rect.right() - 56.0, center_y - 10.0),
        egui::vec2(48.0, 20.0),
    );
    let studio_response = ui.allocate_rect(studio_btn_rect, egui::Sense::click());
    let studio_bg = if state.studio_mode {
        egui::Color32::from_rgba_unmultiplied(
            state.accent_color.r(),
            state.accent_color.g(),
            state.accent_color.b(),
            60,
        )
    } else {
        theme.surface_raised()
    };
    painter.rect_filled(studio_btn_rect, 4.0, studio_bg);
    painter.text(
        studio_btn_rect.center(),
        egui::Align2::CENTER_CENTER,
        "Studio",
        egui::FontId::proportional(11.0),
        if state.studio_mode { state.accent_color } else { theme.text_muted() },
    );
    if studio_response.clicked() {
        state.studio_mode = !state.studio_mode;
        if !state.studio_mode {
            state.preview_scene_id = None;
        }
    }

    // Transition button (only in Studio Mode, left of Studio button)
    if state.studio_mode {
        let trans_btn_rect = egui::Rect::from_min_size(
            egui::pos2(studio_btn_rect.left() - 80.0, center_y - 11.0),
            egui::vec2(72.0, 22.0),
        );
        let trans_response = ui.allocate_rect(trans_btn_rect, egui::Sense::click());
        let has_preview = state.preview_scene_id.is_some();
        let btn_color = if has_preview {
            egui::Color32::from_rgb(247, 118, 142) // #f7768e
        } else {
            theme.text_muted()
        };
        painter.rect_filled(trans_btn_rect, 4.0, btn_color);
        painter.text(
            trans_btn_rect.center(),
            egui::Align2::CENTER_CENTER,
            "Transition",
            egui::FontId::proportional(12.0),
            egui::Color32::from_rgb(17, 17, 22),
        );
        if trans_response.clicked() && has_preview {
            if let (Some(from_id), Some(to_id)) = (state.active_scene_id, state.preview_scene_id) {
                let target_scene = state.scenes.iter().find(|s| s.id == to_id);
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

                state.active_transition = Some(crate::transition::TransitionState {
                    from_scene: from_id,
                    to_scene: to_id,
                    transition_type,
                    started_at: std::time::Instant::now(),
                    duration,
                });
            }
        }
    }
}
```

- [ ] **Step 2: Add PGM/PRV badges to scene thumbnails in Studio Mode**

In the `draw_scene_card()` function, after drawing the scene thumbnail background, add badge rendering:

```rust
// Studio Mode badges
if state.studio_mode {
    let is_program = state.active_scene_id == Some(scene_id);
    let is_preview = state.preview_scene_id == Some(scene_id);
    if is_program || is_preview {
        let (badge_text, badge_color) = if is_program {
            ("PGM", egui::Color32::from_rgb(247, 118, 142))
        } else {
            ("PRV", egui::Color32::from_rgb(158, 206, 106))
        };
        let badge_rect = egui::Rect::from_min_size(
            egui::pos2(thumb_rect.right() - 28.0, thumb_rect.top() - 4.0),
            egui::vec2(24.0, 12.0),
        );
        painter.rect_filled(badge_rect, 3.0, badge_color);
        painter.text(
            badge_rect.center(),
            egui::Align2::CENTER_CENTER,
            badge_text,
            egui::FontId::proportional(8.0),
            egui::Color32::from_rgb(17, 17, 22),
        );
    }
}
```

- [ ] **Step 3: Run `cargo build` to verify compilation**

Run: `cargo build 2>&1 | tail -20`
Expected: Successful compilation.

- [ ] **Step 4: Commit**

```bash
git add src/ui/scenes_panel.rs
git commit -m "feat: add transition bar UI with type toggle, duration, and Studio Mode controls"
```

---

## Task 8: Studio Mode Preview Panel Split

**Files:**
- Modify: `src/ui/preview_panel.rs`
- Modify: `src/main.rs` (update PreviewResources for dual canvas)

- [ ] **Step 1: Add secondary canvas bind group to PreviewResources**

In `src/ui/preview_panel.rs`, extend `PreviewResources`:

```rust
pub struct PreviewResources {
    pub pipeline: Arc<wgpu::RenderPipeline>,
    pub bind_group: Arc<wgpu::BindGroup>,
    /// Bind group for the secondary canvas (Studio Mode / transition preview).
    /// None when secondary canvas is not allocated.
    pub secondary_bind_group: Option<Arc<wgpu::BindGroup>>,
}
```

- [ ] **Step 2: Update PreviewResources insertion in main.rs**

Wherever `PreviewResources` is created/updated (around lines 1315-1319 and GPU init), include the secondary bind group:

```rust
let new_resources = PreviewResources {
    pipeline: gpu.compositor.canvas_pipeline(),
    bind_group: gpu.compositor.canvas_bind_group(),
    secondary_bind_group: gpu.secondary_canvas.as_ref().map(|sc| Arc::clone(&sc.bind_group)),
};
win.egui_renderer.callback_resources.insert(new_resources);
```

Also update the preview resources after each frame (at the end of the about_to_wait composition section) to keep the secondary bind group in sync.

- [ ] **Step 3: Modify preview panel to split in Studio Mode**

In the preview panel's main rendering function, check `state.studio_mode` and render two panes side-by-side when active:

```rust
if state.studio_mode {
    // Split available rect into two halves with a gap.
    let available = ui.available_rect_before_wrap();
    let gap = 8.0;
    let half_width = (available.width() - gap) / 2.0;

    // Left: Preview pane
    let preview_rect = egui::Rect::from_min_size(
        available.min,
        egui::vec2(half_width, available.height()),
    );
    // Right: Program pane
    let program_rect = egui::Rect::from_min_size(
        egui::pos2(available.min.x + half_width + gap, available.min.y),
        egui::vec2(half_width, available.height()),
    );

    // Labels
    let theme = active_theme(&state.settings);
    let painter = ui.painter();

    // Preview label (green)
    let preview_color = egui::Color32::from_rgb(158, 206, 106);
    painter.circle_filled(
        egui::pos2(preview_rect.min.x + 10.0, preview_rect.min.y + 10.0),
        4.0,
        preview_color,
    );
    painter.text(
        egui::pos2(preview_rect.min.x + 20.0, preview_rect.min.y + 10.0),
        egui::Align2::LEFT_CENTER,
        "PREVIEW",
        egui::FontId::proportional(11.0),
        preview_color,
    );

    // Program label (red)
    let program_color = egui::Color32::from_rgb(247, 118, 142);
    painter.circle_filled(
        egui::pos2(program_rect.min.x + 10.0, program_rect.min.y + 10.0),
        4.0,
        program_color,
    );
    painter.text(
        egui::pos2(program_rect.min.x + 20.0, program_rect.min.y + 10.0),
        egui::Align2::LEFT_CENTER,
        "PROGRAM",
        egui::FontId::proportional(11.0),
        program_color,
    );

    // Draw preview canvas (secondary bind group) in left pane.
    // Draw program canvas (primary bind group) in right pane.
    // Use the same PreviewCallback but with different bind groups and rects.
    // The implementer should create two egui paint callbacks — one for each pane,
    // each sampling from the appropriate bind group.

    // Allocate both rects to prevent other UI from overlapping.
    ui.allocate_rect(preview_rect, egui::Sense::hover());
    ui.allocate_rect(program_rect, egui::Sense::hover());

    // Paint callbacks for each pane — render_preview_quad() with appropriate bind group.
    // This reuses the existing PreviewCallback infrastructure.
}
```

The implementer should create the actual paint callbacks for each pane based on the existing `PreviewCallback` pattern, passing the correct bind group (`secondary_bind_group` for the preview pane, `bind_group` for the program pane).

- [ ] **Step 4: Add transition progress bar during active transition**

In the program pane (right side in Studio Mode, or the single pane in Normal Mode), add a progress bar when a transition is active:

```rust
if state.active_transition.is_some() {
    if let Some(ref t) = state.active_transition {
        let progress = t.progress();
        let bar_height = 3.0;
        let bar_rect = egui::Rect::from_min_size(
            egui::pos2(pane_rect.min.x, pane_rect.max.y - bar_height),
            egui::vec2(pane_rect.width() * progress, bar_height),
        );
        painter.rect_filled(bar_rect, 0.0, egui::Color32::from_rgb(224, 175, 104));
    }
    ui.ctx().request_repaint();
}
```

- [ ] **Step 5: Run `cargo build` to verify compilation**

Run: `cargo build 2>&1 | tail -20`
Expected: Successful compilation.

- [ ] **Step 6: Commit**

```bash
git add src/ui/preview_panel.rs src/main.rs
git commit -m "feat: Studio Mode dual-pane preview with transition progress indicator"
```

---

## Task 9: Hotkeys

**Files:**
- Modify: `src/main.rs` (keyboard event handling)

- [ ] **Step 1: Find the keyboard input handler**

Search for where `winit::event::KeyEvent` or `keyboard_input` is handled in `src/main.rs`. This is where hotkeys are processed.

- [ ] **Step 2: Add transition hotkeys**

Add handlers for the following keys (only when not in a text edit):

```rust
// Quick cut: Space
KeyCode::Space if !text_editing => {
    let mut state = self.state.lock().expect("lock AppState");
    if state.studio_mode {
        if let (Some(from_id), Some(to_id)) = (state.active_scene_id, state.preview_scene_id) {
            // Cancel any in-flight transition.
            state.active_transition = None;
            // Instant switch.
            let old_scene = state.scenes.iter().find(|s| s.id == from_id).cloned();
            let new_scene = state.scenes.iter().find(|s| s.id == to_id).cloned();
            state.active_scene_id = Some(to_id);
            state.preview_scene_id = None;
            state.deselect_all();
            // Source diff.
            apply_scene_diff(&state.command_tx, &state.library,
                old_scene.as_ref(), new_scene.as_ref(),
                state.settings.general.exclude_self_from_capture);
            state.mark_dirty();
        }
    }
}

// Trigger transition: Enter
KeyCode::Enter if !text_editing => {
    let mut state = self.state.lock().expect("lock AppState");
    if state.studio_mode && state.active_transition.is_none() {
        if let (Some(from_id), Some(to_id)) = (state.active_scene_id, state.preview_scene_id) {
            let target_scene = state.scenes.iter().find(|s| s.id == to_id);
            let (tt, dur) = target_scene
                .map(|s| crate::transition::resolve_transition(
                    &state.settings.transitions, &s.transition_override))
                .unwrap_or((crate::transition::TransitionType::Fade,
                    std::time::Duration::from_millis(300)));
            state.active_transition = Some(crate::transition::TransitionState {
                from_scene: from_id,
                to_scene: to_id,
                transition_type: tt,
                started_at: std::time::Instant::now(),
                duration: dur,
            });
        }
    }
}

// Toggle Studio Mode: Ctrl+S (Cmd+S on macOS)
KeyCode::KeyS if modifiers.control_key() || modifiers.super_key() => {
    let mut state = self.state.lock().expect("lock AppState");
    state.studio_mode = !state.studio_mode;
    if !state.studio_mode {
        state.preview_scene_id = None;
    }
}

// Scene select by number: 1-9
KeyCode::Digit1..=KeyCode::Digit9 if !text_editing => {
    let index = (key as u32 - KeyCode::Digit1 as u32) as usize;
    let mut state = self.state.lock().expect("lock AppState");
    if let Some(scene) = state.scenes.get(index) {
        let scene_id = scene.id;
        if state.studio_mode {
            state.preview_scene_id = Some(scene_id);
        } else {
            // Trigger transition (reuse the same logic as SceneAction::Switch).
            // The implementer should call into a shared function rather than duplicating.
        }
    }
}
```

**Note:** The exact key code matching syntax depends on how the existing hotkeys are handled. Match the pattern used by existing hotkey code in the file.

- [ ] **Step 3: Run `cargo build` to verify compilation**

Run: `cargo build 2>&1 | tail -20`
Expected: Successful compilation.

- [ ] **Step 4: Commit**

```bash
git add src/main.rs
git commit -m "feat: add hotkeys for transitions (Enter, Space, Ctrl+S, 1-9)"
```

---

## Task 10: Per-Scene Transition Override UI

**Files:**
- Modify: `src/ui/scenes_panel.rs` (scene context menu)

- [ ] **Step 1: Add "Transition Override" to the scene context menu**

Find the existing right-click context menu for scene cards in `draw_scene_card()`. Add a "Transition Override" submenu:

```rust
// Inside the existing context menu for the scene card:
ui.menu_button("Transition Override", |ui| {
    ui.label("Transition into this scene:");

    // Type selector
    ui.horizontal(|ui| {
        ui.label("Type:");
        let current = scene_override.transition_type;
        for (label, value) in [
            ("Default", None),
            ("Fade", Some(crate::transition::TransitionType::Fade)),
            ("Cut", Some(crate::transition::TransitionType::Cut)),
        ] {
            if ui.selectable_label(current == value, label).clicked() {
                // Update the scene's transition_override.transition_type
                if let Some(scene) = state.scenes.iter_mut().find(|s| s.id == scene_id) {
                    scene.transition_override.transition_type = value;
                    state.mark_dirty();
                }
                ui.close_menu();
            }
        }
    });

    // Duration input
    ui.horizontal(|ui| {
        ui.label("Duration:");
        let mut duration_str = state
            .scenes
            .iter()
            .find(|s| s.id == scene_id)
            .and_then(|s| s.transition_override.duration_ms)
            .map(|ms| ms.to_string())
            .unwrap_or_default();
        let response = ui.add(
            egui::TextEdit::singleline(&mut duration_str)
                .desired_width(40.0)
                .hint_text("default"),
        );
        if response.changed() {
            if let Some(scene) = state.scenes.iter_mut().find(|s| s.id == scene_id) {
                scene.transition_override.duration_ms = duration_str.parse::<u32>().ok();
                state.mark_dirty();
            }
        }
        ui.label("ms");
    });
});
```

- [ ] **Step 2: Run `cargo build` to verify compilation**

Run: `cargo build 2>&1 | tail -20`
Expected: Successful compilation.

- [ ] **Step 3: Commit**

```bash
git add src/ui/scenes_panel.rs
git commit -m "feat: per-scene transition override in context menu"
```

---

## Task 11: Unit Tests

**Files:**
- Modify: `src/transition.rs` (add tests module)

- [ ] **Step 1: Add unit tests for transition types**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transition_state_progress_at_start() {
        let state = TransitionState {
            from_scene: SceneId(1),
            to_scene: SceneId(2),
            transition_type: TransitionType::Fade,
            started_at: Instant::now(),
            duration: Duration::from_millis(300),
        };
        assert!(state.progress() < 0.1);
        assert!(!state.is_complete());
    }

    #[test]
    fn transition_state_progress_when_complete() {
        let state = TransitionState {
            from_scene: SceneId(1),
            to_scene: SceneId(2),
            transition_type: TransitionType::Fade,
            started_at: Instant::now() - Duration::from_millis(500),
            duration: Duration::from_millis(300),
        };
        assert_eq!(state.progress(), 1.0);
        assert!(state.is_complete());
    }

    #[test]
    fn transition_state_zero_duration_is_immediately_complete() {
        let state = TransitionState {
            from_scene: SceneId(1),
            to_scene: SceneId(2),
            transition_type: TransitionType::Cut,
            started_at: Instant::now(),
            duration: Duration::ZERO,
        };
        assert_eq!(state.progress(), 1.0);
        assert!(state.is_complete());
    }

    #[test]
    fn resolve_uses_global_defaults() {
        let global = TransitionSettings {
            default_type: TransitionType::Fade,
            default_duration_ms: 500,
        };
        let override_ = SceneTransitionOverride::default();
        let (t, d) = resolve_transition(&global, &override_);
        assert_eq!(t, TransitionType::Fade);
        assert_eq!(d, Duration::from_millis(500));
    }

    #[test]
    fn resolve_per_scene_overrides_global() {
        let global = TransitionSettings {
            default_type: TransitionType::Fade,
            default_duration_ms: 300,
        };
        let override_ = SceneTransitionOverride {
            transition_type: Some(TransitionType::Cut),
            duration_ms: Some(0),
        };
        let (t, d) = resolve_transition(&global, &override_);
        assert_eq!(t, TransitionType::Cut);
        assert_eq!(d, Duration::ZERO);
    }

    #[test]
    fn resolve_partial_override() {
        let global = TransitionSettings {
            default_type: TransitionType::Fade,
            default_duration_ms: 300,
        };
        let override_ = SceneTransitionOverride {
            transition_type: None,
            duration_ms: Some(1000),
        };
        let (t, d) = resolve_transition(&global, &override_);
        assert_eq!(t, TransitionType::Fade); // from global
        assert_eq!(d, Duration::from_millis(1000)); // from override
    }

    #[test]
    fn transition_settings_default() {
        let s = TransitionSettings::default();
        assert_eq!(s.default_type, TransitionType::Fade);
        assert_eq!(s.default_duration_ms, 300);
    }

    #[test]
    fn scene_transition_override_default_is_none() {
        let o = SceneTransitionOverride::default();
        assert!(o.transition_type.is_none());
        assert!(o.duration_ms.is_none());
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test transition 2>&1 | tail -20`
Expected: All 7 tests pass.

- [ ] **Step 3: Run full test suite**

Run: `cargo test 2>&1 | tail -20`
Expected: All tests pass (including existing compositor tests).

- [ ] **Step 4: Commit**

```bash
git add src/transition.rs
git commit -m "test: add unit tests for transition state, resolution, and config defaults"
```

---

## Task 12: Final Build Verification and Clippy

**Files:** None (verification only)

- [ ] **Step 1: Run clippy**

Run: `cargo clippy 2>&1 | tail -30`
Expected: No errors. Fix any warnings in changed files.

- [ ] **Step 2: Run fmt check**

Run: `cargo fmt --check 2>&1`
Expected: No formatting issues. If any, run `cargo fmt` and commit.

- [ ] **Step 3: Run full test suite one final time**

Run: `cargo test 2>&1 | tail -20`
Expected: All tests pass.

- [ ] **Step 4: Final commit if any fixes were needed**

```bash
git add -A
git commit -m "chore: fix clippy warnings and formatting"
```
