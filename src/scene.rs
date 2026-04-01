use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SceneId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SourceId(pub u64);

/// A scene contains an ordered list of source references with optional per-scene overrides.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scene {
    pub id: SceneId,
    pub name: String,
    pub sources: Vec<SceneSource>,
    /// Whether this scene is pinned to the toolbar for quick switching.
    #[serde(default)]
    pub pinned: bool,
    /// Per-scene transition override (type + duration when transitioning INTO this scene).
    #[serde(default)]
    pub transition_override: crate::transition::SceneTransitionOverride,
}

/// Canonical definition of a source in the global library.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibrarySource {
    pub id: SourceId,
    pub name: String,
    pub source_type: SourceType,
    #[serde(default)]
    pub properties: SourceProperties,
    pub transform: Transform,
    /// Original/native size of the source content (e.g., monitor resolution, image
    /// dimensions, camera resolution). Used by "Reset Transform" to restore the
    /// source to its natural size. Updated when the source content changes.
    #[serde(default = "default_native_size")]
    pub native_size: (f32, f32),
    /// When true, changing width or height preserves the current aspect ratio.
    #[serde(default)]
    pub aspect_ratio_locked: bool,
    /// Alpha opacity in the range [0.0, 1.0]. Values outside this range are clamped by the compositor.
    #[serde(default = "default_opacity")]
    pub opacity: f32,
    pub visible: bool,
    pub muted: bool,
    pub volume: f32,
    /// Optional folder for organizing sources in the library UI.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<String>,
    /// Ordered chain of shader effects applied to this source.
    #[serde(default)]
    pub effects: Vec<EffectInstance>,
}

/// Per-scene reference to a library source, with optional property overrides.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneSource {
    pub source_id: SourceId,
    #[serde(default)]
    pub overrides: SourceOverrides,
}

/// Optional per-scene overrides for source properties.
/// `None` means inherit the value from the library source.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SourceOverrides {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transform: Option<Transform>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opacity: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visible: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub muted: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volume: Option<f32>,
    /// Whether this source is locked (not movable/resizable) in this scene.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locked: Option<bool>,
    /// Per-scene effect chain override. Replaces the entire library chain when set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effects: Option<Vec<EffectInstance>>,
}

/// A single effect instance applied to a source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffectInstance {
    /// Effect ID matching the registry (e.g. "circle_crop").
    pub effect_id: String,
    /// Parameter values keyed by name. Missing keys use the effect's default.
    #[serde(default)]
    pub params: std::collections::HashMap<String, f32>,
    /// Whether this effect is active. Disabled effects remain in the chain but are skipped.
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SourceType {
    Display,
    Window,
    Camera,
    Audio,
    Image,
    Browser,
    Text,
    Color,
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Transform {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    #[serde(default)]
    pub rotation: f32, // Degrees, default 0.0
}

/// How an animated image (GIF) should loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum LoopMode {
    #[default]
    Infinite,
    Once,
    Count(u32),
}

/// Text alignment for text sources.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum TextAlignment {
    Left,
    Center,
    Right,
}

/// Text outline configuration.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TextOutline {
    pub color: [f32; 4],
    pub width: f32,
}

/// Color fill for color sources.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ColorFill {
    Solid {
        color: [f32; 4],
    },
    LinearGradient {
        angle: f32,
        stops: Vec<GradientStop>,
    },
    RadialGradient {
        center: (f32, f32),
        radius: f32,
        stops: Vec<GradientStop>,
    },
}

/// A single color stop in a gradient.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GradientStop {
    pub position: f32,
    pub color: [f32; 4],
}

/// Audio input configuration for audio sources.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AudioInput {
    Device {
        device_uid: String,
        device_name: String,
    },
    File {
        path: String,
        #[serde(default)]
        looping: bool,
    },
}

/// How the window source selects its capture target.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum WindowCaptureMode {
    /// Track a specific application by bundle ID.
    Application {
        bundle_id: String,
        app_name: String,
        /// Pin to a specific window by title substring (None = track frontmost).
        pinned_title: Option<String>,
    },
    /// Automatically capture whatever application is fullscreen.
    AnyFullscreen,
}

/// Type-specific source configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SourceProperties {
    Display {
        screen_index: u32,
    },
    Window {
        mode: WindowCaptureMode,
        /// Runtime-only: the currently tracked window ID. Resolved by WindowWatcher.
        #[serde(skip)]
        current_window_id: Option<u32>,
    },
    Camera {
        device_index: u32,
        device_name: String,
    },
    Image {
        path: String,
        /// Loop behavior override for animated GIFs. None = use GIF's embedded loop count.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        loop_mode: Option<LoopMode>,
    },
    Text {
        #[serde(default = "default_text_content")]
        content: String,
        #[serde(default = "default_font_family")]
        font_family: String,
        #[serde(default = "default_font_size")]
        font_size: f32,
        #[serde(default = "default_font_color")]
        font_color: [f32; 4],
        #[serde(default = "default_transparent")]
        background_color: [f32; 4],
        #[serde(default)]
        bold: bool,
        #[serde(default)]
        italic: bool,
        #[serde(default = "default_text_alignment")]
        alignment: TextAlignment,
        #[serde(default)]
        outline: Option<TextOutline>,
        #[serde(default = "default_padding")]
        padding: f32,
        #[serde(default)]
        wrap_width: Option<f32>,
    },
    Color {
        #[serde(default = "default_color_fill")]
        fill: ColorFill,
    },
    Audio {
        #[serde(default = "default_audio_input")]
        input: AudioInput,
    },
    Browser {
        #[serde(default)]
        url: String,
        #[serde(default = "default_browser_width")]
        width: u32,
        #[serde(default = "default_browser_height")]
        height: u32,
    },
}

impl Default for SourceProperties {
    fn default() -> Self {
        Self::Display { screen_index: 0 }
    }
}

pub(crate) fn default_text_content() -> String {
    "Text".to_string()
}
pub(crate) fn default_font_family() -> String {
    "bundled:sans".to_string()
}
pub(crate) fn default_font_size() -> f32 {
    48.0
}
pub(crate) fn default_font_color() -> [f32; 4] {
    [1.0, 1.0, 1.0, 1.0]
}
pub(crate) fn default_transparent() -> [f32; 4] {
    [0.0, 0.0, 0.0, 0.0]
}
pub(crate) fn default_text_alignment() -> TextAlignment {
    TextAlignment::Left
}
pub(crate) fn default_padding() -> f32 {
    12.0
}
pub(crate) fn default_color_fill() -> ColorFill {
    ColorFill::Solid {
        color: [1.0, 1.0, 1.0, 1.0],
    }
}
pub(crate) fn default_audio_input() -> AudioInput {
    AudioInput::Device {
        device_uid: String::new(),
        device_name: String::new(),
    }
}
pub(crate) fn default_browser_width() -> u32 {
    1920
}
pub(crate) fn default_browser_height() -> u32 {
    1080
}

impl SceneSource {
    /// Create a new scene source reference with no overrides.
    pub fn new(source_id: SourceId) -> Self {
        Self {
            source_id,
            overrides: SourceOverrides::default(),
        }
    }

    /// Resolve the effective transform, using the override if set, otherwise the library default.
    pub fn resolve_transform(&self, lib: &LibrarySource) -> Transform {
        self.overrides.transform.unwrap_or(lib.transform)
    }

    /// Resolve the effective opacity, using the override if set, otherwise the library default.
    pub fn resolve_opacity(&self, lib: &LibrarySource) -> f32 {
        self.overrides.opacity.unwrap_or(lib.opacity)
    }

    /// Resolve the effective visibility, using the override if set, otherwise the library default.
    pub fn resolve_visible(&self, lib: &LibrarySource) -> bool {
        self.overrides.visible.unwrap_or(lib.visible)
    }

    /// Resolve the effective muted state, using the override if set, otherwise the library default.
    #[allow(unused)]
    pub fn resolve_muted(&self, lib: &LibrarySource) -> bool {
        self.overrides.muted.unwrap_or(lib.muted)
    }

    /// Resolve the effective locked state (defaults to false if not overridden).
    pub fn resolve_locked(&self) -> bool {
        self.overrides.locked.unwrap_or(false)
    }

    /// Resolve the effective volume, using the override if set, otherwise the library default.
    #[allow(unused)]
    pub fn resolve_volume(&self, lib: &LibrarySource) -> f32 {
        self.overrides.volume.unwrap_or(lib.volume)
    }

    /// Returns true if the transform is overridden for this scene.
    pub fn is_transform_overridden(&self) -> bool {
        self.overrides.transform.is_some()
    }

    /// Returns true if opacity is overridden for this scene.
    pub fn is_opacity_overridden(&self) -> bool {
        self.overrides.opacity.is_some()
    }

    /// Returns true if visibility is overridden for this scene.
    #[allow(unused)]
    pub fn is_visible_overridden(&self) -> bool {
        self.overrides.visible.is_some()
    }

    /// Returns true if muted state is overridden for this scene.
    #[allow(unused)]
    pub fn is_muted_overridden(&self) -> bool {
        self.overrides.muted.is_some()
    }

    /// Returns true if volume is overridden for this scene.
    #[allow(unused)]
    pub fn is_volume_overridden(&self) -> bool {
        self.overrides.volume.is_some()
    }

    /// Resolve the effect chain: use scene override if set, otherwise library defaults.
    pub fn resolve_effects(&self, lib: &LibrarySource) -> Vec<EffectInstance> {
        self.overrides
            .effects
            .clone()
            .unwrap_or_else(|| lib.effects.clone())
    }

    /// Returns true if the scene overrides the effect chain.
    pub fn is_effects_overridden(&self) -> bool {
        self.overrides.effects.is_some()
    }
}

impl Scene {
    /// Move a source one position earlier (lower z-index / further back).
    #[allow(unused)]
    pub fn move_source_up(&mut self, source_id: SourceId) {
        if let Some(pos) = self.sources.iter().position(|s| s.source_id == source_id)
            && pos > 0
        {
            self.sources.swap(pos, pos - 1);
        }
    }

    /// Move a source one position later (higher z-index / further forward).
    #[allow(unused)]
    pub fn move_source_down(&mut self, source_id: SourceId) {
        if let Some(pos) = self.sources.iter().position(|s| s.source_id == source_id)
            && pos + 1 < self.sources.len()
        {
            self.sources.swap(pos, pos + 1);
        }
    }

    /// Returns a list of source IDs in this scene, in order.
    pub fn source_ids(&self) -> Vec<SourceId> {
        self.sources.iter().map(|s| s.source_id).collect()
    }

    /// Find a scene source reference by its source ID.
    pub fn find_source(&self, source_id: SourceId) -> Option<&SceneSource> {
        self.sources.iter().find(|s| s.source_id == source_id)
    }

    /// Find a mutable scene source reference by its source ID.
    pub fn find_source_mut(&mut self, source_id: SourceId) -> Option<&mut SceneSource> {
        self.sources.iter_mut().find(|s| s.source_id == source_id)
    }
}

impl Transform {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
            rotation: 0.0,
        }
    }
}

/// Persistence wrapper for scene/source data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneCollection {
    pub scenes: Vec<Scene>,
    #[serde(alias = "sources")]
    pub library: Vec<LibrarySource>,
    pub active_scene_id: Option<SceneId>,
    #[serde(default = "default_next_id")]
    pub next_scene_id: u64,
    #[serde(default = "default_next_id")]
    pub next_source_id: u64,
}

fn default_next_id() -> u64 {
    1
}

fn default_opacity() -> f32 {
    1.0
}

fn default_native_size() -> (f32, f32) {
    (1920.0, 1080.0)
}

/// Legacy format types for migrating old `scenes.toml` files.
///
/// In the legacy format, scenes contained `sources = [1, 2]` (a plain list of
/// `SourceId` integers) rather than `Vec<SceneSource>` structs. The top-level
/// field was `sources` (now `library`), but that rename is handled by the
/// `#[serde(alias)]` on `SceneCollection`. The scene-level migration is what
/// this module addresses.
mod legacy {
    use super::*;

    /// A scene in the legacy format where `sources` is a plain list of source IDs.
    #[derive(Debug, Clone, Deserialize)]
    pub struct LegacyScene {
        pub id: SceneId,
        pub name: String,
        pub sources: Vec<SourceId>,
    }

    /// The legacy scene collection format with `sources` as `Vec<LibrarySource>`
    /// and scenes containing plain `SourceId` lists.
    #[derive(Debug, Clone, Deserialize)]
    pub struct LegacySceneCollection {
        pub scenes: Vec<LegacyScene>,
        pub sources: Vec<LibrarySource>,
        pub active_scene_id: Option<SceneId>,
        #[serde(default = "super::default_next_id")]
        pub next_scene_id: u64,
        #[serde(default = "super::default_next_id")]
        pub next_source_id: u64,
    }

    impl LegacySceneCollection {
        /// Convert the legacy collection into the new format.
        pub fn into_new_format(self) -> SceneCollection {
            let scenes = self
                .scenes
                .into_iter()
                .map(|legacy_scene| Scene {
                    id: legacy_scene.id,
                    name: legacy_scene.name,
                    sources: legacy_scene
                        .sources
                        .into_iter()
                        .map(SceneSource::new)
                        .collect(),
                    pinned: false,
                    transition_override: Default::default(),
                })
                .collect();

            SceneCollection {
                scenes,
                library: self.sources,
                active_scene_id: self.active_scene_id,
                next_scene_id: self.next_scene_id,
                next_source_id: self.next_source_id,
            }
        }
    }
}

impl SceneCollection {
    /// Parse a TOML string into a `SceneCollection`, automatically detecting
    /// and migrating the legacy format if needed.
    ///
    /// Tries the new format first (with `Vec<SceneSource>` in scenes). If that
    /// fails, falls back to parsing as a legacy collection (with plain
    /// `Vec<SourceId>` in scenes) and converts it to the new format.
    pub fn from_toml_str(s: &str) -> Result<Self, toml::de::Error> {
        // Try the new format first
        let mut coll = match toml::from_str::<SceneCollection>(s) {
            Ok(coll) => coll,
            Err(new_err) => {
                // Fall back to legacy format
                match toml::from_str::<legacy::LegacySceneCollection>(s) {
                    Ok(legacy) => {
                        log::info!("Migrated legacy scenes.toml to new library format");
                        legacy.into_new_format()
                    }
                    Err(_) => return Err(new_err),
                }
            }
        };
        coll.migrate_source_properties();
        Ok(coll)
    }

    /// Ensure every library source has a `SourceProperties` variant matching its `SourceType`.
    /// Sources that were serialized before a new variant existed get default properties.
    fn migrate_source_properties(&mut self) {
        for source in &mut self.library {
            #[allow(clippy::match_like_matches_macro)]
            let needs_migration = match (&source.source_type, &source.properties) {
                (SourceType::Display, SourceProperties::Display { .. }) => false,
                (SourceType::Window, SourceProperties::Window { .. }) => false,
                (SourceType::Camera, SourceProperties::Camera { .. }) => false,
                (SourceType::Image, SourceProperties::Image { .. }) => false,
                (SourceType::Text, SourceProperties::Text { .. }) => false,
                (SourceType::Color, SourceProperties::Color { .. }) => false,
                (SourceType::Audio, SourceProperties::Audio { .. }) => false,
                (SourceType::Browser, SourceProperties::Browser { .. }) => false,
                _ => true,
            };
            if needs_migration {
                source.properties = match source.source_type {
                    SourceType::Text => SourceProperties::Text {
                        content: default_text_content(),
                        font_family: default_font_family(),
                        font_size: default_font_size(),
                        font_color: default_font_color(),
                        background_color: default_transparent(),
                        bold: false,
                        italic: false,
                        alignment: default_text_alignment(),
                        outline: None,
                        padding: default_padding(),
                        wrap_width: None,
                    },
                    SourceType::Color => SourceProperties::Color {
                        fill: default_color_fill(),
                    },
                    SourceType::Audio => SourceProperties::Audio {
                        input: default_audio_input(),
                    },
                    SourceType::Browser => SourceProperties::Browser {
                        url: String::new(),
                        width: default_browser_width(),
                        height: default_browser_height(),
                    },
                    _ => {
                        log::warn!(
                            "Source {:?} has mismatched properties {:?}; keeping as-is",
                            source.source_type,
                            source.properties
                        );
                        source.properties.clone()
                    }
                };
            }
        }
    }

    pub fn default_collection() -> Self {
        let scene_id = SceneId(1);
        let source_id = SourceId(1);
        Self {
            scenes: vec![Scene {
                id: scene_id,
                name: "Scene 1".to_string(),
                sources: vec![SceneSource::new(source_id)],
                pinned: true,
                transition_override: Default::default(),
            }],
            library: vec![LibrarySource {
                id: source_id,
                name: "Display".to_string(),
                source_type: SourceType::Display,
                properties: SourceProperties::Display { screen_index: 0 },
                transform: Transform::new(0.0, 0.0, 1920.0, 1080.0),
                native_size: (1920.0, 1080.0),
                aspect_ratio_locked: false,
                opacity: 1.0,
                visible: true,
                muted: false,
                volume: 1.0,
                folder: None,
                effects: Vec::new(),
            }],
            active_scene_id: Some(scene_id),
            next_scene_id: 2,
            next_source_id: 2,
        }
    }

    pub fn load_from(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(contents) => Self::from_toml_str(&contents).unwrap_or_else(|e| {
                log::warn!("Failed to parse scenes.toml, using default: {e}");
                Self::default_collection()
            }),
            Err(_) => Self::default_collection(),
        }
    }

    pub fn save_to(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let toml_str = toml::to_string_pretty(self)?;
        std::fs::write(path, toml_str)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to create a LibrarySource with sensible defaults for testing.
    fn test_library_source(id: u64) -> LibrarySource {
        LibrarySource {
            id: SourceId(id),
            name: format!("Source {id}"),
            source_type: SourceType::Display,
            properties: SourceProperties::default(),
            transform: Transform::new(0.0, 0.0, 1920.0, 1080.0),
            native_size: (1920.0, 1080.0),
            aspect_ratio_locked: false,
            opacity: 1.0,
            visible: true,
            muted: false,
            volume: 1.0,
            folder: None,
            effects: Vec::new(),
        }
    }

    // ---- SceneSource resolution tests ----

    #[test]
    fn scene_source_inherits_library_defaults() {
        let lib = test_library_source(1);
        let ss = SceneSource::new(SourceId(1));

        assert_eq!(ss.resolve_transform(&lib), lib.transform);
        assert_eq!(ss.resolve_opacity(&lib), lib.opacity);
        assert_eq!(ss.resolve_visible(&lib), lib.visible);
        assert_eq!(ss.resolve_muted(&lib), lib.muted);
        assert_eq!(ss.resolve_volume(&lib), lib.volume);
    }

    #[test]
    fn scene_source_override_takes_precedence() {
        let lib = test_library_source(1);
        let ss = SceneSource {
            source_id: SourceId(1),
            overrides: SourceOverrides {
                transform: Some(Transform::new(10.0, 20.0, 800.0, 600.0)),
                opacity: Some(0.5),
                visible: Some(false),
                muted: Some(true),
                volume: Some(0.25),
                locked: None,
                effects: None,
            },
        };

        assert_eq!(
            ss.resolve_transform(&lib),
            Transform::new(10.0, 20.0, 800.0, 600.0)
        );
        assert_eq!(ss.resolve_opacity(&lib), 0.5);
        assert!(!ss.resolve_visible(&lib));
        assert!(ss.resolve_muted(&lib));
        assert_eq!(ss.resolve_volume(&lib), 0.25);
    }

    #[test]
    fn scene_source_partial_override() {
        let lib = test_library_source(1);
        let ss = SceneSource {
            source_id: SourceId(1),
            overrides: SourceOverrides {
                opacity: Some(0.75),
                ..Default::default()
            },
        };

        // Overridden field uses override value
        assert_eq!(ss.resolve_opacity(&lib), 0.75);
        // Non-overridden fields inherit from library
        assert_eq!(ss.resolve_transform(&lib), lib.transform);
        assert_eq!(ss.resolve_visible(&lib), true);
    }

    #[test]
    fn scene_source_reset_override() {
        let lib = test_library_source(1);
        let mut ss = SceneSource {
            source_id: SourceId(1),
            overrides: SourceOverrides {
                opacity: Some(0.5),
                visible: Some(false),
                ..Default::default()
            },
        };

        assert!(ss.is_opacity_overridden());
        assert!(ss.is_visible_overridden());

        // Reset overrides
        ss.overrides.opacity = None;
        ss.overrides.visible = None;

        assert!(!ss.is_opacity_overridden());
        assert!(!ss.is_visible_overridden());
        assert_eq!(ss.resolve_opacity(&lib), 1.0);
        assert!(ss.resolve_visible(&lib));
    }

    #[test]
    fn scene_source_is_overridden_checks() {
        let ss = SceneSource {
            source_id: SourceId(1),
            overrides: SourceOverrides {
                transform: Some(Transform::new(0.0, 0.0, 100.0, 100.0)),
                opacity: None,
                visible: Some(false),
                muted: None,
                volume: Some(0.5),
                locked: None,
                effects: None,
            },
        };

        assert!(ss.is_transform_overridden());
        assert!(!ss.is_opacity_overridden());
        assert!(ss.is_visible_overridden());
        assert!(!ss.is_muted_overridden());
        assert!(ss.is_volume_overridden());
    }

    // ---- LibrarySource tests ----

    #[test]
    fn library_source_opacity_defaults_to_one() {
        let toml_str = r#"
            id = 1
            name = "Test"
            source_type = "Display"
            visible = true
            muted = false
            volume = 1.0
            [properties.Display]
            screen_index = 0
            [transform]
            x = 0.0
            y = 0.0
            width = 1920.0
            height = 1080.0
        "#;
        let source: LibrarySource = toml::from_str(toml_str).unwrap();
        assert!((source.opacity - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn library_source_opacity_roundtrips() {
        let source = LibrarySource {
            id: SourceId(1),
            name: "Test".into(),
            source_type: SourceType::Display,
            properties: SourceProperties::default(),
            transform: Transform::new(0.0, 0.0, 1920.0, 1080.0),
            native_size: (1920.0, 1080.0),
            aspect_ratio_locked: false,
            opacity: 0.5,
            visible: true,
            muted: false,
            volume: 1.0,
            folder: None,
            effects: Vec::new(),
        };
        let serialized = toml::to_string(&source).unwrap();
        let deserialized: LibrarySource = toml::from_str(&serialized).unwrap();
        assert!((deserialized.opacity - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn library_source_folder_optional() {
        let mut source = test_library_source(1);
        assert!(source.folder.is_none());

        source.folder = Some("Cameras".to_string());
        let serialized = toml::to_string(&source).unwrap();
        let deserialized: LibrarySource = toml::from_str(&serialized).unwrap();
        assert_eq!(deserialized.folder, Some("Cameras".to_string()));
    }

    #[test]
    fn library_source_folder_skipped_when_none() {
        let source = test_library_source(1);
        let serialized = toml::to_string(&source).unwrap();
        assert!(!serialized.contains("folder"));
    }

    // ---- Scene tests ----

    #[test]
    fn scene_move_source_up() {
        let mut scene = Scene {
            id: SceneId(1),
            name: "Test".into(),
            sources: vec![
                SceneSource::new(SourceId(1)),
                SceneSource::new(SourceId(2)),
                SceneSource::new(SourceId(3)),
            ],
            pinned: false,
            transition_override: Default::default(),
        };
        scene.move_source_up(SourceId(2));
        assert_eq!(
            scene.source_ids(),
            vec![SourceId(2), SourceId(1), SourceId(3)]
        );
    }

    #[test]
    fn scene_move_source_up_already_first() {
        let mut scene = Scene {
            id: SceneId(1),
            name: "Test".into(),
            sources: vec![SceneSource::new(SourceId(1)), SceneSource::new(SourceId(2))],
            pinned: false,
            transition_override: Default::default(),
        };
        scene.move_source_up(SourceId(1));
        assert_eq!(scene.source_ids(), vec![SourceId(1), SourceId(2)]);
    }

    #[test]
    fn scene_move_source_down() {
        let mut scene = Scene {
            id: SceneId(1),
            name: "Test".into(),
            sources: vec![
                SceneSource::new(SourceId(1)),
                SceneSource::new(SourceId(2)),
                SceneSource::new(SourceId(3)),
            ],
            pinned: false,
            transition_override: Default::default(),
        };
        scene.move_source_down(SourceId(1));
        assert_eq!(
            scene.source_ids(),
            vec![SourceId(2), SourceId(1), SourceId(3)]
        );
    }

    #[test]
    fn scene_move_source_down_already_last() {
        let mut scene = Scene {
            id: SceneId(1),
            name: "Test".into(),
            sources: vec![SceneSource::new(SourceId(1)), SceneSource::new(SourceId(2))],
            pinned: false,
            transition_override: Default::default(),
        };
        scene.move_source_down(SourceId(2));
        assert_eq!(scene.source_ids(), vec![SourceId(1), SourceId(2)]);
    }

    #[test]
    fn scene_source_ids_helper() {
        let scene = Scene {
            id: SceneId(1),
            name: "Main".to_string(),
            sources: vec![
                SceneSource::new(SourceId(10)),
                SceneSource::new(SourceId(20)),
            ],
            pinned: false,
            transition_override: Default::default(),
        };
        assert_eq!(scene.source_ids(), vec![SourceId(10), SourceId(20)]);
    }

    #[test]
    fn scene_find_source() {
        let scene = Scene {
            id: SceneId(1),
            name: "Test".into(),
            sources: vec![SceneSource::new(SourceId(1)), SceneSource::new(SourceId(2))],
            pinned: false,
            transition_override: Default::default(),
        };
        assert!(scene.find_source(SourceId(1)).is_some());
        assert!(scene.find_source(SourceId(99)).is_none());
    }

    #[test]
    fn scene_find_source_mut() {
        let mut scene = Scene {
            id: SceneId(1),
            name: "Test".into(),
            sources: vec![SceneSource::new(SourceId(1))],
            pinned: false,
            transition_override: Default::default(),
        };
        let ss = scene.find_source_mut(SourceId(1)).unwrap();
        ss.overrides.opacity = Some(0.5);
        assert!(
            scene
                .find_source(SourceId(1))
                .unwrap()
                .is_opacity_overridden()
        );
    }

    // ---- Transform tests ----

    #[test]
    fn transform_constructor() {
        let t = Transform::new(100.0, 200.0, 1920.0, 1080.0);
        assert_eq!(t.x, 100.0);
        assert_eq!(t.width, 1920.0);
    }

    // ---- LibrarySource defaults tests ----

    #[test]
    fn library_source_defaults_visible_unmuted() {
        let source = test_library_source(1);
        assert!(source.visible);
        assert!(!source.muted);
        assert_eq!(source.volume, 1.0);
    }

    #[test]
    fn source_properties_default_is_display_0() {
        let props = SourceProperties::default();
        assert!(matches!(
            props,
            SourceProperties::Display { screen_index: 0 }
        ));
    }

    // ---- SceneCollection tests ----

    #[test]
    fn scene_collection_default_has_one_scene() {
        let coll = SceneCollection::default_collection();
        assert_eq!(coll.scenes.len(), 1);
        assert_eq!(coll.library.len(), 1);
        assert_eq!(coll.next_scene_id, 2);
    }

    #[test]
    fn scene_collection_roundtrip() {
        let coll = SceneCollection::default_collection();
        let toml_str = toml::to_string_pretty(&coll).unwrap();
        let parsed: SceneCollection = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.scenes.len(), 1);
        assert_eq!(parsed.library.len(), 1);
        assert_eq!(parsed.next_scene_id, 2);
    }

    // ---- SourceOverrides serialization tests ----

    #[test]
    fn source_overrides_empty_serializes_cleanly() {
        let ss = SceneSource::new(SourceId(1));
        let serialized = toml::to_string(&ss).unwrap();
        // Empty overrides should not produce override fields
        assert!(!serialized.contains("opacity"));
        assert!(!serialized.contains("visible"));
        assert!(!serialized.contains("muted"));
        assert!(!serialized.contains("volume"));
    }

    #[test]
    fn source_overrides_roundtrip() {
        let ss = SceneSource {
            source_id: SourceId(1),
            overrides: SourceOverrides {
                transform: Some(Transform::new(10.0, 20.0, 800.0, 600.0)),
                opacity: Some(0.5),
                visible: None,
                muted: Some(true),
                volume: None,
                locked: None,
                effects: None,
            },
        };
        let serialized = toml::to_string(&ss).unwrap();
        let deserialized: SceneSource = toml::from_str(&serialized).unwrap();
        assert_eq!(
            deserialized.overrides.transform,
            Some(Transform::new(10.0, 20.0, 800.0, 600.0))
        );
        assert_eq!(deserialized.overrides.opacity, Some(0.5));
        assert_eq!(deserialized.overrides.visible, None);
        assert_eq!(deserialized.overrides.muted, Some(true));
        assert_eq!(deserialized.overrides.volume, None);
    }

    // ---- Legacy migration tests ----

    const LEGACY_SINGLE_SOURCE: &str = r#"
next_scene_id = 2
next_source_id = 2

[[sources]]
id = 1
name = "Display"
source_type = "Display"
visible = true
muted = false
volume = 1.0
opacity = 1.0
[sources.properties.Display]
screen_index = 0
[sources.transform]
x = 0.0
y = 0.0
width = 1920.0
height = 1080.0

[[scenes]]
id = 1
name = "Scene 1"
sources = [1]
"#;

    #[test]
    fn migrate_legacy_single_source() {
        let coll = SceneCollection::from_toml_str(LEGACY_SINGLE_SOURCE).unwrap();

        // Library should contain the source
        assert_eq!(coll.library.len(), 1);
        assert_eq!(coll.library[0].name, "Display");
        assert_eq!(coll.library[0].id, SourceId(1));

        // Scene should reference it via SceneSource with empty overrides
        assert_eq!(coll.scenes.len(), 1);
        assert_eq!(coll.scenes[0].sources.len(), 1);
        assert_eq!(coll.scenes[0].sources[0].source_id, SourceId(1));
        assert!(!coll.scenes[0].sources[0].is_transform_overridden());
        assert!(!coll.scenes[0].sources[0].is_opacity_overridden());

        // IDs preserved
        assert_eq!(coll.next_scene_id, 2);
        assert_eq!(coll.next_source_id, 2);
    }

    #[test]
    fn migrate_legacy_multiple_sources_and_scenes() {
        let toml_str = r#"
next_scene_id = 3
next_source_id = 4

[[sources]]
id = 1
name = "Display"
source_type = "Display"
visible = true
muted = false
volume = 1.0
opacity = 1.0
[sources.properties.Display]
screen_index = 0
[sources.transform]
x = 0.0
y = 0.0
width = 1920.0
height = 1080.0

[[sources]]
id = 2
name = "Camera"
source_type = "Camera"
visible = true
muted = false
volume = 1.0
opacity = 0.8
[sources.properties.Camera]
device_index = 0
device_name = "FaceCam"
[sources.transform]
x = 100.0
y = 100.0
width = 640.0
height = 480.0

[[sources]]
id = 3
name = "Logo"
source_type = "Image"
visible = true
muted = false
volume = 1.0
opacity = 1.0
[sources.properties.Image]
path = "/tmp/logo.png"
[sources.transform]
x = 0.0
y = 0.0
width = 200.0
height = 200.0

[[scenes]]
id = 1
name = "Main Scene"
sources = [1, 2, 3]

[[scenes]]
id = 2
name = "BRB"
sources = [3]
"#;

        let coll = SceneCollection::from_toml_str(toml_str).unwrap();

        assert_eq!(coll.library.len(), 3);
        assert_eq!(coll.library[0].name, "Display");
        assert_eq!(coll.library[1].name, "Camera");
        assert_eq!(coll.library[2].name, "Logo");

        // Scene 1 has all three sources
        assert_eq!(coll.scenes.len(), 2);
        assert_eq!(coll.scenes[0].name, "Main Scene");
        assert_eq!(coll.scenes[0].sources.len(), 3);
        assert_eq!(coll.scenes[0].sources[0].source_id, SourceId(1));
        assert_eq!(coll.scenes[0].sources[1].source_id, SourceId(2));
        assert_eq!(coll.scenes[0].sources[2].source_id, SourceId(3));

        // Scene 2 has just the logo
        assert_eq!(coll.scenes[1].name, "BRB");
        assert_eq!(coll.scenes[1].sources.len(), 1);
        assert_eq!(coll.scenes[1].sources[0].source_id, SourceId(3));

        // All overrides should be empty
        for scene in &coll.scenes {
            for ss in &scene.sources {
                assert!(!ss.is_transform_overridden());
                assert!(!ss.is_opacity_overridden());
                assert!(!ss.is_visible_overridden());
                assert!(!ss.is_muted_overridden());
                assert!(!ss.is_volume_overridden());
            }
        }

        assert_eq!(coll.next_scene_id, 3);
        assert_eq!(coll.next_source_id, 4);
    }

    #[test]
    fn from_toml_str_prefers_new_format() {
        // Serialize a new-format collection and verify it parses without fallback
        let coll = SceneCollection::default_collection();
        let toml_str = toml::to_string_pretty(&coll).unwrap();
        let parsed = SceneCollection::from_toml_str(&toml_str).unwrap();
        assert_eq!(parsed.scenes.len(), 1);
        assert_eq!(parsed.library.len(), 1);
        assert_eq!(parsed.scenes[0].sources[0].source_id, SourceId(1));
    }

    #[test]
    fn from_toml_str_returns_error_on_invalid_toml() {
        let result = SceneCollection::from_toml_str("this is not valid toml {{{{");
        assert!(result.is_err());
    }

    #[test]
    fn migrate_legacy_empty_scene_sources() {
        let toml_str = r#"
next_scene_id = 2
next_source_id = 2

[[sources]]
id = 1
name = "Display"
source_type = "Display"
visible = true
muted = false
volume = 1.0
opacity = 1.0
[sources.properties.Display]
screen_index = 0
[sources.transform]
x = 0.0
y = 0.0
width = 1920.0
height = 1080.0

[[scenes]]
id = 1
name = "Empty Scene"
sources = []
"#;

        let coll = SceneCollection::from_toml_str(toml_str).unwrap();
        assert_eq!(coll.scenes[0].sources.len(), 0);
        assert_eq!(coll.library.len(), 1);
    }

    #[test]
    fn migrate_legacy_preserves_source_properties() {
        let coll = SceneCollection::from_toml_str(LEGACY_SINGLE_SOURCE).unwrap();
        let source = &coll.library[0];

        assert!(matches!(source.source_type, SourceType::Display));
        assert!(matches!(
            source.properties,
            SourceProperties::Display { screen_index: 0 }
        ));
        assert_eq!(source.transform, Transform::new(0.0, 0.0, 1920.0, 1080.0));
        assert!(source.visible);
        assert!(!source.muted);
        assert_eq!(source.volume, 1.0);
        assert!((source.opacity - 1.0).abs() < f32::EPSILON);
    }

    // ---- EffectInstance tests ----

    #[test]
    fn effect_instance_default_enabled() {
        let effect = EffectInstance {
            effect_id: "circle_crop".to_string(),
            params: std::collections::HashMap::new(),
            enabled: true,
        };
        assert!(effect.enabled);
        assert!(effect.params.is_empty());
    }

    #[test]
    fn effect_instance_roundtrip_toml() {
        let effect = EffectInstance {
            effect_id: "circle_crop".to_string(),
            params: [("radius".to_string(), 0.4), ("feather".to_string(), 0.02)]
                .into_iter()
                .collect(),
            enabled: true,
        };
        let toml_str = toml::to_string(&effect).unwrap();
        let restored: EffectInstance = toml::from_str(&toml_str).unwrap();
        assert_eq!(restored.effect_id, "circle_crop");
        assert!((restored.params["radius"] - 0.4).abs() < f32::EPSILON);
        assert!(restored.enabled);
    }

    #[test]
    fn resolve_effects_falls_back_to_library() {
        let mut lib = test_library_source(1);
        lib.effects = vec![EffectInstance {
            effect_id: "circle_crop".to_string(),
            params: std::collections::HashMap::new(),
            enabled: true,
        }];
        let ss = SceneSource::new(SourceId(1));
        let resolved = ss.resolve_effects(&lib);
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].effect_id, "circle_crop");
        assert!(!ss.is_effects_overridden());
    }

    #[test]
    fn resolve_effects_uses_scene_override() {
        let mut lib = test_library_source(1);
        lib.effects = vec![EffectInstance {
            effect_id: "circle_crop".to_string(),
            params: std::collections::HashMap::new(),
            enabled: true,
        }];
        let ss = SceneSource {
            source_id: SourceId(1),
            overrides: SourceOverrides {
                effects: Some(vec![EffectInstance {
                    effect_id: "chroma_key".to_string(),
                    params: std::collections::HashMap::new(),
                    enabled: false,
                }]),
                ..Default::default()
            },
        };
        let resolved = ss.resolve_effects(&lib);
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].effect_id, "chroma_key");
        assert!(!resolved[0].enabled);
        assert!(ss.is_effects_overridden());
    }
}
