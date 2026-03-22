# Settings Window Design

## Overview

A dedicated settings window opened via `Meta+,`, replacing the existing dockable settings panel. Uses egui deferred viewports to create a native OS window with minimal chrome (close button only). Settings apply instantly with debounced disk persistence.

## Window Lifecycle

- **Open:** `Meta+,` toggles the settings window. If already open, closes it.
- **Tracking:** `settings_window_open: bool` on `AppManager` controls whether the viewport is shown.
- **Rendering:** During the main window's egui pass, when `settings_window_open` is true, call `ctx.show_viewport_deferred()` with a `ViewportBuilder`:
  - Title: `"Settings"`
  - `inner_size`: configurable, default 700x500
  - `close_button`: true
  - `minimize_button`: false
  - `maximize_button`: false
  - `resizable`: true
- **Close:** The viewport callback checks `ctx.input(|i| i.viewport().close_requested)` and sets `settings_window_open = false`. Also closes on `Escape`.
- **Singleton:** Only one settings window at a time.

## Settings State & Persistence

### Expanded `AppSettings`

All new fields use `#[serde(default)]` for backwards-compatible TOML deserialization.

```rust
pub struct AppSettings {
    pub ui: UiSettings,
    pub general: GeneralSettings,
    pub stream: StreamSettings,
    pub audio: AudioSettings,
    pub video: VideoSettings,
    pub hotkeys: HotkeySettings,
    pub appearance: AppearanceSettings,
    pub advanced: AdvancedSettings,
    pub settings_window: SettingsWindowConfig,
}

pub struct GeneralSettings {
    pub language: String,              // default: "en-US"
    pub check_for_updates: bool,       // default: true
    pub launch_on_startup: bool,       // default: false
    pub confirm_close_while_streaming: bool, // default: true
}

pub struct StreamSettings {
    pub stream_key: String,
    pub destination: StreamDestination, // reuse existing enum (Twitch, YouTube, Custom)
    pub encoder: String,               // default: "x264"
    pub width: u32,                    // default: 1920
    pub height: u32,                   // default: 1080
    pub fps: u32,                      // default: 30
    pub bitrate_kbps: u32,             // default: 4500
}

pub struct AudioSettings {
    pub input_device: String,          // default: "Default"
    pub output_device: String,         // default: "Default"
    pub sample_rate: u32,              // default: 48000
    pub monitoring: String,            // default: "off"
}

pub struct VideoSettings {
    pub base_resolution: String,       // default: "1920x1080"
    pub output_resolution: String,     // default: "1920x1080"
    pub fps: u32,                      // default: 30
    pub color_space: String,           // default: "sRGB"
}

pub struct HotkeySettings {
    pub bindings: HashMap<String, String>, // stubbed, empty default
}

pub struct AppearanceSettings {
    pub accent_color: String,          // default: "#7c6cf0"
    pub font_size: f32,                // default: 13.0
    pub theme: String,                 // default: "dark"
}

pub struct AdvancedSettings {
    pub process_priority: String,      // default: "normal"
    pub network_buffer_size_kb: u32,   // default: 2048
}

pub struct SettingsWindowConfig {
    pub width: f32,                    // default: 700.0
    pub height: f32,                   // default: 500.0
}
```

### Instant Apply + Debounced Persist

- Every UI control mutates `AppState.settings` immediately on interaction. No staging buffer, no Save button.
- A dirty flag + timestamp is set on each mutation. The render loop checks: if dirty and >500ms since last change, write `settings.toml` to disk. Prevents thrashing during rapid slider drags.

## UI Structure

### Module

New file: `src/ui/settings_window.rs`

Entry point:

```rust
pub fn show(ctx: &egui::Context, state: &Arc<Mutex<AppState>>)
```

Called from the main egui pass when `settings_window_open` is true. Internally calls `ctx.show_viewport_deferred()` with the viewport callback.

### Layout

Inside the viewport callback:

- `egui::SidePanel::left("settings_sidebar")` — fixed width ~190px, renders grouped navigation
- `egui::CentralPanel` — renders active category content in a vertical `ScrollArea`

### Sidebar

Active category tracked as a `SettingsCategory` enum stored in `egui::Context` memory (keyed to the settings viewport ID).

```rust
enum SettingsCategory {
    General,
    Appearance,
    Hotkeys,
    StreamOutput,
    Audio,
    Video,
    Advanced,
}
```

Grouped navigation with section headers:

| Group        | Items                         |
| ------------ | ----------------------------- |
| App          | General, Appearance, Hotkeys  |
| Broadcasting | Stream / Output, Audio, Video |
| System       | Advanced                      |

Styling:

- Section headers: small uppercase text, muted color, letter-spacing
- Items: text labels (no icons), left accent border (`#7c6cf0`) + subtle background highlight on active item
- Inactive items: muted text color

### Content

Each category gets a `draw_<category>(ui: &mut egui::Ui, settings: &mut AppSettings)` function.

- **General** and **Stream / Output** get real controls (migrated from existing `settings_panel.rs`)
- All other categories: stubbed with placeholder text and 1-2 example controls to demonstrate the pattern

Control patterns:

- Toggles: label + description on left, toggle switch on right, separated by subtle dividers
- Dropdowns: label above, `ComboBox` below
- Text inputs: label above, `TextEdit` below
- Sliders: label + value on left, slider on right

## Keyboard Shortcut

In `AppManager::window_event()`, in the `KeyboardInput` handler:

```rust
if modifiers.super_key() && *key_code == KeyCode::Comma {
    self.settings_window_open = !self.settings_window_open;
    return;
}
```

Inside the viewport callback:

```rust
if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
}
```

## Removing the Old Settings Panel

- Delete `src/ui/settings_panel.rs`
- Remove `PanelType::Settings` from `tree.rs` enum and `display_name()`
- Remove the `Settings` arm from `draw_panel()` in `ui/mod.rs`
- Migrate stream key / destination / resolution / bitrate controls to `settings_window.rs` Stream/Output category
- Handle stale layouts: deserialization should gracefully drop tabs with unknown `PanelType` variants rather than failing

## Implementation Approach

Uses egui 0.33's `ctx.show_viewport_deferred()` API:

- Callback is `Fn(&Context, ViewportClass) + Send + Sync + 'static`
- `Arc<Mutex<AppState>>` cloned into the closure for state access
- `Arc<AtomicBool>` for the `settings_window_open` flag (needed for `Send + Sync` in the closure)
- `egui-winit` already supports `ViewportCommand` handling (close, resize, title, etc.)
- If the backend doesn't support multi-viewport (`embed_viewports() == true`), egui falls back to embedding — the callback receives `ViewportClass::Embedded` and should render as an `egui::Window` overlay instead

## Replacing ProfileSettings

The existing `ProfileSettings` struct and profile system (`active_profile`, `profile_path()`) are replaced by the new `StreamSettings` embedded directly in `AppSettings`. The profile concept (multiple named configs) is removed — it was unused and premature. Stream configuration lives in a single `[stream]` table in `settings.toml`.

- Delete `ProfileSettings` struct
- Delete `profile_path()` function
- Remove `active_profile` field from `AppSettings`
- Migrate `ProfileSettings` fields (destination, stream_key, width, height, fps, bitrate_kbps) into `StreamSettings`
- Update tests: replace `profile_roundtrip` test with `stream_settings_roundtrip`

## Files Changed

| File                        | Change                                                                                    |
| --------------------------- | ----------------------------------------------------------------------------------------- |
| `src/ui/settings_window.rs` | New — all settings window UI                                                              |
| `src/settings.rs`           | Expand `AppSettings` with new category structs                                            |
| `src/main.rs`               | Add `settings_window_open`, `Meta+,` handler, call `settings_window::show()` in egui pass |
| `src/state.rs`              | Add dirty flag + timestamp for debounced persist                                          |
| `src/ui/mod.rs`             | Remove `Settings` from `draw_panel()`, add `settings_window` module                       |
| `src/ui/settings_panel.rs`  | Delete                                                                                    |
| `src/ui/layout/tree.rs`     | Remove `PanelType::Settings`, handle deserialization of unknown variants                  |
| `src/ui/layout/render.rs`   | No changes needed (`Settings` is not in `DOCKABLE_TYPES`)                                 |
