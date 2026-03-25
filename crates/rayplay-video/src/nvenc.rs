/// NVENC-based HEVC hardware encoder for Nvidia GPUs (Windows only).
///
/// This module is compiled only on Windows. It provides an implementation
/// of the `VideoEncoder` trait that drives Nvidia's NVENC hardware encoder
/// via the Video Codec SDK, encoding frames directly from DXGI-captured
/// textures with zero copies on the input path (ADR-001, Option B).
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
#[allow(
    clippy::cast_possible_truncation,
    clippy::field_reassign_with_default,
    clippy::borrow_as_ptr,
    clippy::not_unsafe_ptr_arg_deref,
    clippy::similar_names,
    clippy::too_many_lines
)]
mod windows {
    use std::{collections::HashMap, ffi::c_void, mem, ptr};

    use windows::{
        Win32::{
            Foundation::HMODULE,
            System::LibraryLoader::{GetProcAddress, LoadLibraryA},
        },
        core::PCSTR,
    };

    use crate::{
        encoder::{Codec, EncoderConfig, EncoderInput, VideoEncoder, VideoError},
        nvenc_sys::{
            NV_ENC_BUFFER_FORMAT_ARGB, NV_ENC_CODEC_H264_GUID, NV_ENC_CODEC_HEVC_GUID,
            NV_ENC_CREATE_BITSTREAM_BUFFER, NV_ENC_DEVICE_TYPE_DIRECTX,
            NV_ENC_H264_PROFILE_MAIN_GUID, NV_ENC_HEVC_PROFILE_MAIN_GUID, NV_ENC_INITIALIZE_PARAMS,
            NV_ENC_INPUT_RESOURCE_TYPE_DIRECTX, NV_ENC_LOCK_BITSTREAM, NV_ENC_MAP_INPUT_RESOURCE,
            NV_ENC_PARAMS_RC_VBR, NV_ENC_PIC_FLAG_EOS, NV_ENC_PIC_FLAG_FORCEIDR, NV_ENC_PIC_PARAMS,
            NV_ENC_PIC_STRUCT_FRAME, NV_ENC_PIC_TYPE_I, NV_ENC_PIC_TYPE_IDR, NV_ENC_PRESET_CONFIG,
            NV_ENC_PRESET_P1_GUID, NV_ENC_REGISTER_RESOURCE, NV_ENC_SUCCESS,
            NV_ENC_TUNING_INFO_ULTRA_LOW_LATENCY, NV_ENCODE_API_FUNCTION_LIST,
            NVENCAPI_MAJOR_VERSION, NVENCAPI_MINOR_VERSION, PFnNvEncodeAPICreateInstance,
            PFnNvEncodeAPIGetMaxSupportedVersion, nvenc_status_to_string, open_session,
            unpack_max_version,
        },
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
        api: NV_ENCODE_API_FUNCTION_LIST,
        encoder: *mut c_void,
        output_buffers: Vec<*mut c_void>,
        registered_resources: HashMap<*mut c_void, *mut c_void>,
        current_output_idx: usize,
        frame_index: u64,
        _dll_handle: HMODULE, // Keep DLL loaded
    }

    // SAFETY: NVENC session is accessed only from the encoding thread
    unsafe impl Send for NvencEncoder {}

    impl NvencEncoder {
        /// Opens a new NVENC encode session and initialises it for the configured codec.
        ///
        /// # Arguments
        ///
        /// * `config` - Encoder configuration (resolution, bitrate, codec)
        /// * `device_ptr` - Pointer to the D3D11 device from `SharedD3D11Device::device_ptr()`
        ///
        /// # Errors
        ///
        /// - `VideoError::UnsupportedCodec` — GPU does not support the configured codec.
        /// - `VideoError::EncodingFailed`   — Session creation or init failed.
        pub fn new(config: EncoderConfig, device_ptr: *mut c_void) -> Result<Self, VideoError> {
            tracing::info!(
                width = config.width,
                height = config.height,
                fps = config.fps,
                bitrate_bps = config.resolved_bitrate(),
                codec = %config.codec,
                "Initializing NVENC encoder session"
            );

            // Load NVENC DLL dynamically
            let dll_handle = unsafe {
                LoadLibraryA(PCSTR(c"nvEncodeAPI64.dll".as_ptr().cast())).map_err(|e| {
                    VideoError::EncodingFailed {
                        reason: format!("Failed to load nvEncodeAPI64.dll: {e}"),
                    }
                })?
            };

            // Check driver supports our SDK version
            let get_max_version_ptr = unsafe {
                GetProcAddress(
                    dll_handle,
                    PCSTR(c"NvEncodeAPIGetMaxSupportedVersion".as_ptr().cast()),
                )
                .ok_or_else(|| VideoError::EncodingFailed {
                    reason: "NvEncodeAPIGetMaxSupportedVersion not found in DLL".to_string(),
                })?
            };

            let get_max_version: PFnNvEncodeAPIGetMaxSupportedVersion =
                unsafe { mem::transmute(get_max_version_ptr) };

            let mut driver_max_version: u32 = 0;
            let status = unsafe { get_max_version(&mut driver_max_version) };
            if status != NV_ENC_SUCCESS {
                return Err(VideoError::EncodingFailed {
                    reason: format!(
                        "NvEncodeAPIGetMaxSupportedVersion failed: {} (status={})",
                        nvenc_status_to_string(status),
                        status
                    ),
                });
            }

            let (drv_major, drv_minor) = unpack_max_version(driver_max_version);
            tracing::info!(
                driver_major = drv_major,
                driver_minor = drv_minor,
                sdk_major = NVENCAPI_MAJOR_VERSION,
                sdk_minor = NVENCAPI_MINOR_VERSION,
                "NVENC driver version check"
            );

            // Get API creation function
            let create_instance_ptr = unsafe {
                GetProcAddress(
                    dll_handle,
                    PCSTR(c"NvEncodeAPICreateInstance".as_ptr().cast()),
                )
                .ok_or_else(|| VideoError::EncodingFailed {
                    reason: "NvEncodeAPICreateInstance not found in DLL".to_string(),
                })?
            };

            let create_instance: PFnNvEncodeAPICreateInstance =
                unsafe { mem::transmute(create_instance_ptr) };

            // Create API function list
            let mut api = NV_ENCODE_API_FUNCTION_LIST::new_versioned();
            let status = unsafe { create_instance(&mut api) };
            if status != NV_ENC_SUCCESS {
                return Err(VideoError::EncodingFailed {
                    reason: format!(
                        "NvEncodeAPICreateInstance failed: {} (status={})",
                        nvenc_status_to_string(status),
                        status
                    ),
                });
            }

            // Open encoding session (with version validation)
            let encoder = unsafe {
                open_session(
                    driver_max_version,
                    api.nvEncOpenEncodeSessionEx,
                    device_ptr,
                    NV_ENC_DEVICE_TYPE_DIRECTX,
                )?
            };

            tracing::debug!("NVENC session opened successfully");

            // Get preset configuration
            let codec_guid = match config.codec {
                Codec::Hevc => NV_ENC_CODEC_HEVC_GUID,
                Codec::H264 => NV_ENC_CODEC_H264_GUID,
            };

            let mut preset_config = NV_ENC_PRESET_CONFIG::new_versioned();
            let nvenc_get_preset =
                api.nvEncGetEncodePresetConfigEx
                    .ok_or_else(|| VideoError::EncodingFailed {
                        reason: "nvEncGetEncodePresetConfigEx function not available".to_string(),
                    })?;

            let status = unsafe {
                nvenc_get_preset(
                    encoder,
                    codec_guid,
                    NV_ENC_PRESET_P1_GUID,
                    NV_ENC_TUNING_INFO_ULTRA_LOW_LATENCY,
                    &mut preset_config,
                )
            };
            if status != NV_ENC_SUCCESS {
                return Err(VideoError::EncodingFailed {
                    reason: format!(
                        "nvEncGetEncodePresetConfigEx failed: {} (status={})",
                        nvenc_status_to_string(status),
                        status
                    ),
                });
            }

            // Configure encoder parameters
            let mut init_params = NV_ENC_INITIALIZE_PARAMS::new_versioned();
            init_params.encodeGUID = codec_guid;
            init_params.presetGUID = NV_ENC_PRESET_P1_GUID;
            init_params.encodeWidth = config.width;
            init_params.encodeHeight = config.height;
            init_params.darWidth = config.width;
            init_params.darHeight = config.height;
            init_params.frameRateNum = config.fps;
            init_params.frameRateDen = 1;
            init_params.enablePTD = 1;

            // Setup encode config
            let mut encode_config = preset_config.presetCfg;
            encode_config.profileGUID = match config.codec {
                Codec::Hevc => NV_ENC_HEVC_PROFILE_MAIN_GUID,
                Codec::H264 => NV_ENC_H264_PROFILE_MAIN_GUID,
            };
            encode_config.gopLength = (config.fps / 2).max(1); // 0.5 seconds
            encode_config.frameIntervalP = 1; // All P-frames for low latency

            // Rate control configuration
            encode_config.rcParams.rateControlMode = NV_ENC_PARAMS_RC_VBR;
            encode_config.rcParams.averageBitRate = config.resolved_bitrate();
            encode_config.rcParams.maxBitRate = config.resolved_bitrate() * 12 / 10; // 20% headroom
            encode_config.rcParams.set_enableAQ(1);
            encode_config.rcParams.set_zeroReorderDelay(1);

            // Codec-specific configuration — modify the preset's codec config
            // in-place rather than replacing it, preserving driver-tuned values
            // like maxNumRefFrames, level, etc. (matches FFmpeg approach).
            match config.codec {
                Codec::Hevc => {
                    let hevc = unsafe { &mut encode_config.encodeCodecConfig.hevcConfig };
                    hevc.set_repeatSPSPPS(1);
                    hevc.set_chromaFormatIDC(1); // YUV420
                    hevc.idrPeriod = encode_config.gopLength;
                }
                Codec::H264 => {
                    let h264 = unsafe { &mut encode_config.encodeCodecConfig.h264Config };
                    h264.set_repeatSPSPPS(1);
                    h264.idrPeriod = encode_config.gopLength;
                }
            }

            init_params.tuningInfo = NV_ENC_TUNING_INFO_ULTRA_LOW_LATENCY;
            init_params.encodeConfig = &mut encode_config;

            // Initialize encoder
            let nvenc_init =
                api.nvEncInitializeEncoder
                    .ok_or_else(|| VideoError::EncodingFailed {
                        reason: "nvEncInitializeEncoder function not available".to_string(),
                    })?;

            let status = unsafe { nvenc_init(encoder, &mut init_params) };
            if status != NV_ENC_SUCCESS {
                return Err(VideoError::EncodingFailed {
                    reason: format!(
                        "nvEncInitializeEncoder failed: {} (status={})",
                        nvenc_status_to_string(status),
                        status
                    ),
                });
            }

            tracing::info!("NVENC encoder initialized successfully");

            // Create output bitstream buffers (ring buffer with 2 buffers)
            let mut output_buffers = Vec::with_capacity(2);
            let nvenc_create_bitstream =
                api.nvEncCreateBitstreamBuffer
                    .ok_or_else(|| VideoError::EncodingFailed {
                        reason: "nvEncCreateBitstreamBuffer function not available".to_string(),
                    })?;

            for i in 0..2 {
                let mut buffer_params = NV_ENC_CREATE_BITSTREAM_BUFFER::new_versioned();
                buffer_params.size = config.width * config.height; // Conservative estimate

                let status = unsafe { nvenc_create_bitstream(encoder, &mut buffer_params) };
                if status != NV_ENC_SUCCESS {
                    return Err(VideoError::EncodingFailed {
                        reason: format!(
                            "nvEncCreateBitstreamBuffer {} failed: {} (status={})",
                            i,
                            nvenc_status_to_string(status),
                            status
                        ),
                    });
                }

                output_buffers.push(buffer_params.bitstreamBuffer);
            }

            tracing::debug!("Created {} output bitstream buffers", output_buffers.len());

            Ok(Self {
                config,
                api,
                encoder,
                output_buffers,
                registered_resources: HashMap::new(),
                current_output_idx: 0,
                frame_index: 0,
                _dll_handle: dll_handle,
            })
        }

        /// Registers a D3D11 texture with NVENC for zero-copy encoding.
        fn register_texture(
            &mut self,
            texture_ptr: *mut c_void,
            width: u32,
            height: u32,
        ) -> Result<*mut c_void, VideoError> {
            if let Some(&registered_ptr) = self.registered_resources.get(&texture_ptr) {
                return Ok(registered_ptr);
            }

            let nvenc_register =
                self.api
                    .nvEncRegisterResource
                    .ok_or_else(|| VideoError::EncodingFailed {
                        reason: "nvEncRegisterResource function not available".to_string(),
                    })?;

            let mut register_params = NV_ENC_REGISTER_RESOURCE::new_versioned();
            register_params.resourceType = NV_ENC_INPUT_RESOURCE_TYPE_DIRECTX;
            register_params.resourceToRegister = texture_ptr;
            register_params.width = width;
            register_params.height = height;
            register_params.bufferFormat = NV_ENC_BUFFER_FORMAT_ARGB;

            let status = unsafe { nvenc_register(self.encoder, &mut register_params) };
            if status != NV_ENC_SUCCESS {
                return Err(VideoError::EncodingFailed {
                    reason: format!(
                        "nvEncRegisterResource failed: {} (status={})",
                        nvenc_status_to_string(status),
                        status
                    ),
                });
            }

            let registered_ptr = register_params.registeredResource;
            self.registered_resources
                .insert(texture_ptr, registered_ptr);

            tracing::debug!(
                "Registered texture {:p} as {:p}",
                texture_ptr,
                registered_ptr
            );
            Ok(registered_ptr)
        }

        /// Encodes a GPU texture using zero-copy path.
        fn encode_gpu_texture(
            &mut self,
            texture_ptr: *mut c_void,
            width: u32,
            height: u32,
            timestamp_us: u64,
        ) -> Result<Option<EncodedPacket>, VideoError> {
            // Register texture if not already done
            let registered_ptr = self.register_texture(texture_ptr, width, height)?;

            // Map the resource for encoding
            let nvenc_map =
                self.api
                    .nvEncMapInputResource
                    .ok_or_else(|| VideoError::EncodingFailed {
                        reason: "nvEncMapInputResource function not available".to_string(),
                    })?;

            let mut map_params = NV_ENC_MAP_INPUT_RESOURCE::new_versioned();
            map_params.registeredResource = registered_ptr;

            let status = unsafe { nvenc_map(self.encoder, &mut map_params) };
            if status != NV_ENC_SUCCESS {
                return Err(VideoError::EncodingFailed {
                    reason: format!(
                        "nvEncMapInputResource failed: {} (status={})",
                        nvenc_status_to_string(status),
                        status
                    ),
                });
            }

            let mapped_resource = map_params.mappedResource;
            let buffer_fmt = map_params.mappedBufferFmt;

            // Encode the frame
            let result = self.encode_mapped_resource(mapped_resource, buffer_fmt, timestamp_us);

            // Unmap the resource
            let nvenc_unmap =
                self.api
                    .nvEncUnmapInputResource
                    .ok_or_else(|| VideoError::EncodingFailed {
                        reason: "nvEncUnmapInputResource function not available".to_string(),
                    })?;

            let status = unsafe { nvenc_unmap(self.encoder, mapped_resource) };
            if status != NV_ENC_SUCCESS {
                tracing::warn!(
                    "nvEncUnmapInputResource failed: {} (status={})",
                    nvenc_status_to_string(status),
                    status
                );
            }

            result
        }

        /// Encodes a mapped resource.
        fn encode_mapped_resource(
            &mut self,
            mapped_resource: *mut c_void,
            buffer_fmt: i32,
            timestamp_us: u64,
        ) -> Result<Option<EncodedPacket>, VideoError> {
            // Setup encoding parameters
            let mut pic_params = NV_ENC_PIC_PARAMS::new_versioned();
            pic_params.inputBuffer = mapped_resource;
            pic_params.bufferFmt = buffer_fmt;
            pic_params.pictureStruct = NV_ENC_PIC_STRUCT_FRAME;
            pic_params.inputWidth = self.config.width;
            pic_params.inputHeight = self.config.height;
            pic_params.outputBitstream = self.output_buffers[self.current_output_idx];
            pic_params.inputTimeStamp = timestamp_us;
            pic_params.inputDuration = 0;
            pic_params.frameIdx = self.frame_index as u32;

            // Force IDR keyframes every GOP length (0.5 seconds)
            let gop = u64::from((self.config.fps / 2).max(1));
            if self.frame_index.is_multiple_of(gop) {
                pic_params.encodePicFlags = NV_ENC_PIC_FLAG_FORCEIDR;
            }

            // Encode the picture
            let nvenc_encode =
                self.api
                    .nvEncEncodePicture
                    .ok_or_else(|| VideoError::EncodingFailed {
                        reason: "nvEncEncodePicture function not available".to_string(),
                    })?;

            let status = unsafe { nvenc_encode(self.encoder, &mut pic_params) };
            if status != NV_ENC_SUCCESS {
                return Err(VideoError::EncodingFailed {
                    reason: format!(
                        "nvEncEncodePicture failed: {} (status={})",
                        nvenc_status_to_string(status),
                        status
                    ),
                });
            }

            // Lock bitstream and read encoded data
            let packet = self.lock_and_read_bitstream(timestamp_us)?;

            // Move to next output buffer
            self.current_output_idx = (self.current_output_idx + 1) % self.output_buffers.len();
            self.frame_index += 1;

            Ok(packet)
        }

        /// Locks the output bitstream and reads the encoded data.
        fn lock_and_read_bitstream(
            &self,
            timestamp_us: u64,
        ) -> Result<Option<EncodedPacket>, VideoError> {
            let nvenc_lock =
                self.api
                    .nvEncLockBitstream
                    .ok_or_else(|| VideoError::EncodingFailed {
                        reason: "nvEncLockBitstream function not available".to_string(),
                    })?;

            let mut lock_params = NV_ENC_LOCK_BITSTREAM::new_versioned();
            lock_params.outputBitstream = self.output_buffers[self.current_output_idx];

            let status = unsafe { nvenc_lock(self.encoder, &mut lock_params) };
            if status != NV_ENC_SUCCESS {
                return Err(VideoError::EncodingFailed {
                    reason: format!(
                        "nvEncLockBitstream failed: {} (status={})",
                        nvenc_status_to_string(status),
                        status
                    ),
                });
            }

            // Check if we have data
            if lock_params.bitstreamSizeInBytes == 0 {
                tracing::debug!("No encoded data available (encoder buffering)");

                // Unlock the bitstream
                let nvenc_unlock =
                    self.api
                        .nvEncUnlockBitstream
                        .ok_or_else(|| VideoError::EncodingFailed {
                            reason: "nvEncUnlockBitstream function not available".to_string(),
                        })?;
                let _ = unsafe { nvenc_unlock(self.encoder, lock_params.outputBitstream) };

                return Ok(None);
            }

            // Copy encoded data
            let data_size = lock_params.bitstreamSizeInBytes as usize;
            let mut encoded_data = vec![0u8; data_size];

            if !lock_params.bitstreamBufferPtr.is_null() {
                unsafe {
                    ptr::copy_nonoverlapping(
                        lock_params.bitstreamBufferPtr as *const u8,
                        encoded_data.as_mut_ptr(),
                        data_size,
                    );
                }
            }

            // Determine frame type
            let is_keyframe = lock_params.pictureType == NV_ENC_PIC_TYPE_I
                || lock_params.pictureType == NV_ENC_PIC_TYPE_IDR;

            // Calculate frame duration from FPS
            let duration_us = 1_000_000 / u64::from(self.config.fps);

            // Unlock the bitstream
            let nvenc_unlock =
                self.api
                    .nvEncUnlockBitstream
                    .ok_or_else(|| VideoError::EncodingFailed {
                        reason: "nvEncUnlockBitstream function not available".to_string(),
                    })?;

            let status = unsafe { nvenc_unlock(self.encoder, lock_params.outputBitstream) };
            if status != NV_ENC_SUCCESS {
                tracing::warn!(
                    "nvEncUnlockBitstream failed: {} (status={})",
                    nvenc_status_to_string(status),
                    status
                );
            }

            tracing::debug!(
                is_keyframe = is_keyframe,
                size_bytes = data_size,
                timestamp_us = timestamp_us,
                "Encoded frame"
            );

            Ok(Some(EncodedPacket {
                data: encoded_data,
                is_keyframe,
                timestamp_us,
                duration_us,
            }))
        }
    }

    impl VideoEncoder for NvencEncoder {
        fn encode(&mut self, input: EncoderInput<'_>) -> Result<Option<EncodedPacket>, VideoError> {
            let (width, height) = match &input {
                EncoderInput::Cpu(frame) => (frame.width, frame.height),
                EncoderInput::GpuTexture { width, height, .. } => (*width, *height),
            };

            if width != self.config.width || height != self.config.height {
                return Err(VideoError::InvalidDimensions { width, height });
            }

            match input {
                EncoderInput::GpuTexture {
                    handle,
                    timestamp_us,
                    ..
                } => self.encode_gpu_texture(handle.0, width, height, timestamp_us),
                EncoderInput::Cpu(_frame) => {
                    // CPU encoding path not implemented - would need to create input buffers
                    // and copy data. For now, return an error to encourage zero-copy usage.
                    Err(VideoError::EncodingFailed {
                        reason: "CPU encoding not implemented for NVENC - use zero-copy GPU path"
                            .to_string(),
                    })
                }
            }
        }

        fn flush(&mut self) -> Result<Vec<EncodedPacket>, VideoError> {
            let nvenc_encode =
                self.api
                    .nvEncEncodePicture
                    .ok_or_else(|| VideoError::EncodingFailed {
                        reason: "nvEncEncodePicture function not available".to_string(),
                    })?;

            // Send EOS frame to drain encoder
            let mut pic_params = NV_ENC_PIC_PARAMS::new_versioned();
            pic_params.encodePicFlags = NV_ENC_PIC_FLAG_EOS;
            pic_params.outputBitstream = self.output_buffers[self.current_output_idx];

            let status = unsafe { nvenc_encode(self.encoder, &mut pic_params) };
            if status != NV_ENC_SUCCESS {
                tracing::warn!(
                    "EOS frame encoding failed: {} (status={})",
                    nvenc_status_to_string(status),
                    status
                );
                return Ok(vec![]);
            }

            // Collect any remaining packets
            let mut packets = Vec::new();

            // Try to read from both output buffers
            for _ in 0..self.output_buffers.len() {
                if let Ok(Some(packet)) = self.lock_and_read_bitstream(0) {
                    packets.push(packet);
                }
                self.current_output_idx = (self.current_output_idx + 1) % self.output_buffers.len();
            }

            tracing::info!("Flushed {} remaining packets from encoder", packets.len());
            Ok(packets)
        }

        fn config(&self) -> &EncoderConfig {
            &self.config
        }
    }

    impl Drop for NvencEncoder {
        fn drop(&mut self) {
            tracing::debug!("Destroying NVENC encoder session");

            // Unregister all resources
            if let Some(nvenc_unregister) = self.api.nvEncUnregisterResource {
                for (texture_ptr, registered_ptr) in &self.registered_resources {
                    let status = unsafe { nvenc_unregister(self.encoder, *registered_ptr) };
                    if status != NV_ENC_SUCCESS {
                        tracing::warn!(
                            "Failed to unregister texture {:p}: {} (status={})",
                            texture_ptr,
                            nvenc_status_to_string(status),
                            status
                        );
                    }
                }
            }

            // Destroy output buffers
            if let Some(nvenc_destroy_bitstream) = self.api.nvEncDestroyBitstreamBuffer {
                for (i, buffer) in self.output_buffers.iter().enumerate() {
                    let status = unsafe { nvenc_destroy_bitstream(self.encoder, *buffer) };
                    if status != NV_ENC_SUCCESS {
                        tracing::warn!(
                            "Failed to destroy output buffer {}: {} (status={})",
                            i,
                            nvenc_status_to_string(status),
                            status
                        );
                    }
                }
            }

            // Destroy encoder session
            if let Some(nvenc_destroy) = self.api.nvEncDestroyEncoder {
                let status = unsafe { nvenc_destroy(self.encoder) };
                if status != NV_ENC_SUCCESS {
                    tracing::warn!(
                        "Failed to destroy encoder: {} (status={})",
                        nvenc_status_to_string(status),
                        status
                    );
                }
            }

            tracing::debug!("NVENC encoder destroyed");
        }
    }
}

#[cfg(target_os = "windows")]
pub use windows::NvencEncoder;
