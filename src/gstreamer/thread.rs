use anyhow::Context;
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
use super::types::{OutputRuntimeState, PipelineStats, RgbaFrame};
#[cfg(target_os = "macos")]
use super::window_watcher::{WatchedSource, WindowWatcher};
use crate::scene::SourceId;

/// Tracks a window capture source on Windows (foreground window modes).
#[cfg(target_os = "windows")]
struct WinWindowSource {
    mode: crate::scene::WindowCaptureMode,
    /// The HWND currently being captured (None if not yet started).
    current_hwnd: Option<u64>,
    capture_size: (u32, u32),
    fps: u32,
    /// HWND that failed to start capture — skip it until it changes.
    failed_hwnd: Option<u64>,
}

#[cfg(target_os = "windows")]
fn select_fullscreen_candidate(
    ws: &WinWindowSource,
    candidates: &[(u64, u32, u32)],
) -> Option<u64> {
    if let Some(current) = ws.current_hwnd
        && candidates.iter().any(|(hwnd, _, _)| *hwnd == current)
    {
        return Some(current);
    }

    let active_failed = ws
        .failed_hwnd
        .filter(|failed| candidates.iter().any(|(hwnd, _, _)| hwnd == failed));

    candidates
        .iter()
        .find(|(hwnd, _, _)| Some(*hwnd) != active_failed)
        .map(|(hwnd, _, _)| *hwnd)
}

/// State for a game capture source using DLL injection + DirectX hooking.
#[cfg(target_os = "windows")]
struct GameCaptureSource {
    process_id: Option<u32>,
    process_name: String,
    window_title: String,
    handles: Option<super::inject::SharedCaptureHandles>,
    last_frame_index: u64,
    failed_process_id: Option<u32>,
    last_alive_check: std::time::Instant,
    last_resolve_attempt: std::time::Instant,
}

/// How often to poll windows on Windows.
#[cfg(target_os = "windows")]
const WIN_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(500);

/// How often to do full window enumeration for SpecificWindow/Application/AnyFullscreen.
#[cfg(target_os = "windows")]
const WIN_ENUM_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(1500);

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
    stream_started_at: Option<std::time::Instant>,
    stream_total_frames: u64,
    stream_dropped_frames: u64,
    stream_last_stats_at: Option<std::time::Instant>,
    stream_last_output_bytes: u64,
    record_handles: Option<RecordPipelineHandles>,
    record_path: Option<std::path::PathBuf>,
    #[cfg(target_os = "macos")]
    virtual_camera_handle: Option<super::virtual_camera::VirtualCameraHandle>,
    /// Per-source audio pipelines, keyed by SourceId.
    audio_pipelines: HashMap<SourceId, AudioPipeline>,
    /// Watcher that resolves window capture targets.
    #[cfg(target_os = "macos")]
    window_watcher: WindowWatcher,
    /// Active window capture sources being watched.
    #[cfg(target_os = "macos")]
    watched_windows: HashMap<SourceId, WatchedSource>,
    /// Active window capture sources on Windows (foreground window modes).
    #[cfg(target_os = "windows")]
    win_watched_windows: HashMap<SourceId, WinWindowSource>,
    /// Last time the foreground window was polled on Windows.
    #[cfg(target_os = "windows")]
    win_last_fg_poll: std::time::Instant,
    /// Last time full window enumeration was done on Windows.
    #[cfg(target_os = "windows")]
    win_last_enum_poll: std::time::Instant,
    /// Active game capture sources (DLL injection + hook).
    #[cfg(target_os = "windows")]
    game_captures: HashMap<SourceId, GameCaptureSource>,
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
            #[cfg(target_os = "macos")]
            window_watcher: WindowWatcher::new(),
            #[cfg(target_os = "macos")]
            watched_windows: HashMap::new(),
            #[cfg(target_os = "windows")]
            win_watched_windows: HashMap::new(),
            #[cfg(target_os = "windows")]
            win_last_fg_poll: std::time::Instant::now(),
            #[cfg(target_os = "windows")]
            win_last_enum_poll: std::time::Instant::now(),
            #[cfg(target_os = "windows")]
            game_captures: HashMap::new(),
            stream_handles: None,
            stream_started_at: None,
            stream_total_frames: 0,
            stream_dropped_frames: 0,
            stream_last_stats_at: None,
            stream_last_output_bytes: 0,
            record_handles: None,
            record_path: None,
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

    fn publish_runtime_state(&self) {
        let _ = self.channels.runtime_state_tx.send(OutputRuntimeState {
            stream_active: self.stream_handles.is_some(),
            recording_path: self.record_path.clone(),
            virtual_camera_active: self.virtual_camera_is_active(),
        });
    }

    fn virtual_camera_is_active(&self) -> bool {
        #[cfg(target_os = "macos")]
        {
            self.virtual_camera_handle.is_some()
        }
        #[cfg(not(target_os = "macos"))]
        {
            false
        }
    }

    /// Start capturing from the given source, keyed by source_id.
    fn add_capture_source(&mut self, source_id: SourceId, config: &CaptureSourceConfig, fps: u32) {
        self.remove_capture_source(source_id);

        // Display capture uses ScreenCaptureKit on macOS.
        #[cfg(target_os = "macos")]
        if let CaptureSourceConfig::Screen {
            screen_index,
            exclude_self,
            capture_size,
        } = config
        {
            self.add_display_capture_source(
                source_id,
                *screen_index,
                *exclude_self,
                *capture_size,
                fps,
            );
            return;
        }

        // Window capture uses ScreenCaptureKit + WindowWatcher on macOS.
        #[cfg(target_os = "macos")]
        if let CaptureSourceConfig::Window { mode, capture_size } = config {
            self.add_window_capture_source(source_id, mode.clone(), *capture_size, fps);
            return;
        }

        // Window capture on Windows: resolve foreground HWND and start capture.
        #[cfg(target_os = "windows")]
        if let CaptureSourceConfig::Window { mode, capture_size } = config {
            self.add_win_window_capture_source(source_id, mode.clone(), *capture_size, fps);
            return;
        }

        // Game capture uses DLL injection + DirectX hooking (Windows only).
        #[cfg(target_os = "windows")]
        if let CaptureSourceConfig::GameCapture {
            process_id,
            process_name,
            window_title,
        } = config
        {
            self.watch_game_capture(
                source_id,
                *process_id,
                process_name.clone(),
                window_title.clone(),
            );
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

        match build_capture_pipeline(config, 1920, 1080, fps) {
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

    /// Register a window capture source with the WindowWatcher and start capture if a target is found.
    #[cfg(target_os = "macos")]
    fn add_window_capture_source(
        &mut self,
        source_id: SourceId,
        mode: crate::scene::WindowCaptureMode,
        _capture_size: (u32, u32),
        fps: u32,
    ) {
        let watched = WatchedSource {
            mode: mode.clone(),
            current_window_id: None,
            current_window_size: None,
            fps,
        };
        self.watched_windows.insert(source_id, watched);

        self.window_watcher.force_refresh();
        let resolved = self.window_watcher.resolve_target(&mode);

        let Some((window_id, width, height)) = resolved else {
            log::info!("No window found for source {source_id:?}, watcher will retry");
            return;
        };

        self.start_sck_window_capture(source_id, window_id, width, height, fps);
    }

    /// Start a ScreenCaptureKit-based window capture for the given window ID.
    ///
    /// Follows the same pattern as [`add_display_capture_source`]: start SCK capture,
    /// build an appsrc pipeline, spawn a pump thread, and store the handle.
    #[cfg(target_os = "macos")]
    fn start_sck_window_capture(
        &mut self,
        source_id: SourceId,
        window_id: u32,
        width: u32,
        height: u32,
        fps: u32,
    ) {
        use super::capture::build_display_capture_pipeline;
        use super::screencapturekit;

        // Use actual window dimensions, with a minimum of 1px
        let width = width.max(1);
        let height = height.max(1);

        let (sck_handle, frame_rx) =
            match screencapturekit::start_window_capture(window_id, width, height, fps) {
                Ok(result) => result,
                Err(e) => {
                    log::error!("Window capture failed for source {source_id:?}: {e}");
                    let _ = self.channels.error_tx.send(GstError::CaptureFailure {
                        message: format!("Window capture failed: {e}"),
                    });
                    return;
                }
            };

        let (pipeline, appsink, appsrc) = match build_display_capture_pipeline(width, height, fps) {
            Ok(result) => result,
            Err(e) => {
                log::error!("Failed to build window pipeline for source {source_id:?}: {e}");
                let _ = screencapturekit::stop_display_capture(sck_handle);
                return;
            }
        };

        if let Err(e) = pipeline.set_state(gstreamer::State::Playing) {
            log::error!("Failed to start window capture for source {source_id:?}: {e}");
            let _ = screencapturekit::stop_display_capture(sck_handle);
            return;
        }

        let running = Arc::new(AtomicBool::new(true));
        let running_clone = Arc::clone(&running);

        std::thread::Builder::new()
            .name(format!("window-capture-{window_id}"))
            .spawn(move || {
                log::info!("Window capture pump started for window {window_id}");
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
                                log::warn!("Failed to push buffer to window appsrc, stopping");
                                break;
                            }
                        }
                        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                            log::warn!("Window capture channel disconnected");
                            break;
                        }
                    }
                }
                log::info!("Window capture pump exiting for window {window_id}");
            })
            .expect("spawn window capture pump thread");

        if let Some(watched) = self.watched_windows.get_mut(&source_id) {
            watched.current_window_id = Some(window_id);
        }

        self.captures.insert(
            source_id,
            CaptureHandle {
                pipeline,
                appsink,
                capture_running: Some(running),
                sck_handle: Some(sck_handle),
            },
        );
        log::info!("Window capture started for source {source_id:?} (window {window_id})");
    }

    /// Handle a window target change detected by the WindowWatcher.
    #[cfg(target_os = "macos")]
    fn handle_window_target_change(
        &mut self,
        source_id: SourceId,
        new_target: Option<(u32, u32, u32)>,
    ) {
        match new_target {
            Some((wid, width, height)) => {
                // Try to update the existing SCK stream in-place first.
                if let Some(capture) = self.captures.get_mut(&source_id)
                    && let Some(ref mut sck_handle) = capture.sck_handle
                {
                    match super::screencapturekit::update_window_target(sck_handle, wid) {
                        Ok(()) => {
                            if let Some(watched) = self.watched_windows.get_mut(&source_id) {
                                watched.current_window_id = Some(wid);
                            }
                            log::info!("Switched window target for {source_id:?} to {wid}");
                            return;
                        }
                        Err(e) => {
                            log::warn!("Failed to update window target, rebuilding: {e}")
                        }
                    }
                }
                // Fall back: tear down and rebuild.
                let fps = self
                    .watched_windows
                    .get(&source_id)
                    .map(|w| w.fps)
                    .unwrap_or(30);
                self.remove_capture_source(source_id);
                self.start_sck_window_capture(source_id, wid, width, height, fps);
            }
            None => {
                log::info!("Target window gone for {source_id:?}, holding last frame");
            }
        }
    }

    /// Start a display capture pipeline backed by ScreenCaptureKit.
    #[cfg(target_os = "macos")]
    fn add_display_capture_source(
        &mut self,
        source_id: SourceId,
        screen_index: u32,
        exclude_self: bool,
        capture_size: (u32, u32),
        fps: u32,
    ) {
        use super::capture::build_display_capture_pipeline;
        use super::screencapturekit;

        let width = capture_size.0;
        let height = capture_size.1;

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
        let (pipeline, _appsink, volume_name) = build_audio_capture_pipeline(
            AudioSourceKind::Mic,
            device_uid,
            AudioEncoderConfig::default().sample_rate,
        )?;
        let volume = pipeline
            .by_name(&volume_name)
            .context("Failed to find audio source volume element")?;
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
        #[cfg(target_os = "macos")]
        self.watched_windows.remove(&source_id);
        #[cfg(target_os = "windows")]
        self.win_watched_windows.remove(&source_id);

        #[cfg(target_os = "windows")]
        if let Some(gc) = self.game_captures.remove(&source_id) {
            if let Some(handles) = gc.handles {
                Self::stop_game_capture_handles(handles);
            }
            // Also remove from latest_frames.
            if let Ok(mut frames) = self.channels.latest_frames.lock() {
                frames.remove(&source_id);
            }
            log::info!("Game capture stopped for source {source_id:?}");
        }

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

        match build_audio_capture_pipeline(
            kind,
            device_uid,
            AudioEncoderConfig::default().sample_rate,
        ) {
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
    fn push_to_encode(appsrc: &AppSrc, data: &[u8], pts: gstreamer::ClockTime) -> bool {
        let mut buffer = gstreamer::Buffer::with_size(data.len()).unwrap();
        {
            let buffer_ref = buffer.get_mut().unwrap();
            buffer_ref.set_pts(pts);
            let mut map = buffer_ref.map_writable().unwrap();
            map.as_mut_slice().copy_from_slice(data);
        }
        if let Err(e) = appsrc.push_buffer(buffer) {
            log::warn!("Failed to push frame to encoder: {e}");
            return false;
        }
        true
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
            GstCommand::AddCaptureSource {
                source_id,
                config,
                fps,
            } => self.handle_add_capture_source(source_id, config, fps),
            GstCommand::RemoveCaptureSource { source_id } => {
                self.handle_remove_capture_source(source_id)
            }
            GstCommand::LoadImageFrame { source_id, frame } => {
                self.handle_load_image_frame(source_id, frame)
            }
            GstCommand::UpdateDisplayExclusion { exclude_self } => {
                self.handle_update_display_exclusion(exclude_self)
            }
            GstCommand::StartVirtualCamera { fps } => self.handle_start_virtual_camera(fps),
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
            GstCommand::CaptureForegroundWindow => self.handle_capture_foreground_window(),
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
        if self.stream_handles.is_some() {
            log::debug!("Ignoring duplicate StartStream command while stream is active");
            return;
        }

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
                let now = std::time::Instant::now();
                self.stream_started_at = Some(now);
                self.stream_total_frames = 0;
                self.stream_dropped_frames = 0;
                self.stream_last_stats_at = Some(now);
                self.stream_last_output_bytes = 0;
                let _ = self.channels.stats_tx.send(PipelineStats::default());
                self.publish_runtime_state();
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
        if self.record_handles.is_some() {
            log::debug!("Ignoring duplicate StartRecording command while recording is active");
            return;
        }

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
                self.record_path = Some(path);
                self.publish_runtime_state();
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

    fn handle_add_capture_source(
        &mut self,
        source_id: SourceId,
        config: CaptureSourceConfig,
        fps: u32,
    ) {
        self.add_capture_source(source_id, &config, fps);
    }

    fn handle_remove_capture_source(&mut self, source_id: SourceId) {
        self.remove_capture_source(source_id);
    }

    /// Register a window capture source on Windows and start capture if a target is available.
    #[cfg(target_os = "windows")]
    fn add_win_window_capture_source(
        &mut self,
        source_id: SourceId,
        mode: crate::scene::WindowCaptureMode,
        capture_size: (u32, u32),
        fps: u32,
    ) {
        use crate::scene::WindowCaptureMode;

        self.win_watched_windows.insert(
            source_id,
            WinWindowSource {
                mode: mode.clone(),
                current_hwnd: None,
                capture_size,
                fps,
                failed_hwnd: None,
            },
        );

        // Try to resolve target immediately for modes that don't wait for a trigger.
        let resolved = match &mode {
            WindowCaptureMode::ForegroundWindow => {
                super::devices::get_foreground_window_hwnd()
            }
            WindowCaptureMode::SpecificWindow {
                process_name,
                window_title,
            } => super::devices::find_window_by_process_and_title(process_name, window_title)
                .map(|(hwnd, _, _)| hwnd),
            WindowCaptureMode::Application {
                bundle_id, ..
            } => {
                // On Windows, bundle_id stores the process name.
                super::devices::find_app_window_by_process(bundle_id)
                    .map(|(hwnd, _, _)| hwnd)
            }
            WindowCaptureMode::AnyFullscreen => {
                super::devices::find_fullscreen_window().map(|(hwnd, _, _)| hwnd)
            }
            WindowCaptureMode::ForegroundOnHotkey => None,
        };

        if let Some(hwnd) = resolved {
            self.start_win_window_capture(source_id, hwnd);
        } else {
            let desc = match &mode {
                WindowCaptureMode::ForegroundOnHotkey => "waiting for hotkey trigger",
                _ => "no matching window found, will retry on poll",
            };
            log::info!("Window source {source_id:?}: {desc}");
        }
    }

    /// Start or switch the Windows window capture to a specific HWND.
    #[cfg(target_os = "windows")]
    fn start_win_window_capture(&mut self, source_id: SourceId, hwnd: u64) {
        let (capture_size, fps) = match self.win_watched_windows.get(&source_id) {
            Some(ws) => {
                // Skip if this HWND already failed — avoid retry spam.
                if ws.failed_hwnd == Some(hwnd) {
                    return;
                }
                (ws.capture_size, ws.fps)
            }
            None => return,
        };

        // Remove existing capture pipeline for this source (if any).
        if let Some(handle) = self.captures.remove(&source_id) {
            if let Some(ref running) = handle.capture_running {
                running.store(false, Ordering::Relaxed);
            }
            let _ = handle.pipeline.set_state(gstreamer::State::Null);
        }

        let config = CaptureSourceConfig::WindowHandle {
            hwnd,
            capture_size,
        };

        match build_capture_pipeline(&config, capture_size.0, capture_size.1, fps) {
            Ok((pipeline, appsink)) => {
                if let Err(e) = pipeline.set_state(gstreamer::State::Playing) {
                    // Check bus for a more detailed error message.
                    if let Some(bus) = pipeline.bus()
                        && let Some(msg) = bus.timed_pop_filtered(
                            gstreamer::ClockTime::from_mseconds(100),
                            &[gstreamer::MessageType::Error],
                        )
                        && let gstreamer::MessageView::Error(err) = msg.view()
                    {
                        log::error!(
                            "Window capture failed for {source_id:?} (HWND {hwnd:#x}): {}",
                            err.error()
                        );
                        if let Some(debug) = err.debug() {
                            log::error!("  debug: {debug}");
                        }
                    } else {
                        log::error!(
                            "Failed to start window capture for {source_id:?} (HWND {hwnd:#x}): {e}"
                        );
                    }
                    let _ = pipeline.set_state(gstreamer::State::Null);
                    // Clear the active target and mark this HWND as failed to avoid retry spam.
                    if let Some(ws) = self.win_watched_windows.get_mut(&source_id) {
                        ws.current_hwnd = None;
                        ws.failed_hwnd = Some(hwnd);
                    }
                    let title = super::devices::get_window_title_from_hwnd(hwnd);
                    let _ = self.channels.error_tx.send(GstError::CaptureFailure {
                        message: format!(
                            "Window capture failed for \"{title}\". This window may not support Windows Graphics Capture."
                        ),
                    });
                    return;
                }
                if let Some(ws) = self.win_watched_windows.get_mut(&source_id) {
                    ws.current_hwnd = Some(hwnd);
                    ws.failed_hwnd = None;
                }
                // Explicitly disable the WGC capture border after pipeline starts.
                // The border is only enabled when the source is selected in the preview.
                if let Some(src_element) = pipeline.by_name("capture-source") {
                    if src_element.find_property("show-border").is_some() {
                        src_element.set_property("show-border", false);
                    }
                }

                self.captures.insert(
                    source_id,
                    CaptureHandle {
                        pipeline,
                        appsink,
                        capture_running: None,
                    },
                );
                let title = super::devices::get_window_title_from_hwnd(hwnd);
                log::info!(
                    "Window capture started for source {source_id:?}: \"{title}\" (HWND {hwnd:#x})"
                );
            }
            Err(e) => {
                log::error!(
                    "Failed to build window capture pipeline for {source_id:?} (HWND {hwnd:#x}): {e}"
                );
                // Mark as failed to prevent retry spam.
                if let Some(ws) = self.win_watched_windows.get_mut(&source_id) {
                    ws.current_hwnd = None;
                    ws.failed_hwnd = Some(hwnd);
                }
                let _ = self.channels.error_tx.send(GstError::CaptureFailure {
                    message: format!("Window capture failed: {e}"),
                });
            }
        }
    }

    /// Poll window changes on Windows for all tracked window capture sources.
    #[cfg(target_os = "windows")]
    fn poll_win_foreground_window(&mut self) {
        use crate::scene::WindowCaptureMode;

        if self.win_watched_windows.is_empty() {
            return;
        }

        let now = std::time::Instant::now();
        let fg_poll_due = now.duration_since(self.win_last_fg_poll) >= WIN_POLL_INTERVAL;
        let enum_poll_due = now.duration_since(self.win_last_enum_poll) >= WIN_ENUM_POLL_INTERVAL;

        if !fg_poll_due && !enum_poll_due {
            return;
        }

        // Foreground HWND is cheap to query — do it on every fg poll tick.
        let fg_hwnd = if fg_poll_due {
            self.win_last_fg_poll = now;
            super::devices::get_foreground_window_hwnd()
        } else {
            None
        };

        let fullscreen_candidates = if enum_poll_due {
            super::devices::find_fullscreen_windows()
        } else {
            Vec::new()
        };

        // Collect updates needed. For enum-based modes, only run on the slower tick.
        let mut updates: Vec<(SourceId, u64)> = Vec::new();
        let mut stale: Vec<SourceId> = Vec::new();

        for (&source_id, ws) in &self.win_watched_windows {
            match &ws.mode {
                WindowCaptureMode::ForegroundWindow => {
                    if let Some(hwnd) = fg_hwnd
                        && ws.current_hwnd != Some(hwnd)
                        && super::devices::is_window_valid(hwnd)
                    {
                        updates.push((source_id, hwnd));
                    }
                }
                WindowCaptureMode::ForegroundOnHotkey => {
                    // Check if current window died.
                    if let Some(hwnd) = ws.current_hwnd
                        && !super::devices::is_window_valid(hwnd)
                    {
                        stale.push(source_id);
                    }
                }
                WindowCaptureMode::SpecificWindow {
                    process_name,
                    window_title,
                } if enum_poll_due => {
                    // Re-resolve if current window is gone or not yet captured.
                    let needs_resolve = ws.current_hwnd.is_none()
                        || ws
                            .current_hwnd
                            .is_some_and(|h| !super::devices::is_window_valid(h));
                    if needs_resolve {
                        if let Some((hwnd, _, _)) =
                            super::devices::find_window_by_process_and_title(
                                process_name,
                                window_title,
                            )
                        {
                            updates.push((source_id, hwnd));
                        } else if ws.current_hwnd.is_some() {
                            stale.push(source_id);
                        }
                    }
                }
                WindowCaptureMode::Application { bundle_id, .. } if enum_poll_due => {
                    // On Windows, bundle_id is the process name. Track frontmost window.
                    if let Some((hwnd, _, _)) =
                        super::devices::find_app_window_by_process(bundle_id)
                    {
                        if ws.current_hwnd != Some(hwnd) {
                            updates.push((source_id, hwnd));
                        }
                    } else if ws.current_hwnd.is_some() {
                        stale.push(source_id);
                    }
                }
                WindowCaptureMode::AnyFullscreen if enum_poll_due => {
                    if let Some(hwnd) = select_fullscreen_candidate(ws, &fullscreen_candidates) {
                        if ws.current_hwnd != Some(hwnd) {
                            updates.push((source_id, hwnd));
                        }
                    } else if ws.current_hwnd.is_some() {
                        stale.push(source_id);
                    }
                }
                _ => {} // Not due for poll yet
            }
        }

        if enum_poll_due {
            self.win_last_enum_poll = now;
        }

        for (source_id, hwnd) in updates {
            self.start_win_window_capture(source_id, hwnd);
        }

        for source_id in stale {
            log::info!("Captured window gone for {source_id:?}, clearing");
            if let Some(handle) = self.captures.remove(&source_id) {
                if let Some(ref running) = handle.capture_running {
                    running.store(false, Ordering::Relaxed);
                }
                let _ = handle.pipeline.set_state(gstreamer::State::Null);
            }
            if let Some(ws) = self.win_watched_windows.get_mut(&source_id) {
                ws.current_hwnd = None;
            }
        }
    }

    /// Handle the hotkey-triggered foreground window capture.
    #[cfg(target_os = "windows")]
    fn handle_capture_foreground_window(&mut self) {
        use crate::scene::WindowCaptureMode;

        let Some(hwnd) = super::devices::get_foreground_window_hwnd() else {
            log::warn!("CaptureForegroundWindow: no foreground window");
            return;
        };

        let title = super::devices::get_window_title_from_hwnd(hwnd);
        log::info!("Hotkey capture: foreground window \"{title}\" (HWND {hwnd:#x})");

        let hotkey_sources: Vec<SourceId> = self
            .win_watched_windows
            .iter()
            .filter(|(_, ws)| matches!(ws.mode, WindowCaptureMode::ForegroundOnHotkey))
            .map(|(&id, _)| id)
            .collect();

        for source_id in hotkey_sources {
            self.start_win_window_capture(source_id, hwnd);
        }
    }


    #[cfg(not(target_os = "windows"))]
    fn handle_capture_foreground_window(&mut self) {
        log::warn!("CaptureForegroundWindow is only supported on Windows");
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

    // ── Game capture (Windows) ──────────────────────────────────────────

    /// Start a game capture: create shared memory, inject the hook DLL.
    #[cfg(target_os = "windows")]
    fn watch_game_capture(
        &mut self,
        source_id: SourceId,
        process_id: u32,
        process_name: String,
        window_title: String,
    ) {
        let now = std::time::Instant::now();
        self.game_captures.insert(
            source_id,
            GameCaptureSource {
                process_id: None,
                process_name,
                window_title,
                handles: None,
                last_frame_index: 0,
                failed_process_id: None,
                last_alive_check: now,
                last_resolve_attempt: now.checked_sub(WIN_ENUM_POLL_INTERVAL).unwrap_or(now),
            },
        );

        if process_id != 0 {
            self.activate_game_capture(source_id, process_id);
        } else {
            self.resolve_game_capture(source_id);
        }
    }

    /// Start a game capture: create shared memory, inject the hook DLL.
    #[cfg(target_os = "windows")]
    fn activate_game_capture(&mut self, source_id: SourceId, process_id: u32) {
        use super::inject;

        let Some((process_name, failed_process_id)) = self.game_captures.get(&source_id).map(|gc| {
            (gc.process_name.clone(), gc.failed_process_id)
        }) else {
            return;
        };

        if failed_process_id == Some(process_id) {
            return;
        }

        // Create shared memory and events.
        let handles = match inject::create_shared_capture(process_id) {
            Ok(h) => h,
            Err(e) => {
                log::error!("Game capture: failed to create shared memory for {source_id:?}: {e}");
                if let Some(gc) = self.game_captures.get_mut(&source_id) {
                    gc.failed_process_id = Some(process_id);
                    gc.last_resolve_attempt = std::time::Instant::now();
                }
                let _ = self.channels.error_tx.send(GstError::CaptureFailure {
                    message: format!("Game capture setup failed: {e}"),
                });
                return;
            }
        };

        // Locate the hook DLL next to our executable.
        let dll_path = match std::env::current_exe() {
            Ok(exe) => exe.parent().unwrap_or(std::path::Path::new(".")).join("lodestone_hook.dll"),
            Err(_) => std::path::PathBuf::from("lodestone_hook.dll"),
        };

        if !dll_path.exists() {
            log::error!("Game capture: hook DLL not found at {}", dll_path.display());
            inject::cleanup_shared_capture(handles);
            if let Some(gc) = self.game_captures.get_mut(&source_id) {
                gc.failed_process_id = Some(process_id);
                gc.last_resolve_attempt = std::time::Instant::now();
            }
            let _ = self.channels.error_tx.send(GstError::CaptureFailure {
                message: format!(
                    "Hook DLL not found at {}. Build lodestone-hook first.",
                    dll_path.display()
                ),
            });
            return;
        }

        // Inject.
        if let Err(e) = inject::inject_hook_dll(process_id, &dll_path) {
            log::error!("Game capture: injection failed for {source_id:?}: {e}");
            inject::cleanup_shared_capture(handles);
            if let Some(gc) = self.game_captures.get_mut(&source_id) {
                gc.failed_process_id = Some(process_id);
                gc.last_resolve_attempt = std::time::Instant::now();
            }
            let _ = self.channels.error_tx.send(GstError::CaptureFailure {
                message: format!("Game capture injection failed: {e}"),
            });
            return;
        }

        log::info!(
            "Game capture started for source {source_id:?}: process \"{process_name}\" (PID {process_id})"
        );

        if let Some(gc) = self.game_captures.get_mut(&source_id) {
            gc.process_id = Some(process_id);
            gc.handles = Some(handles);
            gc.last_frame_index = 0;
            gc.failed_process_id = None;
            gc.last_alive_check = std::time::Instant::now();
        }
    }

    #[cfg(target_os = "windows")]
    fn resolve_game_capture(&mut self, source_id: SourceId) {
        let Some((process_name, window_title, failed_process_id)) =
            self.game_captures.get(&source_id).map(|gc| {
                (
                    gc.process_name.clone(),
                    gc.window_title.clone(),
                    gc.failed_process_id,
                )
            })
        else {
            return;
        };

        let target = super::devices::enumerate_windows().into_iter().find(|window| {
            window.process_name.eq_ignore_ascii_case(&process_name)
                && (window_title.is_empty()
                    || window.title == window_title
                    || window.title.contains(&window_title))
                && Some(window.process_id) != failed_process_id
        });

        if let Some(window) = target {
            self.activate_game_capture(source_id, window.process_id);
        }
    }

    /// Poll all active game captures for new frames and check process liveness.
    #[cfg(target_os = "windows")]
    fn poll_game_captures(&mut self) {
        use super::inject;

        let now = std::time::Instant::now();
        let mut dead_sources: Vec<SourceId> = Vec::new();
        let mut pending_resolve: Vec<SourceId> = Vec::new();

        for (&source_id, gc) in self.game_captures.iter_mut() {
            let Some(ref handles) = gc.handles else {
                if now.duration_since(gc.last_resolve_attempt) >= WIN_ENUM_POLL_INTERVAL {
                    gc.last_resolve_attempt = now;
                    pending_resolve.push(source_id);
                }
                continue;
            };

            // Try to read a new frame.
            if let Some(frame) = inject::try_read_frame(handles, &mut gc.last_frame_index) {
                if let Ok(mut frames) = self.channels.latest_frames.lock() {
                    frames.insert(source_id, frame);
                }
            }

            // Periodically check if the target process is still alive.
            if now.duration_since(gc.last_alive_check) >= std::time::Duration::from_secs(2) {
                gc.last_alive_check = now;
                if !inject::is_process_alive(handles) {
                    log::info!(
                        "Game capture: process \"{}\" (PID {}) exited — stopping capture for {source_id:?}",
                        gc.process_name,
                        gc.process_id.unwrap_or_default(),
                    );
                    dead_sources.push(source_id);
                }
            }
        }

        // Clean up dead captures but keep the watched source so it can reattach.
        for source_id in dead_sources {
            if let Some(gc) = self.game_captures.get_mut(&source_id) {
                if let Some(handles) = gc.handles.take() {
                    Self::stop_game_capture_handles(handles);
                }
                gc.process_id = None;
                gc.last_frame_index = 0;
                gc.last_alive_check = now;
                gc.last_resolve_attempt = now.checked_sub(WIN_ENUM_POLL_INTERVAL).unwrap_or(now);
                if let Ok(mut frames) = self.channels.latest_frames.lock() {
                    frames.remove(&source_id);
                }
            }
        }

        for source_id in pending_resolve {
            self.resolve_game_capture(source_id);
        }
    }

    /// Internal: signal shutdown and clean up a game capture source.
    #[cfg(target_os = "windows")]
    fn stop_game_capture_handles(handles: super::inject::SharedCaptureHandles) {
        use super::inject;

        inject::signal_shutdown(&handles);
        // Give the hook DLL a moment to unhook before we tear down shared memory.
        std::thread::sleep(std::time::Duration::from_millis(100));
        inject::cleanup_shared_capture(handles);
    }

    #[cfg(target_os = "macos")]
    fn handle_start_virtual_camera(&mut self, fps: u32) {
        use super::virtual_camera;
        if self.virtual_camera_handle.is_some() {
            log::debug!("Ignoring duplicate StartVirtualCamera command while virtual camera is active");
            return;
        }
        let width = 1920u32;
        let height = 1080u32;
        match virtual_camera::start_virtual_camera(width, height, fps) {
            Ok(handle) => {
                self.virtual_camera_handle = Some(handle);
                log::info!("Virtual camera started ({width}x{height} @ {fps}fps)");
                self.publish_runtime_state();
            }
            Err(e) => {
                log::error!("Failed to start virtual camera: {e}");
                let _ = self.channels.error_tx.send(GstError::CaptureFailure {
                    message: format!("Failed to start virtual camera: {e}"),
                });
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
            self.publish_runtime_state();
        }
    }

    #[cfg(not(target_os = "macos"))]
    fn handle_start_virtual_camera(&mut self, _fps: u32) {
        let _ = self.channels.error_tx.send(GstError::CaptureFailure {
            message: "Virtual camera is currently supported only on macOS".to_string(),
        });
    }
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
        // Stop all game captures.
        #[cfg(target_os = "windows")]
        for (_, gc) in self.game_captures.drain() {
            if let Some(handles) = gc.handles {
                Self::stop_game_capture_handles(handles);
            }
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
                self.stream_started_at = None;
                self.stream_total_frames = 0;
                self.stream_dropped_frames = 0;
                self.stream_last_stats_at = None;
                self.stream_last_output_bytes = 0;
                let _ = self.channels.stats_tx.send(PipelineStats::default());
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
                self.record_path = None;
            }
        }
        self.publish_runtime_state();
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
                let _ = Self::push_to_encode(appsrc, data, pts);
            }
            if let Some(appsrc) = record_appsrc {
                let _ = Self::push_to_encode(appsrc, data, pts);
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
                let is_recommended =
                    !found_recommended && (is_hw || encoder_type == EncoderType::H264x264);
                if is_recommended {
                    found_recommended = true;
                }
                encoders.push(AvailableEncoder {
                    encoder_type,
                    is_recommended,
                });
            }
        }

        if !found_recommended
            && let Some(enc) = encoders
                .iter_mut()
                .find(|e| e.encoder_type == EncoderType::H264x264)
        {
            enc.is_recommended = true;
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
            encoders
                .iter()
                .map(|e| e.encoder_type.display_name())
                .collect::<Vec<_>>()
        );
        let _ = self.channels.encoders_tx.send(encoders);

        // Enumerate audio devices for the UI, but do not auto-start capture.
        // Persisted device settings may be stale, and default device selection
        // is noisy when the platform has no accessible mic/loopback source.
        match super::devices::enumerate_audio_input_devices() {
            Ok(devices) => {
                let _ = self.channels.devices_tx.send(devices.clone());
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

            // Poll window watcher for target changes.
            #[cfg(target_os = "macos")]
            {
                let changes = self.window_watcher.poll(&self.watched_windows);
                for (source_id, new_target) in changes {
                    self.handle_window_target_change(source_id, new_target);
                }
            }
            #[cfg(target_os = "windows")]
            self.poll_win_foreground_window();
            #[cfg(target_os = "windows")]
            self.poll_game_captures();

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

            // Forward composited frames to encode pipelines.
            // Drain the channel to a Vec first (bounded snapshot) to avoid
            // starvation from the main thread refilling during processing.
            // Process ALL frames in order to avoid choppiness — every frame
            // gets a correct PTS based on elapsed time.
            {
                let mut frames: Vec<RgbaFrame> = Vec::new();
                while let Ok(frame) = self.channels.composited_frame_rx.try_recv() {
                    frames.push(frame);
                }
                for frame in &frames {
                    let frame_pts =
                        gstreamer::ClockTime::from_nseconds(start_time.elapsed().as_nanos() as u64);
                    if let Some(ref handles) = self.stream_handles {
                        if Self::push_to_encode(&handles.video_appsrc, &frame.data, frame_pts) {
                            self.stream_total_frames += 1;
                        } else {
                            self.stream_dropped_frames += 1;
                        }
                    }
                    if let Some(ref handles) = self.record_handles {
                        let _ = Self::push_to_encode(&handles.video_appsrc, &frame.data, frame_pts);
                    }
                    #[cfg(target_os = "macos")]
                    if let Some(ref handle) = self.virtual_camera_handle
                        && let Err(e) = super::virtual_camera::write_frame(handle, &frame)
                    {
                        log::warn!("Virtual camera frame write failed: {e}");
                    }
                }
            }

            if let Some(started_at) = self.stream_started_at {
                let now = std::time::Instant::now();
                let output_bytes = self
                    .stream_handles
                    .as_ref()
                    .map(|handles| handles.telemetry.output_bytes())
                    .unwrap_or(0);
                let bitrate_kbps = self
                    .stream_last_stats_at
                    .map(|last_at| {
                        let elapsed = now.duration_since(last_at).as_secs_f64();
                        if elapsed <= f64::EPSILON {
                            0.0
                        } else {
                            let delta_bytes = output_bytes.saturating_sub(self.stream_last_output_bytes);
                            ((delta_bytes as f64) * 8.0 / 1000.0) / elapsed
                        }
                    })
                    .unwrap_or(0.0);
                self.stream_last_stats_at = Some(now);
                self.stream_last_output_bytes = output_bytes;
                let _ = self.channels.stats_tx.send(PipelineStats {
                    bitrate_kbps,
                    dropped_frames: self.stream_dropped_frames,
                    total_frames: self.stream_total_frames,
                    uptime_secs: started_at.elapsed().as_secs_f64(),
                });
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
                    capture_size: (1920, 1080),
                },
                fps: 30,
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
