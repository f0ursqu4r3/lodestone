# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

Lodestone is a native streaming/recording application (alternative to OBS Studio/StreamLabs) built in Rust. It uses a game-engine-style architecture: a native render loop with direct GPU access — no Electron, no webview, no DOM.

## Build Commands

```bash
cargo build            # debug build
cargo build --release  # release build
cargo run              # run debug
cargo test             # run all tests
cargo test <name>      # run a single test by name
cargo clippy           # lint
cargo fmt --check      # check formatting
```

## Architecture

The app runs a `winit` event loop driving a `wgpu` render pipeline. The UI layer uses `egui` for layout/input only — all visuals are rendered through custom wgpu pipelines. Text rendering uses `glyphon` + `cosmic-text` instead of egui's default painter.

GStreamer runs on a dedicated thread. It communicates with the render loop exclusively via `tokio` channels — GStreamer handles are never shared across threads. The UI layer never touches video data directly.

```
winit event loop
  └── wgpu device + surface
        ├── custom UI renderer     (wgpu pipelines: panels, shadows, animations)
        ├── egui-wgpu integration  (layout + input only)
        ├── glyphon text pass      (subpixel AA text)
        ├── preview pipeline       (GStreamer frame → wgpu texture)
        └── GStreamer thread        (dedicated thread, channels to render loop)
```

Key modules: `renderer/` (render loop, pipelines, text, preview), `ui/` (egui panels, dockview layout), `gstreamer/` (capture, encoding, streaming, recording), `state.rs` (shared app state via `Arc<Mutex<AppState>>`).

## Coding Conventions

- No `unwrap()` in non-prototype paths — use `anyhow` for error propagation.
- GStreamer thread communicates via channels only.
- GPU resources (buffers, textures, pipelines) are owned by the renderer, never leaked to UI code.
- Settings are plain TOML. No binary formats, no databases.
- All public types get doc comments.
- Prefer explicit over clever.

## License

GPL-3.0.
