use crate::gstreamer::{EncoderType, QualityPreset, RecordingFormat, StreamDestination};
use crate::ui::theme::ThemeId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Top-level application settings, persisted as TOML.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppSettings {
    #[serde(default)]
    pub ui: UiSettings,
    #[serde(default)]
    pub general: GeneralSettings,
    #[serde(default)]
    pub stream: StreamSettings,
    #[serde(default)]
    pub record: RecordSettings,
    #[serde(default)]
    pub audio: AudioSettings,
    #[serde(default)]
    pub video: VideoSettings,
    #[serde(default)]
    pub hotkeys: HotkeySettings,
    #[serde(default)]
    pub appearance: AppearanceSettings,
    #[serde(default)]
    pub advanced: AdvancedSettings,
    #[serde(default)]
    pub settings_window: SettingsWindowConfig,
    #[serde(default)]
    pub transitions: crate::transition::TransitionSettings,
}

/// UI panel visibility settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UiSettings {
    pub scene_panel_open: bool,
    pub mixer_panel_open: bool,
    pub controls_panel_open: bool,
    /// Library grouping mode: "type" or "folders".
    pub library_view: String,
    /// Library display mode: "list" or "grid".
    pub library_display_mode: String,
}

impl Default for UiSettings {
    fn default() -> Self {
        Self {
            scene_panel_open: true,
            mixer_panel_open: true,
            controls_panel_open: true,
            library_view: "type".to_string(),
            library_display_mode: "list".to_string(),
        }
    }
}

/// General application preferences.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GeneralSettings {
    pub language: String,
    pub check_for_updates: bool,
    pub launch_on_startup: bool,
    pub confirm_close_while_streaming: bool,
    pub snap_to_grid: bool,
    pub snap_grid_size: f32,
    /// Exclude Lodestone windows from display capture.
    pub exclude_self_from_capture: bool,
    /// Grid preset name (e.g., "custom").
    #[serde(default)]
    pub grid_preset: String,
    /// Whether to show the grid overlay.
    #[serde(default)]
    pub show_grid: bool,
    /// Whether to show rule-of-thirds overlay.
    #[serde(default)]
    pub show_thirds: bool,
    /// Whether to show safe zone overlays.
    #[serde(default)]
    pub show_safe_zones: bool,
    /// Grid line color (RGB).
    #[serde(default = "default_grid_color")]
    pub grid_color: [u8; 3],
    /// Grid line opacity [0.0, 1.0].
    #[serde(default = "default_grid_opacity")]
    pub grid_opacity: f32,
    /// Guide line color (RGB).
    #[serde(default = "default_guide_color")]
    pub guide_color: [u8; 3],
    /// Guide line opacity [0.0, 1.0].
    #[serde(default = "default_guide_opacity")]
    pub guide_opacity: f32,
}

fn default_grid_color() -> [u8; 3] {
    [255, 255, 255]
}
fn default_grid_opacity() -> f32 {
    0.15
}
fn default_guide_color() -> [u8; 3] {
    [0, 255, 255]
}
fn default_guide_opacity() -> f32 {
    0.60
}

impl Default for GeneralSettings {
    fn default() -> Self {
        Self {
            language: "en-US".to_string(),
            check_for_updates: true,
            launch_on_startup: false,
            confirm_close_while_streaming: true,
            snap_to_grid: true,
            snap_grid_size: 10.0,
            exclude_self_from_capture: true,
            grid_preset: String::new(),
            show_grid: false,
            show_thirds: false,
            show_safe_zones: false,
            grid_color: default_grid_color(),
            grid_opacity: default_grid_opacity(),
            guide_color: default_guide_color(),
            guide_opacity: default_guide_opacity(),
        }
    }
}

/// Stream output configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct StreamSettings {
    pub stream_key: String,
    pub destination: StreamDestination,
    pub encoder: EncoderType,
    pub quality_preset: QualityPreset,
    pub bitrate_kbps: u32,
    pub fps: u32,
}

impl Default for StreamSettings {
    fn default() -> Self {
        Self {
            stream_key: String::new(),
            destination: StreamDestination::Twitch,
            encoder: EncoderType::H264VideoToolbox,
            quality_preset: QualityPreset::Medium,
            bitrate_kbps: 4500,
            fps: 30,
        }
    }
}

/// Recording output settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RecordSettings {
    pub format: RecordingFormat,
    pub output_folder: PathBuf,
    pub filename_template: String,
    pub encoder: EncoderType,
    pub quality_preset: QualityPreset,
    pub bitrate_kbps: u32,
    pub fps: u32,
}

impl Default for RecordSettings {
    fn default() -> Self {
        Self {
            format: RecordingFormat::Mkv,
            output_folder: dirs::video_dir()
                .or_else(dirs::home_dir)
                .unwrap_or_else(|| PathBuf::from(".")),
            filename_template: "{date}_{time}_{scene}".to_string(),
            encoder: EncoderType::H264VideoToolbox,
            quality_preset: QualityPreset::High,
            bitrate_kbps: 8000,
            fps: 30,
        }
    }
}

impl RecordSettings {
    /// Expand a filename template, replacing tokens with actual values.
    pub fn expand_template(template: &str, scene_name: &str, counter: u32) -> String {
        let now = chrono::Local::now();
        let sanitized_scene: String = scene_name
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .collect();

        template
            .replace("{date}", &now.format("%Y-%m-%d").to_string())
            .replace("{time}", &now.format("%H-%M-%S").to_string())
            .replace("{scene}", &sanitized_scene)
            .replace("{n}", &counter.to_string())
    }
}

/// Audio device and capture settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AudioSettings {
    pub input_device: String,
    pub output_device: String,
    pub sample_rate: u32,
    pub monitoring: String,
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            input_device: "Default".to_string(),
            output_device: "Default".to_string(),
            sample_rate: 48000,
            monitoring: "off".to_string(),
        }
    }
}

/// Video capture and rendering settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct VideoSettings {
    pub base_resolution: String,
    pub output_resolution: String,
    pub fps: u32,
    pub color_space: String,
}

impl Default for VideoSettings {
    fn default() -> Self {
        Self {
            base_resolution: "1920x1080".to_string(),
            output_resolution: "1920x1080".to_string(),
            fps: 30,
            color_space: "sRGB".to_string(),
        }
    }
}

/// A parsed hotkey binding: modifier flags + key name.
///
/// Serialized as a human-readable string like `"Ctrl+Shift+W"` or `"F5"`.
/// An empty string means "not set".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HotkeyBinding {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub super_key: bool,
    pub key: String,
}

impl HotkeyBinding {
    /// An unbound (empty) hotkey.
    pub fn none() -> Self {
        Self {
            ctrl: false,
            shift: false,
            alt: false,
            super_key: false,
            key: String::new(),
        }
    }

    /// Whether this binding is set.
    pub fn is_set(&self) -> bool {
        !self.key.is_empty()
    }

    /// Format as a human-readable string like `"Ctrl+Shift+W"`.
    pub fn display(&self) -> String {
        if !self.is_set() {
            return String::new();
        }
        let mut parts = Vec::new();
        if self.ctrl {
            parts.push("Ctrl");
        }
        if self.alt {
            parts.push("Alt");
        }
        if self.shift {
            parts.push("Shift");
        }
        if self.super_key {
            #[cfg(target_os = "macos")]
            parts.push("Cmd");
            #[cfg(not(target_os = "macos"))]
            parts.push("Win");
        }
        parts.push(&self.key);
        parts.join("+")
    }

    /// Parse from a string like `"Ctrl+Shift+W"`.
    pub fn parse(s: &str) -> Self {
        let s = s.trim();
        if s.is_empty() {
            return Self::none();
        }
        let parts: Vec<&str> = s.split('+').map(str::trim).collect();
        let mut binding = Self::none();
        for (i, part) in parts.iter().enumerate() {
            if i == parts.len() - 1 {
                // Last part is the key
                binding.key = part.to_string();
            } else {
                match part.to_lowercase().as_str() {
                    "ctrl" | "control" => binding.ctrl = true,
                    "shift" => binding.shift = true,
                    "alt" | "option" | "opt" => binding.alt = true,
                    "cmd" | "command" | "super" | "win" | "meta" => binding.super_key = true,
                    _ => {} // Ignore unknown modifiers
                }
            }
        }
        binding
    }

    /// Check if this binding matches a winit key event.
    pub fn matches(
        &self,
        key_code: &winit::keyboard::KeyCode,
        modifiers: &winit::keyboard::ModifiersState,
    ) -> bool {
        if !self.is_set() {
            return false;
        }
        if self.ctrl != modifiers.control_key() {
            return false;
        }
        if self.shift != modifiers.shift_key() {
            return false;
        }
        if self.alt != modifiers.alt_key() {
            return false;
        }
        if self.super_key != modifiers.super_key() {
            return false;
        }
        key_code_to_name(key_code).is_some_and(|name| name.eq_ignore_ascii_case(&self.key))
    }

    /// Build from a winit key event.
    #[allow(dead_code)]
    pub fn from_key_event(
        key_code: &winit::keyboard::KeyCode,
        modifiers: &winit::keyboard::ModifiersState,
    ) -> Option<Self> {
        let key = key_code_to_name(key_code)?;
        Some(Self {
            ctrl: modifiers.control_key(),
            shift: modifiers.shift_key(),
            alt: modifiers.alt_key(),
            super_key: modifiers.super_key(),
            key,
        })
    }
}

impl Serialize for HotkeyBinding {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.display())
    }
}

impl<'de> Deserialize<'de> for HotkeyBinding {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Ok(Self::parse(&s))
    }
}

/// Convert a winit `KeyCode` to its canonical display name.
pub fn key_code_to_name(code: &winit::keyboard::KeyCode) -> Option<String> {
    use winit::keyboard::KeyCode;
    let name = match code {
        KeyCode::KeyA => "A",
        KeyCode::KeyB => "B",
        KeyCode::KeyC => "C",
        KeyCode::KeyD => "D",
        KeyCode::KeyE => "E",
        KeyCode::KeyF => "F",
        KeyCode::KeyG => "G",
        KeyCode::KeyH => "H",
        KeyCode::KeyI => "I",
        KeyCode::KeyJ => "J",
        KeyCode::KeyK => "K",
        KeyCode::KeyL => "L",
        KeyCode::KeyM => "M",
        KeyCode::KeyN => "N",
        KeyCode::KeyO => "O",
        KeyCode::KeyP => "P",
        KeyCode::KeyQ => "Q",
        KeyCode::KeyR => "R",
        KeyCode::KeyS => "S",
        KeyCode::KeyT => "T",
        KeyCode::KeyU => "U",
        KeyCode::KeyV => "V",
        KeyCode::KeyW => "W",
        KeyCode::KeyX => "X",
        KeyCode::KeyY => "Y",
        KeyCode::KeyZ => "Z",
        KeyCode::Digit0 => "0",
        KeyCode::Digit1 => "1",
        KeyCode::Digit2 => "2",
        KeyCode::Digit3 => "3",
        KeyCode::Digit4 => "4",
        KeyCode::Digit5 => "5",
        KeyCode::Digit6 => "6",
        KeyCode::Digit7 => "7",
        KeyCode::Digit8 => "8",
        KeyCode::Digit9 => "9",
        KeyCode::F1 => "F1",
        KeyCode::F2 => "F2",
        KeyCode::F3 => "F3",
        KeyCode::F4 => "F4",
        KeyCode::F5 => "F5",
        KeyCode::F6 => "F6",
        KeyCode::F7 => "F7",
        KeyCode::F8 => "F8",
        KeyCode::F9 => "F9",
        KeyCode::F10 => "F10",
        KeyCode::F11 => "F11",
        KeyCode::F12 => "F12",
        KeyCode::Space => "Space",
        KeyCode::Enter => "Enter",
        KeyCode::NumpadEnter => "NumpadEnter",
        KeyCode::Escape => "Escape",
        KeyCode::Backspace => "Backspace",
        KeyCode::Delete => "Delete",
        KeyCode::Tab => "Tab",
        KeyCode::ArrowUp => "Up",
        KeyCode::ArrowDown => "Down",
        KeyCode::ArrowLeft => "Left",
        KeyCode::ArrowRight => "Right",
        KeyCode::Home => "Home",
        KeyCode::End => "End",
        KeyCode::PageUp => "PageUp",
        KeyCode::PageDown => "PageDown",
        KeyCode::Insert => "Insert",
        KeyCode::BracketLeft => "[",
        KeyCode::BracketRight => "]",
        KeyCode::Comma => ",",
        KeyCode::Period => ".",
        KeyCode::Slash => "/",
        KeyCode::Backslash => "\\",
        KeyCode::Semicolon => ";",
        KeyCode::Quote => "'",
        KeyCode::Backquote => "`",
        KeyCode::Minus => "-",
        KeyCode::Equal => "=",
        _ => return None,
    };
    Some(name.to_string())
}

/// All configurable hotkey actions with their display names and default bindings.
pub const HOTKEY_ACTIONS: &[(&str, &str, &str)] = &[
    ("start_streaming", "Start Streaming", ""),
    ("stop_streaming", "Stop Streaming", ""),
    ("start_recording", "Start Recording", ""),
    ("stop_recording", "Stop Recording", ""),
    ("toggle_mute_mic", "Toggle Mute Mic", ""),
    ("toggle_mute_desktop", "Toggle Mute Desktop", ""),
    (
        "capture_foreground_window",
        "Capture Foreground Window",
        "Ctrl+Shift+W",
    ),
];

/// User-defined hotkey bindings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct HotkeySettings {
    pub bindings: HashMap<String, HotkeyBinding>,
}

impl Default for HotkeySettings {
    fn default() -> Self {
        let mut bindings = HashMap::new();
        for &(action, _, default_binding) in HOTKEY_ACTIONS {
            bindings.insert(action.to_string(), HotkeyBinding::parse(default_binding));
        }
        Self { bindings }
    }
}

impl HotkeySettings {
    /// Look up the binding for an action. Returns `None` if unbound.
    pub fn get(&self, action: &str) -> Option<&HotkeyBinding> {
        self.bindings.get(action).filter(|b| b.is_set())
    }
}

/// Font scale presets that proportionally scale all text sizes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum FontScale {
    XS,
    S,
    #[default]
    M,
    L,
    XL,
}

impl FontScale {
    /// Zoom factor for this scale. 1.0 = default (M). Applied via egui's zoom_factor
    /// to scale all text, spacing, and widgets uniformly.
    pub fn zoom_factor(&self) -> f32 {
        match self {
            Self::XS => 0.85,
            Self::S => 0.92,
            Self::M => 1.0,
            Self::L => 1.10,
            Self::XL => 1.20,
        }
    }

    /// All scales in display order.
    pub fn all() -> &'static [FontScale] {
        &[Self::XS, Self::S, Self::M, Self::L, Self::XL]
    }

    /// Human-readable label.
    pub fn label(&self) -> &'static str {
        match self {
            Self::XS => "XS",
            Self::S => "S",
            Self::M => "M",
            Self::L => "L",
            Self::XL => "XL",
        }
    }
}

/// Visual appearance preferences.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppearanceSettings {
    pub theme: ThemeId,
    /// Hex color override for the accent (e.g. "#ff8800"). `None` = use theme default.
    pub accent_color: Option<String>,
    pub font_scale: FontScale,
    pub font_family: String,
}

impl Default for AppearanceSettings {
    fn default() -> Self {
        Self {
            theme: ThemeId::DefaultDark,
            accent_color: None,
            font_scale: FontScale::M,
            font_family: "Default".to_string(),
        }
    }
}

/// Advanced/power-user settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AdvancedSettings {
    pub process_priority: String,
    pub network_buffer_size_kb: u32,
}

impl Default for AdvancedSettings {
    fn default() -> Self {
        Self {
            process_priority: "normal".to_string(),
            network_buffer_size_kb: 2048,
        }
    }
}

/// Settings window geometry.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SettingsWindowConfig {
    pub width: f32,
    pub height: f32,
}

impl Default for SettingsWindowConfig {
    fn default() -> Self {
        Self {
            width: 700.0,
            height: 500.0,
        }
    }
}

impl AppSettings {
    #[allow(dead_code)]
    pub fn load_from(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(contents) => toml::from_str(&contents).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    /// Load settings from disk. On first launch (no settings file), use the
    /// detected monitor resolution for video/stream defaults instead of 1920x1080.
    pub fn load_or_detect(path: &Path, detected: Option<(u32, u32)>) -> Self {
        if path.exists() {
            match std::fs::read_to_string(path) {
                Ok(contents) => toml::from_str(&contents).unwrap_or_default(),
                Err(_) => Self::default(),
            }
        } else if let Some((w, h)) = detected {
            let res_str = format!("{w}x{h}");
            let mut settings = Self::default();
            settings.video.base_resolution = res_str.clone();
            settings.video.output_resolution = res_str;
            settings
        } else {
            Self::default()
        }
    }

    pub fn save_to(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let toml_str = toml::to_string_pretty(self)?;
        std::fs::write(path, toml_str)?;
        Ok(())
    }
}

pub fn config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("lodestone")
}

pub fn settings_path() -> PathBuf {
    config_dir().join("settings.toml")
}

pub fn scenes_path() -> PathBuf {
    config_dir().join("scenes.toml")
}

pub fn transitions_dir() -> PathBuf {
    config_dir().join("transitions")
}

pub fn effects_dir() -> PathBuf {
    config_dir().join("effects")
}

/// Write built-in effect shaders to the effects directory (if not already present).
pub fn seed_builtin_effects() {
    let dir = effects_dir();
    if let Err(e) = std::fs::create_dir_all(&dir) {
        log::warn!("Failed to create effects directory: {e}");
        return;
    }
    let builtins: &[(&str, &str)] = &[
        (
            "circle_crop.wgsl",
            include_str!("renderer/shaders/effect_circle_crop.wgsl"),
        ),
        (
            "rounded_corners.wgsl",
            include_str!("renderer/shaders/effect_rounded_corners.wgsl"),
        ),
        (
            "gradient_fade.wgsl",
            include_str!("renderer/shaders/effect_gradient_fade.wgsl"),
        ),
        (
            "color_correction.wgsl",
            include_str!("renderer/shaders/effect_color_correction.wgsl"),
        ),
        (
            "chroma_key.wgsl",
            include_str!("renderer/shaders/effect_chroma_key.wgsl"),
        ),
        (
            "blur.wgsl",
            include_str!("renderer/shaders/effect_blur.wgsl"),
        ),
        (
            "vignette.wgsl",
            include_str!("renderer/shaders/effect_vignette.wgsl"),
        ),
        (
            "pixelate.wgsl",
            include_str!("renderer/shaders/effect_pixelate.wgsl"),
        ),
        (
            "sepia.wgsl",
            include_str!("renderer/shaders/effect_sepia.wgsl"),
        ),
        (
            "invert.wgsl",
            include_str!("renderer/shaders/effect_invert.wgsl"),
        ),
        (
            "mirror.wgsl",
            include_str!("renderer/shaders/effect_mirror.wgsl"),
        ),
        (
            "scanlines.wgsl",
            include_str!("renderer/shaders/effect_scanlines.wgsl"),
        ),
        (
            "rgb_shift.wgsl",
            include_str!("renderer/shaders/effect_rgb_shift.wgsl"),
        ),
        (
            "film_grain.wgsl",
            include_str!("renderer/shaders/effect_film_grain.wgsl"),
        ),
        (
            "outline.wgsl",
            include_str!("renderer/shaders/effect_outline.wgsl"),
        ),
        (
            "zoom_blur.wgsl",
            include_str!("renderer/shaders/effect_zoom_blur.wgsl"),
        ),
    ];
    for (filename, content) in builtins {
        let path = dir.join(filename);
        if !path.exists() {
            if let Err(e) = std::fs::write(&path, content) {
                log::warn!("Failed to write built-in effect {filename}: {e}");
            }
        }
    }
}

/// Seed the transitions directory with built-in shaders on first launch.
/// Only writes files that don't already exist, so user modifications are preserved.
pub fn seed_builtin_transitions() {
    let dir = transitions_dir();
    if let Err(e) = std::fs::create_dir_all(&dir) {
        log::warn!("Failed to create transitions directory: {e}");
        return;
    }

    let builtins: &[(&str, &str)] = &[
        (
            "fade.wgsl",
            include_str!("renderer/shaders/transition_fade.wgsl"),
        ),
        (
            "dip_to_color.wgsl",
            include_str!("renderer/shaders/transition_dip_to_color.wgsl"),
        ),
        (
            "wipe.wgsl",
            include_str!("renderer/shaders/transition_wipe.wgsl"),
        ),
        (
            "slide.wgsl",
            include_str!("renderer/shaders/transition_slide.wgsl"),
        ),
        (
            "radial_wipe.wgsl",
            include_str!("renderer/shaders/transition_radial_wipe.wgsl"),
        ),
        (
            "dissolve.wgsl",
            include_str!("renderer/shaders/transition_dissolve.wgsl"),
        ),
    ];

    for (filename, source) in builtins {
        let path = dir.join(filename);
        if !path.exists()
            && let Err(e) = std::fs::write(&path, source)
        {
            log::warn!("Failed to write built-in transition {filename}: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gstreamer::{EncoderType, QualityPreset};
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn expanded_settings_roundtrip() {
        let settings = AppSettings::default();
        let toml_str = toml::to_string_pretty(&settings).unwrap();
        let parsed: AppSettings = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.general.language, "en-US");
        assert_eq!(parsed.stream.bitrate_kbps, 4500);
        assert_eq!(parsed.settings_window.width, 700.0);
    }

    #[test]
    fn backwards_compat_empty_toml() {
        let parsed: AppSettings = toml::from_str("").unwrap();
        assert_eq!(parsed.general.language, "en-US");
        assert!(parsed.general.check_for_updates);
    }

    #[test]
    fn backwards_compat_old_format() {
        let old_toml = r#"
active_profile = "Default"

[ui]
scene_panel_open = true
mixer_panel_open = true
controls_panel_open = true
"#;
        let parsed: AppSettings = toml::from_str(old_toml).unwrap();
        assert_eq!(parsed.general.language, "en-US");
        assert_eq!(parsed.stream.bitrate_kbps, 4500);
    }

    #[test]
    fn stream_settings_roundtrip() {
        let settings = AppSettings::default();
        let toml_str = toml::to_string_pretty(&settings).unwrap();
        let parsed: AppSettings = toml::from_str(&toml_str).unwrap();
        assert!(matches!(
            parsed.stream.destination,
            StreamDestination::Twitch
        ));
        assert_eq!(parsed.stream.encoder, EncoderType::H264VideoToolbox);
        assert_eq!(parsed.stream.quality_preset, QualityPreset::Medium);
        assert_eq!(parsed.stream.fps, 30);
    }

    #[test]
    fn load_nonexistent_returns_default() {
        let settings = AppSettings::load_from(Path::new("/nonexistent/path/settings.toml"));
        assert_eq!(settings.general.language, "en-US");
    }

    #[test]
    fn save_and_load_roundtrip() {
        let mut file = NamedTempFile::new().unwrap();
        let settings = AppSettings::default();
        let toml_str = toml::to_string_pretty(&settings).unwrap();
        file.write_all(toml_str.as_bytes()).unwrap();
        let loaded = AppSettings::load_from(file.path());
        assert_eq!(loaded.general.language, settings.general.language);
        assert_eq!(loaded.stream.bitrate_kbps, settings.stream.bitrate_kbps);
    }

    #[test]
    fn load_or_detect_first_launch_uses_detected_resolution() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        // File does not exist — should use detected resolution
        let settings = AppSettings::load_or_detect(&path, Some((3360, 1890)));
        assert_eq!(settings.video.base_resolution, "3360x1890");
        assert_eq!(settings.video.output_resolution, "3360x1890");
    }

    #[test]
    fn load_or_detect_existing_file_ignores_detected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        // Write a settings file with custom resolution
        let mut settings = AppSettings::default();
        settings.video.base_resolution = "2560x1440".to_string();
        settings.save_to(&path).unwrap();
        // Should load from file, not use detected
        let loaded = AppSettings::load_or_detect(&path, Some((3360, 1890)));
        assert_eq!(loaded.video.base_resolution, "2560x1440");
    }

    #[test]
    fn load_or_detect_no_detection_uses_default() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        // No file, no detection — should fall back to defaults
        let settings = AppSettings::load_or_detect(&path, None);
        assert_eq!(settings.video.base_resolution, "1920x1080");
    }

    #[test]
    fn filename_template_basic_expansion() {
        let result = RecordSettings::expand_template("{date}_{time}_{scene}", "Gaming", 1);
        assert!(result.contains("Gaming"));
        assert!(result.contains('_'));
        assert!(!result.contains('{'));
    }

    #[test]
    fn filename_template_scene_sanitization() {
        let result = RecordSettings::expand_template("{scene}", "My Scene/Name", 1);
        assert_eq!(result, "My_Scene_Name");
    }

    #[test]
    fn filename_template_counter() {
        let r1 = RecordSettings::expand_template("{n}", "scene", 1);
        let r3 = RecordSettings::expand_template("{n}", "scene", 3);
        assert_eq!(r1, "1");
        assert_eq!(r3, "3");
    }

    #[test]
    fn record_settings_default() {
        let settings = RecordSettings::default();
        assert_eq!(settings.format, RecordingFormat::Mkv);
        assert_eq!(settings.filename_template, "{date}_{time}_{scene}");
        assert_eq!(settings.quality_preset, QualityPreset::High);
        assert_eq!(settings.fps, 30);
    }

    #[test]
    fn record_settings_roundtrip() {
        let settings = RecordSettings::default();
        let toml_str = toml::to_string(&settings).unwrap();
        let parsed: RecordSettings = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.format, settings.format);
        assert_eq!(parsed.filename_template, settings.filename_template);
    }

    #[test]
    fn transitions_dir_is_inside_config_dir() {
        let td = super::transitions_dir();
        let cd = super::config_dir();
        assert!(td.starts_with(cd));
        assert!(td.ends_with("transitions"));
    }

    #[test]
    fn hotkey_binding_parse_display_roundtrip() {
        let binding = HotkeyBinding::parse("Ctrl+Shift+W");
        assert!(binding.ctrl);
        assert!(binding.shift);
        assert!(!binding.alt);
        assert!(!binding.super_key);
        assert_eq!(binding.key, "W");
        assert_eq!(binding.display(), "Ctrl+Shift+W");
    }

    #[test]
    fn hotkey_binding_parse_single_key() {
        let binding = HotkeyBinding::parse("F5");
        assert!(!binding.ctrl);
        assert!(!binding.shift);
        assert_eq!(binding.key, "F5");
        assert_eq!(binding.display(), "F5");
    }

    #[test]
    fn hotkey_binding_empty_is_unset() {
        let binding = HotkeyBinding::parse("");
        assert!(!binding.is_set());
        assert_eq!(binding.display(), "");
    }

    #[test]
    fn hotkey_binding_serde_roundtrip() {
        // Use TOML serialization since that's how bindings are persisted.
        #[derive(Serialize, Deserialize)]
        struct Wrapper {
            binding: HotkeyBinding,
        }
        let w = Wrapper {
            binding: HotkeyBinding::parse("Ctrl+Shift+W"),
        };
        let toml_str = toml::to_string(&w).unwrap();
        assert!(toml_str.contains("Ctrl+Shift+W"));
        let parsed: Wrapper = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.binding, w.binding);
    }

    #[test]
    fn hotkey_settings_default_has_capture_foreground() {
        let settings = HotkeySettings::default();
        let binding = settings.get("capture_foreground_window");
        assert!(binding.is_some());
        assert_eq!(binding.unwrap().display(), "Ctrl+Shift+W");
    }

    #[test]
    fn hotkey_settings_toml_roundtrip() {
        let settings = AppSettings::default();
        let toml_str = toml::to_string_pretty(&settings).unwrap();
        let parsed: AppSettings = toml::from_str(&toml_str).unwrap();
        assert_eq!(
            parsed
                .hotkeys
                .get("capture_foreground_window")
                .map(|b| b.display()),
            Some("Ctrl+Shift+W".to_string())
        );
    }
}
