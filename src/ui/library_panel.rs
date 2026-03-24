//! Library panel — global source CRUD and browsing.
//!
//! Shows all sources in the global library, organized either by type or by folder.
//! Supports creating new sources, selecting them, and deleting with cascade
//! (removing from all scenes).

use crate::gstreamer::GstCommand;
use crate::scene::{LibrarySource, SourceId, SourceProperties, SourceType, Transform};
use crate::state::AppState;
use crate::ui::layout::tree::PanelId;
use crate::ui::theme::{
    BG_ELEVATED, BORDER, DEFAULT_ACCENT, RADIUS_SM, TEXT_MUTED, TEXT_PRIMARY, TEXT_SECONDARY,
    accent_dim,
};
use egui::{CornerRadius, Rect, Sense, Stroke, vec2};

/// View mode for the library panel.
#[derive(Clone, Copy, PartialEq, Eq)]
enum LibraryView {
    ByType,
    Folders,
}

/// Return a Phosphor icon for a given source type.
fn source_icon(source_type: &SourceType) -> &'static str {
    match source_type {
        SourceType::Display => egui_phosphor::regular::MONITOR,
        SourceType::Camera => egui_phosphor::regular::VIDEO_CAMERA,
        SourceType::Image => egui_phosphor::regular::IMAGE,
        SourceType::Browser => egui_phosphor::regular::BROWSER,
        SourceType::Audio => egui_phosphor::regular::SPEAKER_HIGH,
        SourceType::Window => egui_phosphor::regular::APP_WINDOW,
    }
}

/// Draw the library panel.
pub fn draw(ui: &mut egui::Ui, state: &mut AppState, _id: PanelId) {
    // Persist the view mode across frames using egui's data store.
    let view_id = ui.make_persistent_id("library_view_mode");
    let mut view = ui
        .data(|d| d.get_temp::<LibraryView>(view_id))
        .unwrap_or(LibraryView::ByType);

    // ── Header row: title + view toggles + "+" button ──
    ui.horizontal(|ui| {
        ui.colored_label(TEXT_PRIMARY, "Library");

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // "+" button to create new source
            draw_add_button(ui, state);

            ui.add_space(4.0);

            // View toggle buttons
            if ui
                .selectable_label(view == LibraryView::Folders, "Folders")
                .clicked()
            {
                view = LibraryView::Folders;
            }
            if ui
                .selectable_label(view == LibraryView::ByType, "By Type")
                .clicked()
            {
                view = LibraryView::ByType;
            }
        });
    });

    // Store the updated view mode.
    ui.data_mut(|d| d.insert_temp(view_id, view));

    ui.add_space(4.0);

    if state.library.is_empty() {
        ui.add_space(16.0);
        ui.centered_and_justified(|ui| {
            ui.colored_label(TEXT_MUTED, "No sources. Click + to add one.");
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

    egui::ScrollArea::vertical().show(ui, |ui| match view {
        LibraryView::ByType => {
            draw_by_type_view(ui, state, &rows, &mut delete_source);
        }
        LibraryView::Folders => {
            draw_folders_view(ui, state, &rows, &mut delete_source);
        }
    });

    // Apply deferred deletion.
    if let Some(src_id) = delete_source {
        delete_source_cascade(state, src_id);
    }
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
            use crate::ui::theme::{menu_item_icon, styled_menu};
            styled_menu(ui, |ui| {
                let items: &[(&str, SourceType)] = &[
                    ("Display", SourceType::Display),
                    ("Window", SourceType::Window),
                    ("Camera", SourceType::Camera),
                    ("Image", SourceType::Image),
                ];

                for (label, source_type) in items {
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
                    window_id: 0,
                    window_title: String::new(),
                    owner_name: String::new(),
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
        _ => ("Source".to_string(), SourceProperties::default()),
    };

    let lib_source = LibrarySource {
        id: new_id,
        name,
        source_type,
        properties,
        folder: None,
        transform: Transform::new(0.0, 0.0, 1920.0, 1080.0),
        native_size: (1920.0, 1080.0),
        opacity: 1.0,
        visible: true,
        muted: false,
        volume: 1.0,
    };

    state.library.push(lib_source);
    state.selected_source_id = Some(new_id);
    state.scenes_dirty = true;
    state.scenes_last_changed = std::time::Instant::now();
}

/// Delete a source from the library and cascade-remove it from all scenes.
fn delete_source_cascade(state: &mut AppState, src_id: SourceId) {
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
    if state.selected_source_id == Some(src_id) {
        state.selected_source_id = None;
    }

    state.scenes_dirty = true;
    state.scenes_last_changed = std::time::Instant::now();
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
    delete_source: &mut Option<SourceId>,
) {
    let type_order: &[SourceType] = &[
        SourceType::Display,
        SourceType::Camera,
        SourceType::Window,
        SourceType::Image,
        SourceType::Audio,
        SourceType::Browser,
    ];

    for source_type in type_order {
        let section_rows: Vec<&SourceRow> = rows
            .iter()
            .filter(|r| {
                std::mem::discriminant(&r.source_type) == std::mem::discriminant(source_type)
            })
            .collect();

        if section_rows.is_empty() {
            continue;
        }

        let label = type_section_label(source_type);
        egui::CollapsingHeader::new(label)
            .default_open(true)
            .show(ui, |ui| {
                for (idx, row) in section_rows.iter().enumerate() {
                    draw_source_row(ui, state, row, idx, section_rows.len(), delete_source);
                }
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
    delete_source: &mut Option<SourceId>,
) {
    // Collect unique folder names (sorted).
    let mut folders: Vec<String> = rows.iter().filter_map(|r| r.folder.clone()).collect();
    folders.sort();
    folders.dedup();

    // Named folders first.
    for folder in &folders {
        let folder_rows: Vec<&SourceRow> = rows
            .iter()
            .filter(|r| r.folder.as_deref() == Some(folder.as_str()))
            .collect();

        if folder_rows.is_empty() {
            continue;
        }

        egui::CollapsingHeader::new(folder.as_str())
            .default_open(true)
            .show(ui, |ui| {
                for (idx, row) in folder_rows.iter().enumerate() {
                    draw_source_row(ui, state, row, idx, folder_rows.len(), delete_source);
                }
            });
    }

    // "Unfiled" section for sources without a folder.
    let unfiled_rows: Vec<&SourceRow> = rows.iter().filter(|r| r.folder.is_none()).collect();

    if !unfiled_rows.is_empty() {
        egui::CollapsingHeader::new("Unfiled")
            .default_open(true)
            .show(ui, |ui| {
                for (idx, row) in unfiled_rows.iter().enumerate() {
                    draw_source_row(ui, state, row, idx, unfiled_rows.len(), delete_source);
                }
            });
    }
}

// ---------------------------------------------------------------------------
// Source row rendering
// ---------------------------------------------------------------------------

/// Draw a single source row with icon, name, usage badge, and context menu.
fn draw_source_row(
    ui: &mut egui::Ui,
    state: &mut AppState,
    row: &SourceRow,
    idx: usize,
    total: usize,
    delete_source: &mut Option<SourceId>,
) {
    let is_selected = state.selected_source_id == Some(row.id);
    let selected_bg = accent_dim(DEFAULT_ACCENT);

    ui.push_id(row.id.0, |ui| {
        let row_height = 28.0;
        let available_width = ui.available_width();
        let (row_rect, row_response) =
            ui.allocate_exact_size(vec2(available_width, row_height), Sense::click());

        // Selection highlight background.
        if is_selected {
            ui.painter()
                .rect_filled(row_rect, CornerRadius::same(RADIUS_SM as u8), selected_bg);
        }

        // Handle click for selection.
        if row_response.clicked() {
            state.selected_source_id = Some(row.id);
        }

        // Context menu (right-click).
        row_response.context_menu(|ui| {
            if ui.button("Delete").clicked() {
                *delete_source = Some(row.id);
                ui.close();
            }
        });

        // Paint the row contents.
        let painter = ui.painter_at(row_rect);
        let mut cursor_x = row_rect.left() + 4.0;
        let center_y = row_rect.center().y;

        // ── Icon (16x16, BG_ELEVATED background, RADIUS_SM border radius) ──
        let icon_size = 16.0;
        let icon_rect = Rect::from_center_size(
            egui::pos2(cursor_x + icon_size / 2.0, center_y),
            vec2(icon_size, icon_size),
        );
        painter.rect_filled(icon_rect, CornerRadius::same(RADIUS_SM as u8), BG_ELEVATED);
        let icon_text = source_icon(&row.source_type);
        painter.text(
            icon_rect.center(),
            egui::Align2::CENTER_CENTER,
            icon_text,
            egui::FontId::proportional(10.0),
            TEXT_PRIMARY,
        );
        cursor_x += icon_size + 6.0;

        // ── Name (TEXT_PRIMARY, 11px) ──
        painter.text(
            egui::pos2(cursor_x, center_y),
            egui::Align2::LEFT_CENTER,
            &row.name,
            egui::FontId::proportional(11.0),
            TEXT_PRIMARY,
        );

        // ── Usage count badge (right-aligned) ──
        if row.usage_count > 0 {
            let badge_text = format!("{}", row.usage_count);
            let right_x = row_rect.right() - 8.0;
            painter.text(
                egui::pos2(right_x, center_y),
                egui::Align2::RIGHT_CENTER,
                &badge_text,
                egui::FontId::proportional(10.0),
                TEXT_SECONDARY,
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
                Stroke::new(1.0, BORDER),
            );
        }
    });
}
