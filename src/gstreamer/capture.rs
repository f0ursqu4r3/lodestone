use anyhow::{Context, Result};
use gstreamer::prelude::*;
use gstreamer_app::AppSink;
#[cfg(target_os = "macos")]
use gstreamer_app::AppSrc;

use super::commands::{AudioSourceKind, CaptureSourceConfig};
use crate::scene::{AudioEffectInstance, AudioEffectKind};

/// Build a GStreamer capture pipeline for the given source.
///
/// Returns the pipeline and appsink element. The caller is responsible for
/// setting the pipeline to Playing state and pulling samples from the appsink.
pub fn build_capture_pipeline(
    source: &CaptureSourceConfig,
    width: u32,
    height: u32,
    fps: u32,
) -> Result<(gstreamer::Pipeline, AppSink)> {
    let pipeline = gstreamer::Pipeline::with_name("capture-pipeline");

    // Create source element based on capture config
    let src = match source {
        CaptureSourceConfig::Screen { screen_index, .. } => {
            #[cfg(target_os = "macos")]
            {
                gstreamer::ElementFactory::make("avfvideosrc")
                    .name("capture-source")
                    .property("capture-screen", true)
                    .property("capture-screen-cursor", true)
                    .property("device-index", *screen_index as i32)
                    .build()
                    .context("Failed to create avfvideosrc — is GStreamer installed?")?
            }
            #[cfg(target_os = "windows")]
            {
                // Use d3d11screencapturesrc for hardware-accelerated screen capture on Windows.
                // Falls back to dx9screencapsrc if d3d11 is unavailable.
                gstreamer::ElementFactory::make("d3d11screencapturesrc")
                    .name("capture-source")
                    .property("show-cursor", true)
                    .property("monitor-index", *screen_index as i32)
                    .build()
                    .or_else(|_| {
                        gstreamer::ElementFactory::make("dx9screencapsrc")
                            .name("capture-source")
                            .property("monitor", *screen_index as i32)
                            .build()
                    })
                    .context("Failed to create screen capture source — is GStreamer installed with d3d11 plugins?")?
            }
            #[cfg(not(any(target_os = "macos", target_os = "windows")))]
            {
                let _ = screen_index;
                anyhow::bail!("Screen capture not yet supported on this platform");
            }
        }
        CaptureSourceConfig::Window { .. } => {
            #[cfg(target_os = "macos")]
            {
                anyhow::bail!("Window capture uses ScreenCaptureKit on macOS");
            }
            #[cfg(not(target_os = "macos"))]
            {
                anyhow::bail!("Window capture requires a resolved HWND — use WindowHandle config");
            }
        }
        CaptureSourceConfig::WindowHandle { hwnd, .. } => {
            #[cfg(target_os = "windows")]
            {
                // Per-window capture requires Windows Graphics Capture API (capture-api=wgc).
                // The default DXGI Desktop Duplication API does not support per-window capture.
                gstreamer::ElementFactory::make("d3d11screencapturesrc")
                    .name("capture-source")
                    .property("show-cursor", true)
                    .property("show-border", false)
                    .property_from_str("capture-api", "wgc")
                    .property("window-handle", *hwnd as u64)
                    .build()
                    .context("Failed to create d3d11screencapturesrc with window-handle (WGC) — GStreamer 1.24+ required")?
            }
            #[cfg(not(target_os = "windows"))]
            {
                let _ = hwnd;
                anyhow::bail!("HWND-based window capture is Windows-only");
            }
        }
        CaptureSourceConfig::Camera { device_index } => {
            #[cfg(target_os = "macos")]
            {
                gstreamer::ElementFactory::make("avfvideosrc")
                    .name("capture-source")
                    .property("device-index", *device_index as i32)
                    .build()
                    .context("Failed to create avfvideosrc for camera capture")?
            }
            #[cfg(target_os = "windows")]
            {
                // Use Media Foundation source on Windows; fall back to ksvideosrc.
                gstreamer::ElementFactory::make("mfvideosrc")
                    .name("capture-source")
                    .property("device-index", *device_index as i32)
                    .build()
                    .or_else(|_| {
                        gstreamer::ElementFactory::make("ksvideosrc")
                            .name("capture-source")
                            .property("device-index", *device_index as i32)
                            .build()
                    })
                    .context("Failed to create camera capture source — is GStreamer installed?")?
            }
            #[cfg(not(any(target_os = "macos", target_os = "windows")))]
            {
                let _ = device_index;
                anyhow::bail!("Camera capture not yet supported on this platform");
            }
        }
        // Audio-only sources are not routed through the video capture pipeline.
        CaptureSourceConfig::AudioDevice { .. } | CaptureSourceConfig::AudioFile { .. } => {
            anyhow::bail!("Audio sources are not handled by the video capture pipeline");
        }
        CaptureSourceConfig::GameCapture { .. } => {
            anyhow::bail!(
                "Game capture uses the Windows hook pipeline, not the video capture pipeline"
            );
        }
    };

    let convert = gstreamer::ElementFactory::make("videoconvert")
        .name("capture-convert")
        .build()
        .context("Failed to create videoconvert")?;

    let scale = gstreamer::ElementFactory::make("videoscale")
        .name("capture-scale")
        .build()
        .context("Failed to create videoscale")?;

    let rate = gstreamer::ElementFactory::make("videorate")
        .name("capture-rate")
        .build()
        .context("Failed to create videorate")?;

    // Configure appsink caps. For cameras and window captures, don't force a
    // resolution — let the device negotiate its native size so the aspect ratio
    // is preserved. For screen capture, force the target resolution.
    let caps = match source {
        CaptureSourceConfig::Camera { .. } | CaptureSourceConfig::WindowHandle { .. } => {
            gstreamer_video::VideoCapsBuilder::new()
                .format(gstreamer_video::VideoFormat::Rgba)
                .framerate(gstreamer::Fraction::new(fps as i32, 1))
                .build()
        }
        _ => gstreamer_video::VideoCapsBuilder::new()
            .format(gstreamer_video::VideoFormat::Rgba)
            .width(width as i32)
            .height(height as i32)
            .framerate(gstreamer::Fraction::new(fps as i32, 1))
            .build(),
    };

    let appsink = AppSink::builder()
        .name("capture-sink")
        .caps(&caps)
        .max_buffers(2)
        .drop(true)
        .build();

    pipeline
        .add_many([&src, &convert, &scale, &rate, appsink.upcast_ref()])
        .context("Failed to add elements to capture pipeline")?;

    gstreamer::Element::link_many([&src, &convert, &scale, &rate, appsink.upcast_ref()])
        .context("Failed to link capture pipeline elements")?;

    Ok((pipeline, appsink))
}

/// Build a GStreamer pipeline for display capture using `appsrc`.
///
/// Returns `(pipeline, appsink, appsrc)`. The caller pushes RGBA frames from
/// ScreenCaptureKit into the `appsrc`; the `appsink` emits frames for the
/// compositor.
///
/// Pipeline: `appsrc → videoconvert → videoscale → appsink`
///
/// No `videorate` — ScreenCaptureKit controls frame timing via
/// `setMinimumFrameInterval`.
///
/// The appsrc accepts BGRA (native SCK pixel format) and `videoconvert`
/// handles the BGRA→RGBA conversion using SIMD — much faster than a
/// manual per-pixel Rust loop.
#[cfg(target_os = "macos")]
pub fn build_display_capture_pipeline(
    width: u32,
    height: u32,
    fps: u32,
) -> Result<(gstreamer::Pipeline, AppSink, AppSrc)> {
    let pipeline = gstreamer::Pipeline::with_name("display-capture-pipeline");

    let src_caps = gstreamer_video::VideoCapsBuilder::new()
        .format(gstreamer_video::VideoFormat::Bgra)
        .width(width as i32)
        .height(height as i32)
        .framerate(gstreamer::Fraction::new(fps as i32, 1))
        .build();

    let appsrc = AppSrc::builder()
        .name("display-capture-src")
        .caps(&src_caps)
        .is_live(true)
        .format(gstreamer::Format::Time)
        .do_timestamp(true)
        .build();

    let convert = gstreamer::ElementFactory::make("videoconvert")
        .name("display-capture-convert")
        .build()
        .context("Failed to create videoconvert for display capture")?;

    let scale = gstreamer::ElementFactory::make("videoscale")
        .name("display-capture-scale")
        .build()
        .context("Failed to create videoscale for display capture")?;

    let sink_caps = gstreamer_video::VideoCapsBuilder::new()
        .format(gstreamer_video::VideoFormat::Rgba)
        .width(width as i32)
        .height(height as i32)
        .framerate(gstreamer::Fraction::new(fps as i32, 1))
        .build();

    let appsink = AppSink::builder()
        .name("display-capture-sink")
        .caps(&sink_caps)
        .max_buffers(2)
        .drop(true)
        .build();

    pipeline
        .add_many([appsrc.upcast_ref(), &convert, &scale, appsink.upcast_ref()])
        .context("Failed to add elements to display capture pipeline")?;

    gstreamer::Element::link_many([appsrc.upcast_ref(), &convert, &scale, appsink.upcast_ref()])
        .context("Failed to link display capture pipeline elements")?;

    Ok((pipeline, appsink, appsrc))
}

/// Build an audio capture pipeline for the given device.
///
/// Pipeline: audio-src → audioconvert → audioresample → volume → level → appsink
/// (osxaudiosrc on macOS, wasapisrc on Windows)
/// Returns (pipeline, appsink, volume_element_name).
#[cfg_attr(target_os = "windows", allow(dead_code))]
pub fn build_audio_capture_pipeline(
    source_kind: AudioSourceKind,
    device_uid: &str,
    sample_rate: u32,
    effects: &[AudioEffectInstance],
) -> Result<(gstreamer::Pipeline, AppSink, String, Vec<AudioEffectInstance>)> {
    build_audio_capture_pipeline_with_source(source_kind, device_uid, sample_rate, None, effects)
}

/// Convert a user-facing dB value to a linear amplitude multiplier.
pub fn db_to_linear(db: f32) -> f64 {
    10f64.powf(db as f64 / 20.0)
}

/// The standard name used for an effect element at a given chain position.
/// Live updates rely on this to look up the element by name.
pub fn audio_effect_element_name(pipeline_prefix: &str, index: usize) -> String {
    format!("{pipeline_prefix}-fx-{index}")
}

/// Build the GStreamer element for a single audio effect.
///
/// Callers pass the pre-computed element name (see
/// [`audio_effect_element_name`]) so it can be found later for live updates.
pub fn build_audio_effect_element(
    effect: &AudioEffectInstance,
    name: &str,
) -> Result<gstreamer::Element> {
    match &effect.kind {
        AudioEffectKind::HighPass { cutoff_hz } => {
            let cutoff = cutoff_hz.max(20.0) as f32;
            gstreamer::ElementFactory::make("audiocheblimit")
                .name(name)
                .property_from_str("mode", "high-pass")
                .property("cutoff", cutoff)
                .property("poles", 4i32)
                .build()
                .context("Failed to create audiocheblimit (High-Pass)")
        }
        AudioEffectKind::NoiseSuppression { level } => {
            let level_name = match level.min(&3u32) {
                0 => "low",
                1 => "moderate",
                2 => "high",
                _ => "very-high",
            };
            gstreamer::ElementFactory::make("webrtcdsp")
                .name(name)
                .property_from_str("noise-suppression-level", level_name)
                .property("echo-cancel", false)
                .property("voice-detection", false)
                .property("high-pass-filter", false)
                .property("gain-control", false)
                .build()
                .context(
                    "Failed to create webrtcdsp (Noise Suppression). \
                    The gst-plugins-bad `webrtc` plugin is required for this effect.",
                )
        }
        AudioEffectKind::NoiseGate {
            threshold_db,
            ratio,
        } => {
            let threshold_linear = db_to_linear(*threshold_db).clamp(0.0, 1.0);
            // User-facing ratio is "N:1 attenuation"; audiodynamic's expander
            // ratio is a linear coefficient where lower = more attenuation.
            let gst_ratio = (1.0 / ratio.max(1.0) as f64).clamp(0.0, 1.0);
            gstreamer::ElementFactory::make("audiodynamic")
                .name(name)
                .property_from_str("mode", "expander")
                .property_from_str("characteristics", "soft-knee")
                .property("threshold", threshold_linear)
                .property("ratio", gst_ratio)
                .build()
                .context("Failed to create audiodynamic (Noise Gate)")
        }
        AudioEffectKind::Compressor {
            threshold_db,
            ratio,
        } => {
            let threshold_linear = db_to_linear(*threshold_db).clamp(0.0, 1.0);
            // User-facing ratio is "N:1"; audiodynamic compressor ratio is a
            // linear coefficient where lower = more compression above threshold.
            let gst_ratio = (1.0 / ratio.max(1.0) as f64).clamp(0.0, 1.0);
            gstreamer::ElementFactory::make("audiodynamic")
                .name(name)
                .property_from_str("mode", "compressor")
                .property_from_str("characteristics", "soft-knee")
                .property("threshold", threshold_linear)
                .property("ratio", gst_ratio)
                .build()
                .context("Failed to create audiodynamic (Compressor)")
        }
        AudioEffectKind::Gain { gain_db } => {
            let linear = db_to_linear(*gain_db) as f32;
            gstreamer::ElementFactory::make("audioamplify")
                .name(name)
                .property("amplification", linear)
                .property_from_str("clipping-method", "normal")
                .build()
                .context("Failed to create audioamplify (Gain)")
        }
    }
}

/// Apply new parameter values to an already-built effect element. Used for
/// live updates when the chain's structure hasn't changed.
pub fn apply_audio_effect_params(element: &gstreamer::Element, effect: &AudioEffectInstance) {
    match &effect.kind {
        AudioEffectKind::HighPass { cutoff_hz } => {
            element.set_property("cutoff", cutoff_hz.max(20.0) as f32);
        }
        AudioEffectKind::NoiseSuppression { level } => {
            let level_name = match level.min(&3u32) {
                0 => "low",
                1 => "moderate",
                2 => "high",
                _ => "very-high",
            };
            element.set_property_from_str("noise-suppression-level", level_name);
        }
        AudioEffectKind::NoiseGate {
            threshold_db,
            ratio,
        } => {
            element.set_property("threshold", db_to_linear(*threshold_db).clamp(0.0, 1.0));
            element.set_property(
                "ratio",
                (1.0 / ratio.max(1.0) as f64).clamp(0.0, 1.0),
            );
        }
        AudioEffectKind::Compressor {
            threshold_db,
            ratio,
        } => {
            element.set_property("threshold", db_to_linear(*threshold_db).clamp(0.0, 1.0));
            element.set_property(
                "ratio",
                (1.0 / ratio.max(1.0) as f64).clamp(0.0, 1.0),
            );
        }
        AudioEffectKind::Gain { gain_db } => {
            element.set_property("amplification", db_to_linear(*gain_db) as f32);
        }
    }
}

/// Build the enabled subset of an effect chain, returning (element, instance)
/// pairs in pipeline order.
pub fn build_audio_effect_chain(
    effects: &[AudioEffectInstance],
    name_prefix: &str,
) -> Result<Vec<(gstreamer::Element, AudioEffectInstance)>> {
    let mut out = Vec::new();
    for effect in effects.iter().filter(|e| e.enabled) {
        let index = out.len();
        let name = audio_effect_element_name(name_prefix, index);
        let element = build_audio_effect_element(effect, &name)?;
        out.push((element, effect.clone()));
    }
    Ok(out)
}

/// Build an audio capture pipeline, optionally forcing a specific platform source element.
pub fn build_audio_capture_pipeline_with_source(
    source_kind: AudioSourceKind,
    device_uid: &str,
    sample_rate: u32,
    #[allow(unused_variables)] preferred_source: Option<&str>,
    effects: &[AudioEffectInstance],
) -> Result<(gstreamer::Pipeline, AppSink, String, Vec<AudioEffectInstance>)> {
    let name = match source_kind {
        AudioSourceKind::Mic => "mic-capture",
        AudioSourceKind::System => "system-capture",
    };
    let pipeline = gstreamer::Pipeline::with_name(name);

    let src = {
        #[cfg(target_os = "macos")]
        {
            gstreamer::ElementFactory::make("osxaudiosrc")
                .name(format!("{name}-src"))
                .property("unique-id", device_uid)
                .build()
                .context(format!("Failed to create osxaudiosrc for {name}"))?
        }
        #[cfg(target_os = "windows")]
        {
            let source_name = preferred_source.unwrap_or("wasapi2src");
            let src_name = format!("{name}-src");

            // If we have a specific device_uid, prefer letting GStreamer's
            // DeviceMonitor build the source element — it picks the provider
            // (wasapi2 vs wasapi) that owns the device and sets the right
            // device property for that provider. Falling back to factory
            // construction with `device = <uid>` only works when the uid
            // happens to match what the factory's provider expects, which
            // isn't guaranteed across providers.
            let device_element = if device_uid.is_empty() {
                None
            } else {
                match super::devices::find_audio_input_device(device_uid) {
                    Some(device) => match device.create_element(Some(&src_name)) {
                        Ok(element) => {
                            log::info!(
                                "Built {name} source for uid='{device_uid}' via Device provider (factory='{factory}')",
                                factory = element
                                    .factory()
                                    .map(|f| f.name().to_string())
                                    .unwrap_or_else(|| "unknown".to_string()),
                            );
                            Some(element)
                        }
                        Err(err) => {
                            log::warn!(
                                "Device provider failed to create element for uid='{device_uid}': {err}; falling back to {source_name}"
                            );
                            None
                        }
                    },
                    None => {
                        log::warn!(
                            "No DeviceMonitor match for audio uid='{device_uid}'; falling back to {source_name} with device=<uid>"
                        );
                        None
                    }
                }
            };

            if let Some(element) = device_element {
                element
            } else {
                let builder = gstreamer::ElementFactory::make(source_name).name(&src_name);
                if device_uid.is_empty() {
                    builder
                        .build()
                        .context(format!("Failed to create {source_name} for {name}"))?
                } else {
                    builder
                        .property("device", device_uid)
                        .build()
                        .context(format!("Failed to create {source_name} for {name}"))?
                }
            }
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            let _ = device_uid;
            anyhow::bail!("Audio capture not yet supported on this platform");
        }
    };

    let convert = gstreamer::ElementFactory::make("audioconvert")
        .name(format!("{name}-convert"))
        .build()
        .context("Failed to create audioconvert")?;

    let resample = gstreamer::ElementFactory::make("audioresample")
        .name(format!("{name}-resample"))
        .build()
        .context("Failed to create audioresample")?;

    let volume_name = format!("{name}-volume");
    let volume = gstreamer::ElementFactory::make("volume")
        .name(&volume_name)
        .build()
        .context("Failed to create volume")?;

    let level = gstreamer::ElementFactory::make("level")
        .name(format!("{name}-level"))
        .property("interval", 50_000_000u64) // 50ms in nanoseconds
        .property("post-messages", true)
        .build()
        .context("Failed to create level")?;

    let caps = gstreamer_audio::AudioCapsBuilder::new_interleaved()
        .format(gstreamer_audio::AudioFormat::S16le)
        .rate(sample_rate as i32)
        .channels(2)
        .build();

    let appsink = AppSink::builder()
        .name(format!("{name}-sink"))
        .caps(&caps)
        .max_buffers(4)
        .drop(true)
        .build();

    let effect_elements = build_audio_effect_chain(effects, name)?;

    pipeline
        .add_many([
            &src,
            &convert,
            &resample,
            &volume,
            &level,
            appsink.upcast_ref(),
        ])
        .context("Failed to add audio capture elements")?;
    for (element, _) in &effect_elements {
        pipeline
            .add(element)
            .context("Failed to add audio effect element to pipeline")?;
    }

    // Link src → convert → resample → [effects...] → volume → level → appsink.
    gstreamer::Element::link_many([&src, &convert, &resample])
        .context("Failed to link audio capture pre-effect chain")?;
    let mut prev = resample.clone();
    for (element, _) in &effect_elements {
        prev.link(element)
            .context("Failed to link audio effect into pipeline")?;
        prev = element.clone();
    }
    prev.link(&volume)
        .context("Failed to link audio effect chain to volume")?;
    gstreamer::Element::link_many([&volume, &level, appsink.upcast_ref()])
        .context("Failed to link audio capture post-effect chain")?;

    let applied_effects: Vec<AudioEffectInstance> =
        effect_elements.into_iter().map(|(_, fx)| fx).collect();
    Ok((pipeline, appsink, volume_name, applied_effects))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_capture_pipeline_creates_valid_pipeline() {
        gstreamer::init().unwrap();
        let config = CaptureSourceConfig::Screen {
            screen_index: 0,
            exclude_self: false,
            capture_size: (1920, 1080),
        };
        let result = build_capture_pipeline(&config, 1920, 1080, 30);
        match result {
            Ok((pipeline, appsink)) => {
                assert!(pipeline.name().starts_with("capture"));
                drop(appsink);
                let _ = pipeline.set_state(gstreamer::State::Null);
            }
            Err(e) => {
                eprintln!("Skipping capture pipeline test (missing plugins): {e}");
            }
        }
    }

    #[test]
    fn build_camera_pipeline_creates_valid_pipeline() {
        gstreamer::init().unwrap();
        let config = CaptureSourceConfig::Camera { device_index: 0 };
        let result = build_capture_pipeline(&config, 1920, 1080, 30);
        // May fail if no camera — that's OK in CI.
        match result {
            Ok((pipeline, _sink)) => {
                assert!(pipeline.name().as_str().len() > 0);
            }
            Err(_) => {} // No camera available
        }
    }

    #[test]
    fn build_audio_capture_pipeline_creates_valid_pipeline() {
        gstreamer::init().unwrap();
        let result = build_audio_capture_pipeline(
            crate::gstreamer::commands::AudioSourceKind::Mic,
            "default",
            48000,
        );
        match result {
            Ok((pipeline, appsink, vol_name)) => {
                assert!(pipeline.name().starts_with("mic-capture"));
                assert!(vol_name.contains("volume"));
                drop(appsink);
                let _ = pipeline.set_state(gstreamer::State::Null);
            }
            Err(e) => {
                eprintln!("Skipping audio capture pipeline test (missing plugins): {e}");
            }
        }
    }
}
