mod advanced;
mod appearance;
mod audio;
mod general;
mod hotkeys;
mod stream;
mod video;

use std::time::Instant;

use egui::{
    Align, Color32, CornerRadius, CursorIcon, Id, Layout, Rect, Response, Sense, Stroke,
    StrokeKind, Ui, Vec2, Widget,
};

use crate::state::AppState;
use crate::ui::theme::{
    BG_BASE, BG_ELEVATED, BG_SURFACE, TEXT_MUTED, TEXT_PRIMARY, TEXT_SECONDARY, accent_color_ui,
    parse_hex_color,
};

// ── Category enum ─────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum SettingsCategory {
    General,
    Appearance,
    Hotkeys,
    StreamOutput,
    Audio,
    Video,
    Advanced,
}

impl SettingsCategory {
    fn label(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Appearance => "Appearance",
            Self::Hotkeys => "Hotkeys",
            Self::StreamOutput => "Stream / Output",
            Self::Audio => "Audio",
            Self::Video => "Video",
            Self::Advanced => "Advanced",
        }
    }
}

// ── Sidebar grouping ─────────────────────────────────────────────────────────

struct SidebarGroup {
    title: &'static str,
    items: &'static [SettingsCategory],
}

const SIDEBAR_GROUPS: &[SidebarGroup] = &[
    SidebarGroup {
        title: "APPLICATION",
        items: &[
            SettingsCategory::General,
            SettingsCategory::Appearance,
            SettingsCategory::Hotkeys,
        ],
    },
    SidebarGroup {
        title: "OUTPUT",
        items: &[
            SettingsCategory::StreamOutput,
            SettingsCategory::Audio,
            SettingsCategory::Video,
        ],
    },
    SidebarGroup {
        title: "SYSTEM",
        items: &[SettingsCategory::Advanced],
    },
];

// ── Public entry point (native window) ────────────────────────────────────────

/// Render settings UI directly into a native window's egui context.
/// Called from `WindowState::render_settings()`.
pub fn render_native(ctx: &egui::Context, state: &mut AppState) {
    let settings_id = Id::new("settings_active_category");
    let mut active = ctx
        .data_mut(|d| d.get_temp::<SettingsCategory>(settings_id))
        .unwrap_or(SettingsCategory::General);

    let accent = parse_hex_color(&state.settings.appearance.accent_color);

    // Sidebar panel
    egui::SidePanel::left("settings_sidebar")
        .exact_width(190.0)
        .resizable(false)
        .frame(egui::Frame::NONE.fill(BG_BASE))
        .show(ctx, |ui| {
            ui.add_space(12.0);
            render_sidebar(ui, &mut active, accent);
        });

    // Content panel
    egui::CentralPanel::default()
        .frame(
            egui::Frame::NONE
                .fill(BG_SURFACE)
                .inner_margin(egui::Margin::same(24)),
        )
        .show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.style_mut().spacing.item_spacing.y = 8.0;

                    let changed = render_content_direct(ui, active, state);

                    if changed {
                        state.settings_dirty = true;
                        state.settings_last_changed = Instant::now();
                    }

                    ui.add_space(24.0);
                });
        });

    ctx.data_mut(|d| d.insert_temp(settings_id, active));
}

// ── Sidebar ───────────────────────────────────────────────────────────────────

fn render_sidebar(ui: &mut Ui, active: &mut SettingsCategory, accent: Color32) {
    ui.add_space(16.0);

    for group in SIDEBAR_GROUPS {
        // Group header
        ui.horizontal(|ui| {
            ui.add_space(16.0);
            ui.label(
                egui::RichText::new(group.title)
                    .size(10.0)
                    .color(TEXT_MUTED)
                    .strong(),
            );
        });
        ui.add_space(4.0);

        for &cat in group.items {
            let is_active = *active == cat;
            let (rect, response) =
                ui.allocate_exact_size(Vec2::new(ui.available_width(), 28.0), Sense::click());

            if response.clicked() {
                *active = cat;
            }

            let hovered = response.hovered();

            // Background on hover or active
            if is_active {
                ui.painter()
                    .rect_filled(rect, CornerRadius::same(4), BG_ELEVATED);
                // Accent bar on the left edge
                ui.painter().rect_filled(
                    Rect::from_min_size(rect.min, Vec2::new(3.0, rect.height())),
                    CornerRadius::same(2),
                    accent,
                );
            } else if hovered {
                ui.painter().rect_filled(
                    rect,
                    CornerRadius::same(4),
                    Color32::from_rgba_premultiplied(0x22, 0x22, 0x2c, 0x80),
                );
            }

            if hovered {
                ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
            }

            // Label
            let text_color = if is_active {
                TEXT_PRIMARY
            } else {
                TEXT_SECONDARY
            };
            let galley = ui.painter().layout_no_wrap(
                cat.label().to_string(),
                egui::FontId::proportional(13.0),
                text_color,
            );
            let text_pos = egui::pos2(rect.min.x + 20.0, rect.center().y - galley.size().y / 2.0);
            ui.painter().galley(text_pos, galley, text_color);
        }

        ui.add_space(12.0);
    }
}

// ── Content dispatch ──────────────────────────────────────────────────────────

/// Render the content area for the active category, taking `&mut AppState` directly.
fn render_content_direct(ui: &mut Ui, category: SettingsCategory, state: &mut AppState) -> bool {
    // Section title
    ui.horizontal(|ui| {
        ui.add_space(24.0);
        ui.label(
            egui::RichText::new(category.label())
                .size(20.0)
                .color(TEXT_PRIMARY)
                .strong(),
        );
    });
    ui.add_space(8.0);

    ui.horizontal(|ui| {
        ui.add_space(24.0);
        ui.with_layout(Layout::top_down(Align::Min), |ui| {
            ui.set_width(ui.available_width() - 24.0);
            match category {
                SettingsCategory::General => general::draw(ui, state),
                SettingsCategory::StreamOutput => stream::draw(ui, &mut state.settings.stream),
                SettingsCategory::Audio => audio::draw(ui, state),
                SettingsCategory::Video => video::draw(ui, &mut state.settings.video, state.detected_resolution),
                SettingsCategory::Hotkeys => hotkeys::draw(ui, &mut state.settings.hotkeys),
                SettingsCategory::Appearance => appearance::draw(ui, state),
                SettingsCategory::Advanced => advanced::draw(ui, &mut state.settings.advanced),
            }
        })
        .inner
    })
    .inner
}

// ── Section helpers ──────────────────────────────────────────────────────────

pub(super) fn section_header(ui: &mut Ui, label: &str) {
    ui.add_space(12.0);
    ui.label(
        egui::RichText::new(label)
            .size(11.0)
            .color(TEXT_MUTED)
            .strong(),
    );
    ui.add_space(4.0);
}

pub(super) fn labeled_row(ui: &mut Ui, label: &str) {
    ui.label(egui::RichText::new(label).size(13.0).color(TEXT_PRIMARY));
}

/// Label a row as not yet implemented -- gray text and disabled controls.
pub(super) fn labeled_row_unimplemented(ui: &mut Ui, label: &str) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(label).size(13.0).color(TEXT_MUTED));
        ui.label(
            egui::RichText::new("(not yet implemented)")
                .size(10.0)
                .color(TEXT_MUTED)
                .italics(),
        );
    });
}

/// Draw a toggle that's grayed out / not implemented.
pub(super) fn draw_toggle_unimplemented(ui: &mut Ui, label: &str, _value: &mut bool) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(label).size(13.0).color(TEXT_MUTED));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(
                egui::RichText::new("not implemented")
                    .size(10.0)
                    .color(TEXT_MUTED)
                    .italics(),
            );
        });
    });
}

// ── Toggle helper ─────────────────────────────────────────────────────────────

pub(super) fn draw_toggle(ui: &mut Ui, label: &str, value: &mut bool) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        labeled_row(ui, label);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui.add(toggle_switch(value)).changed() {
                changed = true;
            }
        });
    });
    changed
}

/// A custom iOS-style toggle switch widget with smooth animation.
pub(super) fn toggle_switch(on: &mut bool) -> impl Widget + '_ {
    move |ui: &mut Ui| -> Response {
        let desired_size = Vec2::new(36.0, 20.0);
        let (rect, mut response) = ui.allocate_exact_size(desired_size, Sense::click());

        if response.clicked() {
            *on = !*on;
            response.mark_changed();
        }

        if ui.is_rect_visible(rect) {
            // Animate the toggle position
            let anim_id = response.id.with("toggle_anim");
            let t = ui.ctx().animate_bool_with_time(anim_id, *on, 0.15);

            let bg_color = if *on {
                accent_color_ui(ui)
            } else {
                BG_ELEVATED
            };

            let knob_radius = 7.0;
            let knob_x = egui::lerp(
                rect.left() + knob_radius + 3.0..=rect.right() - knob_radius - 3.0,
                t,
            );
            let knob_center = egui::pos2(knob_x, rect.center().y);

            // Track background
            ui.painter()
                .rect_filled(rect, CornerRadius::same(10), bg_color);

            // Track border
            if !*on {
                ui.painter().rect_stroke(
                    rect,
                    CornerRadius::same(10),
                    Stroke::new(1.0, TEXT_MUTED),
                    StrokeKind::Outside,
                );
            }

            // Knob
            ui.painter()
                .circle_filled(knob_center, knob_radius, Color32::WHITE);
        }

        response
    }
}
