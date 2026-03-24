/// Errors reported from the GStreamer thread to the main thread.
#[derive(Debug, Clone)]
pub enum GstError {
    /// Screen/window/camera capture failed.
    CaptureFailure { message: String },
    /// H.264 encoding failed.
    EncodeFailure { message: String },
    /// RTMP connection was lost during streaming.
    #[allow(dead_code)]
    StreamConnectionLost { message: String },
    /// GStreamer pipeline state transition failed.
    #[allow(dead_code)]
    PipelineStateChange {
        from: String,
        to: String,
        message: String,
    },
    /// macOS Screen Recording permission denied.
    #[allow(dead_code)]
    PermissionDenied { message: String },
    /// Audio capture device failed (mic unplugged, permission denied).
    AudioCaptureFailure { message: String },
}

impl std::fmt::Display for GstError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CaptureFailure { message } => write!(f, "Capture failed: {message}"),
            Self::EncodeFailure { message } => write!(f, "Encode failed: {message}"),
            Self::StreamConnectionLost { message } => write!(f, "Stream lost: {message}"),
            Self::PipelineStateChange { from, to, message } => {
                write!(f, "Pipeline state {from} -> {to}: {message}")
            }
            Self::PermissionDenied { message } => write!(f, "Permission denied: {message}"),
            Self::AudioCaptureFailure { message } => {
                write!(f, "Audio capture failed: {message}")
            }
        }
    }
}

impl std::error::Error for GstError {}
