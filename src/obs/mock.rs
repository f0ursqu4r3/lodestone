use anyhow::{Result, anyhow};
use std::collections::HashMap;
use std::path::Path;
use tokio::sync::mpsc::{self, Receiver};

use crate::scene::{Scene, SceneId, Source, SourceConfig, SourceId, Transform};
use super::{
    ObsEngine, ObsStats, RgbaFrame,
    encoder::EncoderConfig,
    output::StreamConfig,
};

pub struct MockObsEngine {
    scenes: HashMap<SceneId, Scene>,
    #[allow(dead_code)]
    sources: HashMap<SourceId, Source>,
    #[allow(dead_code)]
    next_scene_id: u64,
    #[allow(dead_code)]
    next_source_id: u64,
    active_scene: Option<SceneId>,
    #[allow(dead_code)]
    streaming: bool,
    #[allow(dead_code)]
    recording: bool,
    #[allow(dead_code)]
    encoder_config: EncoderConfig,
}

impl MockObsEngine {
    pub fn new() -> Self {
        let mut scenes = HashMap::new();
        let default_scene_id = SceneId(1);
        scenes.insert(
            default_scene_id,
            Scene {
                id: default_scene_id,
                name: "Scene 1".to_string(),
                sources: vec![],
            },
        );

        Self {
            scenes,
            sources: HashMap::new(),
            next_scene_id: 2,
            next_source_id: 1,
            active_scene: Some(default_scene_id),
            streaming: false,
            recording: false,
            encoder_config: EncoderConfig::default(),
        }
    }

    #[allow(dead_code)]
    pub fn get_source(&self, id: SourceId) -> Option<&Source> {
        self.sources.get(&id)
    }

    #[allow(dead_code)]
    pub fn is_streaming(&self) -> bool {
        self.streaming
    }

    #[allow(dead_code)]
    pub fn is_recording(&self) -> bool {
        self.recording
    }

    pub fn active_scene_id(&self) -> Option<SceneId> {
        self.active_scene
    }
}

impl Default for MockObsEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl ObsEngine for MockObsEngine {
    fn scenes(&self) -> Vec<Scene> {
        let mut scenes: Vec<Scene> = self.scenes.values().cloned().collect();
        scenes.sort_by_key(|s| s.id.0);
        scenes
    }

    fn create_scene(&mut self, name: &str) -> Result<SceneId> {
        let id = SceneId(self.next_scene_id);
        self.next_scene_id += 1;
        self.scenes.insert(
            id,
            Scene {
                id,
                name: name.to_string(),
                sources: vec![],
            },
        );
        Ok(id)
    }

    fn remove_scene(&mut self, id: SceneId) -> Result<()> {
        if self.scenes.remove(&id).is_none() {
            return Err(anyhow!("Scene {:?} not found", id));
        }
        if self.active_scene == Some(id) {
            self.active_scene = self.scenes.keys().next().copied();
        }
        Ok(())
    }

    fn set_active_scene(&mut self, id: SceneId) -> Result<()> {
        if !self.scenes.contains_key(&id) {
            return Err(anyhow!("Scene {:?} not found", id));
        }
        self.active_scene = Some(id);
        Ok(())
    }

    fn add_source(&mut self, scene: SceneId, config: SourceConfig) -> Result<SourceId> {
        let id = SourceId(self.next_source_id);
        self.next_source_id += 1;

        let source = Source {
            id,
            name: config.name,
            source_type: config.source_type,
            transform: config.transform,
            visible: true,
            muted: false,
            volume: 1.0,
        };

        self.sources.insert(id, source);

        let scene_entry = self
            .scenes
            .get_mut(&scene)
            .ok_or_else(|| anyhow!("Scene {:?} not found", scene))?;
        scene_entry.sources.push(id);

        Ok(id)
    }

    fn remove_source(&mut self, scene: SceneId, source: SourceId) -> Result<()> {
        let scene_entry = self
            .scenes
            .get_mut(&scene)
            .ok_or_else(|| anyhow!("Scene {:?} not found", scene))?;

        let pos = scene_entry
            .sources
            .iter()
            .position(|&s| s == source)
            .ok_or_else(|| anyhow!("Source {:?} not found in scene {:?}", source, scene))?;

        scene_entry.sources.remove(pos);
        self.sources.remove(&source);

        Ok(())
    }

    fn update_source_transform(&mut self, source: SourceId, transform: Transform) -> Result<()> {
        let entry = self
            .sources
            .get_mut(&source)
            .ok_or_else(|| anyhow!("Source {:?} not found", source))?;
        entry.transform = transform;
        Ok(())
    }

    fn set_volume(&mut self, source: SourceId, volume: f32) -> Result<()> {
        let entry = self
            .sources
            .get_mut(&source)
            .ok_or_else(|| anyhow!("Source {:?} not found", source))?;
        entry.volume = volume;
        Ok(())
    }

    fn set_muted(&mut self, source: SourceId, muted: bool) -> Result<()> {
        let entry = self
            .sources
            .get_mut(&source)
            .ok_or_else(|| anyhow!("Source {:?} not found", source))?;
        entry.muted = muted;
        Ok(())
    }

    fn start_stream(&mut self, _config: StreamConfig) -> Result<()> {
        self.streaming = true;
        Ok(())
    }

    fn stop_stream(&mut self) -> Result<()> {
        self.streaming = false;
        Ok(())
    }

    fn start_recording(&mut self, _path: &Path) -> Result<()> {
        self.recording = true;
        Ok(())
    }

    fn stop_recording(&mut self) -> Result<()> {
        self.recording = false;
        Ok(())
    }

    fn configure_encoder(&mut self, config: EncoderConfig) -> Result<()> {
        self.encoder_config = config;
        Ok(())
    }

    fn subscribe_stats(&self) -> Receiver<ObsStats> {
        let (_, rx) = mpsc::channel(16);
        rx
    }

    fn get_frame(&self) -> Option<RgbaFrame> {
        let width = self.encoder_config.width;
        let height = self.encoder_config.height;
        // Solid dark gray frame (RGBA: 40, 40, 40, 255)
        let size = (width * height * 4) as usize;
        let mut data = vec![0u8; size];
        for chunk in data.chunks_mut(4) {
            chunk[0] = 40;
            chunk[1] = 40;
            chunk[2] = 40;
            chunk[3] = 255;
        }
        Some(RgbaFrame {
            data,
            width,
            height,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::output::StreamDestination;
    use crate::scene::SourceType;
    use super::*;

    #[test]
    fn init_creates_default_scene() {
        let engine = MockObsEngine::new();
        let scenes = engine.scenes();
        assert_eq!(scenes.len(), 1);
        assert_eq!(scenes[0].name, "Scene 1");
    }

    #[test]
    fn create_and_remove_scene() {
        let mut engine = MockObsEngine::new();
        let id = engine.create_scene("Test Scene").unwrap();
        assert_eq!(engine.scenes().len(), 2);
        engine.remove_scene(id).unwrap();
        assert_eq!(engine.scenes().len(), 1);
    }

    #[test]
    fn remove_nonexistent_scene_errors() {
        let mut engine = MockObsEngine::new();
        assert!(engine.remove_scene(SceneId(999)).is_err());
    }

    #[test]
    fn add_source_to_scene() {
        let mut engine = MockObsEngine::new();
        let scenes = engine.scenes();
        let scene_id = scenes[0].id;
        let source_id = engine
            .add_source(
                scene_id,
                SourceConfig {
                    name: "Webcam".to_string(),
                    source_type: SourceType::Camera,
                    transform: Transform::new(0.0, 0.0, 640.0, 480.0),
                },
            )
            .unwrap();
        let scenes = engine.scenes();
        assert!(scenes[0].sources.contains(&source_id));
    }

    #[test]
    fn set_volume_and_mute() {
        let mut engine = MockObsEngine::new();
        let scene_id = engine.scenes()[0].id;
        let source_id = engine
            .add_source(
                scene_id,
                SourceConfig {
                    name: "Mic".to_string(),
                    source_type: SourceType::Audio,
                    transform: Transform::new(0.0, 0.0, 0.0, 0.0),
                },
            )
            .unwrap();
        engine.set_volume(source_id, 0.5).unwrap();
        engine.set_muted(source_id, true).unwrap();
        let source = engine.get_source(source_id).unwrap();
        assert_eq!(source.volume, 0.5);
        assert!(source.muted);
    }

    #[test]
    fn active_scene_management() {
        let mut engine = MockObsEngine::new();
        let scene2 = engine.create_scene("Scene 2").unwrap();
        engine.set_active_scene(scene2).unwrap();
        assert_eq!(engine.active_scene_id(), Some(scene2));
    }

    #[test]
    fn stream_start_stop() {
        let mut engine = MockObsEngine::new();
        assert!(!engine.is_streaming());
        engine
            .start_stream(StreamConfig {
                destination: StreamDestination::Twitch,
                stream_key: "live_test_key".to_string(),
            })
            .unwrap();
        assert!(engine.is_streaming());
        engine.stop_stream().unwrap();
        assert!(!engine.is_streaming());
    }
}
