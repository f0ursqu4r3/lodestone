# Scene Transitions Design

## Overview

Scene transitions enable smooth visual switching between scenes in Lodestone, starting with Cut (instant) and Fade (crossfade), with architecture designed for future custom shader transitions. Includes Studio Mode — a toggleable dual-preview workflow where users stage a scene before transitioning it to program output.

## Transition Types

### Cut
Instant scene switch. Equivalent to current behavior. Zero-duration transition — no blending, no secondary canvas needed.

### Fade (Crossfade)
Linear blend between outgoing and incoming scenes over a configurable duration. Both scenes render live frames during the transition (no frozen snapshots). Default duration: 300ms.

### Future: Custom Shader
The transition pipeline accepts any `.wgsl` shader with a standard interface. Not exposed in UI for this iteration, but `TransitionType::Shader(String)` is reserved in the enum.

## Data Model

### Transition Configuration (persisted in settings TOML)

```rust
struct TransitionConfig {
    default_type: TransitionType,  // Cut | Fade
    default_duration_ms: u32,      // default: 300
}
```

### Per-Scene Override (persisted on Scene)

```rust
struct SceneTransitionOverride {
    transition_type: Option<TransitionType>,  // None = use global default
    duration_ms: Option<u32>,                 // None = use global default
}
```

The override specifies the transition used when transitioning *into* this scene. Resolved at transition time: per-scene override takes priority, falls back to global default.

### Runtime Transition State (on AppState, not persisted)

```rust
struct TransitionState {
    from_scene: SceneId,
    to_scene: SceneId,
    transition_type: TransitionType,
    started_at: Instant,
    duration: Duration,
}
```

### AppState Additions

```rust
studio_mode: bool,                           // toggle, default false
preview_scene_id: Option<SceneId>,           // which scene is in Preview (Studio Mode)
active_transition: Option<TransitionState>,  // None when idle
```

## Architecture: Dual Canvas with Lazy Activation

The compositor maintains a primary canvas (program) at all times. A secondary canvas (preview) is allocated on demand — only when Studio Mode is on or a transition is in progress.

### GPU Resources

**Primary canvas** (always exists): `canvas_texture`, `canvas_view`, `canvas_bind_group`, `source_layers: HashMap<SourceId, SourceLayer>` — same as current implementation.

**SecondaryCanvas** (allocated on demand):
- `texture: wgpu::Texture` — same format/size as primary
- `view: wgpu::TextureView`
- `bind_group: wgpu::BindGroup`
- `source_layers: HashMap<SourceId, SourceLayer>` — independent GPU resources for preview scene sources

**TransitionPipeline**: Fullscreen-quad render pipeline that blends two canvas textures.
- `pipeline: wgpu::RenderPipeline`
- `bind_group_layout: wgpu::BindGroupLayout` — accepts two textures + uniform buffer
- `uniform_buffer: wgpu::Buffer` — contains `progress: f32` (0.0→1.0) and `time: f32` (elapsed seconds)
- Shader module is swappable for future custom transitions

### Render Loop Modes

**Normal Mode (Studio OFF, no transition):**
1. Resolve sources for `active_scene_id` (program scene)
2. Compose onto primary canvas
3. Output = primary canvas (no transition pass)
4. Scale to output resolution → readback → GStreamer

Cost: identical to current implementation.

**Studio Mode (no transition):**
1. Resolve and compose program scene → primary canvas
2. Resolve and compose preview scene → secondary canvas
3. Output = primary canvas (program is what goes live)
4. Scale → readback → GStreamer
5. Preview panel shows both canvases side-by-side

Cost: 2× compose passes. Readback is still single (program only).

**Transitioning:**
1. Resolve and compose "from" scene → primary canvas
2. Resolve and compose "to" scene → secondary canvas
3. Transition pass: blend primary + secondary → output texture, using `progress` uniform
4. Scale → readback → GStreamer
5. Preview/program panels show the blended output (or individual canvases in Studio Mode)

Cost: 2× compose + 1 fullscreen blend pass.

### Transition Lifecycle

1. **Trigger:** User clicks scene (Normal Mode) or clicks Transition button / presses Enter (Studio Mode)
2. **Start:** Allocate secondary canvas if not already present. Set `active_transition` with `started_at = Instant::now()`. Run `apply_scene_diff()` to start incoming scene's exclusive sources.
3. **Each frame:** Compute `progress = elapsed / duration`, clamped to 0.0–1.0. Update uniform buffer. Render both canvases. Run transition blend pass. Call `request_repaint()` to drive continuous redraws.
4. **Complete (progress >= 1.0):**
   - Set `active_scene_id = to_scene`
   - Clear `active_transition = None`
   - Studio Mode ON: keep secondary canvas, reset `preview_scene_id` to None
   - Studio Mode OFF: deallocate secondary canvas, send `RemoveCaptureSource` for sources exclusive to the old scene
5. **Quick cut (bypass):** Set `active_scene_id` directly. If a transition is in flight, cancel it (clear `active_transition`). Deallocate secondary canvas if Studio Mode is off. Immediate source diff.

### Canvas Swap Semantics

After a transition completes, the "to" scene becomes program. Rather than physically swapping texture handles, update which scene ID maps to which canvas. The primary canvas always represents program output.

## GStreamer & Source Lifecycle

No GStreamer pipeline changes required. The existing `AddCaptureSource` / `RemoveCaptureSource` command channel handles all source lifecycle.

**During transitions:** Both scenes' sources must be active. Sources shared between scenes (same `SourceId`) are never interrupted. Sources exclusive to the outgoing scene stay alive until the transition completes.

**Source diff timing:**
- Transition start: `apply_scene_diff()` for incoming scene — starts new sources
- Transition complete: reverse diff for outgoing scene — stops sources not needed by the new program scene (or preview scene in Studio Mode)

**Studio Mode:** Sources for both program and preview scenes stay alive. When preview scene changes, diff against the union of program + old preview to decide starts/stops. No source is stopped if still needed by either context.

## UI Design

### Scenes Panel — Transition Bar

A compact control bar sits below the scene thumbnails in the scenes panel:
- **Type toggle:** Segmented control with Fade / Cut options
- **Duration input:** Numeric field (milliseconds) next to type toggle
- **Studio Mode button:** Toggle button, right-aligned. Highlighted when active.
- **Transition button:** Appears only when Studio Mode is ON. Triggers the transition from preview → program. Styled prominently (accent color).

### Scene Thumbnails (Studio Mode)

- **Program scene:** Red border with "PGM" badge (top-right corner)
- **Preview scene:** Green border with "PRV" badge
- **Other scenes:** Default styling. Clicking sets as preview scene.

### Preview Panel — Studio Mode

When Studio Mode is on, the preview panel splits into two side-by-side panes:
- **Left: Preview** — green label, green border, shows secondary canvas (preview scene)
- **Right: Program** — red label, red border, shows primary canvas (live output)

During a transition, the Program pane shows the blended output with a progress bar overlay and "Transitioning..." indicator.

When Studio Mode is off, the preview panel shows the single program output as it does today.

### Per-Scene Transition Override

Right-click a scene thumbnail to access a context menu with "Transition Override" submenu:
- **Type:** Default / Fade / Cut (Default uses global setting)
- **Duration:** Numeric input in milliseconds

This controls the transition used when transitioning *into* the selected scene.

### Hotkeys

| Key | Action | Notes |
|-----|--------|-------|
| `Enter` | Trigger transition | Studio Mode: preview → program. Normal Mode: no-op |
| `Space` | Quick cut | Instant switch, bypasses configured transition |
| `1`–`9` | Select scene by index | Normal: triggers transition. Studio: sets preview |
| `Ctrl+S` | Toggle Studio Mode | Allocates/deallocates secondary canvas |

## Shader Extensibility

The transition pipeline is a fullscreen-quad shader pass from day one. The fade transition is the first (and currently only) shader:

```wgsl
@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4f {
    let a = textureSample(tex_program, samp, in.uv);
    let b = textureSample(tex_preview, samp, in.uv);
    return mix(a, b, uniforms.progress);
}
```

**Standard shader interface:**
- Inputs: `tex_program` (sampler2D), `tex_preview` (sampler2D), `uniforms.progress` (f32, 0.0→1.0), `uniforms.time` (f32, elapsed seconds)
- Output: blended fragment color

The uniform buffer includes both `progress` and `time` from the start, even though the built-in fade only uses `progress`. This ensures custom shaders have access to elapsed time for animated effects (wipes, dissolves, etc.) without changing the buffer layout.

**Not in scope for this iteration:** shader file discovery/loading, UI for selecting custom shaders, shader hot-reload, shader compilation error handling. The `TransitionType::Shader(String)` variant exists in the enum but is not exposed in any UI.

## Testing Strategy

- **Unit tests:** Transition state machine (start, progress, complete, cancel). Duration/type resolution (per-scene override vs global default). Source diff logic with shared sources.
- **Visual verification:** Fade renders correctly (no flicker, smooth alpha). Studio Mode shows correct scenes in correct panes. Transition bar controls update state correctly.
- **Edge cases:** Rapid scene switching during transition. Transition to same scene (no-op). Studio Mode toggle during active transition. Quick cut during active transition cancels cleanly.
