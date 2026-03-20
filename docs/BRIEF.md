# Lodestone — Project Brief

## What is Lodestone?

Lodestone is a high-performance, cross-platform streaming and recording application — a lean, polished alternative to StreamLabs/OBS Studio. It is built like a game engine: a native render loop with direct GPU access, no Electron, no webview, no DOM. Every frame is intentional.

---

## Goals

- **Performance first** — minimal CPU/GPU overhead, no framework tax
- **Pixel-perfect UI** — custom rendering pipeline, not off-the-shelf widget themes
- **Solid foundation** — architecture designed for expansion; MVP scope is intentionally narrow
- **Cross-platform** — Windows, macOS, Linux from a single codebase

---

## Technology Stack

| Layer              | Crate / Tool              | Role                                               |
| ------------------ | ------------------------- | -------------------------------------------------- |
| Window + input     | `winit`                   | OS window, input events, event loop                |
| GPU abstraction    | `wgpu`                    | DX12 / Metal / Vulkan / WebGPU                     |
| UI layout + input  | `egui` + `egui-wgpu`      | Immediate-mode layout, hit testing, widget logic   |
| Text rendering     | `glyphon` + `cosmic-text` | Subpixel-quality GPU text, cross-platform shaping  |
| OBS engine         | `libobs-rs`               | libobs C API — scenes, sources, encoders, outputs  |
| Async runtime      | `tokio`                   | Settings I/O, IPC channels, background tasks       |
| Custom UI renderer | (in-repo)                 | wgpu pipelines for panels, animations, blur, glows |

The UI model: egui handles layout math and input routing. Custom wgpu render passes handle all visuals. egui's default painter is replaced or supplemented by our own pipeline.

---

## Architecture

```text
winit event loop
  └── wgpu device + surface
        ├── custom UI renderer     (wgpu pipelines: panels, shadows, animations)
        ├── egui-wgpu integration  (layout + input only)
        ├── glyphon text pass      (subpixel AA, all text rendered here)
        ├── preview pipeline       (OBS frame → wgpu texture, composited directly)
        └── libobs-rs thread       (dedicated OS thread, channels to render loop)
```

**Key principle:** the UI layer never touches video data. All OBS interaction lives on a dedicated Rust thread. Communication back to the render loop is via `tokio` channels (stats, frame events, status updates).

---

## MVP Scope

Build a working, shippable streaming tool. Nothing more.

### Rust core

- [ ] libobs-rs integration: context init, scene/source lifecycle, encoder setup
- [ ] Output manager: RTMP stream start/stop + local recording
- [ ] Settings persistence: profiles stored as TOML, loaded at startup
- [ ] Channel layer: typed events from OBS thread → render loop (bitrate, dropped frames, status)

### Render + UI

- [ ] `winit` + `wgpu` shell: window, surface, event loop, clear pass
- [ ] `egui-wgpu` integration: panels rendering, input working
- [ ] `glyphon` text pass: font rendering layer replacing egui's default
- [ ] Custom widget renderer: buttons, panels, sliders, VU meters
- [ ] Preview pipeline: OBS frame as wgpu texture, updated async, composited in render loop

### UI panels (MVP)

- [ ] Scene editor: scene list, source list, basic transform (position/size)
- [ ] Audio mixer: per-source volume + mute, VU meters driven by OBS audio callbacks
- [ ] Stream controls: go live / stop, stream key input, destination selector, live stats overlay

---

## Explicitly Out of Scope (MVP)

These are intentionally deferred. Do not build them yet.

- Overlay / alert system (Twitch webhooks, donations, etc.)
- Scene transitions
- Virtual camera output
- Plugin / extension system
- Multi-track audio recording
- Cloud profile sync
- Marketplace or store

---

## Build Order

Follow this sequence to maintain momentum and avoid blocking:

1. `winit` + `wgpu` shell — window opens, clear to color, event loop running
2. `egui-wgpu` integration — basic panels, input working
3. `glyphon` text pass — own font rendering layer wired in
4. Custom widget renderer — design language established, core widgets built
5. `libobs-rs` thread + channels — OBS init, scene management, events flowing
6. Preview texture pipeline — OBS frame into wgpu texture, live in UI
7. Stream controls wired end to end — first full user-facing flow working

---

## Project Structure (target)

```text
lodestone/
├── Cargo.toml
├── docs/
│   └── BRIEF.md              ← this file
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
│   │   ├── mod.rs            ← libobs-rs thread, channel definitions
│   │   ├── scene.rs
│   │   ├── output.rs
│   │   └── encoder.rs
│   └── state.rs              ← shared app state (Arc<Mutex<AppState>>)
└── assets/
    └── fonts/                ← bundled typeface
```

---

## Coding Conventions

- Prefer explicit over clever. This codebase will be worked on by multiple people.
- No `unwrap()` in non-prototype paths. Use `anyhow` for error propagation.
- OBS thread communicates via channels only — never share OBS handles across threads.
- GPU resources (buffers, textures, pipelines) are owned by the renderer, never leaked to UI code.
- Settings are plain TOML. No binary formats, no databases for MVP.
- All public types get doc comments.

---

## Licensing Note

`libobs-rs` is GPL-3.0. Lodestone inherits this. Keep that in mind for any future commercial distribution strategy.
