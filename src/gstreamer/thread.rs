use gstreamer::prelude::*;
use gstreamer_app::{AppSink, AppSrc};
use log;
use std::thread::JoinHandle;

use super::capture::{build_audio_capture_pipeline, build_capture_pipeline};
use super::commands::{
    AudioEncoderConfig, AudioSourceKind, CaptureSourceConfig, GstCommand, GstThreadChannels,
};
use crate::scene::SourceId;
use super::encode::{
    RecordPipelineHandles, StreamPipelineHandles, build_record_pipeline_with_audio,
    build_stream_pipeline_with_audio,
};
use super::error::GstError;
use super::types::RgbaFrame;

#[derive(Debug)]
enum PipelineKind {
    Stream,
    Record,
}

/// State held by the GStreamer thread.
struct GstThread {
    channels: GstThreadChannels,
    capture_pipeline: Option<gstreamer::Pipeline>,
    capture_appsink: Option<AppSink>,
    // Encode pipeline handles
    stream_handles: Option<StreamPipelineHandles>,
    record_handles: Option<RecordPipelineHandles>,
    // Audio capture
    mic_pipeline: Option<gstreamer::Pipeline>,
    mic_appsink: Option<AppSink>,
    mic_volume_name: Option<String>,
    system_pipeline: Option<gstreamer::Pipeline>,
    system_appsink: Option<AppSink>,
    system_volume_name: Option<String>,
    has_system_audio: bool,
    // Config
    encoder_config: super::commands::EncoderConfig,
    audio_encoder_config: AudioEncoderConfig,
}

impl GstThread {
    fn new(channels: GstThreadChannels) -> Self {
        Self {
            channels,
            capture_pipeline: None,
            capture_appsink: None,
            stream_handles: None,
            record_handles: None,
            mic_pipeline: None,
            mic_appsink: None,
            mic_volume_name: None,
            system_pipeline: None,
            system_appsink: None,
            system_volume_name: None,
            has_system_audio: false,
            encoder_config: super::commands::EncoderConfig::default(),
            audio_encoder_config: AudioEncoderConfig::default(),
        }
    }

    /// Start capturing from the given source.
    fn start_capture(&mut self, source: &CaptureSourceConfig) {
        self.stop_capture();

        match build_capture_pipeline(
            source,
            self.encoder_config.width,
            self.encoder_config.height,
            self.encoder_config.fps,
        ) {
            Ok((pipeline, appsink)) => {
                if let Err(e) = pipeline.set_state(gstreamer::State::Playing) {
                    let _ = self.channels.error_tx.send(GstError::CaptureFailure {
                        message: format!("Failed to start capture: {e}"),
                    });
                    return;
                }
                self.capture_pipeline = Some(pipeline);
                self.capture_appsink = Some(appsink);
                log::info!("Capture pipeline started");
            }
            Err(e) => {
                let _ = self.channels.error_tx.send(GstError::CaptureFailure {
                    message: format!("{e}"),
                });
            }
        }
    }

    fn stop_capture(&mut self) {
        if let Some(pipeline) = self.capture_pipeline.take() {
            let _ = pipeline.set_state(gstreamer::State::Null);
        }
        self.capture_appsink = None;
    }

    /// Start audio capture for the given source kind and device.
    fn start_audio_capture(&mut self, kind: AudioSourceKind, device_uid: &str) {
        self.stop_audio_capture(kind);

        match build_audio_capture_pipeline(kind, device_uid, self.audio_encoder_config.sample_rate)
        {
            Ok((pipeline, appsink, volume_name)) => {
                log::info!("Starting {kind:?} audio pipeline for device '{device_uid}'");
                if let Err(e) = pipeline.set_state(gstreamer::State::Playing) {
                    // Check the bus for more detailed error info
                    if let Some(bus) = pipeline.bus()
                        && let Some(msg) = bus.timed_pop_filtered(
                            gstreamer::ClockTime::from_mseconds(100),
                            &[gstreamer::MessageType::Error],
                        )
                        && let gstreamer::MessageView::Error(err) = msg.view()
                    {
                        log::error!("Audio pipeline error detail: {}", err.error());
                        if let Some(debug) = err.debug() {
                            log::error!("Audio pipeline debug: {debug}");
                        }
                    }
                    let _ = pipeline.set_state(gstreamer::State::Null);
                    let _ = self.channels.error_tx.send(GstError::AudioCaptureFailure {
                        message: format!("Failed to start {kind:?} audio capture: {e}"),
                    });
                    return;
                }
                match kind {
                    AudioSourceKind::Mic => {
                        self.mic_pipeline = Some(pipeline);
                        self.mic_appsink = Some(appsink);
                        self.mic_volume_name = Some(volume_name);
                    }
                    AudioSourceKind::System => {
                        self.system_pipeline = Some(pipeline);
                        self.system_appsink = Some(appsink);
                        self.system_volume_name = Some(volume_name);
                        self.has_system_audio = true;
                    }
                }
                log::info!("{kind:?} audio capture started for device {device_uid}");
            }
            Err(e) => {
                let _ = self.channels.error_tx.send(GstError::AudioCaptureFailure {
                    message: format!("{e}"),
                });
            }
        }
    }

    /// Stop audio capture for the given source kind.
    fn stop_audio_capture(&mut self, kind: AudioSourceKind) {
        match kind {
            AudioSourceKind::Mic => {
                if let Some(pipeline) = self.mic_pipeline.take() {
                    let _ = pipeline.set_state(gstreamer::State::Null);
                }
                self.mic_appsink = None;
                self.mic_volume_name = None;
            }
            AudioSourceKind::System => {
                if let Some(pipeline) = self.system_pipeline.take() {
                    let _ = pipeline.set_state(gstreamer::State::Null);
                }
                self.system_appsink = None;
                self.system_volume_name = None;
                self.has_system_audio = false;
            }
        }
    }

    /// Push a frame buffer to an active encode appsrc.
    fn push_to_encode(appsrc: &AppSrc, data: &[u8], pts: gstreamer::ClockTime) {
        let mut buffer = gstreamer::Buffer::with_size(data.len()).unwrap();
        {
            let buffer_ref = buffer.get_mut().unwrap();
            buffer_ref.set_pts(pts);
            let mut map = buffer_ref.map_writable().unwrap();
            map.as_mut_slice().copy_from_slice(data);
        }
        let _ = appsrc.push_buffer(buffer);
    }

    fn handle_command(&mut self, cmd: GstCommand) -> bool {
        match cmd {
            GstCommand::SetCaptureSource(source) => {
                self.start_capture(&source);
            }
            GstCommand::StartStream(config) => {
                let url = format!("{}/{}", config.destination.rtmp_url(), config.stream_key);
                match build_stream_pipeline_with_audio(
                    &self.encoder_config,
                    &self.audio_encoder_config,
                    &url,
                    self.has_system_audio,
                ) {
                    Ok(handles) => {
                        if let Err(e) = handles.pipeline.set_state(gstreamer::State::Playing) {
                            let _ = self.channels.error_tx.send(GstError::EncodeFailure {
                                message: format!("Failed to start stream: {e}"),
                            });
                            return false;
                        }
                        log::info!("Stream pipeline started");
                        self.stream_handles = Some(handles);
                    }
                    Err(e) => {
                        let _ = self.channels.error_tx.send(GstError::EncodeFailure {
                            message: format!("{e}"),
                        });
                    }
                }
            }
            GstCommand::StopStream => {
                self.stop_pipeline(PipelineKind::Stream);
            }
            GstCommand::StopRecording => {
                self.stop_pipeline(PipelineKind::Record);
            }
            GstCommand::StartRecording { path, format } => {
                match build_record_pipeline_with_audio(
                    &self.encoder_config,
                    &self.audio_encoder_config,
                    &path,
                    format,
                    self.has_system_audio,
                ) {
                    Ok(handles) => {
                        if let Err(e) = handles.pipeline.set_state(gstreamer::State::Playing) {
                            let _ = self.channels.error_tx.send(GstError::EncodeFailure {
                                message: format!("Failed to start recording: {e}"),
                            });
                            return false;
                        }
                        log::info!("Record pipeline started to {}", path.display());
                        self.record_handles = Some(handles);
                    }
                    Err(e) => {
                        let _ = self.channels.error_tx.send(GstError::EncodeFailure {
                            message: format!("{e}"),
                        });
                    }
                }
            }
            GstCommand::UpdateEncoder(config) => {
                self.encoder_config = config;
            }
            GstCommand::SetAudioDevice { source, device_uid } => {
                self.stop_audio_capture(source);
                self.start_audio_capture(source, &device_uid);
            }
            GstCommand::SetAudioVolume { source, volume } => {
                let (pipeline, vol_name) = match source {
                    AudioSourceKind::Mic => (&self.mic_pipeline, &self.mic_volume_name),
                    AudioSourceKind::System => (&self.system_pipeline, &self.system_volume_name),
                };
                if let (Some(pipeline), Some(name)) = (pipeline, vol_name)
                    && let Some(element) = pipeline.by_name(name)
                {
                    element.set_property("volume", volume as f64);
                }
            }
            GstCommand::SetAudioMuted { source, muted } => {
                let (pipeline, vol_name) = match source {
                    AudioSourceKind::Mic => (&self.mic_pipeline, &self.mic_volume_name),
                    AudioSourceKind::System => (&self.system_pipeline, &self.system_volume_name),
                };
                if let (Some(pipeline), Some(name)) = (pipeline, vol_name)
                    && let Some(element) = pipeline.by_name(name)
                {
                    element.set_property("mute", muted);
                }
            }
            GstCommand::StopCapture => {
                self.stop_capture();
                log::info!("Capture stopped");
            }
            GstCommand::AddCaptureSource { source_id, config } => {
                // Placeholder: per-source pipeline management is wired in Task 4.
                log::info!("AddCaptureSource {source_id:?} — deferring to Task 4");
                self.start_capture(&config);
            }
            GstCommand::RemoveCaptureSource { source_id } => {
                // Placeholder: per-source pipeline management is wired in Task 4.
                log::info!("RemoveCaptureSource {source_id:?} — deferring to Task 4");
                self.stop_capture();
            }
            GstCommand::Shutdown => {
                self.stop_pipeline(PipelineKind::Stream);
                self.stop_pipeline(PipelineKind::Record);
                self.stop_audio_capture(AudioSourceKind::Mic);
                self.stop_audio_capture(AudioSourceKind::System);
                self.stop_capture();
                return true;
            }
        }
        false
    }

    fn stop_pipeline(&mut self, kind: PipelineKind) {
        match kind {
            PipelineKind::Stream => {
                if let Some(handles) = self.stream_handles.take() {
                    let _ = handles.video_appsrc.end_of_stream();
                    let _ = handles.audio_appsrc_mic.end_of_stream();
                    if let Some(ref sys) = handles.audio_appsrc_system {
                        let _ = sys.end_of_stream();
                    }
                    let bus = handles.pipeline.bus().unwrap();
                    let _ = bus.timed_pop_filtered(
                        gstreamer::ClockTime::from_seconds(2),
                        &[gstreamer::MessageType::Eos],
                    );
                    let _ = handles.pipeline.set_state(gstreamer::State::Null);
                }
            }
            PipelineKind::Record => {
                if let Some(handles) = self.record_handles.take() {
                    let _ = handles.video_appsrc.end_of_stream();
                    let _ = handles.mic_appsrc.end_of_stream();
                    if let Some(ref sys) = handles.system_appsrc {
                        let _ = sys.end_of_stream();
                    }
                    let bus = handles.pipeline.bus().unwrap();
                    let _ = bus.timed_pop_filtered(
                        gstreamer::ClockTime::from_seconds(2),
                        &[gstreamer::MessageType::Eos],
                    );
                    let _ = handles.pipeline.set_state(gstreamer::State::Null);
                }
            }
        }
        log::info!("{:?} pipeline stopped", kind);
    }

    /// Read audio level information from a pipeline's bus messages.
    fn read_level_from_bus(
        pipeline: &gstreamer::Pipeline,
    ) -> Option<crate::gstreamer::types::AudioLevels> {
        let bus = pipeline.bus()?;
        let mut result = None;
        while let Some(msg) = bus.pop() {
            if let gstreamer::MessageView::Element(elem) = msg.view()
                && let Some(structure) = elem.structure()
                && structure.name().as_str() == "level"
            {
                let peak = structure
                    .get::<gstreamer::glib::ValueArray>("peak")
                    .ok()
                    .and_then(|arr| arr.as_slice().first().and_then(|v| v.get::<f64>().ok()))
                    .unwrap_or(-60.0) as f32;
                let rms = structure
                    .get::<gstreamer::glib::ValueArray>("rms")
                    .ok()
                    .and_then(|arr| arr.as_slice().first().and_then(|v| v.get::<f64>().ok()))
                    .unwrap_or(-60.0) as f32;
                result = Some(crate::gstreamer::types::AudioLevels {
                    peak_db: peak,
                    rms_db: rms,
                });
            }
        }
        result
    }

    /// Poll audio levels from capture pipelines and send updates.
    fn poll_audio_levels(&self) {
        let mut update = crate::gstreamer::types::AudioLevelUpdate::default();
        if let Some(ref pipeline) = self.mic_pipeline {
            update.mic = Self::read_level_from_bus(pipeline);
        }
        if let Some(ref pipeline) = self.system_pipeline {
            update.system = Self::read_level_from_bus(pipeline);
        }
        if update.mic.is_some() || update.system.is_some() {
            let _ = self.channels.audio_level_tx.send(update);
        }
    }

    /// Pull audio samples from a capture appsink and forward to encode appsrcs.
    fn forward_audio(
        appsink: &AppSink,
        stream_appsrc: Option<&AppSrc>,
        record_appsrc: Option<&AppSrc>,
        pts: gstreamer::ClockTime,
    ) {
        if let Some(sample) = appsink.try_pull_sample(gstreamer::ClockTime::from_mseconds(0))
            && let Some(buffer) = sample.buffer()
            && let Ok(map) = buffer.map_readable()
        {
            let data = map.as_slice();
            if let Some(appsrc) = stream_appsrc {
                Self::push_to_encode(appsrc, data, pts);
            }
            if let Some(appsrc) = record_appsrc {
                Self::push_to_encode(appsrc, data, pts);
            }
        }
    }

    /// Main run loop for the GStreamer thread.
    fn run(mut self) {
        // Enumerate audio devices and start default audio capture.
        match super::devices::enumerate_audio_input_devices() {
            Ok(devices) => {
                let _ = self.channels.devices_tx.send(devices.clone());
                if let Some(mic) = devices.iter().find(|d| !d.is_loopback) {
                    self.start_audio_capture(AudioSourceKind::Mic, &mic.uid);
                }
                if let Some(loopback) = devices.iter().find(|d| d.is_loopback) {
                    self.start_audio_capture(AudioSourceKind::System, &loopback.uid);
                }
            }
            Err(e) => log::warn!("Failed to enumerate audio devices: {e}"),
        }

        let start_time = std::time::Instant::now();

        loop {
            // Check for commands (non-blocking)
            match self.channels.command_rx.try_recv() {
                Ok(cmd) => {
                    if self.handle_command(cmd) {
                        return; // Shutdown
                    }
                }
                Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                    log::info!("Command channel disconnected, shutting down GStreamer thread");
                    self.stop_pipeline(PipelineKind::Stream);
                    self.stop_pipeline(PipelineKind::Record);
                    self.stop_audio_capture(AudioSourceKind::Mic);
                    self.stop_audio_capture(AudioSourceKind::System);
                    self.stop_capture();
                    return;
                }
                Err(tokio::sync::mpsc::error::TryRecvError::Empty) => {}
            }

            let pts = gstreamer::ClockTime::from_nseconds(start_time.elapsed().as_nanos() as u64);

            // Pull frame from capture, forward to preview and encode pipelines
            if let Some(appsink) = &self.capture_appsink
                && let Some(sample) =
                    appsink.try_pull_sample(gstreamer::ClockTime::from_mseconds(0))
            {
                let (width, height) = sample
                    .caps()
                    .and_then(|caps| gstreamer_video::VideoInfo::from_caps(caps).ok())
                    .map(|info| (info.width(), info.height()))
                    .unwrap_or((self.encoder_config.width, self.encoder_config.height));

                if let Some(buffer) = sample.buffer()
                    && let Ok(map) = buffer.map_readable()
                {
                    let data = map.as_slice();

                    // Send to preview — temporary: all frames keyed under SourceId(0)
                    // until per-source capture is wired in Task 4.
                    let frame = RgbaFrame {
                        data: data.to_vec(),
                        width,
                        height,
                    };
                    self.channels
                        .latest_frames
                        .lock()
                        .unwrap()
                        .insert(SourceId(0), frame);

                    // Feed active encode pipelines (video)
                    if let Some(ref handles) = self.stream_handles {
                        Self::push_to_encode(&handles.video_appsrc, data, pts);
                    }
                    if let Some(ref handles) = self.record_handles {
                        Self::push_to_encode(&handles.video_appsrc, data, pts);
                    }
                }
            }

            // Forward mic audio to encode pipelines
            if let Some(ref appsink) = self.mic_appsink {
                let stream_appsrc = self.stream_handles.as_ref().map(|h| &h.audio_appsrc_mic);
                let record_appsrc = self.record_handles.as_ref().map(|h| &h.mic_appsrc);
                Self::forward_audio(appsink, stream_appsrc, record_appsrc, pts);
            }

            // Forward system audio to encode pipelines
            if let Some(ref appsink) = self.system_appsink {
                let stream_appsrc = self
                    .stream_handles
                    .as_ref()
                    .and_then(|h| h.audio_appsrc_system.as_ref());
                let record_appsrc = self
                    .record_handles
                    .as_ref()
                    .and_then(|h| h.system_appsrc.as_ref());
                Self::forward_audio(appsink, stream_appsrc, record_appsrc, pts);
            }

            // Poll audio levels
            self.poll_audio_levels();

            // Brief sleep to avoid busy-spinning
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }
}

/// Spawn the GStreamer thread. Returns a join handle.
///
/// Call this from `AppManager::new()`. The thread initializes GStreamer,
/// starts the default screen capture, and listens for commands.
pub fn spawn_gstreamer_thread(channels: GstThreadChannels) -> JoinHandle<()> {
    std::thread::Builder::new()
        .name("gstreamer".to_string())
        .spawn(move || {
            if let Err(e) = gstreamer::init() {
                log::error!("Failed to initialize GStreamer: {e}");
                return;
            }
            log::info!("GStreamer initialized on dedicated thread");

            let thread = GstThread::new(channels);
            thread.run();

            log::info!("GStreamer thread exiting");
        })
        .expect("spawn GStreamer thread")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gstreamer::commands::create_channels;

    #[test]
    fn pipeline_kind_debug() {
        assert_eq!(format!("{:?}", PipelineKind::Stream), "Stream");
        assert_eq!(format!("{:?}", PipelineKind::Record), "Record");
    }

    #[test]
    fn gst_thread_new_has_defaults() {
        let (_main_ch, thread_ch) = create_channels();
        let thread = GstThread::new(thread_ch);
        assert!(thread.capture_pipeline.is_none());
        assert!(thread.capture_appsink.is_none());
        assert!(thread.stream_handles.is_none());
        assert!(thread.record_handles.is_none());
        assert!(thread.mic_pipeline.is_none());
        assert!(thread.system_pipeline.is_none());
        assert!(thread.mic_appsink.is_none());
        assert!(thread.system_appsink.is_none());
        assert!(!thread.has_system_audio);
        assert_eq!(thread.encoder_config.width, 1920);
        assert_eq!(thread.encoder_config.height, 1080);
        assert_eq!(thread.encoder_config.fps, 30);
    }

    #[test]
    fn handle_shutdown_returns_true() {
        let (_main_ch, thread_ch) = create_channels();
        let mut thread = GstThread::new(thread_ch);
        assert!(thread.handle_command(GstCommand::Shutdown));
    }

    #[test]
    fn handle_update_encoder_stores_config() {
        use crate::gstreamer::commands::EncoderConfig;
        let (_main_ch, thread_ch) = create_channels();
        let mut thread = GstThread::new(thread_ch);
        let new_config = EncoderConfig {
            width: 1280,
            height: 720,
            fps: 60,
            bitrate_kbps: 6000,
        };
        assert!(!thread.handle_command(GstCommand::UpdateEncoder(new_config.clone())));
        assert_eq!(thread.encoder_config.width, 1280);
        assert_eq!(thread.encoder_config.height, 720);
        assert_eq!(thread.encoder_config.fps, 60);
        assert_eq!(thread.encoder_config.bitrate_kbps, 6000);
    }

    #[test]
    fn handle_stop_stream_no_panic_when_idle() {
        let (_main_ch, thread_ch) = create_channels();
        let mut thread = GstThread::new(thread_ch);
        // Should not panic even when there's no active stream pipeline
        assert!(!thread.handle_command(GstCommand::StopStream));
    }

    #[test]
    fn handle_stop_recording_no_panic_when_idle() {
        let (_main_ch, thread_ch) = create_channels();
        let mut thread = GstThread::new(thread_ch);
        // Should not panic even when there's no active record pipeline
        assert!(!thread.handle_command(GstCommand::StopRecording));
    }

    #[test]
    fn handle_audio_volume_no_panic_when_idle() {
        let (_main_ch, thread_ch) = create_channels();
        let mut thread = GstThread::new(thread_ch);
        // Should not panic even when there's no active audio pipeline
        assert!(!thread.handle_command(GstCommand::SetAudioVolume {
            source: AudioSourceKind::Mic,
            volume: 0.5,
        }));
    }

    #[test]
    fn handle_audio_muted_no_panic_when_idle() {
        let (_main_ch, thread_ch) = create_channels();
        let mut thread = GstThread::new(thread_ch);
        assert!(!thread.handle_command(GstCommand::SetAudioMuted {
            source: AudioSourceKind::System,
            muted: true,
        }));
    }

    #[test]
    fn stop_audio_capture_no_panic_when_idle() {
        let (_main_ch, thread_ch) = create_channels();
        let mut thread = GstThread::new(thread_ch);
        // Should not panic when no audio is capturing
        thread.stop_audio_capture(AudioSourceKind::Mic);
        thread.stop_audio_capture(AudioSourceKind::System);
    }
}
