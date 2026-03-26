use gstreamer::prelude::*;
use gstreamer_app::{AppSink, AppSrc};
use log;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;

use super::capture::{build_audio_capture_pipeline, build_capture_pipeline};
use super::commands::{
    AudioEncoderConfig, AudioSourceKind, AvailableEncoder, CaptureSourceConfig, EncoderConfig,
    EncoderType, GstCommand, GstThreadChannels, RecordingFormat, StreamDestination,
};
use super::encode::{
    RecordPipelineHandles, StreamPipelineHandles, build_record_pipeline_with_audio,
    build_stream_pipeline_with_audio,
};
use super::error::GstError;
use super::types::RgbaFrame;
use crate::scene::SourceId;

#[derive(Debug)]
enum PipelineKind {
    Stream,
    Record,
}

/// Per-source audio pipeline (device or file), keyed by SourceId.
struct AudioPipeline {
    pipeline: gstreamer::Pipeline,
    volume_element: gstreamer::Element,
    /// Holds the bus watch guard alive — dropping it removes the watch.
    _bus_watch: Option<gstreamer::bus::BusWatchGuard>,
}

/// Per-source capture pipeline and its appsink.
struct CaptureHandle {
    pipeline: gstreamer::Pipeline,
    appsink: AppSink,
    /// Set to `false` to stop the window/display capture thread (if any).
    capture_running: Option<Arc<AtomicBool>>,
    /// ScreenCaptureKit stream handle for display capture (macOS only).
    #[cfg(target_os = "macos")]
    sck_handle: Option<super::screencapturekit::SCStreamHandle>,
}

/// State held by the GStreamer thread.
struct GstThread {
    channels: GstThreadChannels,
    /// Active capture pipelines keyed by source id.
    captures: HashMap<SourceId, CaptureHandle>,
    // Encode pipeline handles
    stream_handles: Option<StreamPipelineHandles>,
    record_handles: Option<RecordPipelineHandles>,
    #[cfg(target_os = "macos")]
    virtual_camera_handle: Option<super::virtual_camera::VirtualCameraHandle>,
    /// Per-source audio pipelines, keyed by SourceId.
    audio_pipelines: HashMap<SourceId, AudioPipeline>,
    // Audio capture
    mic_pipeline: Option<gstreamer::Pipeline>,
    mic_appsink: Option<AppSink>,
    mic_volume_name: Option<String>,
    system_pipeline: Option<gstreamer::Pipeline>,
    system_appsink: Option<AppSink>,
    system_volume_name: Option<String>,
    has_system_audio: bool,
}

impl GstThread {
    fn new(channels: GstThreadChannels) -> Self {
        Self {
            channels,
            captures: HashMap::new(),
            audio_pipelines: HashMap::new(),
            stream_handles: None,
            record_handles: None,
            #[cfg(target_os = "macos")]
            virtual_camera_handle: None,
            mic_pipeline: None,
            mic_appsink: None,
            mic_volume_name: None,
            system_pipeline: None,
            system_appsink: None,
            system_volume_name: None,
            has_system_audio: false,
        }
    }

    /// Start capturing from the given source, keyed by source_id.
    fn add_capture_source(&mut self, source_id: SourceId, config: &CaptureSourceConfig) {
        self.remove_capture_source(source_id);

        // Display capture uses ScreenCaptureKit on macOS.
        #[cfg(target_os = "macos")]
        if let CaptureSourceConfig::Screen {
            screen_index,
            exclude_self,
        } = config
        {
            self.add_display_capture_source(source_id, *screen_index, *exclude_self);
            return;
        }

        // Window capture uses a dedicated appsrc pipeline + grab thread.
        #[cfg(target_os = "macos")]
        if let CaptureSourceConfig::Window { window_id } = config {
            self.add_window_capture_source(source_id, *window_id);
            return;
        }

        // Audio sources use dedicated audio pipelines (no video).
        if let CaptureSourceConfig::AudioDevice { device_uid } = config {
            match self.build_audio_device_pipeline(source_id, device_uid) {
                Ok(()) => log::info!("Started audio device capture for {source_id:?}"),
                Err(e) => log::error!("Failed to start audio device capture: {e}"),
            }
            return;
        }
        if let CaptureSourceConfig::AudioFile { path, looping } = config {
            match self.build_audio_file_pipeline(source_id, path, *looping) {
                Ok(()) => log::info!("Started audio file playback for {source_id:?}"),
                Err(e) => log::error!("Failed to start audio file playback: {e}"),
            }
            return;
        }

        match build_capture_pipeline(config, 1920, 1080, 30) {
            Ok((pipeline, appsink)) => {
                if let Err(e) = pipeline.set_state(gstreamer::State::Playing) {
                    log::error!("Failed to start capture for source {source_id:?}: {e}");
                    return;
                }
                self.captures.insert(
                    source_id,
                    CaptureHandle {
                        pipeline,
                        appsink,
                        capture_running: None,
                        #[cfg(target_os = "macos")]
                        sck_handle: None,
                    },
                );
                log::info!("Capture pipeline started for source {source_id:?}");
            }
            Err(e) => {
                log::error!("Failed to build capture pipeline for source {source_id:?}: {e}")
            }
        }
    }

    /// Start a window capture pipeline with a dedicated frame-grabbing thread.
    #[cfg(target_os = "macos")]
    fn add_window_capture_source(&mut self, source_id: SourceId, window_id: u32) {
        use super::capture::{build_window_capture_pipeline, grab_window_frame};

        // Grab one frame to determine the window dimensions.
        let (_, initial_width, initial_height) = match grab_window_frame(window_id) {
            Some(frame) => frame,
            None => {
                log::error!(
                    "Cannot capture window {window_id} for source {source_id:?}: window unavailable"
                );
                let _ = self.channels.error_tx.send(GstError::CaptureFailure {
                    message: format!("Window {window_id} is not available for capture"),
                });
                return;
            }
        };

        let fps = 30u32;

        let (pipeline, appsink, appsrc) =
            match build_window_capture_pipeline(initial_width, initial_height, fps) {
                Ok(handles) => handles,
                Err(e) => {
                    log::error!(
                        "Failed to build window capture pipeline for source {source_id:?}: {e}"
                    );
                    return;
                }
            };

        if let Err(e) = pipeline.set_state(gstreamer::State::Playing) {
            log::error!("Failed to start window capture for source {source_id:?}: {e}");
            return;
        }

        let running = Arc::new(AtomicBool::new(true));
        let running_clone = Arc::clone(&running);
        let error_tx = self.channels.error_tx.clone();
        let frame_interval = std::time::Duration::from_nanos(1_000_000_000 / fps as u64);

        std::thread::Builder::new()
            .name(format!("window-grab-{}", window_id))
            .spawn(move || {
                log::info!("Window grab thread started for window {window_id}");
                while running_clone.load(Ordering::Relaxed) {
                    let frame_start = std::time::Instant::now();

                    match grab_window_frame(window_id) {
                        Some((rgba_data, _w, _h)) => {
                            let mut buffer = gstreamer::Buffer::with_size(rgba_data.len()).unwrap();
                            {
                                let buf_ref = buffer.get_mut().unwrap();
                                let mut map = buf_ref.map_writable().unwrap();
                                map.as_mut_slice().copy_from_slice(&rgba_data);
                            }
                            if appsrc.push_buffer(buffer).is_err() {
                                log::warn!("Failed to push buffer to window appsrc, stopping");
                                break;
                            }
                        }
                        None => {
                            log::warn!("Window {window_id} became unavailable, stopping capture");
                            let _ = error_tx.send(GstError::CaptureFailure {
                                message: format!("Window {window_id} is no longer available"),
                            });
                            break;
                        }
                    }

                    let elapsed = frame_start.elapsed();
                    if elapsed < frame_interval {
                        std::thread::sleep(frame_interval - elapsed);
                    }
                }
                log::info!("Window grab thread exiting for window {window_id}");
            })
            .expect("spawn window grab thread");

        self.captures.insert(
            source_id,
            CaptureHandle {
                pipeline,
                appsink,
                capture_running: Some(running),
                #[cfg(target_os = "macos")]
                sck_handle: None,
            },
        );
        log::info!("Window capture pipeline started for source {source_id:?} (window {window_id})");
    }

    /// Start a display capture pipeline backed by ScreenCaptureKit.
    #[cfg(target_os = "macos")]
    fn add_display_capture_source(
        &mut self,
        source_id: SourceId,
        screen_index: u32,
        exclude_self: bool,
    ) {
        use super::capture::build_display_capture_pipeline;
        use super::screencapturekit;

        let width = 1920u32;
        let height = 1080u32;
        let fps = 30u32;

        // 1. Start SCK capture
        let (sck_handle, frame_rx) = match screencapturekit::start_display_capture(
            screen_index as usize,
            width,
            height,
            fps,
            exclude_self,
        ) {
            Ok(result) => result,
            Err(e) => {
                log::error!("Display capture failed for source {source_id:?}: {e}");
                let _ = self.channels.error_tx.send(GstError::CaptureFailure {
                    message: format!(
                        "Display capture failed: {e}. Check screen recording permission."
                    ),
                });
                return;
            }
        };

        // 2. Build GStreamer pipeline
        let (pipeline, appsink, appsrc) = match build_display_capture_pipeline(width, height, fps) {
            Ok(result) => result,
            Err(e) => {
                log::error!(
                    "Failed to build display capture pipeline for source {source_id:?}: {e}"
                );
                let _ = screencapturekit::stop_display_capture(sck_handle);
                return;
            }
        };

        // 3. Set pipeline to Playing
        if let Err(e) = pipeline.set_state(gstreamer::State::Playing) {
            log::error!("Failed to start display capture for source {source_id:?}: {e}");
            let _ = screencapturekit::stop_display_capture(sck_handle);
            return;
        }

        // 4. Spawn frame-pump thread that forwards SCK frames into appsrc
        let running = Arc::new(AtomicBool::new(true));
        let running_clone = Arc::clone(&running);

        std::thread::Builder::new()
            .name(format!("display-capture-{screen_index}"))
            .spawn(move || {
                log::info!("Display capture pump thread started for screen {screen_index}");
                while running_clone.load(Ordering::Relaxed) {
                    match frame_rx.recv_timeout(std::time::Duration::from_millis(100)) {
                        Ok(frame) => {
                            let mut buffer =
                                gstreamer::Buffer::with_size(frame.data.len()).unwrap();
                            {
                                let buf_ref = buffer.get_mut().unwrap();
                                let mut map = buf_ref.map_writable().unwrap();
                                map.as_mut_slice().copy_from_slice(&frame.data);
                            }
                            if appsrc.push_buffer(buffer).is_err() {
                                log::warn!("Failed to push buffer to display appsrc, stopping");
                                break;
                            }
                        }
                        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                            log::warn!("Display capture frame channel disconnected");
                            break;
                        }
                    }
                }
                log::info!("Display capture pump thread exiting for screen {screen_index}");
            })
            .expect("spawn display capture pump thread");

        // 5. Store handle
        self.captures.insert(
            source_id,
            CaptureHandle {
                pipeline,
                appsink,
                capture_running: Some(running),
                sck_handle: Some(sck_handle),
            },
        );
        log::info!(
            "Display capture pipeline started for source {source_id:?} (screen {screen_index})"
        );
    }

    /// Build and start an audio device capture pipeline for a per-source audio input.
    fn build_audio_device_pipeline(
        &mut self,
        source_id: SourceId,
        device_uid: &str,
    ) -> anyhow::Result<()> {
        let pipeline = gstreamer::Pipeline::new();
        let src = gstreamer::ElementFactory::make("osxaudiosrc")
            .property("unique-id", device_uid)
            .build()?;
        let convert = gstreamer::ElementFactory::make("audioconvert").build()?;
        let resample = gstreamer::ElementFactory::make("audioresample").build()?;
        let volume = gstreamer::ElementFactory::make("volume").build()?;
        let sink = gstreamer_app::AppSink::builder().build();

        pipeline.add_many([&src, &convert, &resample, &volume, sink.upcast_ref()])?;
        gstreamer::Element::link_many([&src, &convert, &resample, &volume, sink.upcast_ref()])?;
        pipeline.set_state(gstreamer::State::Playing)?;

        self.audio_pipelines.insert(
            source_id,
            AudioPipeline {
                pipeline,
                volume_element: volume,
                _bus_watch: None,
            },
        );
        Ok(())
    }

    /// Build and start an audio file playback pipeline for a per-source audio file.
    fn build_audio_file_pipeline(
        &mut self,
        source_id: SourceId,
        path: &str,
        looping: bool,
    ) -> anyhow::Result<()> {
        let uri = if path.starts_with("file://") {
            path.to_string()
        } else {
            format!("file://{path}")
        };

        let pipeline = gstreamer::Pipeline::new();
        let src = gstreamer::ElementFactory::make("uridecodebin")
            .property("uri", &uri)
            .build()?;
        let convert = gstreamer::ElementFactory::make("audioconvert").build()?;
        let resample = gstreamer::ElementFactory::make("audioresample").build()?;
        let volume = gstreamer::ElementFactory::make("volume").build()?;
        let sink = gstreamer_app::AppSink::builder().build();

        pipeline.add_many([&src, &convert, &resample, &volume, sink.upcast_ref()])?;
        // uridecodebin pads are dynamic — link on pad-added
        gstreamer::Element::link_many([&convert, &resample, &volume, sink.upcast_ref()])?;

        let convert_weak = convert.downgrade();
        src.connect_pad_added(move |_, pad| {
            if let Some(convert) = convert_weak.upgrade() {
                let sink_pad = convert.static_pad("sink").unwrap();
                if !sink_pad.is_linked() {
                    let _ = pad.link(&sink_pad);
                }
            }
        });

        let bus_watch = if looping {
            let pipeline_weak = pipeline.downgrade();
            let bus = pipeline.bus().unwrap();
            let guard = bus.add_watch(move |_, msg| {
                if let gstreamer::MessageView::Eos(..) = msg.view()
                    && let Some(pipeline) = pipeline_weak.upgrade()
                {
                    let _ = pipeline
                        .seek_simple(gstreamer::SeekFlags::FLUSH, gstreamer::ClockTime::ZERO);
                }
                gstreamer::glib::ControlFlow::Continue
            })?;
            Some(guard)
        } else {
            None
        };

        pipeline.set_state(gstreamer::State::Playing)?;

        self.audio_pipelines.insert(
            source_id,
            AudioPipeline {
                pipeline,
                volume_element: volume,
                _bus_watch: bus_watch,
            },
        );
        Ok(())
    }

    /// Stop and remove the capture pipeline for the given source_id.
    fn remove_capture_source(&mut self, source_id: SourceId) {
        if let Some(handle) = self.captures.remove(&source_id) {
            // Signal the capture thread to stop (if any).
            if let Some(ref running) = handle.capture_running {
                running.store(false, Ordering::Relaxed);
            }
            // Stop ScreenCaptureKit stream before tearing down the pipeline.
            #[cfg(target_os = "macos")]
            if let Some(sck) = handle.sck_handle {
                let _ = super::screencapturekit::stop_display_capture(sck);
            }
            let _ = handle.pipeline.set_state(gstreamer::State::Null);
            log::info!("Capture pipeline stopped for source {source_id:?}");
        }
        // Also remove audio pipeline if present
        if let Some(audio) = self.audio_pipelines.remove(&source_id) {
            let _ = audio.pipeline.set_state(gstreamer::State::Null);
            log::info!("Audio pipeline stopped for source {source_id:?}");
        }
    }

    /// Start audio capture for the given source kind and device.
    fn start_audio_capture(&mut self, kind: AudioSourceKind, device_uid: &str) {
        self.stop_audio_capture(kind);

        match build_audio_capture_pipeline(kind, device_uid, AudioEncoderConfig::default().sample_rate)
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
            GstCommand::StartStream {
                destination,
                stream_key,
                encoder_config,
            } => self.handle_start_stream(destination, stream_key, encoder_config),
            GstCommand::StopStream => self.handle_stop_stream(),
            GstCommand::StopRecording => self.handle_stop_recording(),
            GstCommand::StartRecording {
                path,
                format,
                encoder_config,
            } => self.handle_start_recording(path, format, encoder_config),
            GstCommand::SetAudioDevice { source, device_uid } => {
                self.handle_set_audio_device(source, device_uid)
            }
            GstCommand::SetAudioVolume { source, volume } => {
                self.handle_set_audio_volume(source, volume)
            }
            GstCommand::SetAudioMuted { source, muted } => {
                self.handle_set_audio_muted(source, muted)
            }
            GstCommand::StopCapture => self.handle_stop_capture(),
            GstCommand::AddCaptureSource { source_id, config } => {
                self.handle_add_capture_source(source_id, config)
            }
            GstCommand::RemoveCaptureSource { source_id } => {
                self.handle_remove_capture_source(source_id)
            }
            GstCommand::LoadImageFrame { source_id, frame } => {
                self.handle_load_image_frame(source_id, frame)
            }
            GstCommand::UpdateDisplayExclusion { exclude_self } => {
                self.handle_update_display_exclusion(exclude_self)
            }
            GstCommand::StartVirtualCamera => self.handle_start_virtual_camera(),
            GstCommand::StopVirtualCamera => self.handle_stop_virtual_camera(),
            GstCommand::SetSourceVolume { source_id, volume } => {
                if let Some(audio) = self.audio_pipelines.get(&source_id) {
                    audio.volume_element.set_property("volume", volume as f64);
                }
            }
            GstCommand::SetSourceMuted { source_id, muted } => {
                if let Some(audio) = self.audio_pipelines.get(&source_id) {
                    audio.volume_element.set_property("mute", muted);
                }
            }
            GstCommand::Shutdown => return self.handle_shutdown(),
        }
        false
    }

    fn handle_start_stream(
        &mut self,
        destination: StreamDestination,
        stream_key: String,
        encoder_config: EncoderConfig,
    ) {
        let rtmp_url = match &destination {
            StreamDestination::CustomRtmp { url } => url.clone(),
            other => format!("{}/{}", other.rtmp_url(), stream_key),
        };

        let audio_config = AudioEncoderConfig::default();
        match build_stream_pipeline_with_audio(
            &encoder_config,
            &audio_config,
            &rtmp_url,
            self.has_system_audio,
        ) {
            Ok(handles) => {
                if let Err(e) = handles.pipeline.set_state(gstreamer::State::Playing) {
                    let _ = self.channels.error_tx.send(GstError::EncodeFailure {
                        message: format!("Failed to start stream: {e}"),
                    });
                    return;
                }
                log::info!("Stream pipeline started to {}", destination.rtmp_url());
                self.stream_handles = Some(handles);
            }
            Err(e) => {
                let _ = self.channels.error_tx.send(GstError::EncodeFailure {
                    message: format!("{e}"),
                });
            }
        }
    }

    fn handle_stop_stream(&mut self) {
        self.stop_pipeline(PipelineKind::Stream);
    }

    fn handle_stop_recording(&mut self) {
        self.stop_pipeline(PipelineKind::Record);
    }

    fn handle_start_recording(
        &mut self,
        path: std::path::PathBuf,
        format: RecordingFormat,
        encoder_config: EncoderConfig,
    ) {
        let audio_config = AudioEncoderConfig::default();
        match build_record_pipeline_with_audio(
            &encoder_config,
            &audio_config,
            &path,
            format,
            self.has_system_audio,
        ) {
            Ok(handles) => {
                if let Err(e) = handles.pipeline.set_state(gstreamer::State::Playing) {
                    let _ = self.channels.error_tx.send(GstError::EncodeFailure {
                        message: format!("Failed to start recording: {e}"),
                    });
                    return;
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

    fn handle_set_audio_device(&mut self, source: AudioSourceKind, device_uid: String) {
        self.stop_audio_capture(source);
        self.start_audio_capture(source, &device_uid);
    }

    fn handle_set_audio_volume(&mut self, source: AudioSourceKind, volume: f32) {
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

    fn handle_set_audio_muted(&mut self, source: AudioSourceKind, muted: bool) {
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

    fn handle_stop_capture(&mut self) {
        self.handle_stop_virtual_camera();
        // Stop all per-source audio pipelines
        for (_, audio) in self.audio_pipelines.drain() {
            let _ = audio.pipeline.set_state(gstreamer::State::Null);
        }
        for (_, handle) in self.captures.drain() {
            if let Some(ref running) = handle.capture_running {
                running.store(false, Ordering::Relaxed);
            }
            #[cfg(target_os = "macos")]
            if let Some(sck) = handle.sck_handle {
                let _ = super::screencapturekit::stop_display_capture(sck);
            }
            let _ = handle.pipeline.set_state(gstreamer::State::Null);
        }
        log::info!("All captures stopped");
    }

    fn handle_add_capture_source(&mut self, source_id: SourceId, config: CaptureSourceConfig) {
        self.add_capture_source(source_id, &config);
    }

    fn handle_remove_capture_source(&mut self, source_id: SourceId) {
        self.remove_capture_source(source_id);
    }

    #[cfg(target_os = "macos")]
    fn handle_update_display_exclusion(&mut self, exclude_self: bool) {
        use super::screencapturekit;
        for (source_id, handle) in &self.captures {
            if let Some(sck) = &handle.sck_handle
                && let Err(e) = screencapturekit::update_exclusion(sck, exclude_self)
            {
                log::warn!("Failed to update display exclusion for source {source_id:?}: {e}");
            }
        }
    }

    #[cfg(not(target_os = "macos"))]
    fn handle_update_display_exclusion(&mut self, _exclude_self: bool) {}

    #[cfg(target_os = "macos")]
    fn handle_start_virtual_camera(&mut self) {
        use super::virtual_camera;
        let width = 1920u32;
        let height = 1080u32;
        let fps = 30u32;
        match virtual_camera::start_virtual_camera(width, height, fps) {
            Ok(handle) => {
                self.virtual_camera_handle = Some(handle);
                log::info!("Virtual camera started ({width}x{height} @ {fps}fps)");
            }
            Err(e) => {
                log::error!("Failed to start virtual camera: {e}");
            }
        }
    }

    #[cfg(target_os = "macos")]
    fn handle_stop_virtual_camera(&mut self) {
        use super::virtual_camera;
        if let Some(handle) = self.virtual_camera_handle.take() {
            if let Err(e) = virtual_camera::stop_virtual_camera(handle) {
                log::warn!("Error stopping virtual camera: {e}");
            }
            log::info!("Virtual camera stopped");
        }
    }

    #[cfg(not(target_os = "macos"))]
    fn handle_start_virtual_camera(&mut self) {}
    #[cfg(not(target_os = "macos"))]
    fn handle_stop_virtual_camera(&mut self) {}

    fn handle_load_image_frame(&mut self, source_id: SourceId, frame: RgbaFrame) {
        self.channels
            .latest_frames
            .lock()
            .unwrap()
            .insert(source_id, frame);
    }

    /// Returns `true` to signal the run loop to exit.
    fn handle_shutdown(&mut self) -> bool {
        self.handle_stop_virtual_camera();
        self.stop_pipeline(PipelineKind::Stream);
        self.stop_pipeline(PipelineKind::Record);
        self.stop_audio_capture(AudioSourceKind::Mic);
        self.stop_audio_capture(AudioSourceKind::System);
        // Stop all per-source audio pipelines
        for (_, audio) in self.audio_pipelines.drain() {
            let _ = audio.pipeline.set_state(gstreamer::State::Null);
        }
        for (_, handle) in self.captures.drain() {
            if let Some(ref running) = handle.capture_running {
                running.store(false, Ordering::Relaxed);
            }
            #[cfg(target_os = "macos")]
            if let Some(sck) = handle.sck_handle {
                let _ = super::screencapturekit::stop_display_capture(sck);
            }
            let _ = handle.pipeline.set_state(gstreamer::State::Null);
        }
        true
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

    /// Probe GStreamer for available H.264 encoders.
    fn enumerate_encoders() -> Vec<AvailableEncoder> {
        let mut encoders = Vec::new();
        let mut found_recommended = false;

        for &encoder_type in EncoderType::all() {
            if gstreamer::ElementFactory::make(encoder_type.element_name())
                .build()
                .is_ok()
            {
                let is_hw = encoder_type.is_hardware();
                let is_recommended = !found_recommended
                    && (is_hw || encoder_type == EncoderType::H264x264);
                if is_recommended {
                    found_recommended = true;
                }
                encoders.push(AvailableEncoder {
                    encoder_type,
                    is_recommended,
                });
            }
        }

        if !found_recommended {
            if let Some(enc) = encoders.iter_mut().find(|e| e.encoder_type == EncoderType::H264x264) {
                enc.is_recommended = true;
            }
        }

        encoders
    }

    /// Main run loop for the GStreamer thread.
    fn run(mut self) {
        // Detect available encoders
        let encoders = Self::enumerate_encoders();
        log::info!(
            "Detected {} encoder(s): {:?}",
            encoders.len(),
            encoders.iter().map(|e| e.encoder_type.display_name()).collect::<Vec<_>>()
        );
        let _ = self.channels.encoders_tx.send(encoders);

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
            // Drain pending commands (up to 8 per tick) so the channel doesn't back up
            // while still leaving time for frame pulling.
            for _ in 0..8 {
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
                        for (_, handle) in self.captures.drain() {
                            if let Some(ref running) = handle.capture_running {
                                running.store(false, Ordering::Relaxed);
                            }
                            #[cfg(target_os = "macos")]
                            if let Some(sck) = handle.sck_handle {
                                let _ = super::screencapturekit::stop_display_capture(sck);
                            }
                            let _ = handle.pipeline.set_state(gstreamer::State::Null);
                        }
                        return;
                    }
                    Err(tokio::sync::mpsc::error::TryRecvError::Empty) => break,
                }
            }

            let pts = gstreamer::ClockTime::from_nseconds(start_time.elapsed().as_nanos() as u64);

            // Pull frames from all active capture pipelines and update latest_frames map.
            for (&source_id, handle) in self.captures.iter() {
                if let Some(sample) = handle
                    .appsink
                    .try_pull_sample(gstreamer::ClockTime::from_mseconds(0))
                {
                    let (width, height) = sample
                        .caps()
                        .and_then(|caps| gstreamer_video::VideoInfo::from_caps(caps).ok())
                        .map(|info| (info.width(), info.height()))
                        .unwrap_or((1920, 1080));

                    if let Some(buffer) = sample.buffer()
                        && let Ok(map) = buffer.map_readable()
                    {
                        let frame = RgbaFrame {
                            data: map.as_slice().to_vec(),
                            width,
                            height,
                        };
                        if let Ok(mut frames) = self.channels.latest_frames.lock() {
                            frames.insert(source_id, frame);
                        }
                    }
                }
            }

            // Forward composited frames to active encode pipelines.
            while let Ok(frame) = self.channels.composited_frame_rx.try_recv() {
                if let Some(ref handles) = self.stream_handles {
                    Self::push_to_encode(&handles.video_appsrc, &frame.data, pts);
                }
                if let Some(ref handles) = self.record_handles {
                    Self::push_to_encode(&handles.video_appsrc, &frame.data, pts);
                }
                #[cfg(target_os = "macos")]
                if let Some(ref handle) = self.virtual_camera_handle
                    && let Err(e) = super::virtual_camera::write_frame(handle, &frame)
                {
                    log::warn!("Virtual camera frame write failed: {e}");
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
    fn enumerate_encoders_returns_at_least_one() {
        gstreamer::init().unwrap();
        let encoders = GstThread::enumerate_encoders();
        assert!(!encoders.is_empty(), "should detect at least x264");
        assert_eq!(
            encoders.iter().filter(|e| e.is_recommended).count(),
            1,
            "exactly one encoder should be recommended"
        );
    }

    #[test]
    fn pipeline_kind_debug() {
        assert_eq!(format!("{:?}", PipelineKind::Stream), "Stream");
        assert_eq!(format!("{:?}", PipelineKind::Record), "Record");
    }

    #[test]
    fn gst_thread_new_has_defaults() {
        let (_main_ch, thread_ch) = create_channels();
        let thread = GstThread::new(thread_ch);
        assert!(thread.captures.is_empty());
        assert!(thread.audio_pipelines.is_empty());
        assert!(thread.stream_handles.is_none());
        assert!(thread.record_handles.is_none());
        assert!(thread.mic_pipeline.is_none());
        assert!(thread.system_pipeline.is_none());
        assert!(thread.mic_appsink.is_none());
        assert!(thread.system_appsink.is_none());
        assert!(!thread.has_system_audio);
    }

    #[test]
    fn handle_shutdown_returns_true() {
        let (_main_ch, thread_ch) = create_channels();
        let mut thread = GstThread::new(thread_ch);
        assert!(thread.handle_command(GstCommand::Shutdown));
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

    #[test]
    fn add_and_remove_capture_source_commands() {
        use crate::gstreamer::commands::GstCommand;
        use crate::scene::SourceId;
        let (main_ch, _thread_ch) = create_channels();
        main_ch
            .command_tx
            .try_send(GstCommand::AddCaptureSource {
                source_id: SourceId(1),
                config: CaptureSourceConfig::Screen {
                    screen_index: 0,
                    exclude_self: false,
                },
            })
            .unwrap();
        main_ch
            .command_tx
            .try_send(GstCommand::RemoveCaptureSource {
                source_id: SourceId(1),
            })
            .unwrap();
    }
}
