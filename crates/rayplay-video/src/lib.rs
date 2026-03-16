//! Video capture, encoding, and decoding for `RayPlay`.
//!
//! Provides screen capture, the `VideoEncoder` / `VideoDecoder` traits, and
//! supporting types for the streaming pipeline. Platform-specific
//! implementations live behind `cfg` guards.
//!
//! # Pipeline overview
//!
//! ```text
//! RawFrame ──► VideoEncoder ──► EncodedPacket ──► FrameChunker ──► NetworkChunk[]
//!                                    │
//!                             (network transport)
//!                                    │
//!                             EncodedPacket ──► VideoDecoder ──► DecodedFrame
//! ```

pub mod capture;
pub mod chunker;
pub mod decoded_frame;
pub mod decoder;
pub mod encoder;
pub mod frame;
pub mod nvenc;
pub mod packet;
pub mod videotoolbox;

#[cfg(target_os = "windows")]
pub mod dxgi_capture;

pub use capture::{CaptureConfig, CaptureError, CapturedFrame, ScreenCapturer, create_capturer};
pub use chunker::{DEFAULT_CHUNK_SIZE, FrameChunker, NetworkChunk};
pub use decoded_frame::{DecodedFrame, PixelFormat};
pub use decoder::VideoDecoder;
pub use encoder::{Bitrate, Codec, EncoderConfig, VideoEncoder, VideoError, create_encoder};
pub use frame::RawFrame;
pub use packet::EncodedPacket;

#[cfg(target_os = "windows")]
pub use nvenc::NvencEncoder;

#[cfg(target_os = "macos")]
pub use videotoolbox::VtDecoder;
