/// VideoToolbox-based HEVC hardware decoder for Apple Silicon (macOS only).
///
/// This module is compiled only on macOS. It provides an implementation of
/// the `VideoDecoder` trait that drives Apple's `VideoToolbox` framework via
/// direct framework bindings (ADR-004, Option B), decoding HEVC frames into
/// `CVPixelBuffer`s backed by `IOSurface` for zero-copy GPU rendering (ADR-005).
///
/// # Architecture
///
/// ```text
/// EncodedPacket (Annex B) ──► Annex B → HVCC conversion
///                                  │
///                          CMBlockBuffer + CMSampleBuffer
///                                  │
///                    VTDecompressionSessionDecodeFrame
///                                  │
///                          CVPixelBuffer (IOSurface)
///                                  │
///                         DecodedFrame (hardware-backed)
///                                  │
///                        wgpu IOSurface external texture
/// ```
///
/// # Initialization
///
/// The `VTDecompressionSession` is initialized lazily on the first keyframe.
/// Non-keyframe packets received before the first keyframe are rejected with
/// `VideoError::DecodingFailed` — the caller must wait for a keyframe.
///
/// # TODO (UC-004 full implementation)
///
/// The session initialization and frame decode steps are stubs pending
/// the full `VideoToolbox` FFI integration. See inline `TODO(UC-004)` comments
/// for the exact API sequence required.
///
/// Required frameworks: `VideoToolbox`, `CoreMedia`, `CoreVideo`.
#[cfg(target_os = "macos")]
mod macos {
    use std::ffi::c_void;

    use crate::{
        decoded_frame::DecodedFrame,
        decoder::VideoDecoder,
        encoder::{Codec, VideoError},
        packet::EncodedPacket,
    };

    // ── VideoToolbox / CoreMedia opaque handle types ───────────────────────────
    //
    // All VideoToolbox and CoreMedia objects are reference-counted Core Foundation
    // types exposed as opaque pointers. We store them as `*mut c_void` and call
    // CFRelease / session-specific invalidation on Drop.
    //
    // TODO(UC-004): Replace with bindgen-generated `vt_sys` / `core_media_sys`
    // crate bindings once the FFI layer is introduced. Required extern blocks:
    //
    //   #[link(name = "VideoToolbox", kind = "framework")]
    //   extern "C" {
    //       fn VTDecompressionSessionCreate(
    //           allocator: *const c_void,
    //           video_format_description: *mut c_void,
    //           video_decoder_specification: *const c_void,
    //           destination_image_buffer_attributes: *const c_void,
    //           output_callback: *const VtOutputCallbackRecord,
    //           session_out: *mut *mut c_void,
    //       ) -> i32;
    //       fn VTDecompressionSessionDecodeFrame(
    //           session: *mut c_void,
    //           sample_buffer: *mut c_void,
    //           decode_flags: u32,
    //           source_frame_ref_con: *mut c_void,
    //           info_flags_out: *mut u32,
    //       ) -> i32;
    //       fn VTDecompressionSessionWaitForAsynchronousFrames(
    //           session: *mut c_void,
    //       ) -> i32;
    //       fn VTDecompressionSessionInvalidate(session: *mut c_void);
    //   }
    //
    //   #[link(name = "CoreMedia", kind = "framework")]
    //   extern "C" {
    //       fn CMVideoFormatDescriptionCreateFromHEVCParameterSets(
    //           allocator: *const c_void,
    //           parameter_set_count: usize,
    //           parameter_set_pointers: *const *const u8,
    //           parameter_set_sizes: *const usize,
    //           nal_unit_header_length: i32,
    //           extensions: *const c_void,
    //           format_description_out: *mut *mut c_void,
    //       ) -> i32;
    //       fn CMBlockBufferCreateWithMemoryBlock(...) -> i32;
    //       fn CMSampleBufferCreateReady(...) -> i32;
    //       fn CFRelease(cf: *const c_void);
    //   }
    //
    //   #[link(name = "CoreVideo", kind = "framework")]
    //   extern "C" {
    //       fn CVPixelBufferGetWidth(pixel_buffer: *mut c_void) -> usize;
    //       fn CVPixelBufferGetHeight(pixel_buffer: *mut c_void) -> usize;
    //       fn CVPixelBufferGetBytesPerRowOfPlane(
    //           pixel_buffer: *mut c_void, plane_index: usize,
    //       ) -> usize;
    //       fn CVPixelBufferLockBaseAddress(
    //           pixel_buffer: *mut c_void, lock_flags: u64,
    //       ) -> i32;
    //       fn CVPixelBufferUnlockBaseAddress(
    //           pixel_buffer: *mut c_void, unlock_flags: u64,
    //       ) -> i32;
    //       fn CVPixelBufferGetBaseAddressOfPlane(
    //           pixel_buffer: *mut c_void, plane_index: usize,
    //       ) -> *mut c_void;
    //   }

    /// HEVC hardware decoder session backed by Apple's `VideoToolbox` framework.
    ///
    /// The session is initialized lazily on the first keyframe. Until then,
    /// `session` and `format_description` are null.
    ///
    /// # Thread Safety
    ///
    /// `VtDecoder` is `Send` but not `Sync` — each decode thread must own its
    /// own session. The raw pointers are only accessed through `&mut self`
    /// methods, guaranteeing exclusive access.
    pub struct VtDecoder {
        /// Active `VTDecompressionSessionRef`. Null until a keyframe is received.
        session: *mut c_void,
        /// `CMVideoFormatDescriptionRef` derived from the first keyframe's
        /// parameter sets (VPS/SPS/PPS). Null until initialized.
        format_description: *mut c_void,
    }

    // SAFETY: VTDecompressionSession is safe to move between threads provided
    // it is accessed from at most one thread at a time, which is guaranteed by
    // the `&mut self` receiver on all methods.
    unsafe impl Send for VtDecoder {}

    impl VtDecoder {
        /// Creates a new decoder. The `VideoToolbox` session is not started until
        /// the first keyframe is received via `decode`.
        ///
        /// # Errors
        ///
        /// Currently infallible; returns `Err` in future when pre-flight
        /// hardware capability checks are added.
        pub fn new() -> Result<Self, VideoError> {
            tracing::debug!("VtDecoder::new — session deferred until first keyframe");
            Ok(Self {
                session: std::ptr::null_mut(),
                format_description: std::ptr::null_mut(),
            })
        }

        /// Returns `true` when both the `VTDecompressionSession` and its
        /// `CMVideoFormatDescription` have been successfully initialized.
        fn is_session_ready(&self) -> bool {
            !self.session.is_null() && !self.format_description.is_null()
        }
    }

    impl VideoDecoder for VtDecoder {
        fn decode(&mut self, packet: &EncodedPacket) -> Result<Option<DecodedFrame>, VideoError> {
            if !packet.is_keyframe && !self.is_session_ready() {
                return Err(VideoError::DecodingFailed {
                    reason: "waiting for keyframe to initialize VTDecompressionSession".to_string(),
                });
            }

            // TODO(UC-004): Initialize VTDecompressionSession from keyframe (when null):
            //   1. Parse HEVC Annex B bitstream to extract VPS/SPS/PPS NAL units.
            //   2. CMVideoFormatDescriptionCreateFromHEVCParameterSets with NAL unit
            //      header length = 4.
            //   3. Build destination_image_buffer_attributes CFDictionary requesting
            //      kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange (NV12) and
            //      kCVPixelBufferIOSurfacePropertiesKey for IOSurface-backed output.
            //   4. Set kVTVideoDecoderSpecification_RequireHardwareAcceleratedVideoDecoder
            //      to kCFBooleanTrue in the decoder specification dictionary.
            //   5. Populate VTDecompressionOutputCallbackRecord with a C callback that
            //      writes the CVImageBufferRef into a caller-supplied slot via
            //      sourceFrameRefCon.
            //   6. VTDecompressionSessionCreate → store in self.session.
            //   7. Store format_description for CMSampleBuffer construction.

            // TODO(UC-004): Decode frame with active session:
            //   1. Convert Annex B packet (0x00_00_00_01 start codes) to HVCC
            //      length-prefixed format (4-byte big-endian NAL unit length).
            //   2. CMBlockBufferCreateWithMemoryBlock wrapping the converted bytes.
            //   3. CMSampleBufferCreateReady with the block buffer, format description,
            //      and CMSampleTimingInfo (presentation timestamp from packet.timestamp_us).
            //   4. VTDecompressionSessionDecodeFrame with flags = 0 (synchronous).
            //   5. VTDecompressionSessionWaitForAsynchronousFrames to drain the session.
            //   6. Retrieve CVImageBufferRef from the callback slot.
            //   7. Return DecodedFrame::new_hardware (IOSurface path) or
            //      DecodedFrame::new_cpu after CVPixelBufferLockBaseAddress copy
            //      (fallback if IOSurface import is unavailable).

            Err(VideoError::DecodingFailed {
                reason: "VideoToolbox session initialization pending (UC-004)".to_string(),
            })
        }

        fn flush(&mut self) -> Result<Vec<DecodedFrame>, VideoError> {
            // TODO(UC-004): When session is active, call
            //   VTDecompressionSessionWaitForAsynchronousFrames(self.session)
            // to drain all in-flight frames before returning.
            Ok(vec![])
        }

        fn codec(&self) -> Codec {
            Codec::Hevc
        }
    }

    impl Drop for VtDecoder {
        fn drop(&mut self) {
            // TODO(UC-004): Release VideoToolbox resources:
            //   if !self.session.is_null() {
            //       VTDecompressionSessionInvalidate(self.session);
            //       CFRelease(self.session as *const c_void);
            //   }
            //   if !self.format_description.is_null() {
            //       CFRelease(self.format_description as *const c_void);
            //   }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::packet::EncodedPacket;

        #[test]
        fn test_vt_decoder_new_returns_ok() {
            let dec = VtDecoder::new().unwrap();
            assert_eq!(dec.codec(), Codec::Hevc);
        }

        #[test]
        fn test_vt_decoder_decode_non_keyframe_without_session_returns_error() {
            let mut dec = VtDecoder::new().unwrap();
            let packet = EncodedPacket::new(vec![0u8; 64], false, 0, 16_667);
            let err = dec.decode(&packet).unwrap_err();
            assert!(matches!(err, VideoError::DecodingFailed { .. }));
            let msg = err.to_string();
            assert!(msg.contains("keyframe"));
        }

        #[test]
        fn test_vt_decoder_decode_keyframe_returns_decoding_failed_pending() {
            let mut dec = VtDecoder::new().unwrap();
            let packet = EncodedPacket::new(vec![0u8; 64], true, 1000, 16_667);
            let err = dec.decode(&packet).unwrap_err();
            assert!(matches!(err, VideoError::DecodingFailed { .. }));
        }

        #[test]
        fn test_vt_decoder_flush_returns_empty_without_session() {
            let mut dec = VtDecoder::new().unwrap();
            let frames = dec.flush().unwrap();
            assert!(frames.is_empty());
        }

        #[test]
        fn test_vt_decoder_codec_is_hevc() {
            let dec = VtDecoder::new().unwrap();
            assert_eq!(dec.codec(), Codec::Hevc);
        }

        #[test]
        fn test_vt_decoder_drop_without_session_does_not_panic() {
            let dec = VtDecoder::new().unwrap();
            drop(dec); // must not panic or call CFRelease on null
        }
    }
}

#[cfg(target_os = "macos")]
pub use macos::VtDecoder;
