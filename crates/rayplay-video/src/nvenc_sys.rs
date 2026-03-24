/// Raw FFI bindings for NVENC SDK (Windows only).
///
/// These types are manually defined to match the NVENC SDK without requiring
/// bindgen or build-time dependencies. NVENC is loaded dynamically from
/// `nvEncodeAPI64.dll` at runtime.
#[cfg(target_os = "windows")]
#[allow(
    non_camel_case_types,
    non_snake_case,
    clippy::missing_safety_doc,
    clippy::pub_underscore_fields,
    clippy::unreadable_literal,
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
        NV_ENC_BUFFER_FORMAT_UNDEFINED = 0,
        NV_ENC_BUFFER_FORMAT_NV12 = 1,
        NV_ENC_BUFFER_FORMAT_YV12 = 16,
        NV_ENC_BUFFER_FORMAT_IYUV = 256,
        NV_ENC_BUFFER_FORMAT_YUV444 = 4,
        NV_ENC_BUFFER_FORMAT_YUV420_10BIT = 16777216,
        NV_ENC_BUFFER_FORMAT_YUV444_10BIT = 16777220,
        NV_ENC_BUFFER_FORMAT_ARGB = 0x20,
        NV_ENC_BUFFER_FORMAT_ARGB10 = 0x40,
        NV_ENC_BUFFER_FORMAT_AYUV = 0x80,
        NV_ENC_BUFFER_FORMAT_ABGR = 0x100,
        NV_ENC_BUFFER_FORMAT_ABGR10 = 0x200,
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

    #[repr(C)]
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

    // Function list struct
    #[repr(C)]
    pub struct NV_ENCODE_API_FUNCTION_LIST {
        pub version: u32,
        pub reserved: u32,
        pub nvEncOpenEncodeSession: Option<PFnNvEncOpenEncodeSessionEx>,
        pub nvEncGetEncodeGUIDCount: Option<PFnNvEncGetEncodeGUIDCount>,
        pub nvEncGetEncodeGUIDs: Option<PFnNvEncGetEncodeGUIDs>,
        pub nvEncGetEncodeProfileGUIDCount: Option<PFnNvEncGetEncodeGUIDCount>,
        pub nvEncGetEncodeProfileGUIDs: Option<PFnNvEncGetEncodeGUIDs>,
        pub nvEncGetInputFormatCount: Option<PFnNvEncGetEncodeGUIDCount>,
        pub nvEncGetInputFormats: Option<PFnNvEncGetEncodeGUIDs>,
        pub nvEncGetEncodePresetCount: Option<PFnNvEncGetEncodeGUIDCount>,
        pub nvEncGetEncodePresetGUIDs: Option<PFnNvEncGetEncodeGUIDs>,
        pub nvEncGetEncodePresetConfig: Option<PFnNvEncGetEncodePresetConfigEx>,
        pub nvEncGetEncodePresetConfigEx: Option<PFnNvEncGetEncodePresetConfigEx>,
        pub nvEncInitializeEncoder: Option<PFnNvEncInitializeEncoder>,
        pub nvEncCreateInputBuffer: Option<PFnNvEncCreateInputBuffer>,
        pub nvEncDestroyInputBuffer: Option<PFnNvEncDestroyInputBuffer>,
        pub nvEncCreateBitstreamBuffer: Option<PFnNvEncCreateBitstreamBuffer>,
        pub nvEncDestroyBitstreamBuffer: Option<PFnNvEncDestroyBitstreamBuffer>,
        pub nvEncRegisterResource: Option<PFnNvEncRegisterResource>,
        pub nvEncUnregisterResource: Option<PFnNvEncUnregisterResource>,
        pub nvEncMapInputResource: Option<PFnNvEncMapInputResource>,
        pub nvEncUnmapInputResource: Option<PFnNvEncUnmapInputResource>,
        pub nvEncEncodePicture: Option<PFnNvEncEncodePicture>,
        pub nvEncLockBitstream: Option<PFnNvEncLockBitstream>,
        pub nvEncUnlockBitstream: Option<PFnNvEncUnlockBitstream>,
        pub nvEncDestroyEncoder: Option<PFnNvEncDestroyEncoder>,
        pub reserved1: [*mut c_void; 287], // Pad to full API function list size
    }

    impl Default for NV_ENCODE_API_FUNCTION_LIST {
        fn default() -> Self {
            Self {
                version: struct_ver::<Self>(2),
                reserved: 0,
                nvEncOpenEncodeSession: None,
                nvEncGetEncodeGUIDCount: None,
                nvEncGetEncodeGUIDs: None,
                nvEncGetEncodeProfileGUIDCount: None,
                nvEncGetEncodeProfileGUIDs: None,
                nvEncGetInputFormatCount: None,
                nvEncGetInputFormats: None,
                nvEncGetEncodePresetCount: None,
                nvEncGetEncodePresetGUIDs: None,
                nvEncGetEncodePresetConfig: None,
                nvEncGetEncodePresetConfigEx: None,
                nvEncInitializeEncoder: None,
                nvEncCreateInputBuffer: None,
                nvEncDestroyInputBuffer: None,
                nvEncCreateBitstreamBuffer: None,
                nvEncDestroyBitstreamBuffer: None,
                nvEncRegisterResource: None,
                nvEncUnregisterResource: None,
                nvEncMapInputResource: None,
                nvEncUnmapInputResource: None,
                nvEncEncodePicture: None,
                nvEncLockBitstream: None,
                nvEncUnlockBitstream: None,
                nvEncDestroyEncoder: None,
                reserved1: [std::ptr::null_mut(); 287],
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
}

#[cfg(target_os = "windows")]
pub use ffi::*;
