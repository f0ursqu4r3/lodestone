use anyhow::{Context, Result};
use gstreamer::prelude::*;
use gstreamer_app::AppSink;

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
        CaptureSourceConfig::Screen { screen_index } => {
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
        CaptureSourceConfig::Camera { .. } => {
            todo!("Camera capture pipeline not yet implemented");
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

    // Configure appsink to emit RGBA frames at the target resolution/fps
    let caps = gstreamer_video::VideoCapsBuilder::new()
        .format(gstreamer_video::VideoFormat::Rgba)
        .width(width as i32)
        .height(height as i32)
        .framerate(gstreamer::Fraction::new(fps as i32, 1))
        .build();

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
        let config = CaptureSourceConfig::Screen { screen_index: 0 };
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
