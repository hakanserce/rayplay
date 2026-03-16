/// NVENC-based HEVC hardware encoder for Nvidia GPUs (Windows only).
///
/// This module is compiled only on Windows. It provides an implementation
/// of the `VideoEncoder` trait that drives Nvidia's NVENC hardware encoder
/// via the Video Codec SDK, encoding frames directly from DXGI-captured
/// textures with zero copies on the input path (ADR-001, Option B).
///
/// # Setup
///
/// The NVENC SDK headers must be present at build time. Download from the
/// Nvidia Developer Program and set the `NVENC_SDK` environment variable
/// to the SDK root before building (see `build.rs`).
///
/// # Architecture
///
/// ```text
/// DXGI texture ──► NvEncRegisterResource ──► NvEncEncodePicture
///                                                     │
///                                              NvEncLockBitstream
///                                                     │
///                                           EncodedPacket (NAL units)
///                                                     │
///                                            FrameChunker → UDP
/// ```
#[cfg(target_os = "windows")]
mod windows {
    use crate::{
        encoder::{EncoderConfig, VideoEncoder, VideoError},
        frame::RawFrame,
        packet::EncodedPacket,
    };

    /// NVENC hardware encoder session.
    ///
    /// Wraps an `NV_ENCODE_API_FUNCTION_LIST` session and manages the
    /// encoder lifetime. The session is destroyed on `Drop`.
    ///
    /// # Thread Safety
    ///
    /// `NvencEncoder` is `Send` but not `Sync` — each encoding thread
    /// must own its own session.
    pub struct NvencEncoder {
        config: EncoderConfig,
        // TODO(UC-002): Add NVENC session handle once SDK bindings land.
        // session: nvenc_sys::NvEncodeAPICreateInstance,
        // input_buffer: nvenc_sys::NV_ENC_INPUT_PTR,
        // output_buffer: nvenc_sys::NV_ENC_OUTPUT_PTR,
    }

    impl NvencEncoder {
        /// Opens a new NVENC encode session and initialises it for HEVC.
        ///
        /// # Errors
        ///
        /// - `VideoError::UnsupportedCodec` — GPU does not support HEVC NVENC.
        /// - `VideoError::EncodingFailed`   — Session creation or init failed.
        pub fn new(config: EncoderConfig) -> Result<Self, VideoError> {
            // TODO(UC-002): Implement NVENC initialisation sequence:
            //
            //   1. NvEncOpenEncodeSessionEx  — open D3D11 device session
            //   2. NvEncGetEncodeGUIDCount / NvEncGetEncodeGUIDs
            //      — enumerate supported codecs, verify HEVC availability
            //   3. NvEncGetEncodeProfileGUIDCount / NvEncGetEncodeProfileGUIDs
            //      — select Main10 profile for HDR support
            //   4. NvEncGetInputFormatCount / NvEncGetInputFormats
            //      — confirm NV12 / ARGB input support
            //   5. NvEncInitializeEncoder (NV_ENC_INITIALIZE_PARAMS)
            //      — set resolution, fps, bitrate, GOP structure
            //   6. NvEncCreateInputBuffer / NvEncRegisterResource (DXGI path)
            //   7. NvEncCreateBitstreamBuffer — output ring buffer
            //
            // For now, return a placeholder session.
            tracing::info!(
                width = config.width,
                height = config.height,
                fps = config.fps,
                bitrate_bps = config.resolved_bitrate(),
                "NvencEncoder::new — session placeholder (SDK integration pending)"
            );
            Ok(Self { config })
        }
    }

    impl VideoEncoder for NvencEncoder {
        fn encode(&mut self, frame: &RawFrame) -> Result<Option<EncodedPacket>, VideoError> {
            if frame.width != self.config.width || frame.height != self.config.height {
                return Err(VideoError::InvalidDimensions {
                    width: frame.width,
                    height: frame.height,
                });
            }

            // TODO(UC-002): Submit frame to NVENC:
            //
            //   1. Map DXGI texture via NvEncMapInputResource (zero-copy)
            //      or lock input buffer and copy for the CPU-copy fallback.
            //   2. NvEncEncodePicture — submit to encoder pipeline.
            //   3. Poll / wait on output event object.
            //   4. NvEncLockBitstream — get pointer to encoded NAL data.
            //   5. Copy NAL bytes into EncodedPacket::data.
            //   6. NvEncUnlockBitstream — release output buffer.
            //
            // Placeholder: signal that the session isn't connected yet.
            Err(VideoError::EncodingFailed {
                reason: "NVENC SDK integration pending".to_string(),
            })
        }

        fn flush(&mut self) -> Result<Vec<EncodedPacket>, VideoError> {
            // TODO(UC-002): Send EOS picture to drain buffered frames:
            //   NvEncEncodePicture with NV_ENC_PIC_FLAG_EOS
            Ok(vec![])
        }

        fn config(&self) -> &EncoderConfig {
            &self.config
        }
    }
}

#[cfg(target_os = "windows")]
pub use windows::NvencEncoder;
