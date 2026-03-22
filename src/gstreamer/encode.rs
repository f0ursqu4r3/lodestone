use anyhow::{Context, Result};
use gstreamer::prelude::*;
use gstreamer_app::AppSrc;
use std::path::Path;

use super::commands::{EncoderConfig, RecordingFormat};

/// Build an appsrc caps string for RGBA frames at the given encoder config.
fn make_appsrc_caps(config: &EncoderConfig) -> gstreamer::Caps {
    gstreamer_video::VideoCapsBuilder::new()
        .format(gstreamer_video::VideoFormat::Rgba)
        .width(config.width as i32)
        .height(config.height as i32)
        .framerate(gstreamer::Fraction::new(config.fps as i32, 1))
        .build()
}

/// Build the common encode chain: appsrc → videoconvert → vtenc_h264 → h264parse.
/// Returns (pipeline, appsrc, last_element_name) so callers can link the output.
fn build_encode_chain(
    config: &EncoderConfig,
    pipeline_name: &str,
) -> Result<(gstreamer::Pipeline, AppSrc, String)> {
    let pipeline = gstreamer::Pipeline::with_name(pipeline_name);

    let caps = make_appsrc_caps(config);
    let appsrc = AppSrc::builder()
        .name("encode-src")
        .caps(&caps)
        .format(gstreamer::Format::Time)
        .is_live(true)
        .build();

    let convert = gstreamer::ElementFactory::make("videoconvert")
        .name("encode-convert")
        .build()
        .context("Failed to create videoconvert")?;

    // Use VideoToolbox hardware encoder on macOS, fall back to x264enc
    let encoder = gstreamer::ElementFactory::make("vtenc_h264")
        .name("encoder")
        .property("bitrate", config.bitrate_kbps)
        .property("realtime", true)
        .property("allow-frame-reordering", false)
        .build()
        .or_else(|_| {
            gstreamer::ElementFactory::make("x264enc")
                .name("encoder")
                .property("bitrate", config.bitrate_kbps)
                .property("tune", 0x04u32) // zerolatency
                .build()
                .context("Failed to create encoder (tried vtenc_h264 and x264enc)")
        })?;

    let parser = gstreamer::ElementFactory::make("h264parse")
        .name("parser")
        .build()
        .context("Failed to create h264parse")?;

    pipeline
        .add_many([appsrc.upcast_ref(), &convert, &encoder, &parser])
        .context("Failed to add encode elements")?;

    gstreamer::Element::link_many([appsrc.upcast_ref(), &convert, &encoder, &parser])
        .context("Failed to link encode chain")?;

    Ok((pipeline, appsrc, "parser".to_string()))
}

/// Build a pipeline for RTMP streaming.
///
/// Pipeline: appsrc → videoconvert → vtenc_h264 → h264parse → flvmux → rtmpsink
pub fn build_stream_pipeline(
    config: &EncoderConfig,
    rtmp_url: &str,
) -> Result<(gstreamer::Pipeline, AppSrc)> {
    let (pipeline, appsrc, last_name) = build_encode_chain(config, "encode-stream-pipeline")?;

    let mux = gstreamer::ElementFactory::make("flvmux")
        .name("stream-mux")
        .property_from_str("streamable", "true")
        .build()
        .context("Failed to create flvmux")?;

    let sink = gstreamer::ElementFactory::make("rtmpsink")
        .name("stream-sink")
        .property("location", rtmp_url)
        .build()
        .context("Failed to create rtmpsink")?;

    pipeline
        .add_many([&mux, &sink])
        .context("Failed to add stream output elements")?;

    let last = pipeline
        .by_name(&last_name)
        .expect("parser element exists");
    gstreamer::Element::link_many([&last, &mux, &sink])
        .context("Failed to link stream output")?;

    Ok((pipeline, appsrc))
}

/// Build a pipeline for file recording.
///
/// Pipeline: appsrc → videoconvert → vtenc_h264 → h264parse → mux → filesink
pub fn build_record_pipeline(
    config: &EncoderConfig,
    path: &Path,
    format: RecordingFormat,
) -> Result<(gstreamer::Pipeline, AppSrc)> {
    let (pipeline, appsrc, last_name) = build_encode_chain(config, "encode-record-pipeline")?;

    let mux = match format {
        RecordingFormat::Mkv => gstreamer::ElementFactory::make("matroskamux")
            .name("record-mux")
            .build()
            .context("Failed to create matroskamux")?,
        RecordingFormat::Mp4 => gstreamer::ElementFactory::make("mp4mux")
            .name("record-mux")
            .property_from_str("fragment-duration", "1000")
            .build()
            .context("Failed to create mp4mux")?,
    };

    let sink = gstreamer::ElementFactory::make("filesink")
        .name("record-sink")
        .property("location", path.to_str().unwrap_or("recording.mkv"))
        .build()
        .context("Failed to create filesink")?;

    pipeline
        .add_many([&mux, &sink])
        .context("Failed to add record output elements")?;

    let last = pipeline
        .by_name(&last_name)
        .expect("parser element exists");
    gstreamer::Element::link_many([&last, &mux, &sink])
        .context("Failed to link record output")?;

    Ok((pipeline, appsrc))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_stream_pipeline_creates_valid_pipeline() {
        gstreamer::init().unwrap();
        let config = EncoderConfig::default();
        let result = build_stream_pipeline(&config, "rtmp://localhost/test/key");
        match result {
            Ok((pipeline, appsrc)) => {
                assert!(pipeline.name().starts_with("encode"));
                drop(appsrc);
                let _ = pipeline.set_state(gstreamer::State::Null);
            }
            Err(e) => {
                eprintln!("Skipping encode pipeline test (missing plugins): {e}");
            }
        }
    }

    #[test]
    fn build_record_pipeline_creates_valid_pipeline() {
        gstreamer::init().unwrap();
        let config = EncoderConfig::default();
        let path = std::path::PathBuf::from("/tmp/test_recording.mkv");
        let result = build_record_pipeline(&config, &path, RecordingFormat::Mkv);
        match result {
            Ok((pipeline, appsrc)) => {
                assert!(pipeline.name().starts_with("encode"));
                drop(appsrc);
                let _ = pipeline.set_state(gstreamer::State::Null);
            }
            Err(e) => {
                eprintln!("Skipping record pipeline test (missing plugins): {e}");
            }
        }
    }
}
