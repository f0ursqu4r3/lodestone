# Settings Window Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the dockable settings panel with a dedicated settings window opened via `Meta+,`, using egui deferred viewports.

**Architecture:** The settings window is a native OS window created by egui's `ctx.show_viewport_deferred()` API. Settings state lives in expanded `AppSettings` structs within `AppState` (shared via `Arc<Mutex<AppState>>`). Changes apply instantly; disk persistence is debounced. The old `PanelType::Settings` and profile system are removed.

**Tech Stack:** Rust, egui 0.33 (deferred viewports), winit 0.30, serde + TOML

**Spec:** `docs/superpowers/specs/2026-03-21-settings-window-design.md`

---

### Task 1: Expand AppSettings with category structs

**Files:**
- Modify: `src/settings.rs` (full rewrite of structs and tests)
- Modify: `src/state.rs:60` (update Default)

- [ ] **Step 1: Write failing tests for new settings structs**

Add tests to `src/settings.rs` that exercise the new category structs:

```rust
#[test]
fn expanded_settings_roundtrip() {
    let settings = AppSettings::default();
    let toml_str = toml::to_string_pretty(&settings).unwrap();
    let parsed: AppSettings = toml::from_str(&toml_str).unwrap();
    assert_eq!(parsed.general.language, "en-US");
    assert_eq!(parsed.stream.bitrate_kbps, 4500);
    assert_eq!(parsed.settings_window.width, 700.0);
}

#[test]
fn backwards_compat_empty_toml() {
    // An empty TOML string should deserialize to defaults (serde(default))
    let parsed: AppSettings = toml::from_str("").unwrap();
    assert_eq!(parsed.general.language, "en-US");
    assert!(parsed.general.check_for_updates);
}

#[test]
fn backwards_compat_old_format() {
    // Old format with just active_profile and ui should still parse
    let old_toml = r#"
active_profile = "Default"

[ui]
scene_panel_open = true
mixer_panel_open = true
controls_panel_open = true
"#;
    let parsed: AppSettings = toml::from_str(old_toml).unwrap();
    assert_eq!(parsed.general.language, "en-US");
    assert_eq!(parsed.stream.bitrate_kbps, 4500);
}

#[test]
fn stream_settings_roundtrip() {
    let settings = AppSettings::default();
    let toml_str = toml::to_string_pretty(&settings).unwrap();
    let parsed: AppSettings = toml::from_str(&toml_str).unwrap();
    assert!(matches!(parsed.stream.destination, StreamDestination::Twitch));
    assert_eq!(parsed.stream.width, 1920);
    assert_eq!(parsed.stream.height, 1080);
    assert_eq!(parsed.stream.fps, 30);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib settings::tests`
Expected: FAIL — structs `GeneralSettings`, `StreamSettings`, etc. don't exist yet.

- [ ] **Step 3: Implement the expanded AppSettings**

Replace the structs in `src/settings.rs`. Remove `ProfileSettings`, `profile_path()`, and `active_profile`. Keep `config_dir()`, `settings_path()`, `AppSettings::load_from()`, `AppSettings::save_to()`. All new category structs need `#[derive(Debug, Clone, Serialize, Deserialize)]` and `#[serde(default)]` on the parent field.

```rust
use crate::obs::StreamDestination;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    #[serde(default)]
    pub ui: UiSettings,
    #[serde(default)]
    pub general: GeneralSettings,
    #[serde(default)]
    pub stream: StreamSettings,
    #[serde(default)]
    pub audio: AudioSettings,
    #[serde(default)]
    pub video: VideoSettings,
    #[serde(default)]
    pub hotkeys: HotkeySettings,
    #[serde(default)]
    pub appearance: AppearanceSettings,
    #[serde(default)]
    pub advanced: AdvancedSettings,
    #[serde(default)]
    pub settings_window: SettingsWindowConfig,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            ui: UiSettings::default(),
            general: GeneralSettings::default(),
            stream: StreamSettings::default(),
            audio: AudioSettings::default(),
            video: VideoSettings::default(),
            hotkeys: HotkeySettings::default(),
            appearance: AppearanceSettings::default(),
            advanced: AdvancedSettings::default(),
            settings_window: SettingsWindowConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiSettings {
    pub scene_panel_open: bool,
    pub mixer_panel_open: bool,
    pub controls_panel_open: bool,
}

impl Default for UiSettings {
    fn default() -> Self {
        Self {
            scene_panel_open: true,
            mixer_panel_open: true,
            controls_panel_open: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralSettings {
    pub language: String,
    pub check_for_updates: bool,
    pub launch_on_startup: bool,
    pub confirm_close_while_streaming: bool,
}

impl Default for GeneralSettings {
    fn default() -> Self {
        Self {
            language: "en-US".to_string(),
            check_for_updates: true,
            launch_on_startup: false,
            confirm_close_while_streaming: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamSettings {
    pub stream_key: String,
    pub destination: StreamDestination,
    pub encoder: String,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub bitrate_kbps: u32,
}

impl Default for StreamSettings {
    fn default() -> Self {
        Self {
            stream_key: String::new(),
            destination: StreamDestination::Twitch,
            encoder: "x264".to_string(),
            width: 1920,
            height: 1080,
            fps: 30,
            bitrate_kbps: 4500,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioSettings {
    pub input_device: String,
    pub output_device: String,
    pub sample_rate: u32,
    pub monitoring: String,
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            input_device: "Default".to_string(),
            output_device: "Default".to_string(),
            sample_rate: 48000,
            monitoring: "off".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoSettings {
    pub base_resolution: String,
    pub output_resolution: String,
    pub fps: u32,
    pub color_space: String,
}

impl Default for VideoSettings {
    fn default() -> Self {
        Self {
            base_resolution: "1920x1080".to_string(),
            output_resolution: "1920x1080".to_string(),
            fps: 30,
            color_space: "sRGB".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotkeySettings {
    pub bindings: HashMap<String, String>,
}

impl Default for HotkeySettings {
    fn default() -> Self {
        Self {
            bindings: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppearanceSettings {
    pub accent_color: String,
    pub font_size: f32,
    pub theme: String,
}

impl Default for AppearanceSettings {
    fn default() -> Self {
        Self {
            accent_color: "#7c6cf0".to_string(),
            font_size: 13.0,
            theme: "dark".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdvancedSettings {
    pub process_priority: String,
    pub network_buffer_size_kb: u32,
}

impl Default for AdvancedSettings {
    fn default() -> Self {
        Self {
            process_priority: "normal".to_string(),
            network_buffer_size_kb: 2048,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettingsWindowConfig {
    pub width: f32,
    pub height: f32,
}

impl Default for SettingsWindowConfig {
    fn default() -> Self {
        Self {
            width: 700.0,
            height: 500.0,
        }
    }
}
```

Keep `load_from`, `save_to`, `config_dir()`, `settings_path()` unchanged.

- [ ] **Step 4: Fix compilation errors in state.rs**

In `src/state.rs`, the `AppState::default()` creates `AppSettings::default()` which should just work. Grep for any remaining references to `active_profile` or `ProfileSettings` across the codebase and remove them.

Also update `src/state.rs` to add dirty tracking fields:

```rust
pub struct AppState {
    // ... existing fields ...
    pub settings: AppSettings,
    pub settings_dirty: bool,
    pub settings_last_changed: std::time::Instant,
    // ... existing fields ...
}
```

In `Default for AppState`:
```rust
settings_dirty: false,
settings_last_changed: std::time::Instant::now(),
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --lib settings::tests`
Expected: All 4 new tests PASS.

- [ ] **Step 6: Run full test suite**

Run: `cargo test`
Expected: All tests pass. Fix any compilation errors from removed `active_profile` / `ProfileSettings` references.

- [ ] **Step 7: Commit**

```bash
git add src/settings.rs src/state.rs
git commit -m "feat: expand AppSettings with category structs, remove ProfileSettings"
```

---

### Task 2: Remove PanelType::Settings and old settings panel

**Files:**
- Delete: `src/ui/settings_panel.rs`
- Modify: `src/ui/mod.rs:5,11-18` (remove settings_panel module and match arm)
- Modify: `src/ui/layout/tree.rs:14-38` (remove Settings variant, is_dockable)
- Modify: `src/ui/layout/serialize.rs` (handle unknown PanelType gracefully)

- [ ] **Step 1: Write a test for deserializing layouts with unknown panel types**

Add to `src/ui/layout/serialize.rs` tests:

```rust
#[test]
fn unknown_panel_type_drops_gracefully() {
    // Simulate a saved layout with "Settings" panel type that no longer exists
    let toml_str = r#"
[tree]
type = "leaf"
group_id = 1

[[groups]]
id = 1
active_tab = 0
[[groups.tabs]]
panel_id = 1
panel_type = "Settings"
[[groups.tabs]]
panel_id = 2
panel_type = "Preview"
"#;
    let result = deserialize_full_layout(toml_str);
    assert!(result.is_ok());
    let (layout, _) = result.unwrap();
    // The Settings tab should be dropped, leaving only Preview
    let all_panels = layout.collect_all_panels();
    assert_eq!(all_panels.len(), 1);
    assert_eq!(all_panels[0].1, PanelType::Preview);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib layout::serialize::tests::unknown_panel_type_drops_gracefully`
Expected: FAIL — `Settings` variant still exists, so it deserializes fine (not dropped). After removing the variant, deserialization will error instead of dropping.

- [ ] **Step 3: Remove PanelType::Settings**

In `src/ui/layout/tree.rs`:
- Remove `Settings` from `PanelType` enum (line 19)
- Remove `Self::Settings => "Settings"` from `display_name()` (line 29)
- Remove the `is_dockable()` method entirely (lines 34-37) — it only existed to exclude Settings

In `src/ui/mod.rs`:
- Remove `pub mod settings_panel;` (line 5)
- Remove `PanelType::Settings => settings_panel::draw(ui, state, id),` from `draw_panel()` (line 17)

Delete `src/ui/settings_panel.rs`.

- [ ] **Step 4: Handle unknown panel types in deserialization**

Use separate types for serialization (writes `PanelType` directly) and deserialization (tolerates unknown variants via `toml::Value`).

In `src/ui/layout/serialize.rs`, keep `SerializedTab` for serialization and add parallel deserialization types:

```rust
/// Used for serialization — writes the PanelType enum directly.
#[derive(Serialize, Debug, Clone)]
struct SerializedTab {
    panel_id: u64,
    panel_type: PanelType,
}

/// Used for deserialization — tolerates unknown panel types.
#[derive(Deserialize, Debug, Clone)]
struct DeserializedTab {
    panel_id: u64,
    panel_type: toml::Value,
}

/// Used for deserialization — tolerates groups with unknown panel types.
#[derive(Deserialize, Debug, Clone)]
struct DeserializedGroup {
    id: u64,
    active_tab: usize,
    tabs: Vec<DeserializedTab>,
}

/// Top-level TOML document for deserialization (tolerant of unknown panel types).
#[derive(Deserialize, Debug, Clone)]
struct DeserializedLayout {
    tree: SerializedNode,
    #[serde(default)]
    groups: Vec<DeserializedGroup>,
    #[serde(default)]
    floating: Vec<SerializedFloating>,
    #[serde(default)]
    detached: Vec<DetachedEntry>,
}
```

Keep the existing `SerializedGroup` and `SerializedLayout` for serialization only (add `#[derive(Serialize)]` only, remove `Deserialize`).

In `deserialize_full_layout`, change to use `DeserializedLayout` and filter tabs:

```rust
pub fn deserialize_full_layout(toml_str: &str) -> Result<(DockLayout, Vec<DetachedEntry>)> {
    let doc: DeserializedLayout = toml::from_str(toml_str).context("failed to parse layout TOML")?;

    let mut groups: HashMap<GroupId, Group> = HashMap::new();
    let mut max_group_id: u64 = 0;
    let mut max_panel_id: u64 = 0;

    for sg in &doc.groups {
        max_group_id = max_group_id.max(sg.id);
        let tabs: Vec<TabEntry> = sg
            .tabs
            .iter()
            .filter_map(|t| {
                let panel_type: PanelType = t.panel_type.clone().try_into().ok()?;
                max_panel_id = max_panel_id.max(t.panel_id);
                Some(TabEntry {
                    panel_id: PanelId(t.panel_id),
                    panel_type,
                })
            })
            .collect();

        // Skip groups where all tabs had unknown panel types
        if tabs.is_empty() {
            log::warn!("Dropping group {} — all tabs had unknown panel types", sg.id);
            continue;
        }

        let active_tab = sg.active_tab.min(tabs.len().saturating_sub(1));
        let group = Group {
            id: GroupId(sg.id),
            tabs,
            active_tab,
        };
        groups.insert(group.id, group);
    }

    // ... rest of function unchanged (rebuild tree, floating, etc.) ...
```

**Handling orphaned tree nodes:** After rebuilding the tree, leaf nodes may reference groups that were dropped (all-unknown-tabs). Add a post-processing step after `rebuild_node`:

```rust
/// Replace leaf nodes referencing missing groups with a default Preview group.
fn repair_orphaned_leaves(
    nodes: &mut HashMap<NodeId, SplitNode>,
    groups: &mut HashMap<GroupId, Group>,
    root: NodeId,
) {
    let node_ids: Vec<NodeId> = nodes.keys().copied().collect();
    for node_id in node_ids {
        if let Some(SplitNode::Leaf { group_id }) = nodes.get(&node_id) {
            if !groups.contains_key(group_id) {
                // Replace with a new default group
                let new_group = Group::new(PanelType::Preview);
                let new_gid = new_group.id;
                groups.insert(new_gid, new_group);
                nodes.insert(node_id, SplitNode::Leaf { group_id: new_gid });
                log::warn!("Replaced orphaned leaf node {:?} with default Preview group", node_id);
            }
        }
    }
}
```

Call `repair_orphaned_leaves(&mut nodes, &mut groups, root_id)` after `rebuild_node` returns, before constructing the `DockLayout`.

- [ ] **Step 5: Run tests**

Run: `cargo test`
Expected: All tests pass, including the new `unknown_panel_type_drops_gracefully` test.

- [ ] **Step 6: Commit**

```bash
git add -u
git commit -m "refactor: remove PanelType::Settings, handle unknown types in deserialization"
```

---

### Task 3: Add debounced settings persistence to render loop

**Files:**
- Modify: `src/main.rs` (add settings persist check in RedrawRequested)

- [ ] **Step 1: Write the debounced persist logic**

In `src/main.rs`, inside the `WindowEvent::RedrawRequested` handler, after the render call and before requesting redraw, add a settings persistence check. Only do this for the main window:

```rust
// Debounced settings persistence
if Some(window_id) == self.main_window_id {
    let mut app_state = self.state.lock().unwrap();
    if app_state.settings_dirty
        && app_state.settings_last_changed.elapsed() > std::time::Duration::from_millis(500)
    {
        let path = settings::settings_path();
        if let Err(e) = app_state.settings.save_to(&path) {
            log::warn!("Failed to save settings: {e}");
        }
        app_state.settings_dirty = false;
    }
    drop(app_state);
}
```

Note: This needs to be placed carefully — the `app_state` lock is already taken and dropped earlier in the `RedrawRequested` handler (lines 423-435). This new lock acquisition must happen after that drop.

- [ ] **Step 2: Verify it compiles**

Run: `cargo build`
Expected: Compiles without errors.

- [ ] **Step 3: Commit**

```bash
git add src/main.rs
git commit -m "feat: add debounced settings persistence in render loop"
```

---

### Task 4: Add Meta+, keyboard shortcut

**Files:**
- Modify: `src/main.rs` (add settings_window_open field, keyboard handler)

- [ ] **Step 1: Add settings_window_open to AppManager**

Add field to `AppManager` struct:
```rust
settings_window_open: Arc<AtomicBool>,
```

Import at top of `src/main.rs`:
```rust
use std::sync::atomic::{AtomicBool, Ordering};
```

Initialize in `AppManager::new()`:
```rust
settings_window_open: Arc::new(AtomicBool::new(false)),
```

- [ ] **Step 2: Add Meta+, handler**

In the `KeyboardInput` match arm (after the existing `Ctrl+Shift+R` check at line 388-391), add:

```rust
if self.modifiers.super_key() && *key_code == KeyCode::Comma {
    let current = self.settings_window_open.load(Ordering::Relaxed);
    self.settings_window_open.store(!current, Ordering::Relaxed);
    return;
}
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo build`
Expected: Compiles. The `settings_window_open` flag is set but not yet consumed by any viewport code (that comes in Task 5).

- [ ] **Step 4: Commit**

```bash
git add src/main.rs
git commit -m "feat: add Meta+, keyboard shortcut to toggle settings window"
```

---

### Task 5: Create settings window with egui deferred viewport

**Files:**
- Create: `src/ui/settings_window.rs`
- Modify: `src/ui/mod.rs` (add module)
- Modify: `src/main.rs` (call show() in render pass)

- [ ] **Step 1: Create the settings_window module**

Create `src/ui/settings_window.rs` with the full settings window implementation:

```rust
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::settings::{
    AppSettings, AppearanceSettings, AdvancedSettings, AudioSettings, GeneralSettings,
    HotkeySettings, SettingsWindowConfig, StreamSettings, VideoSettings,
};
use crate::state::AppState;

/// Settings category for sidebar navigation.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum SettingsCategory {
    General,
    Appearance,
    Hotkeys,
    StreamOutput,
    Audio,
    Video,
    Advanced,
}

impl SettingsCategory {
    fn label(&self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Appearance => "Appearance",
            Self::Hotkeys => "Hotkeys",
            Self::StreamOutput => "Stream / Output",
            Self::Audio => "Audio",
            Self::Video => "Video",
            Self::Advanced => "Advanced",
        }
    }
}

/// Groups for the sidebar section headers.
struct SidebarGroup {
    label: &'static str,
    categories: &'static [SettingsCategory],
}

const SIDEBAR_GROUPS: &[SidebarGroup] = &[
    SidebarGroup {
        label: "APP",
        categories: &[
            SettingsCategory::General,
            SettingsCategory::Appearance,
            SettingsCategory::Hotkeys,
        ],
    },
    SidebarGroup {
        label: "BROADCASTING",
        categories: &[
            SettingsCategory::StreamOutput,
            SettingsCategory::Audio,
            SettingsCategory::Video,
        ],
    },
    SidebarGroup {
        label: "SYSTEM",
        categories: &[SettingsCategory::Advanced],
    },
];

const SETTINGS_VIEWPORT_ID: &str = "settings_window";
const ACCENT_COLOR: egui::Color32 = egui::Color32::from_rgb(124, 108, 240);

/// Show the settings window as a deferred viewport.
/// Call this during the main window's egui pass when the settings window should be open.
pub fn show(ctx: &egui::Context, state: &Arc<Mutex<AppState>>, open: &Arc<AtomicBool>) {
    if !open.load(Ordering::Relaxed) {
        return;
    }

    let state_clone = Arc::clone(state);
    let open_clone = Arc::clone(open);

    let window_size = {
        let app_state = state.lock().unwrap();
        let cfg = &app_state.settings.settings_window;
        egui::vec2(cfg.width, cfg.height)
    };

    let viewport_id = egui::ViewportId::from_hash_of(SETTINGS_VIEWPORT_ID);
    let viewport_builder = egui::ViewportBuilder::default()
        .with_title("Settings")
        .with_inner_size(window_size)
        .with_min_inner_size(egui::vec2(500.0, 350.0))
        .with_close_button(true)
        .with_minimize_button(false)
        .with_maximize_button(false)
        .with_resizable(true);

    ctx.show_viewport_deferred(viewport_id, viewport_builder, move |ctx, _class| {
        // Handle close
        if ctx.input(|i| i.viewport().close_requested) {
            open_clone.store(false, Ordering::Relaxed);
            return;
        }

        // Handle Escape
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            open_clone.store(false, Ordering::Relaxed);
            return;
        }

        // Get active category from egui memory
        let category_id = egui::Id::new("settings_active_category");
        let mut active = ctx.memory(|m| {
            m.data
                .get_temp::<SettingsCategory>(category_id)
                .unwrap_or(SettingsCategory::General)
        });

        // Sidebar
        egui::SidePanel::left("settings_sidebar")
            .exact_width(190.0)
            .resizable(false)
            .frame(egui::Frame::NONE.fill(egui::Color32::from_rgb(24, 24, 37)))
            .show(ctx, |ui| {
                ui.add_space(12.0);
                render_sidebar(ui, &mut active);
            });

        // Content
        egui::CentralPanel::default()
            .frame(
                egui::Frame::NONE
                    .fill(egui::Color32::from_rgb(30, 30, 46))
                    .inner_margin(egui::Margin::same(24)),
            )
            .show(ctx, |ui| {
                let mut app_state = state_clone.lock().unwrap();
                render_content(ui, active, &mut app_state);
            });

        // Store active category
        ctx.memory_mut(|m| m.data.insert_temp(category_id, active));
    });
}

/// Render the grouped sidebar navigation.
fn render_sidebar(ui: &mut egui::Ui, active: &mut SettingsCategory) {
    for (i, group) in SIDEBAR_GROUPS.iter().enumerate() {
        if i > 0 {
            ui.add_space(12.0);
        }

        // Section header
        ui.horizontal(|ui| {
            ui.add_space(16.0);
            ui.label(
                egui::RichText::new(group.label)
                    .size(10.0)
                    .color(egui::Color32::from_rgb(88, 91, 112))
                    .strong(),
            );
        });
        ui.add_space(2.0);

        // Category items
        for &category in group.categories {
            let is_active = *active == category;
            let (bg, text_color, border_color) = if is_active {
                (
                    egui::Color32::from_rgba_premultiplied(124, 108, 240, 25),
                    egui::Color32::from_rgb(205, 214, 244),
                    ACCENT_COLOR,
                )
            } else {
                (
                    egui::Color32::TRANSPARENT,
                    egui::Color32::from_rgb(108, 112, 134),
                    egui::Color32::TRANSPARENT,
                )
            };

            let desired_size = egui::vec2(ui.available_width(), 28.0);
            let (rect, response) = ui.allocate_exact_size(desired_size, egui::Sense::click());

            if response.clicked() {
                *active = category;
            }

            // Hover effect for inactive items
            let bg = if !is_active && response.hovered() {
                egui::Color32::from_rgba_premultiplied(255, 255, 255, 8)
            } else {
                bg
            };

            // Background
            ui.painter().rect_filled(rect, 0.0, bg);

            // Left accent border
            if is_active {
                let border_rect = egui::Rect::from_min_size(
                    rect.min,
                    egui::vec2(2.0, rect.height()),
                );
                ui.painter().rect_filled(border_rect, 0.0, border_color);
            }

            // Label
            let text_pos = egui::pos2(rect.min.x + 16.0, rect.center().y);
            ui.painter().text(
                text_pos,
                egui::Align2::LEFT_CENTER,
                category.label(),
                egui::FontId::proportional(13.0),
                text_color,
            );
        }
    }
}

/// Render the content area for the active category.
fn render_content(ui: &mut egui::Ui, category: SettingsCategory, state: &mut AppState) {
    // Category header
    ui.label(
        egui::RichText::new(category.label())
            .size(18.0)
            .strong()
            .color(egui::Color32::from_rgb(205, 214, 244)),
    );
    ui.add_space(4.0);
    ui.label(
        egui::RichText::new(category_description(category))
            .size(12.0)
            .color(egui::Color32::from_rgb(108, 112, 134)),
    );
    ui.add_space(20.0);

    egui::ScrollArea::vertical().show(ui, |ui| {
        let changed = match category {
            SettingsCategory::General => draw_general(ui, &mut state.settings.general),
            SettingsCategory::StreamOutput => draw_stream(ui, &mut state.settings.stream),
            SettingsCategory::Audio => draw_audio(ui, &mut state.settings.audio),
            SettingsCategory::Video => draw_video(ui, &mut state.settings.video),
            SettingsCategory::Hotkeys => draw_hotkeys(ui, &mut state.settings.hotkeys),
            SettingsCategory::Appearance => draw_appearance(ui, &mut state.settings.appearance),
            SettingsCategory::Advanced => draw_advanced(ui, &mut state.settings.advanced),
        };
        if changed {
            state.settings_dirty = true;
            state.settings_last_changed = std::time::Instant::now();
        }
    });
}

fn category_description(category: SettingsCategory) -> &'static str {
    match category {
        SettingsCategory::General => "Application behavior and startup",
        SettingsCategory::Appearance => "Theme, colors, and visual preferences",
        SettingsCategory::Hotkeys => "Keyboard shortcuts",
        SettingsCategory::StreamOutput => "Stream destination and encoding",
        SettingsCategory::Audio => "Audio devices and monitoring",
        SettingsCategory::Video => "Video capture and output",
        SettingsCategory::Advanced => "Performance and network tuning",
    }
}

// ---------------------------------------------------------------------------
// Category draw functions — return true if any value changed
// ---------------------------------------------------------------------------

fn draw_general(ui: &mut egui::Ui, settings: &mut GeneralSettings) -> bool {
    let mut changed = false;

    // Language dropdown
    ui.label(egui::RichText::new("Language").size(12.0).color(egui::Color32::from_rgb(166, 173, 200)));
    ui.add_space(4.0);
    let prev = settings.language.clone();
    egui::ComboBox::from_id_salt("general_language")
        .selected_text(&settings.language)
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut settings.language, "en-US".to_string(), "English (US)");
        });
    changed |= settings.language != prev;
    ui.add_space(12.0);

    changed |= draw_toggle(
        ui,
        "Check for updates automatically",
        "Notify when a new version is available",
        &mut settings.check_for_updates,
    );
    changed |= draw_toggle(
        ui,
        "Launch on startup",
        "Start Lodestone when you log in",
        &mut settings.launch_on_startup,
    );
    changed |= draw_toggle(
        ui,
        "Confirm before closing",
        "Show a dialog when quitting while streaming",
        &mut settings.confirm_close_while_streaming,
    );

    changed
}

fn draw_stream(ui: &mut egui::Ui, settings: &mut StreamSettings) -> bool {
    let mut changed = false;

    // Destination
    ui.label(egui::RichText::new("Destination").size(12.0).color(egui::Color32::from_rgb(166, 173, 200)));
    ui.add_space(4.0);
    let dest_label = match &settings.destination {
        crate::obs::StreamDestination::Twitch => "Twitch",
        crate::obs::StreamDestination::YouTube => "YouTube",
        crate::obs::StreamDestination::CustomRtmp { .. } => "Custom RTMP",
    };
    egui::ComboBox::from_id_salt("stream_destination")
        .selected_text(dest_label)
        .show_ui(ui, |ui| {
            if ui.selectable_label(matches!(settings.destination, crate::obs::StreamDestination::Twitch), "Twitch").clicked() {
                settings.destination = crate::obs::StreamDestination::Twitch;
                changed = true;
            }
            if ui.selectable_label(matches!(settings.destination, crate::obs::StreamDestination::YouTube), "YouTube").clicked() {
                settings.destination = crate::obs::StreamDestination::YouTube;
                changed = true;
            }
            if ui.selectable_label(matches!(settings.destination, crate::obs::StreamDestination::CustomRtmp { .. }), "Custom RTMP").clicked() {
                settings.destination = crate::obs::StreamDestination::CustomRtmp { url: String::new() };
                changed = true;
            }
        });
    ui.add_space(12.0);

    // Stream key
    ui.label(egui::RichText::new("Stream Key").size(12.0).color(egui::Color32::from_rgb(166, 173, 200)));
    ui.add_space(4.0);
    let resp = ui.add(egui::TextEdit::singleline(&mut settings.stream_key).password(true));
    changed |= resp.changed();
    ui.add_space(12.0);

    // Resolution
    ui.label(egui::RichText::new("Output Resolution").size(12.0).color(egui::Color32::from_rgb(166, 173, 200)));
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        changed |= ui.add(egui::DragValue::new(&mut settings.width).range(320..=7680).suffix(" w")).changed();
        ui.label("x");
        changed |= ui.add(egui::DragValue::new(&mut settings.height).range(240..=4320).suffix(" h")).changed();
    });
    ui.add_space(12.0);

    // FPS
    ui.label(egui::RichText::new("Frame Rate").size(12.0).color(egui::Color32::from_rgb(166, 173, 200)));
    ui.add_space(4.0);
    changed |= ui.add(egui::DragValue::new(&mut settings.fps).range(1..=240).suffix(" fps")).changed();
    ui.add_space(12.0);

    // Bitrate
    ui.label(egui::RichText::new("Bitrate").size(12.0).color(egui::Color32::from_rgb(166, 173, 200)));
    ui.add_space(4.0);
    changed |= ui.add(egui::Slider::new(&mut settings.bitrate_kbps, 500..=20000).suffix(" kbps")).changed();
    ui.add_space(12.0);

    // Encoder
    ui.label(egui::RichText::new("Encoder").size(12.0).color(egui::Color32::from_rgb(166, 173, 200)));
    ui.add_space(4.0);
    let prev_enc = settings.encoder.clone();
    egui::ComboBox::from_id_salt("stream_encoder")
        .selected_text(&settings.encoder)
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut settings.encoder, "x264".to_string(), "x264 (CPU)");
            ui.selectable_value(&mut settings.encoder, "nvenc".to_string(), "NVENC (NVIDIA)");
            ui.selectable_value(&mut settings.encoder, "qsv".to_string(), "Quick Sync (Intel)");
            ui.selectable_value(&mut settings.encoder, "amf".to_string(), "AMF (AMD)");
        });
    changed |= settings.encoder != prev_enc;

    changed
}

fn draw_audio(ui: &mut egui::Ui, settings: &mut AudioSettings) -> bool {
    let mut changed = false;

    ui.label(egui::RichText::new("Input Device").size(12.0).color(egui::Color32::from_rgb(166, 173, 200)));
    ui.add_space(4.0);
    let prev = settings.input_device.clone();
    egui::ComboBox::from_id_salt("audio_input")
        .selected_text(&settings.input_device)
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut settings.input_device, "Default".to_string(), "Default");
        });
    changed |= settings.input_device != prev;
    ui.add_space(12.0);

    ui.label(egui::RichText::new("Output Device").size(12.0).color(egui::Color32::from_rgb(166, 173, 200)));
    ui.add_space(4.0);
    let prev = settings.output_device.clone();
    egui::ComboBox::from_id_salt("audio_output")
        .selected_text(&settings.output_device)
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut settings.output_device, "Default".to_string(), "Default");
        });
    changed |= settings.output_device != prev;
    ui.add_space(12.0);

    ui.label(egui::RichText::new("Sample Rate").size(12.0).color(egui::Color32::from_rgb(166, 173, 200)));
    ui.add_space(4.0);
    let prev = settings.sample_rate;
    egui::ComboBox::from_id_salt("audio_sample_rate")
        .selected_text(format!("{} Hz", settings.sample_rate))
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut settings.sample_rate, 44100, "44100 Hz");
            ui.selectable_value(&mut settings.sample_rate, 48000, "48000 Hz");
        });
    changed |= settings.sample_rate != prev;

    changed
}

fn draw_video(ui: &mut egui::Ui, settings: &mut VideoSettings) -> bool {
    let mut changed = false;

    ui.label(egui::RichText::new("Base (Canvas) Resolution").size(12.0).color(egui::Color32::from_rgb(166, 173, 200)));
    ui.add_space(4.0);
    let prev = settings.base_resolution.clone();
    egui::ComboBox::from_id_salt("video_base_res")
        .selected_text(&settings.base_resolution)
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut settings.base_resolution, "1920x1080".to_string(), "1920x1080");
            ui.selectable_value(&mut settings.base_resolution, "2560x1440".to_string(), "2560x1440");
            ui.selectable_value(&mut settings.base_resolution, "3840x2160".to_string(), "3840x2160");
        });
    changed |= settings.base_resolution != prev;
    ui.add_space(12.0);

    ui.label(egui::RichText::new("Color Space").size(12.0).color(egui::Color32::from_rgb(166, 173, 200)));
    ui.add_space(4.0);
    let prev = settings.color_space.clone();
    egui::ComboBox::from_id_salt("video_color_space")
        .selected_text(&settings.color_space)
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut settings.color_space, "sRGB".to_string(), "sRGB");
            ui.selectable_value(&mut settings.color_space, "Rec. 709".to_string(), "Rec. 709");
        });
    changed |= settings.color_space != prev;

    changed
}

fn draw_hotkeys(ui: &mut egui::Ui, _settings: &mut HotkeySettings) -> bool {
    ui.label(
        egui::RichText::new("Hotkey configuration coming soon.")
            .size(13.0)
            .color(egui::Color32::from_rgb(108, 112, 134)),
    );
    false
}

fn draw_appearance(ui: &mut egui::Ui, settings: &mut AppearanceSettings) -> bool {
    let mut changed = false;

    ui.label(egui::RichText::new("Theme").size(12.0).color(egui::Color32::from_rgb(166, 173, 200)));
    ui.add_space(4.0);
    let prev = settings.theme.clone();
    egui::ComboBox::from_id_salt("appearance_theme")
        .selected_text(&settings.theme)
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut settings.theme, "dark".to_string(), "Dark");
        });
    changed |= settings.theme != prev;
    ui.add_space(12.0);

    ui.label(egui::RichText::new("Font Size").size(12.0).color(egui::Color32::from_rgb(166, 173, 200)));
    ui.add_space(4.0);
    changed |= ui.add(egui::Slider::new(&mut settings.font_size, 10.0..=20.0).suffix(" px")).changed();

    changed
}

fn draw_advanced(ui: &mut egui::Ui, settings: &mut AdvancedSettings) -> bool {
    let mut changed = false;

    ui.label(egui::RichText::new("Process Priority").size(12.0).color(egui::Color32::from_rgb(166, 173, 200)));
    ui.add_space(4.0);
    let prev = settings.process_priority.clone();
    egui::ComboBox::from_id_salt("advanced_priority")
        .selected_text(&settings.process_priority)
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut settings.process_priority, "normal".to_string(), "Normal");
            ui.selectable_value(&mut settings.process_priority, "high".to_string(), "High");
            ui.selectable_value(&mut settings.process_priority, "above_normal".to_string(), "Above Normal");
        });
    changed |= settings.process_priority != prev;
    ui.add_space(12.0);

    ui.label(egui::RichText::new("Network Buffer Size").size(12.0).color(egui::Color32::from_rgb(166, 173, 200)));
    ui.add_space(4.0);
    changed |= ui.add(egui::Slider::new(&mut settings.network_buffer_size_kb, 256..=8192).suffix(" KB")).changed();

    changed
}

// ---------------------------------------------------------------------------
// Toggle helper
// ---------------------------------------------------------------------------

/// Draw a toggle row with label, description, and switch. Returns true if changed.
fn draw_toggle(ui: &mut egui::Ui, label: &str, description: &str, value: &mut bool) -> bool {
    ui.add(egui::Separator::default().spacing(1.0));
    ui.add_space(8.0);

    let mut changed = false;
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label(
                egui::RichText::new(label)
                    .size(13.0)
                    .color(egui::Color32::from_rgb(205, 214, 244)),
            );
            ui.label(
                egui::RichText::new(description)
                    .size(11.0)
                    .color(egui::Color32::from_rgb(108, 112, 134)),
            );
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let resp = ui.add(toggle_switch(value));
            changed = resp.changed();
        });
    });
    ui.add_space(8.0);

    changed
}

/// A custom toggle switch widget.
fn toggle_switch(on: &mut bool) -> impl egui::Widget + '_ {
    move |ui: &mut egui::Ui| -> egui::Response {
        let desired_size = egui::vec2(36.0, 20.0);
        let (rect, mut response) = ui.allocate_exact_size(desired_size, egui::Sense::click());

        if response.clicked() {
            *on = !*on;
            response.mark_changed();
        }

        let how_on = ui.ctx().animate_bool_with_time(response.id, *on, 0.15);

        let bg_color = egui::Color32::from_rgb(
            (69.0 + (124.0 - 69.0) * how_on) as u8,
            (71.0 + (108.0 - 71.0) * how_on) as u8,
            (90.0 + (240.0 - 90.0) * how_on) as u8,
        );
        let circle_x = egui::lerp((rect.left() + 10.0)..=(rect.right() - 10.0), how_on);
        let circle_color = if *on {
            egui::Color32::WHITE
        } else {
            egui::Color32::from_rgb(108, 112, 134)
        };

        ui.painter().rect_filled(rect, 10.0, bg_color);
        ui.painter()
            .circle_filled(egui::pos2(circle_x, rect.center().y), 8.0, circle_color);

        response
    }
}
```

- [ ] **Step 2: Register the module and wire into main**

In `src/ui/mod.rs`, add:
```rust
pub mod settings_window;
```

In `src/main.rs`, in the `WindowState::render()` call site inside `RedrawRequested`, we need to call `settings_window::show()` during the egui pass. However, the egui pass happens inside `win.render()` which takes `&mut self`. The settings window needs the egui context.

The safest approach: call `settings_window::show()` from `main.rs` **after** `win.render()` returns (not inside the `ctx.run()` closure), so there is no overlap between the `&mut AppState` borrow and the settings viewport's `Arc<Mutex<AppState>>` lock.

In `src/main.rs`, in the `RedrawRequested` handler, after the existing render call and `drop(app_state)`, add:

```rust
// Show settings window (main window only, after releasing AppState lock)
if Some(window_id) == self.main_window_id {
    if let Some(win) = self.windows.get(&window_id) {
        crate::ui::settings_window::show(
            &win.egui_ctx,
            &self.state,
            &self.settings_window_open,
        );
    }
}
```

**Important:** `show_viewport_deferred()` only *registers* the callback — it does not execute it inline. The callback runs in a separate egui viewport pass managed by `egui-winit`. This means:
- No deadlock: the `Arc<Mutex<AppState>>` is only locked inside the deferred callback, which runs outside the main window's `ctx.run()` pass.
- The `&mut AppState` borrow from the main render is already dropped before `show()` is called.

**Embedded viewport fallback:** If egui's `embed_viewports()` returns true (backend doesn't support multi-viewport), the callback runs **inline** during `show_viewport_deferred()`. In this case, the callback would try to lock `AppState` while it's not held — safe because we call `show()` after `drop(app_state)`. However, the settings UI would render as an embedded egui panel rather than a separate OS window. The `show()` function should check `ViewportClass::Embedded` and render as an `egui::Window` overlay in that case:

```rust
ctx.show_viewport_deferred(viewport_id, viewport_builder, move |ctx, class| {
    match class {
        egui::ViewportClass::Embedded => {
            // Fallback: render as an egui::Window overlay
            let mut is_open = open_clone.load(Ordering::Relaxed);
            egui::Window::new("Settings")
                .open(&mut is_open)
                .default_size(window_size)
                .show(ctx, |ui| {
                    // ... same sidebar + content rendering ...
                });
            if !is_open {
                open_clone.store(false, Ordering::Relaxed);
            }
        }
        _ => {
            // Normal deferred viewport path
            // ... existing close/escape/sidebar/content logic ...
        }
    }
});
```

No changes needed to `WindowState::render()` signature — it stays as-is.

- [ ] **Step 3: Verify it compiles**

Run: `cargo build`
Expected: Compiles without errors.

- [ ] **Step 4: Run the app and test**

Run: `cargo run`
Test:
1. Press `Meta+,` — settings window should appear
2. Click sidebar categories — content should switch
3. Press `Escape` — window should close
4. Press `Meta+,` again — window should reappear
5. Click the close button — window should close

- [ ] **Step 5: Commit**

```bash
git add src/ui/settings_window.rs src/ui/mod.rs src/window.rs src/main.rs
git commit -m "feat: add settings window with egui deferred viewport and sidebar navigation"
```

---

### Task 6: Final integration test and cleanup

**Files:**
- Modify: Various (fix any remaining issues)

- [ ] **Step 1: Run full test suite**

Run: `cargo test`
Expected: All tests pass.

- [ ] **Step 2: Run clippy**

Run: `cargo clippy`
Expected: No warnings.

- [ ] **Step 3: Run format check**

Run: `cargo fmt --check`
Expected: No formatting issues.

- [ ] **Step 4: Manual smoke test**

Run: `cargo run`
Verify:
1. `Meta+,` opens/closes settings window
2. Sidebar navigation works with grouped headers
3. General settings toggles animate and persist (check `settings.toml` after ~1s)
4. Stream settings controls work (destination, stream key, resolution, bitrate)
5. Stubbed categories show placeholder content
6. Existing layout loads without errors (no `PanelType::Settings` crash)
7. Closing and reopening works
8. Main window remains interactive while settings is open

- [ ] **Step 5: Commit any final fixes**

```bash
git add -u
git commit -m "fix: settings window integration cleanup"
```
