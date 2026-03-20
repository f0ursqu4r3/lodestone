// Glyphon text rendering — Task 8
//
// TODO: Implement real GPU text rendering with glyphon + cosmic-text.
// Currently a no-op stub because glyphon 0.10 requires wgpu 28, but
// egui-wgpu 0.33 re-exports wgpu 27. Once glyphon ships a version
// compatible with wgpu 27 (or we upgrade egui-wgpu), swap this stub
// for a real implementation.
//
// In the meantime, text can be rendered through egui's built-in text
// facilities, which are already integrated via the egui-wgpu pipeline.

use anyhow::Result;

/// A text section to be rendered on screen.
#[derive(Debug, Clone)]
pub struct TextSection {
    pub text: String,
    pub position: [f32; 2],
    pub size: f32,
    pub color: [u8; 4], // RGBA
}

/// GPU text renderer (stub).
///
/// This is a no-op placeholder. When glyphon becomes compatible with the
/// wgpu version used by egui-wgpu, it will be replaced with a real
/// implementation that:
/// 1. Initializes a `cosmic_text::FontSystem` (system fonts)
/// 2. Creates a `glyphon::SwashCache` for glyph rasterization
/// 3. Creates a `glyphon::TextAtlas` backed by the wgpu device
/// 4. Creates a `glyphon::TextRenderer`
/// 5. Prepares and renders text sections into a wgpu render pass
pub struct GlyphonRenderer {
    sections: Vec<TextSection>,
}

impl GlyphonRenderer {
    /// Create a new stub text renderer.
    ///
    /// In the real implementation this would take `&wgpu::Device`, `&wgpu::Queue`,
    /// and `wgpu::TextureFormat` to initialise the font system, swash cache,
    /// text atlas, and text renderer.
    pub fn new() -> Self {
        log::info!("GlyphonRenderer: using stub (glyphon/wgpu version mismatch)");
        Self {
            sections: Vec::new(),
        }
    }

    /// Buffer text sections for rendering.
    ///
    /// In the real implementation this would layout the text with cosmic-text,
    /// rasterize glyphs via the swash cache, and upload them to the text atlas.
    pub fn prepare(&mut self, sections: &[TextSection]) -> Result<()> {
        self.sections = sections.to_vec();
        // No-op: real implementation would prepare glyphs here.
        Ok(())
    }

    /// Render prepared text into a render pass.
    ///
    /// In the real implementation this would call `glyphon::TextRenderer::render()`
    /// to draw the prepared glyphs into the active render pass.
    pub fn render(&self) -> Result<()> {
        // No-op: real implementation would render glyphs here.
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_section_creation() {
        let section = TextSection {
            text: "Lodestone".to_string(),
            position: [20.0, 20.0],
            size: 24.0,
            color: [255, 255, 255, 255],
        };
        assert_eq!(section.text, "Lodestone");
        assert_eq!(section.position, [20.0, 20.0]);
        assert_eq!(section.size, 24.0);
        assert_eq!(section.color, [255, 255, 255, 255]);
    }

    #[test]
    fn glyphon_renderer_stub_lifecycle() {
        let mut renderer = GlyphonRenderer::new();

        // Prepare with test label
        let sections = vec![TextSection {
            text: "Lodestone".to_string(),
            position: [20.0, 20.0],
            size: 24.0,
            color: [255, 255, 255, 255],
        }];
        renderer.prepare(&sections).unwrap();

        // Render (no-op)
        renderer.render().unwrap();
    }

    #[test]
    fn glyphon_renderer_prepare_multiple_sections() {
        let mut renderer = GlyphonRenderer::new();
        let sections = vec![
            TextSection {
                text: "Hello".to_string(),
                position: [10.0, 10.0],
                size: 16.0,
                color: [255, 0, 0, 255],
            },
            TextSection {
                text: "World".to_string(),
                position: [10.0, 30.0],
                size: 16.0,
                color: [0, 255, 0, 255],
            },
        ];
        renderer.prepare(&sections).unwrap();
        assert_eq!(renderer.sections.len(), 2);
    }
}
