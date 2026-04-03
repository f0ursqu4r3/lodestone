# Lodestone

A high-performance, native streaming and recording application built in Rust. Lodestone is a modern alternative to OBS Studio and StreamLabs — no Electron, no webview, no DOM. Every frame is rendered directly on the GPU.

## Why Lodestone?

Existing streaming tools are either dated and cluttered (OBS) or bloated with upsells (StreamLabs). Lodestone takes a different approach: a game-engine-style architecture with a custom GPU-rendered UI, designed to feel like a professional creative tool (think DaVinci Resolve or Logic Pro) while staying approachable for casual streamers.

## Features

- **Native GPU rendering** — Custom `wgpu` render pipeline (Metal / Vulkan / DX12). No web tech, no framework overhead.
- **Scene composition** — Multiple sources (display capture, camera, images, color, text) with transform controls, layering, and live preview.
- **Streaming & recording** — RTMP/RTMPS output via GStreamer with hardware-accelerated encoding (H.264/H.265, AAC).
- **Transition effects** — GPU-accelerated scene transitions with a shader-based effect system.
- **Virtual camera** — System extension that exposes composited output to other apps via IOSurface.
- **Dockable panel layout** — Flexible workspace with draggable, resizable, detachable panels.
- **Settings as TOML** — Human-readable configuration. No databases, no binary formats.

## Tech Stack

| Layer | Technology | Role |
|---|---|---|
| Window + input | `winit` | OS window, input events, event loop |
| GPU | `wgpu` | Metal / Vulkan / DX12 abstraction |
| UI layout | `egui` + `egui-wgpu` | Immediate-mode layout and input |
| Text rendering | `cosmic-text` | Subpixel-quality GPU text shaping |
| Video pipeline | GStreamer | Capture, encoding, streaming, recording |
| Icons | Phosphor Icons | Consistent line icon set |
| Async runtime | `tokio` | Channels, settings I/O, background tasks |

## Architecture

```
winit event loop
  └── wgpu device + surface
        ├── custom UI renderer     (wgpu pipelines: panels, shadows, animations)
        ├── egui-wgpu integration  (layout + input only)
        ├── text pass              (subpixel AA text via cosmic-text)
        ├── preview pipeline       (GStreamer frame → wgpu texture)
        └── GStreamer thread       (dedicated thread, channels to render loop)
```

The UI layer never touches video data directly. GStreamer runs on a dedicated thread and communicates with the render loop exclusively via `tokio` channels.

## Prerequisites

- **Rust** (2024 edition) — install via [rustup](https://rustup.rs/)
- **GStreamer** — required for video capture and encoding
  ```bash
  # macOS
  brew install gstreamer gst-plugins-base gst-plugins-good gst-plugins-bad gst-plugins-ugly
  ```
- **Xcode Command Line Tools** (macOS) — required for the virtual camera system extension

## Building

```bash
cargo build              # debug build
cargo build --release    # release build
cargo run                # run in debug mode
```

### macOS App Bundle

To create a signed `.app` bundle (includes virtual camera system extension):

```bash
cargo build --release
./scripts/bundle.sh          # release bundle
./scripts/bundle.sh --debug  # debug bundle
```

## Development

```bash
cargo test             # run all tests
cargo clippy           # lint
cargo fmt --check      # check formatting
```

### Project Structure

```
lodestone/
├── src/
│   ├── main.rs                  # winit event loop, wgpu init
│   ├── state.rs                 # shared app state (Arc<Mutex<AppState>>)
│   ├── renderer/                # render loop, wgpu pipelines, text, transitions
│   ├── ui/                      # egui panels, dockview layout, widgets, theme
│   ├── gstreamer/               # capture, encoding, streaming, recording, virtual camera
│   ├── scene.rs                 # scene/source data model
│   └── settings.rs              # TOML settings persistence
├── lodestone-camera-dal/        # CoreMediaIO DAL plugin (virtual camera)
├── lodestone-camera-extension/  # macOS system extension (virtual camera)
├── scripts/                     # build & install scripts
└── docs/                        # project documentation
```

## License

[GPL-3.0](https://www.gnu.org/licenses/gpl-3.0.en.html)
