use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::sync::{mpsc, watch};

use super::error::GstError;
use super::types::{AudioDevice, AudioLevelUpdate, PipelineStats, RgbaFrame};

/// Identifies which audio source a command targets.
#[derive(Debug, Clone, Copy)]
pub enum AudioSourceKind {
    Mic,
    System,
}

/// Audio encoder settings.
#[derive(Debug, Clone)]
pub struct AudioEncoderConfig {
    pub bitrate_kbps: u32,
    pub sample_rate: u32,
    pub channels: u32,
}

impl Default for AudioEncoderConfig {
    fn default() -> Self {
        Self {
            bitrate_kbps: 128,
            sample_rate: 48000,
            channels: 2,
        }
    }
}

/// Commands sent from the UI thread to the GStreamer thread.
#[derive(Debug)]
#[allow(dead_code)]
pub enum GstCommand {
    SetCaptureSource(CaptureSourceConfig),
    StartStream(StreamConfig),
    StopStream,
    StartRecording {
        path: PathBuf,
        format: RecordingFormat,
    },
    StopRecording,
    UpdateEncoder(EncoderConfig),
    SetAudioDevice {
        source: AudioSourceKind,
        device_uid: String,
    },
    SetAudioVolume {
        source: AudioSourceKind,
        volume: f32,
    },
    SetAudioMuted {
        source: AudioSourceKind,
        muted: bool,
    },
    Shutdown,
}

/// Capture source selection.
#[derive(Debug, Clone)]
pub enum CaptureSourceConfig {
    Screen { screen_index: u32 },
}

/// Recording container format.
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub enum RecordingFormat {
    Mkv,
    Mp4,
}

/// RTMP streaming destination.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum StreamDestination {
    Twitch,
    YouTube,
    CustomRtmp { url: String },
}

impl StreamDestination {
    pub fn rtmp_url(&self) -> &str {
        match self {
            Self::Twitch => "rtmp://live.twitch.tv/app",
            Self::YouTube => "rtmp://a.rtmp.youtube.com/live2",
            Self::CustomRtmp { url } => url,
        }
    }
}

/// Stream output configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamConfig {
    pub destination: StreamDestination,
    pub stream_key: String,
}

/// H.264 encoder settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncoderConfig {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub bitrate_kbps: u32,
}

impl Default for EncoderConfig {
    fn default() -> Self {
        Self {
            width: 1920,
            height: 1080,
            fps: 30,
            bitrate_kbps: 4500,
        }
    }
}

/// Channel bundle for communication between the main thread and GStreamer thread.
pub struct GstChannels {
    pub command_tx: mpsc::Sender<GstCommand>,
    pub frame_rx: mpsc::Receiver<RgbaFrame>,
    #[allow(dead_code)]
    pub stats_rx: watch::Receiver<PipelineStats>,
    pub error_rx: mpsc::UnboundedReceiver<GstError>,
    pub audio_level_rx: watch::Receiver<AudioLevelUpdate>,
    pub devices_rx: watch::Receiver<Vec<AudioDevice>>,
}

/// Internal channel handles held by the GStreamer thread.
pub(crate) struct GstThreadChannels {
    pub command_rx: mpsc::Receiver<GstCommand>,
    pub frame_tx: mpsc::Sender<RgbaFrame>,
    #[allow(dead_code)]
    pub stats_tx: watch::Sender<PipelineStats>,
    pub error_tx: mpsc::UnboundedSender<GstError>,
    pub audio_level_tx: watch::Sender<AudioLevelUpdate>,
    pub devices_tx: watch::Sender<Vec<AudioDevice>>,
}

/// Create all channels and return both ends.
pub fn create_channels() -> (GstChannels, GstThreadChannels) {
    let (command_tx, command_rx) = mpsc::channel(16);
    let (frame_tx, frame_rx) = mpsc::channel(2);
    let (stats_tx, stats_rx) = watch::channel(PipelineStats::default());
    let (error_tx, error_rx) = mpsc::unbounded_channel();
    let (audio_level_tx, audio_level_rx) = watch::channel(AudioLevelUpdate::default());
    let (devices_tx, devices_rx) = watch::channel(Vec::new());

    let main_channels = GstChannels {
        command_tx,
        frame_rx,
        stats_rx,
        error_rx,
        audio_level_rx,
        devices_rx,
    };

    let thread_channels = GstThreadChannels {
        command_rx,
        frame_tx,
        stats_tx,
        error_tx,
        audio_level_tx,
        devices_tx,
    };

    (main_channels, thread_channels)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recording_format_debug() {
        assert_eq!(format!("{:?}", RecordingFormat::Mkv), "Mkv");
        assert_eq!(format!("{:?}", RecordingFormat::Mp4), "Mp4");
    }

    #[test]
    fn capture_source_config_screen() {
        let config = CaptureSourceConfig::Screen { screen_index: 0 };
        assert!(matches!(
            config,
            CaptureSourceConfig::Screen { screen_index: 0 }
        ));
    }

    #[test]
    fn stream_destination_rtmp_urls() {
        assert_eq!(
            StreamDestination::Twitch.rtmp_url(),
            "rtmp://live.twitch.tv/app"
        );
        assert_eq!(
            StreamDestination::YouTube.rtmp_url(),
            "rtmp://a.rtmp.youtube.com/live2"
        );
        let custom = StreamDestination::CustomRtmp {
            url: "rtmp://my.server/live".to_string(),
        };
        assert_eq!(custom.rtmp_url(), "rtmp://my.server/live");
    }

    #[test]
    fn encoder_config_default() {
        let config = EncoderConfig::default();
        assert_eq!(config.width, 1920);
        assert_eq!(config.height, 1080);
        assert_eq!(config.fps, 30);
        assert_eq!(config.bitrate_kbps, 4500);
    }

    #[test]
    fn create_channels_returns_valid_handles() {
        let (main_ch, _thread_ch) = create_channels();
        main_ch.command_tx.try_send(GstCommand::Shutdown).unwrap();
    }
}
