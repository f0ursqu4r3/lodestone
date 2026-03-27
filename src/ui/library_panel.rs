//! Library panel — global source CRUD and browsing.
//!
//! Shows all sources in the global library, organized either by type or by folder.
//! Supports creating new sources, selecting them, and deleting with cascade
//! (removing from all scenes).

use crate::gstreamer::GstCommand;
use crate::scene::{LibrarySource, SourceId, SourceProperties, SourceType, Transform};
use crate::state::AppState;
use crate::ui::draw_helpers::{draw_segmented_buttons, draw_selection_highlight, source_icon};
use crate::ui::layout::tree::PanelId;
use crate::ui::theme::active_theme;
use egui::{CornerRadius, Rect, Sense, Stroke, vec2};

/// Content grouping mode for the library panel.
#[derive(Clone, Copy, PartialEq, Eq)]
enum LibraryView {
    ByType,
    Folders,
}

/// Display mode: how individual sources are rendered.
#[derive(Clone, Copy, PartialEq, Eq)]
enum LibraryDisplayMode {
    List,
    Grid,
}

/// Start a rename: set state and bump the focus generation so the TextEdit gets focused.
fn start_rename_source(ui: &egui::Ui, state: &mut AppState, id: SourceId, name: &str) {
    state.renaming_source_id = Some(id);
    state.rename_buffer = name.to_string();
    let gen_id = egui::Id::new("rename_gen");
    ui.data_mut(|d| {
        let g: u64 = d.get_temp(gen_id).unwrap_or(0);
        d.insert_temp(gen_id, g + 1);
    });
}

/// Draw the library panel.
pub fn draw(ui: &mut egui::Ui, state: &mut AppState, _id: PanelId) {
    // Load view mode and display mode from persisted settings.
    let view = match state.settings.ui.library_view.as_str() {
        "folders" => LibraryView::Folders,
        _ => LibraryView::ByType,
    };
    let display_mode = match state.settings.ui.library_display_mode.as_str() {
        "grid" => LibraryDisplayMode::Grid,
        _ => LibraryDisplayMode::List,
    };

    // ── Header row ──
    let (view, display_mode) = draw_header(ui, state, view, display_mode);

    // Persist updated modes to settings.
    let new_view = match view {
        LibraryView::ByType => "type",
        LibraryView::Folders => "folders",
    };
    let new_display = match display_mode {
        LibraryDisplayMode::List => "list",
        LibraryDisplayMode::Grid => "grid",
    };
    if state.settings.ui.library_view != new_view
        || state.settings.ui.library_display_mode != new_display
    {
        state.settings.ui.library_view = new_view.to_string();
        state.settings.ui.library_display_mode = new_display.to_string();
        state.settings_dirty = true;
        state.settings_last_changed = std::time::Instant::now();
    }

    ui.add_space(4.0);

    let theme = active_theme(ui.ctx());

    if state.library.is_empty() {
        ui.add_space(16.0);
        ui.centered_and_justified(|ui| {
            ui.colored_label(theme.text_muted, "No sources. Click + to add one.");
        });
        return;
    }

    // Snapshot library data for rendering to avoid borrow conflicts.
    let rows: Vec<SourceRow> = state
        .library
        .iter()
        .map(|lib_src| SourceRow {
            id: lib_src.id,
            name: lib_src.name.clone(),
            source_type: lib_src.source_type.clone(),
            folder: lib_src.folder.clone(),
            usage_count: state.source_usage_count(lib_src.id),
        })
        .collect();

    // Track deferred deletion (collected after rendering).
    let mut delete_source: Option<SourceId> = None;

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            match view {
                LibraryView::ByType => {
                    draw_by_type_view(ui, state, &rows, display_mode, &mut delete_source);
                }
                LibraryView::Folders => {
                    draw_folders_view(ui, state, &rows, display_mode, &mut delete_source);
                }
            }
        });

    // Apply deferred deletion.
    if let Some(src_id) = delete_source {
        delete_source_cascade(state, src_id);
    }

    // ── Drag ghost: floating label following cursor ──
    if let Some(payload) = egui::DragAndDrop::payload::<SourceId>(ui.ctx())
        && let Some(pointer_pos) = ui.ctx().pointer_interact_pos()
    {
        // Look up the source name for the ghost label.
        let src_id = *payload;
        if let Some(row) = rows.iter().find(|r| r.id == src_id) {
            let ghost_layer =
                egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("library_drag_ghost"));
            let painter = ui.ctx().layer_painter(ghost_layer);
            let icon = source_icon(&row.source_type);
            let text = format!("{} {}", icon, row.name);
            let font = egui::FontId::proportional(11.0);
            let galley = painter.layout_no_wrap(text, font, theme.text_primary);
            let text_rect =
                egui::Rect::from_min_size(pointer_pos + vec2(12.0, -8.0), galley.size())
                    .expand(4.0);
            painter.rect_filled(text_rect, CornerRadius::same(theme.radius_sm as u8), theme.bg_elevated);
            painter.rect_stroke(
                text_rect,
                CornerRadius::same(theme.radius_sm as u8),
                egui::Stroke::new(1.0, theme.border),
                egui::StrokeKind::Outside,
            );
            painter.galley(text_rect.min + vec2(4.0, 4.0), galley, theme.text_primary);
        }
    }
}

/// Draw the library panel header: add button + view/display segmented toggles.
/// Returns updated (LibraryView, LibraryDisplayMode).
fn draw_header(
    ui: &mut egui::Ui,
    state: &mut AppState,
    view: LibraryView,
    display_mode: LibraryDisplayMode,
) -> (LibraryView, LibraryDisplayMode) {
    let mut view = view;
    let mut display_mode = display_mode;
    ui.horizontal(|ui| {
        draw_add_button(ui, state);

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // Group-by segment (rightmost)
            let group_btns: &[(&str, &str, bool)] = &[
                (
                    egui_phosphor::regular::SQUARES_FOUR,
                    "Group by type",
                    view == LibraryView::ByType,
                ),
                (
                    egui_phosphor::regular::FOLDER,
                    "Group by folder",
                    view == LibraryView::Folders,
                ),
            ];
            let group_clicked = draw_segmented_buttons(ui, "lib_group_seg", group_btns);
            if group_clicked == Some(0) {
                view = LibraryView::ByType;
            } else if group_clicked == Some(1) {
                view = LibraryView::Folders;
            }

            // Separator
            ui.add_space(2.0);
            let sep_rect = ui
                .allocate_exact_size(vec2(1.0, ui.available_height()), Sense::hover())
                .0;
            let theme_local = active_theme(ui.ctx());
            ui.painter().line_segment(
                [sep_rect.left_top(), sep_rect.left_bottom()],
                egui::Stroke::new(1.0, theme_local.border),
            );
            ui.add_space(2.0);

            // View-as segment
            let view_btns: &[(&str, &str, bool)] = &[
                (
                    egui_phosphor::regular::LIST,
                    "List view",
                    display_mode == LibraryDisplayMode::List,
                ),
                (
                    egui_phosphor::regular::GRID_FOUR,
                    "Icon view",
                    display_mode == LibraryDisplayMode::Grid,
                ),
            ];
            let view_clicked = draw_segmented_buttons(ui, "lib_view_seg", view_btns);
            if view_clicked == Some(0) {
                display_mode = LibraryDisplayMode::List;
            } else if view_clicked == Some(1) {
                display_mode = LibraryDisplayMode::Grid;
            }
        });
    });
    (view, display_mode)
}

/// Draw the "+" button with a popup to create new sources.
fn draw_add_button(ui: &mut egui::Ui, state: &mut AppState) {
    let add_response = ui
        .button(egui_phosphor::regular::PLUS)
        .on_hover_text("Create source");

    let popup_id = ui.make_persistent_id("library_add_menu");
    if add_response.clicked() {
        #[allow(deprecated)]
        ui.memory_mut(|m: &mut egui::Memory| m.toggle_popup(popup_id));
    }

    #[allow(deprecated)]
    egui::popup_below_widget(
        ui,
        popup_id,
        &add_response,
        egui::PopupCloseBehavior::CloseOnClickOutside,
        |ui: &mut egui::Ui| {
            use crate::ui::widgets::menu::{menu_item_icon, styled_menu};
            styled_menu(ui, |ui| {
                let capture_items: &[(&str, SourceType)] = &[
                    ("Display", SourceType::Display),
                    ("Window", SourceType::Window),
                    ("Camera", SourceType::Camera),
                    ("Image", SourceType::Image),
                ];
                let synthetic_items: &[(&str, SourceType)] = &[
                    ("Text", SourceType::Text),
                    ("Color", SourceType::Color),
                    ("Audio", SourceType::Audio),
                    ("Browser", SourceType::Browser),
                ];

                for (label, source_type) in capture_items {
                    if menu_item_icon(ui, source_icon(source_type), label) {
                        add_library_source(state, source_type.clone());
                        ui.memory_mut(|m| m.close_popup(popup_id));
                    }
                }
                ui.separator();
                for (label, source_type) in synthetic_items {
                    if menu_item_icon(ui, source_icon(source_type), label) {
                        add_library_source(state, source_type.clone());
                        ui.memory_mut(|m| m.close_popup(popup_id));
                    }
                }
            });
        },
    );
}

/// Create a new library source of the given type.
fn add_library_source(state: &mut AppState, source_type: SourceType) {
    let new_id = SourceId(state.next_source_id);
    state.next_source_id += 1;

    let (name, properties) = match source_type {
        SourceType::Display => {
            let count = state
                .library
                .iter()
                .filter(|s| matches!(s.source_type, SourceType::Display))
                .count();
            (
                format!("Display {}", count + 1),
                SourceProperties::Display { screen_index: 0 },
            )
        }
        SourceType::Window => {
            let count = state
                .library
                .iter()
                .filter(|s| matches!(s.source_type, SourceType::Window))
                .count();
            (
                format!("Window {}", count + 1),
                SourceProperties::Window {
                    mode: crate::scene::WindowCaptureMode::AnyFullscreen,
                    current_window_id: None,
                },
            )
        }
        SourceType::Camera => {
            let count = state
                .library
                .iter()
                .filter(|s| matches!(s.source_type, SourceType::Camera))
                .count();
            (
                format!("Camera {}", count + 1),
                SourceProperties::Camera {
                    device_index: 0,
                    device_name: String::new(),
                },
            )
        }
        SourceType::Image => {
            let count = state
                .library
                .iter()
                .filter(|s| matches!(s.source_type, SourceType::Image))
                .count();
            (
                format!("Image {}", count + 1),
                SourceProperties::Image {
                    path: String::new(),
                },
            )
        }
        SourceType::Text => {
            let count = state
                .library
                .iter()
                .filter(|s| matches!(s.source_type, SourceType::Text))
                .count();
            (
                format!("Text {}", count + 1),
                SourceProperties::Text {
                    content: crate::scene::default_text_content(),
                    font_family: crate::scene::default_font_family(),
                    font_size: crate::scene::default_font_size(),
                    font_color: crate::scene::default_font_color(),
                    background_color: crate::scene::default_transparent(),
                    bold: false,
                    italic: false,
                    alignment: crate::scene::TextAlignment::Left,
                    outline: None,
                    padding: crate::scene::default_padding(),
                    wrap_width: None,
                },
            )
        }
        SourceType::Color => {
            let count = state
                .library
                .iter()
                .filter(|s| matches!(s.source_type, SourceType::Color))
                .count();
            (
                format!("Color {}", count + 1),
                SourceProperties::Color {
                    fill: crate::scene::default_color_fill(),
                },
            )
        }
        SourceType::Audio => {
            let count = state
                .library
                .iter()
                .filter(|s| matches!(s.source_type, SourceType::Audio))
                .count();
            (
                format!("Audio {}", count + 1),
                SourceProperties::Audio {
                    input: crate::scene::default_audio_input(),
                },
            )
        }
        SourceType::Browser => {
            let count = state
                .library
                .iter()
                .filter(|s| matches!(s.source_type, SourceType::Browser))
                .count();
            (
                format!("Browser {}", count + 1),
                SourceProperties::Browser {
                    url: String::new(),
                    width: crate::scene::default_browser_width(),
                    height: crate::scene::default_browser_height(),
                },
            )
        }
    };

    // Determine native size from detected resolution for display/camera,
    // or use default 1920x1080 for other source types.
    let (native_w, native_h) = match &properties {
        SourceProperties::Display { screen_index } => {
            state
                .available_displays
                .iter()
                .find(|d| d.index == *screen_index as usize)
                .map(|d| (d.width as f32, d.height as f32))
                .unwrap_or((1920.0, 1080.0))
        }
        SourceProperties::Camera { device_index, .. } => {
            state
                .available_cameras
                .iter()
                .find(|c| c.device_index == *device_index)
                .map(|c| (c.resolution.0 as f32, c.resolution.1 as f32))
                .unwrap_or((1920.0, 1080.0))
        }
        _ => (1920.0, 1080.0),
    };

    let lib_source = LibrarySource {
        id: new_id,
        name,
        source_type,
        properties,
        folder: None,
        transform: Transform::new(0.0, 0.0, native_w, native_h),
        native_size: (native_w, native_h),
        aspect_ratio_locked: false,
        opacity: 1.0,
        visible: true,
        muted: false,
        volume: 1.0,
    };

    state.library.push(lib_source);
    state.selected_library_source_id = Some(new_id);
    state.mark_dirty();

    // Push initial frame for synthetic visual sources so they appear on canvas immediately.
    match &state.library.last().unwrap().properties {
        SourceProperties::Text { .. } => {
            let props = state.library.last().unwrap().properties.clone();
            if let Some(frame) = crate::text_source::render_text_source(&props) {
                let source = state.library.last_mut().unwrap();
                source.native_size = (frame.width as f32, frame.height as f32);
                source.transform.width = frame.width as f32;
                source.transform.height = frame.height as f32;
                if let Some(ref tx) = state.command_tx {
                    let _ = tx.try_send(GstCommand::LoadImageFrame {
                        source_id: new_id,
                        frame,
                    });
                }
            }
        }
        SourceProperties::Color { fill } => {
            let fill = fill.clone();
            let transform = &state.library.last().unwrap().transform;
            let frame = crate::color_source::render_color_source(
                &fill,
                transform.width as u32,
                transform.height as u32,
            );
            if let Some(ref tx) = state.command_tx {
                let _ = tx.try_send(GstCommand::LoadImageFrame {
                    source_id: new_id,
                    frame,
                });
            }
        }
        SourceProperties::Browser { width, height, .. } => {
            let width = *width;
            let height = *height;
            let frame = crate::ui::properties_panel::generate_browser_placeholder(width, height);
            let source = state.library.last_mut().unwrap();
            source.native_size = (width as f32, height as f32);
            if let Some(ref tx) = state.command_tx {
                let _ = tx.try_send(GstCommand::LoadImageFrame {
                    source_id: new_id,
                    frame,
                });
            }
        }
        _ => {}
    }
}

/// Delete a source from the library and cascade-remove it from all scenes.
pub(crate) fn delete_source_cascade(state: &mut AppState, src_id: SourceId) {
    // Remove from all scenes.
    for scene in &mut state.scenes {
        scene.sources.retain(|s| s.source_id != src_id);
    }

    // Remove from library.
    state.library.retain(|s| s.id != src_id);

    // Stop capture if running.
    if let Some(tx) = &state.command_tx {
        let _ = tx.try_send(GstCommand::RemoveCaptureSource { source_id: src_id });
    }

    // Clear selection if deleted.
    if state.selected_source_id() == Some(src_id) {
        state.deselect_all();
    }
    if state.selected_library_source_id == Some(src_id) {
        state.selected_library_source_id = None;
    }

    state.mark_dirty();
}

// ---------------------------------------------------------------------------
// By Type view
// ---------------------------------------------------------------------------

/// Section label for a SourceType.
fn type_section_label(source_type: &SourceType) -> &'static str {
    match source_type {
        SourceType::Display => "Displays",
        SourceType::Camera => "Cameras",
        SourceType::Window => "Windows",
        SourceType::Image => "Images",
        SourceType::Audio => "Audio",
        SourceType::Browser => "Browsers",
        SourceType::Text => "Text",
        SourceType::Color => "Colors",
    }
}

/// Snapshot of a source row for rendering.
struct SourceRow {
    id: SourceId,
    name: String,
    source_type: SourceType,
    folder: Option<String>,
    usage_count: usize,
}

/// Draw the "By Type" view with collapsible sections per source type.
fn draw_by_type_view(
    ui: &mut egui::Ui,
    state: &mut AppState,
    rows: &[SourceRow],
    display_mode: LibraryDisplayMode,
    delete_source: &mut Option<SourceId>,
) {
    // Sorted alphabetically by section label.
    let type_order: &[SourceType] = &[
        SourceType::Audio,
        SourceType::Browser,
        SourceType::Camera,
        SourceType::Color,
        SourceType::Display,
        SourceType::Image,
        SourceType::Text,
        SourceType::Window,
    ];

    for source_type in type_order {
        let mut section_rows: Vec<&SourceRow> = rows
            .iter()
            .filter(|r| {
                std::mem::discriminant(&r.source_type) == std::mem::discriminant(source_type)
            })
            .collect();
        section_rows.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

        if section_rows.is_empty() {
            continue;
        }

        let label = type_section_label(source_type);
        egui::CollapsingHeader::new(label)
            .default_open(true)
            .show(ui, |ui| {
                draw_section_items(ui, state, &section_rows, display_mode, delete_source);
            });
    }
}

// ---------------------------------------------------------------------------
// Folders view
// ---------------------------------------------------------------------------

/// Draw the "Folders" view with collapsible sections per folder.
fn draw_folders_view(
    ui: &mut egui::Ui,
    state: &mut AppState,
    rows: &[SourceRow],
    display_mode: LibraryDisplayMode,
    delete_source: &mut Option<SourceId>,
) {
    // Collect unique folder names (sorted).
    let mut folders: Vec<String> = rows.iter().filter_map(|r| r.folder.clone()).collect();
    folders.sort();
    folders.dedup();

    // Named folders first.
    for folder in &folders {
        let mut folder_rows: Vec<&SourceRow> = rows
            .iter()
            .filter(|r| r.folder.as_deref() == Some(folder.as_str()))
            .collect();
        folder_rows.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

        if folder_rows.is_empty() {
            continue;
        }

        egui::CollapsingHeader::new(folder.as_str())
            .default_open(true)
            .show(ui, |ui| {
                draw_section_items(ui, state, &folder_rows, display_mode, delete_source);
            });
    }

    // "Unfiled" section for sources without a folder.
    let mut unfiled_rows: Vec<&SourceRow> = rows.iter().filter(|r| r.folder.is_none()).collect();
    unfiled_rows.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    if !unfiled_rows.is_empty() {
        egui::CollapsingHeader::new("Unfiled")
            .default_open(true)
            .show(ui, |ui| {
                draw_section_items(ui, state, &unfiled_rows, display_mode, delete_source);
            });
    }
}

// ---------------------------------------------------------------------------
// Source row rendering
// ---------------------------------------------------------------------------

/// Draw section items in either list or grid mode.
fn draw_section_items(
    ui: &mut egui::Ui,
    state: &mut AppState,
    items: &[&SourceRow],
    display_mode: LibraryDisplayMode,
    delete_source: &mut Option<SourceId>,
) {
    match display_mode {
        LibraryDisplayMode::List => {
            for (idx, row) in items.iter().enumerate() {
                draw_source_row(ui, state, row, idx, items.len(), delete_source);
            }
        }
        LibraryDisplayMode::Grid => {
            draw_source_grid(ui, state, items, delete_source);
        }
    }
}

/// Draw sources as a grid of icon tiles.
fn draw_source_grid(
    ui: &mut egui::Ui,
    state: &mut AppState,
    items: &[&SourceRow],
    delete_source: &mut Option<SourceId>,
) {
    let theme = active_theme(ui.ctx());
    let spacing = 4.0;
    let tile_size = 56.0;
    let available_width = ui.available_width();
    let cols = ((available_width + spacing) / (tile_size + spacing))
        .floor()
        .max(1.0) as usize;
    let selected_bg = theme.accent_dim;

    let rows_count = items.len().div_ceil(cols);
    for row_idx in 0..rows_count {
        ui.horizontal(|ui| {
            ui.add_space(2.0);
            for col_idx in 0..cols {
                let item_idx = row_idx * cols + col_idx;
                let Some(row) = items.get(item_idx) else {
                    break;
                };

                let is_selected = state.selected_library_source_id == Some(row.id);

                ui.push_id(row.id.0, |ui| {
                    let (tile_rect, tile_response) =
                        ui.allocate_exact_size(vec2(tile_size, tile_size), Sense::click_and_drag());

                    let painter = ui.painter_at(tile_rect);

                    // Background.
                    let bg = if is_selected {
                        selected_bg
                    } else {
                        theme.bg_elevated
                    };
                    painter.rect_filled(tile_rect, CornerRadius::same(theme.radius_sm as u8), bg);

                    // Border on hover.
                    if tile_response.hovered() {
                        painter.rect_stroke(
                            tile_rect,
                            CornerRadius::same(theme.radius_sm as u8),
                            egui::Stroke::new(1.0, theme.border),
                            egui::StrokeKind::Inside,
                        );
                    }

                    // Type icon (large, centered in upper portion).
                    let icon_y = tile_rect.center().y - 4.0;
                    painter.text(
                        egui::pos2(tile_rect.center().x, icon_y),
                        egui::Align2::CENTER_CENTER,
                        source_icon(&row.source_type),
                        egui::FontId::proportional(18.0),
                        theme.text_primary,
                    );

                    // Name (small, bottom of tile, truncated).
                    let name_y = tile_rect.bottom() - 10.0;
                    let max_name_width = tile_size - 4.0;
                    let name_galley = painter.layout(
                        row.name.clone(),
                        egui::FontId::proportional(8.0),
                        theme.text_secondary,
                        max_name_width,
                    );
                    // Only draw the first line to avoid overflow.
                    painter.galley(
                        egui::pos2(
                            tile_rect.center().x - name_galley.size().x.min(max_name_width) / 2.0,
                            name_y - name_galley.rows[0].height() / 2.0,
                        ),
                        name_galley,
                        theme.text_secondary,
                    );

                    // Usage count badge (top-right corner).
                    if row.usage_count > 0 {
                        painter.text(
                            egui::pos2(tile_rect.right() - 4.0, tile_rect.top() + 8.0),
                            egui::Align2::RIGHT_CENTER,
                            format!("{}", row.usage_count),
                            egui::FontId::proportional(8.0),
                            theme.text_muted,
                        );
                    }

                    let is_renaming = state.renaming_source_id == Some(row.id);

                    // Click to select.
                    if tile_response.clicked() && !is_renaming {
                        state.selected_library_source_id = Some(row.id);
                        state.deselect_all();
                        let in_active_scene = state
                            .active_scene()
                            .map(|s| s.sources.iter().any(|ss| ss.source_id == row.id))
                            .unwrap_or(false);
                        if in_active_scene {
                            state.flash_source_id = Some(row.id);
                            state.flash_start = Some(std::time::Instant::now());
                        }
                    }

                    // Double-click to rename.
                    if tile_response.double_clicked() {
                        start_rename_source(ui, state, row.id, &row.name);
                    }

                    // Drag payload (only when not renaming).
                    if tile_response.drag_started() && !is_renaming {
                        tile_response.dnd_set_drag_payload(row.id);
                    }

                    // Context menu.
                    tile_response.context_menu(|ui| {
                        if ui.button("Rename").clicked() {
                            start_rename_source(ui, state, row.id, &row.name);
                            ui.close();
                        }
                        if ui.button("Delete").clicked() {
                            *delete_source = Some(row.id);
                            ui.close();
                        }
                    });

                    // Inline rename for grid tile: overlay a TextEdit on the name area.
                    if is_renaming {
                        let te_rect = Rect::from_min_size(
                            egui::pos2(tile_rect.left() + 2.0, tile_rect.bottom() - 18.0),
                            vec2(tile_size - 4.0, 16.0),
                        );
                        let mut child_ui =
                            ui.new_child(egui::UiBuilder::new().max_rect(te_rect).layout(
                                egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
                            ));
                        let te = egui::TextEdit::singleline(&mut state.rename_buffer)
                            .desired_width(tile_size - 8.0)
                            .font(egui::FontId::proportional(8.0))
                            .horizontal_align(egui::Align::Center);
                        let te_response = child_ui.add(te);
                        // Focus on first frame only — check a generation counter
                        // that resets each time a rename starts.
                        let gen_id = egui::Id::new("rename_gen");
                        let focused_gen_id = egui::Id::new(("rename_focused_gen", row.id.0));
                        let current_gen: u64 = ui.data(|d| d.get_temp(gen_id).unwrap_or(0));
                        let focused_gen: u64 = ui.data(|d| d.get_temp(focused_gen_id).unwrap_or(0));
                        if focused_gen != current_gen {
                            te_response.request_focus();
                            ui.data_mut(|d| d.insert_temp(focused_gen_id, current_gen));
                        }
                        let confirmed = te_response.lost_focus()
                            && !ui.input(|i| i.key_pressed(egui::Key::Escape));
                        let cancelled = ui.input(|i| i.key_pressed(egui::Key::Escape));
                        if confirmed {
                            let new_name = state.rename_buffer.trim().to_string();
                            if !new_name.is_empty() {
                                if let Some(lib_src) =
                                    state.library.iter_mut().find(|s| s.id == row.id)
                                {
                                    lib_src.name = new_name;
                                }
                                state.mark_dirty();
                            }
                            state.renaming_source_id = None;
                        } else if cancelled {
                            state.renaming_source_id = None;
                        }
                    }
                });

                if col_idx + 1 < cols && item_idx + 1 < items.len() {
                    ui.add_space(spacing);
                }
            }
        });
        if row_idx + 1 < rows_count {
            ui.add_space(spacing);
        }
    }
}

/// Draw a single source row with icon, name, usage badge, and context menu.
fn draw_source_row(
    ui: &mut egui::Ui,
    state: &mut AppState,
    row: &SourceRow,
    idx: usize,
    total: usize,
    delete_source: &mut Option<SourceId>,
) {
    let theme = active_theme(ui.ctx());
    let is_selected = state.selected_library_source_id == Some(row.id);
    let selected_bg = theme.accent_dim;

    ui.push_id(row.id.0, |ui| {
        let row_height = 28.0;
        let available_width = ui.available_width();
        let (row_rect, row_response) =
            ui.allocate_exact_size(vec2(available_width, row_height), Sense::click_and_drag());

        // Selection highlight background.
        if is_selected {
            draw_selection_highlight(ui.painter(), row_rect, selected_bg);
        }

        let is_renaming = state.renaming_source_id == Some(row.id);

        // Handle click for selection (library selection, not scene selection).
        if row_response.clicked() && !is_renaming {
            state.selected_library_source_id = Some(row.id);
            state.deselect_all();
            let in_active_scene = state
                .active_scene()
                .map(|s| s.sources.iter().any(|ss| ss.source_id == row.id))
                .unwrap_or(false);
            if in_active_scene {
                state.flash_source_id = Some(row.id);
                state.flash_start = Some(std::time::Instant::now());
            }
        }

        // Double-click to start rename.
        if row_response.double_clicked() {
            start_rename_source(ui, state, row.id, &row.name);
        }

        // Set drag payload (only when not renaming).
        if row_response.drag_started() && !is_renaming {
            row_response.dnd_set_drag_payload(row.id);
        }

        // Context menu (right-click).
        row_response.context_menu(|ui| {
            if ui.button("Rename").clicked() {
                start_rename_source(ui, state, row.id, &row.name);
                ui.close();
            }
            if ui.button("Delete").clicked() {
                *delete_source = Some(row.id);
                ui.close();
            }
        });

        // Paint the row contents.
        let painter = ui.painter_at(row_rect);
        let mut cursor_x = row_rect.left() + 4.0;
        let center_y = row_rect.center().y;

        // ── Icon (16x16, bg_elevated background, radius_sm border radius) ──
        let icon_size = 16.0;
        let icon_rect = Rect::from_center_size(
            egui::pos2(cursor_x + icon_size / 2.0, center_y),
            vec2(icon_size, icon_size),
        );
        painter.rect_filled(icon_rect, CornerRadius::same(theme.radius_sm as u8), theme.bg_elevated);
        painter.text(
            icon_rect.center(),
            egui::Align2::CENTER_CENTER,
            source_icon(&row.source_type),
            egui::FontId::proportional(10.0),
            theme.text_primary,
        );
        cursor_x += icon_size + 6.0;

        // ── Name: inline TextEdit when renaming, painted text otherwise ──
        if is_renaming {
            // Place a TextEdit at the name position.
            let name_width = row_rect.right() - cursor_x - 30.0;
            let text_edit_rect = Rect::from_min_size(
                egui::pos2(cursor_x, row_rect.top() + 2.0),
                vec2(name_width, row_height - 4.0),
            );
            let mut child_ui = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(text_edit_rect)
                    .layout(egui::Layout::left_to_right(egui::Align::Center)),
            );
            let te = egui::TextEdit::singleline(&mut state.rename_buffer)
                .desired_width(name_width)
                .font(egui::FontId::proportional(11.0));
            let te_response = child_ui.add(te);

            // Auto-focus on first frame.
            if te_response.gained_focus() || !te_response.has_focus() {
                te_response.request_focus();
            }

            // Confirm on Enter or loss of focus, cancel on Escape.
            let confirmed =
                te_response.lost_focus() && !ui.input(|i| i.key_pressed(egui::Key::Escape));
            let cancelled = ui.input(|i| i.key_pressed(egui::Key::Escape));

            if confirmed {
                let new_name = state.rename_buffer.trim().to_string();
                if !new_name.is_empty() {
                    if let Some(lib_src) = state.library.iter_mut().find(|s| s.id == row.id) {
                        lib_src.name = new_name;
                    }
                    state.mark_dirty();
                }
                state.renaming_source_id = None;
            } else if cancelled {
                state.renaming_source_id = None;
            }
        } else {
            painter.text(
                egui::pos2(cursor_x, center_y),
                egui::Align2::LEFT_CENTER,
                &row.name,
                egui::FontId::proportional(11.0),
                theme.text_primary,
            );
        }

        // ── Usage count badge (right-aligned) ──
        if row.usage_count > 0 {
            let badge_text = format!("{}", row.usage_count);
            let right_x = row_rect.right() - 8.0;
            painter.text(
                egui::pos2(right_x, center_y),
                egui::Align2::RIGHT_CENTER,
                &badge_text,
                egui::FontId::proportional(10.0),
                theme.text_secondary,
            );
        }

        // Separator line between items.
        if idx + 1 < total {
            let sep_y = row_rect.bottom();
            painter.line_segment(
                [
                    egui::pos2(row_rect.left(), sep_y),
                    egui::pos2(row_rect.right(), sep_y),
                ],
                Stroke::new(1.0, theme.border),
            );
        }
    });
}
