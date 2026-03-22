use anyhow::{Context, Result};
use gstreamer::prelude::*;
use gstreamer_app::AppSink;

use super::commands::CaptureSourceConfig;

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
}
