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
//! ```

pub mod capture;
pub mod chunker;
pub mod encoder;
pub mod frame;
pub mod nvenc;
pub mod packet;

#[cfg(target_os = "windows")]
pub mod dxgi_capture;

pub use capture::{CaptureConfig, CaptureError, CapturedFrame, ScreenCapturer, create_capturer};
pub use chunker::{DEFAULT_CHUNK_SIZE, FrameChunker, NetworkChunk};
pub use encoder::{Bitrate, Codec, EncoderConfig, VideoEncoder, VideoError};
pub use frame::RawFrame;
pub use packet::EncodedPacket;

#[cfg(target_os = "windows")]
pub use nvenc::NvencEncoder;
