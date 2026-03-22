use crate::gstreamer::StreamDestination;
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
}

impl Default for UiSettings {
    fn default() -> Self {
        Self {
            scene_panel_open: true,
            mixer_panel_open: true,
            controls_panel_open: true,
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
}

impl Default for GeneralSettings {
    fn default() -> Self {
        Self {
            language: "en-US".to_string(),
            check_for_updates: true,
            launch_on_startup: false,
            confirm_close_while_streaming: true,
        }
    }
}

/// Stream output configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct StreamSettings {
    pub stream_key: String,
    pub destination: StreamDestination,
    pub encoder: String,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub bitrate_kbps: u32,
}

impl Default for StreamSettings {
    fn default() -> Self {
        Self {
            stream_key: String::new(),
            destination: StreamDestination::Twitch,
            encoder: "x264".to_string(),
            width: 1920,
            height: 1080,
            fps: 30,
            bitrate_kbps: 4500,
        }
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

/// Visual appearance preferences.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppearanceSettings {
    pub accent_color: String,
    pub font_size: f32,
    pub theme: String,
}

impl Default for AppearanceSettings {
    fn default() -> Self {
        Self {
            accent_color: "#7c6cf0".to_string(),
            font_size: 13.0,
            theme: "dark".to_string(),
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
        assert_eq!(parsed.stream.width, 1920);
        assert_eq!(parsed.stream.height, 1080);
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
}
