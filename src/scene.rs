use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SceneId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SourceId(pub u64);

/// Backward-compatible alias for `LibrarySource`. Will be removed in Task 3.
#[deprecated(note = "Use LibrarySource instead — will be removed in Task 3")]
pub type Source = LibrarySource;

/// A scene contains an ordered list of source references with optional per-scene overrides.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scene {
    pub id: SceneId,
    pub name: String,
    pub sources: Vec<SceneSource>,
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
    /// Alpha opacity in the range [0.0, 1.0]. Values outside this range are clamped by the compositor.
    #[serde(default = "default_opacity")]
    pub opacity: f32,
    pub visible: bool,
    pub muted: bool,
    pub volume: f32,
    /// Optional folder for organizing sources in the library UI.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<String>,
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SourceType {
    Display,
    Window,
    Camera,
    Audio,
    Image,
    Browser,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Transform {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// Type-specific source configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SourceProperties {
    Display {
        screen_index: u32,
    },
    Window {
        window_id: u32,
        window_title: String,
        owner_name: String,
    },
    Camera {
        device_index: u32,
        device_name: String,
    },
    Image {
        path: String,
    },
}

impl Default for SourceProperties {
    fn default() -> Self {
        Self::Display { screen_index: 0 }
    }
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
    pub fn resolve_muted(&self, lib: &LibrarySource) -> bool {
        self.overrides.muted.unwrap_or(lib.muted)
    }

    /// Resolve the effective volume, using the override if set, otherwise the library default.
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
    pub fn is_visible_overridden(&self) -> bool {
        self.overrides.visible.is_some()
    }

    /// Returns true if muted state is overridden for this scene.
    pub fn is_muted_overridden(&self) -> bool {
        self.overrides.muted.is_some()
    }

    /// Returns true if volume is overridden for this scene.
    pub fn is_volume_overridden(&self) -> bool {
        self.overrides.volume.is_some()
    }
}

impl Scene {
    /// Move a source one position earlier (lower z-index / further back).
    pub fn move_source_up(&mut self, source_id: SourceId) {
        if let Some(pos) = self.sources.iter().position(|s| s.source_id == source_id)
            && pos > 0
        {
            self.sources.swap(pos, pos - 1);
        }
    }

    /// Move a source one position later (higher z-index / further forward).
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

impl SceneCollection {
    /// Backward-compatible accessor for the source library. Will be removed in Task 3.
    #[deprecated(note = "Use .library instead — will be removed in Task 3")]
    pub fn sources(&self) -> &Vec<LibrarySource> {
        &self.library
    }

    /// Backward-compatible mutable accessor for the source library. Will be removed in Task 3.
    #[deprecated(note = "Use .library instead — will be removed in Task 3")]
    pub fn sources_mut(&mut self) -> &mut Vec<LibrarySource> {
        &mut self.library
    }

    pub fn default_collection() -> Self {
        let scene_id = SceneId(1);
        let source_id = SourceId(1);
        Self {
            scenes: vec![Scene {
                id: scene_id,
                name: "Scene 1".to_string(),
                sources: vec![SceneSource::new(source_id)],
            }],
            library: vec![LibrarySource {
                id: source_id,
                name: "Display".to_string(),
                source_type: SourceType::Display,
                properties: SourceProperties::Display { screen_index: 0 },
                transform: Transform::new(0.0, 0.0, 1920.0, 1080.0),
                native_size: (1920.0, 1080.0),
                opacity: 1.0,
                visible: true,
                muted: false,
                volume: 1.0,
                folder: None,
            }],
            active_scene_id: Some(scene_id),
            next_scene_id: 2,
            next_source_id: 2,
        }
    }

    pub fn load_from(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(contents) => toml::from_str(&contents).unwrap_or_else(|e| {
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
            opacity: 1.0,
            visible: true,
            muted: false,
            volume: 1.0,
            folder: None,
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
            opacity: 0.5,
            visible: true,
            muted: false,
            volume: 1.0,
            folder: None,
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
        };
        assert_eq!(scene.source_ids(), vec![SourceId(10), SourceId(20)]);
    }

    #[test]
    fn scene_find_source() {
        let scene = Scene {
            id: SceneId(1),
            name: "Test".into(),
            sources: vec![SceneSource::new(SourceId(1)), SceneSource::new(SourceId(2))],
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
}
