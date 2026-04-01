//! UI event and action types for communication between UI and network threads.

use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

/// Events sent from the network thread to the UI thread.
///
/// The UI drains these events each frame to update its state.
#[derive(Debug, Clone)]
pub enum UiEvent {
    /// A PIN is required for pairing with the host.
    PinRequired {
        /// Host name for display purposes.
        host_name: String,
    },
    /// Successfully connected to a host and streaming has started.
    Connected {
        /// Host name for display purposes.
        host_name: String,
    },
    /// Currently reconnecting to the host due to connection loss.
    Reconnecting {
        /// Host name for display purposes.
        host_name: String,
        /// Seconds remaining before giving up.
        countdown_secs: u32,
    },
    /// Disconnected from the host (either intentionally or due to error).
    Disconnected {
        /// Optional error message if disconnection was due to an error.
        error: Option<String>,
    },
    /// Stream statistics update.
    StreamStats {
        /// Current resolution in "`WIDTHxHEIGHT`" format.
        resolution: String,
        /// Current frame rate.
        fps: u32,
        /// Current codec.
        codec: String,
        /// Current latency in milliseconds.
        latency_ms: u32,
    },
    /// Result of a pairing attempt.
    PairingResult {
        /// True if pairing was successful, false otherwise.
        success: bool,
        /// Error message if pairing failed.
        error: Option<String>,
    },
}

/// Actions sent from the UI thread to the network thread.
///
/// The network thread processes these to initiate connections, submit PINs, etc.
#[derive(Debug, Clone)]
pub enum UiAction {
    /// Connect to a specific host.
    Connect {
        /// Host address to connect to.
        host: SocketAddr,
        /// Port number (usually 7860).
        port: u16,
        /// Whether this host requires pairing.
        needs_pairing: bool,
    },
    /// Submit a PIN for the current pairing attempt.
    SubmitPin(String),
    /// Disconnect from the current host.
    Disconnect,
    /// Toggle fullscreen mode.
    ToggleFullscreen,
}

/// Settings for video streaming parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoSettings {
    /// Target resolution.
    pub resolution: Resolution,
    /// Target frame rate.
    pub fps: u32,
    /// Target bitrate in Mbps.
    pub bitrate: u32,
}

/// Video resolution options.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Resolution {
    /// 1920x1080
    Hd1080,
    /// 2560x1440
    Qhd1440,
    /// 3840x2160
    Uhd4K,
    /// Match the host's native resolution
    MatchHost,
}

impl Resolution {
    /// Returns the (width, height) for this resolution, or None for `MatchHost`.
    #[must_use]
    pub fn dimensions(self) -> Option<(u32, u32)> {
        match self {
            Self::Hd1080 => Some((1920, 1080)),
            Self::Qhd1440 => Some((2560, 1440)),
            Self::Uhd4K => Some((3840, 2160)),
            Self::MatchHost => None,
        }
    }

    /// Returns a display string for this resolution.
    #[must_use]
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Hd1080 => "1920x1080",
            Self::Qhd1440 => "2560x1440",
            Self::Uhd4K => "3840x2160",
            Self::MatchHost => "Match Host",
        }
    }
}

impl Default for VideoSettings {
    fn default() -> Self {
        Self {
            resolution: Resolution::Hd1080,
            fps: 60,
            bitrate: 50,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Resolution ────────────────────────────────────────────────────

    #[test]
    fn test_resolution_dimensions_1080p() {
        assert_eq!(Resolution::Hd1080.dimensions(), Some((1920, 1080)));
    }

    #[test]
    fn test_resolution_dimensions_1440p() {
        assert_eq!(Resolution::Qhd1440.dimensions(), Some((2560, 1440)));
    }

    #[test]
    fn test_resolution_dimensions_4k() {
        assert_eq!(Resolution::Uhd4K.dimensions(), Some((3840, 2160)));
    }

    #[test]
    fn test_resolution_dimensions_match_host_returns_none() {
        assert_eq!(Resolution::MatchHost.dimensions(), None);
    }

    #[test]
    fn test_resolution_display_names() {
        assert_eq!(Resolution::Hd1080.display_name(), "1920x1080");
        assert_eq!(Resolution::Qhd1440.display_name(), "2560x1440");
        assert_eq!(Resolution::Uhd4K.display_name(), "3840x2160");
        assert_eq!(Resolution::MatchHost.display_name(), "Match Host");
    }

    #[test]
    fn test_resolution_equality() {
        assert_eq!(Resolution::Hd1080, Resolution::Hd1080);
        assert_ne!(Resolution::Hd1080, Resolution::Qhd1440);
    }

    // ── VideoSettings ─────────────────────────────────────────────────

    #[test]
    fn test_video_settings_default() {
        let settings = VideoSettings::default();
        assert_eq!(settings.resolution, Resolution::Hd1080);
        assert_eq!(settings.fps, 60);
        assert_eq!(settings.bitrate, 50);
    }

    #[test]
    fn test_video_settings_serialization_roundtrip() {
        let settings = VideoSettings {
            resolution: Resolution::Uhd4K,
            fps: 120,
            bitrate: 80,
        };
        let json = serde_json::to_string(&settings).expect("serialize");
        let deserialized: VideoSettings = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(deserialized.resolution, Resolution::Uhd4K);
        assert_eq!(deserialized.fps, 120);
        assert_eq!(deserialized.bitrate, 80);
    }

    // ── Resolution serialization ──────────────────────────────────────

    #[test]
    fn test_resolution_serialization_roundtrip() {
        for res in [
            Resolution::Hd1080,
            Resolution::Qhd1440,
            Resolution::Uhd4K,
            Resolution::MatchHost,
        ] {
            let json = serde_json::to_string(&res).expect("serialize");
            let deserialized: Resolution = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(res, deserialized);
        }
    }
}
