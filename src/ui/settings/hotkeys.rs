use egui::{Align, Layout, Ui};

use crate::settings::{HOTKEY_ACTIONS, HotkeyBinding, HotkeySettings};
use crate::ui::theme::active_theme;

use super::{labeled_row, section_header};

/// egui ID for the hotkey row currently in "recording" (capture) mode.
const RECORDING_ID: &str = "hotkey_recording_action";

pub(super) fn draw(ui: &mut Ui, settings: &mut HotkeySettings) -> bool {
    let theme = active_theme(ui.ctx());
    let mut changed = false;

    // Track which action is in recording mode (persisted across frames).
    let recording_id = egui::Id::new(RECORDING_ID);
    let mut recording_action: Option<String> =
        ui.ctx().data_mut(|d| d.get_temp::<String>(recording_id));

    section_header(ui, "BINDINGS");

    ui.label(
        egui::RichText::new("Click a binding to record a new shortcut. Press Escape to cancel.")
            .size(11.0)
            .color(theme.text_muted),
    );
    ui.add_space(4.0);

    for &(action_id, label, _default) in HOTKEY_ACTIONS {
        let is_recording = recording_action.as_deref() == Some(action_id);
        let binding = settings
            .bindings
            .get(action_id)
            .cloned()
            .unwrap_or_else(HotkeyBinding::none);

        ui.horizontal(|ui| {
            labeled_row(ui, label);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                // Clear button
                if binding.is_set() {
                    if ui
                        .button(
                            egui::RichText::new(egui_phosphor::regular::X)
                                .size(12.0)
                                .color(theme.text_muted),
                        )
                        .on_hover_text("Clear binding")
                        .clicked()
                    {
                        settings
                            .bindings
                            .insert(action_id.to_string(), HotkeyBinding::none());
                        changed = true;
                        if is_recording {
                            recording_action = None;
                        }
                    }
                }

                // Binding button (click to record)
                let display_text = if is_recording {
                    "Press a key...".to_string()
                } else if binding.is_set() {
                    binding.display()
                } else {
                    "Not set".to_string()
                };

                let text_color = if is_recording {
                    theme.accent
                } else if binding.is_set() {
                    theme.text_primary
                } else {
                    theme.text_muted
                };

                let btn = ui.add(
                    egui::Button::new(
                        egui::RichText::new(&display_text)
                            .size(12.0)
                            .color(text_color),
                    )
                    .min_size(egui::vec2(150.0, 24.0))
                    .stroke(if is_recording {
                        egui::Stroke::new(1.0, theme.accent)
                    } else {
                        egui::Stroke::new(1.0, theme.border_subtle)
                    }),
                );

                if btn.clicked() {
                    if is_recording {
                        // Cancel recording
                        recording_action = None;
                    } else {
                        recording_action = Some(action_id.to_string());
                    }
                }
            });
        });
    }

    // Handle key capture when in recording mode.
    if let Some(action) = recording_action.clone() {
        // Collect captured key from input events.
        let mut captured: Option<HotkeyBinding> = None;
        let mut cancelled = false;

        ui.ctx().input(|input| {
            for event in &input.events {
                if let egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } = event
                {
                    if *key == egui::Key::Escape {
                        cancelled = true;
                        return;
                    }
                    // Skip bare special keys without modifiers.
                    if matches!(
                        key,
                        egui::Key::Backspace | egui::Key::Tab | egui::Key::Enter
                    ) && !modifiers.ctrl
                        && !modifiers.shift
                        && !modifiers.alt
                        && !modifiers.command
                    {
                        return;
                    }

                    if let Some(binding) = egui_key_to_binding(*key, modifiers) {
                        captured = Some(binding);
                    }
                }
            }
        });

        if cancelled {
            recording_action = None;
        } else if let Some(binding) = captured {
            settings.bindings.insert(action, binding);
            changed = true;
            recording_action = None;
        }

        // Request repaint while recording so we catch key events promptly.
        ui.ctx().request_repaint();
    }

    // Persist recording state.
    ui.ctx().data_mut(|d| {
        if let Some(ref action) = recording_action {
            d.insert_temp(recording_id, action.clone());
        } else {
            d.remove_temp::<String>(recording_id);
        }
    });

    // Reset to defaults button.
    ui.add_space(16.0);
    section_header(ui, "");
    if ui.button("Reset to Defaults").clicked() {
        *settings = HotkeySettings::default();
        changed = true;
    }

    changed
}

/// Convert an egui `Key` + `Modifiers` to a `HotkeyBinding`.
fn egui_key_to_binding(key: egui::Key, mods: &egui::Modifiers) -> Option<HotkeyBinding> {
    let key_name = egui_key_name(key)?;
    Some(HotkeyBinding {
        ctrl: mods.ctrl,
        shift: mods.shift,
        alt: mods.alt,
        super_key: mods.command,
        key: key_name,
    })
}

/// Map egui::Key to a canonical key name matching our format.
fn egui_key_name(key: egui::Key) -> Option<String> {
    let name = match key {
        egui::Key::A => "A",
        egui::Key::B => "B",
        egui::Key::C => "C",
        egui::Key::D => "D",
        egui::Key::E => "E",
        egui::Key::F => "F",
        egui::Key::G => "G",
        egui::Key::H => "H",
        egui::Key::I => "I",
        egui::Key::J => "J",
        egui::Key::K => "K",
        egui::Key::L => "L",
        egui::Key::M => "M",
        egui::Key::N => "N",
        egui::Key::O => "O",
        egui::Key::P => "P",
        egui::Key::Q => "Q",
        egui::Key::R => "R",
        egui::Key::S => "S",
        egui::Key::T => "T",
        egui::Key::U => "U",
        egui::Key::V => "V",
        egui::Key::W => "W",
        egui::Key::X => "X",
        egui::Key::Y => "Y",
        egui::Key::Z => "Z",
        egui::Key::Num0 => "0",
        egui::Key::Num1 => "1",
        egui::Key::Num2 => "2",
        egui::Key::Num3 => "3",
        egui::Key::Num4 => "4",
        egui::Key::Num5 => "5",
        egui::Key::Num6 => "6",
        egui::Key::Num7 => "7",
        egui::Key::Num8 => "8",
        egui::Key::Num9 => "9",
        egui::Key::F1 => "F1",
        egui::Key::F2 => "F2",
        egui::Key::F3 => "F3",
        egui::Key::F4 => "F4",
        egui::Key::F5 => "F5",
        egui::Key::F6 => "F6",
        egui::Key::F7 => "F7",
        egui::Key::F8 => "F8",
        egui::Key::F9 => "F9",
        egui::Key::F10 => "F10",
        egui::Key::F11 => "F11",
        egui::Key::F12 => "F12",
        egui::Key::Space => "Space",
        egui::Key::Enter => "Enter",
        egui::Key::Escape => "Escape",
        egui::Key::Backspace => "Backspace",
        egui::Key::Delete => "Delete",
        egui::Key::Tab => "Tab",
        egui::Key::ArrowUp => "Up",
        egui::Key::ArrowDown => "Down",
        egui::Key::ArrowLeft => "Left",
        egui::Key::ArrowRight => "Right",
        egui::Key::Home => "Home",
        egui::Key::End => "End",
        egui::Key::PageUp => "PageUp",
        egui::Key::PageDown => "PageDown",
        egui::Key::Insert => "Insert",
        egui::Key::OpenBracket => "[",
        egui::Key::CloseBracket => "]",
        egui::Key::Comma => ",",
        egui::Key::Period => ".",
        egui::Key::Minus => "-",
        egui::Key::Plus => "=",
        egui::Key::Semicolon => ";",
        egui::Key::Backtick => "`",
        egui::Key::Backslash => "\\",
        egui::Key::Slash => "/",
        egui::Key::Quote => "'",
        _ => return None,
    };
    Some(name.to_string())
}
