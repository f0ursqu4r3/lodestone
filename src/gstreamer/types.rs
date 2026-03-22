/// Raw RGBA frame data from the capture pipeline.
#[derive(Debug, Clone)]
pub struct RgbaFrame {
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// Pipeline statistics sent periodically from the GStreamer thread.
#[derive(Debug, Clone)]
pub struct PipelineStats {
    pub bitrate_kbps: f64,
    pub dropped_frames: u64,
    pub total_frames: u64,
    pub uptime_secs: f64,
}

impl Default for PipelineStats {
    fn default() -> Self {
        Self {
            bitrate_kbps: 0.0,
            dropped_frames: 0,
            total_frames: 0,
            uptime_secs: 0.0,
        }
    }
}
