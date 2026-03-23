//! Image source loading — decodes image files to RgbaFrame.

use anyhow::Context;
use crate::gstreamer::RgbaFrame;

/// Load an image file and convert to an RgbaFrame.
pub fn load_image_source(path: &str) -> anyhow::Result<RgbaFrame> {
    let img = image::open(path)
        .with_context(|| format!("Failed to open image: {path}"))?
        .into_rgba8();
    let width = img.width();
    let height = img.height();
    let data = img.into_raw();
    Ok(RgbaFrame { data, width, height })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_nonexistent_file_returns_error() {
        let result = load_image_source("/nonexistent/path.png");
        assert!(result.is_err());
    }
}
