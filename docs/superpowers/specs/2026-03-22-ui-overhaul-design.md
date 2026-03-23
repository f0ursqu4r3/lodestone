# UI Overhaul Design Spec

Full redesign of Lodestone's user interface: layout, panel structure, visual language, and interaction model.

## Design Decisions

- **Aesthetic:** Pro Neutral — clean grays, sharp borders, dense and professional. Inspired by DaVinci Resolve, Ableton, Blender.
- **Layout:** Balanced workspace as the default. Preview is prominent but shares space with functional panels. Users have full freedom to rearrange, resize, split, dock, and float panels via the dockview system.
- **Accent color:** Ships with a neutral default (white/light gray accents against dark grays). User-configurable in settings — the UI itself is deliberately colorless so functional color (live indicators, audio levels, errors) stands out.
- **Flair level:** Tasteful accents. Smooth transitions on state changes, hover animations, live pulse on the Go Live button, VU meter glow at peak, panel drop shadows, smooth drag ghosts. Nothing decorative — every animation communicates state.

## Color System

All colors are defined as named tokens. The palette is intentionally narrow.

### Base Surfaces

| Token | Hex | Usage |
|-------|-----|-------|
| `bg-base` | `#111116` | App background, deepest layer |
| `bg-surface` | `#1a1a21` | Toolbar, panel headers, elevated chrome |
| `bg-elevated` | `#22222c` | Hover states, active items, inputs |
| `bg-panel` | `#16161c` | Panel content background |

### Borders

| Token | Hex | Usage |
|-------|-----|-------|
| `border` | `#2a2a34` | Primary borders — panels, dividers, inputs |
| `border-subtle` | `#222230` | Inner separators, secondary lines |

### Text

| Token | Hex | Usage |
|-------|-----|-------|
| `text-primary` | `#e0e0e8` | Labels, values, active tab text |
| `text-secondary` | `#8888a0` | Inactive tabs, descriptions |
| `text-muted` | `#555568` | Disabled items, hints, timestamps |

### Functional Color

| Token | Hex | Usage |
|-------|-----|-------|
| `red-live` | `#e74c3c` | Live indicator, Go Live button |
| `red-glow` | `#e74c3c40` | Live button pulse shadow |
| `green-online` | `#2ecc71` | Connected indicator |
| `yellow-warn` | `#f1c40f` | Warning states |
| `vu-green` | `#2ecc71` | VU meter — signal below -18 dB |
| `vu-yellow` | `#f1c40f` | VU meter — signal -18 to -6 dB |
| `vu-red` | `#e74c3c` | VU meter — signal above -6 dB |
| `accent-dim` | accent @ 15% opacity | Source selection background, derived from accent color |

### Accent

The accent color defaults to `text-primary` (`#e0e0e8`). Active tab underlines, selection highlights, and focused inputs all use this token. When the user configures a custom accent, it replaces this single token — the entire UI updates consistently.

## Typography

- **Font:** System font stack (`-apple-system`, `BlinkMacSystemFont`, `Inter`, `Segoe UI`). No custom fonts shipped.
- **Base size:** 11px for panel content.
- **Panel tabs:** 11px, `text-secondary` inactive / `text-primary` active.
- **Labels (uppercase):** 9px, `text-muted`, `letter-spacing: 0.5px`, uppercase. Used for section headers in Properties and Audio.
- **Values/inputs:** 10px, `text-primary`, `tabular-nums` for numeric alignment.
- **Toolbar logo:** 13px, weight 700, `text-primary`.
- **Stats:** 10px, `text-muted`, `tabular-nums`.
- User-configurable font size (8-24px) scales the base; all other sizes scale proportionally.

## Layout Architecture

### Toolbar (Fixed)

A single 40px-tall bar pinned to the top of the window. Not a panel — it cannot be rearranged, hidden, or docked. Contains:

1. **App logo** — "Lodestone", left-aligned, weight 700.
2. **Scene quick-switcher** — Horizontal pill group showing all scenes. Click to switch. Active scene gets `bg-elevated` + `text-primary`. A `+` button at the end creates a new scene. This provides instant scene switching without needing the Scenes panel open.
3. **Spacer** — Pushes remaining items right.
4. **Stream stats** — Visible only when live. Shows: green dot + uptime (HH:MM:SS), bitrate (kbps), dropped frames. `text-muted`, `tabular-nums`.
5. **Go Live button** — `red-live` background with `live-pulse` animation (2s ease-in-out shadow pulse) when live. White text "LIVE" with dot prefix. When offline: outlined style, "Go Live" text.
6. **Record button** — Outlined style. "REC" when idle, red fill when recording.
7. **Settings gear** — Opens native settings window (existing implementation).

Dividers (`1px`, `border` color, 20px tall) separate logical groups.

### Default Panel Layout

```
[=================== Toolbar ====================]
[          |                        |             ]
[ Sources  |       Preview          | Properties  ]
[  (220px) |       (flex)           |   (240px)   ]
[----------|                        |-------------|
[ Scenes   |                        | Audio Mixer ]
[          |                        |             ]
[=============================================----]
```

- **Left column** (220px): Sources panel (top, ~60%) and Scenes panel (bottom, ~40%), split by a horizontal divider.
- **Center** (flex): Preview panel fills remaining space.
- **Right column** (240px): Properties panel (top, ~60%) and Audio Mixer (bottom, ~40%), split by a horizontal divider.
- Vertical dividers separate the three columns.

All panels are dockable — users can drag tabs to rearrange, split, float, or close. The default layout above is restored via "Reset Layout" in the View menu or context menu.

### Dividers

- 3px wide/tall interactive hit area.
- Default color: `border`.
- Hover: transitions to `text-muted` over 150ms.
- Cursor changes to `col-resize` or `row-resize`.
- Ratio clamped to 0.1–0.9 to prevent collapsing panels.

## Panel Specifications

### Shared Panel Chrome

Every dockable panel has:

- **Header bar:** 28px tall, `bg-surface` background, `border` bottom edge.
- **Tabs:** 11px text, 2px bottom border on active tab (`text-primary` color by default, accent color when configured). Inactive tabs are `text-secondary`, hover transitions to `text-primary` over 150ms.
- **Content area:** `bg-panel` background, 8px padding, scrollable when content overflows.
- **Scrollbars:** 4px wide, `border` thumb color, transparent track, 2px border-radius.

### Preview Panel

Displays the composited video output.

- **Background:** `bg-base` (darkest) — the viewport sits inside this.
- **Viewport:** Maintains 16:9 aspect ratio. Centered with letterbox/pillarbox bars in `bg-base`. No visible border on the viewport itself — it's distinguished by content darkness.
- **LIVE badge:** Top-left of viewport, 9px bold white text on `red-live` background, 3px border-radius, `red-glow` box-shadow. Only visible when streaming.
- **Resolution overlay:** Bottom-right of viewport, 9px `text-muted` on `#00000080` background. Shows "{width}x{height} . {fps}fps".
- **Empty state:** Play button icon (circle + triangle) centered, `#ffffff10` circle with `#ffffff40` triangle.

### Sources Panel

Manages sources for the active scene.

- **Source list:** Vertical list, no grid. Each source item is a row:
  - **Icon:** 16x16 px, `bg-elevated` background, 2px border-radius. Emoji or monogram for source type.
  - **Name:** `text-primary`, 11px. Editable on double-click.
  - **Visibility toggle:** Eye icon, right-aligned. `text-muted` at 50% opacity when visible, full `text-primary` on hover. Hidden sources dim the entire row to 40% opacity.
- **Selection:** Selected source gets `accent-dim` background (`#88889030`). Clicking a source selects it and populates the Properties panel.
- **Reorder:** Drag to reorder. Smooth drag ghost with 150ms transition.
- **Add/Remove:** `+` button in panel header to add source (opens source type picker). Right-click context menu for delete, duplicate, rename.

### Scenes Panel

Manages scene collection.

- **Scene grid:** 2-column grid of scene thumbnails.
  - **Thumbnail:** 16:9 aspect ratio, `bg-elevated` background, 1px `border`, 3px border-radius.
  - **Active scene:** `text-primary` border color.
  - **Hover:** Border transitions to `text-muted` over 150ms.
  - **Label:** 9px `text-secondary` below thumbnail. Active scene label is `text-primary`.
- **Add scene:** Dashed border thumbnail with `+` icon, "Add" label in `text-muted`.
- **Interaction:** Click to switch scene (also updates toolbar switcher). Right-click for rename, duplicate, delete.

### Properties Panel (Context-Sensitive)

Shows properties for the currently selected source. Empty state when nothing is selected.

- **Sections:** Grouped by category with uppercase 9px `text-muted` labels.
- **Transform section:**
  - X, Y, W, H as paired inputs. Key labels (10px `text-muted`, right-aligned, 24px wide) + value inputs.
  - Inputs: 22px tall, `bg-base` background, 1px `border`, 3px border-radius, 10px `text-primary` text, `tabular-nums`.
  - Drag to adjust values (like Blender's drag-value fields).
- **Opacity section:**
  - Horizontal slider: 4px track (`bg-base` + `border`), `text-primary` fill and 8px circular thumb.
  - Percentage readout right-aligned, 10px `text-secondary`.
- **Source section:**
  - Source-type-specific fields. For display capture: monitor selector dropdown. For webcam: device selector. For text: text content + font settings.
  - Dropdowns use the same input styling as transform fields.
- **Empty state:** Centered `text-muted` message: "Select a source to view properties".

### Audio Mixer Panel

Horizontal channel strips for all audio sources.

- **Channel strip:** Vertical layout per channel:
  1. **Label:** 9px uppercase `text-muted`, centered.
  2. **VU meter:** 8px wide, 80px tall track (`bg-base` + `border`, 4px border-radius). Fill from bottom:
     - Green (`vu-green`): signal below -18 dB.
     - Yellow gradient (green → `vu-yellow`): signal -18 to -6 dB.
     - Red gradient (green → yellow → `vu-red`): signal above -6 dB. Adds `box-shadow: 0 0 6px #e74c3c30` glow at peak.
  3. **dB readout:** 9px `text-muted`, `tabular-nums`.
  4. **Mute button:** 20x16px, 1px `border`, 2px border-radius. "M" in 8px bold. Muted state: `red-live` fill, white text.
- **Layout:** Flex row, equal spacing per channel, 8px gap.
- **Channels:** One per audio source. Default: Mic, Desktop. Additional channels appear as audio sources are added.

## Animations and Transitions

All durations and easings:

| Element | Property | Duration | Easing |
|---------|----------|----------|--------|
| Tab hover | color | 150ms | ease |
| Tab active underline | opacity | 150ms | ease |
| Source item hover | background | 100ms | ease |
| Button hover | border-color, color | 150ms | ease |
| Divider hover | background | 150ms | ease |
| Scene thumb hover | border-color | 150ms | ease |
| Mute button state | background, color | 150ms | ease |
| Go Live pulse | box-shadow | 2000ms | ease-in-out, infinite |
| VU meter fill | height | 100ms | linear |
| VU peak glow | box-shadow | 100ms | linear |
| Drag ghost | opacity | 150ms | ease |
| Panel drop shadow (floating) | box-shadow | 150ms | ease |
| Settings toggle switch | position | 150ms | ease |

## Dockview System

The existing dockview implementation (binary split tree + tab groups + floating panels) remains architecturally unchanged. Visual changes:

- **Tab bar:** 28px height, `bg-surface`, 1px `border` bottom. Active tab indicated by 2px bottom border in accent color.
- **Drop zones:** Five zones (left/right/top/bottom/center) + tab bar insertion. Overlay tint uses accent color at 15% opacity (was 25% with purple).
- **Floating panels:** `bg-panel` background, 1px `border`, subtle `box-shadow: 0 4px 12px #00000040`. Draggable header bar matches panel header style.
- **Drag ghost:** Semi-transparent tab preview, 60% opacity, follows cursor with slight offset.
- **Grip drag:** 28px-wide drag handle on panel header enables dragging entire group.

## Settings Window

The native settings window retains its current category structure but adopts the new color tokens:

- **Sidebar:** `bg-base` background, 190px wide.
- **Content area:** `bg-surface` background.
- **Controls:** Same input, slider, toggle, dropdown styles as panels.
- **New addition:** Accent color picker in Appearance section — hex input + preview swatch. Default: `#e0e0e8`.

## Implementation Boundaries

### What Changes

- **Color constants:** Replace Catppuccin Mocha palette in `render.rs` and `settings_window.rs` with the new token system.
- **Layout constants:** Update `TAB_BAR_HEIGHT` (28px stays), `PANEL_PADDING` (6→8px), add toolbar constants.
- **Toolbar component:** New `ui/toolbar.rs` — fixed bar with scene switcher, stats, live/record buttons, settings.
- **Panel chrome:** Update all panel header rendering in `render.rs` to new styles.
- **Sources panel:** Extract from scene editor into `ui/sources_panel.rs`. Source list only, no scene management.
- **Scenes panel:** Extract from scene editor into `ui/scenes_panel.rs`. Scene grid with thumbnails.
- **Properties panel:** New `ui/properties_panel.rs`. Context-sensitive, shows properties for selected source.
- **Audio mixer:** Restyle existing `audio_mixer.rs` to match new visual language.
- **Preview panel:** Restyle existing `preview_panel.rs` — add LIVE badge, resolution overlay.
- **Stream controls panel:** Demoted to optional panel (not in default layout). Core controls (Go Live, Record) move to toolbar. The panel retains stream destination selection, custom RTMP URL input, and stream key configuration — users can dock it when needed for setup.
- **Settings window:** Update colors to new tokens. Add accent color picker.
- **Default layout:** Update `DockLayout::default()` to new 3-column arrangement.
- **Drop zone tint:** Update color from purple to accent-derived.

### What Stays

- **Dockview architecture:** Binary split tree, tab groups, floating panels, serialization — all unchanged.
- **Compositor pipeline:** No changes to GPU rendering.
- **GStreamer integration:** No changes to capture/encoding/streaming.
- **State management:** `AppState` structure unchanged beyond adding selected-source tracking.
- **Settings data model:** `AppSettings` gains an `accent_color` field; otherwise unchanged.
- **Window management:** Main window + detached windows unchanged.

### New State

- `AppState.selected_source_id: Option<SourceId>` — which source is selected (for Properties panel).
- `AppSettings.accent_color: String` — hex color string, default `"#e0e0e8"`.

## File Structure (Proposed)

```
src/ui/
  mod.rs              # Panel dispatcher (updated)
  toolbar.rs          # NEW — fixed toolbar component
  preview_panel.rs    # Updated — LIVE badge, resolution overlay
  sources_panel.rs    # NEW — extracted from scene_editor.rs
  scenes_panel.rs     # NEW — extracted from scene_editor.rs
  properties_panel.rs # NEW — context-sensitive property editor
  audio_mixer.rs      # Updated — new visual language
  stream_controls.rs  # Updated — demoted to optional panel
  settings_window.rs  # Updated — new colors, accent picker
  layout/
    tree.rs           # Updated — new default layout
    render.rs         # Updated — new color tokens, panel chrome
    interactions.rs   # Unchanged
    serialize.rs      # Unchanged
```
