//! Image source loading — decodes static images and animated GIFs to frames.

use std::time::Duration;

use image::AnimationDecoder;

use crate::gstreamer::RgbaFrame;
use crate::scene::LoopMode;
use anyhow::Context;

/// Decoded image data, either a static frame or a multi-frame animation.
pub enum ImageData {
    /// A single static frame (PNG, JPEG, non-animated GIF, etc.).
    Static(RgbaFrame),
    /// A multi-frame animated GIF.
    Animated(GifAnimation),
}

/// Decoded animated GIF data.
#[derive(Debug, Clone)]
pub struct GifAnimation {
    /// Decoded RGBA frames in display order.
    pub frames: Vec<RgbaFrame>,
    /// Per-frame display duration. Parallel to `frames`.
    pub delays: Vec<Duration>,
    /// Loop count embedded in the GIF file.
    pub embedded_loop_count: LoopMode,
}

/// Load an image file and return `ImageData`.
///
/// For GIF files with multiple frames, returns `ImageData::Animated`.
/// For all other images (including single-frame GIFs), returns `ImageData::Static`.
///
/// GIF frame delays below 20 ms are promoted to 100 ms per the GIF spec convention
/// (many GIFs encode 0 cs to mean "as fast as possible", which browsers treat as 100 ms).
pub fn load_image_source(path: &str) -> anyhow::Result<ImageData> {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".gif") {
        load_gif(path)
    } else {
        let img = image::open(path)
            .with_context(|| format!("Failed to open image: {path}"))?
            .into_rgba8();
        let width = img.width();
        let height = img.height();
        let data = img.into_raw();
        Ok(ImageData::Static(RgbaFrame { data, width, height }))
    }
}

fn load_gif(path: &str) -> anyhow::Result<ImageData> {
    use image::codecs::gif::GifDecoder;
    use std::fs::File;
    use std::io::BufReader;

    let file =
        File::open(path).with_context(|| format!("Failed to open GIF file: {path}"))?;
    let reader = BufReader::new(file);
    let decoder =
        GifDecoder::new(reader).with_context(|| format!("Failed to decode GIF: {path}"))?;

    let gif_frames: Vec<image::Frame> = decoder
        .into_frames()
        .collect_frames()
        .with_context(|| format!("Failed to collect GIF frames: {path}"))?;

    if gif_frames.len() <= 1 {
        // Single-frame GIF — treat as static.
        let frame = gif_frames
            .into_iter()
            .next()
            .map(|f| {
                let buf = f.into_buffer();
                let width = buf.width();
                let height = buf.height();
                RgbaFrame { data: buf.into_raw(), width, height }
            })
            .unwrap_or_else(|| RgbaFrame { data: vec![], width: 0, height: 0 });
        return Ok(ImageData::Static(frame));
    }

    let mut frames = Vec::with_capacity(gif_frames.len());
    let mut delays = Vec::with_capacity(gif_frames.len());

    for gif_frame in gif_frames {
        let (numer, denom) = gif_frame.delay().numer_denom_ms();
        let raw_ms = if denom == 0 { 0 } else { numer / denom };
        let ms = if raw_ms < 20 { 100 } else { raw_ms };
        delays.push(Duration::from_millis(ms as u64));

        let buf = gif_frame.into_buffer();
        let width = buf.width();
        let height = buf.height();
        frames.push(RgbaFrame { data: buf.into_raw(), width, height });
    }

    Ok(ImageData::Animated(GifAnimation {
        frames,
        delays,
        // The image crate does not expose the Netscape loop-count extension,
        // so we default to infinite looping (browsers also default to infinite).
        embedded_loop_count: LoopMode::Infinite,
    }))
}

/// Backwards-compatible wrapper — loads static images only.
/// Used by callers that haven't been updated for animated GIF support yet.
pub fn load_static_image(path: &str) -> anyhow::Result<RgbaFrame> {
    match load_image_source(path)? {
        ImageData::Static(frame) => Ok(frame),
        ImageData::Animated(anim) => anim
            .frames
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("GIF has no frames")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_nonexistent_file_returns_error() {
        let result = load_image_source("/nonexistent/path.png");
        assert!(result.is_err());
    }

    #[test]
    fn load_nonexistent_gif_returns_error() {
        let result = load_image_source("/nonexistent/animation.gif");
        assert!(result.is_err());
    }

    #[test]
    fn loop_mode_default_is_infinite() {
        assert_eq!(LoopMode::default(), LoopMode::Infinite);
    }

    #[test]
    fn image_data_static_variant() {
        let frame = RgbaFrame { data: vec![255, 0, 0, 255], width: 1, height: 1 };
        let data = ImageData::Static(frame);
        assert!(matches!(data, ImageData::Static(_)));
    }

    #[test]
    fn image_data_animated_variant() {
        let frame = RgbaFrame { data: vec![255, 0, 0, 255], width: 1, height: 1 };
        let anim = GifAnimation {
            frames: vec![frame],
            delays: vec![Duration::from_millis(100)],
            embedded_loop_count: LoopMode::Infinite,
        };
        let data = ImageData::Animated(anim);
        assert!(matches!(data, ImageData::Animated(_)));
    }

    #[test]
    fn load_static_image_nonexistent_returns_error() {
        let result = load_static_image("/nonexistent/path.png");
        assert!(result.is_err());
    }
}
