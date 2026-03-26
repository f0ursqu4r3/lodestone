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

## Design Context

### Users
Broad audience from casual Twitch/YouTube streamers to technical power users who want a native, non-Electron alternative to OBS. Users are in a creative/production context — they need the tool to be reliable and stay out of the way while they focus on content.

### Brand Personality
**Sleek, powerful, modern.** Premium feel — capable but approachable. Not a toy, not intimidating.

### Aesthetic Direction
- **Visual tone:** Professional creative tool. Think DaVinci Resolve / Logic Pro — dense but organized, information-rich without feeling cluttered.
- **Theme:** Dark mode primary (neutral dark palette: `#111116` base, `#e0e0e8` text). Light mode planned but not yet implemented.
- **References:** DaVinci Resolve, Logic Pro — professional creative tools with organized density.
- **Anti-references:** OBS Studio (dated, cluttered, Windows-era UI), StreamLabs (over-designed, gamery, upsell-heavy). Avoid both clutter AND gamer aesthetic.
- **Icons:** Phosphor Icons (consistent, clean line icons).
- **Typography:** System proportional font, 13px base. Subpixel rendering via glyphon planned.
- **Corners:** 4px (buttons/inputs), 6px (cards/panels), 12px (pills).

### Design Principles
1. **Every pixel is intentional** — Custom GPU-rendered UI, not themed widgets. If something is on screen, it earned its place.
2. **Dense but not cluttered** — Show information professionals need without overwhelming. Use hierarchy (text weight, color, spacing) to create visual order.
3. **Performance is a feature** — Native render loop, no DOM, no webview. The UI should feel instant. Animations serve function (state feedback), not decoration.
4. **Explicit over clever** — Controls should be discoverable and predictable. No hidden gestures, no magic. Label things clearly.
5. **Platform-native when possible** — Native menus, native file dialogs, OS-appropriate behavior. Don't reinvent what the OS already does well.

## License

GPL-3.0.
