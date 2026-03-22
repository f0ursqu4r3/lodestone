pub mod capture;
pub mod commands;
pub mod encode;
pub mod error;
pub mod thread;
pub mod types;

pub use commands::{
    CaptureSourceConfig, EncoderConfig, GstChannels, GstCommand, RecordingFormat, StreamConfig,
    StreamDestination, create_channels,
};
pub use error::GstError;
pub use thread::spawn_gstreamer_thread;
pub use types::{PipelineStats, RgbaFrame};
