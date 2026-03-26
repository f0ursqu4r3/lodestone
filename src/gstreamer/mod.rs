pub mod capture;
pub mod commands;
pub mod devices;
pub mod encode;
pub mod error;
#[cfg(target_os = "macos")]
#[allow(non_snake_case)]
pub mod screencapturekit;
pub mod thread;
pub mod types;
#[cfg(target_os = "macos")]
pub mod virtual_camera;

#[allow(unused_imports)]
pub use commands::{
    AudioEncoderConfig, AudioSourceKind, CaptureSourceConfig, GstChannels, GstCommand,
    RecordingFormat, StreamConfig, StreamDestination, create_channels,
};
pub use devices::{CameraDevice, DisplayInfo, WindowInfo};
pub use error::GstError;
pub use thread::spawn_gstreamer_thread;
#[allow(unused_imports)]
pub use types::{AudioDevice, AudioLevelUpdate, AudioLevels, RgbaFrame};
