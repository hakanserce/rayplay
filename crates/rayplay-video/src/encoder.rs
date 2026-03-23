use std::{fmt, str::FromStr};

use thiserror::Error;

use crate::{frame::RawFrame, packet::EncodedPacket, pipeline_mode::PipelineMode};

/// Opaque handle to a GPU-resident texture for zero-copy encoding.
///
/// On Windows, wraps a `*mut ID3D11Texture2D` COM pointer transferred from
/// [`ZeroCopyCapturer::acquire_texture`].  The resource must remain valid
/// (i.e. the DXGI frame must not be released) until the encoder has consumed
/// it and [`ZeroCopyCapturer::release_frame`] has been called.
#[repr(transparent)]
pub struct GpuTextureHandle(pub *mut std::ffi::c_void);

// SAFETY: only accessed on the single encoding thread that owns the D3D11 context.
unsafe impl Send for GpuTextureHandle {}

impl fmt::Debug for GpuTextureHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "GpuTextureHandle({:p})", self.0)
    }
}

/// Input for a video encoder — CPU pixels or GPU-resident texture.
pub enum EncoderInput<'a> {
    /// CPU-accessible raw pixel data (ADR-001 Option A fallback).
    Cpu(&'a RawFrame),
    /// Opaque GPU texture handle (zero-copy ADR-001 Option B).
    GpuTexture {
        handle: GpuTextureHandle,
        width: u32,
        height: u32,
        timestamp_us: u64,
    },
}

// SAFETY: `GpuTexture::handle` is only dereferenced on the encoding thread that
// owns the D3D11 device context — the same single-thread model as `DxgiCapture`.
unsafe impl Send for EncoderInput<'_> {}

impl fmt::Debug for EncoderInput<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cpu(frame) => f.debug_tuple("Cpu").field(frame).finish(),
            Self::GpuTexture {
                handle,
                width,
                height,
                timestamp_us,
            } => f
                .debug_struct("GpuTexture")
                .field("handle", handle)
                .field("width", width)
                .field("height", height)
                .field("timestamp_us", timestamp_us)
                .finish(),
        }
    }
}

/// Supported video codecs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Codec {
    /// H.265 / HEVC — default codec, hardware-accelerated on Nvidia RTX 2060+.
    Hevc,
    /// H.264 / AVC — widely supported codec, hardware-accelerated on most GPUs.
    H264,
}

impl fmt::Display for Codec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Hevc => write!(f, "hevc"),
            Self::H264 => write!(f, "h264"),
        }
    }
}

impl FromStr for Codec {
    type Err = VideoError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "hevc" => Ok(Self::Hevc),
            "h264" => Ok(Self::H264),
            _ => Err(VideoError::UnsupportedCodec {
                codec: s.to_string(),
            }),
        }
    }
}

/// Encoder bitrate setting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Bitrate {
    /// Automatically computed from resolution and frame rate.
    Auto,
    /// Fixed bitrate in megabits per second.
    Mbps(u32),
}

impl Bitrate {
    /// Resolves the bitrate to bits per second for the given stream parameters.
    #[must_use]
    pub fn resolve(&self, codec: Codec, width: u32, height: u32, fps: u32) -> u32 {
        match self {
            Self::Auto => compute_auto_bitrate(codec, width, height, fps),
            Self::Mbps(mbps) => mbps.saturating_mul(1_000_000),
        }
    }
}

/// Computes a codec-aware bitrate heuristic from resolution and frame rate.
///
/// For HEVC: targets approximately 6 Mbps at 1080p60 and 25 Mbps at 4K60.
/// For H.264: targets approximately 8 Mbps at 1080p60 and 33 Mbps at 4K60.
/// Clamped between 1 Mbps and 100 Mbps for both codecs.
///
/// Formula: `pixels * fps / COMPRESSION_FACTOR`, where the factor depends
/// on the codec's compression efficiency.
fn compute_auto_bitrate(codec: Codec, width: u32, height: u32, fps: u32) -> u32 {
    let compression_factor = match codec {
        Codec::Hevc => 20, // HEVC encodes ~20× more efficiently than raw pixel throughput
        Codec::H264 => 15, // H.264 encodes ~15× more efficiently than raw pixel throughput
    };
    let pixels = u64::from(width) * u64::from(height);
    let bps = pixels * u64::from(fps) / compression_factor;
    bps.clamp(1_000_000, 100_000_000) as u32
}

/// Configuration for a video encoder session.
#[derive(Debug, Clone)]
pub struct EncoderConfig {
    /// Video codec to use for encoding.
    pub codec: Codec,
    /// Frame width in pixels.
    pub width: u32,
    /// Frame height in pixels.
    pub height: u32,
    /// Target frame rate.
    pub fps: u32,
    /// Bitrate setting (auto or fixed).
    pub bitrate: Bitrate,
}

impl EncoderConfig {
    /// Creates a new config with HEVC codec and auto bitrate.
    #[must_use]
    pub fn new(width: u32, height: u32, fps: u32) -> Self {
        Self {
            codec: Codec::Hevc,
            width,
            height,
            fps,
            bitrate: Bitrate::Auto,
        }
    }

    /// Creates a new config with the specified codec and auto bitrate.
    #[must_use]
    pub fn with_codec(width: u32, height: u32, fps: u32, codec: Codec) -> Self {
        Self {
            codec,
            width,
            height,
            fps,
            bitrate: Bitrate::Auto,
        }
    }

    /// Overrides the bitrate setting.
    #[must_use]
    pub fn with_bitrate(mut self, bitrate: Bitrate) -> Self {
        self.bitrate = bitrate;
        self
    }

    /// Returns the resolved bitrate in bits per second.
    #[must_use]
    pub fn resolved_bitrate(&self) -> u32 {
        self.bitrate
            .resolve(self.codec, self.width, self.height, self.fps)
    }
}

/// Errors produced by video encoder and decoder operations.
#[derive(Debug, Error)]
pub enum VideoError {
    #[error("encoder session not initialized")]
    NotInitialized,

    #[error("unsupported codec: {codec}")]
    UnsupportedCodec { codec: String },

    #[error("invalid frame dimensions: {width}x{height}")]
    InvalidDimensions { width: u32, height: u32 },

    #[error("encoding failed: {reason}")]
    EncodingFailed { reason: String },

    #[error("decoding failed: {reason}")]
    DecodingFailed { reason: String },

    #[error("corrupt packet: {reason}")]
    CorruptPacket { reason: String },

    #[error("video encoding is not supported on this platform")]
    UnsupportedPlatform,
}

/// Returns the platform-appropriate hardware encoder.
///
/// On Windows, returns an [`NvencEncoder`](crate::nvenc::NvencEncoder) backed
/// by NVENC.  On other platforms returns [`VideoError::UnsupportedPlatform`].
///
/// # Errors
///
/// Returns [`VideoError::UnsupportedPlatform`] on non-Windows, or
/// [`VideoError::EncodingFailed`] if the NVENC session cannot be opened.
// On Windows the config is consumed by NvencEncoder::new; the non-Windows branch
// must accept the same signature.
#[allow(clippy::needless_pass_by_value)]
pub fn create_encoder(
    config: EncoderConfig,
    mode: PipelineMode,
) -> Result<Box<dyn VideoEncoder>, VideoError> {
    if mode == PipelineMode::Software {
        return create_software_encoder(config);
    }

    #[cfg(target_os = "windows")]
    {
        use crate::nvenc::NvencEncoder;
        NvencEncoder::new(config).map(|e| Box::new(e) as Box<dyn VideoEncoder>)
    }
    #[cfg(not(target_os = "windows"))]
    {
        create_software_encoder(config)
    }
}

#[allow(clippy::needless_pass_by_value)]
fn create_software_encoder(config: EncoderConfig) -> Result<Box<dyn VideoEncoder>, VideoError> {
    #[cfg(feature = "ffmpeg-fallback")]
    {
        use crate::ffmpeg_enc::FfmpegEncoder;
        FfmpegEncoder::new(config).map(|e| Box::new(e) as Box<dyn VideoEncoder>)
    }
    #[cfg(all(feature = "fallback", not(feature = "ffmpeg-fallback")))]
    {
        use crate::openh264_enc::OpenH264Encoder;
        let fallback_config =
            EncoderConfig::with_codec(config.width, config.height, config.fps, Codec::H264)
                .with_bitrate(config.bitrate);
        OpenH264Encoder::new(fallback_config).map(|e| Box::new(e) as Box<dyn VideoEncoder>)
    }
    #[cfg(not(any(feature = "fallback", feature = "ffmpeg-fallback")))]
    {
        let _ = config;
        Err(VideoError::UnsupportedPlatform)
    }
}

/// Trait for hardware or software video encoders.
///
/// Implementations must be `Send` so they can be driven from a dedicated
/// encoding thread. The `encode` → `flush` lifecycle mirrors NVENC's
/// asynchronous pipeline.
pub trait VideoEncoder: Send {
    /// Submits a raw frame for encoding.
    ///
    /// Returns `Ok(Some(packet))` when an encoded packet is immediately
    /// available, `Ok(None)` when the encoder is buffering, or an error.
    ///
    /// # Errors
    ///
    /// Returns `VideoError::InvalidDimensions` if the frame size does not
    /// match the encoder configuration, or `VideoError::EncodingFailed` on
    /// an internal encoder error.
    fn encode(&mut self, input: EncoderInput<'_>) -> Result<Option<EncodedPacket>, VideoError>;

    /// Flushes any buffered frames and returns all remaining encoded packets.
    ///
    /// Call this at end-of-stream or before reconfiguring the encoder.
    ///
    /// # Errors
    ///
    /// Returns `VideoError::EncodingFailed` if flushing fails.
    fn flush(&mut self) -> Result<Vec<EncodedPacket>, VideoError>;

    /// Returns the active encoder configuration.
    fn config(&self) -> &EncoderConfig;
}

#[cfg(test)]
mod tests;
