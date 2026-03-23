use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SceneId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SourceId(pub u64);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scene {
    pub id: SceneId,
    pub name: String,
    pub sources: Vec<SourceId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Source {
    pub id: SourceId,
    pub name: String,
    pub source_type: SourceType,
    #[serde(default)]
    pub properties: SourceProperties,
    pub transform: Transform,
    /// Alpha opacity in the range [0.0, 1.0]. Values outside this range are clamped by the compositor.
    #[serde(default = "default_opacity")]
    pub opacity: f32,
    pub visible: bool,
    pub muted: bool,
    pub volume: f32,
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Transform {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// Type-specific source configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SourceProperties {
    Display { screen_index: u32 },
    Image { path: String },
}

impl Default for SourceProperties {
    fn default() -> Self {
        Self::Display { screen_index: 0 }
    }
}

impl Scene {
    /// Move a source one position earlier (lower z-index / further back).
    pub fn move_source_up(&mut self, source_id: SourceId) {
        if let Some(pos) = self.sources.iter().position(|&id| id == source_id)
            && pos > 0
        {
            self.sources.swap(pos, pos - 1);
        }
    }

    /// Move a source one position later (higher z-index / further forward).
    pub fn move_source_down(&mut self, source_id: SourceId) {
        if let Some(pos) = self.sources.iter().position(|&id| id == source_id)
            && pos + 1 < self.sources.len()
        {
            self.sources.swap(pos, pos + 1);
        }
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
    pub sources: Vec<Source>,
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

impl SceneCollection {
    pub fn default_collection() -> Self {
        let scene_id = SceneId(1);
        let source_id = SourceId(1);
        Self {
            scenes: vec![Scene {
                id: scene_id,
                name: "Scene 1".to_string(),
                sources: vec![source_id],
            }],
            sources: vec![Source {
                id: source_id,
                name: "Display".to_string(),
                source_type: SourceType::Display,
                properties: SourceProperties::Display { screen_index: 0 },
                transform: Transform::new(0.0, 0.0, 1920.0, 1080.0),
                opacity: 1.0,
                visible: true,
                muted: false,
                volume: 1.0,
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

    #[test]
    fn source_opacity_defaults_to_one() {
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
        let source: Source = toml::from_str(toml_str).unwrap();
        assert!((source.opacity - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn source_opacity_roundtrips() {
        let source = Source {
            id: SourceId(1),
            name: "Test".into(),
            source_type: SourceType::Display,
            properties: SourceProperties::default(),
            transform: Transform {
                x: 0.0,
                y: 0.0,
                width: 1920.0,
                height: 1080.0,
            },
            opacity: 0.5,
            visible: true,
            muted: false,
            volume: 1.0,
        };
        let serialized = toml::to_string(&source).unwrap();
        let deserialized: Source = toml::from_str(&serialized).unwrap();
        assert!((deserialized.opacity - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn scene_move_source_up() {
        let mut scene = Scene {
            id: SceneId(1),
            name: "Test".into(),
            sources: vec![SourceId(1), SourceId(2), SourceId(3)],
        };
        scene.move_source_up(SourceId(2));
        assert_eq!(scene.sources, vec![SourceId(2), SourceId(1), SourceId(3)]);
    }

    #[test]
    fn scene_move_source_up_already_first() {
        let mut scene = Scene {
            id: SceneId(1),
            name: "Test".into(),
            sources: vec![SourceId(1), SourceId(2)],
        };
        scene.move_source_up(SourceId(1));
        assert_eq!(scene.sources, vec![SourceId(1), SourceId(2)]);
    }

    #[test]
    fn scene_move_source_down() {
        let mut scene = Scene {
            id: SceneId(1),
            name: "Test".into(),
            sources: vec![SourceId(1), SourceId(2), SourceId(3)],
        };
        scene.move_source_down(SourceId(1));
        assert_eq!(scene.sources, vec![SourceId(2), SourceId(1), SourceId(3)]);
    }

    #[test]
    fn scene_move_source_down_already_last() {
        let mut scene = Scene {
            id: SceneId(1),
            name: "Test".into(),
            sources: vec![SourceId(1), SourceId(2)],
        };
        scene.move_source_down(SourceId(2));
        assert_eq!(scene.sources, vec![SourceId(1), SourceId(2)]);
    }

    #[test]
    fn scene_stores_source_ids() {
        let scene = Scene {
            id: SceneId(1),
            name: "Main".to_string(),
            sources: vec![SourceId(10), SourceId(20)],
        };
        assert_eq!(scene.sources.len(), 2);
        assert_eq!(scene.sources[0], SourceId(10));
    }

    #[test]
    fn transform_constructor() {
        let t = Transform::new(100.0, 200.0, 1920.0, 1080.0);
        assert_eq!(t.x, 100.0);
        assert_eq!(t.width, 1920.0);
    }

    #[test]
    fn source_defaults_visible_unmuted() {
        let source = Source {
            id: SourceId(1),
            name: "Webcam".to_string(),
            source_type: SourceType::Camera,
            properties: SourceProperties::default(),
            transform: Transform::new(0.0, 0.0, 640.0, 480.0),
            opacity: 1.0,
            visible: true,
            muted: false,
            volume: 1.0,
        };
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

    #[test]
    fn scene_collection_default_has_one_scene() {
        let coll = SceneCollection::default_collection();
        assert_eq!(coll.scenes.len(), 1);
        assert_eq!(coll.sources.len(), 1);
        assert_eq!(coll.next_scene_id, 2);
    }

    #[test]
    fn scene_collection_roundtrip() {
        let coll = SceneCollection::default_collection();
        let toml_str = toml::to_string_pretty(&coll).unwrap();
        let parsed: SceneCollection = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.scenes.len(), 1);
        assert_eq!(parsed.sources.len(), 1);
        assert_eq!(parsed.next_scene_id, 2);
    }
}
