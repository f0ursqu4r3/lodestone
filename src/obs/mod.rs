pub mod encoder;
pub mod mock;
pub mod output;
pub mod scene;

use anyhow::Result;
use std::path::Path;
use tokio::sync::mpsc::Receiver;

pub use encoder::EncoderConfig;
pub use output::{StreamConfig, StreamDestination};
pub use scene::{Scene, SceneId, Source, SourceConfig, SourceId, SourceType, Transform};

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct ObsStats {
    pub bitrate_kbps: f64,
    pub dropped_frames: u64,
    pub total_frames: u64,
    pub uptime_secs: f64,
}

#[derive(Debug, Clone)]
pub struct RgbaFrame {
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

#[allow(dead_code)]
pub trait ObsEngine {
    fn scenes(&self) -> Vec<Scene>;
    fn create_scene(&mut self, name: &str) -> Result<SceneId>;
    fn remove_scene(&mut self, id: SceneId) -> Result<()>;
    fn set_active_scene(&mut self, id: SceneId) -> Result<()>;
    fn add_source(&mut self, scene: SceneId, source: SourceConfig) -> Result<SourceId>;
    fn remove_source(&mut self, scene: SceneId, source: SourceId) -> Result<()>;
    fn update_source_transform(&mut self, source: SourceId, transform: Transform) -> Result<()>;
    fn set_volume(&mut self, source: SourceId, volume: f32) -> Result<()>;
    fn set_muted(&mut self, source: SourceId, muted: bool) -> Result<()>;
    fn start_stream(&mut self, config: StreamConfig) -> Result<()>;
    fn stop_stream(&mut self) -> Result<()>;
    fn start_recording(&mut self, path: &Path) -> Result<()>;
    fn stop_recording(&mut self) -> Result<()>;
    fn configure_encoder(&mut self, config: EncoderConfig) -> Result<()>;
    fn subscribe_stats(&self) -> Receiver<ObsStats>;
    fn get_frame(&self) -> Option<RgbaFrame>;
}
