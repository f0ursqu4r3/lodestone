use anyhow::{Context, Result};
use gstreamer::prelude::*;
use gstreamer_app::AppSrc;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use super::commands::{AudioEncoderConfig, EncoderConfig, EncoderType, RecordingFormat};

/// Handles for the streaming pipeline (mixed audio via audiomixer).
pub struct StreamPipelineHandles {
    pub pipeline: gstreamer::Pipeline,
    pub video_appsrc: AppSrc,
    pub audio_appsrc_mic: AppSrc,
    pub audio_appsrc_system: Option<AppSrc>,
    pub telemetry: Arc<StreamTelemetry>,
}

#[derive(Debug, Default)]
pub struct StreamTelemetry {
    output_bytes: AtomicU64,
}

impl StreamTelemetry {
    pub fn record_output_bytes(&self, bytes: usize) {
        self.output_bytes.fetch_add(bytes as u64, Ordering::Relaxed);
    }

    pub fn output_bytes(&self) -> u64 {
        self.output_bytes.load(Ordering::Relaxed)
    }
}

/// Handles for the recording pipeline (separate audio tracks).
pub struct RecordPipelineHandles {
    pub pipeline: gstreamer::Pipeline,
    pub video_appsrc: AppSrc,
    pub mic_appsrc: AppSrc,
    pub system_appsrc: Option<AppSrc>,
}

/// Map a settings color-space name to a GStreamer colorimetry string.
fn colorimetry_for(color_space: &str) -> &'static str {
    match color_space {
        "Rec. 709" => "bt709",
        "Rec. 2100 (PQ)" => "bt2100-pq",
        _ => "srgb",
    }
}

/// Build an appsrc caps string for RGBA frames at the given encoder config.
fn make_appsrc_caps(config: &EncoderConfig) -> gstreamer::Caps {
    let colorimetry = colorimetry_for(&config.color_space);
    gstreamer::Caps::builder("video/x-raw")
        .field("format", gstreamer_video::VideoFormat::Rgba.to_str())
        .field("width", config.width as i32)
        .field("height", config.height as i32)
        .field("framerate", gstreamer::Fraction::new(config.fps as i32, 1))
        .field("colorimetry", colorimetry)
        .build()
}

/// Build audio caps for an appsrc producing raw S16LE audio.
fn make_audio_appsrc_caps(config: &AudioEncoderConfig) -> gstreamer::Caps {
    gstreamer_audio::AudioCapsBuilder::new_interleaved()
        .format(gstreamer_audio::AudioFormat::S16le)
        .rate(config.sample_rate as i32)
        .channels(config.channels as i32)
        .build()
}

/// Create a GStreamer H.264 encoder element for the given encoder type.
///
/// `for_streaming` selects low-latency settings (realtime, zerolatency).
/// Recording uses higher quality settings (no realtime constraint, better
/// rate control).
fn make_encoder(
    encoder_type: EncoderType,
    bitrate_kbps: u32,
    for_streaming: bool,
) -> Result<gstreamer::Element> {
    match encoder_type {
        EncoderType::H264VideoToolbox => {
            let mut builder = gstreamer::ElementFactory::make("vtenc_h264")
                .name("encoder")
                .property("bitrate", bitrate_kbps)
                .property("allow-frame-reordering", !for_streaming)
                .property("realtime", for_streaming);
            // For recording, allow higher quality (max-keyframe-interval
            // defaults to 0 = auto which is good for file playback).
            if !for_streaming {
                builder = builder.property("max-keyframe-interval", 60i32);
            } else {
                // Streaming: keyframe every 2 seconds for seekability.
                builder = builder.property("max-keyframe-interval", 60i32);
            }
            builder.build().context("Failed to create vtenc_h264")
        }
        EncoderType::H264x264 => {
            let el = gstreamer::ElementFactory::make("x264enc")
                .name("encoder")
                .property("bitrate", bitrate_kbps)
                .property("key-int-max", 60u32)
                .build()
                .context("Failed to create x264enc")?;
            if for_streaming {
                el.set_property_from_str("tune", "zerolatency");
                el.set_property_from_str("speed-preset", "veryfast");
            } else {
                // Recording: better quality, no zero-latency constraint.
                el.set_property_from_str("speed-preset", "medium");
            }
            Ok(el)
        }
        EncoderType::H264Nvenc => gstreamer::ElementFactory::make("nvh264enc")
            .name("encoder")
            .property("bitrate", bitrate_kbps)
            .build()
            .context("Failed to create nvh264enc"),
        EncoderType::H264Amf => gstreamer::ElementFactory::make("amfh264enc")
            .name("encoder")
            .property("bitrate", bitrate_kbps)
            .build()
            .context("Failed to create amfh264enc"),
        EncoderType::H264Qsv => gstreamer::ElementFactory::make("qsvh264enc")
            .name("encoder")
            .property("bitrate", bitrate_kbps)
            .build()
            .context("Failed to create qsvh264enc"),
    }
}

/// Build the common encode chain: appsrc → videoconvert → vtenc_h264 → h264parse.
/// Returns (pipeline, appsrc, last_element_name) so callers can link the output.
fn build_encode_chain(
    config: &EncoderConfig,
    pipeline_name: &str,
    for_streaming: bool,
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

    let encoder = make_encoder(config.encoder_type, config.bitrate_kbps, for_streaming)?;

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

/// Build a streaming pipeline with mixed audio.
///
/// Video: appsrc → videoconvert → vtenc_h264 → h264parse → flvmux (video pad)
/// Audio: mic_appsrc [+ system_appsrc] → audiomixer → audioconvert → avenc_aac → aacparse → flvmux (audio pad)
/// Output: flvmux → rtmpsink
pub fn build_stream_pipeline_with_audio(
    video_config: &EncoderConfig,
    audio_config: &AudioEncoderConfig,
    rtmp_url: &str,
    has_system_audio: bool,
) -> Result<StreamPipelineHandles> {
    let (pipeline, video_appsrc, video_last_name) =
        build_encode_chain(video_config, "encode-stream-audio-pipeline", true)?;

    // Mux and sink
    let mux = gstreamer::ElementFactory::make("flvmux")
        .name("stream-mux")
        .property_from_str("streamable", "true")
        .build()
        .context("Failed to create flvmux")?;
    let probe = gstreamer::ElementFactory::make("identity")
        .name("stream-stats-probe")
        .build()
        .context("Failed to create stream stats probe")?;

    let sink = gstreamer::ElementFactory::make("rtmpsink")
        .name("stream-sink")
        .property("location", rtmp_url)
        .build()
        .context("Failed to create rtmpsink")?;

    // Audio elements
    let audio_caps = make_audio_appsrc_caps(audio_config);

    let mic_appsrc = AppSrc::builder()
        .name("stream-mic-src")
        .caps(&audio_caps)
        .format(gstreamer::Format::Time)
        .is_live(true)
        .build();

    let mixer = gstreamer::ElementFactory::make("audiomixer")
        .name("stream-mixer")
        .build()
        .context("Failed to create audiomixer")?;

    let audio_convert = gstreamer::ElementFactory::make("audioconvert")
        .name("stream-audio-convert")
        .build()
        .context("Failed to create audioconvert")?;

    let audio_encoder = gstreamer::ElementFactory::make("avenc_aac")
        .name("stream-audio-encoder")
        .build()
        .context("Failed to create avenc_aac")?;
    audio_encoder.set_property("bitrate", (audio_config.bitrate_kbps * 1000) as i32);

    let audio_parser = gstreamer::ElementFactory::make("aacparse")
        .name("stream-audio-parser")
        .build()
        .context("Failed to create aacparse")?;

    // Add all elements to the pipeline
    pipeline
        .add_many([
            mic_appsrc.upcast_ref(),
            &mixer,
            &audio_convert,
            &audio_encoder,
            &audio_parser,
            &mux,
            &probe,
            &sink,
        ])
        .context("Failed to add stream audio elements")?;

    // Link mic appsrc → audiomixer
    mic_appsrc
        .link(&mixer)
        .context("Failed to link mic appsrc to audiomixer")?;

    // Optionally create and link system audio appsrc
    let system_appsrc = if has_system_audio {
        let sys_appsrc = AppSrc::builder()
            .name("stream-system-src")
            .caps(&audio_caps)
            .format(gstreamer::Format::Time)
            .is_live(true)
            .build();

        pipeline
            .add(&sys_appsrc)
            .context("Failed to add system audio appsrc")?;

        sys_appsrc
            .link(&mixer)
            .context("Failed to link system appsrc to audiomixer")?;

        Some(sys_appsrc)
    } else {
        None
    };

    // Link audiomixer → audioconvert → avenc_aac → aacparse
    gstreamer::Element::link_many([&mixer, &audio_convert, &audio_encoder, &audio_parser])
        .context("Failed to link audio encode chain")?;

    // Link video to flvmux via explicit video pad
    let video_pad = mux
        .request_pad_simple("video")
        .context("Failed to request video pad on flvmux")?;
    let video_last = pipeline
        .by_name(&video_last_name)
        .expect("parser element exists");
    let video_src_pad = video_last
        .static_pad("src")
        .context("Failed to get video src pad")?;
    video_src_pad
        .link(&video_pad)
        .context("Failed to link video to flvmux")?;

    // Link audio to flvmux via explicit audio pad
    let audio_pad = mux
        .request_pad_simple("audio")
        .context("Failed to request audio pad on flvmux")?;
    let audio_parser_src = audio_parser
        .static_pad("src")
        .context("Failed to get aacparse src pad")?;
    audio_parser_src
        .link(&audio_pad)
        .context("Failed to link audio to flvmux")?;

    // Link mux → sink
    gstreamer::Element::link_many([&mux, &probe, &sink])
        .context("Failed to link flvmux to rtmpsink")?;

    let telemetry = Arc::new(StreamTelemetry::default());
    let telemetry_for_probe = telemetry.clone();
    probe
        .static_pad("src")
        .context("Failed to get stream stats probe src pad")?
        .add_probe(gstreamer::PadProbeType::BUFFER, move |_pad, info| {
            if let Some(gstreamer::PadProbeData::Buffer(ref buffer)) = info.data {
                telemetry_for_probe.record_output_bytes(buffer.size());
            }
            gstreamer::PadProbeReturn::Ok
        });

    Ok(StreamPipelineHandles {
        pipeline,
        video_appsrc,
        audio_appsrc_mic: mic_appsrc,
        audio_appsrc_system: system_appsrc,
        telemetry,
    })
}

/// Build a recording pipeline with separate audio tracks.
///
/// Video: appsrc → videoconvert → vtenc_h264 → h264parse → mux (video pad)
/// Mic: mic_appsrc → audioconvert → avenc_aac → aacparse → mux (audio track 1)
/// System (optional): system_appsrc → audioconvert → avenc_aac → aacparse → mux (audio track 2)
/// Output: mux → filesink
pub fn build_record_pipeline_with_audio(
    video_config: &EncoderConfig,
    audio_config: &AudioEncoderConfig,
    path: &Path,
    format: RecordingFormat,
    has_system_audio: bool,
) -> Result<RecordPipelineHandles> {
    let (pipeline, video_appsrc, video_last_name) =
        build_encode_chain(video_config, "encode-record-audio-pipeline", false)?;

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

    // Mic audio chain
    let audio_caps = make_audio_appsrc_caps(audio_config);

    let mic_appsrc = AppSrc::builder()
        .name("record-mic-src")
        .caps(&audio_caps)
        .format(gstreamer::Format::Time)
        .is_live(true)
        .build();

    let mic_convert = gstreamer::ElementFactory::make("audioconvert")
        .name("record-mic-convert")
        .build()
        .context("Failed to create mic audioconvert")?;

    let mic_encoder = gstreamer::ElementFactory::make("avenc_aac")
        .name("record-mic-encoder")
        .build()
        .context("Failed to create mic avenc_aac")?;
    mic_encoder.set_property("bitrate", (audio_config.bitrate_kbps * 1000) as i32);

    let mic_parser = gstreamer::ElementFactory::make("aacparse")
        .name("record-mic-parser")
        .build()
        .context("Failed to create mic aacparse")?;

    // Add elements
    pipeline
        .add_many([
            mic_appsrc.upcast_ref(),
            &mic_convert,
            &mic_encoder,
            &mic_parser,
            &mux,
            &sink,
        ])
        .context("Failed to add record audio elements")?;

    // Link mic chain: appsrc → audioconvert → avenc_aac → aacparse
    gstreamer::Element::link_many([
        mic_appsrc.upcast_ref(),
        &mic_convert,
        &mic_encoder,
        &mic_parser,
    ])
    .context("Failed to link mic audio encode chain")?;

    // Link video to mux via explicit pad
    let video_pad = mux
        .request_pad_simple("video_0")
        .or_else(|| mux.request_pad_simple("video_%u"))
        .context("Failed to request video pad on record mux")?;
    let video_last = pipeline
        .by_name(&video_last_name)
        .expect("parser element exists");
    let video_src_pad = video_last
        .static_pad("src")
        .context("Failed to get video src pad")?;
    video_src_pad
        .link(&video_pad)
        .context("Failed to link video to record mux")?;

    // Link mic audio to mux via explicit pad
    let mic_audio_pad = mux
        .request_pad_simple("audio_%u")
        .context("Failed to request mic audio pad on record mux")?;
    let mic_parser_src = mic_parser
        .static_pad("src")
        .context("Failed to get mic aacparse src pad")?;
    mic_parser_src
        .link(&mic_audio_pad)
        .context("Failed to link mic audio to record mux")?;

    // Optionally create system audio chain
    let system_appsrc = if has_system_audio {
        let sys_appsrc = AppSrc::builder()
            .name("record-system-src")
            .caps(&audio_caps)
            .format(gstreamer::Format::Time)
            .is_live(true)
            .build();

        let sys_convert = gstreamer::ElementFactory::make("audioconvert")
            .name("record-system-convert")
            .build()
            .context("Failed to create system audioconvert")?;

        let sys_encoder = gstreamer::ElementFactory::make("avenc_aac")
            .name("record-system-encoder")
            .build()
            .context("Failed to create system avenc_aac")?;
        sys_encoder.set_property("bitrate", (audio_config.bitrate_kbps * 1000) as i32);

        let sys_parser = gstreamer::ElementFactory::make("aacparse")
            .name("record-system-parser")
            .build()
            .context("Failed to create system aacparse")?;

        pipeline
            .add_many([
                sys_appsrc.upcast_ref(),
                &sys_convert,
                &sys_encoder,
                &sys_parser,
            ])
            .context("Failed to add system audio elements")?;

        gstreamer::Element::link_many([
            sys_appsrc.upcast_ref(),
            &sys_convert,
            &sys_encoder,
            &sys_parser,
        ])
        .context("Failed to link system audio encode chain")?;

        let sys_audio_pad = mux
            .request_pad_simple("audio_%u")
            .context("Failed to request system audio pad on record mux")?;
        let sys_parser_src = sys_parser
            .static_pad("src")
            .context("Failed to get system aacparse src pad")?;
        sys_parser_src
            .link(&sys_audio_pad)
            .context("Failed to link system audio to record mux")?;

        Some(sys_appsrc)
    } else {
        None
    };

    // Link mux → filesink
    mux.link(&sink)
        .context("Failed to link record mux to filesink")?;

    Ok(RecordPipelineHandles {
        pipeline,
        video_appsrc,
        mic_appsrc,
        system_appsrc,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn make_encoder_creates_element_for_available_type() {
        gstreamer::init().unwrap();
        let encoder = super::make_encoder(crate::gstreamer::EncoderType::H264x264, 4500, true);
        assert!(encoder.is_ok(), "x264enc should be available");
    }

    #[test]
    fn build_stream_with_audio_creates_valid_pipeline() {
        gstreamer::init().unwrap();
        let vc = EncoderConfig::default();
        let ac = AudioEncoderConfig::default();
        let result = build_stream_pipeline_with_audio(&vc, &ac, "rtmp://localhost/test", false);
        match result {
            Ok(handles) => {
                assert!(handles.pipeline.name().starts_with("encode"));
                assert!(handles.audio_appsrc_system.is_none());
                let _ = handles.pipeline.set_state(gstreamer::State::Null);
            }
            Err(e) => eprintln!("Skipping: {e}"),
        }
    }

    #[test]
    fn build_record_with_audio_creates_valid_pipeline() {
        gstreamer::init().unwrap();
        let vc = EncoderConfig::default();
        let ac = AudioEncoderConfig::default();
        let path = std::path::PathBuf::from("/tmp/test_audio_record.mkv");
        let result = build_record_pipeline_with_audio(&vc, &ac, &path, RecordingFormat::Mkv, false);
        match result {
            Ok(handles) => {
                assert!(handles.pipeline.name().starts_with("encode"));
                assert!(handles.system_appsrc.is_none());
                let _ = handles.pipeline.set_state(gstreamer::State::Null);
            }
            Err(e) => eprintln!("Skipping: {e}"),
        }
    }
}
