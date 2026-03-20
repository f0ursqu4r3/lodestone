use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncoderConfig {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub bitrate_kbps: u32,
}

impl Default for EncoderConfig {
    fn default() -> Self {
        Self {
            width: 1920,
            height: 1080,
            fps: 30,
            bitrate_kbps: 4500,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_encoder_config() {
        let config = EncoderConfig::default();
        assert_eq!(config.width, 1920);
        assert_eq!(config.bitrate_kbps, 4500);
    }
}
