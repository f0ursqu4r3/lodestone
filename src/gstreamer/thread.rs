use gstreamer::prelude::*;
use gstreamer_app::{AppSink, AppSrc};
use log;
use std::thread::JoinHandle;

use super::capture::build_capture_pipeline;
use super::commands::{CaptureSourceConfig, GstCommand, GstThreadChannels};
use super::encode::{build_record_pipeline, build_stream_pipeline};
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
    stream_pipeline: Option<gstreamer::Pipeline>,
    stream_appsrc: Option<AppSrc>,
    record_pipeline: Option<gstreamer::Pipeline>,
    record_appsrc: Option<AppSrc>,
    encoder_config: super::commands::EncoderConfig,
}

impl GstThread {
    fn new(channels: GstThreadChannels) -> Self {
        Self {
            channels,
            capture_pipeline: None,
            capture_appsink: None,
            stream_pipeline: None,
            stream_appsrc: None,
            record_pipeline: None,
            record_appsrc: None,
            encoder_config: super::commands::EncoderConfig::default(),
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
                match build_stream_pipeline(&self.encoder_config, &url) {
                    Ok((pipeline, appsrc)) => {
                        if let Err(e) = pipeline.set_state(gstreamer::State::Playing) {
                            let _ = self.channels.error_tx.send(GstError::EncodeFailure {
                                message: format!("Failed to start stream: {e}"),
                            });
                            return false;
                        }
                        self.stream_pipeline = Some(pipeline);
                        self.stream_appsrc = Some(appsrc);
                        log::info!("Stream pipeline started");
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
                match build_record_pipeline(&self.encoder_config, &path, format) {
                    Ok((pipeline, appsrc)) => {
                        if let Err(e) = pipeline.set_state(gstreamer::State::Playing) {
                            let _ = self.channels.error_tx.send(GstError::EncodeFailure {
                                message: format!("Failed to start recording: {e}"),
                            });
                            return false;
                        }
                        self.record_pipeline = Some(pipeline);
                        self.record_appsrc = Some(appsrc);
                        log::info!("Record pipeline started to {}", path.display());
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
            GstCommand::SetAudioDevice { .. }
            | GstCommand::SetAudioVolume { .. }
            | GstCommand::SetAudioMuted { .. } => {
                // Audio command handling will be implemented in a later task.
            }
            GstCommand::Shutdown => {
                self.stop_pipeline(PipelineKind::Stream);
                self.stop_pipeline(PipelineKind::Record);
                self.stop_capture();
                return true;
            }
        }
        false
    }

    fn stop_pipeline(&mut self, kind: PipelineKind) {
        let (appsrc, pipeline) = match kind {
            PipelineKind::Stream => (self.stream_appsrc.take(), self.stream_pipeline.take()),
            PipelineKind::Record => (self.record_appsrc.take(), self.record_pipeline.take()),
        };
        if let Some(appsrc) = appsrc {
            let _ = appsrc.end_of_stream();
        }
        if let Some(pipeline) = pipeline {
            let bus = pipeline.bus().unwrap();
            let _ = bus.timed_pop_filtered(
                gstreamer::ClockTime::from_seconds(2),
                &[gstreamer::MessageType::Eos],
            );
            let _ = pipeline.set_state(gstreamer::State::Null);
        }
        log::info!("{:?} pipeline stopped", kind);
    }

    /// Main run loop for the GStreamer thread.
    fn run(mut self) {
        self.start_capture(&CaptureSourceConfig::Screen { screen_index: 0 });
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
                    self.stop_capture();
                    return;
                }
                Err(tokio::sync::mpsc::error::TryRecvError::Empty) => {}
            }

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
                    let pts =
                        gstreamer::ClockTime::from_nseconds(start_time.elapsed().as_nanos() as u64);

                    // Send to preview
                    let frame = RgbaFrame {
                        data: data.to_vec(),
                        width,
                        height,
                    };
                    let _ = self.channels.frame_tx.try_send(frame);

                    // Feed active encode pipelines
                    if let Some(ref appsrc) = self.stream_appsrc {
                        Self::push_to_encode(appsrc, data, pts);
                    }
                    if let Some(ref appsrc) = self.record_appsrc {
                        Self::push_to_encode(appsrc, data, pts);
                    }
                }
            }

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
        assert!(thread.stream_pipeline.is_none());
        assert!(thread.stream_appsrc.is_none());
        assert!(thread.record_pipeline.is_none());
        assert!(thread.record_appsrc.is_none());
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
}
