use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::sync::{mpsc, watch};

use super::error::GstError;
use super::types::{AudioDevice, AudioLevelUpdate, OutputRuntimeState, PipelineStats, RgbaFrame};
use crate::scene::{AudioEffectInstance, SourceId};

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
pub enum GstCommand {
    StartStream {
        destination: StreamDestination,
        stream_key: String,
        encoder_config: EncoderConfig,
    },
    StopStream,
    StartRecording {
        path: PathBuf,
        format: RecordingFormat,
        encoder_config: EncoderConfig,
    },
    StopRecording,
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
    StopCapture,
    AddCaptureSource {
        source_id: SourceId,
        config: CaptureSourceConfig,
        fps: u32,
    },
    RemoveCaptureSource {
        source_id: SourceId,
    },
    /// Push a decoded image frame directly into the shared frame map (no capture pipeline).
    LoadImageFrame {
        source_id: SourceId,
        frame: RgbaFrame,
    },
    /// Update display capture exclusion on all active display sources.
    UpdateDisplayExclusion {
        exclude_self: bool,
    },
    StartVirtualCamera {
        fps: u32,
    },
    StopVirtualCamera,
    /// Per-source volume control (distinct from global SetAudioVolume).
    SetSourceVolume {
        source_id: SourceId,
        volume: f32,
    },
    /// Per-source mute (distinct from global SetAudioMuted).
    SetSourceMuted {
        source_id: SourceId,
        muted: bool,
    },
    /// Replace the pre-fader audio effect chain for a source.
    ///
    /// If the chain's structural shape (kinds in order, enabled flags) matches
    /// the currently-running pipeline, effect parameters are updated live.
    /// Otherwise the audio pipeline is rebuilt with the new chain.
    SetSourceAudioEffects {
        source_id: SourceId,
        effects: Vec<AudioEffectInstance>,
    },
    /// Trigger foreground window capture for all ForegroundOnHotkey sources.
    CaptureForegroundWindow,
    #[allow(dead_code)]
    Shutdown,
}

/// Capture source selection.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum CaptureSourceConfig {
    Screen {
        screen_index: u32,
        exclude_self: bool,
        /// Capture resolution (width, height). Derived from base_resolution setting.
        capture_size: (u32, u32),
    },
    Window {
        mode: crate::scene::WindowCaptureMode,
        /// Capture resolution (width, height). Derived from base_resolution setting.
        capture_size: (u32, u32),
    },
    /// Window capture via HWND (Windows). Resolved from WindowCaptureMode at runtime.
    WindowHandle {
        hwnd: u64,
        /// Capture resolution (width, height). Derived from base_resolution setting.
        capture_size: (u32, u32),
    },
    Camera {
        device_index: u32,
    },
    AudioDevice {
        device_uid: String,
        /// Pre-fader effect chain to insert into the capture pipeline.
        /// Empty if no effects are configured.
        #[allow(dead_code)]
        effects: Vec<AudioEffectInstance>,
    },
    AudioFile {
        path: String,
        looping: bool,
        /// Pre-fader effect chain to insert into the playback pipeline.
        #[allow(dead_code)]
        effects: Vec<AudioEffectInstance>,
    },
    /// Game capture via DLL injection + DirectX hooking (Windows only).
    GameCapture {
        /// Current target PID if already resolved, or 0 to let the backend wait and resolve later.
        process_id: u32,
        process_name: String,
        /// Optional window title used to prefer the same game window after relaunch.
        window_title: String,
    },
}

/// Recording container format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecordingFormat {
    Mkv,
    Mp4,
}

/// Available H.264 encoder backends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EncoderType {
    H264VideoToolbox,
    H264x264,
    H264Nvenc,
    H264Amf,
    H264Qsv,
}

impl EncoderType {
    pub fn element_name(&self) -> &'static str {
        match self {
            Self::H264VideoToolbox => "vtenc_h264",
            Self::H264x264 => "x264enc",
            Self::H264Nvenc => "nvh264enc",
            Self::H264Amf => "amfh264enc",
            Self::H264Qsv => "qsvh264enc",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::H264VideoToolbox => "VideoToolbox (Hardware)",
            Self::H264x264 => "x264 (Software)",
            Self::H264Nvenc => "NVENC (Hardware)",
            Self::H264Amf => "AMF (Hardware)",
            Self::H264Qsv => "QuickSync (Hardware)",
        }
    }

    pub fn is_hardware(&self) -> bool {
        !matches!(self, Self::H264x264)
    }

    pub fn all() -> &'static [EncoderType] {
        #[cfg(target_os = "macos")]
        {
            &[
                Self::H264VideoToolbox,
                Self::H264Nvenc,
                Self::H264Amf,
                Self::H264Qsv,
                Self::H264x264,
            ]
        }
        #[cfg(not(target_os = "macos"))]
        {
            // VideoToolbox is macOS-only; prefer hardware encoders common on Windows/Linux.
            &[
                Self::H264Nvenc,
                Self::H264Amf,
                Self::H264Qsv,
                Self::H264x264,
            ]
        }
    }
}

/// Named quality presets mapping to bitrate values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum QualityPreset {
    Low,
    Medium,
    High,
    Custom,
}

impl QualityPreset {
    pub fn bitrate_kbps(&self) -> u32 {
        match self {
            Self::Low => 2500,
            Self::Medium => 4500,
            Self::High => 8000,
            Self::Custom => 0,
        }
    }

    pub fn all() -> &'static [QualityPreset] {
        &[Self::Low, Self::Medium, Self::High, Self::Custom]
    }
}

/// An encoder detected as available at startup.
#[derive(Debug, Clone)]
pub struct AvailableEncoder {
    pub encoder_type: EncoderType,
    pub is_recommended: bool,
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

/// H.264 encoder settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncoderConfig {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub bitrate_kbps: u32,
    pub encoder_type: EncoderType,
    pub color_space: String,
}

impl Default for EncoderConfig {
    fn default() -> Self {
        Self {
            width: 1920,
            height: 1080,
            fps: 30,
            bitrate_kbps: 4500,
            encoder_type: EncoderType::H264VideoToolbox,
            color_space: "sRGB".to_string(),
        }
    }
}

/// Channel bundle for communication between the main thread and GStreamer thread.
pub struct GstChannels {
    pub command_tx: mpsc::Sender<GstCommand>,
    /// Shared map of the latest frame received per source. Written by the GStreamer thread,
    /// read by the render thread.
    pub latest_frames: Arc<Mutex<HashMap<SourceId, RgbaFrame>>>,
    /// Composited output frames produced by the compositor, sent to preview rendering.
    #[allow(dead_code)]
    pub composited_frame_tx: mpsc::Sender<RgbaFrame>,
    #[allow(dead_code)]
    pub stats_rx: watch::Receiver<PipelineStats>,
    pub error_rx: mpsc::UnboundedReceiver<GstError>,
    pub audio_level_rx: watch::Receiver<AudioLevelUpdate>,
    pub devices_rx: watch::Receiver<Vec<AudioDevice>>,
    pub encoders_rx: watch::Receiver<Vec<AvailableEncoder>>,
    pub runtime_state_rx: watch::Receiver<OutputRuntimeState>,
}

/// Internal channel handles held by the GStreamer thread.
pub(crate) struct GstThreadChannels {
    pub command_rx: mpsc::Receiver<GstCommand>,
    /// Shared map of the latest frame per source. Written by this thread.
    pub latest_frames: Arc<Mutex<HashMap<SourceId, RgbaFrame>>>,
    /// Composited frames consumed by the GStreamer encode pipeline.
    #[allow(dead_code)]
    pub composited_frame_rx: mpsc::Receiver<RgbaFrame>,
    #[allow(dead_code)]
    pub stats_tx: watch::Sender<PipelineStats>,
    pub error_tx: mpsc::UnboundedSender<GstError>,
    pub audio_level_tx: watch::Sender<AudioLevelUpdate>,
    pub devices_tx: watch::Sender<Vec<AudioDevice>>,
    pub encoders_tx: watch::Sender<Vec<AvailableEncoder>>,
    pub runtime_state_tx: watch::Sender<OutputRuntimeState>,
}

/// Create all channels and return both ends.
pub fn create_channels() -> (GstChannels, GstThreadChannels) {
    let (command_tx, command_rx) = mpsc::channel(64);
    let latest_frames = Arc::new(Mutex::new(HashMap::new()));
    let (composited_frame_tx, composited_frame_rx) = mpsc::channel(16);
    let (stats_tx, stats_rx) = watch::channel(PipelineStats::default());
    let (error_tx, error_rx) = mpsc::unbounded_channel();
    let (audio_level_tx, audio_level_rx) = watch::channel(AudioLevelUpdate::default());
    let (devices_tx, devices_rx) = watch::channel(Vec::new());
    let (encoders_tx, encoders_rx) = watch::channel(Vec::new());
    let (runtime_state_tx, runtime_state_rx) = watch::channel(OutputRuntimeState::default());

    let main_channels = GstChannels {
        command_tx,
        latest_frames: Arc::clone(&latest_frames),
        composited_frame_tx,
        stats_rx,
        error_rx,
        audio_level_rx,
        devices_rx,
        encoders_rx,
        runtime_state_rx,
    };

    let thread_channels = GstThreadChannels {
        command_rx,
        latest_frames,
        composited_frame_rx,
        stats_tx,
        error_tx,
        audio_level_tx,
        devices_tx,
        encoders_tx,
        runtime_state_tx,
    };

    (main_channels, thread_channels)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::SourceId;

    #[test]
    fn create_channels_with_shared_frames() {
        let (main_ch, thread_ch) = create_channels();
        let source_id = SourceId(1);
        let frame = RgbaFrame {
            data: vec![0u8; 4],
            width: 1,
            height: 1,
        };
        thread_ch
            .latest_frames
            .lock()
            .unwrap()
            .insert(source_id, frame);
        let frames = main_ch.latest_frames.lock().unwrap();
        assert!(frames.contains_key(&source_id));
    }

    #[test]
    fn composited_frame_channel_works() {
        let (main_ch, _thread_ch) = create_channels();
        let frame = RgbaFrame {
            data: vec![0u8; 4],
            width: 1,
            height: 1,
        };
        main_ch.composited_frame_tx.try_send(frame).unwrap();
    }

    #[test]
    fn recording_format_debug() {
        assert_eq!(format!("{:?}", RecordingFormat::Mkv), "Mkv");
        assert_eq!(format!("{:?}", RecordingFormat::Mp4), "Mp4");
    }

    #[test]
    fn capture_source_config_screen() {
        let config = CaptureSourceConfig::Screen {
            screen_index: 0,
            exclude_self: false,
            capture_size: (1920, 1080),
        };
        assert!(matches!(
            config,
            CaptureSourceConfig::Screen {
                screen_index: 0,
                ..
            }
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
    fn encoder_type_gstreamer_element_name() {
        assert_eq!(EncoderType::H264VideoToolbox.element_name(), "vtenc_h264");
        assert_eq!(EncoderType::H264x264.element_name(), "x264enc");
        assert_eq!(EncoderType::H264Nvenc.element_name(), "nvh264enc");
        assert_eq!(EncoderType::H264Amf.element_name(), "amfh264enc");
        assert_eq!(EncoderType::H264Qsv.element_name(), "qsvh264enc");
    }

    #[test]
    fn encoder_type_display_name() {
        assert_eq!(
            EncoderType::H264VideoToolbox.display_name(),
            "VideoToolbox (Hardware)"
        );
        assert_eq!(EncoderType::H264x264.display_name(), "x264 (Software)");
    }

    #[test]
    fn encoder_type_is_hardware() {
        assert!(EncoderType::H264VideoToolbox.is_hardware());
        assert!(!EncoderType::H264x264.is_hardware());
        assert!(EncoderType::H264Nvenc.is_hardware());
    }

    #[test]
    fn quality_preset_to_bitrate() {
        assert_eq!(QualityPreset::Low.bitrate_kbps(), 2500);
        assert_eq!(QualityPreset::Medium.bitrate_kbps(), 4500);
        assert_eq!(QualityPreset::High.bitrate_kbps(), 8000);
    }

    #[test]
    fn quality_preset_custom_returns_none() {
        assert_eq!(QualityPreset::Custom.bitrate_kbps(), 0);
    }

    #[test]
    fn create_channels_returns_valid_handles() {
        let (main_ch, _thread_ch) = create_channels();
        main_ch.command_tx.try_send(GstCommand::Shutdown).unwrap();
    }
}
