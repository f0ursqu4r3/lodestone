# Lodestone MVP — Design Spec

## Overview

Lodestone is a native streaming/recording application built in Rust. No Electron, no webview — a game-engine-style render loop with direct GPU access. This spec covers the MVP: a working, shippable streaming tool with a modern UI.

## Architecture

Monolithic render loop. One `winit` event loop drives everything.

```text
winit event loop
  └── wgpu device + surface
        ├── custom UI renderer     (wgpu pipelines: panels, shadows, animations)
        ├── egui-wgpu integration  (layout + input only)
        ├── glyphon text pass      (subpixel AA text)
        ├── preview pipeline       (OBS frame → wgpu texture, stubbed initially)
        └── libobs-rs thread       (dedicated OS thread, added later)
```

### Frame Loop

Each frame executes in order:

1. **Poll events** — `winit` delivers input, resize, close
2. **Update state** — process pending channel messages, update `AppState`
3. **Build UI** — egui layout pass produces draw commands (no painting)
4. **Render frame** — wgpu render passes:
   - Clear pass (background)
   - Custom widget pass (panels, shadows, glows)
   - egui geometry pass (hit testing regions, not visuals)
   - Glyphon text pass (all text)
   - Preview texture pass (OBS frame composite, stubbed)

### Build Strategy

UI-first, OBS later. Steps 1-4 of the build order (shell, egui, glyphon, widgets) are built with mock data. The OBS layer is abstracted behind a trait so real integration slots in without changing UI code.

## Tech Stack

| Layer | Crate | Role |
|---|---|---|
| Window + input | `winit` | OS window, input events, event loop |
| GPU abstraction | `wgpu` | DX12 / Metal / Vulkan / WebGPU |
| UI layout + input | `egui` + `egui-wgpu` | Immediate-mode layout, hit testing, widget logic |
| Text rendering | `glyphon` + `cosmic-text` | Subpixel-quality GPU text |
| OBS engine | `libobs-rs` | libobs C API (deferred to later build phase) |
| Async runtime | `tokio` | Settings I/O, mock data driver, background tasks |

## egui Integration & Custom Rendering

egui runs in layout-only mode:

- egui computes widget positions, sizes, and hit testing
- egui handles input routing (clicks, hovers, keyboard focus)
- We intercept egui's `ClippedPrimitive` output but render our own visuals at those positions using custom wgpu pipelines

**Custom widget renderer** draws:

- **Panels** — rounded rects with subtle borders, drop shadows
- **Buttons** — state-driven (idle, hover, active, disabled), GPU quads
- **Sliders/faders** — vertical for audio, horizontal for other controls
- **VU meters** — animated bar segments, driven by audio data
- **Indicators** — live status dots, connection quality badges

**Glyphon text pass** replaces egui's built-in text rendering entirely. All text goes through `glyphon` + `cosmic-text`. One bundled typeface (clean sans-serif, finalized in style guide).

Separation principle: egui says *where*, our renderer says *what it looks like*.

## Layout

Full-canvas workspace with floating HUD panels. Breaks from the traditional OBS docked-toolbar layout.

### Preview

Fills the entire window edge-to-edge. No chrome around it. The stream preview is the primary focus.

### Floating Panels

Translucent, blurred overlays on top of the preview (HUD-style, backdrop blur):

- **Left edge — Scene/Source panel:** Collapsible vertical strip. Scene list on top, source list below with basic transform controls (position/size).
- **Bottom bar — Audio mixer:** Horizontal strip. Per-source vertical faders, mute toggles, VU meters. Can collapse to meters only.
- **Top-right — Stream controls:** Floating HUD cluster. Go live button, stream key config, destination selector. Live stats (bitrate, dropped frames, uptime) appear when streaming.

### Panel Behaviors

- Subtle transparency with backdrop blur — preview bleeds through
- Panels collapse/minimize to icons at screen edges
- When streaming, non-essential UI dims to emphasize preview and live stats
- No menu bar — settings via gear icon opening a modal

## State Management

Single `AppState` as source of truth:

```text
AppState
├── scenes: Vec<Scene>            — name, id, active flag
├── sources: Vec<Source>          — name, type, transform (pos/size), visibility, muted
├── audio_levels: Vec<AudioLevel> — per-source current dB, peak hold
├── stream_status: StreamStatus   — Offline | Connecting | Live { uptime, bitrate, dropped_frames }
├── settings: AppSettings         — stream key, destination, encoder prefs, profiles
└── ui_state: UiState             — panel open/collapsed state, modal state
```

Access: `Arc<Mutex<AppState>>` shared between main loop and (eventually) OBS thread.

### Mock Data Driver

A tokio task periodically updates audio levels and stream stats with realistic fake data. VU meters animate, stats tick, UI feels alive during development without OBS.

## OBS Abstraction

Trait-based interface for the OBS engine:

```rust
trait ObsEngine {
    fn init() -> Result<Self>;
    fn scenes(&self) -> Vec<Scene>;
    fn start_stream(&mut self, config: StreamConfig) -> Result<()>;
    fn stop_stream(&mut self) -> Result<()>;
    fn subscribe_stats(&self) -> Receiver<ObsStats>;
    fn get_frame(&self) -> Option<FrameTexture>;
}
```

`MockObsEngine` implements this for MVP development. `LiveObsEngine` (backed by `libobs-rs`) implements it later — UI code doesn't change.

Key constraint: OBS runs on a dedicated OS thread. Communication to the render loop is via tokio channels only. OBS handles never cross thread boundaries.

## Settings & Persistence

TOML files, no databases.

- `~/.config/lodestone/settings.toml` — global settings (default destination, UI preferences, panel layout)
- `~/.config/lodestone/profiles/<name>.toml` — stream profiles (encoder, bitrate, resolution, stream key)

First launch creates a default profile. Settings save on change (debounced async write via tokio). No save button.

MVP settings scope: stream key, destination (Twitch/YouTube/Custom RTMP), output resolution, bitrate, active profile.

## Error Handling

- `anyhow::Result` throughout
- Render path errors: log and skip frame, never crash the loop
- Settings I/O errors: fall back to defaults, show UI toast
- OBS trait boundary returns `Result` — callers handle gracefully (e.g., "failed to connect" in stream controls)
- No `unwrap()` in non-prototype paths

## Testing Strategy

- **Unit tests:** state logic (scene/source CRUD), settings serialization round-trips, mock engine behavior
- **Integration tests:** `MockObsEngine` produces expected event sequences
- **Settings round-trip tests:** write TOML, read back, assert equality
- **No GPU tests in CI** — render pipeline validated manually. Screenshot comparison deferred.

## Project Structure

```text
lodestone/
├── Cargo.toml
├── docs/
│   └── BRIEF.md
├── src/
│   ├── main.rs               ← winit event loop, wgpu init
│   ├── renderer/
│   │   ├── mod.rs            ← render loop orchestration
│   │   ├── pipelines.rs      ← wgpu pipeline definitions
│   │   ├── text.rs           ← glyphon integration
│   │   └── preview.rs        ← OBS frame texture pipeline
│   ├── ui/
│   │   ├── mod.rs            ← egui context, layout root
│   │   ├── scene_editor.rs
│   │   ├── audio_mixer.rs
│   │   └── stream_controls.rs
│   ├── obs/
│   │   ├── mod.rs            ← ObsEngine trait, channel defs
│   │   ├── mock.rs           ← MockObsEngine
│   │   ├── scene.rs
│   │   ├── output.rs
│   │   └── settings.rs
│   └── state.rs              ← AppState, shared types
└── assets/
    └── fonts/
```

## Out of Scope (MVP)

- Overlay / alert system
- Scene transitions
- Virtual camera output
- Plugin / extension system
- Multi-track audio recording
- Cloud profile sync
- Marketplace
