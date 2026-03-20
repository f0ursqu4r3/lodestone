use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use crate::obs::StreamDestination;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub active_profile: String,
    pub ui: UiSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiSettings {
    pub scene_panel_open: bool,
    pub mixer_panel_open: bool,
    pub controls_panel_open: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            active_profile: "Default".to_string(),
            ui: UiSettings {
                scene_panel_open: true,
                mixer_panel_open: true,
                controls_panel_open: true,
            },
        }
    }
}

impl AppSettings {
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileSettings {
    pub name: String,
    pub destination: StreamDestination,
    pub stream_key: String,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub bitrate_kbps: u32,
}

impl Default for ProfileSettings {
    fn default() -> Self {
        Self {
            name: "Default".to_string(),
            destination: StreamDestination::Twitch,
            stream_key: String::new(),
            width: 1920,
            height: 1080,
            fps: 30,
            bitrate_kbps: 4500,
        }
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

pub fn profile_path(name: &str) -> PathBuf {
    config_dir().join("profiles").join(format!("{name}.toml"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn default_settings_roundtrip() {
        let settings = AppSettings::default();
        let toml_str = toml::to_string_pretty(&settings).unwrap();
        let parsed: AppSettings = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.active_profile, settings.active_profile);
    }

    #[test]
    fn profile_roundtrip() {
        let profile = ProfileSettings {
            name: "Streaming".to_string(),
            destination: StreamDestination::Twitch,
            stream_key: "live_abc123".to_string(),
            width: 1920,
            height: 1080,
            fps: 30,
            bitrate_kbps: 4500,
        };
        let toml_str = toml::to_string_pretty(&profile).unwrap();
        let parsed: ProfileSettings = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.name, "Streaming");
        assert_eq!(parsed.bitrate_kbps, 4500);
    }

    #[test]
    fn load_nonexistent_returns_default() {
        let settings = AppSettings::load_from(Path::new("/nonexistent/path/settings.toml"));
        assert_eq!(settings.active_profile, "Default");
    }

    #[test]
    fn save_and_load_roundtrip() {
        let mut file = NamedTempFile::new().unwrap();
        let settings = AppSettings::default();
        let toml_str = toml::to_string_pretty(&settings).unwrap();
        file.write_all(toml_str.as_bytes()).unwrap();
        let loaded = AppSettings::load_from(file.path());
        assert_eq!(loaded.active_profile, settings.active_profile);
    }
}
