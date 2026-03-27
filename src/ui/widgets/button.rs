//! Themed button widget with multiple visual variants.
#![allow(dead_code)]

use egui::{Color32, CornerRadius, Response, Sense, Stroke, StrokeKind, Ui, Vec2, Widget};

use crate::ui::theme::active_theme;

/// Visual variant for [`StyledButton`].
#[derive(Debug, Clone, Copy)]
pub enum ButtonVariant {
    /// Accent-colored fill with white text.
    Primary,
    /// Danger (red) fill with white text.
    Danger,
    /// Success (green) fill with white text.
    Success,
    /// Transparent fill with `text_secondary` color; border on hover.
    Ghost,
    /// Compact toolbar button: transparent fill, `text_muted`; `bg_elevated` on hover.
    Toolbar,
}

/// A themed button that reads colors from the active theme.
pub struct StyledButton<'a> {
    text: &'a str,
    variant: ButtonVariant,
    min_size: Option<Vec2>,
}

impl<'a> StyledButton<'a> {
    /// Create a new styled button with the given text and variant.
    pub fn new(text: &'a str, variant: ButtonVariant) -> Self {
        Self {
            text,
            variant,
            min_size: None,
        }
    }

    /// Set the minimum size of the button.
    pub fn min_size(mut self, size: Vec2) -> Self {
        self.min_size = Some(size);
        self
    }
}

impl Widget for StyledButton<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let theme = active_theme(ui.ctx());

        let padding = egui::vec2(10.0, 4.0);
        let galley = ui.painter().layout_no_wrap(
            self.text.to_owned(),
            egui::FontId::proportional(13.0),
            Color32::WHITE, // placeholder; actual color applied below
        );
        let text_size = galley.size();
        let desired = text_size + padding * 2.0;
        let desired = if let Some(min) = self.min_size {
            desired.max(min)
        } else {
            desired
        };

        let (rect, response) = ui.allocate_exact_size(desired, Sense::click());

        if ui.is_rect_visible(rect) {
            let is_hovered = response.hovered();
            let radius = CornerRadius::same(theme.radius_sm as u8);

            let (fill, text_color, stroke) = match self.variant {
                ButtonVariant::Primary => {
                    let fill = if is_hovered {
                        theme.accent_hover
                    } else {
                        theme.accent
                    };
                    (fill, Color32::WHITE, Stroke::NONE)
                }
                ButtonVariant::Danger => {
                    let fill = if is_hovered {
                        brighten(theme.danger)
                    } else {
                        theme.danger
                    };
                    (fill, Color32::WHITE, Stroke::NONE)
                }
                ButtonVariant::Success => {
                    let fill = if is_hovered {
                        brighten(theme.success)
                    } else {
                        theme.success
                    };
                    (fill, Color32::WHITE, Stroke::NONE)
                }
                ButtonVariant::Ghost => {
                    let fill = Color32::TRANSPARENT;
                    let stroke = if is_hovered {
                        Stroke::new(1.0, theme.border)
                    } else {
                        Stroke::NONE
                    };
                    (fill, theme.text_secondary, stroke)
                }
                ButtonVariant::Toolbar => {
                    let fill = if is_hovered {
                        theme.bg_elevated
                    } else {
                        Color32::TRANSPARENT
                    };
                    (fill, theme.text_muted, Stroke::NONE)
                }
            };

            // Background
            ui.painter().rect_filled(rect, radius, fill);

            // Border (Ghost hover)
            if stroke != Stroke::NONE {
                ui.painter()
                    .rect_stroke(rect, radius, stroke, StrokeKind::Outside);
            }

            // Text
            let text_pos = rect.center() - text_size / 2.0;
            ui.painter().galley(
                text_pos,
                ui.painter().layout_no_wrap(
                    self.text.to_owned(),
                    egui::FontId::proportional(13.0),
                    text_color,
                ),
                text_color,
            );
        }

        response
    }
}

/// Slightly brighten a color for hover effects.
fn brighten(c: Color32) -> Color32 {
    Color32::from_rgb(
        (c.r() as u16 + 20).min(255) as u8,
        (c.g() as u16 + 20).min(255) as u8,
        (c.b() as u16 + 20).min(255) as u8,
    )
}
