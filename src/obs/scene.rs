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
    pub transform: Transform,
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

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct SourceConfig {
    pub name: String,
    pub source_type: SourceType,
    pub transform: Transform,
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

#[cfg(test)]
mod tests {
    use super::*;

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
            transform: Transform::new(0.0, 0.0, 640.0, 480.0),
            visible: true,
            muted: false,
            volume: 1.0,
        };
        assert!(source.visible);
        assert!(!source.muted);
        assert_eq!(source.volume, 1.0);
    }
}
