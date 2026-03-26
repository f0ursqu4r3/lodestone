//! Color source rendering — generates solid color and gradient RgbaFrames.

use crate::gstreamer::types::RgbaFrame;
use crate::scene::{ColorFill, GradientStop};

/// Render a color source to an RgbaFrame.
///
/// For solid fills, renders at the given dimensions.
/// For gradients, renders at the given dimensions (capped at 1920x1080).
pub fn render_color_source(fill: &ColorFill, width: u32, height: u32) -> RgbaFrame {
    let w = width.clamp(1, 1920) as usize;
    let h = height.clamp(1, 1080) as usize;

    let mut data = vec![0u8; w * h * 4];

    match fill {
        ColorFill::Solid { color } => {
            let pixel = color_to_bytes(color);
            for chunk in data.chunks_exact_mut(4) {
                chunk.copy_from_slice(&pixel);
            }
        }
        ColorFill::LinearGradient { angle, stops } => {
            let angle_rad = angle.to_radians();
            let dx = angle_rad.cos();
            let dy = angle_rad.sin();

            for y in 0..h {
                for x in 0..w {
                    let nx = x as f32 / (w - 1).max(1) as f32;
                    let ny = y as f32 / (h - 1).max(1) as f32;
                    // Project onto gradient axis, centered at (0.5, 0.5)
                    let t = ((nx - 0.5) * dx + (ny - 0.5) * dy + 0.5).clamp(0.0, 1.0);
                    let color = interpolate_stops(stops, t);
                    let offset = (y * w + x) * 4;
                    data[offset..offset + 4].copy_from_slice(&color_to_bytes(&color));
                }
            }
        }
        ColorFill::RadialGradient {
            center,
            radius,
            stops,
        } => {
            let r = radius.max(0.001);
            for y in 0..h {
                for x in 0..w {
                    let nx = x as f32 / (w - 1).max(1) as f32;
                    let ny = y as f32 / (h - 1).max(1) as f32;
                    let dist = ((nx - center.0).powi(2) + (ny - center.1).powi(2)).sqrt() / r;
                    let t = dist.clamp(0.0, 1.0);
                    let color = interpolate_stops(stops, t);
                    let offset = (y * w + x) * 4;
                    data[offset..offset + 4].copy_from_slice(&color_to_bytes(&color));
                }
            }
        }
    }

    RgbaFrame {
        data,
        width: w as u32,
        height: h as u32,
    }
}

/// Interpolate between gradient stops at position t (0.0..1.0).
fn interpolate_stops(stops: &[GradientStop], t: f32) -> [f32; 4] {
    if stops.is_empty() {
        return [0.0, 0.0, 0.0, 1.0];
    }
    if stops.len() == 1 {
        return stops[0].color;
    }

    if t <= stops[0].position {
        return stops[0].color;
    }
    if t >= stops[stops.len() - 1].position {
        return stops[stops.len() - 1].color;
    }

    for i in 0..stops.len() - 1 {
        let s0 = &stops[i];
        let s1 = &stops[i + 1];
        if t >= s0.position && t <= s1.position {
            let range = s1.position - s0.position;
            if range < f32::EPSILON {
                return s0.color;
            }
            let local_t = (t - s0.position) / range;
            return [
                s0.color[0] + (s1.color[0] - s0.color[0]) * local_t,
                s0.color[1] + (s1.color[1] - s0.color[1]) * local_t,
                s0.color[2] + (s1.color[2] - s0.color[2]) * local_t,
                s0.color[3] + (s1.color[3] - s0.color[3]) * local_t,
            ];
        }
    }

    stops[stops.len() - 1].color
}

/// Convert a float RGBA color to byte RGBA.
fn color_to_bytes(color: &[f32; 4]) -> [u8; 4] {
    [
        (color[0].clamp(0.0, 1.0) * 255.0) as u8,
        (color[1].clamp(0.0, 1.0) * 255.0) as u8,
        (color[2].clamp(0.0, 1.0) * 255.0) as u8,
        (color[3].clamp(0.0, 1.0) * 255.0) as u8,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solid_color_fills_all_pixels() {
        let fill = ColorFill::Solid {
            color: [1.0, 0.0, 0.0, 1.0],
        };
        let frame = render_color_source(&fill, 4, 4);
        assert_eq!(frame.width, 4);
        assert_eq!(frame.height, 4);
        assert_eq!(frame.data.len(), 64);
        for chunk in frame.data.chunks_exact(4) {
            assert_eq!(chunk, &[255, 0, 0, 255]);
        }
    }

    #[test]
    fn linear_gradient_endpoints() {
        let stops = vec![
            GradientStop {
                position: 0.0,
                color: [0.0, 0.0, 0.0, 1.0],
            },
            GradientStop {
                position: 1.0,
                color: [1.0, 1.0, 1.0, 1.0],
            },
        ];
        let fill = ColorFill::LinearGradient { angle: 0.0, stops };
        let frame = render_color_source(&fill, 10, 1);
        assert!(frame.data[0] < 50);
        assert!(frame.data[(9 * 4)] > 200);
    }

    #[test]
    fn radial_gradient_center_is_first_stop() {
        let stops = vec![
            GradientStop {
                position: 0.0,
                color: [1.0, 0.0, 0.0, 1.0],
            },
            GradientStop {
                position: 1.0,
                color: [0.0, 0.0, 1.0, 1.0],
            },
        ];
        let fill = ColorFill::RadialGradient {
            center: (0.5, 0.5),
            radius: 0.5,
            stops,
        };
        let frame = render_color_source(&fill, 11, 11);
        let center_offset = (5 * 11 + 5) * 4;
        assert_eq!(frame.data[center_offset], 255);
        assert_eq!(frame.data[center_offset + 2], 0);
    }

    #[test]
    fn empty_stops_returns_black() {
        let fill = ColorFill::LinearGradient {
            angle: 0.0,
            stops: vec![],
        };
        let frame = render_color_source(&fill, 2, 2);
        for chunk in frame.data.chunks_exact(4) {
            assert_eq!(chunk, &[0, 0, 0, 255]);
        }
    }

    #[test]
    fn dimensions_capped_at_1920x1080() {
        let fill = ColorFill::Solid {
            color: [1.0, 1.0, 1.0, 1.0],
        };
        let frame = render_color_source(&fill, 4000, 3000);
        assert_eq!(frame.width, 1920);
        assert_eq!(frame.height, 1080);
    }

    #[test]
    fn color_to_bytes_clamps() {
        assert_eq!(color_to_bytes(&[1.5, -0.5, 0.5, 0.0]), [255, 0, 127, 0]);
    }

    #[test]
    fn interpolate_single_stop() {
        let stops = vec![GradientStop {
            position: 0.5,
            color: [1.0, 0.0, 0.0, 1.0],
        }];
        assert_eq!(interpolate_stops(&stops, 0.0), [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(interpolate_stops(&stops, 1.0), [1.0, 0.0, 0.0, 1.0]);
    }
}
