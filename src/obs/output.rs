use serde::{Deserialize, Serialize};

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamConfig {
    pub destination: StreamDestination,
    pub stream_key: String,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum StreamDestination {
    Twitch,
    YouTube,
    CustomRtmp { url: String },
}

impl StreamDestination {
    #[allow(dead_code)]
    pub fn rtmp_url(&self) -> &str {
        match self {
            Self::Twitch => "rtmp://live.twitch.tv/app",
            Self::YouTube => "rtmp://a.rtmp.youtube.com/live2",
            Self::CustomRtmp { url } => url,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn twitch_rtmp_url() {
        assert_eq!(
            StreamDestination::Twitch.rtmp_url(),
            "rtmp://live.twitch.tv/app"
        );
    }

    #[test]
    fn custom_rtmp_url() {
        let dest = StreamDestination::CustomRtmp {
            url: "rtmp://my.server/live".to_string(),
        };
        assert_eq!(dest.rtmp_url(), "rtmp://my.server/live");
    }
}
