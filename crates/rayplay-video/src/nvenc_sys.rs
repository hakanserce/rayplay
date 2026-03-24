/// Raw FFI bindings for NVENC SDK (Windows only).
///
/// These types are manually defined to match the NVENC SDK without requiring
/// bindgen or build-time dependencies. NVENC is loaded dynamically from
/// `nvEncodeAPI64.dll` at runtime.
#[cfg(any(target_os = "windows", test))]
#[allow(
    non_camel_case_types,
    non_snake_case,
    clippy::enum_variant_names,
    clippy::missing_safety_doc,
    clippy::module_name_repetitions,
    clippy::pub_underscore_fields,
    clippy::similar_names,
    clippy::struct_field_names,
    clippy::unreadable_literal,
    clippy::upper_case_acronyms,
    clippy::cast_possible_truncation,
    dead_code
)]
pub(crate) mod ffi {
    use std::ffi::c_void;

    // NVENC API version information
    pub const NVENCAPI_MAJOR_VERSION: u32 = 12;
    pub const NVENCAPI_MINOR_VERSION: u32 = 2;
    pub const NVENCAPI_VERSION: u32 = (NVENCAPI_MAJOR_VERSION << 4) | NVENCAPI_MINOR_VERSION;

    // Version macro for structs
    pub const fn struct_ver<T>(ver: u32) -> u32 {
        std::mem::size_of::<T>() as u32 | (ver << 16) | (NVENCAPI_VERSION << 28)
    }

    // GUID type compatible with Windows GUID
    #[repr(C)]
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct GUID {
        pub data1: u32,
        pub data2: u16,
        pub data3: u16,
        pub data4: [u8; 8],
    }

    // NVENC codec GUIDs
    pub const NV_ENC_CODEC_H264_GUID: GUID = GUID {
        data1: 0x6bc82762,
        data2: 0x4e63,
        data3: 0x4ca4,
        data4: [0xaa, 0x85, 0x1e, 0x50, 0xf3, 0x21, 0xf6, 0xbf],
    };

    pub const NV_ENC_CODEC_HEVC_GUID: GUID = GUID {
        data1: 0x790cdc88,
        data2: 0x4522,
        data3: 0x4d7b,
        data4: [0x9c, 0x13, 0x09, 0x84, 0xbd, 0x71, 0x6c, 0x8d],
    };

    // NVENC preset GUIDs (P1 = lowest latency)
    pub const NV_ENC_PRESET_P1_GUID: GUID = GUID {
        data1: 0xfc0a8d3e,
        data2: 0x45f8,
        data3: 0x4cf8,
        data4: [0x80, 0xc7, 0x29, 0x8e, 0x5e, 0x24, 0x01, 0x4c],
    };

    // NVENC profile GUIDs
    pub const NV_ENC_H264_PROFILE_MAIN_GUID: GUID = GUID {
        data1: 0x60b5c1d4,
        data2: 0x67fe,
        data3: 0x4790,
        data4: [0x94, 0xd5, 0xc4, 0x72, 0x6d, 0x7b, 0x6e, 0x6d],
    };

    pub const NV_ENC_HEVC_PROFILE_MAIN_GUID: GUID = GUID {
        data1: 0xb514c39a,
        data2: 0xb55b,
        data3: 0x40fa,
        data4: [0x87, 0x8f, 0xf1, 0x25, 0x3b, 0x4d, 0xfd, 0x3d],
    };

    // Status codes
    pub type NVENCSTATUS = u32;
    pub const NV_ENC_SUCCESS: NVENCSTATUS = 0;
    pub const NV_ENC_ERR_NO_ENCODE_DEVICE: NVENCSTATUS = 1;
    pub const NV_ENC_ERR_UNSUPPORTED_PARAM: NVENCSTATUS = 2;
    pub const NV_ENC_ERR_OUT_OF_MEMORY: NVENCSTATUS = 3;
    pub const NV_ENC_ERR_INVALID_PARAM: NVENCSTATUS = 4;
    pub const NV_ENC_ERR_INVALID_CALL: NVENCSTATUS = 5;
    pub const NV_ENC_ERR_GENERIC: NVENCSTATUS = 6;
    pub const NV_ENC_ERR_INCOMPATIBLE_CLIENT_KEY: NVENCSTATUS = 7;
    pub const NV_ENC_ERR_UNIMPLEMENTED: NVENCSTATUS = 8;
    pub const NV_ENC_ERR_RESOURCE_REGISTER_FAILED: NVENCSTATUS = 9;
    pub const NV_ENC_ERR_RESOURCE_NOT_REGISTERED: NVENCSTATUS = 10;
    pub const NV_ENC_ERR_RESOURCE_NOT_MAPPED: NVENCSTATUS = 11;

    // Enums
    #[repr(u32)]
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum NV_ENC_INPUT_RESOURCE_TYPE {
        NV_ENC_INPUT_RESOURCE_TYPE_DIRECTX = 0,
        NV_ENC_INPUT_RESOURCE_TYPE_CUDADEVICEPTR = 1,
        NV_ENC_INPUT_RESOURCE_TYPE_CUDAARRAY = 2,
        NV_ENC_INPUT_RESOURCE_TYPE_OPENGL_TEX = 3,
    }

    #[repr(u32)]
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum NV_ENC_BUFFER_FORMAT {
        NV_ENC_BUFFER_FORMAT_UNDEFINED = 0x0000_0000,
        NV_ENC_BUFFER_FORMAT_NV12 = 0x0000_0001,
        NV_ENC_BUFFER_FORMAT_YV12 = 0x0000_0010,
        NV_ENC_BUFFER_FORMAT_IYUV = 0x0000_0100,
        NV_ENC_BUFFER_FORMAT_YUV444 = 0x0000_1000,
        NV_ENC_BUFFER_FORMAT_YUV420_10BIT = 0x0100_0000,
        NV_ENC_BUFFER_FORMAT_ARGB = 0x0200_0000,
        NV_ENC_BUFFER_FORMAT_ARGB10 = 0x0400_0000,
        NV_ENC_BUFFER_FORMAT_AYUV = 0x0800_0000,
        NV_ENC_BUFFER_FORMAT_ABGR = 0x1000_0000,
        NV_ENC_BUFFER_FORMAT_ABGR10 = 0x2000_0000,
    }

    #[repr(u32)]
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum NV_ENC_PIC_TYPE {
        NV_ENC_PIC_TYPE_P = 0,
        NV_ENC_PIC_TYPE_B = 1,
        NV_ENC_PIC_TYPE_I = 2,
        NV_ENC_PIC_TYPE_IDR = 3,
        NV_ENC_PIC_TYPE_BI = 4,
        NV_ENC_PIC_TYPE_SKIPPED = 5,
        NV_ENC_PIC_TYPE_INTRA_REFRESH = 6,
        NV_ENC_PIC_TYPE_UNKNOWN = 0xFF,
    }

    #[repr(u32)]
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum NV_ENC_DEVICE_TYPE {
        NV_ENC_DEVICE_TYPE_DIRECTX = 0,
        NV_ENC_DEVICE_TYPE_CUDA = 1,
        NV_ENC_DEVICE_TYPE_OPENGL = 2,
    }

    #[repr(u32)]
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum NV_ENC_TUNING_INFO {
        NV_ENC_TUNING_INFO_UNDEFINED = 0,
        NV_ENC_TUNING_INFO_HIGH_QUALITY = 1,
        NV_ENC_TUNING_INFO_LOW_LATENCY = 2,
        NV_ENC_TUNING_INFO_ULTRA_LOW_LATENCY = 3,
        NV_ENC_TUNING_INFO_LOSSLESS = 4,
    }

    #[repr(u32)]
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum NV_ENC_MULTI_PASS {
        NV_ENC_MULTI_PASS_DISABLED = 0,
        NV_ENC_MULTI_PASS_QUARTER_RESOLUTION = 1,
        NV_ENC_MULTI_PASS_FULL_RESOLUTION = 2,
    }

    // Picture flags
    pub const NV_ENC_PIC_FLAG_FORCEINTRA: u32 = 0x1;
    pub const NV_ENC_PIC_FLAG_FORCEIDR: u32 = 0x2;
    pub const NV_ENC_PIC_FLAG_OUTPUT_SPSPPS: u32 = 0x4;
    pub const NV_ENC_PIC_FLAG_EOS: u32 = 0x8;

    // Rate control modes
    pub const NV_ENC_PARAMS_RC_CONSTQP: u32 = 0x0;
    pub const NV_ENC_PARAMS_RC_VBR: u32 = 0x1;
    pub const NV_ENC_PARAMS_RC_CBR: u32 = 0x2;

    // Structs (with version fields set to appropriate values)
    #[repr(C)]
    pub struct NV_ENC_OPEN_ENCODE_SESSION_EX_PARAMS {
        pub version: u32,
        pub deviceType: NV_ENC_DEVICE_TYPE,
        pub device: *mut c_void,
        pub reserved: *mut c_void,
        pub apiVersion: u32,
        pub reserved1: [*mut c_void; 56],
        pub reserved2: [u32; 64],
    }

    impl Default for NV_ENC_OPEN_ENCODE_SESSION_EX_PARAMS {
        fn default() -> Self {
            Self {
                version: struct_ver::<Self>(1),
                deviceType: NV_ENC_DEVICE_TYPE::NV_ENC_DEVICE_TYPE_DIRECTX,
                device: std::ptr::null_mut(),
                reserved: std::ptr::null_mut(),
                apiVersion: NVENCAPI_VERSION,
                reserved1: [std::ptr::null_mut(); 56],
                reserved2: [0; 64],
            }
        }
    }

    #[repr(C)]
    pub struct NV_ENC_INITIALIZE_PARAMS {
        pub version: u32,
        pub encodeGUID: GUID,
        pub presetGUID: GUID,
        pub encodeWidth: u32,
        pub encodeHeight: u32,
        pub darWidth: u32,
        pub darHeight: u32,
        pub frameRateNum: u32,
        pub frameRateDen: u32,
        pub enableEncodeAsync: u32,
        pub enablePTD: u32,
        pub reportSliceOffsets: u32,
        pub enableSubFrameWrite: u32,
        pub enableExternalMEHints: u32,
        pub enableMEOnlyMode: u32,
        pub enableWeightedPrediction: u32,
        pub enableOutputInVidmem: u32,
        pub reservedBitFields: u32,
        pub privDataSize: u32,
        pub privData: *mut c_void,
        pub encodeConfig: *mut NV_ENC_CONFIG,
        pub maxEncodeWidth: u32,
        pub maxEncodeHeight: u32,
        pub reserved1: [u32; 27],
        pub reserved2: [*mut c_void; 64],
    }

    impl Default for NV_ENC_INITIALIZE_PARAMS {
        fn default() -> Self {
            Self {
                version: struct_ver::<Self>(5),
                encodeGUID: GUID {
                    data1: 0,
                    data2: 0,
                    data3: 0,
                    data4: [0; 8],
                },
                presetGUID: GUID {
                    data1: 0,
                    data2: 0,
                    data3: 0,
                    data4: [0; 8],
                },
                encodeWidth: 0,
                encodeHeight: 0,
                darWidth: 0,
                darHeight: 0,
                frameRateNum: 0,
                frameRateDen: 1,
                enableEncodeAsync: 0,
                enablePTD: 1,
                reportSliceOffsets: 0,
                enableSubFrameWrite: 0,
                enableExternalMEHints: 0,
                enableMEOnlyMode: 0,
                enableWeightedPrediction: 0,
                enableOutputInVidmem: 0,
                reservedBitFields: 0,
                privDataSize: 0,
                privData: std::ptr::null_mut(),
                encodeConfig: std::ptr::null_mut(),
                maxEncodeWidth: 0,
                maxEncodeHeight: 0,
                reserved1: [0; 27],
                reserved2: [std::ptr::null_mut(); 64],
            }
        }
    }

    #[repr(C)]
    pub struct NV_ENC_RC_PARAMS {
        pub version: u32,
        pub rateControlMode: u32,
        pub constQP: NV_ENC_QP,
        pub averageBitRate: u32,
        pub maxBitRate: u32,
        pub vbvBufferSize: u32,
        pub vbvInitialDelay: u32,
        pub enableMinQP: u32,
        pub enableMaxQP: u32,
        pub enableInitialRCQP: u32,
        pub enableAQ: u32,
        pub enableLookahead: u32,
        pub disableIadapt: u32,
        pub disableBadapt: u32,
        pub enableTemporalAQ: u32,
        pub zeroReorderDelay: u32,
        pub enableNonRefP: u32,
        pub strictGOPTarget: u32,
        pub aqStrength: u32,
        pub minQPP: u32,
        pub minQPB: u32,
        pub minQPI: u32,
        pub maxQPP: u32,
        pub maxQPB: u32,
        pub maxQPI: u32,
        pub initialRCQPP: u32,
        pub initialRCQPB: u32,
        pub initialRCQPI: u32,
        pub temporalLayerIdxMask: u32,
        pub baseLayerBitRate: u32,
        pub temporalLayerBitRate: [u32; 8],
        pub targetQuality: u32,
        pub targetQualityLSB: u32,
        pub lookaheadDepth: u32,
        pub lowDelayKeyFrameScale: u32,
        pub targetFrameSizeMapDeltaQPMax: u32,
        pub targetFrameSizeMapDeltaQPMin: u32,
        pub reserved: [u32; 10],
    }

    #[repr(C)]
    pub struct NV_ENC_QP {
        pub qpInterP: u32,
        pub qpInterB: u32,
        pub qpIntra: u32,
    }

    #[repr(C)]
    pub struct NV_ENC_CONFIG {
        pub version: u32,
        pub profileGUID: GUID,
        pub gopLength: u32,
        pub frameIntervalP: u32,
        pub monoChromeEncoding: u32,
        pub frameFieldMode: u32,
        pub mvPrecision: u32,
        pub rcParams: NV_ENC_RC_PARAMS,
        pub encodeCodecConfig: NV_ENC_CODEC_CONFIG,
        pub reserved: [u32; 278],
        pub reserved2: [*mut c_void; 64],
    }

    #[repr(C)]
    pub union NV_ENC_CODEC_CONFIG {
        pub h264Config: NV_ENC_CONFIG_H264,
        pub hevcConfig: NV_ENC_CONFIG_HEVC,
        pub reserved: [u32; 320],
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct NV_ENC_CONFIG_H264 {
        pub enableTemporalSVC: u32,
        pub enableStereoMVC: u32,
        pub hierarchicalPFrames: u32,
        pub hierarchicalBFrames: u32,
        pub outputBufferingPeriodSEI: u32,
        pub outputPictureTimingSEI: u32,
        pub outputAUD: u32,
        pub disableSPSPPS: u32,
        pub outputFramePackingSEI: u32,
        pub outputRecoveryPointSEI: u32,
        pub enableIntraRefresh: u32,
        pub enableConstrainedEncoding: u32,
        pub repeatSPSPPS: u32,
        pub enableVFR: u32,
        pub enableLTR: u32,
        pub qpPrimeYZeroTransformBypassFlag: u32,
        pub useConstrainedIntraPred: u32,
        pub reserved1: [u32; 15],
        pub reserved2: [*mut c_void; 64],
    }

    impl Default for NV_ENC_CONFIG_H264 {
        fn default() -> Self {
            // SAFETY: All fields are primitive types and raw pointers — zero is valid.
            unsafe { std::mem::zeroed() }
        }
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct NV_ENC_CONFIG_HEVC {
        pub level: u32,
        pub tier: u32,
        pub minCUSize: u32,
        pub maxCUSize: u32,
        pub useConstrainedIntraPred: u32,
        pub disableDeblockAcrossSliceBoundary: u32,
        pub outputBufferingPeriodSEI: u32,
        pub outputPictureTimingSEI: u32,
        pub outputAUD: u32,
        pub enableLTR: u32,
        pub disableSPSPPS: u32,
        pub repeatSPSPPS: u32,
        pub enableIntraRefresh: u32,
        pub chromaFormatIDC: u32,
        pub pixelBitDepthMinus8: u32,
        pub enableFillerDataInsertion: u32,
        pub enableConstrainedEncoding: u32,
        pub reserved: [u32; 15],
        pub reserved1: [*mut c_void; 64],
    }

    impl Default for NV_ENC_CONFIG_HEVC {
        fn default() -> Self {
            // SAFETY: All fields are primitive types and raw pointers — zero is valid.
            unsafe { std::mem::zeroed() }
        }
    }

    impl Default for NV_ENC_CONFIG {
        fn default() -> Self {
            Self {
                version: struct_ver::<Self>(7),
                profileGUID: GUID {
                    data1: 0,
                    data2: 0,
                    data3: 0,
                    data4: [0; 8],
                },
                gopLength: 60,
                frameIntervalP: 1,
                monoChromeEncoding: 0,
                frameFieldMode: 0,
                mvPrecision: 0,
                rcParams: NV_ENC_RC_PARAMS {
                    version: struct_ver::<NV_ENC_RC_PARAMS>(1),
                    rateControlMode: NV_ENC_PARAMS_RC_VBR,
                    constQP: NV_ENC_QP {
                        qpInterP: 25,
                        qpInterB: 27,
                        qpIntra: 23,
                    },
                    averageBitRate: 0,
                    maxBitRate: 0,
                    vbvBufferSize: 0,
                    vbvInitialDelay: 0,
                    enableMinQP: 0,
                    enableMaxQP: 0,
                    enableInitialRCQP: 0,
                    enableAQ: 1,
                    enableLookahead: 0,
                    disableIadapt: 0,
                    disableBadapt: 0,
                    enableTemporalAQ: 0,
                    zeroReorderDelay: 1,
                    enableNonRefP: 0,
                    strictGOPTarget: 0,
                    aqStrength: 0,
                    minQPP: 0,
                    minQPB: 0,
                    minQPI: 0,
                    maxQPP: 51,
                    maxQPB: 51,
                    maxQPI: 51,
                    initialRCQPP: 25,
                    initialRCQPB: 27,
                    initialRCQPI: 23,
                    temporalLayerIdxMask: 0,
                    baseLayerBitRate: 0,
                    temporalLayerBitRate: [0; 8],
                    targetQuality: 0,
                    targetQualityLSB: 0,
                    lookaheadDepth: 0,
                    lowDelayKeyFrameScale: 1,
                    targetFrameSizeMapDeltaQPMax: 0,
                    targetFrameSizeMapDeltaQPMin: 0,
                    reserved: [0; 10],
                },
                encodeCodecConfig: NV_ENC_CODEC_CONFIG {
                    hevcConfig: NV_ENC_CONFIG_HEVC {
                        level: 0,
                        tier: 0,
                        minCUSize: 0,
                        maxCUSize: 0,
                        useConstrainedIntraPred: 0,
                        disableDeblockAcrossSliceBoundary: 0,
                        outputBufferingPeriodSEI: 0,
                        outputPictureTimingSEI: 0,
                        outputAUD: 0,
                        enableLTR: 0,
                        disableSPSPPS: 0,
                        repeatSPSPPS: 1,
                        enableIntraRefresh: 0,
                        chromaFormatIDC: 1,
                        pixelBitDepthMinus8: 0,
                        enableFillerDataInsertion: 0,
                        enableConstrainedEncoding: 0,
                        reserved: [0; 15],
                        reserved1: [std::ptr::null_mut(); 64],
                    },
                },
                reserved: [0; 278],
                reserved2: [std::ptr::null_mut(); 64],
            }
        }
    }

    #[repr(C)]
    pub struct NV_ENC_PRESET_CONFIG {
        pub version: u32,
        pub presetCfg: NV_ENC_CONFIG,
        pub reserved1: [*mut c_void; 256],
    }

    impl Default for NV_ENC_PRESET_CONFIG {
        fn default() -> Self {
            Self {
                version: struct_ver::<Self>(4),
                presetCfg: NV_ENC_CONFIG::default(),
                reserved1: [std::ptr::null_mut(); 256],
            }
        }
    }

    #[repr(C)]
    pub struct NV_ENC_REGISTER_RESOURCE {
        pub version: u32,
        pub resourceType: NV_ENC_INPUT_RESOURCE_TYPE,
        pub width: u32,
        pub height: u32,
        pub pitch: u32,
        pub subResourceIndex: u32,
        pub resourceToRegister: *mut c_void,
        pub registeredResource: *mut c_void,
        pub bufferFormat: NV_ENC_BUFFER_FORMAT,
        pub bufferUsage: u32,
        pub reserved1: [u32; 4],
        pub reserved2: [*mut c_void; 62],
    }

    impl Default for NV_ENC_REGISTER_RESOURCE {
        fn default() -> Self {
            Self {
                version: struct_ver::<Self>(3),
                resourceType: NV_ENC_INPUT_RESOURCE_TYPE::NV_ENC_INPUT_RESOURCE_TYPE_DIRECTX,
                width: 0,
                height: 0,
                pitch: 0,
                subResourceIndex: 0,
                resourceToRegister: std::ptr::null_mut(),
                registeredResource: std::ptr::null_mut(),
                bufferFormat: NV_ENC_BUFFER_FORMAT::NV_ENC_BUFFER_FORMAT_UNDEFINED,
                bufferUsage: 1, // NV_ENC_INPUT_IMAGE
                reserved1: [0; 4],
                reserved2: [std::ptr::null_mut(); 62],
            }
        }
    }

    #[repr(C)]
    pub struct NV_ENC_MAP_INPUT_RESOURCE {
        pub version: u32,
        pub subResourceIndex: u32,
        pub inputResource: *mut c_void,
        pub registeredResource: *mut c_void,
        pub mappedResource: *mut c_void,
        pub mappedBufferFmt: NV_ENC_BUFFER_FORMAT,
        pub reserved1: [u32; 6],
        pub reserved2: [*mut c_void; 57],
    }

    impl Default for NV_ENC_MAP_INPUT_RESOURCE {
        fn default() -> Self {
            Self {
                version: struct_ver::<Self>(4),
                subResourceIndex: 0,
                inputResource: std::ptr::null_mut(),
                registeredResource: std::ptr::null_mut(),
                mappedResource: std::ptr::null_mut(),
                mappedBufferFmt: NV_ENC_BUFFER_FORMAT::NV_ENC_BUFFER_FORMAT_UNDEFINED,
                reserved1: [0; 6],
                reserved2: [std::ptr::null_mut(); 57],
            }
        }
    }

    #[repr(C)]
    pub struct NV_ENC_PIC_PARAMS {
        pub version: u32,
        pub inputWidth: u32,
        pub inputHeight: u32,
        pub inputPitch: u32,
        pub encodePicFlags: u32,
        pub frameIdx: u32,
        pub inputTimeStamp: u64,
        pub inputDuration: u64,
        pub inputBuffer: *mut c_void,
        pub outputBitstream: *mut c_void,
        pub completionEvent: *mut c_void,
        pub bufferFmt: NV_ENC_BUFFER_FORMAT,
        pub pictureStruct: u32,
        pub pictureType: NV_ENC_PIC_TYPE,
        pub codecPicParams: NV_ENC_CODEC_PIC_PARAMS,
        pub reserved1: [u32; 6],
        pub reserved2: [*mut c_void; 58],
    }

    #[repr(C)]
    pub union NV_ENC_CODEC_PIC_PARAMS {
        pub h264PicParams: NV_ENC_PIC_PARAMS_H264,
        pub hevcPicParams: NV_ENC_PIC_PARAMS_HEVC,
        pub reserved: [u32; 256],
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct NV_ENC_PIC_PARAMS_H264 {
        pub displayPOCSyntax: u32,
        pub reserved3: u32,
        pub refPicFlag: u32,
        pub colourPlaneId: u32,
        pub forceIntraRefreshWithFrameCnt: u32,
        pub constrainedFrame: u32,
        pub sliceModeData: u32,
        pub ltrMarkFrame: u32,
        pub ltrUseFrames: u32,
        pub reservedBitFields: u32,
        pub sliceMode: u32,
        pub sliceType: [u32; 3],
        pub reserved: [u32; 11],
        pub reserved1: [*mut c_void; 62],
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct NV_ENC_PIC_PARAMS_HEVC {
        pub displayPOCSyntax: u32,
        pub refPicFlag: u32,
        pub temporalId: u32,
        pub forceIntraRefreshWithFrameCnt: u32,
        pub constrainedFrame: u32,
        pub sliceModeData: u32,
        pub ltrMarkFrame: u32,
        pub ltrUseFrames: u32,
        pub reservedBitFields: u32,
        pub sliceMode: u32,
        pub sliceType: [u32; 3],
        pub reserved: [u32; 12],
        pub reserved1: [*mut c_void; 61],
    }

    impl Default for NV_ENC_PIC_PARAMS {
        fn default() -> Self {
            Self {
                version: struct_ver::<Self>(4),
                inputWidth: 0,
                inputHeight: 0,
                inputPitch: 0,
                encodePicFlags: 0,
                frameIdx: 0,
                inputTimeStamp: 0,
                inputDuration: 0,
                inputBuffer: std::ptr::null_mut(),
                outputBitstream: std::ptr::null_mut(),
                completionEvent: std::ptr::null_mut(),
                bufferFmt: NV_ENC_BUFFER_FORMAT::NV_ENC_BUFFER_FORMAT_UNDEFINED,
                pictureStruct: 0,
                pictureType: NV_ENC_PIC_TYPE::NV_ENC_PIC_TYPE_UNKNOWN,
                codecPicParams: NV_ENC_CODEC_PIC_PARAMS { reserved: [0; 256] },
                reserved1: [0; 6],
                reserved2: [std::ptr::null_mut(); 58],
            }
        }
    }

    #[repr(C)]
    pub struct NV_ENC_LOCK_BITSTREAM {
        pub version: u32,
        pub doNotWait: u32,
        pub ltrFrame: u32,
        pub reservedBitFields: u32,
        pub outputBitstream: *mut c_void,
        pub sliceOffsets: *mut u32,
        pub frameIdx: u32,
        pub hwEncodeStatus: u32,
        pub numSlices: u32,
        pub bitstreamSizeInBytes: u32,
        pub outputTimeStamp: u64,
        pub outputDuration: u64,
        pub bitstreamBufferPtr: *mut c_void,
        pub pictureType: NV_ENC_PIC_TYPE,
        pub pictureStruct: u32,
        pub frameAvgQP: u32,
        pub frameSatd: u32,
        pub ltrFrameIdx: u32,
        pub ltrFrameBitmap: u32,
        pub reserved: [u32; 12],
        pub reserved1: [*mut c_void; 58],
    }

    impl Default for NV_ENC_LOCK_BITSTREAM {
        fn default() -> Self {
            Self {
                version: struct_ver::<Self>(1),
                doNotWait: 0,
                ltrFrame: 0,
                reservedBitFields: 0,
                outputBitstream: std::ptr::null_mut(),
                sliceOffsets: std::ptr::null_mut(),
                frameIdx: 0,
                hwEncodeStatus: 0,
                numSlices: 0,
                bitstreamSizeInBytes: 0,
                outputTimeStamp: 0,
                outputDuration: 0,
                bitstreamBufferPtr: std::ptr::null_mut(),
                pictureType: NV_ENC_PIC_TYPE::NV_ENC_PIC_TYPE_UNKNOWN,
                pictureStruct: 0,
                frameAvgQP: 0,
                frameSatd: 0,
                ltrFrameIdx: 0,
                ltrFrameBitmap: 0,
                reserved: [0; 12],
                reserved1: [std::ptr::null_mut(); 58],
            }
        }
    }

    #[repr(C)]
    pub struct NV_ENC_CREATE_BITSTREAM_BUFFER {
        pub version: u32,
        pub size: u32,
        pub memoryHeap: u32,
        pub reserved: u32,
        pub bitstreamBuffer: *mut c_void,
        pub bitstreamBufferPtr: *mut c_void,
        pub reserved1: [u32; 58],
        pub reserved2: [*mut c_void; 64],
    }

    impl Default for NV_ENC_CREATE_BITSTREAM_BUFFER {
        fn default() -> Self {
            Self {
                version: struct_ver::<Self>(1),
                size: 0,
                memoryHeap: 0,
                reserved: 0,
                bitstreamBuffer: std::ptr::null_mut(),
                bitstreamBufferPtr: std::ptr::null_mut(),
                reserved1: [0; 58],
                reserved2: [std::ptr::null_mut(); 64],
            }
        }
    }

    // Function pointer types
    pub type PFnNvEncOpenEncodeSessionEx = unsafe extern "C" fn(
        params: *mut NV_ENC_OPEN_ENCODE_SESSION_EX_PARAMS,
        encoder: *mut *mut c_void,
    ) -> NVENCSTATUS;

    pub type PFnNvEncGetEncodeGUIDCount =
        unsafe extern "C" fn(encoder: *mut c_void, count: *mut u32) -> NVENCSTATUS;

    pub type PFnNvEncGetEncodeGUIDs = unsafe extern "C" fn(
        encoder: *mut c_void,
        guids: *mut GUID,
        arraysize: u32,
        count: *mut u32,
    ) -> NVENCSTATUS;

    pub type PFnNvEncInitializeEncoder = unsafe extern "C" fn(
        encoder: *mut c_void,
        params: *mut NV_ENC_INITIALIZE_PARAMS,
    ) -> NVENCSTATUS;

    pub type PFnNvEncCreateInputBuffer = unsafe extern "C" fn(
        encoder: *mut c_void,
        params: *mut NV_ENC_CREATE_BITSTREAM_BUFFER, // Using bitstream buffer struct for input too
    ) -> NVENCSTATUS;

    pub type PFnNvEncDestroyInputBuffer =
        unsafe extern "C" fn(encoder: *mut c_void, buffer: *mut c_void) -> NVENCSTATUS;

    pub type PFnNvEncCreateBitstreamBuffer = unsafe extern "C" fn(
        encoder: *mut c_void,
        params: *mut NV_ENC_CREATE_BITSTREAM_BUFFER,
    ) -> NVENCSTATUS;

    pub type PFnNvEncDestroyBitstreamBuffer =
        unsafe extern "C" fn(encoder: *mut c_void, buffer: *mut c_void) -> NVENCSTATUS;

    pub type PFnNvEncRegisterResource = unsafe extern "C" fn(
        encoder: *mut c_void,
        params: *mut NV_ENC_REGISTER_RESOURCE,
    ) -> NVENCSTATUS;

    pub type PFnNvEncUnregisterResource =
        unsafe extern "C" fn(encoder: *mut c_void, resource: *mut c_void) -> NVENCSTATUS;

    pub type PFnNvEncMapInputResource = unsafe extern "C" fn(
        encoder: *mut c_void,
        params: *mut NV_ENC_MAP_INPUT_RESOURCE,
    ) -> NVENCSTATUS;

    pub type PFnNvEncUnmapInputResource =
        unsafe extern "C" fn(encoder: *mut c_void, mapped_resource: *mut c_void) -> NVENCSTATUS;

    pub type PFnNvEncEncodePicture =
        unsafe extern "C" fn(encoder: *mut c_void, params: *mut NV_ENC_PIC_PARAMS) -> NVENCSTATUS;

    pub type PFnNvEncLockBitstream = unsafe extern "C" fn(
        encoder: *mut c_void,
        params: *mut NV_ENC_LOCK_BITSTREAM,
    ) -> NVENCSTATUS;

    pub type PFnNvEncUnlockBitstream =
        unsafe extern "C" fn(encoder: *mut c_void, buffer: *mut c_void) -> NVENCSTATUS;

    pub type PFnNvEncDestroyEncoder = unsafe extern "C" fn(encoder: *mut c_void) -> NVENCSTATUS;

    pub type PFnNvEncGetEncodePresetConfigEx = unsafe extern "C" fn(
        encoder: *mut c_void,
        encode_guid: GUID,
        preset_guid: GUID,
        tuning_info: NV_ENC_TUNING_INFO,
        preset_config: *mut NV_ENC_PRESET_CONFIG,
    ) -> NVENCSTATUS;

    // Function list struct — must match NVENC SDK 12.2 layout exactly.
    //
    // Each named field occupies one function-pointer slot (8 bytes on 64-bit).
    // The SDK fills this struct by offset, so even unused slots must be present
    // at the correct position.  Unused slots use `*mut c_void` placeholders.
    //
    // Reference: nvEncodeAPI.h `NV_ENCODE_API_FUNCTION_LIST` (SDK 12.2)
    #[repr(C)]
    pub struct NV_ENCODE_API_FUNCTION_LIST {
        pub version: u32,  // +0
        pub reserved: u32, // +4
        // ── slots 1–7 (correct in previous version) ──
        pub _nvEncOpenEncodeSession: *mut c_void, // slot  1 (deprecated)
        pub nvEncGetEncodeGUIDCount: Option<PFnNvEncGetEncodeGUIDCount>, // slot  2
        pub nvEncGetEncodeGUIDs: Option<PFnNvEncGetEncodeGUIDs>, // slot  3
        pub nvEncGetEncodeProfileGUIDCount: Option<PFnNvEncGetEncodeGUIDCount>, // slot  4
        pub nvEncGetEncodeProfileGUIDs: Option<PFnNvEncGetEncodeGUIDs>, // slot  5
        pub nvEncGetInputFormatCount: Option<PFnNvEncGetEncodeGUIDCount>, // slot  6
        pub nvEncGetInputFormats: Option<PFnNvEncGetEncodeGUIDs>, // slot  7
        // ── slot 8 was missing ──
        pub _nvEncGetEncodeCaps: *mut c_void, // slot  8
        // ── slots 9–11 (shifted by 1 in previous version) ──
        pub nvEncGetEncodePresetCount: Option<PFnNvEncGetEncodeGUIDCount>, // slot  9
        pub nvEncGetEncodePresetGUIDs: Option<PFnNvEncGetEncodeGUIDs>,     // slot 10
        pub _nvEncGetEncodePresetConfig: *mut c_void, // slot 11 (non-Ex, unused)
        // ── slots 12–16 (were correct) ──
        pub nvEncInitializeEncoder: Option<PFnNvEncInitializeEncoder>, // slot 12
        pub nvEncCreateInputBuffer: Option<PFnNvEncCreateInputBuffer>, // slot 13
        pub nvEncDestroyInputBuffer: Option<PFnNvEncDestroyInputBuffer>, // slot 14
        pub nvEncCreateBitstreamBuffer: Option<PFnNvEncCreateBitstreamBuffer>, // slot 15
        pub nvEncDestroyBitstreamBuffer: Option<PFnNvEncDestroyBitstreamBuffer>, // slot 16
        // ── slots 17–19 (were at wrong positions) ──
        pub nvEncEncodePicture: Option<PFnNvEncEncodePicture>, // slot 17
        pub nvEncLockBitstream: Option<PFnNvEncLockBitstream>, // slot 18
        pub nvEncUnlockBitstream: Option<PFnNvEncUnlockBitstream>, // slot 19
        // ── slots 20–25 (were entirely missing) ──
        pub _nvEncLockInputBuffer: *mut c_void,      // slot 20
        pub _nvEncUnlockInputBuffer: *mut c_void,    // slot 21
        pub _nvEncGetEncodeStats: *mut c_void,       // slot 22
        pub _nvEncGetSequenceParams: *mut c_void,    // slot 23
        pub _nvEncRegisterAsyncEvent: *mut c_void,   // slot 24
        pub _nvEncUnregisterAsyncEvent: *mut c_void, // slot 25
        // ── slots 26–28 (were at wrong positions) ──
        pub nvEncMapInputResource: Option<PFnNvEncMapInputResource>, // slot 26
        pub nvEncUnmapInputResource: Option<PFnNvEncUnmapInputResource>, // slot 27
        pub nvEncDestroyEncoder: Option<PFnNvEncDestroyEncoder>,     // slot 28
        // ── slots 29–33 (were missing) ──
        pub _nvEncInvalidateRefFrames: *mut c_void, // slot 29
        pub nvEncOpenEncodeSessionEx: Option<PFnNvEncOpenEncodeSessionEx>, // slot 30
        pub nvEncRegisterResource: Option<PFnNvEncRegisterResource>, // slot 31
        pub nvEncUnregisterResource: Option<PFnNvEncUnregisterResource>, // slot 32
        pub _nvEncReconfigureEncoder: *mut c_void,  // slot 33
        // ── slot 34 (reserved in SDK) ──
        pub _reserved1: *mut c_void, // slot 34
        // ── slots 35–39 (unused newer functions) ──
        pub _nvEncCreateMVBuffer: *mut c_void,  // slot 35
        pub _nvEncDestroyMVBuffer: *mut c_void, // slot 36
        pub _nvEncRunMotionEstimationOnly: *mut c_void, // slot 37
        pub _nvEncGetLastErrorString: *mut c_void, // slot 38
        pub _nvEncSetIOCudaStreams: *mut c_void, // slot 39
        // ── slot 40: the preset config Ex function we actually use ──
        pub nvEncGetEncodePresetConfigEx: Option<PFnNvEncGetEncodePresetConfigEx>, // slot 40
        // ── slots 41–42 (unused newer functions) ──
        pub _nvEncGetSequenceParamEx: *mut c_void, // slot 41
        pub _nvEncLookaheadPicture: *mut c_void,   // slot 42
        // ── remaining reserved slots to reach 319 total function pointer slots ──
        pub _reserved2: [*mut c_void; 277], // slots 43–319
    }

    impl Default for NV_ENCODE_API_FUNCTION_LIST {
        fn default() -> Self {
            Self {
                version: struct_ver::<Self>(2),
                reserved: 0,
                _nvEncOpenEncodeSession: std::ptr::null_mut(),
                nvEncGetEncodeGUIDCount: None,
                nvEncGetEncodeGUIDs: None,
                nvEncGetEncodeProfileGUIDCount: None,
                nvEncGetEncodeProfileGUIDs: None,
                nvEncGetInputFormatCount: None,
                nvEncGetInputFormats: None,
                _nvEncGetEncodeCaps: std::ptr::null_mut(),
                nvEncGetEncodePresetCount: None,
                nvEncGetEncodePresetGUIDs: None,
                _nvEncGetEncodePresetConfig: std::ptr::null_mut(),
                nvEncInitializeEncoder: None,
                nvEncCreateInputBuffer: None,
                nvEncDestroyInputBuffer: None,
                nvEncCreateBitstreamBuffer: None,
                nvEncDestroyBitstreamBuffer: None,
                nvEncEncodePicture: None,
                nvEncLockBitstream: None,
                nvEncUnlockBitstream: None,
                _nvEncLockInputBuffer: std::ptr::null_mut(),
                _nvEncUnlockInputBuffer: std::ptr::null_mut(),
                _nvEncGetEncodeStats: std::ptr::null_mut(),
                _nvEncGetSequenceParams: std::ptr::null_mut(),
                _nvEncRegisterAsyncEvent: std::ptr::null_mut(),
                _nvEncUnregisterAsyncEvent: std::ptr::null_mut(),
                nvEncMapInputResource: None,
                nvEncUnmapInputResource: None,
                nvEncDestroyEncoder: None,
                _nvEncInvalidateRefFrames: std::ptr::null_mut(),
                nvEncOpenEncodeSessionEx: None,
                nvEncRegisterResource: None,
                nvEncUnregisterResource: None,
                _nvEncReconfigureEncoder: std::ptr::null_mut(),
                _reserved1: std::ptr::null_mut(),
                _nvEncCreateMVBuffer: std::ptr::null_mut(),
                _nvEncDestroyMVBuffer: std::ptr::null_mut(),
                _nvEncRunMotionEstimationOnly: std::ptr::null_mut(),
                _nvEncGetLastErrorString: std::ptr::null_mut(),
                _nvEncSetIOCudaStreams: std::ptr::null_mut(),
                nvEncGetEncodePresetConfigEx: None,
                _nvEncGetSequenceParamEx: std::ptr::null_mut(),
                _nvEncLookaheadPicture: std::ptr::null_mut(),
                _reserved2: [std::ptr::null_mut(); 277],
            }
        }
    }

    // Entry point function type
    pub type PFnNvEncodeAPICreateInstance =
        unsafe extern "C" fn(function_list: *mut NV_ENCODE_API_FUNCTION_LIST) -> NVENCSTATUS;

    /// Converts NVENC status code to human-readable string.
    pub fn nvenc_status_to_string(status: NVENCSTATUS) -> &'static str {
        match status {
            NV_ENC_SUCCESS => "NV_ENC_SUCCESS",
            NV_ENC_ERR_NO_ENCODE_DEVICE => "NV_ENC_ERR_NO_ENCODE_DEVICE",
            NV_ENC_ERR_UNSUPPORTED_PARAM => "NV_ENC_ERR_UNSUPPORTED_PARAM",
            NV_ENC_ERR_OUT_OF_MEMORY => "NV_ENC_ERR_OUT_OF_MEMORY",
            NV_ENC_ERR_INVALID_PARAM => "NV_ENC_ERR_INVALID_PARAM",
            NV_ENC_ERR_INVALID_CALL => "NV_ENC_ERR_INVALID_CALL",
            NV_ENC_ERR_GENERIC => "NV_ENC_ERR_GENERIC",
            NV_ENC_ERR_INCOMPATIBLE_CLIENT_KEY => "NV_ENC_ERR_INCOMPATIBLE_CLIENT_KEY",
            NV_ENC_ERR_UNIMPLEMENTED => "NV_ENC_ERR_UNIMPLEMENTED",
            NV_ENC_ERR_RESOURCE_REGISTER_FAILED => "NV_ENC_ERR_RESOURCE_REGISTER_FAILED",
            NV_ENC_ERR_RESOURCE_NOT_REGISTERED => "NV_ENC_ERR_RESOURCE_NOT_REGISTERED",
            NV_ENC_ERR_RESOURCE_NOT_MAPPED => "NV_ENC_ERR_RESOURCE_NOT_MAPPED",
            _ => "UNKNOWN_NVENC_ERROR",
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::ffi::c_void;
        use std::mem;

        const PTR: usize = mem::size_of::<*mut c_void>();

        // ── struct total size ──

        #[test]
        fn function_list_total_size() {
            // version(4) + reserved(4) + 319 function-pointer slots
            let expected = 4 + 4 + 319 * PTR;
            assert_eq!(
                mem::size_of::<NV_ENCODE_API_FUNCTION_LIST>(),
                expected,
                "NV_ENCODE_API_FUNCTION_LIST size mismatch — SDK expects 319 fn-ptr slots"
            );
        }

        // ── critical field offsets (must match NVENC SDK 12.2) ──

        #[test]
        fn offset_open_encode_session_ex() {
            // Slot 30 → offset = 8 + 29 * PTR
            let expected = 8 + 29 * PTR;
            assert_eq!(
                mem::offset_of!(NV_ENCODE_API_FUNCTION_LIST, nvEncOpenEncodeSessionEx),
                expected,
                "nvEncOpenEncodeSessionEx must be at SDK slot 30"
            );
        }

        #[test]
        fn offset_encode_picture() {
            // Slot 17 → offset = 8 + 16 * PTR
            let expected = 8 + 16 * PTR;
            assert_eq!(
                mem::offset_of!(NV_ENCODE_API_FUNCTION_LIST, nvEncEncodePicture),
                expected,
                "nvEncEncodePicture must be at SDK slot 17"
            );
        }

        #[test]
        fn offset_lock_bitstream() {
            // Slot 18 → offset = 8 + 17 * PTR
            let expected = 8 + 17 * PTR;
            assert_eq!(
                mem::offset_of!(NV_ENCODE_API_FUNCTION_LIST, nvEncLockBitstream),
                expected,
                "nvEncLockBitstream must be at SDK slot 18"
            );
        }

        #[test]
        fn offset_unlock_bitstream() {
            // Slot 19 → offset = 8 + 18 * PTR
            let expected = 8 + 18 * PTR;
            assert_eq!(
                mem::offset_of!(NV_ENCODE_API_FUNCTION_LIST, nvEncUnlockBitstream),
                expected,
                "nvEncUnlockBitstream must be at SDK slot 19"
            );
        }

        #[test]
        fn offset_map_input_resource() {
            // Slot 26 → offset = 8 + 25 * PTR
            let expected = 8 + 25 * PTR;
            assert_eq!(
                mem::offset_of!(NV_ENCODE_API_FUNCTION_LIST, nvEncMapInputResource),
                expected,
                "nvEncMapInputResource must be at SDK slot 26"
            );
        }

        #[test]
        fn offset_unmap_input_resource() {
            // Slot 27 → offset = 8 + 26 * PTR
            let expected = 8 + 26 * PTR;
            assert_eq!(
                mem::offset_of!(NV_ENCODE_API_FUNCTION_LIST, nvEncUnmapInputResource),
                expected,
                "nvEncUnmapInputResource must be at SDK slot 27"
            );
        }

        #[test]
        fn offset_destroy_encoder() {
            // Slot 28 → offset = 8 + 27 * PTR
            let expected = 8 + 27 * PTR;
            assert_eq!(
                mem::offset_of!(NV_ENCODE_API_FUNCTION_LIST, nvEncDestroyEncoder),
                expected,
                "nvEncDestroyEncoder must be at SDK slot 28"
            );
        }

        #[test]
        fn offset_register_resource() {
            // Slot 31 → offset = 8 + 30 * PTR
            let expected = 8 + 30 * PTR;
            assert_eq!(
                mem::offset_of!(NV_ENCODE_API_FUNCTION_LIST, nvEncRegisterResource),
                expected,
                "nvEncRegisterResource must be at SDK slot 31"
            );
        }

        #[test]
        fn offset_unregister_resource() {
            // Slot 32 → offset = 8 + 31 * PTR
            let expected = 8 + 31 * PTR;
            assert_eq!(
                mem::offset_of!(NV_ENCODE_API_FUNCTION_LIST, nvEncUnregisterResource),
                expected,
                "nvEncUnregisterResource must be at SDK slot 32"
            );
        }

        #[test]
        fn offset_initialize_encoder() {
            // Slot 12 → offset = 8 + 11 * PTR
            let expected = 8 + 11 * PTR;
            assert_eq!(
                mem::offset_of!(NV_ENCODE_API_FUNCTION_LIST, nvEncInitializeEncoder),
                expected,
                "nvEncInitializeEncoder must be at SDK slot 12"
            );
        }

        #[test]
        fn offset_create_bitstream_buffer() {
            // Slot 15 → offset = 8 + 14 * PTR
            let expected = 8 + 14 * PTR;
            assert_eq!(
                mem::offset_of!(NV_ENCODE_API_FUNCTION_LIST, nvEncCreateBitstreamBuffer),
                expected,
                "nvEncCreateBitstreamBuffer must be at SDK slot 15"
            );
        }

        #[test]
        fn offset_destroy_bitstream_buffer() {
            // Slot 16 → offset = 8 + 15 * PTR
            let expected = 8 + 15 * PTR;
            assert_eq!(
                mem::offset_of!(NV_ENCODE_API_FUNCTION_LIST, nvEncDestroyBitstreamBuffer),
                expected,
                "nvEncDestroyBitstreamBuffer must be at SDK slot 16"
            );
        }

        #[test]
        fn offset_get_encode_preset_config_ex() {
            // Slot 40 → offset = 8 + 39 * PTR
            let expected = 8 + 39 * PTR;
            assert_eq!(
                mem::offset_of!(NV_ENCODE_API_FUNCTION_LIST, nvEncGetEncodePresetConfigEx),
                expected,
                "nvEncGetEncodePresetConfigEx must be at SDK slot 40"
            );
        }

        // ── version defaults ──

        #[test]
        fn function_list_version_default() {
            let list = NV_ENCODE_API_FUNCTION_LIST::default();
            assert_eq!(list.version, struct_ver::<NV_ENCODE_API_FUNCTION_LIST>(2));
        }

        #[test]
        fn session_params_version_default() {
            let params = NV_ENC_OPEN_ENCODE_SESSION_EX_PARAMS::default();
            assert_eq!(
                params.version,
                struct_ver::<NV_ENC_OPEN_ENCODE_SESSION_EX_PARAMS>(1)
            );
            assert_eq!(params.apiVersion, NVENCAPI_VERSION);
        }

        #[test]
        fn initialize_params_version_default() {
            let params = NV_ENC_INITIALIZE_PARAMS::default();
            assert_eq!(params.version, struct_ver::<NV_ENC_INITIALIZE_PARAMS>(5));
        }

        #[test]
        fn config_version_default() {
            let config = NV_ENC_CONFIG::default();
            assert_eq!(config.version, struct_ver::<NV_ENC_CONFIG>(7));
        }

        #[test]
        fn nvenc_status_strings() {
            assert_eq!(nvenc_status_to_string(NV_ENC_SUCCESS), "NV_ENC_SUCCESS");
            assert_eq!(
                nvenc_status_to_string(NV_ENC_ERR_RESOURCE_REGISTER_FAILED),
                "NV_ENC_ERR_RESOURCE_REGISTER_FAILED"
            );
            assert_eq!(nvenc_status_to_string(999), "UNKNOWN_NVENC_ERROR");
        }
    }
}

#[cfg(target_os = "windows")]
pub use ffi::*;
