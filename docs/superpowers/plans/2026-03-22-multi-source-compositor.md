# Multi-Source Compositor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** GPU-based wgpu compositor that blends multiple capture sources into a scene canvas with per-source transforms and opacity.

**Architecture:** GStreamer manages multiple capture pipelines (one per source) writing latest frames to a shared `Arc<Mutex<HashMap<SourceId, RgbaFrame>>>`. The render thread drains frames into per-source wgpu textures, composites them onto a canvas texture in scene vec order using a custom shader, and reads back the composited frame for encoding via a new channel back to GStreamer.

**Tech Stack:** wgpu 29, GStreamer 0.23, egui 0.33, tokio channels, bytemuck

**Spec:** `docs/superpowers/specs/2026-03-22-multi-source-compositor-design.md`

---

## File Structure

| File | Action | Responsibility |
|------|--------|----------------|
| `src/renderer/compositor.rs` | Create | GPU compositor: canvas texture, source layer pool, compose shader, readback |
| `src/renderer/mod.rs` | Modify | Add `pub mod compositor`, add `Compositor` to `SharedGpuState` |
| `src/renderer/preview.rs` | Modify | Remove texture/upload, sample compositor's canvas bind group instead |
| `src/scene.rs` | Modify | Add `opacity` field to `Source`, add reorder methods to `Scene` |
| `src/gstreamer/commands.rs` | Modify | Add `AddCaptureSource`/`RemoveCaptureSource` commands, replace frame channel with shared map + composited frame channel |
| `src/gstreamer/thread.rs` | Modify | Multi-capture `HashMap<SourceId, CaptureHandle>`, iterate captures in run loop, receive composited frames for encoding |
| `src/gstreamer/mod.rs` | Modify | Re-export new types |
| `src/state.rs` | Modify | Update channel types, remove preview dimensions |
| `src/main.rs` | Modify | Wire compositor: drain frames, compose, readback, scene switch diffing |
| `src/window.rs` | Modify | Update preview resource wiring |
| `src/ui/scene_editor.rs` | Modify | Multi-source UI: list all sources, add/remove, reorder, opacity slider |
| `src/ui/preview_panel.rs` | Modify | Accept compositor's canvas bind group |

---

### Task 1: Add `opacity` field to `Source` and reorder methods to `Scene`

**Files:**
- Modify: `src/scene.rs:19-29` (Source struct), `src/scene.rs:12-16` (Scene struct)

- [ ] **Step 1: Write failing tests for opacity serialization and source reordering**

In `src/scene.rs`, add to the existing `#[cfg(test)] mod tests` block:

```rust
#[test]
fn source_opacity_defaults_to_one() {
    let toml_str = r#"
        id = 1
        name = "Test"
        source_type = "Display"
        visible = true
        muted = false
        volume = 1.0
        [properties]
        Display = { screen_index = 0 }
        [transform]
        x = 0.0
        y = 0.0
        width = 1920.0
        height = 1080.0
    "#;
    let source: Source = toml::from_str(toml_str).unwrap();
    assert!((source.opacity - 1.0).abs() < f32::EPSILON);
}

#[test]
fn source_opacity_roundtrips() {
    let source = Source {
        id: SourceId(1),
        name: "Test".into(),
        source_type: SourceType::Display,
        properties: SourceProperties::default(),
        transform: Transform { x: 0.0, y: 0.0, width: 1920.0, height: 1080.0 },
        opacity: 0.5,
        visible: true,
        muted: false,
        volume: 1.0,
    };
    let serialized = toml::to_string(&source).unwrap();
    let deserialized: Source = toml::from_str(&serialized).unwrap();
    assert!((deserialized.opacity - 0.5).abs() < f32::EPSILON);
}

#[test]
fn scene_move_source_up() {
    let mut scene = Scene {
        id: SceneId(1),
        name: "Test".into(),
        sources: vec![SourceId(1), SourceId(2), SourceId(3)],
    };
    scene.move_source_up(SourceId(2));
    assert_eq!(scene.sources, vec![SourceId(2), SourceId(1), SourceId(3)]);
}

#[test]
fn scene_move_source_up_already_first() {
    let mut scene = Scene {
        id: SceneId(1),
        name: "Test".into(),
        sources: vec![SourceId(1), SourceId(2)],
    };
    scene.move_source_up(SourceId(1));
    assert_eq!(scene.sources, vec![SourceId(1), SourceId(2)]);
}

#[test]
fn scene_move_source_down() {
    let mut scene = Scene {
        id: SceneId(1),
        name: "Test".into(),
        sources: vec![SourceId(1), SourceId(2), SourceId(3)],
    };
    scene.move_source_down(SourceId(1));
    assert_eq!(scene.sources, vec![SourceId(2), SourceId(1), SourceId(3)]);
}

#[test]
fn scene_move_source_down_already_last() {
    let mut scene = Scene {
        id: SceneId(1),
        name: "Test".into(),
        sources: vec![SourceId(1), SourceId(2)],
    };
    scene.move_source_down(SourceId(2));
    assert_eq!(scene.sources, vec![SourceId(1), SourceId(2)]);
}
```

**Note:** The TOML format for `SourceProperties` uses serde's default externally-tagged enum representation. Verify the test's inline TOML matches what `toml::to_string(&source)` actually produces — run an existing serialization test first to see the format. Adjust the test's `[properties]` section if needed.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test scene -- --nocapture 2>&1 | tail -20`
Expected: compilation failures — `opacity` field missing, `move_source_up`/`move_source_down` don't exist.

- [ ] **Step 3: Add `opacity` to `Source` and reorder methods to `Scene`**

In `src/scene.rs`, add the default function before the `Source` struct:

```rust
fn default_opacity() -> f32 {
    1.0
}
```

Add the `opacity` field to `Source` (after `transform`):

```rust
pub struct Source {
    pub id: SourceId,
    pub name: String,
    pub source_type: SourceType,
    #[serde(default)]
    pub properties: SourceProperties,
    pub transform: Transform,
    #[serde(default = "default_opacity")]
    pub opacity: f32,
    pub visible: bool,
    pub muted: bool,
    pub volume: f32,
}
```

Add reorder methods to `Scene`:

```rust
impl Scene {
    /// Move a source one position earlier (lower z-index / further back).
    pub fn move_source_up(&mut self, source_id: SourceId) {
        if let Some(pos) = self.sources.iter().position(|&id| id == source_id) {
            if pos > 0 {
                self.sources.swap(pos, pos - 1);
            }
        }
    }

    /// Move a source one position later (higher z-index / further forward).
    pub fn move_source_down(&mut self, source_id: SourceId) {
        if let Some(pos) = self.sources.iter().position(|&id| id == source_id) {
            if pos + 1 < self.sources.len() {
                self.sources.swap(pos, pos + 1);
            }
        }
    }
}
```

Fix any existing code that constructs `Source` without `opacity` — add `opacity: 1.0` to all construction sites (search for `Source {` in `scene.rs` and `scene_editor.rs`).

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test scene -- --nocapture 2>&1 | tail -20`
Expected: all scene tests pass including the 6 new ones.

- [ ] **Step 5: Run clippy and fix any warnings**

Run: `cargo clippy 2>&1 | tail -10`

- [ ] **Step 6: Commit**

```bash
git add src/scene.rs src/ui/scene_editor.rs
git commit -m "feat: add opacity field to Source and reorder methods to Scene"
```

---

### Task 2: Add new GStreamer commands and replace frame channel with shared map

**Files:**
- Modify: `src/gstreamer/commands.rs:36-60` (GstCommand enum), `src/gstreamer/commands.rs:122-171` (channels)
- Modify: `src/gstreamer/mod.rs` (re-exports)

- [ ] **Step 1: Write failing tests for new commands and channel types**

In `src/gstreamer/commands.rs`, add to existing `#[cfg(test)] mod tests`:

```rust
use crate::scene::SourceId;

#[test]
fn create_channels_with_shared_frames() {
    let (main_ch, thread_ch) = create_channels();
    // Verify latest_frames is shared between both sides
    let source_id = SourceId(1);
    let frame = RgbaFrame {
        data: vec![0u8; 4],
        width: 1,
        height: 1,
    };
    thread_ch.latest_frames.lock().unwrap().insert(source_id, frame);
    let frames = main_ch.latest_frames.lock().unwrap();
    assert!(frames.contains_key(&source_id));
}

#[test]
fn composited_frame_channel_works() {
    let (main_ch, _thread_ch) = create_channels();
    let frame = RgbaFrame {
        data: vec![0u8; 4],
        width: 1,
        height: 1,
    };
    main_ch.composited_frame_tx.try_send(frame).unwrap();
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test commands -- --nocapture 2>&1 | tail -20`
Expected: compilation failures.

- [ ] **Step 3: Implement the command and channel changes**

In `src/gstreamer/commands.rs`:

Add import at top:

```rust
use crate::scene::SourceId;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
```

Add new variants to `GstCommand`:

```rust
pub enum GstCommand {
    // ... existing variants ...
    /// Add a new capture source.
    AddCaptureSource {
        source_id: SourceId,
        config: CaptureSourceConfig,
    },
    /// Remove a capture source.
    RemoveCaptureSource {
        source_id: SourceId,
    },
}
```

Update `GstChannels` (main-thread side):

```rust
pub struct GstChannels {
    pub command_tx: mpsc::Sender<GstCommand>,
    pub latest_frames: Arc<Mutex<HashMap<SourceId, RgbaFrame>>>,
    pub composited_frame_tx: mpsc::Sender<RgbaFrame>,
    #[allow(dead_code)]
    pub stats_rx: watch::Receiver<PipelineStats>,
    pub error_rx: mpsc::UnboundedReceiver<GstError>,
    pub audio_level_rx: watch::Receiver<AudioLevelUpdate>,
    pub devices_rx: watch::Receiver<Vec<AudioDevice>>,
}
```

Update `GstThreadChannels` (gstreamer-thread side):

```rust
pub(crate) struct GstThreadChannels {
    pub command_rx: mpsc::Receiver<GstCommand>,
    pub latest_frames: Arc<Mutex<HashMap<SourceId, RgbaFrame>>>,
    pub composited_frame_rx: mpsc::Receiver<RgbaFrame>,
    #[allow(dead_code)]
    pub stats_tx: watch::Sender<PipelineStats>,
    pub error_tx: mpsc::UnboundedSender<GstError>,
    pub audio_level_tx: watch::Sender<AudioLevelUpdate>,
    pub devices_tx: watch::Sender<Vec<AudioDevice>>,
}
```

Update `create_channels()`:

```rust
pub fn create_channels() -> (GstChannels, GstThreadChannels) {
    let (command_tx, command_rx) = mpsc::channel(16);
    let latest_frames = Arc::new(Mutex::new(HashMap::new()));
    let (composited_frame_tx, composited_frame_rx) = mpsc::channel(2);
    let (stats_tx, stats_rx) = watch::channel(PipelineStats::default());
    let (error_tx, error_rx) = mpsc::unbounded_channel();
    let (audio_level_tx, audio_level_rx) = watch::channel(AudioLevelUpdate::default());
    let (devices_tx, devices_rx) = watch::channel(Vec::new());

    let main_channels = GstChannels {
        command_tx,
        latest_frames: Arc::clone(&latest_frames),
        composited_frame_tx,
        stats_rx,
        error_rx,
        audio_level_rx,
        devices_rx,
    };

    let thread_channels = GstThreadChannels {
        command_rx,
        latest_frames,
        composited_frame_rx,
        stats_tx,
        error_tx,
        audio_level_tx,
        devices_tx,
    };

    (main_channels, thread_channels)
}
```

In `src/gstreamer/mod.rs`, add `SourceId` is already available via `crate::scene`. No new re-exports needed for SourceId.

- [ ] **Step 4: Fix compilation errors in dependent code**

The `frame_rx` and `frame_tx` fields are removed. Update all references:

- `src/gstreamer/thread.rs`: Replace `self.channels.frame_tx.try_send(frame)` with `self.channels.latest_frames.lock().unwrap().insert(source_id, frame)` — this will be fully reworked in Task 4, so for now just make it compile by writing to the shared map with a placeholder `SourceId(0)`.
- `src/main.rs:613`: Replace `channels.frame_rx.try_recv()` with reading from `channels.latest_frames` — temporary: drain the map and upload the first frame found. This will be fully reworked in Task 6.
- `src/state.rs`: Remove `preview_width` and `preview_height` fields from `AppState` (lines 48-49) — the compositor owns canvas dimensions now. Update `capture_active` semantics: this field should reflect whether any captures are running (check `captures.is_empty()` on the GStreamer side). Remove any test references to the removed fields.

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test 2>&1 | tail -20`
Expected: all tests pass.

- [ ] **Step 6: Run clippy**

Run: `cargo clippy 2>&1 | tail -10`

- [ ] **Step 7: Commit**

```bash
git add src/gstreamer/commands.rs src/gstreamer/mod.rs src/gstreamer/thread.rs src/main.rs
git commit -m "feat: add multi-source commands and shared frame map"
```

---

### Task 3: Create the GPU compositor module

**Files:**
- Create: `src/renderer/compositor.rs`
- Modify: `src/renderer/mod.rs:1` (add `pub mod compositor`)

- [ ] **Step 1: Write failing tests for compositor types**

Create `src/renderer/compositor.rs` with test module first:

```rust
use std::collections::HashMap;

use bytemuck::{Pod, Zeroable};
use egui_wgpu::wgpu;
use egui_wgpu::wgpu::{Device, Queue};

use crate::gstreamer::RgbaFrame;
use crate::scene::{SourceId, Transform};

/// Per-source GPU state.
struct SourceLayer {
    texture: wgpu::Texture,
    texture_view: wgpu::TextureView,
    bind_group: wgpu::BindGroup,
    size: (u32, u32),
}

/// Transform uniforms pushed per draw call.
/// `compose()` converts pixel-space Transform values to normalized 0.0..1.0.
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
struct SourceUniforms {
    /// Normalized x, y, width, height on canvas (0.0..1.0)
    rect: [f32; 4],
    /// Source opacity (0.0 = transparent, 1.0 = opaque)
    opacity: f32,
    _padding: [f32; 3],
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_uniforms_is_32_bytes() {
        assert_eq!(std::mem::size_of::<SourceUniforms>(), 32);
    }

    #[test]
    fn source_uniforms_normalizes_transform() {
        let transform = Transform {
            x: 480.0,
            y: 270.0,
            width: 960.0,
            height: 540.0,
        };
        let canvas_w = 1920u32;
        let canvas_h = 1080u32;
        let uniforms = SourceUniforms {
            rect: [
                transform.x / canvas_w as f32,
                transform.y / canvas_h as f32,
                transform.width / canvas_w as f32,
                transform.height / canvas_h as f32,
            ],
            opacity: 0.75,
            _padding: [0.0; 3],
        };
        assert!((uniforms.rect[0] - 0.25).abs() < f32::EPSILON);
        assert!((uniforms.rect[1] - 0.25).abs() < f32::EPSILON);
        assert!((uniforms.rect[2] - 0.5).abs() < f32::EPSILON);
        assert!((uniforms.rect[3] - 0.5).abs() < f32::EPSILON);
        assert!((uniforms.opacity - 0.75).abs() < f32::EPSILON);
    }
}
```

- [ ] **Step 2: Add `pub mod compositor` to `src/renderer/mod.rs`**

Add at the top of `src/renderer/mod.rs`:

```rust
pub mod compositor;
```

- [ ] **Step 3: Run tests to verify they pass**

Run: `cargo test compositor -- --nocapture 2>&1 | tail -20`
Expected: 2 tests pass.

- [ ] **Step 4: Implement the compositor shader**

Add the compositor shader constant to `src/renderer/compositor.rs`:

```rust
const COMPOSITOR_SHADER: &str = r#"
struct SourceUniforms {
    rect: vec4<f32>,
    opacity: f32,
    _padding1: f32,
    _padding2: f32,
    _padding3: f32,
};

@group(0) @binding(0) var t_source: texture_2d<f32>;
@group(0) @binding(1) var s_source: sampler;
@group(1) @binding(0) var<uniform> uniforms: SourceUniforms;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    // Generate quad vertices from rect uniform (normalized 0..1 coords → NDC -1..1)
    let u = f32(vertex_index & 1u);
    let v = f32(vertex_index >> 1u);

    let x = uniforms.rect.x + u * uniforms.rect.z;
    let y = uniforms.rect.y + v * uniforms.rect.w;

    // Convert from 0..1 to NDC (-1..1), flip Y
    let ndc_x = x * 2.0 - 1.0;
    let ndc_y = 1.0 - y * 2.0;

    var out: VertexOutput;
    out.position = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
    out.uv = vec2<f32>(u, v);
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let color = textureSample(t_source, s_source, in.uv);
    return vec4<f32>(color.rgb, color.a * uniforms.opacity);
}
"#;
```

- [ ] **Step 5: Implement the `Compositor` struct and `new()`**

```rust
/// GPU compositor that blends multiple source textures onto a canvas.
pub struct Compositor {
    canvas_texture: wgpu::Texture,
    canvas_view: wgpu::TextureView,
    pub canvas_width: u32,
    pub canvas_height: u32,
    pipeline: wgpu::RenderPipeline,
    sampler: wgpu::Sampler,
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group_layout: wgpu::BindGroupLayout,
    uniform_bind_group: wgpu::BindGroup,
    texture_bind_group_layout: wgpu::BindGroupLayout,
    source_layers: HashMap<SourceId, SourceLayer>,
    readback_buffer: wgpu::Buffer,
    /// Bind group for the canvas texture (used by preview).
    canvas_bind_group: std::sync::Arc<wgpu::BindGroup>,
    /// Pipeline for sampling the canvas (used by preview).
    canvas_pipeline: std::sync::Arc<wgpu::RenderPipeline>,
}

impl Compositor {
    pub fn new(
        device: &Device,
        surface_format: wgpu::TextureFormat,
        canvas_width: u32,
        canvas_height: u32,
    ) -> Self {
        // Canvas texture — render target + copy source for readback
        let canvas_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("compositor_canvas"),
            size: wgpu::Extent3d {
                width: canvas_width,
                height: canvas_height,
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
        let canvas_view = canvas_texture.create_view(&Default::default());

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("compositor_sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });

        // Bind group layout for source texture + sampler (group 0)
        let texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("compositor_texture_bgl"),
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

        // Uniform buffer for per-source transform + opacity (group 1)
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("compositor_uniforms"),
            size: std::mem::size_of::<SourceUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let uniform_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("compositor_uniform_bgl"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("compositor_uniform_bg"),
            layout: &uniform_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        // Compositor pipeline — renders source quads onto canvas
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("compositor_shader"),
            source: wgpu::ShaderSource::Wgsl(COMPOSITOR_SHADER.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("compositor_pipeline_layout"),
            bind_group_layouts: &[&texture_bind_group_layout, &uniform_bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("compositor_pipeline"),
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
                    format: wgpu::TextureFormat::Rgba8UnormSrgb,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::SrcAlpha,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                    }),
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

        // Readback buffer — for copying composited canvas to CPU
        // wgpu requires rows aligned to 256 bytes
        let bytes_per_row_padded = ((canvas_width * 4) + 255) & !255;
        let readback_size = (bytes_per_row_padded * canvas_height) as u64;
        let readback_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("compositor_readback"),
            size: readback_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        // Canvas bind group + pipeline for preview sampling (reuses existing preview shader pattern)
        let canvas_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("canvas_preview_bgl"),
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

        let canvas_bind_group =
            std::sync::Arc::new(device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("canvas_preview_bg"),
                layout: &canvas_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&canvas_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&sampler),
                    },
                ],
            }));

        // Preview pipeline — same fullscreen quad shader as PreviewRenderer
        let preview_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("canvas_preview_shader"),
            source: wgpu::ShaderSource::Wgsl(CANVAS_PREVIEW_SHADER.into()),
        });

        let canvas_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("canvas_preview_pl"),
                bind_group_layouts: &[&canvas_bind_group_layout],
                push_constant_ranges: &[],
            });

        let canvas_pipeline = std::sync::Arc::new(device.create_render_pipeline(
            &wgpu::RenderPipelineDescriptor {
                label: Some("canvas_preview_pipeline"),
                layout: Some(&canvas_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &preview_shader,
                    entry_point: Some("vs_main"),
                    buffers: &[],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &preview_shader,
                    entry_point: Some("fs_main"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: surface_format,
                        blend: Some(wgpu::BlendState::REPLACE),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleStrip,
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            },
        ));

        Self {
            canvas_texture,
            canvas_view,
            canvas_width,
            canvas_height,
            pipeline,
            sampler,
            uniform_buffer,
            uniform_bind_group_layout,
            uniform_bind_group,
            texture_bind_group_layout,
            source_layers: HashMap::new(),
            readback_buffer,
            canvas_bind_group,
            canvas_pipeline,
        }
    }
}
```

Also add the canvas preview shader constant (same as the existing `PREVIEW_SHADER` in `preview.rs`):

```rust
const CANVAS_PREVIEW_SHADER: &str = r#"
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    let x = f32((vertex_index & 1u) * 2u) - 1.0;
    let y = 1.0 - f32((vertex_index >> 1u) * 2u);
    let u = f32(vertex_index & 1u);
    let v = f32(vertex_index >> 1u);

    var out: VertexOutput;
    out.position = vec4<f32>(x, y, 0.0, 1.0);
    out.uv = vec2<f32>(u, v);
    return out;
}

@group(0) @binding(0) var t_canvas: texture_2d<f32>;
@group(0) @binding(1) var s_canvas: sampler;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return textureSample(t_canvas, s_canvas, in.uv);
}
"#;
```

- [ ] **Step 6: Implement `upload_frame()`, `remove_source()`**

```rust
impl Compositor {
    // ... (new() above) ...

    /// Upload a captured frame to the source's GPU texture.
    /// Creates or resizes the texture if dimensions changed.
    pub fn upload_frame(
        &mut self,
        device: &Device,
        queue: &Queue,
        source_id: SourceId,
        frame: &RgbaFrame,
    ) {
        let needs_recreate = match self.source_layers.get(&source_id) {
            Some(layer) => layer.size != (frame.width, frame.height),
            None => true,
        };

        if needs_recreate {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("source_texture"),
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
                label: Some("source_bind_group"),
                layout: &self.texture_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&texture_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                ],
            });
            self.source_layers.insert(
                source_id,
                SourceLayer {
                    texture,
                    texture_view,
                    bind_group,
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
                bytes_per_row: Some(4 * frame.width),
                rows_per_image: Some(frame.height),
            },
            wgpu::Extent3d {
                width: frame.width,
                height: frame.height,
                depth_or_array_layers: 1,
            },
        );
    }

    /// Remove a source's GPU resources.
    pub fn remove_source(&mut self, source_id: SourceId) {
        self.source_layers.remove(&source_id);
    }
}
```

- [ ] **Step 7: Implement `compose()`**

```rust
impl Compositor {
    /// Composite all visible sources onto the canvas texture.
    /// `sources` must be pre-resolved and in draw order (first = bottom).
    pub fn compose(
        &self,
        queue: &Queue,
        encoder: &mut wgpu::CommandEncoder,
        sources: &[&crate::scene::Source],
    ) {
        // Clear canvas to black
        {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("compositor_clear"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.canvas_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
        }

        // Draw each source as a textured quad
        for source in sources {
            let layer = match self.source_layers.get(&source.id) {
                Some(l) => l,
                None => continue, // no frame received yet
            };

            if !source.visible {
                continue;
            }

            let uniforms = SourceUniforms {
                rect: [
                    source.transform.x / self.canvas_width as f32,
                    source.transform.y / self.canvas_height as f32,
                    source.transform.width / self.canvas_width as f32,
                    source.transform.height / self.canvas_height as f32,
                ],
                opacity: source.opacity,
                _padding: [0.0; 3],
            };
            queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));

            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("compositor_source"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.canvas_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });

            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &layer.bind_group, &[]);
            pass.set_bind_group(1, &self.uniform_bind_group, &[]);
            pass.draw(0..4, 0..1);
        }
    }
}
```

- [ ] **Step 8: Implement `readback()`**

```rust
impl Compositor {
    /// Read the composited canvas back to CPU as an RgbaFrame.
    /// This is a blocking call (~1-2ms for 1080p).
    pub fn readback(&self, device: &Device, queue: &Queue) -> RgbaFrame {
        let bytes_per_row_unpadded = 4 * self.canvas_width;
        // wgpu requires rows aligned to 256 bytes
        let bytes_per_row_padded = (bytes_per_row_unpadded + 255) & !255;

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("compositor_readback_encoder"),
        });

        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &self.canvas_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &self.readback_buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(bytes_per_row_padded),
                    rows_per_image: Some(self.canvas_height),
                },
            },
            wgpu::Extent3d {
                width: self.canvas_width,
                height: self.canvas_height,
                depth_or_array_layers: 1,
            },
        );

        queue.submit(std::iter::once(encoder.finish()));

        let buffer_slice = self.readback_buffer.slice(..);
        buffer_slice.map_async(wgpu::MapMode::Read, |_| {});
        device.poll(wgpu::Maintain::Wait);

        let mapped = buffer_slice.get_mapped_range();
        // Copy data, removing row padding if present
        let mut data = Vec::with_capacity((bytes_per_row_unpadded * self.canvas_height) as usize);
        for row in 0..self.canvas_height {
            let start = (row * bytes_per_row_padded) as usize;
            let end = start + bytes_per_row_unpadded as usize;
            data.extend_from_slice(&mapped[start..end]);
        }
        drop(mapped);
        self.readback_buffer.unmap();

        RgbaFrame {
            data,
            width: self.canvas_width,
            height: self.canvas_height,
        }
    }

    /// Arc-wrapped bind group for the canvas texture (for preview panel).
    pub fn canvas_bind_group(&self) -> std::sync::Arc<wgpu::BindGroup> {
        std::sync::Arc::clone(&self.canvas_bind_group)
    }

    /// Arc-wrapped pipeline for sampling the canvas (for preview panel).
    pub fn canvas_pipeline(&self) -> std::sync::Arc<wgpu::RenderPipeline> {
        std::sync::Arc::clone(&self.canvas_pipeline)
    }
}
```

- [ ] **Step 9: Run tests and clippy**

Run: `cargo test compositor -- --nocapture 2>&1 | tail -20`
Run: `cargo clippy 2>&1 | tail -10`

- [ ] **Step 10: Commit**

```bash
git add src/renderer/compositor.rs src/renderer/mod.rs
git commit -m "feat: add GPU compositor module with compose, readback, and source management"
```

---

### Task 4: Update GStreamer thread for multi-source capture

**Files:**
- Modify: `src/gstreamer/thread.rs`

- [ ] **Step 1: Write failing tests for multi-capture management**

Add to `src/gstreamer/thread.rs` test module:

```rust
#[test]
fn add_and_remove_capture_source_commands() {
    use crate::scene::SourceId;
    let (main_ch, _thread_ch) = crate::gstreamer::create_channels();
    // Verify AddCaptureSource and RemoveCaptureSource can be sent
    main_ch
        .command_tx
        .try_send(GstCommand::AddCaptureSource {
            source_id: SourceId(1),
            config: CaptureSourceConfig::Screen { screen_index: 0 },
        })
        .unwrap();
    main_ch
        .command_tx
        .try_send(GstCommand::RemoveCaptureSource {
            source_id: SourceId(1),
        })
        .unwrap();
}
```

- [ ] **Step 2: Run test to verify it passes** (should pass since commands were added in Task 2)

Run: `cargo test add_and_remove_capture -- --nocapture`

- [ ] **Step 3: Refactor `GstThread` for multi-capture**

Replace the single capture fields with a `HashMap`:

```rust
use crate::scene::SourceId;
use std::collections::HashMap;

/// Bundles a capture pipeline and its appsink for one source.
struct CaptureHandle {
    pipeline: gstreamer::Pipeline,
    appsink: AppSink,
}

struct GstThread {
    channels: GstThreadChannels,
    // Multi-source captures
    captures: HashMap<SourceId, CaptureHandle>,
    // ... (keep all existing audio and encode fields unchanged)
    stream_handles: Option<StreamPipelineHandles>,
    record_handles: Option<RecordPipelineHandles>,
    mic_pipeline: Option<gstreamer::Pipeline>,
    mic_appsink: Option<AppSink>,
    mic_volume_name: Option<String>,
    system_pipeline: Option<gstreamer::Pipeline>,
    system_appsink: Option<AppSink>,
    system_volume_name: Option<String>,
    has_system_audio: bool,
    encoder_config: super::commands::EncoderConfig,
    audio_encoder_config: AudioEncoderConfig,
}
```

Update `new()` to initialize with empty `captures: HashMap::new()` and remove `capture_pipeline`/`capture_appsink` fields.

- [ ] **Step 4: Add `add_capture_source()` and `remove_capture_source()` methods**

```rust
impl GstThread {
    fn add_capture_source(&mut self, source_id: SourceId, config: &CaptureSourceConfig) {
        // Stop existing capture for this source if any
        self.remove_capture_source(source_id);

        match build_capture_pipeline(
            config,
            self.encoder_config.width,
            self.encoder_config.height,
            self.encoder_config.fps,
        ) {
            Ok((pipeline, appsink)) => {
                if let Err(e) = pipeline.set_state(gstreamer::State::Playing) {
                    log::error!("Failed to start capture for source {source_id:?}: {e}");
                    return;
                }
                self.captures.insert(source_id, CaptureHandle { pipeline, appsink });
                log::info!("Started capture for source {source_id:?}");
            }
            Err(e) => {
                log::error!("Failed to build capture pipeline for source {source_id:?}: {e}");
            }
        }
    }

    fn remove_capture_source(&mut self, source_id: SourceId) {
        if let Some(handle) = self.captures.remove(&source_id) {
            let _ = handle.pipeline.set_state(gstreamer::State::Null);
            log::info!("Removed capture for source {source_id:?}");
        }
    }
}
```

- [ ] **Step 5: Update `handle_command()` for new commands**

Add match arms to `handle_command()`:

```rust
GstCommand::AddCaptureSource { source_id, config } => {
    self.add_capture_source(source_id, &config);
}
GstCommand::RemoveCaptureSource { source_id } => {
    self.remove_capture_source(source_id);
}
```

Also update the existing `SetCaptureSource` handler to use the new multi-capture system (route to `add_capture_source` with `SourceId(0)` for backwards compat, or remove if no longer needed).

Update the `Shutdown` handler to stop all captures:

```rust
GstCommand::Shutdown => {
    for (_, handle) in self.captures.drain() {
        let _ = handle.pipeline.set_state(gstreamer::State::Null);
    }
    // ... existing shutdown logic for audio/encode pipelines ...
    return false; // signal exit
}
```

- [ ] **Step 6: Update the run loop to iterate all captures**

In `run()`, replace the single capture appsink pull with:

```rust
let pts = gstreamer::ClockTime::from_nseconds(start_time.elapsed().as_nanos() as u64);

// Pull frames from all capture sources
for (&source_id, handle) in self.captures.iter() {
    if let Some(sample) =
        handle.appsink.try_pull_sample(gstreamer::ClockTime::from_mseconds(0))
    {
        let (width, height) = sample
            .caps()
            .and_then(|caps| gstreamer_video::VideoInfo::from_caps(caps).ok())
            .map(|info| (info.width(), info.height()))
            .unwrap_or((self.encoder_config.width, self.encoder_config.height));

        if let Some(buffer) = sample.buffer()
            && let Ok(map) = buffer.map_readable()
        {
            let frame = RgbaFrame {
                data: map.as_slice().to_vec(),
                width,
                height,
            };

            // Write to shared latest_frames map
            if let Ok(mut frames) = self.channels.latest_frames.lock() {
                frames.insert(source_id, frame);
            }
        }
    }
}

// Receive composited frames and push to encode pipelines
while let Ok(frame) = self.channels.composited_frame_rx.try_recv() {
    // push_to_encode is a static method: fn push_to_encode(appsrc: &AppSrc, data: &[u8], pts: ClockTime)
    if let Some(ref handles) = self.stream_handles {
        Self::push_to_encode(&handles.video_appsrc, &frame.data, pts);
    }
    if let Some(ref handles) = self.record_handles {
        Self::push_to_encode(&handles.video_appsrc, &frame.data, pts);
    }
}
```

- [ ] **Step 7: Run tests and clippy**

Run: `cargo test 2>&1 | tail -20`
Run: `cargo clippy 2>&1 | tail -10`

- [ ] **Step 8: Commit**

```bash
git add src/gstreamer/thread.rs
git commit -m "feat: multi-source capture with HashMap<SourceId, CaptureHandle>"
```

---

### Task 5: Update preview pipeline to use compositor canvas

**Files:**
- Modify: `src/renderer/preview.rs`
- Modify: `src/renderer/mod.rs`
- Modify: `src/ui/preview_panel.rs`
- Modify: `src/window.rs`

- [ ] **Step 1: Update `PreviewResources` to use compositor's canvas**

In `src/ui/preview_panel.rs`, `PreviewResources` already holds `pipeline: Arc<RenderPipeline>` and `bind_group: Arc<BindGroup>`. These will now come from the compositor instead of `PreviewRenderer`. No struct change needed — just the source of the values changes.

- [ ] **Step 2: Update `SharedGpuState` to hold a `Compositor` instead of `PreviewRenderer`**

In `src/renderer/mod.rs`, replace `preview_renderer: PreviewRenderer` with `compositor: compositor::Compositor`:

```rust
use compositor::Compositor;

pub struct SharedGpuState {
    pub instance: wgpu::Instance,
    pub device: Arc<Device>,
    pub queue: Arc<Queue>,
    pub format: TextureFormat,
    pub compositor: Compositor,
    #[allow(dead_code)]
    pub widget_pipeline: WidgetPipeline,
    pub text_renderer: GlyphonRenderer,
}
```

Update `SharedGpuState::new()` to create a `Compositor` instead of `PreviewRenderer`:

```rust
let compositor = Compositor::new(&device, format, 1920, 1080);

// Upload a dark gray test frame to compositor is not needed —
// compositor starts with a black canvas (cleared on compose)

Ok(Self {
    instance,
    device,
    queue,
    format,
    compositor,
    widget_pipeline,
    text_renderer,
})
```

- [ ] **Step 3: Update all `preview_renderer` references throughout the codebase**

In `src/main.rs`:
- Replace `gpu.preview_renderer.upload_frame(...)` with compositor frame draining (Task 6 will fully wire this)
- Replace `gpu.preview_renderer.pipeline()` with `gpu.compositor.canvas_pipeline()`
- Replace `gpu.preview_renderer.bind_group()` with `gpu.compositor.canvas_bind_group()`

In `src/window.rs`, find where `PreviewResources` is constructed (search for `PreviewResources {`). Update it to use the compositor's canvas:

```rust
let preview_resources = PreviewResources {
    pipeline: gpu.compositor.canvas_pipeline(),
    bind_group: gpu.compositor.canvas_bind_group(),
};
```

Also search `src/main.rs` for the same pattern (around line 715-718 in `about_to_wait`) and update identically.

- [ ] **Step 4: Remove `PreviewRenderer` if no longer used**

If `PreviewRenderer` is completely replaced by the compositor's canvas pipeline, remove it. Keep the file if any utility functions remain, otherwise delete `src/renderer/preview.rs` and remove `pub mod preview` from `src/renderer/mod.rs`.

- [ ] **Step 5: Run tests and clippy**

Run: `cargo test 2>&1 | tail -20`
Run: `cargo clippy 2>&1 | tail -10`

- [ ] **Step 6: Commit**

```bash
git add src/renderer/mod.rs src/renderer/compositor.rs src/renderer/preview.rs src/ui/preview_panel.rs src/window.rs src/main.rs
git commit -m "refactor: replace PreviewRenderer with compositor canvas for preview"
```

---

### Task 6: Wire compositor into the main render loop

**Files:**
- Modify: `src/main.rs:610-665` (about_to_wait frame polling + compose + readback)
- Modify: `src/main.rs:131-189` (AppManager struct)
- Modify: `src/state.rs`

- [ ] **Step 1: Add `composited_frame_tx` to `AppManager`**

In `src/main.rs`, the `AppManager` struct stores `gst_channels: Option<GstChannels>`. The `composited_frame_tx` is accessible via `gst_channels.composited_frame_tx`. No struct change needed — access it through the existing channels.

- [ ] **Step 2: Update `about_to_wait()` to drain frames, compose, and readback**

Replace the frame polling section in `about_to_wait()` (lines 612-617) with:

```rust
// Drain latest frames from shared map and upload to compositor
if let Some(ref channels) = self.gst_channels {
    if let Some(ref gpu) = self.gpu {
        let mut frames = channels.latest_frames.lock().expect("lock latest_frames");
        for (source_id, frame) in frames.drain() {
            gpu.compositor.upload_frame(&gpu.device, &gpu.queue, source_id, &frame);
        }
    }
}

// Compose active scene sources
if let Some(ref gpu) = self.gpu {
    let app_state = self.state.lock().expect("lock AppState");
    if let Some(active_scene_id) = app_state.active_scene_id {
        // Resolve source IDs to Source references
        let source_ids: Vec<_> = app_state
            .scenes
            .iter()
            .find(|s| s.id == active_scene_id)
            .map(|s| s.sources.clone())
            .unwrap_or_default();

        let resolved_sources: Vec<&crate::scene::Source> = source_ids
            .iter()
            .filter_map(|sid| app_state.sources.iter().find(|s| s.id == *sid))
            .collect();

        let mut encoder = gpu.device.create_command_encoder(
            &wgpu::CommandEncoderDescriptor {
                label: Some("compositor_encoder"),
            },
        );
        gpu.compositor.compose(&gpu.queue, &mut encoder, &resolved_sources);
        gpu.queue.submit(std::iter::once(encoder.finish()));

        // Readback for encoding if streaming or recording
        let is_encoding = app_state.stream_status.is_live()
            || matches!(app_state.recording_status, crate::state::RecordingStatus::Recording { .. });

        if is_encoding {
            drop(app_state); // release lock before blocking readback
            let frame = gpu.compositor.readback(&gpu.device, &gpu.queue);
            if let Some(ref channels) = self.gst_channels {
                let _ = channels.composited_frame_tx.try_send(frame);
            }
        }
    }
}
```

Note: `gpu.compositor` needs to be `&mut` for `upload_frame`. Since `SharedGpuState` is not behind a mutex (it's owned by `AppManager`), this should be fine — just make `self.gpu` a mutable reference where needed.

- [ ] **Step 3: Update the blank preview logic**

Replace the existing "Upload blank preview when capture is stopped" section (lines 641-665) — the compositor now handles this: when no sources are active, `compose()` clears to black automatically. Remove this block.

- [ ] **Step 4: Implement scene switch diffing**

Add a method to `AppManager` or a free function:

```rust
fn diff_scene_sources(
    old_scene: Option<&crate::scene::Scene>,
    new_scene: Option<&crate::scene::Scene>,
) -> (Vec<SourceId>, Vec<SourceId>) {
    let old_ids: std::collections::HashSet<_> = old_scene
        .map(|s| s.sources.iter().copied().collect())
        .unwrap_or_default();
    let new_ids: std::collections::HashSet<_> = new_scene
        .map(|s| s.sources.iter().copied().collect())
        .unwrap_or_default();

    let to_add: Vec<_> = new_ids.difference(&old_ids).copied().collect();
    let to_remove: Vec<_> = old_ids.difference(&new_ids).copied().collect();
    (to_add, to_remove)
}
```

Add tests for the diff function:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{Scene, SceneId, SourceId};

    #[test]
    fn diff_empty_to_scene() {
        let scene = Scene { id: SceneId(1), name: "S".into(), sources: vec![SourceId(1), SourceId(2)] };
        let (to_add, to_remove) = diff_scene_sources(None, Some(&scene));
        assert_eq!(to_add.len(), 2);
        assert!(to_remove.is_empty());
    }

    #[test]
    fn diff_scene_to_empty() {
        let scene = Scene { id: SceneId(1), name: "S".into(), sources: vec![SourceId(1)] };
        let (to_add, to_remove) = diff_scene_sources(Some(&scene), None);
        assert!(to_add.is_empty());
        assert_eq!(to_remove.len(), 1);
    }

    #[test]
    fn diff_shared_sources_not_touched() {
        let old = Scene { id: SceneId(1), name: "A".into(), sources: vec![SourceId(1), SourceId(2)] };
        let new = Scene { id: SceneId(2), name: "B".into(), sources: vec![SourceId(2), SourceId(3)] };
        let (to_add, to_remove) = diff_scene_sources(Some(&old), Some(&new));
        assert!(to_add.contains(&SourceId(3)));
        assert!(!to_add.contains(&SourceId(2)));
        assert!(to_remove.contains(&SourceId(1)));
        assert!(!to_remove.contains(&SourceId(2)));
    }

    #[test]
    fn diff_identical_scenes() {
        let scene = Scene { id: SceneId(1), name: "A".into(), sources: vec![SourceId(1)] };
        let (to_add, to_remove) = diff_scene_sources(Some(&scene), Some(&scene));
        assert!(to_add.is_empty());
        assert!(to_remove.is_empty());
    }
}
```

Wire this into the scene switching logic in `scene_editor.rs` or wherever `active_scene_id` changes. When the active scene changes, compute the diff and send `AddCaptureSource`/`RemoveCaptureSource` commands for the delta.

- [ ] **Step 5: Run tests and clippy**

Run: `cargo test 2>&1 | tail -20`
Run: `cargo clippy 2>&1 | tail -10`

- [ ] **Step 6: Commit**

```bash
git add src/main.rs src/state.rs src/ui/scene_editor.rs
git commit -m "feat: wire compositor into render loop with frame drain, compose, and readback"
```

---

### Task 7: Update scene editor UI for multi-source

**Files:**
- Modify: `src/ui/scene_editor.rs`

- [ ] **Step 1: Update source listing to show all sources in scene**

Replace the single-source lookup (currently `scene.sources.first()`) with iteration over all source IDs. For each source in the scene, display:
- Source name
- Visibility toggle (eye icon or checkbox)
- Opacity slider (0.0 to 1.0)
- Move up / Move down buttons
- Delete button

- [ ] **Step 2: Update "Add Source" to append without replacing**

The current "Add Display Source" button should add to the scene's sources vec without removing existing ones. It should call `AddCaptureSource` command to start the new capture.

- [ ] **Step 3: Update `send_capture_for_scene()` to handle multi-source**

Replace the current function that sends a single `SetCaptureSource` with one that diffs and sends `AddCaptureSource`/`RemoveCaptureSource` for each source in the scene.

- [ ] **Step 4: Add per-source transform controls**

For the selected source, show the existing transform grid (x, y, width, height) plus the new opacity slider.

- [ ] **Step 5: Add source reordering**

Add "Move Up" and "Move Down" buttons that call `scene.move_source_up(id)` and `scene.move_source_down(id)`.

- [ ] **Step 6: Run the app and manually verify**

Run: `cargo run`
Verify:
- Can add multiple display sources to a scene
- Sources appear in the source list
- Can toggle visibility, adjust opacity
- Can reorder sources
- Preview shows composited output

- [ ] **Step 7: Run clippy**

Run: `cargo clippy 2>&1 | tail -10`

- [ ] **Step 8: Commit**

```bash
git add src/ui/scene_editor.rs
git commit -m "feat: multi-source scene editor with opacity, reorder, and visibility"
```

---

### Task 8: Final integration testing and cleanup

**Files:**
- All modified files

- [ ] **Step 1: Run full test suite**

Run: `cargo test 2>&1 | tail -30`
Expected: all tests pass.

- [ ] **Step 2: Run clippy with strict mode**

Run: `cargo clippy -- -W clippy::all 2>&1 | tail -20`

- [ ] **Step 3: Check formatting**

Run: `cargo fmt --check 2>&1`

- [ ] **Step 4: Manual integration test**

Run: `cargo run`
Test the following scenarios:
1. Create a scene with one display source → preview shows screen capture
2. Add a second display source → both visible in preview
3. Adjust opacity of top source → bottom source shows through
4. Toggle visibility off → source disappears from preview
5. Reorder sources → z-order changes in preview
6. Switch scenes → sources start/stop correctly (diff-based)
7. Start streaming → verify encoded output includes composited frame
8. Start recording → verify recorded file includes composited output

- [ ] **Step 5: Remove deprecated `SetCaptureSource` if all callers migrated**

Search for remaining `SetCaptureSource` usage. If none, remove the variant from `GstCommand` and its handler.

- [ ] **Step 6: Final commit**

```bash
git add -A
git commit -m "chore: cleanup deprecated SetCaptureSource, final clippy/fmt fixes"
```
