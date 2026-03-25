use anyhow::{Context, Result};
use gstreamer::prelude::*;
use gstreamer_app::{AppSink, AppSrc};

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
            gstreamer::ElementFactory::make("avfvideosrc")
                .name("capture-source")
                .property("capture-screen", true)
                .property("capture-screen-cursor", true)
                .property("device-index", *screen_index as i32)
                .build()
                .context("Failed to create avfvideosrc — is GStreamer installed?")?
        }
        CaptureSourceConfig::Window { .. } => {
            anyhow::bail!("Window capture built separately");
        }
        CaptureSourceConfig::Camera { device_index } => {
            gstreamer::ElementFactory::make("avfvideosrc")
                .name("capture-source")
                .property("device-index", *device_index as i32)
                .build()
                .context("Failed to create avfvideosrc for camera capture")?
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

/// Capture a single frame from a macOS window using CoreGraphics.
///
/// Returns `Some((rgba_bytes, width, height))` if the window is available,
/// or `None` if the window cannot be captured (e.g., it has been closed).
/// CoreGraphics outputs BGRA pixel data, so this function swaps B and R
/// channels to produce RGBA output.
#[cfg(target_os = "macos")]
pub fn grab_window_frame(window_id: u32) -> Option<(Vec<u8>, u32, u32)> {
    use core_graphics::geometry::{CGPoint, CGRect, CGSize};
    use core_graphics::window::{
        kCGWindowImageBoundsIgnoreFraming, kCGWindowImageNominalResolution,
        kCGWindowListOptionIncludingWindow,
    };

    // A zero-sized rect tells CoreGraphics to use the window's own bounds.
    let null_rect = CGRect::new(&CGPoint::new(0.0, 0.0), &CGSize::new(0.0, 0.0));

    let image = core_graphics::window::create_image(
        null_rect,
        kCGWindowListOptionIncludingWindow,
        window_id,
        kCGWindowImageBoundsIgnoreFraming | kCGWindowImageNominalResolution,
    )?;

    let width = image.width() as u32;
    let height = image.height() as u32;
    let bytes_per_row = image.bytes_per_row();
    let cf_data = image.data();
    let raw = cf_data.bytes();

    // Convert from BGRA (CoreGraphics native) to RGBA, handling row padding.
    let stride = (width as usize) * 4;
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for row in 0..height as usize {
        let row_start = row * bytes_per_row;
        let row_end = row_start + stride;
        if row_end > raw.len() {
            break;
        }
        let row_slice = &raw[row_start..row_end];
        for pixel in row_slice.chunks_exact(4) {
            // BGRA → RGBA: swap B and R
            rgba.push(pixel[2]); // R
            rgba.push(pixel[1]); // G
            rgba.push(pixel[0]); // B
            rgba.push(pixel[3]); // A
        }
    }

    Some((rgba, width, height))
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
#[cfg(target_os = "macos")]
pub fn build_display_capture_pipeline(
    width: u32,
    height: u32,
    fps: u32,
) -> Result<(gstreamer::Pipeline, AppSink, AppSrc)> {
    let pipeline = gstreamer::Pipeline::with_name("display-capture-pipeline");

    let src_caps = gstreamer_video::VideoCapsBuilder::new()
        .format(gstreamer_video::VideoFormat::Rgba)
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

/// Build a GStreamer pipeline for window capture using `appsrc`.
///
/// Returns `(pipeline, appsink, appsrc)`. The caller pushes RGBA frames into
/// the `appsrc` from a dedicated capture thread; the `appsink` emits frames
/// for the compositor.
///
/// Pipeline: `appsrc → videoconvert → videoscale → appsink`
#[cfg(target_os = "macos")]
pub fn build_window_capture_pipeline(
    width: u32,
    height: u32,
    fps: u32,
) -> Result<(gstreamer::Pipeline, AppSink, AppSrc)> {
    let pipeline = gstreamer::Pipeline::with_name("window-capture-pipeline");

    let src_caps = gstreamer_video::VideoCapsBuilder::new()
        .format(gstreamer_video::VideoFormat::Rgba)
        .width(width as i32)
        .height(height as i32)
        .framerate(gstreamer::Fraction::new(fps as i32, 1))
        .build();

    let appsrc = AppSrc::builder()
        .name("window-capture-src")
        .caps(&src_caps)
        .is_live(true)
        .format(gstreamer::Format::Time)
        .do_timestamp(true)
        .build();

    let convert = gstreamer::ElementFactory::make("videoconvert")
        .name("window-capture-convert")
        .build()
        .context("Failed to create videoconvert for window capture")?;

    let scale = gstreamer::ElementFactory::make("videoscale")
        .name("window-capture-scale")
        .build()
        .context("Failed to create videoscale for window capture")?;

    let sink_caps = gstreamer_video::VideoCapsBuilder::new()
        .format(gstreamer_video::VideoFormat::Rgba)
        .width(width as i32)
        .height(height as i32)
        .framerate(gstreamer::Fraction::new(fps as i32, 1))
        .build();

    let appsink = AppSink::builder()
        .name("window-capture-sink")
        .caps(&sink_caps)
        .max_buffers(2)
        .drop(true)
        .build();

    pipeline
        .add_many([appsrc.upcast_ref(), &convert, &scale, appsink.upcast_ref()])
        .context("Failed to add elements to window capture pipeline")?;

    gstreamer::Element::link_many([appsrc.upcast_ref(), &convert, &scale, appsink.upcast_ref()])
        .context("Failed to link window capture pipeline elements")?;

    Ok((pipeline, appsink, appsrc))
}

/// Build an audio capture pipeline for the given device.
///
/// Pipeline: osxaudiosrc → audioconvert → audioresample → volume → level → appsink
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

    let src = gstreamer::ElementFactory::make("osxaudiosrc")
        .name(format!("{name}-src"))
        .property("unique-id", device_uid)
        .build()
        .context(format!("Failed to create osxaudiosrc for {name}"))?;

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

    let caps = gstreamer_audio::AudioCapsBuilder::new()
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
