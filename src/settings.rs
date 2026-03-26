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
            .map(|c| if c.is_alphanumeric() || c == '-' { c } else { '_' })
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

/// User-defined hotkey bindings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
#[derive(Default)]
pub struct HotkeySettings {
    pub bindings: HashMap<String, String>,
}

/// Font scale presets that proportionally scale all text sizes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FontScale {
    XS,
    S,
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

impl Default for FontScale {
    fn default() -> Self {
        Self::M
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
}
