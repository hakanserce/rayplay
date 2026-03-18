/// `VideoToolbox`-based HEVC/H.264 hardware decoder for Apple Silicon (macOS only).
///
/// This module is compiled only on macOS. It provides an implementation of
/// the `VideoDecoder` trait that drives Apple's `VideoToolbox` framework via
/// direct framework bindings (ADR-004, Option B), decoding HEVC/H.264 frames into
/// `CVPixelBuffer`s backed by `IOSurface` for zero-copy GPU rendering (ADR-005).
///
/// # Architecture
///
/// ```text
/// EncodedPacket (Annex B) ──► Annex B → HVCC/AVCC conversion
///                                  │
///                          CMBlockBuffer + CMSampleBuffer
///                                  │
///                    VTDecompressionSessionDecodeFrame
///                                  │
///                          CVPixelBuffer (`IOSurface`)
///                                  │
///                         DecodedFrame (hardware-backed)
///                                  │
///                        wgpu `IOSurface` external texture
/// ```
///
/// # Initialization
///
/// The `VTDecompressionSession` is initialized lazily on the first keyframe.
/// Non-keyframe packets received before the first keyframe are rejected with
/// `VideoError::DecodingFailed` — the caller must wait for a keyframe.
///
/// # Hardware Tests
///
/// The actual `VideoToolbox` API calls require Apple Silicon hardware and are
/// gated behind the `hw-codec-tests` feature flag. Without that feature,
/// `VtDecoder` parses the bitstream and validates parameter sets but returns
/// `VideoError::DecodingFailed` rather than submitting to hardware.
///
/// Required frameworks: `VideoToolbox`, `CoreMedia`, `CoreFoundation`, `CoreVideo`.
#[cfg(target_os = "macos")]
mod macos {
    use std::ffi::c_void;

    use crate::{
        decoded_frame::DecodedFrame,
        decoder::VideoDecoder,
        encoder::{Codec, VideoError},
        packet::EncodedPacket,
    };

    #[cfg(feature = "hw-codec-tests")]
    use crate::decoded_frame::{IoSurfaceHandle, PixelFormat};

    // ── Hardware-only FFI (compiled only with --features hw-codec-tests) ────────

    /// Mirrors `CMTime` from `CoreMedia`. Must match the C layout exactly.
    #[cfg(feature = "hw-codec-tests")]
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CmTime {
        value: i64,
        timescale: i32,
        flags: u32,
        epoch: i64,
    }

    #[cfg(feature = "hw-codec-tests")]
    impl CmTime {
        const INVALID: Self = Self {
            value: 0,
            timescale: 0,
            flags: 0,
            epoch: 0,
        };

        #[allow(clippy::cast_possible_wrap)] // timestamps stay well within i64 range
        fn from_micros(us: u64) -> Self {
            Self {
                value: us as i64,
                timescale: 1_000_000,
                flags: 1, // kCMTimeFlags_Valid
                epoch: 0,
            }
        }
    }

    /// Mirrors `CMSampleTimingInfo` from `CoreMedia`.
    #[cfg(feature = "hw-codec-tests")]
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CmSampleTimingInfo {
        duration: CmTime,
        presentation_timestamp: CmTime,
        decode_timestamp: CmTime,
    }

    /// Output slot populated by the `VTDecompressionOutputCallback`.
    #[cfg(feature = "hw-codec-tests")]
    struct FrameSlot {
        image_buffer: *mut c_void,
        status: i32,
    }

    /// `VTDecompressionOutputCallback` — called by `VideoToolbox` for each decoded frame.
    ///
    /// # Safety
    ///
    /// `source_frame_ref_con` must point to a valid `FrameSlot` alive for the duration
    /// of `VTDecompressionSessionDecodeFrame`.
    #[cfg(feature = "hw-codec-tests")]
    unsafe extern "C" fn decode_callback(
        _decompression_output_ref_con: *mut c_void,
        source_frame_ref_con: *mut c_void,
        status: i32,
        _info_flags: u32,
        image_buffer: *mut c_void,
        _presentation_timestamp: CmTime,
        _presentation_duration: CmTime,
    ) {
        // SAFETY: caller guarantees source_frame_ref_con is a valid &mut FrameSlot.
        let slot = unsafe { &mut *(source_frame_ref_con.cast::<FrameSlot>()) };
        slot.status = status;
        slot.image_buffer = image_buffer;
    }

    /// Mirrors `VTDecompressionOutputCallbackRecord`.
    #[cfg(feature = "hw-codec-tests")]
    #[repr(C)]
    struct VtOutputCallbackRecord {
        callback:
            unsafe extern "C" fn(*mut c_void, *mut c_void, i32, u32, *mut c_void, CmTime, CmTime),
        ref_con: *mut c_void,
    }

    #[cfg(feature = "hw-codec-tests")]
    #[link(name = "VideoToolbox", kind = "framework")]
    unsafe extern "C" {
        fn VTDecompressionSessionCreate(
            allocator: *const c_void,
            video_format_description: *mut c_void,
            video_decoder_specification: *const c_void,
            destination_image_buffer_attributes: *const c_void,
            output_callback: *const VtOutputCallbackRecord,
            session_out: *mut *mut c_void,
        ) -> i32;

        fn VTDecompressionSessionDecodeFrame(
            session: *mut c_void,
            sample_buffer: *mut c_void,
            decode_flags: u32,
            source_frame_ref_con: *mut c_void,
            info_flags_out: *mut u32,
        ) -> i32;

        fn VTDecompressionSessionWaitForAsynchronousFrames(session: *mut c_void) -> i32;

        fn VTDecompressionSessionInvalidate(session: *mut c_void);
    }

    #[cfg(feature = "hw-codec-tests")]
    #[link(name = "CoreMedia", kind = "framework")]
    unsafe extern "C" {
        fn CMVideoFormatDescriptionCreateFromHEVCParameterSets(
            allocator: *const c_void,
            parameter_set_count: usize,
            parameter_set_pointers: *const *const u8,
            parameter_set_sizes: *const usize,
            nal_unit_header_length: i32,
            extensions: *const c_void,
            format_description_out: *mut *mut c_void,
        ) -> i32;

        fn CMVideoFormatDescriptionCreateFromH264ParameterSets(
            allocator: *const c_void,
            parameter_set_count: usize,
            parameter_set_pointers: *const *const u8,
            parameter_set_sizes: *const usize,
            nal_unit_header_length: i32,
            format_description_out: *mut *mut c_void,
        ) -> i32;

        fn CMBlockBufferCreateWithMemoryBlock(
            allocator: *const c_void,
            memory_block: *mut c_void,
            block_length: usize,
            block_allocator: *const c_void,
            custom_block_source: *const c_void,
            offset_to_data: usize,
            data_length: usize,
            flags: u32,
            block_buffer_out: *mut *mut c_void,
        ) -> i32;

        fn CMSampleBufferCreateReady(
            allocator: *const c_void,
            data_buffer: *mut c_void,
            format_description: *mut c_void,
            num_samples: i32,
            num_sample_timing_entries: i32,
            sample_timing_array: *const CmSampleTimingInfo,
            num_sample_size_entries: i32,
            sample_size_array: *const usize,
            sample_buffer_out: *mut *mut c_void,
        ) -> i32;
    }

    #[cfg(feature = "hw-codec-tests")]
    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFRelease(cf: *const c_void);
    }

    #[cfg(feature = "hw-codec-tests")]
    #[link(name = "CoreVideo", kind = "framework")]
    unsafe extern "C" {
        fn CVPixelBufferGetWidth(pixel_buffer: *mut c_void) -> usize;
        fn CVPixelBufferGetHeight(pixel_buffer: *mut c_void) -> usize;
        fn CVPixelBufferGetBytesPerRowOfPlane(
            pixel_buffer: *mut c_void,
            plane_index: usize,
        ) -> usize;
        fn CVPixelBufferGetIOSurface(pixel_buffer: *mut c_void) -> *mut c_void;
    }

    #[cfg(feature = "hw-codec-tests")]
    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFRetain(cf: *const c_void) -> *const c_void;
    }

    // ── Annex B → HVCC/AVCC conversion (always compiled, fully testable) ───────

    /// Converts an HEVC/H.264 Annex B bitstream (start-code delimited) to HVCC/AVCC
    /// length-prefixed format required by `VideoToolbox`.
    ///
    /// Each NAL unit's `0x00 0x00 0x00 0x01` or `0x00 0x00 0x01` start code is
    /// replaced with a 4-byte big-endian NAL unit length prefix.
    #[cfg_attr(not(feature = "hw-codec-tests"), allow(dead_code))]
    #[allow(clippy::cast_possible_truncation)] // NAL units are always < 4 GiB
    fn annex_b_to_length_prefixed(data: &[u8]) -> Vec<u8> {
        let nals = split_nal_units(data);
        let mut out = Vec::with_capacity(data.len());
        for nal in nals {
            if !nal.is_empty() {
                let len = nal.len() as u32;
                out.extend_from_slice(&len.to_be_bytes());
                out.extend_from_slice(nal);
            }
        }
        out
    }

    /// Splits an Annex B bitstream into individual NAL unit byte slices
    /// (without start codes).
    fn split_nal_units(data: &[u8]) -> Vec<&[u8]> {
        let mut nals = Vec::new();
        let mut i = 0;
        let mut nal_start: Option<usize> = None;

        while i < data.len() {
            if i + 3 < data.len()
                && data[i] == 0
                && data[i + 1] == 0
                && data[i + 2] == 0
                && data[i + 3] == 1
            {
                if let Some(start) = nal_start {
                    nals.push(&data[start..i]);
                }
                nal_start = Some(i + 4);
                i += 4;
            } else if i + 2 < data.len() && data[i] == 0 && data[i + 1] == 0 && data[i + 2] == 1 {
                if let Some(start) = nal_start {
                    nals.push(&data[start..i]);
                }
                nal_start = Some(i + 3);
                i += 3;
            } else {
                i += 1;
            }
        }
        if let Some(start) = nal_start.filter(|&s| s < data.len()) {
            nals.push(&data[start..]);
        }
        nals
    }

    /// Returns `true` if the NAL unit type is an HEVC parameter set (VPS=32, SPS=33, PPS=34).
    fn is_hevc_parameter_set(nal: &[u8]) -> bool {
        if nal.is_empty() {
            return false;
        }
        let nal_type = (nal[0] >> 1) & 0x3F;
        (32..=34).contains(&nal_type)
    }

    /// Returns `true` if the NAL unit type is an H.264 parameter set (SPS=7, PPS=8).
    fn is_h264_parameter_set(nal: &[u8]) -> bool {
        if nal.is_empty() {
            return false;
        }
        let nal_type = nal[0] & 0x1F;
        (7..=8).contains(&nal_type)
    }

    // ── VtDecoder ──────────────────────────────────────────────────────────────

    /// HEVC/H.264 hardware decoder session backed by Apple's `VideoToolbox` framework.
    ///
    /// The session is initialized lazily on the first keyframe. Until then,
    /// `session` and `format_description` are null.
    ///
    /// # Thread Safety
    ///
    /// `VtDecoder` is `Send` but not `Sync` — each decode thread must own its
    /// own session. Raw pointers are accessed exclusively via `&mut self` methods.
    pub struct VtDecoder {
        /// The codec this decoder handles.
        codec: Codec,
        /// Active `VTDecompressionSessionRef`. Null until a keyframe initializes the session.
        session: *mut c_void,
        /// `CMVideoFormatDescriptionRef` from the first keyframe. Null until initialized.
        format_description: *mut c_void,
    }

    // SAFETY: VTDecompressionSession may be moved between threads provided it is
    // accessed from at most one thread at a time (`&mut self` guarantees this).
    unsafe impl Send for VtDecoder {}

    impl VtDecoder {
        /// Creates a new decoder for the specified codec. The `VideoToolbox` session
        /// is not started until the first keyframe is received via `decode`.
        ///
        /// # Errors
        ///
        /// Currently infallible; reserved for future pre-flight hardware checks.
        pub fn new(codec: Codec) -> Result<Self, VideoError> {
            tracing::debug!(
                "VtDecoder::new({:?}) — session deferred until first keyframe",
                codec
            );
            Ok(Self {
                codec,
                session: std::ptr::null_mut(),
                format_description: std::ptr::null_mut(),
            })
        }

        /// Returns `true` when both the session and format description are initialized.
        fn is_session_ready(&self) -> bool {
            !self.session.is_null() && !self.format_description.is_null()
        }

        /// Initializes the `VTDecompressionSession` from the parameter sets
        /// embedded in a keyframe's Annex B bitstream.
        ///
        /// For HEVC: extracts VPS/SPS/PPS parameter sets.
        /// For H.264: extracts SPS/PPS parameter sets.
        #[cfg_attr(not(feature = "hw-codec-tests"), allow(clippy::unused_self))]
        fn init_session(&mut self, packet: &EncodedPacket) -> Result<(), VideoError> {
            let nals = split_nal_units(&packet.data);
            let param_sets: Vec<&[u8]> = nals
                .iter()
                .copied()
                .filter(|n| match self.codec {
                    Codec::Hevc => is_hevc_parameter_set(n),
                    Codec::H264 => is_h264_parameter_set(n),
                })
                .collect();

            if param_sets.is_empty() {
                let param_names = match self.codec {
                    Codec::Hevc => "HEVC parameter sets (VPS/SPS/PPS)",
                    Codec::H264 => "H.264 parameter sets (SPS/PPS)",
                };
                return Err(VideoError::CorruptPacket {
                    reason: format!("keyframe contains no {param_names}"),
                });
            }

            #[cfg(feature = "hw-codec-tests")]
            {
                let ptrs: Vec<*const u8> = param_sets.iter().map(|n| n.as_ptr()).collect();
                let sizes: Vec<usize> = param_sets.iter().map(|n| n.len()).collect();

                let mut fmt_desc: *mut c_void = std::ptr::null_mut();
                // SAFETY: ptrs and sizes are valid for the duration of this call.
                let status = unsafe {
                    match self.codec {
                        Codec::Hevc => CMVideoFormatDescriptionCreateFromHEVCParameterSets(
                            std::ptr::null(),
                            param_sets.len(),
                            ptrs.as_ptr(),
                            sizes.as_ptr(),
                            4, // 4-byte length prefix (HVCC)
                            std::ptr::null(),
                            &raw mut fmt_desc,
                        ),
                        Codec::H264 => CMVideoFormatDescriptionCreateFromH264ParameterSets(
                            std::ptr::null(),
                            param_sets.len(),
                            ptrs.as_ptr(),
                            sizes.as_ptr(),
                            4, // 4-byte length prefix (AVCC)
                            &raw mut fmt_desc,
                        ),
                    }
                };
                if status != 0 || fmt_desc.is_null() {
                    let codec_name = match self.codec {
                        Codec::Hevc => "HEVC",
                        Codec::H264 => "H.264",
                    };
                    return Err(VideoError::DecodingFailed {
                        reason: format!(
                            "CMVideoFormatDescriptionCreateFrom{codec_name}ParameterSets failed: {status}"
                        ),
                    });
                }

                let callback_record = VtOutputCallbackRecord {
                    callback: decode_callback,
                    ref_con: std::ptr::null_mut(),
                };

                let mut session: *mut c_void = std::ptr::null_mut();
                // SAFETY: fmt_desc is valid; callback_record lives for this call.
                let status = unsafe {
                    VTDecompressionSessionCreate(
                        std::ptr::null(),
                        fmt_desc,
                        std::ptr::null(),
                        std::ptr::null(),
                        &raw const callback_record,
                        &raw mut session,
                    )
                };
                if status != 0 || session.is_null() {
                    // SAFETY: fmt_desc was successfully created above.
                    unsafe { CFRelease(fmt_desc.cast()) };
                    return Err(VideoError::DecodingFailed {
                        reason: format!("VTDecompressionSessionCreate failed: {status}"),
                    });
                }

                self.format_description = fmt_desc;
                self.session = session;
                tracing::debug!("VtDecoder: {:?} session initialized", self.codec);
                return Ok(());
            }

            // Without hw-codec-tests, the session cannot be created.
            Err(VideoError::DecodingFailed {
                reason: "hardware VideoToolbox decode requires --features hw-codec-tests"
                    .to_string(),
            })
        }

        /// Submits one length-prefixed packet to the active session and collects the
        /// decoded `CVPixelBuffer` via the synchronous callback path.
        ///
        /// Only available with `--features hw-codec-tests`.
        #[cfg(feature = "hw-codec-tests")]
        fn decode_packet(
            &mut self,
            packet: &EncodedPacket,
        ) -> Result<Option<DecodedFrame>, VideoError> {
            let mut length_prefixed = annex_b_to_length_prefixed(&packet.data);
            if length_prefixed.is_empty() {
                return Err(VideoError::CorruptPacket {
                    reason: "packet produced empty length-prefixed bitstream".to_string(),
                });
            }

            let data_len = length_prefixed.len();
            let mut block_buf: *mut c_void = std::ptr::null_mut();
            // SAFETY: length_prefixed is valid for the duration of the CMBlockBuffer lifetime.
            let status = unsafe {
                CMBlockBufferCreateWithMemoryBlock(
                    std::ptr::null(),
                    length_prefixed.as_mut_ptr().cast(),
                    data_len,
                    std::ptr::null(),
                    std::ptr::null(),
                    0,
                    data_len,
                    0,
                    &raw mut block_buf,
                )
            };
            if status != 0 || block_buf.is_null() {
                return Err(VideoError::DecodingFailed {
                    reason: format!("CMBlockBufferCreateWithMemoryBlock failed: {status}"),
                });
            }

            let timing = CmSampleTimingInfo {
                duration: CmTime::INVALID,
                presentation_timestamp: CmTime::from_micros(packet.timestamp_us),
                decode_timestamp: CmTime::INVALID,
            };
            let sample_size = data_len;
            let mut sample_buf: *mut c_void = std::ptr::null_mut();
            // SAFETY: block_buf and format_description are valid.
            let status = unsafe {
                CMSampleBufferCreateReady(
                    std::ptr::null(),
                    block_buf,
                    self.format_description,
                    1,
                    1,
                    &raw const timing,
                    1,
                    &raw const sample_size,
                    &raw mut sample_buf,
                )
            };
            // SAFETY: block_buf was successfully created.
            unsafe { CFRelease(block_buf.cast()) };
            if status != 0 || sample_buf.is_null() {
                return Err(VideoError::DecodingFailed {
                    reason: format!("CMSampleBufferCreateReady failed: {status}"),
                });
            }

            let mut slot = FrameSlot {
                image_buffer: std::ptr::null_mut(),
                status: 0,
            };
            let mut info_flags: u32 = 0;
            // SAFETY: session and sample_buf are valid; slot lives until after
            // WaitForAsynchronousFrames returns.
            let status = unsafe {
                VTDecompressionSessionDecodeFrame(
                    self.session,
                    sample_buf,
                    0,
                    (&raw mut slot).cast(),
                    &raw mut info_flags,
                )
            };
            // SAFETY: sample_buf was successfully created.
            unsafe { CFRelease(sample_buf.cast()) };
            if status != 0 {
                return Err(VideoError::DecodingFailed {
                    reason: format!("VTDecompressionSessionDecodeFrame failed: {status}"),
                });
            }

            // Drain any async frames (noop for synchronous decode, safe to call).
            // SAFETY: session is valid.
            unsafe { VTDecompressionSessionWaitForAsynchronousFrames(self.session) };

            if slot.status != 0 {
                return Err(VideoError::DecodingFailed {
                    reason: format!("decode callback reported error: {}", slot.status),
                });
            }
            if slot.image_buffer.is_null() {
                return Ok(None);
            }

            let frame = Self::pixel_buffer_to_frame(slot.image_buffer, packet.timestamp_us)?;
            Ok(Some(frame))
        }

        /// Extracts the `IOSurface` from a `CVPixelBuffer` and wraps it in a
        /// hardware-backed `DecodedFrame` for zero-copy GPU rendering (ADR-005).
        ///
        /// No CPU copy occurs — the `IOSurface` is `CFRetain`ed and handed to
        /// the renderer, which imports it as a Metal texture.
        ///
        /// Only available with `--features hw-codec-tests`.
        #[cfg(feature = "hw-codec-tests")]
        #[allow(clippy::cast_possible_truncation)] // pixel dimensions fit in u32 for any real frame
        fn pixel_buffer_to_frame(
            pixel_buffer: *mut c_void,
            timestamp_us: u64,
        ) -> Result<DecodedFrame, VideoError> {
            // SAFETY: pixel_buffer is a valid CVPixelBufferRef.
            let (width, height, stride) = unsafe {
                (
                    CVPixelBufferGetWidth(pixel_buffer) as u32,
                    CVPixelBufferGetHeight(pixel_buffer) as u32,
                    CVPixelBufferGetBytesPerRowOfPlane(pixel_buffer, 0) as u32,
                )
            };

            // SAFETY: pixel_buffer is a valid CVPixelBufferRef backed by an IOSurface.
            let iosurface_ptr = unsafe { CVPixelBufferGetIOSurface(pixel_buffer) };
            if iosurface_ptr.is_null() {
                return Err(VideoError::DecodingFailed {
                    reason: "CVPixelBufferGetIOSurface returned null".to_string(),
                });
            }

            // The IOSurface is owned by the pixel buffer; retain our own reference.
            // SAFETY: iosurface_ptr is a valid IOSurfaceRef.
            unsafe { CFRetain(iosurface_ptr.cast_const()) };

            // SAFETY: we just retained the IOSurface above.
            let handle = unsafe { IoSurfaceHandle::from_retained(iosurface_ptr) };

            Ok(DecodedFrame::new_hardware(
                width,
                height,
                stride,
                PixelFormat::Nv12,
                timestamp_us,
                handle,
            ))
        }
    }

    impl VideoDecoder for VtDecoder {
        fn decode(&mut self, packet: &EncodedPacket) -> Result<Option<DecodedFrame>, VideoError> {
            if !packet.is_keyframe && !self.is_session_ready() {
                return Err(VideoError::DecodingFailed {
                    reason: "waiting for keyframe to initialize VTDecompressionSession".to_string(),
                });
            }
            if !self.is_session_ready() {
                self.init_session(packet)?;
            }
            #[cfg(feature = "hw-codec-tests")]
            {
                return self.decode_packet(packet);
            }
            // Without hw-codec-tests, init_session always returns Err before reaching here.
            #[cfg(not(feature = "hw-codec-tests"))]
            {
                Ok(None)
            }
        }

        fn flush(&mut self) -> Result<Vec<DecodedFrame>, VideoError> {
            #[cfg(feature = "hw-codec-tests")]
            if !self.session.is_null() {
                // SAFETY: session is valid.
                unsafe { VTDecompressionSessionWaitForAsynchronousFrames(self.session) };
            }
            Ok(vec![])
        }

        fn codec(&self) -> Codec {
            self.codec
        }
    }

    impl Drop for VtDecoder {
        fn drop(&mut self) {
            // SAFETY: session and format_description are valid non-null pointers
            // that must be released. In non-hw-codec-tests builds they are always null.
            #[cfg(feature = "hw-codec-tests")]
            unsafe {
                if !self.session.is_null() {
                    VTDecompressionSessionInvalidate(self.session);
                    CFRelease(self.session.cast());
                }
                if !self.format_description.is_null() {
                    CFRelease(self.format_description.cast());
                }
            }
        }
    }

    // ── Unit tests ─────────────────────────────────────────────────────────────

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::packet::EncodedPacket;

        // ── annex_b_to_length_prefixed ─────────────────────────────────────────

        #[test]
        fn test_annex_b_to_length_prefixed_empty_input_returns_empty() {
            assert!(annex_b_to_length_prefixed(&[]).is_empty());
        }

        #[test]
        fn test_annex_b_to_length_prefixed_4byte_start_code_replaced_with_length() {
            let input = [0x00u8, 0x00, 0x00, 0x01, 0x26, 0x01];
            let out = annex_b_to_length_prefixed(&input);
            assert_eq!(&out[..4], &[0, 0, 0, 2]);
            assert_eq!(&out[4..], &[0x26, 0x01]);
        }

        #[test]
        fn test_annex_b_to_length_prefixed_3byte_start_code_replaced_with_length() {
            let input = [0x00u8, 0x00, 0x01, 0x26, 0x01];
            let out = annex_b_to_length_prefixed(&input);
            assert_eq!(&out[..4], &[0, 0, 0, 2]);
            assert_eq!(&out[4..], &[0x26, 0x01]);
        }

        #[test]
        fn test_annex_b_to_length_prefixed_two_nal_units() {
            let input = [
                0x00u8, 0x00, 0x00, 0x01, 0xAA, // NAL 1 (1 byte)
                0x00, 0x00, 0x00, 0x01, 0xBB, 0xCC, // NAL 2 (2 bytes)
            ];
            let out = annex_b_to_length_prefixed(&input);
            assert_eq!(&out[..4], &[0, 0, 0, 1]);
            assert_eq!(out[4], 0xAA);
            assert_eq!(&out[5..9], &[0, 0, 0, 2]);
            assert_eq!(&out[9..], &[0xBB, 0xCC]);
        }

        #[test]
        fn test_annex_b_to_length_prefixed_data_without_start_codes_returns_empty() {
            // No start codes → split_nal_units returns [] → no output
            let input = [0xAAu8, 0xBB, 0xCC];
            assert!(annex_b_to_length_prefixed(&input).is_empty());
        }

        #[test]
        fn test_annex_b_to_length_prefixed_trailing_start_code_with_no_nal_bytes_returns_empty() {
            // Start code at end with no bytes following — produces no NAL unit.
            let input = [0x00u8, 0x00, 0x00, 0x01];
            assert!(annex_b_to_length_prefixed(&input).is_empty());
        }

        // ── split_nal_units ────────────────────────────────────────────────────

        #[test]
        fn test_split_nal_units_empty_returns_empty() {
            assert!(split_nal_units(&[]).is_empty());
        }

        #[test]
        fn test_split_nal_units_single_4byte_start_code() {
            let input = [0x00u8, 0x00, 0x00, 0x01, 0x40, 0x01];
            let nals = split_nal_units(&input);
            assert_eq!(nals.len(), 1);
            assert_eq!(nals[0], &[0x40u8, 0x01]);
        }

        #[test]
        fn test_split_nal_units_single_3byte_start_code() {
            let input = [0x00u8, 0x00, 0x01, 0x40, 0x01];
            let nals = split_nal_units(&input);
            assert_eq!(nals.len(), 1);
            assert_eq!(nals[0], &[0x40u8, 0x01]);
        }

        #[test]
        fn test_split_nal_units_two_nals() {
            let input = [
                0x00u8, 0x00, 0x00, 0x01, 0x40, // VPS
                0x00, 0x00, 0x00, 0x01, 0x42, // SPS
            ];
            let nals = split_nal_units(&input);
            assert_eq!(nals.len(), 2);
            assert_eq!(nals[0], &[0x40u8]);
            assert_eq!(nals[1], &[0x42u8]);
        }

        #[test]
        fn test_split_nal_units_two_3byte_start_codes() {
            // Both NAL units use 3-byte start codes; the second start code must
            // push the first NAL (line 263 in split_nal_units).
            let input = [
                0x00u8, 0x00, 0x01, 0x40, // first NAL: 3-byte start + payload
                0x00, 0x00, 0x01, 0x42, // second NAL: 3-byte start + payload
            ];
            let nals = split_nal_units(&input);
            assert_eq!(nals.len(), 2);
            assert_eq!(nals[0], &[0x40u8]);
            assert_eq!(nals[1], &[0x42u8]);
        }

        #[test]
        fn test_split_nal_units_no_trailing_start_code() {
            let input = [0x00u8, 0x00, 0x00, 0x01, 0x44, 0x01, 0x02];
            let nals = split_nal_units(&input);
            assert_eq!(nals.len(), 1);
            assert_eq!(nals[0], &[0x44u8, 0x01, 0x02]);
        }

        #[test]
        fn test_split_nal_units_no_start_codes_returns_empty() {
            assert!(split_nal_units(&[0xAAu8, 0xBB]).is_empty());
        }

        // ── is_hevc_parameter_set ──────────────────────────────────────────────

        #[test]
        fn test_is_hevc_parameter_set_vps_type_32() {
            assert!(is_hevc_parameter_set(&[0x40, 0x01])); // (32 << 1) = 0x40
        }

        #[test]
        fn test_is_hevc_parameter_set_sps_type_33() {
            assert!(is_hevc_parameter_set(&[0x42, 0x01])); // (33 << 1) = 0x42
        }

        #[test]
        fn test_is_hevc_parameter_set_pps_type_34() {
            assert!(is_hevc_parameter_set(&[0x44, 0x01])); // (34 << 1) = 0x44
        }

        #[test]
        fn test_is_hevc_parameter_set_idr_not_param_set() {
            assert!(!is_hevc_parameter_set(&[0x26, 0x01])); // NAL type 19 = IDR
        }

        #[test]
        fn test_is_hevc_parameter_set_empty_returns_false() {
            assert!(!is_hevc_parameter_set(&[]));
        }

        // ── is_h264_parameter_set ──────────────────────────────────────────────

        #[test]
        fn test_is_h264_parameter_set_sps_type_7() {
            assert!(is_h264_parameter_set(&[0x67, 0x01])); // NAL type 7 = SPS
        }

        #[test]
        fn test_is_h264_parameter_set_pps_type_8() {
            assert!(is_h264_parameter_set(&[0x68, 0x01])); // NAL type 8 = PPS
        }

        #[test]
        fn test_is_h264_parameter_set_idr_not_param_set() {
            assert!(!is_h264_parameter_set(&[0x65, 0x01])); // NAL type 5 = IDR
        }

        #[test]
        fn test_is_h264_parameter_set_empty_returns_false() {
            assert!(!is_h264_parameter_set(&[]));
        }

        // ── VtDecoder lifecycle ────────────────────────────────────────────────

        #[test]
        fn test_vt_decoder_new_hevc_returns_ok() {
            let dec = VtDecoder::new(Codec::Hevc).unwrap();
            assert_eq!(dec.codec(), Codec::Hevc);
        }

        #[test]
        fn test_vt_decoder_new_h264_returns_ok() {
            let dec = VtDecoder::new(Codec::H264).unwrap();
            assert_eq!(dec.codec(), Codec::H264);
        }

        #[test]
        fn test_vt_decoder_is_session_ready_false_after_new() {
            let dec = VtDecoder::new(Codec::Hevc).unwrap();
            assert!(!dec.is_session_ready());
        }

        #[test]
        fn test_vt_decoder_decode_non_keyframe_without_session_returns_error() {
            let mut dec = VtDecoder::new(Codec::Hevc).unwrap();
            let packet = EncodedPacket::new(vec![0u8; 64], false, 0, 16_667);
            let err = dec.decode(&packet).unwrap_err();
            assert!(matches!(err, VideoError::DecodingFailed { .. }));
            assert!(err.to_string().contains("keyframe"));
        }

        #[test]
        fn test_vt_decoder_decode_hevc_keyframe_no_param_sets_returns_corrupt_packet() {
            let mut dec = VtDecoder::new(Codec::Hevc).unwrap();
            // IDR slice only — no VPS/SPS/PPS
            let idr = vec![0x00u8, 0x00, 0x00, 0x01, 0x26, 0x01, 0x00];
            let packet = EncodedPacket::new(idr, true, 0, 16_667);
            let err = dec.decode(&packet).unwrap_err();
            assert!(matches!(err, VideoError::CorruptPacket { .. }));
            assert!(err.to_string().contains("HEVC parameter sets"));
        }

        #[test]
        fn test_vt_decoder_decode_h264_keyframe_no_param_sets_returns_corrupt_packet() {
            let mut dec = VtDecoder::new(Codec::H264).unwrap();
            // IDR slice only — no SPS/PPS
            let idr = vec![0x00u8, 0x00, 0x00, 0x01, 0x65, 0x01, 0x00];
            let packet = EncodedPacket::new(idr, true, 0, 16_667);
            let err = dec.decode(&packet).unwrap_err();
            assert!(matches!(err, VideoError::CorruptPacket { .. }));
            assert!(err.to_string().contains("H.264 parameter sets"));
        }

        #[test]
        fn test_vt_decoder_decode_hevc_keyframe_with_param_sets_returns_decoding_failed() {
            let mut dec = VtDecoder::new(Codec::Hevc).unwrap();
            // Fake VPS + SPS + PPS NALs — real VT will reject invalid SPS data.
            let packet_data = vec![
                0x00u8, 0x00, 0x00, 0x01, 0x40, 0x01, // fake VPS
                0x00, 0x00, 0x00, 0x01, 0x42, 0x01, // fake SPS
                0x00, 0x00, 0x00, 0x01, 0x44, 0x01, // fake PPS
            ];
            let packet = EncodedPacket::new(packet_data, true, 0, 16_667);
            let err = dec.decode(&packet).unwrap_err();
            // Either CMVideoFormatDescriptionCreate fails (hw build) or
            // we hit the non-hw fallback error (non-hw build).
            assert!(matches!(
                err,
                VideoError::DecodingFailed { .. } | VideoError::CorruptPacket { .. }
            ));
        }

        #[test]
        fn test_vt_decoder_decode_h264_keyframe_with_param_sets_returns_decoding_failed() {
            let mut dec = VtDecoder::new(Codec::H264).unwrap();
            // Fake SPS + PPS NALs — real VT will reject invalid SPS data.
            let packet_data = vec![
                0x00u8, 0x00, 0x00, 0x01, 0x67, 0x01, // fake SPS
                0x00, 0x00, 0x00, 0x01, 0x68, 0x01, // fake PPS
            ];
            let packet = EncodedPacket::new(packet_data, true, 0, 16_667);
            let err = dec.decode(&packet).unwrap_err();
            // Either CMVideoFormatDescriptionCreate fails (hw build) or
            // we hit the non-hw fallback error (non-hw build).
            assert!(matches!(
                err,
                VideoError::DecodingFailed { .. } | VideoError::CorruptPacket { .. }
            ));
        }

        #[test]
        fn test_vt_decoder_flush_returns_empty() {
            let mut dec = VtDecoder::new(Codec::Hevc).unwrap();
            assert!(dec.flush().unwrap().is_empty());
        }

        #[test]
        fn test_vt_decoder_codec_is_hevc() {
            let dec = VtDecoder::new(Codec::Hevc).unwrap();
            assert_eq!(dec.codec(), Codec::Hevc);
        }

        #[test]
        fn test_vt_decoder_codec_is_h264() {
            let dec = VtDecoder::new(Codec::H264).unwrap();
            assert_eq!(dec.codec(), Codec::H264);
        }

        #[test]
        fn test_vt_decoder_drop_without_session_does_not_panic() {
            let dec = VtDecoder::new(Codec::Hevc).unwrap();
            drop(dec);
        }
    }
}

#[cfg(target_os = "macos")]
pub use macos::VtDecoder;
