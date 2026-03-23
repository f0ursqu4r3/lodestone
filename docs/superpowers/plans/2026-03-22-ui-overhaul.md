# UI Overhaul Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace Lodestone's Catppuccin Mocha UI with a Pro Neutral design: new color system, toolbar, restructured panels (Sources, Scenes, Properties, Audio, Preview), and user-configurable accent color.

**Architecture:** The overhaul replaces visual constants and panel code while preserving the dockview layout engine, compositor pipeline, and GStreamer integration. New color tokens are centralized in a `theme.rs` module. The scene editor is split into three focused panels (Sources, Scenes, Properties). A fixed toolbar takes over streaming controls from the Stream Controls panel.

**Tech Stack:** Rust, wgpu, egui (layout/input), winit, GStreamer, TOML settings

**Spec:** `docs/superpowers/specs/2026-03-22-ui-overhaul-design.md`

---

## File Structure

```
src/ui/
  theme.rs              # NEW — centralized color tokens, accent color helpers
  toolbar.rs            # NEW — fixed toolbar component
  sources_panel.rs      # NEW — extracted source list from scene_editor.rs
  scenes_panel.rs       # NEW — extracted scene grid from scene_editor.rs
  properties_panel.rs   # NEW — context-sensitive property editor
  mod.rs                # MODIFY — add new modules, update PanelType, update draw_panel
  preview_panel.rs      # MODIFY — add LIVE badge, resolution overlay, new colors
  audio_mixer.rs        # MODIFY — restyle to new visual language
  stream_controls.rs    # MODIFY — remove Go Live/Record (moved to toolbar)
  settings_window.rs    # MODIFY — new colors, add accent color picker
  scene_editor.rs       # DELETE — functionality moved to sources/scenes/properties panels
  layout/
    tree.rs             # MODIFY — add PanelType variants, new default layout
    render.rs           # MODIFY — replace color/layout constants with theme tokens
    serialize.rs        # MODIFY — handle new PanelType variants in serialization
```

---

### Task 1: Create Theme Module

**Files:**
- Create: `src/ui/theme.rs`
- Modify: `src/ui/mod.rs:1-6` (add module declaration)

- [ ] **Step 1: Create `src/ui/theme.rs` with all color tokens**

```rust
//! Centralized color tokens for the Pro Neutral theme.
//!
//! All UI colors flow through this module. The accent color is user-configurable;
//! every other token is fixed.

use egui::Color32;

// ── Base Surfaces ──

pub const BG_BASE: Color32 = Color32::from_rgb(0x11, 0x11, 0x16);
pub const BG_SURFACE: Color32 = Color32::from_rgb(0x1a, 0x1a, 0x21);
pub const BG_ELEVATED: Color32 = Color32::from_rgb(0x22, 0x22, 0x2c);
pub const BG_PANEL: Color32 = Color32::from_rgb(0x16, 0x16, 0x1c);

// ── Borders ──

pub const BORDER: Color32 = Color32::from_rgb(0x2a, 0x2a, 0x34);
pub const BORDER_SUBTLE: Color32 = Color32::from_rgb(0x22, 0x22, 0x30);

// ── Text ──

pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(0xe0, 0xe0, 0xe8);
pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(0x88, 0x88, 0xa0);
pub const TEXT_MUTED: Color32 = Color32::from_rgb(0x55, 0x55, 0x68);

// ── Functional Color ──

pub const RED_LIVE: Color32 = Color32::from_rgb(0xe7, 0x4c, 0x3c);
pub const RED_GLOW: Color32 = Color32::from_rgba_premultiplied(0xe7, 0x4c, 0x3c, 0x40);
pub const GREEN_ONLINE: Color32 = Color32::from_rgb(0x2e, 0xcc, 0x71);
pub const YELLOW_WARN: Color32 = Color32::from_rgb(0xf1, 0xc4, 0x0f);

// ── VU Meter ──

pub const VU_GREEN: Color32 = Color32::from_rgb(0x2e, 0xcc, 0x71);
pub const VU_YELLOW: Color32 = Color32::from_rgb(0xf1, 0xc4, 0x0f);
pub const VU_RED: Color32 = Color32::from_rgb(0xe7, 0x4c, 0x3c);

// ── Layout Constants ──

pub const TOOLBAR_HEIGHT: f32 = 40.0;
pub const TAB_BAR_HEIGHT: f32 = 28.0;
pub const PANEL_PADDING: f32 = 8.0;
pub const ADD_BUTTON_WIDTH: f32 = 28.0;
pub const DOCK_GRIP_WIDTH: f32 = 28.0;
pub const FLOATING_HEADER_HEIGHT: f32 = 28.0;
pub const FLOATING_MIN_SIZE: egui::Vec2 = egui::Vec2::new(200.0, 100.0);

// ── Accent Color Helpers ──

/// Default accent color (neutral white-gray).
pub const DEFAULT_ACCENT: Color32 = Color32::from_rgb(0xe0, 0xe0, 0xe8);

/// Parse a hex color string like "#e0e0e8" into a Color32.
/// Returns `DEFAULT_ACCENT` on parse failure.
pub fn parse_hex_color(hex: &str) -> Color32 {
    let hex = hex.trim_start_matches('#');
    if hex.len() != 6 {
        return DEFAULT_ACCENT;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(0xe0);
    let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(0xe0);
    let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(0xe8);
    Color32::from_rgb(r, g, b)
}

/// Derive a dim version of the accent color at ~15% opacity for selection backgrounds.
pub fn accent_dim(accent: Color32) -> Color32 {
    Color32::from_rgba_premultiplied(
        (accent.r() as u16 * 38 / 255) as u8,
        (accent.g() as u16 * 38 / 255) as u8,
        (accent.b() as u16 * 38 / 255) as u8,
        38,
    )
}

/// Format a Color32 as a hex string like "#e0e0e8".
pub fn color_to_hex(c: Color32) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b())
}
```

- [ ] **Step 2: Add `theme` module to `src/ui/mod.rs`**

Add `pub mod theme;` to the module declarations at the top of `src/ui/mod.rs` (line 1).

- [ ] **Step 3: Write tests for accent color helpers**

Add tests at the bottom of `src/ui/theme.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_valid_hex() {
        let c = parse_hex_color("#ff8800");
        assert_eq!(c, Color32::from_rgb(0xff, 0x88, 0x00));
    }

    #[test]
    fn parse_hex_without_hash() {
        let c = parse_hex_color("ff8800");
        assert_eq!(c, Color32::from_rgb(0xff, 0x88, 0x00));
    }

    #[test]
    fn parse_invalid_hex_returns_default() {
        let c = parse_hex_color("nope");
        assert_eq!(c, DEFAULT_ACCENT);
    }

    #[test]
    fn accent_dim_produces_low_alpha() {
        let dim = accent_dim(Color32::from_rgb(0xff, 0xff, 0xff));
        assert_eq!(dim.a(), 38);
    }

    #[test]
    fn color_to_hex_roundtrip() {
        let hex = color_to_hex(Color32::from_rgb(0xe0, 0xe0, 0xe8));
        assert_eq!(hex, "#e0e0e8");
    }
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test theme::tests -v`
Expected: All 5 tests pass.

- [ ] **Step 5: Run clippy**

Run: `cargo clippy`
Expected: No warnings.

- [ ] **Step 6: Commit**

```bash
git add src/ui/theme.rs src/ui/mod.rs
git commit -m "feat(ui): add centralized theme module with Pro Neutral color tokens"
```

---

### Task 2: Add New State Fields

**Files:**
- Modify: `src/state.rs:31-50` (AppState struct)
- Modify: `src/state.rs:52-75` (AppState Default impl)
- Modify: `src/settings.rs:149-163` (AppearanceSettings struct)

- [ ] **Step 1: Add `selected_source_id` to AppState**

In `src/state.rs`, add to the AppState struct (after `active_scene_id` at line 34):

```rust
pub selected_source_id: Option<SourceId>,
```

And in the `Default` impl (after line 57), add:

```rust
selected_source_id: None,
```

- [ ] **Step 2: Update `accent_color` default in AppearanceSettings**

The `accent_color` field already exists in `src/settings.rs:150`. Change only the default value in the `Default` impl (line 158) from `"#7c6cf0"` (purple) to `"#e0e0e8"` (neutral):

```rust
accent_color: "#e0e0e8".to_string(),
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo build`
Expected: Compiles with no errors.

- [ ] **Step 4: Commit**

```bash
git add src/state.rs src/settings.rs
git commit -m "feat(state): add selected_source_id and accent_color settings"
```

---

### Task 3: Add New PanelType Variants and Default Layout

**Files:**
- Modify: `src/ui/layout/tree.rs:14-30` (PanelType enum + display_name)
- Modify: `src/ui/layout/tree.rs:368-440` (default_layout)
- Modify: `src/ui/layout/serialize.rs` (handle new variants)

- [ ] **Step 1: Add new PanelType variants**

In `src/ui/layout/tree.rs`, update the `PanelType` enum to include the new panel types:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum PanelType {
    Preview,
    SceneEditor,  // kept for backward compat with saved layouts
    AudioMixer,
    StreamControls,
    Sources,
    Scenes,
    Properties,
}
```

Update `display_name()`:

```rust
pub fn display_name(&self) -> &'static str {
    match self {
        Self::Preview => "Preview",
        Self::SceneEditor => "Scene Editor",
        Self::AudioMixer => "Audio",
        Self::StreamControls => "Stream Controls",
        Self::Sources => "Sources",
        Self::Scenes => "Scenes",
        Self::Properties => "Properties",
    }
}
```

- [ ] **Step 2: Update default_layout()**

Replace the body of `default_layout()` with the new 3-column arrangement. **Important:** Follow the existing pattern — use `Group::new()` (which calls `GroupId::next()`) and `layout.alloc_node_id()` instead of hardcoded IDs. The `root` field is `NodeId`, not `Option<NodeId>`.

```rust
pub fn default_layout() -> Self {
    let mut layout = Self {
        nodes: HashMap::new(),
        root: NodeId(0),
        next_node_id: 0,
        groups: HashMap::new(),
        floating: Vec::new(),
        drag: None,
    };

    // Left column: Sources (top) + Scenes (bottom)
    let sources_group = Group::new(PanelType::Sources);
    let sources_gid = sources_group.id;
    layout.groups.insert(sources_gid, sources_group);

    let scenes_group = Group::new(PanelType::Scenes);
    let scenes_gid = scenes_group.id;
    layout.groups.insert(scenes_gid, scenes_group);

    // Center: Preview
    let preview_group = Group::new(PanelType::Preview);
    let preview_gid = preview_group.id;
    layout.groups.insert(preview_gid, preview_group);

    // Right column: Properties (top) + Audio (bottom)
    let properties_group = Group::new(PanelType::Properties);
    let properties_gid = properties_group.id;
    layout.groups.insert(properties_gid, properties_group);

    let audio_group = Group::new(PanelType::AudioMixer);
    let audio_gid = audio_group.id;
    layout.groups.insert(audio_gid, audio_group);

    // Leaf nodes
    let sources_node = layout.alloc_node_id();
    layout.nodes.insert(sources_node, SplitNode::Leaf { group_id: sources_gid });

    let scenes_node = layout.alloc_node_id();
    layout.nodes.insert(scenes_node, SplitNode::Leaf { group_id: scenes_gid });

    let preview_node = layout.alloc_node_id();
    layout.nodes.insert(preview_node, SplitNode::Leaf { group_id: preview_gid });

    let properties_node = layout.alloc_node_id();
    layout.nodes.insert(properties_node, SplitNode::Leaf { group_id: properties_gid });

    let audio_node = layout.alloc_node_id();
    layout.nodes.insert(audio_node, SplitNode::Leaf { group_id: audio_gid });

    // Left column: Sources (60%) / Scenes (40%)
    let left_split = layout.alloc_node_id();
    layout.nodes.insert(left_split, SplitNode::Split {
        direction: SplitDirection::Horizontal,
        ratio: 0.6,
        first: sources_node,
        second: scenes_node,
    });

    // Right column: Properties (60%) / Audio (40%)
    let right_split = layout.alloc_node_id();
    layout.nodes.insert(right_split, SplitNode::Split {
        direction: SplitDirection::Horizontal,
        ratio: 0.6,
        first: properties_node,
        second: audio_node,
    });

    // Center+Right: Preview (75%) | Right column (25%)
    let center_right = layout.alloc_node_id();
    layout.nodes.insert(center_right, SplitNode::Split {
        direction: SplitDirection::Vertical,
        ratio: 0.75,
        first: preview_node,
        second: right_split,
    });

    // Root: Left (20%) | Center+Right (80%)
    let root = layout.alloc_node_id();
    layout.nodes.insert(root, SplitNode::Split {
        direction: SplitDirection::Vertical,
        ratio: 0.2,
        first: left_split,
        second: center_right,
    });
    layout.root = root;

    layout
}
```

- [ ] **Step 3: Update serialization for new PanelType variants**

In `src/ui/layout/serialize.rs`, the `PanelType` enum derives `serde::Serialize` and `serde::Deserialize`, so the new variants are handled automatically. Verify that the deserialization tolerates unknown types by checking the existing skip logic. No code changes needed — just verify.

- [ ] **Step 4: Update `draw_panel` in `src/ui/mod.rs` to handle new types**

Keep the existing signature: `draw_panel(panel_type: PanelType, ui: &mut egui::Ui, state: &mut AppState, id: PanelId)`. All new panels receive `id` for consistency, even if they don't use it yet.

```rust
pub fn draw_panel(panel_type: PanelType, ui: &mut egui::Ui, state: &mut AppState, id: PanelId) {
    match panel_type {
        PanelType::Preview => preview_panel::draw(ui, state, id),
        PanelType::SceneEditor => {
            // Legacy: render sources panel as fallback for saved layouts
            sources_panel::draw(ui, state, id);
        }
        PanelType::AudioMixer => audio_mixer::draw(ui, state, id),
        PanelType::StreamControls => stream_controls::draw(ui, state, id),
        PanelType::Sources => sources_panel::draw(ui, state, id),
        PanelType::Scenes => scenes_panel::draw(ui, state, id),
        PanelType::Properties => properties_panel::draw(ui, state, id),
    }
}
```

- [ ] **Step 5: Verify it compiles**

Run: `cargo build`
Expected: Compile errors for missing modules (sources_panel, scenes_panel, properties_panel). This is expected — they'll be created in subsequent tasks. Create stubs if needed to unblock.

- [ ] **Step 6: Create stub modules for new panels**

Create minimal stubs so the project compiles:

`src/ui/sources_panel.rs`:
```rust
use crate::state::AppState;
use crate::ui::layout::tree::PanelId;

pub fn draw(ui: &mut egui::Ui, _state: &mut AppState, _id: PanelId) {
    ui.label("Sources (coming soon)");
}
```

`src/ui/scenes_panel.rs`:
```rust
use crate::state::AppState;
use crate::ui::layout::tree::PanelId;

pub fn draw(ui: &mut egui::Ui, _state: &mut AppState, _id: PanelId) {
    ui.label("Scenes (coming soon)");
}
```

`src/ui/properties_panel.rs`:
```rust
use crate::state::AppState;
use crate::ui::layout::tree::PanelId;

pub fn draw(ui: &mut egui::Ui, _state: &mut AppState, _id: PanelId) {
    ui.label("Properties (coming soon)");
}
```

Add module declarations to `src/ui/mod.rs`:
```rust
pub mod sources_panel;
pub mod scenes_panel;
pub mod properties_panel;
```

- [ ] **Step 7: Update default layout tests**

In `src/ui/layout/tree.rs`, update the test `default_layout_has_3_groups_4_panels` (line 849) to match the new layout:

```rust
#[test]
fn default_layout_has_5_groups_5_panels() {
    let layout = DockLayout::default_layout();
    assert_eq!(layout.groups.len(), 5);
    let all_panels = layout.collect_all_panels();
    assert_eq!(all_panels.len(), 5);
    let types: Vec<PanelType> = all_panels.iter().map(|(_, t)| *t).collect();
    assert!(types.contains(&PanelType::Sources));
    assert!(types.contains(&PanelType::Scenes));
    assert!(types.contains(&PanelType::Preview));
    assert!(types.contains(&PanelType::Properties));
    assert!(types.contains(&PanelType::AudioMixer));
}
```

Also update `default_layout_group_rects` (line 862) to assert 5 groups:

```rust
#[test]
fn default_layout_group_rects() {
    let layout = DockLayout::default_layout();
    let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1000.0, 600.0));
    let groups = layout.collect_groups_with_rects(rect);
    assert_eq!(groups.len(), 5);
}
```

- [ ] **Step 8: Verify it compiles and run tests**

Run: `cargo build && cargo test`
Expected: Compiles, all tests pass.

- [ ] **Step 9: Commit**

```bash
git add src/ui/layout/tree.rs src/ui/layout/serialize.rs src/ui/mod.rs \
  src/ui/sources_panel.rs src/ui/scenes_panel.rs src/ui/properties_panel.rs
git commit -m "feat(layout): add new panel types, 3-column default layout, panel stubs"
```

---

### Task 4: Replace Color Constants in Layout Renderer

**Files:**
- Modify: `src/ui/layout/render.rs:13-29` (replace color/layout constants)

- [ ] **Step 1: Replace color constants with theme imports**

At the top of `src/ui/layout/render.rs`, replace the existing color and layout constants (lines 13-29) with:

```rust
use crate::ui::theme::{
    BG_BASE, BG_SURFACE, BG_ELEVATED, BG_PANEL,
    BORDER, TEXT_PRIMARY, TEXT_SECONDARY, TEXT_MUTED,
    TAB_BAR_HEIGHT, PANEL_PADDING, ADD_BUTTON_WIDTH,
    DOCK_GRIP_WIDTH, FLOATING_HEADER_HEIGHT, FLOATING_MIN_SIZE,
    DEFAULT_ACCENT,
};
```

- [ ] **Step 2: Update all color references throughout render.rs**

Replace old constant names with new theme names throughout the file:

| Old | New |
|-----|-----|
| `TAB_BAR_BG` | `BG_SURFACE` |
| `TAB_ACTIVE_BG` | `BG_ELEVATED` |
| `TAB_HOVER_BG` | `BG_ELEVATED` |
| `TAB_ACCENT` | `DEFAULT_ACCENT` (or read from settings) |
| `CONTENT_BG` | `BG_PANEL` |
| `TEXT_DIM` | `TEXT_SECONDARY` |
| `TEXT_BRIGHT` | `TEXT_PRIMARY` |
| `DIVIDER_COLOR` | `BORDER` |
| `FLOATING_BORDER` | `BORDER` |
| `DROP_ZONE_TINT` | Compute from accent: `Color32::from_rgba_premultiplied(accent.r(), accent.g(), accent.b(), 38)` |

Note: `PANEL_PADDING` and `TAB_BAR_HEIGHT` keep their names — just sourced from theme now.

- [ ] **Step 3: Verify it compiles**

Run: `cargo build`
Expected: No errors.

- [ ] **Step 4: Commit**

```bash
git add src/ui/layout/render.rs
git commit -m "refactor(layout): replace Catppuccin colors with Pro Neutral theme tokens"
```

---

### Task 5: Build the Toolbar

**Files:**
- Create: `src/ui/toolbar.rs`
- Modify: `src/ui/mod.rs` (add module declaration)
- Modify: `src/main.rs` or wherever the top-level UI frame is rendered (to insert toolbar above the dock layout)

- [ ] **Step 1: Identify where the top-level UI is rendered**

Read `src/main.rs` and `src/window.rs` to find where `egui::CentralPanel` or the dock layout's `render()` is called. The toolbar needs to be rendered as a `TopBottomPanel::top()` before the dock layout's `CentralPanel`.

- [ ] **Step 2: Create `src/ui/toolbar.rs`**

```rust
//! Fixed toolbar rendered above the dock layout.
//!
//! Contains: app logo, scene quick-switcher, stream stats, Go Live / Record
//! buttons, and settings gear.

use egui::{self, Color32, RichText, Sense, Vec2};

use crate::state::{AppState, StreamStatus, RecordingStatus};
use crate::ui::theme::{
    BG_SURFACE, BORDER, BG_BASE, BG_ELEVATED,
    TEXT_PRIMARY, TEXT_SECONDARY, TEXT_MUTED,
    RED_LIVE, GREEN_ONLINE, TOOLBAR_HEIGHT,
};

/// Draw the fixed toolbar at the top of the window.
/// Returns true if the settings button was clicked.
pub fn draw(ctx: &egui::Context, state: &mut AppState) -> bool {
    let mut open_settings = false;

    egui::TopBottomPanel::top("toolbar")
        .exact_height(TOOLBAR_HEIGHT)
        .frame(egui::Frame::new()
            .fill(BG_SURFACE)
            .stroke(egui::Stroke::new(1.0, BORDER))
            .inner_margin(egui::Margin::symmetric(12.0, 0.0)))
        .show(ctx, |ui| {
            ui.horizontal_centered(|ui| {
                // ── Logo ──
                ui.label(
                    RichText::new("Lodestone")
                        .color(TEXT_PRIMARY)
                        .size(13.0)
                        .strong(),
                );
                ui.add_space(8.0);
                toolbar_divider(ui);
                ui.add_space(8.0);

                // ── Scene Quick-Switcher ──
                draw_scene_switcher(ui, state);

                // ── Spacer ──
                ui.add_space(ui.available_width()
                    - stats_and_actions_width(state));

                // ── Stream Stats (only when live) ──
                if let StreamStatus::Live { uptime_secs, bitrate_kbps, dropped_frames } = &state.stream_status {
                    let total_secs = *uptime_secs as u64;
                    let hours = total_secs / 3600;
                    let mins = (total_secs % 3600) / 60;
                    let secs = total_secs % 60;

                    // Green dot
                    let (dot_rect, _) = ui.allocate_exact_size(Vec2::new(5.0, 5.0), Sense::hover());
                    ui.painter().circle_filled(dot_rect.center(), 2.5, GREEN_ONLINE);
                    ui.add_space(4.0);

                    ui.label(
                        RichText::new(format!("{hours:02}:{mins:02}:{secs:02}"))
                            .color(TEXT_MUTED).size(10.0).family(egui::FontFamily::Monospace),
                    );
                    ui.add_space(12.0);
                    ui.label(
                        RichText::new(format!("{:.0} kbps", bitrate_kbps))
                            .color(TEXT_MUTED).size(10.0),
                    );
                    ui.add_space(12.0);
                    ui.label(
                        RichText::new(format!("{dropped_frames} dropped"))
                            .color(TEXT_MUTED).size(10.0),
                    );
                    ui.add_space(12.0);
                    toolbar_divider(ui);
                    ui.add_space(8.0);
                }

                // ── Go Live / Stop Button ──
                let is_live = matches!(state.stream_status, StreamStatus::Live { .. });
                let live_btn = if is_live {
                    egui::Button::new(
                        RichText::new("● LIVE").color(Color32::WHITE).size(11.0).strong(),
                    )
                    .fill(RED_LIVE)
                    .stroke(egui::Stroke::NONE)
                    .corner_radius(4.0)
                } else {
                    egui::Button::new(
                        RichText::new("Go Live").color(TEXT_SECONDARY).size(11.0).strong(),
                    )
                    .fill(Color32::TRANSPARENT)
                    .stroke(egui::Stroke::new(1.0, BORDER))
                    .corner_radius(4.0)
                };
                if ui.add(live_btn).clicked() {
                    if let Some(tx) = &state.command_tx {
                        if is_live {
                            let _ = tx.try_send(crate::gstreamer::GstCommand::StopStream);
                        } else {
                            // StartStream requires a StreamConfig with destination + stream_key.
                            // For the toolbar, default to Twitch. The stream controls panel
                            // provides full destination selection — the toolbar is a quick action.
                            let config = crate::gstreamer::StreamConfig {
                                destination: crate::gstreamer::StreamDestination::Twitch,
                                stream_key: String::new(), // read from egui memory or settings
                            };
                            let _ = tx.try_send(crate::gstreamer::GstCommand::StartStream(config));
                        }
                    }
                }
                ui.add_space(6.0);

                // ── Record Button ──
                let is_recording = matches!(state.recording_status, RecordingStatus::Recording { .. });
                let rec_btn = if is_recording {
                    egui::Button::new(
                        RichText::new("● REC").color(Color32::WHITE).size(11.0).strong(),
                    )
                    .fill(RED_LIVE)
                    .stroke(egui::Stroke::NONE)
                    .corner_radius(4.0)
                } else {
                    egui::Button::new(
                        RichText::new("REC").color(TEXT_SECONDARY).size(11.0),
                    )
                    .fill(Color32::TRANSPARENT)
                    .stroke(egui::Stroke::new(1.0, BORDER))
                    .corner_radius(4.0)
                };
                if ui.add(rec_btn).clicked() {
                    if let Some(tx) = &state.command_tx {
                        if is_recording {
                            let _ = tx.try_send(crate::gstreamer::GstCommand::StopRecording);
                        } else {
                            // StartRecording requires { path: PathBuf, format: RecordingFormat }.
                            // Match the existing pattern from stream_controls.rs:
                            let video_dir = dirs::video_dir()
                                .or_else(dirs::home_dir)
                                .unwrap_or_else(|| std::path::PathBuf::from("."));
                            let timestamp = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .map(|d| d.as_secs())
                                .unwrap_or(0);
                            let path = video_dir.join(format!("lodestone-{timestamp}.mkv"));
                            let _ = tx.try_send(crate::gstreamer::GstCommand::StartRecording {
                                path,
                                format: crate::gstreamer::RecordingFormat::Mkv,
                            });
                        }
                    }
                }

                ui.add_space(8.0);
                toolbar_divider(ui);
                ui.add_space(8.0);

                // ── Settings Gear ──
                let gear = ui.add(
                    egui::Button::new(
                        RichText::new("⚙").color(TEXT_MUTED).size(14.0),
                    )
                    .fill(Color32::TRANSPARENT)
                    .stroke(egui::Stroke::NONE)
                    .corner_radius(4.0)
                    .min_size(Vec2::new(28.0, 28.0)),
                );
                if gear.clicked() {
                    open_settings = true;
                }
            });
        });

    open_settings
}

fn draw_scene_switcher(ui: &mut egui::Ui, state: &mut AppState) {
    let frame = egui::Frame::new()
        .fill(BG_BASE)
        .stroke(egui::Stroke::new(1.0, BORDER))
        .corner_radius(4.0)
        .inner_margin(egui::Margin::symmetric(2.0, 2.0));

    frame.show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            let active_id = state.active_scene_id;
            for scene in &state.scenes {
                let is_active = active_id == Some(scene.id);
                let text_color = if is_active { TEXT_PRIMARY } else { TEXT_SECONDARY };
                let bg = if is_active { BG_ELEVATED } else { Color32::TRANSPARENT };
                let btn = egui::Button::new(
                    RichText::new(&scene.name).color(text_color).size(11.0),
                )
                .fill(bg)
                .stroke(egui::Stroke::NONE)
                .corner_radius(3.0);
                if ui.add(btn).clicked() {
                    state.active_scene_id = Some(scene.id);
                }
            }
            // + button
            let plus = egui::Button::new(
                RichText::new("+").color(TEXT_MUTED).size(13.0),
            )
            .fill(Color32::TRANSPARENT)
            .stroke(egui::Stroke::NONE)
            .corner_radius(3.0);
            if ui.add(plus).clicked() {
                let id = crate::scene::SceneId(state.next_scene_id);
                state.next_scene_id += 1;
                state.scenes.push(crate::scene::Scene {
                    id,
                    name: format!("Scene {}", state.scenes.len() + 1),
                    source_ids: Vec::new(),
                });
                state.active_scene_id = Some(id);
                state.scenes_dirty = true;
            }
        });
    });
}

fn toolbar_divider(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(1.0, 20.0), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, BORDER);
}

/// Estimate the width needed for stats + action buttons so we can right-align.
fn stats_and_actions_width(state: &AppState) -> f32 {
    let stats_w = if matches!(state.stream_status, StreamStatus::Live { .. }) {
        200.0  // approximate width of stats section
    } else {
        0.0
    };
    stats_w + 180.0  // buttons + dividers + spacing
}
```

- [ ] **Step 3: Add `toolbar` module to `src/ui/mod.rs`**

Add `pub mod toolbar;` to module declarations.

- [ ] **Step 4: Wire toolbar into the main render loop**

Find where the dock layout is rendered (likely in `src/window.rs` or `src/main.rs`) and insert the toolbar call before it. The toolbar should be rendered as a `TopBottomPanel::top()` so it reserves space at the top, and the dock layout fills the remaining `CentralPanel`.

This step requires reading the actual render call site — the implementer should:
1. Find the `egui_ctx.run()` or equivalent frame closure
2. Add `let open_settings = crate::ui::toolbar::draw(&egui_ctx, &mut state);` before the dock layout render
3. If `open_settings` is true, trigger the settings window open logic

- [ ] **Step 5: Verify it compiles**

Run: `cargo build`
Expected: Compiles. May need adjustments based on exact GstCommand variant names and StreamStatus field access patterns.

- [ ] **Step 6: Commit**

```bash
git add src/ui/toolbar.rs src/ui/mod.rs
git commit -m "feat(ui): add fixed toolbar with scene switcher, live controls, and stats"
```

---

### Task 6: Implement Sources Panel

**Files:**
- Modify: `src/ui/sources_panel.rs` (replace stub)
- Reference: `src/ui/scene_editor.rs:171-412` (source management code to extract)

- [ ] **Step 1: Implement the full sources panel**

Replace the stub in `src/ui/sources_panel.rs` with the full implementation. The panel displays the source list for the active scene with:
- Source items: icon, name, visibility toggle
- Selection: clicking a source sets `state.selected_source_id`
- Reorder: up/down buttons (drag reorder deferred)
- Add/remove: + button in header, delete via context menu

Extract the source list rendering logic from `src/ui/scene_editor.rs` lines 171-412 and adapt it:
- Remove scene management code (that goes to scenes_panel)
- Replace hardcoded colors with theme tokens
- Add `state.selected_source_id` tracking on click
- Style source items per spec: icon (16x16, BG_ELEVATED), name (TEXT_PRIMARY, 11px), visibility eye (TEXT_MUTED)
- Selected item gets `accent_dim` background
- Hidden sources render at 40% opacity

- [ ] **Step 2: Verify it compiles**

Run: `cargo build`

- [ ] **Step 3: Commit**

```bash
git add src/ui/sources_panel.rs
git commit -m "feat(ui): implement sources panel with selection and visibility"
```

---

### Task 7: Implement Scenes Panel

**Files:**
- Modify: `src/ui/scenes_panel.rs` (replace stub)
- Reference: `src/ui/scene_editor.rs:74-167` (scene management code)

- [ ] **Step 1: Implement the full scenes panel**

Replace the stub in `src/ui/scenes_panel.rs`. The panel displays a 2-column grid of scene thumbnails:
- Each scene: thumbnail (16:9, BG_ELEVATED, 1px BORDER, 3px corner radius), label below (9px TEXT_SECONDARY)
- Active scene: TEXT_PRIMARY border, TEXT_PRIMARY label
- Hover: border transitions to TEXT_MUTED
- Add scene: dashed border thumbnail with "+" icon
- Click switches active scene (also updates toolbar switcher via `state.active_scene_id`)
- Right-click context menu: rename, duplicate, delete

Extract scene list logic from `src/ui/scene_editor.rs` lines 74-167 and restyle.

- [ ] **Step 2: Verify it compiles**

Run: `cargo build`

- [ ] **Step 3: Commit**

```bash
git add src/ui/scenes_panel.rs
git commit -m "feat(ui): implement scenes panel with thumbnail grid"
```

---

### Task 8: Implement Properties Panel

**Files:**
- Modify: `src/ui/properties_panel.rs` (replace stub)

- [ ] **Step 1: Implement the full properties panel**

Replace the stub. The panel is context-sensitive — it reads `state.selected_source_id` and shows properties for that source:

**Empty state:** Centered TEXT_MUTED label "Select a source to view properties".

**When a source is selected:**
- **Transform section:** "TRANSFORM" label (9px uppercase TEXT_MUTED). X/Y/W/H as paired `DragValue` inputs. Labels 10px TEXT_MUTED right-aligned (24px wide). Inputs: 22px tall, BG_BASE background, 1px BORDER, 3px corner radius.
- **Opacity section:** "OPACITY" label. Horizontal slider (0.0-1.0). Percentage readout right-aligned.
- **Source section:** "SOURCE" label. Source-type-specific: monitor selector for display capture (reuse from scene_editor.rs), device selector for webcam.

All property changes write back to the source in `state.sources` and mark `state.scenes_dirty = true`.

- [ ] **Step 2: Verify it compiles**

Run: `cargo build`

- [ ] **Step 3: Commit**

```bash
git add src/ui/properties_panel.rs
git commit -m "feat(ui): implement context-sensitive properties panel"
```

---

### Task 9: Restyle Preview Panel

**Files:**
- Modify: `src/ui/preview_panel.rs`

- [ ] **Step 1: Add LIVE badge and resolution overlay**

Update the `draw()` function in `preview_panel.rs`:

1. After the preview viewport is rendered, add overlay elements:
   - **LIVE badge** (top-left): Only when `state.stream_status` is `Live`. Red rectangle (RED_LIVE fill), white bold "LIVE" text at 9px, 3px corner radius.
   - **Resolution overlay** (bottom-right): Always visible. Semi-transparent black background (#000000 at 50%), TEXT_MUTED text showing "1920×1080 · 60fps" (read from settings).

2. Replace any hardcoded background colors with `BG_BASE`.

- [ ] **Step 2: Verify it compiles**

Run: `cargo build`

- [ ] **Step 3: Commit**

```bash
git add src/ui/preview_panel.rs
git commit -m "feat(ui): add LIVE badge and resolution overlay to preview"
```

---

### Task 10: Restyle Audio Mixer

**Files:**
- Modify: `src/ui/audio_mixer.rs`

- [ ] **Step 1: Replace colors with theme tokens**

Replace all hardcoded Color32 values in `audio_mixer.rs` with theme tokens:
- VU meter colors: `VU_GREEN`, `VU_YELLOW`, `VU_RED`
- Background: `BG_BASE` for meter tracks, `BG_PANEL` for panel
- Labels: `TEXT_MUTED` (9px uppercase)
- dB readout: `TEXT_MUTED`, monospace
- Mute button: `BORDER` border, `TEXT_MUTED` text. Muted state: `RED_LIVE` fill, white text

- [ ] **Step 2: Update VU meter rendering**

Restyle VU meters to match spec:
- 8px wide, 80px tall track
- BG_BASE + 1px BORDER, 4px corner radius
- Fill from bottom with gradient: green only (<-18dB), green→yellow (-18 to -6dB), green→yellow→red (>-6dB)
- Add box-shadow glow at peak (>-6dB): `VU_RED` at 30% opacity

- [ ] **Step 3: Update channel strip layout**

Restyle to vertical column layout per channel:
1. Label (9px uppercase TEXT_MUTED, centered)
2. VU meter
3. dB readout (9px TEXT_MUTED, tabular-nums)
4. Mute button (20x16px, 1px BORDER, "M" in 8px bold)

Flex row layout with 8px gap between channels.

- [ ] **Step 4: Verify it compiles**

Run: `cargo build`

- [ ] **Step 5: Commit**

```bash
git add src/ui/audio_mixer.rs
git commit -m "refactor(ui): restyle audio mixer with Pro Neutral theme"
```

---

### Task 11: Restyle Settings Window

**Files:**
- Modify: `src/ui/settings_window.rs:17-24` (color constants)
- Modify: `src/ui/settings_window.rs:725-786` (appearance section)

- [ ] **Step 1: Replace color constants**

Replace the Catppuccin constants (lines 17-24) with theme imports:

```rust
use crate::ui::theme::{
    BG_BASE, BG_SURFACE, BG_ELEVATED, BORDER,
    TEXT_PRIMARY, TEXT_SECONDARY, TEXT_MUTED,
    DEFAULT_ACCENT, parse_hex_color, color_to_hex,
};
```

Map old constants:
| Old | New |
|-----|-----|
| `ACCENT` | Read from `state.settings.appearance.accent_color` via `parse_hex_color()` |
| `TEXT` | `TEXT_PRIMARY` |
| `SUBTEXT` | `TEXT_SECONDARY` |
| `MUTED` | `TEXT_MUTED` |
| `SURFACE` | `BG_ELEVATED` |
| `SECTION_HEADER` | `TEXT_MUTED` |
| `SIDEBAR_BG` | `BG_BASE` |
| `CONTENT_BG` | `BG_SURFACE` |

- [ ] **Step 2: Add accent color picker to Appearance section**

In the `draw_appearance()` function, add after the existing controls:

```rust
// ── Accent Color ──
ui.add_space(16.0);
ui.label(RichText::new("Accent Color").color(TEXT_PRIMARY).size(13.0));
ui.add_space(8.0);

ui.horizontal(|ui| {
    // Color swatch preview
    let accent = parse_hex_color(&state.settings.appearance.accent_color);
    let (swatch_rect, _) = ui.allocate_exact_size(egui::Vec2::new(24.0, 24.0), egui::Sense::hover());
    ui.painter().rect_filled(swatch_rect, 4.0, accent);
    ui.painter().rect_stroke(swatch_rect, 4.0, egui::Stroke::new(1.0, BORDER));

    ui.add_space(8.0);

    // Hex input
    let mut hex = state.settings.appearance.accent_color.clone();
    let response = ui.add(
        egui::TextEdit::singleline(&mut hex)
            .desired_width(80.0)
            .font(egui::TextStyle::Monospace),
    );
    if response.changed() {
        state.settings.appearance.accent_color = hex;
        state.settings_dirty = true;
    }

    ui.add_space(8.0);

    // Reset to default
    if ui.button("Reset").clicked() {
        state.settings.appearance.accent_color = color_to_hex(DEFAULT_ACCENT);
        state.settings_dirty = true;
    }
});
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo build`

- [ ] **Step 4: Commit**

```bash
git add src/ui/settings_window.rs
git commit -m "refactor(ui): restyle settings window, add accent color picker"
```

---

### Task 12: Update Stream Controls Panel

**Files:**
- Modify: `src/ui/stream_controls.rs`

- [ ] **Step 1: Remove Go Live and Record buttons**

Remove the Go Live and Record button rendering from `stream_controls.rs` — these now live in the toolbar. Keep:
- Destination selector (Twitch, YouTube, Custom RTMP)
- Custom RTMP URL input
- Stream key input
- Recording path display

Restyle all remaining controls with theme tokens.

- [ ] **Step 2: Verify it compiles**

Run: `cargo build`

- [ ] **Step 3: Commit**

```bash
git add src/ui/stream_controls.rs
git commit -m "refactor(ui): slim down stream controls, move live/rec to toolbar"
```

---

### Task 13: Delete Scene Editor, Clean Up

**Files:**
- Delete: `src/ui/scene_editor.rs`
- Modify: `src/ui/mod.rs` (remove `scene_editor` module)

- [ ] **Step 1: Remove scene_editor module**

Delete `src/ui/scene_editor.rs`. Remove `pub mod scene_editor;` from `src/ui/mod.rs`.

Update the `draw_panel` match arm for `PanelType::SceneEditor` — it should now render the sources panel as a fallback for saved layouts that reference the old type:

```rust
PanelType::SceneEditor => sources_panel::draw(ui, state, panel_id),
```

- [ ] **Step 2: Move any shared helper functions**

If `scene_editor.rs` had helper functions used by the new panels (like `apply_scene_diff()` or `send_capture_for_scene()`), ensure they've been moved to the appropriate new location (likely `sources_panel.rs` or a shared helper).

- [ ] **Step 3: Verify it compiles and all tests pass**

Run: `cargo build && cargo test`
Expected: No errors, all tests pass.

- [ ] **Step 4: Run clippy**

Run: `cargo clippy`
Expected: No warnings.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "refactor(ui): remove scene_editor.rs, functionality split into sources/scenes/properties"
```

---

### Task 14: Final Integration and Visual Polish

**Files:**
- All UI files for final adjustments

- [ ] **Step 1: Run the application**

Run: `cargo run`
Verify: App launches with the new toolbar, 3-column layout, and Pro Neutral colors. All panels render without crashes.

- [ ] **Step 2: Visual verification checklist**

Verify each element matches the spec:
- [ ] Toolbar: logo, scene switcher, stats area, Go Live/Record buttons, settings gear
- [ ] Sources panel: source list with icons, names, visibility toggles, selection highlighting
- [ ] Scenes panel: 2-column thumbnail grid, active scene indicator, add button
- [ ] Properties panel: empty state message, transform/opacity/source fields when source selected
- [ ] Audio mixer: VU meters with correct colors, mute buttons, labels
- [ ] Preview: LIVE badge when streaming, resolution overlay
- [ ] Panel chrome: 28px headers, correct tab styling, scrollbars
- [ ] Dividers: 3px hit area, hover color change
- [ ] Colors: all surfaces, text, borders match the spec hex values

- [ ] **Step 3: Fix any visual discrepancies**

Address any issues found during visual verification.

- [ ] **Step 4: Run full test suite**

Run: `cargo test`
Expected: All tests pass.

- [ ] **Step 5: Run clippy and fmt**

Run: `cargo clippy && cargo fmt --check`
Expected: Clean.

- [ ] **Step 6: Final commit**

```bash
git add -A
git commit -m "feat(ui): complete Pro Neutral UI overhaul"
```
