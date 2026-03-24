/// Raw FFI bindings for NVENC SDK 12.2 (Windows only).
///
/// These types are manually defined to match the NVENC Video Codec SDK 12.2
/// `nvEncodeAPI.h` header without requiring bindgen or build-time dependencies.
/// NVENC is loaded dynamically from `nvEncodeAPI64.dll` at runtime.
///
/// Reference: <https://github.com/FFmpeg/nv-codec-headers/blob/n12.2.72.0/include/ffnvcodec/nvEncodeAPI.h>
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

    // ── NVENC API version ──

    pub const NVENCAPI_MAJOR_VERSION: u32 = 12;
    pub const NVENCAPI_MINOR_VERSION: u32 = 2;
    pub const NVENCAPI_VERSION: u32 = NVENCAPI_MAJOR_VERSION | (NVENCAPI_MINOR_VERSION << 24);

    /// Constructs the version field for an NVENC struct.
    /// Matches the SDK macro: `NVENCAPI_STRUCT_VERSION(ver)`.
    pub const fn nvencapi_struct_version(ver: u32) -> u32 {
        NVENCAPI_VERSION | (ver << 16) | (0x7 << 28)
    }

    /// Version with high bit set — used by some structs in SDK 12.2.
    pub const fn nvencapi_struct_version_high(ver: u32) -> u32 {
        nvencapi_struct_version(ver) | (1 << 31)
    }

    // ── Driver version helpers ──

    /// Encodes our SDK version in the format returned by `NvEncodeAPIGetMaxSupportedVersion`.
    /// Format: `(major << 4) | minor`.
    pub const fn nvencapi_max_version() -> u32 {
        (NVENCAPI_MAJOR_VERSION << 4) | NVENCAPI_MINOR_VERSION
    }

    /// Decodes a packed max-version value into `(major, minor)`.
    pub const fn unpack_max_version(packed: u32) -> (u32, u32) {
        (packed >> 4, packed & 0xF)
    }

    /// Returns `true` if the driver's max supported version is >= the SDK version we require.
    pub const fn is_driver_version_compatible(driver_max: u32, sdk_required: u32) -> bool {
        driver_max >= sdk_required
    }

    // ── GUID ──

    #[repr(C)]
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct GUID {
        pub data1: u32,
        pub data2: u16,
        pub data3: u16,
        pub data4: [u8; 8],
    }

    // ── Codec GUIDs ──

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

    // ── Preset GUIDs ──

    pub const NV_ENC_PRESET_P1_GUID: GUID = GUID {
        data1: 0xfc0a8d3e,
        data2: 0x45f8,
        data3: 0x4cf8,
        data4: [0x80, 0xc7, 0x29, 0x8e, 0x5e, 0x24, 0x01, 0x4c],
    };

    // ── Profile GUIDs ──

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

    // ── Status codes ──

    pub type NVENCSTATUS = u32;
    pub const NV_ENC_SUCCESS: NVENCSTATUS = 0;
    pub const NV_ENC_ERR_NO_ENCODE_DEVICE: NVENCSTATUS = 1;
    pub const NV_ENC_ERR_UNSUPPORTED_DEVICE: NVENCSTATUS = 2;
    pub const NV_ENC_ERR_INVALID_ENCODERDEVICE: NVENCSTATUS = 3;
    pub const NV_ENC_ERR_INVALID_DEVICE: NVENCSTATUS = 4;
    pub const NV_ENC_ERR_DEVICE_NOT_EXIST: NVENCSTATUS = 5;
    pub const NV_ENC_ERR_INVALID_PTR: NVENCSTATUS = 6;
    pub const NV_ENC_ERR_INVALID_EVENT: NVENCSTATUS = 7;
    pub const NV_ENC_ERR_INVALID_PARAM: NVENCSTATUS = 8;
    pub const NV_ENC_ERR_INVALID_CALL: NVENCSTATUS = 9;
    pub const NV_ENC_ERR_OUT_OF_MEMORY: NVENCSTATUS = 10;
    pub const NV_ENC_ERR_ENCODER_NOT_INITIALIZED: NVENCSTATUS = 11;
    pub const NV_ENC_ERR_UNSUPPORTED_PARAM: NVENCSTATUS = 12;
    pub const NV_ENC_ERR_LOCK_BUSY: NVENCSTATUS = 13;
    pub const NV_ENC_ERR_NOT_ENOUGH_BUFFER: NVENCSTATUS = 14;
    pub const NV_ENC_ERR_INVALID_VERSION: NVENCSTATUS = 15;
    pub const NV_ENC_ERR_MAP_FAILED: NVENCSTATUS = 16;
    pub const NV_ENC_ERR_NEED_MORE_INPUT: NVENCSTATUS = 17;
    pub const NV_ENC_ERR_ENCODER_BUSY: NVENCSTATUS = 18;
    pub const NV_ENC_ERR_EVENT_NOT_REGISTERD: NVENCSTATUS = 19;
    pub const NV_ENC_ERR_GENERIC: NVENCSTATUS = 20;
    pub const NV_ENC_ERR_INCOMPATIBLE_CLIENT_KEY: NVENCSTATUS = 21;
    pub const NV_ENC_ERR_UNIMPLEMENTED: NVENCSTATUS = 22;
    pub const NV_ENC_ERR_RESOURCE_REGISTER_FAILED: NVENCSTATUS = 23;
    pub const NV_ENC_ERR_RESOURCE_NOT_REGISTERED: NVENCSTATUS = 24;
    pub const NV_ENC_ERR_RESOURCE_NOT_MAPPED: NVENCSTATUS = 25;
    pub const NV_ENC_ERR_NEED_MORE_OUTPUT: NVENCSTATUS = 26;

    // ── Enums (all u32) ──

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
        NV_ENC_BUFFER_FORMAT_UNDEFINED = 0x00000000,
        NV_ENC_BUFFER_FORMAT_NV12 = 0x00000001,
        NV_ENC_BUFFER_FORMAT_YV12 = 0x00000010,
        NV_ENC_BUFFER_FORMAT_IYUV = 0x00000100,
        NV_ENC_BUFFER_FORMAT_YUV444 = 0x00001000,
        NV_ENC_BUFFER_FORMAT_YUV420_10BIT = 0x01000000,
        NV_ENC_BUFFER_FORMAT_ARGB = 0x02000000,
        NV_ENC_BUFFER_FORMAT_ARGB10 = 0x04000000,
        NV_ENC_BUFFER_FORMAT_AYUV = 0x08000000,
        NV_ENC_BUFFER_FORMAT_ABGR = 0x10000000,
        NV_ENC_BUFFER_FORMAT_ABGR10 = 0x20000000,
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
        NV_ENC_TWO_PASS_QUARTER_RESOLUTION = 1,
        NV_ENC_TWO_PASS_FULL_RESOLUTION = 2,
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

    // ── RC bitfield flags (NV_ENC_RC_PARAMS.rc_flags) ──

    pub const RC_FLAG_ENABLE_MIN_QP: u32 = 1 << 0;
    pub const RC_FLAG_ENABLE_MAX_QP: u32 = 1 << 1;
    pub const RC_FLAG_ENABLE_INITIAL_RCQP: u32 = 1 << 2;
    pub const RC_FLAG_ENABLE_AQ: u32 = 1 << 3;
    pub const RC_FLAG_ENABLE_LOOKAHEAD: u32 = 1 << 5;
    pub const RC_FLAG_DISABLE_IADAPT: u32 = 1 << 6;
    pub const RC_FLAG_DISABLE_BADAPT: u32 = 1 << 7;
    pub const RC_FLAG_ENABLE_TEMPORAL_AQ: u32 = 1 << 8;
    pub const RC_FLAG_ZERO_REORDER_DELAY: u32 = 1 << 9;
    pub const RC_FLAG_ENABLE_NONREF_P: u32 = 1 << 10;
    pub const RC_FLAG_STRICT_GOP_TARGET: u32 = 1 << 11;

    // ── H264 config bitfield flags (NV_ENC_CONFIG_H264.h264_flags) ──

    pub const H264_FLAG_REPEAT_SPS_PPS: u32 = 1 << 12;

    // ── HEVC config bitfield flags (NV_ENC_CONFIG_HEVC.hevc_flags) ──

    pub const HEVC_FLAG_REPEAT_SPS_PPS: u32 = 1 << 7;
    pub const HEVC_FLAG_CHROMA_FORMAT_IDC_SHIFT: u32 = 9;

    // ── Structs ──

    #[repr(C)]
    pub struct NV_ENC_QP {
        pub qpInterP: u32,
        pub qpInterB: u32,
        pub qpIntra: u32,
    }

    // SDK 12.2: 128 bytes
    #[repr(C)]
    pub struct NV_ENC_RC_PARAMS {
        pub version: u32,
        pub rateControlMode: u32, // NV_ENC_PARAMS_RC_MODE
        pub constQP: NV_ENC_QP,
        pub averageBitRate: u32,
        pub maxBitRate: u32,
        pub vbvBufferSize: u32,
        pub vbvInitialDelay: u32,
        pub rc_flags: u32, // packed bitfield
        pub minQP: NV_ENC_QP,
        pub maxQP: NV_ENC_QP,
        pub initialRCQP: NV_ENC_QP,
        pub temporallayerIdxMask: u32,
        pub temporalLayerQP: [u8; 8],
        pub targetQuality: u8,
        pub targetQualityLSB: u8,
        pub lookaheadDepth: u16,
        pub lowDelayKeyFrameScale: u8,
        pub yDcQPIndexOffset: i8,
        pub uDcQPIndexOffset: i8,
        pub vDcQPIndexOffset: i8,
        pub qpMapMode: u32, // NV_ENC_QP_MAP_MODE
        pub multiPass: u32, // NV_ENC_MULTI_PASS
        pub alphaLayerBitrateRatio: u32,
        pub cbQPIndexOffset: i8,
        pub crQPIndexOffset: i8,
        pub _reserved2: u16,
        pub lookaheadLevel: u32, // NV_ENC_LOOKAHEAD_LEVEL
        pub reserved: [u32; 3],
    }

    impl Default for NV_ENC_RC_PARAMS {
        fn default() -> Self {
            // SAFETY: All fields are integers — zero is valid.
            let mut s: Self = unsafe { std::mem::zeroed() };
            s.version = nvencapi_struct_version(1);
            s.rateControlMode = NV_ENC_PARAMS_RC_VBR;
            s
        }
    }

    // SDK 12.2: NV_ENC_CONFIG_H264_VUI_PARAMETERS = 112 bytes (16 u32 fields + reserved[12])
    // We don't actively use VUI params, so represent as opaque bytes.
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct NV_ENC_CONFIG_H264_VUI_PARAMETERS {
        pub _data: [u32; 28], // 112 bytes
    }

    impl Default for NV_ENC_CONFIG_H264_VUI_PARAMETERS {
        fn default() -> Self {
            unsafe { std::mem::zeroed() }
        }
    }

    pub type NV_ENC_CONFIG_HEVC_VUI_PARAMETERS = NV_ENC_CONFIG_H264_VUI_PARAMETERS;

    // SDK 12.2: NV_ENC_TIME_CODE = 32 bytes
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct NV_ENC_TIME_CODE {
        pub _data: [u32; 8], // 32 bytes
    }

    impl Default for NV_ENC_TIME_CODE {
        fn default() -> Self {
            unsafe { std::mem::zeroed() }
        }
    }

    // SDK 12.2: NV_ENC_PIC_PARAMS_H264_EXT = 128 bytes (union with reserved[32])
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct NV_ENC_PIC_PARAMS_H264_EXT {
        pub _reserved: [u32; 32],
    }

    impl Default for NV_ENC_PIC_PARAMS_H264_EXT {
        fn default() -> Self {
            unsafe { std::mem::zeroed() }
        }
    }

    // SDK 12.2: NVENC_EXTERNAL_ME_HINT_COUNTS_PER_BLOCKTYPE = 16 bytes
    #[repr(C)]
    pub struct NVENC_EXTERNAL_ME_HINT_COUNTS_PER_BLOCKTYPE {
        pub _bitfield: u32,
        pub reserved1: [u32; 3],
    }

    impl Default for NVENC_EXTERNAL_ME_HINT_COUNTS_PER_BLOCKTYPE {
        fn default() -> Self {
            unsafe { std::mem::zeroed() }
        }
    }

    // SDK 12.2: 1792 bytes
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct NV_ENC_CONFIG_H264 {
        pub h264_flags: u32, // 22 packed bitfields
        pub level: u32,
        pub idrPeriod: u32,
        pub separateColourPlaneFlag: u32,
        pub disableDeblockingFilterIDC: u32,
        pub numTemporalLayers: u32,
        pub spsId: u32,
        pub ppsId: u32,
        pub adaptiveTransformMode: u32, // enum
        pub fmoMode: u32,               // enum
        pub bdirectMode: u32,           // enum
        pub entropyCodingMode: u32,     // enum
        pub stereoMode: u32,            // enum
        pub intraRefreshPeriod: u32,
        pub intraRefreshCnt: u32,
        pub maxNumRefFrames: u32,
        pub sliceMode: u32,
        pub sliceModeData: u32,
        pub h264VUIParameters: NV_ENC_CONFIG_H264_VUI_PARAMETERS, // 112 bytes
        pub ltrNumFrames: u32,
        pub ltrTrustMode: u32,
        pub chromaFormatIDC: u32,
        pub maxTemporalLayers: u32,
        pub useBFramesAsRef: u32, // enum
        pub numRefL0: u32,        // enum
        pub numRefL1: u32,        // enum
        pub outputBitDepth: u32,  // enum NV_ENC_BIT_DEPTH
        pub inputBitDepth: u32,   // enum NV_ENC_BIT_DEPTH
        pub reserved1: [u32; 265],
        pub reserved2: [*mut c_void; 64],
    }

    impl Default for NV_ENC_CONFIG_H264 {
        fn default() -> Self {
            unsafe { std::mem::zeroed() }
        }
    }

    // SDK 12.2: 1560 bytes
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct NV_ENC_CONFIG_HEVC {
        pub level: u32,
        pub tier: u32,
        pub minCUSize: u32,  // enum NV_ENC_HEVC_CUSIZE
        pub maxCUSize: u32,  // enum NV_ENC_HEVC_CUSIZE
        pub hevc_flags: u32, // packed bitfields (32 bits)
        pub idrPeriod: u32,
        pub intraRefreshPeriod: u32,
        pub intraRefreshCnt: u32,
        pub maxNumRefFramesInDPB: u32,
        pub ltrNumFrames: u32,
        pub vpsId: u32,
        pub spsId: u32,
        pub ppsId: u32,
        pub sliceMode: u32,
        pub sliceModeData: u32,
        pub maxTemporalLayersMinus1: u32,
        pub hevcVUIParameters: NV_ENC_CONFIG_HEVC_VUI_PARAMETERS, // 112 bytes
        pub ltrTrustMode: u32,
        pub useBFramesAsRef: u32, // enum
        pub numRefL0: u32,        // enum
        pub numRefL1: u32,        // enum
        pub tfLevel: u32,         // enum NV_ENC_TEMPORAL_FILTER_LEVEL
        pub disableDeblockingFilterIDC: u32,
        pub outputBitDepth: u32, // enum NV_ENC_BIT_DEPTH
        pub inputBitDepth: u32,  // enum NV_ENC_BIT_DEPTH
        pub reserved1: [u32; 210],
        pub reserved2: [*mut c_void; 64],
    }

    impl Default for NV_ENC_CONFIG_HEVC {
        fn default() -> Self {
            unsafe { std::mem::zeroed() }
        }
    }

    // SDK 12.2: 1792 bytes (union, size = largest member or reserved[320])
    #[repr(C)]
    pub union NV_ENC_CODEC_CONFIG {
        pub h264Config: NV_ENC_CONFIG_H264,
        pub hevcConfig: NV_ENC_CONFIG_HEVC,
        pub reserved: [u32; 448], // 1792 bytes to match union size
    }

    // SDK 12.2: 3584 bytes, version = NVENCAPI_STRUCT_VERSION(9) | (1<<31)
    #[repr(C)]
    pub struct NV_ENC_CONFIG {
        pub version: u32,
        pub profileGUID: GUID,
        pub gopLength: u32,
        pub frameIntervalP: i32,
        pub monoChromeEncoding: u32,
        pub frameFieldMode: u32,                    // enum
        pub mvPrecision: u32,                       // enum
        pub rcParams: NV_ENC_RC_PARAMS,             // 128 bytes
        pub encodeCodecConfig: NV_ENC_CODEC_CONFIG, // 1792 bytes
        pub reserved: [u32; 278],
        pub reserved2: [*mut c_void; 64],
    }

    impl Default for NV_ENC_CONFIG {
        fn default() -> Self {
            let mut s: Self = unsafe { std::mem::zeroed() };
            s.version = nvencapi_struct_version_high(9);
            s.gopLength = 60;
            s.frameIntervalP = 1;
            s.rcParams = NV_ENC_RC_PARAMS::default();
            s
        }
    }

    // SDK 12.2: 5128 bytes, version = NVENCAPI_STRUCT_VERSION(5) | (1<<31)
    #[repr(C)]
    pub struct NV_ENC_PRESET_CONFIG {
        pub version: u32,
        pub _reserved_pad: u32,       // SDK has `uint32_t reserved`
        pub presetCfg: NV_ENC_CONFIG, // 3584 bytes
        pub reserved1: [u32; 256],
        pub reserved2: [*mut c_void; 64],
    }

    impl Default for NV_ENC_PRESET_CONFIG {
        fn default() -> Self {
            let mut s: Self = unsafe { std::mem::zeroed() };
            s.version = nvencapi_struct_version_high(5);
            s.presetCfg = NV_ENC_CONFIG::default();
            s
        }
    }

    // SDK 12.2: 1800 bytes, version = NVENCAPI_STRUCT_VERSION(7) | (1<<31)
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
        pub init_flags: u32, // packed bitfield (reportSliceOffsets etc.)
        pub privDataSize: u32,
        pub _reserved_pad: u32, // SDK: `uint32_t reserved`
        pub privData: *mut c_void,
        pub encodeConfig: *mut NV_ENC_CONFIG,
        pub maxEncodeWidth: u32,
        pub maxEncodeHeight: u32,
        pub maxMEHintCountsPerBlock: [NVENC_EXTERNAL_ME_HINT_COUNTS_PER_BLOCKTYPE; 2], // 32 bytes
        pub tuningInfo: NV_ENC_TUNING_INFO,
        pub bufferFormat: u32, // NV_ENC_BUFFER_FORMAT for D3D12
        pub numStateBuffers: u32,
        pub outputStatsLevel: u32, // NV_ENC_OUTPUT_STATS_LEVEL
        pub reserved1: [u32; 284],
        pub reserved2: [*mut c_void; 64],
    }

    impl Default for NV_ENC_INITIALIZE_PARAMS {
        fn default() -> Self {
            let mut s: Self = unsafe { std::mem::zeroed() };
            s.version = nvencapi_struct_version_high(7);
            s.frameRateDen = 1;
            s.enablePTD = 1;
            s.tuningInfo = NV_ENC_TUNING_INFO::NV_ENC_TUNING_INFO_UNDEFINED;
            s
        }
    }

    // SDK 12.2: 1552 bytes, version = NVENCAPI_STRUCT_VERSION(1)
    #[repr(C)]
    pub struct NV_ENC_OPEN_ENCODE_SESSION_EX_PARAMS {
        pub version: u32,
        pub deviceType: NV_ENC_DEVICE_TYPE,
        pub device: *mut c_void,
        pub reserved: *mut c_void,
        pub apiVersion: u32,
        pub reserved1: [u32; 253],
        pub reserved2: [*mut c_void; 64],
    }

    impl Default for NV_ENC_OPEN_ENCODE_SESSION_EX_PARAMS {
        fn default() -> Self {
            let mut s: Self = unsafe { std::mem::zeroed() };
            s.version = nvencapi_struct_version(1);
            s.deviceType = NV_ENC_DEVICE_TYPE::NV_ENC_DEVICE_TYPE_DIRECTX;
            s.apiVersion = NVENCAPI_VERSION;
            s
        }
    }

    // SDK 12.2: 1536 bytes, version = NVENCAPI_STRUCT_VERSION(5)
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
        pub bufferUsage: u32,              // NV_ENC_BUFFER_USAGE
        pub pInputFencePoint: *mut c_void, // NV_ENC_FENCE_POINT_D3D12*
        pub chromaOffset: [u32; 2],
        pub reserved1: [u32; 246],
        pub reserved2: [*mut c_void; 61],
    }

    impl Default for NV_ENC_REGISTER_RESOURCE {
        fn default() -> Self {
            let mut s: Self = unsafe { std::mem::zeroed() };
            s.version = nvencapi_struct_version(5);
            s.resourceType = NV_ENC_INPUT_RESOURCE_TYPE::NV_ENC_INPUT_RESOURCE_TYPE_DIRECTX;
            s.bufferFormat = NV_ENC_BUFFER_FORMAT::NV_ENC_BUFFER_FORMAT_UNDEFINED;
            s.bufferUsage = 1; // NV_ENC_INPUT_IMAGE
            s
        }
    }

    // SDK 12.2: 1544 bytes, version = NVENCAPI_STRUCT_VERSION(4)
    #[repr(C)]
    pub struct NV_ENC_MAP_INPUT_RESOURCE {
        pub version: u32,
        pub subResourceIndex: u32,
        pub inputResource: *mut c_void,
        pub registeredResource: *mut c_void,
        pub mappedResource: *mut c_void,
        pub mappedBufferFmt: NV_ENC_BUFFER_FORMAT,
        pub reserved1: [u32; 251],
        pub reserved2: [*mut c_void; 63],
    }

    impl Default for NV_ENC_MAP_INPUT_RESOURCE {
        fn default() -> Self {
            let mut s: Self = unsafe { std::mem::zeroed() };
            s.version = nvencapi_struct_version(4);
            s.mappedBufferFmt = NV_ENC_BUFFER_FORMAT::NV_ENC_BUFFER_FORMAT_UNDEFINED;
            s
        }
    }

    // SDK 12.2: NV_ENC_PIC_PARAMS_H264 = 1536 bytes
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct NV_ENC_PIC_PARAMS_H264 {
        pub displayPOCSyntax: u32,
        pub reserved3: u32,
        pub refPicFlag: u32,
        pub colourPlaneId: u32,
        pub forceIntraRefreshWithFrameCnt: u32,
        pub h264_pic_flags: u32, // constrainedFrame:1, sliceModeDataUpdate:1, ltrMarkFrame:1, ltrUseFrames:1, reservedBitFields:28
        pub sliceTypeData: *mut u8,
        pub sliceTypeArrayCnt: u32,
        pub seiPayloadArrayCnt: u32,
        pub seiPayloadArray: *mut c_void, // NV_ENC_SEI_PAYLOAD*
        pub sliceMode: u32,
        pub sliceModeData: u32,
        pub ltrMarkFrameIdx: u32,
        pub ltrUseFrameBitmap: u32,
        pub ltrUsageMode: u32,
        pub forceIntraSliceCount: u32,
        pub forceIntraSliceIdx: *mut u32,
        pub h264ExtPicParams: NV_ENC_PIC_PARAMS_H264_EXT, // 128 bytes
        pub timeCode: NV_ENC_TIME_CODE,                   // 32 bytes
        pub reserved: [u32; 202],
        pub reserved2: [*mut c_void; 61],
    }

    impl Default for NV_ENC_PIC_PARAMS_H264 {
        fn default() -> Self {
            unsafe { std::mem::zeroed() }
        }
    }

    // SDK 12.2: NV_ENC_PIC_PARAMS_HEVC = 1536 bytes
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct NV_ENC_PIC_PARAMS_HEVC {
        pub displayPOCSyntax: u32,
        pub refPicFlag: u32,
        pub temporalId: u32,
        pub forceIntraRefreshWithFrameCnt: u32,
        pub hevc_pic_flags: u32, // constrainedFrame:1, sliceModeDataUpdate:1, ltrMarkFrame:1, ltrUseFrames:1, reservedBitFields:28
        pub reserved1: u32,
        pub sliceTypeData: *mut u8,
        pub sliceTypeArrayCnt: u32,
        pub sliceMode: u32,
        pub sliceModeData: u32,
        pub ltrMarkFrameIdx: u32,
        pub ltrUseFrameBitmap: u32,
        pub ltrUsageMode: u32,
        pub seiPayloadArrayCnt: u32,
        pub _reserved_pad: u32,
        pub seiPayloadArray: *mut c_void, // NV_ENC_SEI_PAYLOAD*
        pub timeCode: NV_ENC_TIME_CODE,   // 32 bytes
        pub reserved2: [u32; 236],
        pub reserved3: [*mut c_void; 61],
    }

    impl Default for NV_ENC_PIC_PARAMS_HEVC {
        fn default() -> Self {
            unsafe { std::mem::zeroed() }
        }
    }

    // SDK 12.2: NV_ENC_CODEC_PIC_PARAMS = 1544 bytes (union, reserved[256] + padding)
    // Actual size driven by largest member. reserved[256] = 1024 bytes, but H264/HEVC are 1536 bytes.
    // So the union is 1536 bytes, plus possible padding. SDK says reserved[256] but actual size
    // must accommodate the larger members. Let's match the SDK sizeof = 1544.
    #[repr(C)]
    pub union NV_ENC_CODEC_PIC_PARAMS {
        pub h264PicParams: NV_ENC_PIC_PARAMS_H264,
        pub hevcPicParams: NV_ENC_PIC_PARAMS_HEVC,
        pub reserved: [u32; 386], // 1544 bytes to match SDK sizeof
    }

    // SDK 12.2: 3360 bytes, version = NVENCAPI_STRUCT_VERSION(7) | (1<<31)
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
        pub pictureStruct: u32, // NV_ENC_PIC_STRUCT
        pub pictureType: NV_ENC_PIC_TYPE,
        pub codecPicParams: NV_ENC_CODEC_PIC_PARAMS, // 1544 bytes
        pub meHintCountsPerBlock: [NVENC_EXTERNAL_ME_HINT_COUNTS_PER_BLOCKTYPE; 2], // 32 bytes
        pub meExternalHints: *mut c_void,
        pub reserved2: [u32; 7],
        pub reserved5: [*mut c_void; 2],
        pub qpDeltaMap: *mut i8,
        pub qpDeltaMapSize: u32,
        pub reservedBitFields: u32,
        pub meHintRefPicDist: [u16; 2],
        pub reserved4: u32,
        pub alphaBuffer: *mut c_void,
        pub meExternalSbHints: *mut c_void,
        pub meSbHintsCount: u32,
        pub stateBufferIdx: u32,
        pub outputReconBuffer: *mut c_void,
        pub reserved3: [u32; 284],
        pub reserved6: [*mut c_void; 57],
    }

    impl Default for NV_ENC_PIC_PARAMS {
        fn default() -> Self {
            let mut s: Self = unsafe { std::mem::zeroed() };
            s.version = nvencapi_struct_version_high(7);
            s.pictureType = NV_ENC_PIC_TYPE::NV_ENC_PIC_TYPE_UNKNOWN;
            s
        }
    }

    // SDK 12.2: 1544 bytes, version = NVENCAPI_STRUCT_VERSION(2) | (1<<31)
    #[repr(C)]
    pub struct NV_ENC_LOCK_BITSTREAM {
        pub version: u32,
        pub lock_flags: u32, // doNotWait:1, ltrFrame:1, getRCStats:1, reservedBitFields:29
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
        pub pictureStruct: u32, // NV_ENC_PIC_STRUCT
        pub frameAvgQP: u32,
        pub frameSatd: u32,
        pub ltrFrameIdx: u32,
        pub ltrFrameBitmap: u32,
        pub temporalId: u32,
        pub intraMBCount: u32,
        pub interMBCount: u32,
        pub averageMVX: i32,
        pub averageMVY: i32,
        pub alphaLayerSizeInBytes: u32,
        pub outputStatsPtrSize: u32,
        pub _reserved_pad: u32,
        pub outputStatsPtr: *mut c_void,
        pub frameIdxDisplay: u32,
        pub reserved1: [u32; 219],
        pub reserved2: [*mut c_void; 63],
        pub reservedInternal: [u32; 8],
    }

    impl Default for NV_ENC_LOCK_BITSTREAM {
        fn default() -> Self {
            let mut s: Self = unsafe { std::mem::zeroed() };
            s.version = nvencapi_struct_version_high(2);
            s
        }
    }

    // SDK 12.2: 776 bytes, version = NVENCAPI_STRUCT_VERSION(1)
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
            let mut s: Self = unsafe { std::mem::zeroed() };
            s.version = nvencapi_struct_version(1);
            s
        }
    }

    // ── Function pointer types ──

    pub type PFnNvEncOpenEncodeSessionEx = unsafe extern "system" fn(
        params: *mut NV_ENC_OPEN_ENCODE_SESSION_EX_PARAMS,
        encoder: *mut *mut c_void,
    ) -> NVENCSTATUS;

    pub type PFnNvEncGetEncodeGUIDCount =
        unsafe extern "system" fn(encoder: *mut c_void, count: *mut u32) -> NVENCSTATUS;

    pub type PFnNvEncGetEncodeGUIDs = unsafe extern "system" fn(
        encoder: *mut c_void,
        guids: *mut GUID,
        arraysize: u32,
        count: *mut u32,
    ) -> NVENCSTATUS;

    pub type PFnNvEncInitializeEncoder = unsafe extern "system" fn(
        encoder: *mut c_void,
        params: *mut NV_ENC_INITIALIZE_PARAMS,
    ) -> NVENCSTATUS;

    pub type PFnNvEncCreateInputBuffer = unsafe extern "system" fn(
        encoder: *mut c_void,
        params: *mut NV_ENC_CREATE_BITSTREAM_BUFFER,
    ) -> NVENCSTATUS;

    pub type PFnNvEncDestroyInputBuffer =
        unsafe extern "system" fn(encoder: *mut c_void, buffer: *mut c_void) -> NVENCSTATUS;

    pub type PFnNvEncCreateBitstreamBuffer = unsafe extern "system" fn(
        encoder: *mut c_void,
        params: *mut NV_ENC_CREATE_BITSTREAM_BUFFER,
    ) -> NVENCSTATUS;

    pub type PFnNvEncDestroyBitstreamBuffer =
        unsafe extern "system" fn(encoder: *mut c_void, buffer: *mut c_void) -> NVENCSTATUS;

    pub type PFnNvEncRegisterResource = unsafe extern "system" fn(
        encoder: *mut c_void,
        params: *mut NV_ENC_REGISTER_RESOURCE,
    ) -> NVENCSTATUS;

    pub type PFnNvEncUnregisterResource =
        unsafe extern "system" fn(encoder: *mut c_void, resource: *mut c_void) -> NVENCSTATUS;

    pub type PFnNvEncMapInputResource = unsafe extern "system" fn(
        encoder: *mut c_void,
        params: *mut NV_ENC_MAP_INPUT_RESOURCE,
    ) -> NVENCSTATUS;

    pub type PFnNvEncUnmapInputResource = unsafe extern "system" fn(
        encoder: *mut c_void,
        mapped_resource: *mut c_void,
    ) -> NVENCSTATUS;

    pub type PFnNvEncEncodePicture = unsafe extern "system" fn(
        encoder: *mut c_void,
        params: *mut NV_ENC_PIC_PARAMS,
    ) -> NVENCSTATUS;

    pub type PFnNvEncLockBitstream = unsafe extern "system" fn(
        encoder: *mut c_void,
        params: *mut NV_ENC_LOCK_BITSTREAM,
    ) -> NVENCSTATUS;

    pub type PFnNvEncUnlockBitstream =
        unsafe extern "system" fn(encoder: *mut c_void, buffer: *mut c_void) -> NVENCSTATUS;

    pub type PFnNvEncDestroyEncoder =
        unsafe extern "system" fn(encoder: *mut c_void) -> NVENCSTATUS;

    pub type PFnNvEncGetLastErrorString =
        unsafe extern "system" fn(encoder: *mut c_void) -> *const std::ffi::c_char;

    pub type PFnNvEncGetEncodePresetConfigEx = unsafe extern "system" fn(
        encoder: *mut c_void,
        encode_guid: GUID,
        preset_guid: GUID,
        tuning_info: NV_ENC_TUNING_INFO,
        preset_config: *mut NV_ENC_PRESET_CONFIG,
    ) -> NVENCSTATUS;

    // ── Function list (SDK 12.2, 318 fn-ptr slots) ──

    #[repr(C)]
    pub struct NV_ENCODE_API_FUNCTION_LIST {
        pub version: u32,
        pub reserved: u32,
        pub _nvEncOpenEncodeSession: *mut c_void, // slot 0
        pub nvEncGetEncodeGUIDCount: Option<PFnNvEncGetEncodeGUIDCount>, // slot 1
        pub nvEncGetEncodeProfileGUIDCount: Option<PFnNvEncGetEncodeGUIDCount>, // slot 2
        pub nvEncGetEncodeProfileGUIDs: Option<PFnNvEncGetEncodeGUIDs>, // slot 3
        pub nvEncGetEncodeGUIDs: Option<PFnNvEncGetEncodeGUIDs>, // slot 4
        pub nvEncGetInputFormatCount: Option<PFnNvEncGetEncodeGUIDCount>, // slot 5
        pub nvEncGetInputFormats: Option<PFnNvEncGetEncodeGUIDs>, // slot 6
        pub _nvEncGetEncodeCaps: *mut c_void,     // slot 7
        pub nvEncGetEncodePresetCount: Option<PFnNvEncGetEncodeGUIDCount>, // slot 8
        pub nvEncGetEncodePresetGUIDs: Option<PFnNvEncGetEncodeGUIDs>, // slot 9
        pub _nvEncGetEncodePresetConfig: *mut c_void, // slot 10
        pub nvEncInitializeEncoder: Option<PFnNvEncInitializeEncoder>, // slot 11
        pub nvEncCreateInputBuffer: Option<PFnNvEncCreateInputBuffer>, // slot 12
        pub nvEncDestroyInputBuffer: Option<PFnNvEncDestroyInputBuffer>, // slot 13
        pub nvEncCreateBitstreamBuffer: Option<PFnNvEncCreateBitstreamBuffer>, // slot 14
        pub nvEncDestroyBitstreamBuffer: Option<PFnNvEncDestroyBitstreamBuffer>, // slot 15
        pub nvEncEncodePicture: Option<PFnNvEncEncodePicture>, // slot 16
        pub nvEncLockBitstream: Option<PFnNvEncLockBitstream>, // slot 17
        pub nvEncUnlockBitstream: Option<PFnNvEncUnlockBitstream>, // slot 18
        pub _nvEncLockInputBuffer: *mut c_void,   // slot 19
        pub _nvEncUnlockInputBuffer: *mut c_void, // slot 20
        pub _nvEncGetEncodeStats: *mut c_void,    // slot 21
        pub _nvEncGetSequenceParams: *mut c_void, // slot 22
        pub _nvEncRegisterAsyncEvent: *mut c_void, // slot 23
        pub _nvEncUnregisterAsyncEvent: *mut c_void, // slot 24
        pub nvEncMapInputResource: Option<PFnNvEncMapInputResource>, // slot 25
        pub nvEncUnmapInputResource: Option<PFnNvEncUnmapInputResource>, // slot 26
        pub nvEncDestroyEncoder: Option<PFnNvEncDestroyEncoder>, // slot 27
        pub _nvEncInvalidateRefFrames: *mut c_void, // slot 28
        pub nvEncOpenEncodeSessionEx: Option<PFnNvEncOpenEncodeSessionEx>, // slot 29
        pub nvEncRegisterResource: Option<PFnNvEncRegisterResource>, // slot 30
        pub nvEncUnregisterResource: Option<PFnNvEncUnregisterResource>, // slot 31
        pub _nvEncReconfigureEncoder: *mut c_void, // slot 32
        pub _reserved1: *mut c_void,              // slot 33
        pub _nvEncCreateMVBuffer: *mut c_void,    // slot 34
        pub _nvEncDestroyMVBuffer: *mut c_void,   // slot 35
        pub _nvEncRunMotionEstimationOnly: *mut c_void, // slot 36
        pub nvEncGetLastErrorString: Option<PFnNvEncGetLastErrorString>, // slot 37
        pub _nvEncSetIOCudaStreams: *mut c_void,  // slot 38
        pub nvEncGetEncodePresetConfigEx: Option<PFnNvEncGetEncodePresetConfigEx>, // slot 39
        pub _nvEncGetSequenceParamEx: *mut c_void, // slot 40
        pub _nvEncRestoreEncoderState: *mut c_void, // slot 41
        pub _nvEncLookaheadPicture: *mut c_void,  // slot 42
        pub _reserved2: [*mut c_void; 275],
    }

    impl Default for NV_ENCODE_API_FUNCTION_LIST {
        fn default() -> Self {
            let mut s: Self = unsafe { std::mem::zeroed() };
            s.version = nvencapi_struct_version(2);
            s
        }
    }

    // Entry points
    pub type PFnNvEncodeAPICreateInstance =
        unsafe extern "system" fn(function_list: *mut NV_ENCODE_API_FUNCTION_LIST) -> NVENCSTATUS;

    pub type PFnNvEncodeAPIGetMaxSupportedVersion =
        unsafe extern "system" fn(version: *mut u32) -> NVENCSTATUS;

    /// Opens an NVENC encode session after validating driver version compatibility.
    ///
    /// Encapsulates the version check and session creation so it can be tested
    /// with mock NVENC functions on any platform.
    ///
    /// # Safety
    ///
    /// `open_session_fn` must be a valid NVENC function pointer (or a safe mock).
    /// `device_ptr` must be a valid device pointer for the given `device_type`.
    ///
    /// # Errors
    ///
    /// - [`VideoError::DriverVersionTooOld`] if `driver_max_version < nvencapi_max_version()`
    /// - [`VideoError::EncodingFailed`] if the open-session call returns a non-success status
    pub unsafe fn open_session(
        driver_max_version: u32,
        open_session_fn: PFnNvEncOpenEncodeSessionEx,
        device_ptr: *mut c_void,
        device_type: NV_ENC_DEVICE_TYPE,
    ) -> Result<*mut c_void, crate::encoder::VideoError> {
        if !is_driver_version_compatible(driver_max_version, nvencapi_max_version()) {
            let (drv_maj, drv_min) = unpack_max_version(driver_max_version);
            return Err(crate::encoder::VideoError::DriverVersionTooOld {
                driver_version: format!("{drv_maj}.{drv_min}"),
                sdk_version: format!("{NVENCAPI_MAJOR_VERSION}.{NVENCAPI_MINOR_VERSION}"),
            });
        }

        let mut params = NV_ENC_OPEN_ENCODE_SESSION_EX_PARAMS {
            device: device_ptr,
            deviceType: device_type,
            ..NV_ENC_OPEN_ENCODE_SESSION_EX_PARAMS::default()
        };

        let mut encoder: *mut c_void = std::ptr::null_mut();
        let status = unsafe { open_session_fn(&raw mut params, &raw mut encoder) };

        if status != NV_ENC_SUCCESS {
            return Err(crate::encoder::VideoError::EncodingFailed {
                reason: format!(
                    "nvEncOpenEncodeSession failed: {} (status={})",
                    nvenc_status_to_string(status),
                    status
                ),
            });
        }

        Ok(encoder)
    }

    /// Converts NVENC status code to human-readable string.
    pub fn nvenc_status_to_string(status: NVENCSTATUS) -> String {
        match status {
            NV_ENC_SUCCESS => "NV_ENC_SUCCESS".to_string(),
            NV_ENC_ERR_NO_ENCODE_DEVICE => "NV_ENC_ERR_NO_ENCODE_DEVICE".to_string(),
            NV_ENC_ERR_UNSUPPORTED_DEVICE => "NV_ENC_ERR_UNSUPPORTED_DEVICE".to_string(),
            NV_ENC_ERR_INVALID_ENCODERDEVICE => "NV_ENC_ERR_INVALID_ENCODERDEVICE".to_string(),
            NV_ENC_ERR_INVALID_DEVICE => "NV_ENC_ERR_INVALID_DEVICE".to_string(),
            NV_ENC_ERR_DEVICE_NOT_EXIST => "NV_ENC_ERR_DEVICE_NOT_EXIST".to_string(),
            NV_ENC_ERR_INVALID_PTR => "NV_ENC_ERR_INVALID_PTR".to_string(),
            NV_ENC_ERR_INVALID_EVENT => "NV_ENC_ERR_INVALID_EVENT".to_string(),
            NV_ENC_ERR_INVALID_PARAM => "NV_ENC_ERR_INVALID_PARAM".to_string(),
            NV_ENC_ERR_INVALID_CALL => "NV_ENC_ERR_INVALID_CALL".to_string(),
            NV_ENC_ERR_OUT_OF_MEMORY => "NV_ENC_ERR_OUT_OF_MEMORY".to_string(),
            NV_ENC_ERR_ENCODER_NOT_INITIALIZED => "NV_ENC_ERR_ENCODER_NOT_INITIALIZED".to_string(),
            NV_ENC_ERR_UNSUPPORTED_PARAM => "NV_ENC_ERR_UNSUPPORTED_PARAM".to_string(),
            NV_ENC_ERR_LOCK_BUSY => "NV_ENC_ERR_LOCK_BUSY".to_string(),
            NV_ENC_ERR_NOT_ENOUGH_BUFFER => "NV_ENC_ERR_NOT_ENOUGH_BUFFER".to_string(),
            NV_ENC_ERR_INVALID_VERSION => "NV_ENC_ERR_INVALID_VERSION".to_string(),
            NV_ENC_ERR_MAP_FAILED => "NV_ENC_ERR_MAP_FAILED".to_string(),
            NV_ENC_ERR_NEED_MORE_INPUT => "NV_ENC_ERR_NEED_MORE_INPUT".to_string(),
            NV_ENC_ERR_ENCODER_BUSY => "NV_ENC_ERR_ENCODER_BUSY".to_string(),
            NV_ENC_ERR_EVENT_NOT_REGISTERD => "NV_ENC_ERR_EVENT_NOT_REGISTERD".to_string(),
            NV_ENC_ERR_GENERIC => "NV_ENC_ERR_GENERIC".to_string(),
            NV_ENC_ERR_INCOMPATIBLE_CLIENT_KEY => "NV_ENC_ERR_INCOMPATIBLE_CLIENT_KEY".to_string(),
            NV_ENC_ERR_UNIMPLEMENTED => "NV_ENC_ERR_UNIMPLEMENTED".to_string(),
            NV_ENC_ERR_RESOURCE_REGISTER_FAILED => {
                "NV_ENC_ERR_RESOURCE_REGISTER_FAILED".to_string()
            }
            NV_ENC_ERR_RESOURCE_NOT_REGISTERED => "NV_ENC_ERR_RESOURCE_NOT_REGISTERED".to_string(),
            NV_ENC_ERR_RESOURCE_NOT_MAPPED => "NV_ENC_ERR_RESOURCE_NOT_MAPPED".to_string(),
            NV_ENC_ERR_NEED_MORE_OUTPUT => "NV_ENC_ERR_NEED_MORE_OUTPUT".to_string(),
            _ => format!("UNKNOWN_NVENC_ERROR (code {status})"),
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::ffi::c_void;
        use std::mem;

        const PTR: usize = mem::size_of::<*mut c_void>();

        // ── SDK 12.2 struct sizes (from nvEncodeAPI.h compiled with cc) ──

        #[test]
        fn size_nv_enc_rc_params() {
            assert_eq!(mem::size_of::<NV_ENC_RC_PARAMS>(), 128);
        }

        #[test]
        fn size_nv_enc_config_h264() {
            assert_eq!(mem::size_of::<NV_ENC_CONFIG_H264>(), 1792);
        }

        #[test]
        fn size_nv_enc_config_hevc() {
            assert_eq!(mem::size_of::<NV_ENC_CONFIG_HEVC>(), 1560);
        }

        #[test]
        fn size_nv_enc_codec_config() {
            assert_eq!(mem::size_of::<NV_ENC_CODEC_CONFIG>(), 1792);
        }

        #[test]
        fn size_nv_enc_config() {
            assert_eq!(mem::size_of::<NV_ENC_CONFIG>(), 3584);
        }

        #[test]
        fn size_nv_enc_preset_config() {
            assert_eq!(mem::size_of::<NV_ENC_PRESET_CONFIG>(), 5128);
        }

        #[test]
        fn size_nv_enc_initialize_params() {
            assert_eq!(mem::size_of::<NV_ENC_INITIALIZE_PARAMS>(), 1800);
        }

        #[test]
        fn size_nv_enc_open_encode_session_ex_params() {
            assert_eq!(mem::size_of::<NV_ENC_OPEN_ENCODE_SESSION_EX_PARAMS>(), 1552);
        }

        #[test]
        fn size_nv_enc_register_resource() {
            assert_eq!(mem::size_of::<NV_ENC_REGISTER_RESOURCE>(), 1536);
        }

        #[test]
        fn size_nv_enc_map_input_resource() {
            assert_eq!(mem::size_of::<NV_ENC_MAP_INPUT_RESOURCE>(), 1544);
        }

        #[test]
        fn size_nv_enc_create_bitstream_buffer() {
            assert_eq!(mem::size_of::<NV_ENC_CREATE_BITSTREAM_BUFFER>(), 776);
        }

        #[test]
        fn size_nv_enc_lock_bitstream() {
            assert_eq!(mem::size_of::<NV_ENC_LOCK_BITSTREAM>(), 1544);
        }

        #[test]
        fn size_nv_enc_pic_params() {
            assert_eq!(mem::size_of::<NV_ENC_PIC_PARAMS>(), 3360);
        }

        // ── SDK 12.2 version values ──

        #[test]
        fn nvencapi_version_value() {
            assert_eq!(NVENCAPI_VERSION, 0x0200_000C);
        }

        #[test]
        fn version_nv_enc_open_encode_session_ex_params() {
            let params = NV_ENC_OPEN_ENCODE_SESSION_EX_PARAMS::default();
            assert_eq!(params.version, 0x7201_000C);
            assert_eq!(params.apiVersion, NVENCAPI_VERSION);
        }

        #[test]
        fn version_nv_enc_config() {
            let config = NV_ENC_CONFIG::default();
            assert_eq!(config.version, 0xF209_000C);
        }

        #[test]
        fn version_nv_enc_preset_config() {
            let config = NV_ENC_PRESET_CONFIG::default();
            assert_eq!(config.version, 0xF205_000C);
        }

        #[test]
        fn version_nv_enc_initialize_params() {
            let params = NV_ENC_INITIALIZE_PARAMS::default();
            assert_eq!(params.version, 0xF207_000C);
        }

        #[test]
        fn version_nv_enc_register_resource() {
            let res = NV_ENC_REGISTER_RESOURCE::default();
            assert_eq!(res.version, 0x7205_000C);
        }

        #[test]
        fn version_nv_enc_map_input_resource() {
            let res = NV_ENC_MAP_INPUT_RESOURCE::default();
            assert_eq!(res.version, 0x7204_000C);
        }

        #[test]
        fn version_nv_enc_lock_bitstream() {
            let bs = NV_ENC_LOCK_BITSTREAM::default();
            assert_eq!(bs.version, 0xF202_000C);
        }

        #[test]
        fn version_nv_enc_pic_params() {
            let p = NV_ENC_PIC_PARAMS::default();
            assert_eq!(p.version, 0xF207_000C);
        }

        #[test]
        fn version_nv_enc_create_bitstream_buffer() {
            let b = NV_ENC_CREATE_BITSTREAM_BUFFER::default();
            assert_eq!(b.version, 0x7201_000C);
        }

        #[test]
        fn version_nv_encode_api_function_list() {
            let list = NV_ENCODE_API_FUNCTION_LIST::default();
            assert_eq!(list.version, 0x7202_000C);
        }

        #[test]
        fn nvenc_status_strings() {
            assert_eq!(nvenc_status_to_string(NV_ENC_SUCCESS), "NV_ENC_SUCCESS");
            assert_eq!(
                nvenc_status_to_string(NV_ENC_ERR_RESOURCE_REGISTER_FAILED),
                "NV_ENC_ERR_RESOURCE_REGISTER_FAILED"
            );
            assert_eq!(
                nvenc_status_to_string(999),
                "UNKNOWN_NVENC_ERROR (code 999)"
            );
        }

        // ── Struct configuration API (mirrors how nvenc.rs should use them) ──

        #[test]
        #[allow(clippy::field_reassign_with_default)]
        fn configure_rc_params_via_bitflags() {
            let mut rc = NV_ENC_RC_PARAMS::default();
            rc.rateControlMode = NV_ENC_PARAMS_RC_VBR;
            rc.averageBitRate = 20_000_000;
            rc.maxBitRate = 24_000_000;
            rc.rc_flags = RC_FLAG_ENABLE_AQ | RC_FLAG_ZERO_REORDER_DELAY;

            assert_eq!(rc.rc_flags & RC_FLAG_ENABLE_AQ, RC_FLAG_ENABLE_AQ);
            assert_eq!(
                rc.rc_flags & RC_FLAG_ZERO_REORDER_DELAY,
                RC_FLAG_ZERO_REORDER_DELAY
            );
        }

        #[test]
        #[allow(clippy::field_reassign_with_default)]
        fn configure_hevc_config_via_bitflags() {
            let mut hevc = NV_ENC_CONFIG_HEVC::default();
            hevc.hevc_flags = HEVC_FLAG_REPEAT_SPS_PPS | (1 << HEVC_FLAG_CHROMA_FORMAT_IDC_SHIFT); // chromaFormatIDC = 1 (YUV420)

            assert_ne!(hevc.hevc_flags & HEVC_FLAG_REPEAT_SPS_PPS, 0);
        }

        #[test]
        #[allow(clippy::field_reassign_with_default)]
        fn configure_h264_config_via_bitflags() {
            let mut h264 = NV_ENC_CONFIG_H264::default();
            h264.h264_flags = H264_FLAG_REPEAT_SPS_PPS;

            assert_ne!(h264.h264_flags & H264_FLAG_REPEAT_SPS_PPS, 0);
        }

        #[test]
        #[allow(clippy::field_reassign_with_default)]
        fn configure_init_params_with_tuning_info() {
            let mut params = NV_ENC_INITIALIZE_PARAMS::default();
            params.encodeGUID = NV_ENC_CODEC_HEVC_GUID;
            params.presetGUID = NV_ENC_PRESET_P1_GUID;
            params.encodeWidth = 1920;
            params.encodeHeight = 1080;
            params.tuningInfo = NV_ENC_TUNING_INFO::NV_ENC_TUNING_INFO_ULTRA_LOW_LATENCY;

            assert_eq!(
                params.tuningInfo,
                NV_ENC_TUNING_INFO::NV_ENC_TUNING_INFO_ULTRA_LOW_LATENCY
            );
        }

        // ── Function list layout (must match SDK 12.2 slot positions) ──

        #[test]
        fn function_list_total_size() {
            let expected = 4 + 4 + 318 * PTR;
            assert_eq!(mem::size_of::<NV_ENCODE_API_FUNCTION_LIST>(), expected);
        }

        #[test]
        fn offset_open_encode_session_ex() {
            assert_eq!(
                mem::offset_of!(NV_ENCODE_API_FUNCTION_LIST, nvEncOpenEncodeSessionEx),
                8 + 29 * PTR
            );
        }

        #[test]
        fn offset_encode_picture() {
            assert_eq!(
                mem::offset_of!(NV_ENCODE_API_FUNCTION_LIST, nvEncEncodePicture),
                8 + 16 * PTR
            );
        }

        #[test]
        fn offset_lock_bitstream() {
            assert_eq!(
                mem::offset_of!(NV_ENCODE_API_FUNCTION_LIST, nvEncLockBitstream),
                8 + 17 * PTR
            );
        }

        #[test]
        fn offset_unlock_bitstream() {
            assert_eq!(
                mem::offset_of!(NV_ENCODE_API_FUNCTION_LIST, nvEncUnlockBitstream),
                8 + 18 * PTR
            );
        }

        #[test]
        fn offset_map_input_resource() {
            assert_eq!(
                mem::offset_of!(NV_ENCODE_API_FUNCTION_LIST, nvEncMapInputResource),
                8 + 25 * PTR
            );
        }

        #[test]
        fn offset_unmap_input_resource() {
            assert_eq!(
                mem::offset_of!(NV_ENCODE_API_FUNCTION_LIST, nvEncUnmapInputResource),
                8 + 26 * PTR
            );
        }

        #[test]
        fn offset_destroy_encoder() {
            assert_eq!(
                mem::offset_of!(NV_ENCODE_API_FUNCTION_LIST, nvEncDestroyEncoder),
                8 + 27 * PTR
            );
        }

        #[test]
        fn offset_register_resource() {
            assert_eq!(
                mem::offset_of!(NV_ENCODE_API_FUNCTION_LIST, nvEncRegisterResource),
                8 + 30 * PTR
            );
        }

        #[test]
        fn offset_unregister_resource() {
            assert_eq!(
                mem::offset_of!(NV_ENCODE_API_FUNCTION_LIST, nvEncUnregisterResource),
                8 + 31 * PTR
            );
        }

        #[test]
        fn offset_initialize_encoder() {
            assert_eq!(
                mem::offset_of!(NV_ENCODE_API_FUNCTION_LIST, nvEncInitializeEncoder),
                8 + 11 * PTR
            );
        }

        #[test]
        fn offset_create_bitstream_buffer() {
            assert_eq!(
                mem::offset_of!(NV_ENCODE_API_FUNCTION_LIST, nvEncCreateBitstreamBuffer),
                8 + 14 * PTR
            );
        }

        #[test]
        fn offset_destroy_bitstream_buffer() {
            assert_eq!(
                mem::offset_of!(NV_ENCODE_API_FUNCTION_LIST, nvEncDestroyBitstreamBuffer),
                8 + 15 * PTR
            );
        }

        #[test]
        fn offset_get_encode_preset_config_ex() {
            assert_eq!(
                mem::offset_of!(NV_ENCODE_API_FUNCTION_LIST, nvEncGetEncodePresetConfigEx),
                8 + 39 * PTR
            );
        }

        // ── New tests for status code fix ──

        #[test]
        fn status_code_values_match_sdk() {
            assert_eq!(NV_ENC_SUCCESS, 0);
            assert_eq!(NV_ENC_ERR_NO_ENCODE_DEVICE, 1);
            assert_eq!(NV_ENC_ERR_UNSUPPORTED_DEVICE, 2);
            assert_eq!(NV_ENC_ERR_INVALID_ENCODERDEVICE, 3);
            assert_eq!(NV_ENC_ERR_INVALID_DEVICE, 4);
            assert_eq!(NV_ENC_ERR_DEVICE_NOT_EXIST, 5);
            assert_eq!(NV_ENC_ERR_INVALID_PTR, 6);
            assert_eq!(NV_ENC_ERR_INVALID_EVENT, 7);
            assert_eq!(NV_ENC_ERR_INVALID_PARAM, 8);
            assert_eq!(NV_ENC_ERR_INVALID_CALL, 9);
            assert_eq!(NV_ENC_ERR_OUT_OF_MEMORY, 10);
            assert_eq!(NV_ENC_ERR_ENCODER_NOT_INITIALIZED, 11);
            assert_eq!(NV_ENC_ERR_UNSUPPORTED_PARAM, 12);
            assert_eq!(NV_ENC_ERR_LOCK_BUSY, 13);
            assert_eq!(NV_ENC_ERR_NOT_ENOUGH_BUFFER, 14);
            assert_eq!(NV_ENC_ERR_INVALID_VERSION, 15);
            assert_eq!(NV_ENC_ERR_MAP_FAILED, 16);
            assert_eq!(NV_ENC_ERR_NEED_MORE_INPUT, 17);
            assert_eq!(NV_ENC_ERR_ENCODER_BUSY, 18);
            assert_eq!(NV_ENC_ERR_EVENT_NOT_REGISTERD, 19);
            assert_eq!(NV_ENC_ERR_GENERIC, 20);
            assert_eq!(NV_ENC_ERR_INCOMPATIBLE_CLIENT_KEY, 21);
            assert_eq!(NV_ENC_ERR_UNIMPLEMENTED, 22);
            assert_eq!(NV_ENC_ERR_RESOURCE_REGISTER_FAILED, 23);
            assert_eq!(NV_ENC_ERR_RESOURCE_NOT_REGISTERED, 24);
            assert_eq!(NV_ENC_ERR_RESOURCE_NOT_MAPPED, 25);
            assert_eq!(NV_ENC_ERR_NEED_MORE_OUTPUT, 26);
        }

        #[test]
        fn status_to_string_returns_correct_name_for_every_sdk_code() {
            let expected: &[(u32, &str)] = &[
                (0, "NV_ENC_SUCCESS"),
                (1, "NV_ENC_ERR_NO_ENCODE_DEVICE"),
                (2, "NV_ENC_ERR_UNSUPPORTED_DEVICE"),
                (3, "NV_ENC_ERR_INVALID_ENCODERDEVICE"),
                (4, "NV_ENC_ERR_INVALID_DEVICE"),
                (5, "NV_ENC_ERR_DEVICE_NOT_EXIST"),
                (6, "NV_ENC_ERR_INVALID_PTR"),
                (7, "NV_ENC_ERR_INVALID_EVENT"),
                (8, "NV_ENC_ERR_INVALID_PARAM"),
                (9, "NV_ENC_ERR_INVALID_CALL"),
                (10, "NV_ENC_ERR_OUT_OF_MEMORY"),
                (11, "NV_ENC_ERR_ENCODER_NOT_INITIALIZED"),
                (12, "NV_ENC_ERR_UNSUPPORTED_PARAM"),
                (13, "NV_ENC_ERR_LOCK_BUSY"),
                (14, "NV_ENC_ERR_NOT_ENOUGH_BUFFER"),
                (15, "NV_ENC_ERR_INVALID_VERSION"),
                (16, "NV_ENC_ERR_MAP_FAILED"),
                (17, "NV_ENC_ERR_NEED_MORE_INPUT"),
                (18, "NV_ENC_ERR_ENCODER_BUSY"),
                (19, "NV_ENC_ERR_EVENT_NOT_REGISTERD"),
                (20, "NV_ENC_ERR_GENERIC"),
                (21, "NV_ENC_ERR_INCOMPATIBLE_CLIENT_KEY"),
                (22, "NV_ENC_ERR_UNIMPLEMENTED"),
                (23, "NV_ENC_ERR_RESOURCE_REGISTER_FAILED"),
                (24, "NV_ENC_ERR_RESOURCE_NOT_REGISTERED"),
                (25, "NV_ENC_ERR_RESOURCE_NOT_MAPPED"),
                (26, "NV_ENC_ERR_NEED_MORE_OUTPUT"),
            ];
            for &(code, name) in expected {
                assert_eq!(
                    nvenc_status_to_string(code),
                    name,
                    "mismatch for code {code}"
                );
            }
        }

        #[test]
        fn all_sdk_status_codes_are_recognized() {
            for code in 0..=26u32 {
                let s = nvenc_status_to_string(code);
                assert!(
                    !s.contains("UNKNOWN"),
                    "SDK code {code} fell through to UNKNOWN: {s}"
                );
            }
        }

        #[test]
        fn status_to_string_unknown_code_includes_numeric_value() {
            let s = nvenc_status_to_string(999);
            assert!(
                s.contains("999"),
                "unknown code should include numeric value: {s}"
            );
            assert!(
                s.contains("UNKNOWN"),
                "unknown code should contain UNKNOWN: {s}"
            );
        }

        #[test]
        fn offset_restore_encoder_state() {
            assert_eq!(
                mem::offset_of!(NV_ENCODE_API_FUNCTION_LIST, _nvEncRestoreEncoderState),
                8 + 41 * PTR
            );
        }

        #[test]
        fn offset_get_last_error_string() {
            assert_eq!(
                mem::offset_of!(NV_ENCODE_API_FUNCTION_LIST, nvEncGetLastErrorString),
                8 + 37 * PTR
            );
        }

        // ── FFI type sizes ──

        #[test]
        fn test_pfn_get_max_supported_version_size() {
            assert_eq!(mem::size_of::<PFnNvEncodeAPIGetMaxSupportedVersion>(), PTR);
        }

        // ── Driver version helpers ──

        #[test]
        fn test_nvencapi_max_version_value() {
            assert_eq!(nvencapi_max_version(), 0xC2);
        }

        #[test]
        fn test_unpack_max_version_sdk_12_2() {
            assert_eq!(unpack_max_version(0xC2), (12, 2));
        }

        #[test]
        #[allow(clippy::identity_op)]
        fn test_unpack_max_version_sdk_11_0() {
            assert_eq!(unpack_max_version((11 << 4) | 0), (11, 0));
        }

        #[test]
        fn test_is_driver_version_compatible_exact_match() {
            assert!(is_driver_version_compatible(0xC2, 0xC2));
        }

        #[test]
        fn test_is_driver_version_compatible_driver_newer() {
            assert!(is_driver_version_compatible((12 << 4) | 3, 0xC2));
        }

        #[test]
        #[allow(clippy::identity_op)]
        fn test_is_driver_version_compatible_driver_older() {
            assert!(!is_driver_version_compatible((11 << 4) | 0, 0xC2));
        }

        #[test]
        fn test_is_driver_version_compatible_same_major_lower_minor() {
            assert!(!is_driver_version_compatible((12 << 4) | 1, 0xC2));
        }

        // ── E2E tests with mock NVENC functions ──

        /// Mock: always rejects session (simulates old driver).
        unsafe extern "system" fn mock_open_session_reject(
            _params: *mut NV_ENC_OPEN_ENCODE_SESSION_EX_PARAMS,
            _encoder: *mut *mut c_void,
        ) -> NVENCSTATUS {
            NV_ENC_ERR_INVALID_VERSION
        }

        /// Mock: validates version fields like a real SDK 12.2 driver, returns success.
        unsafe extern "system" fn mock_open_session_validate(
            params: *mut NV_ENC_OPEN_ENCODE_SESSION_EX_PARAMS,
            encoder: *mut *mut c_void,
        ) -> NVENCSTATUS {
            let p = unsafe { &*params };
            if p.version != nvencapi_struct_version(1) {
                return NV_ENC_ERR_INVALID_VERSION;
            }
            if p.apiVersion != NVENCAPI_VERSION {
                return NV_ENC_ERR_INVALID_VERSION;
            }
            unsafe {
                *encoder = 0xBEEF_CAFE as *mut c_void;
            }
            NV_ENC_SUCCESS
        }

        #[test]
        fn test_e2e_old_driver_rejects_with_driver_version_too_old() {
            let driver_max = (12 << 4) | 1; // 12.1 — too old for SDK 12.2
            let result = unsafe {
                open_session(
                    driver_max,
                    mock_open_session_reject, // should never be called
                    0xDEAD as *mut c_void,
                    NV_ENC_DEVICE_TYPE::NV_ENC_DEVICE_TYPE_DIRECTX,
                )
            };
            assert!(
                matches!(
                    result,
                    Err(crate::encoder::VideoError::DriverVersionTooOld { .. })
                ),
                "expected DriverVersionTooOld, got {result:?}"
            );
        }

        #[test]
        fn test_e2e_compatible_driver_opens_session_successfully() {
            let driver_max = (12 << 4) | 2; // 12.2 — exact match
            let result = unsafe {
                open_session(
                    driver_max,
                    mock_open_session_validate,
                    0xDEAD as *mut c_void,
                    NV_ENC_DEVICE_TYPE::NV_ENC_DEVICE_TYPE_DIRECTX,
                )
            };
            assert!(result.is_ok(), "expected Ok, got {result:?}");
            assert_eq!(result.unwrap(), 0xBEEF_CAFE as *mut c_void);
        }

        #[test]
        fn test_e2e_newer_driver_opens_session_successfully() {
            let driver_max = (12 << 4) | 3; // 12.3 — newer than required
            let result = unsafe {
                open_session(
                    driver_max,
                    mock_open_session_validate,
                    0xDEAD as *mut c_void,
                    NV_ENC_DEVICE_TYPE::NV_ENC_DEVICE_TYPE_DIRECTX,
                )
            };
            assert!(result.is_ok(), "expected Ok, got {result:?}");
        }

        #[test]
        fn test_e2e_session_params_have_correct_version_fields() {
            /// Mock that captures and validates the version fields.
            unsafe extern "system" fn mock_check_versions(
                params: *mut NV_ENC_OPEN_ENCODE_SESSION_EX_PARAMS,
                encoder: *mut *mut c_void,
            ) -> NVENCSTATUS {
                let p = unsafe { &*params };
                assert_eq!(p.version, 0x7201_000C, "struct version mismatch");
                assert_eq!(p.apiVersion, 0x0200_000C, "API version mismatch");
                unsafe {
                    *encoder = 0xCAFE as *mut c_void;
                }
                NV_ENC_SUCCESS
            }

            let result = unsafe {
                open_session(
                    (12 << 4) | 2,
                    mock_check_versions,
                    0xDEAD as *mut c_void,
                    NV_ENC_DEVICE_TYPE::NV_ENC_DEVICE_TYPE_DIRECTX,
                )
            };
            assert!(result.is_ok(), "expected Ok, got {result:?}");
        }

        #[test]
        fn test_e2e_mock_driver_rejects_wrong_api_version() {
            // Driver version is compatible but mock always rejects — tests error categorisation
            let driver_max = (12 << 4) | 2;
            let result = unsafe {
                open_session(
                    driver_max,
                    mock_open_session_reject, // rejects regardless
                    0xDEAD as *mut c_void,
                    NV_ENC_DEVICE_TYPE::NV_ENC_DEVICE_TYPE_DIRECTX,
                )
            };
            assert!(
                matches!(
                    result,
                    Err(crate::encoder::VideoError::EncodingFailed { .. })
                ),
                "expected EncodingFailed, got {result:?}"
            );
        }

        // ── Error-path tests ──

        #[test]
        fn nvenc_error_chain_reports_real_error_name() {
            // These are the error codes nvEncOpenEncodeSessionEx can return
            // (SDK docs, nvEncodeAPI.h lines 2852-2858)
            let session_open_errors: &[(u32, &str)] = &[
                (6, "NV_ENC_ERR_INVALID_PTR"),
                (3, "NV_ENC_ERR_INVALID_ENCODERDEVICE"),
                (5, "NV_ENC_ERR_DEVICE_NOT_EXIST"),
                (12, "NV_ENC_ERR_UNSUPPORTED_PARAM"),
                (10, "NV_ENC_ERR_OUT_OF_MEMORY"),
                (8, "NV_ENC_ERR_INVALID_PARAM"),
                (20, "NV_ENC_ERR_GENERIC"),
                (15, "NV_ENC_ERR_INVALID_VERSION"),
            ];

            for &(code, expected_name) in session_open_errors {
                let status_str = nvenc_status_to_string(code);
                assert!(
                    !status_str.contains("UNKNOWN"),
                    "SDK status code {code} reported as UNKNOWN — error chain is broken"
                );
                assert_eq!(
                    status_str, expected_name,
                    "code {code}: expected '{expected_name}', got '{status_str}'"
                );
                // Simulate the format! pattern from nvenc.rs line 151-154
                let error_msg =
                    format!("nvEncOpenEncodeSession failed: {status_str} (status={code})");
                assert!(
                    error_msg.contains(expected_name),
                    "error msg missing name for code {code}"
                );
                assert!(
                    error_msg.contains(&format!("status={code}")),
                    "error msg missing raw code {code}"
                );
            }
        }
    }
}

#[cfg(target_os = "windows")]
pub use ffi::*;
