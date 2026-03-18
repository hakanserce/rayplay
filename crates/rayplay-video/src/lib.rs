//! Video capture, encoding, decoding, and rendering for `RayPlay`.
//!
//! Provides screen capture, the `VideoEncoder` / `VideoDecoder` / `Renderer`
//! traits, and supporting types for the streaming pipeline.  Platform-specific
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
//!                                                                      │
//!                                                                 WgpuRenderer
//!                                                                      │
//!                                                               display window
//! ```

pub mod capture;
pub mod chunker;
pub mod decoded_frame;
pub mod decoder;
pub mod encoder;
pub mod frame;
pub mod nvenc;
pub mod packet;
pub mod render_window;
pub mod renderer;
pub mod videotoolbox;
pub mod wgpu_renderer;
mod wgpu_surface;

#[cfg(target_os = "windows")]
pub mod d3d11_device;
#[cfg(target_os = "windows")]
pub mod dxgi_capture;

pub use capture::{CaptureConfig, CaptureError, CapturedFrame, ScreenCapturer, create_capturer};
#[cfg(target_os = "windows")]
pub use capture::{CapturedTexture, ZeroCopyCapturer};
pub use chunker::{DEFAULT_CHUNK_SIZE, FrameChunker, NetworkChunk};
#[cfg(target_os = "windows")]
pub use d3d11_device::SharedD3D11Device;
pub use decoded_frame::{DecodedFrame, PixelFormat};
pub use decoder::{VideoDecoder, create_decoder};
pub use encoder::{
    Bitrate, Codec, EncoderConfig, EncoderInput, GpuTextureHandle, VideoEncoder, VideoError,
    create_encoder,
};
pub use frame::RawFrame;
pub use packet::EncodedPacket;
pub use render_window::RenderWindow;
pub use renderer::{RenderError, Renderer};
pub use wgpu_renderer::WgpuRenderer;

#[cfg(target_os = "windows")]
pub use nvenc::NvencEncoder;

#[cfg(target_os = "macos")]
pub use videotoolbox::VtDecoder;
