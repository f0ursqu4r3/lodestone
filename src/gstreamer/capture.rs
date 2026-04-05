use anyhow::{Context, Result};
use gstreamer::prelude::*;
use gstreamer_app::AppSink;
#[cfg(target_os = "macos")]
use gstreamer_app::AppSrc;

use super::commands::{AudioSourceKind, CaptureSourceConfig};

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
            anyhow::bail!("Window capture built separately");
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

    // Configure appsink caps. For cameras, don't force a resolution — let the
    // device negotiate its native size so the aspect ratio is preserved. For
    // screen capture, force the target resolution.
    let caps = match source {
        CaptureSourceConfig::Camera { .. } => gstreamer_video::VideoCapsBuilder::new()
            .format(gstreamer_video::VideoFormat::Rgba)
            .framerate(gstreamer::Fraction::new(fps as i32, 1))
            .build(),
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
pub fn build_audio_capture_pipeline(
    source_kind: AudioSourceKind,
    device_uid: &str,
    sample_rate: u32,
) -> Result<(gstreamer::Pipeline, AppSink, String)> {
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
            // Prefer wasapi2src (modern WASAPI2 plugin, better device ID handling)
            // over the legacy wasapisrc. An empty UID selects the default device.
            if device_uid.is_empty() {
                gstreamer::ElementFactory::make("wasapi2src")
                    .name(format!("{name}-src"))
                    .build()
                    .or_else(|_| {
                        gstreamer::ElementFactory::make("wasapisrc")
                            .name(format!("{name}-src"))
                            .build()
                    })
                    .context(format!("Failed to create audio source for {name}"))?
            } else {
                gstreamer::ElementFactory::make("wasapi2src")
                    .name(format!("{name}-src"))
                    .property("device", device_uid)
                    .build()
                    .or_else(|_| {
                        gstreamer::ElementFactory::make("wasapisrc")
                            .name(format!("{name}-src"))
                            .property("device", device_uid)
                            .build()
                    })
                    .context(format!("Failed to create audio source for {name}"))?
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

    gstreamer::Element::link_many([
        &src,
        &convert,
        &resample,
        &volume,
        &level,
        appsink.upcast_ref(),
    ])
    .context("Failed to link audio capture elements")?;

    Ok((pipeline, appsink, volume_name))
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
