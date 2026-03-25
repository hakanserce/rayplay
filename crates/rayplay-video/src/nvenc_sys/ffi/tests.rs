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
    let params = NV_ENC_OPEN_ENCODE_SESSION_EX_PARAMS::new_versioned();
    assert_eq!(params.version, 0x7201_000C);
    assert_eq!(params.apiVersion, NVENCAPI_VERSION);
}

#[test]
fn version_nv_enc_config() {
    let config = NV_ENC_CONFIG::new_versioned();
    assert_eq!(config.version, 0xF209_000C);
}

#[test]
fn version_nv_enc_preset_config() {
    let config = NV_ENC_PRESET_CONFIG::new_versioned();
    assert_eq!(config.version, 0xF205_000C);
}

#[test]
fn version_nv_enc_initialize_params() {
    let params = NV_ENC_INITIALIZE_PARAMS::new_versioned();
    assert_eq!(params.version, 0xF207_000C);
}

#[test]
fn version_nv_enc_register_resource() {
    let res = NV_ENC_REGISTER_RESOURCE::new_versioned();
    assert_eq!(res.version, 0x7205_000C);
}

#[test]
fn version_nv_enc_map_input_resource() {
    let res = NV_ENC_MAP_INPUT_RESOURCE::new_versioned();
    assert_eq!(res.version, 0x7204_000C);
}

#[test]
fn version_nv_enc_lock_bitstream() {
    let bs = NV_ENC_LOCK_BITSTREAM::new_versioned();
    assert_eq!(bs.version, 0xF202_000C);
}

#[test]
fn version_nv_enc_pic_params() {
    let p = NV_ENC_PIC_PARAMS::new_versioned();
    assert_eq!(p.version, 0xF207_000C);
}

#[test]
fn version_nv_enc_create_bitstream_buffer() {
    let b = NV_ENC_CREATE_BITSTREAM_BUFFER::new_versioned();
    assert_eq!(b.version, 0x7201_000C);
}

#[test]
fn version_nv_encode_api_function_list() {
    let list = NV_ENCODE_API_FUNCTION_LIST::new_versioned();
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
//    Note: bitfield setters may not be available in current bindgen output,
//    so these tests are commented out until we can verify the API.

#[test]
#[allow(clippy::field_reassign_with_default)]
fn configure_rc_params_basic() {
    let mut rc = NV_ENC_RC_PARAMS::default();
    rc.rateControlMode = NV_ENC_PARAMS_RC_VBR;
    rc.averageBitRate = 20_000_000;
    rc.maxBitRate = 24_000_000;

    assert_eq!(rc.rateControlMode, NV_ENC_PARAMS_RC_VBR);
    assert_eq!(rc.averageBitRate, 20_000_000);
    assert_eq!(rc.maxBitRate, 24_000_000);
}

// TODO: Re-enable once we understand the bitfield API
// #[test]
// #[allow(clippy::field_reassign_with_default)]
// fn configure_hevc_config_via_bitflags() {
//     let mut hevc = NV_ENC_CONFIG_HEVC::default();
//     // hevc.set_repeatSPSPPS(1);
//     // hevc.set_chromaFormatIDC(1);
// }

// #[test]
// #[allow(clippy::field_reassign_with_default)]
// fn configure_h264_config_via_bitflags() {
//     let mut h264 = NV_ENC_CONFIG_H264::default();
//     // h264.set_repeatSPSPPS(1);
// }

#[test]
#[allow(clippy::field_reassign_with_default)]
fn configure_init_params_with_tuning_info() {
    let mut params = NV_ENC_INITIALIZE_PARAMS::new_versioned();
    params.encodeGUID = NV_ENC_CODEC_HEVC_GUID;
    params.presetGUID = NV_ENC_PRESET_P1_GUID;
    params.encodeWidth = 1920;
    params.encodeHeight = 1080;
    params.tuningInfo = NV_ENC_TUNING_INFO_ULTRA_LOW_LATENCY;

    assert_eq!(params.tuningInfo, NV_ENC_TUNING_INFO_ULTRA_LOW_LATENCY);
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
    let expected: &[(NVENCSTATUS, &str)] = &[
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
    for code in 0..=26 as NVENCSTATUS {
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

// ── GUID values (must match SDK 12.2 nvEncodeAPI.h) ──

#[test]
fn codec_h264_guid_matches_sdk() {
    // {6BC82762-4E63-4CA4-AA85-1E50F321F6BF}
    assert_eq!(NV_ENC_CODEC_H264_GUID.Data1, 0x6bc82762);
    assert_eq!(NV_ENC_CODEC_H264_GUID.Data2, 0x4e63);
    assert_eq!(NV_ENC_CODEC_H264_GUID.Data3, 0x4ca4);
    assert_eq!(
        NV_ENC_CODEC_H264_GUID.Data4,
        [0xaa, 0x85, 0x1e, 0x50, 0xf3, 0x21, 0xf6, 0xbf]
    );
}

#[test]
fn codec_hevc_guid_matches_sdk() {
    // {790CDC88-4522-4D7B-9425-BDA9975F7603}
    assert_eq!(NV_ENC_CODEC_HEVC_GUID.Data1, 0x790cdc88);
    assert_eq!(NV_ENC_CODEC_HEVC_GUID.Data2, 0x4522);
    assert_eq!(NV_ENC_CODEC_HEVC_GUID.Data3, 0x4d7b);
    assert_eq!(
        NV_ENC_CODEC_HEVC_GUID.Data4,
        [0x94, 0x25, 0xbd, 0xa9, 0x97, 0x5f, 0x76, 0x03]
    );
}

#[test]
fn preset_p1_guid_matches_sdk() {
    // {FC0A8D3E-45F8-4CF8-80C7-298871590EBF}
    assert_eq!(NV_ENC_PRESET_P1_GUID.Data1, 0xfc0a8d3e);
    assert_eq!(NV_ENC_PRESET_P1_GUID.Data2, 0x45f8);
    assert_eq!(NV_ENC_PRESET_P1_GUID.Data3, 0x4cf8);
    assert_eq!(
        NV_ENC_PRESET_P1_GUID.Data4,
        [0x80, 0xc7, 0x29, 0x88, 0x71, 0x59, 0x0e, 0xbf]
    );
}

#[test]
fn h264_profile_main_guid_matches_sdk() {
    // {60B5C1D4-67FE-4790-94D5-C4726D7B6E6D}
    assert_eq!(NV_ENC_H264_PROFILE_MAIN_GUID.Data1, 0x60b5c1d4);
    assert_eq!(NV_ENC_H264_PROFILE_MAIN_GUID.Data2, 0x67fe);
    assert_eq!(NV_ENC_H264_PROFILE_MAIN_GUID.Data3, 0x4790);
    assert_eq!(
        NV_ENC_H264_PROFILE_MAIN_GUID.Data4,
        [0x94, 0xd5, 0xc4, 0x72, 0x6d, 0x7b, 0x6e, 0x6d]
    );
}

#[test]
fn hevc_profile_main_guid_matches_sdk() {
    // {B514C39A-B55B-40FA-878F-F1253B4DFDEC}
    assert_eq!(NV_ENC_HEVC_PROFILE_MAIN_GUID.Data1, 0xb514c39a);
    assert_eq!(NV_ENC_HEVC_PROFILE_MAIN_GUID.Data2, 0xb55b);
    assert_eq!(NV_ENC_HEVC_PROFILE_MAIN_GUID.Data3, 0x40fa);
    assert_eq!(
        NV_ENC_HEVC_PROFILE_MAIN_GUID.Data4,
        [0x87, 0x8f, 0xf1, 0x25, 0x3b, 0x4d, 0xfd, 0xec]
    );
}

#[test]
fn offset_restore_encoder_state() {
    assert_eq!(
        mem::offset_of!(NV_ENCODE_API_FUNCTION_LIST, nvEncRestoreEncoderState),
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
unsafe extern "C" fn mock_open_session_reject(
    _params: *mut NV_ENC_OPEN_ENCODE_SESSION_EX_PARAMS,
    _encoder: *mut *mut c_void,
) -> NVENCSTATUS {
    NV_ENC_ERR_INVALID_VERSION
}

/// Mock: validates version fields like a real SDK 12.2 driver, returns success.
unsafe extern "C" fn mock_open_session_validate(
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
            Some(mock_open_session_reject), // should never be called
            0xDEAD as *mut c_void,
            NV_ENC_DEVICE_TYPE_DIRECTX,
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
            Some(mock_open_session_validate),
            0xDEAD as *mut c_void,
            NV_ENC_DEVICE_TYPE_DIRECTX,
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
            Some(mock_open_session_validate),
            0xDEAD as *mut c_void,
            NV_ENC_DEVICE_TYPE_DIRECTX,
        )
    };
    assert!(result.is_ok(), "expected Ok, got {result:?}");
}

#[test]
fn test_e2e_session_params_have_correct_version_fields() {
    /// Mock that captures and validates the version fields.
    unsafe extern "C" fn mock_check_versions(
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
            Some(mock_check_versions),
            0xDEAD as *mut c_void,
            NV_ENC_DEVICE_TYPE_DIRECTX,
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
            Some(mock_open_session_reject), // rejects regardless
            0xDEAD as *mut c_void,
            NV_ENC_DEVICE_TYPE_DIRECTX,
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
    let session_open_errors: &[(NVENCSTATUS, &str)] = &[
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
        let error_msg = format!("nvEncOpenEncodeSession failed: {status_str} (status={code})");
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

// ── E2E mock test for nvEncGetEncodePresetConfigEx ──

/// Mock that validates GUID parameters match SDK values.
unsafe extern "C" fn mock_get_preset_config_ex(
    _encoder: *mut c_void,
    encode_guid: GUID,
    preset_guid: GUID,
    tuning_info: NV_ENC_TUNING_INFO,
    preset_config: *mut NV_ENC_PRESET_CONFIG,
) -> NVENCSTATUS {
    // Reject unknown codec GUIDs
    if encode_guid != NV_ENC_CODEC_HEVC_GUID && encode_guid != NV_ENC_CODEC_H264_GUID {
        return NV_ENC_ERR_INVALID_PARAM;
    }

    // Reject unknown preset GUIDs
    if preset_guid != NV_ENC_PRESET_P1_GUID {
        return NV_ENC_ERR_INVALID_PARAM;
    }

    // Reject undefined tuning info
    if tuning_info == NV_ENC_TUNING_INFO_UNDEFINED {
        return NV_ENC_ERR_INVALID_PARAM;
    }

    // Validate version fields on the preset config struct
    let pc = unsafe { &*preset_config };
    if pc.version != nvencapi_struct_version_high(5) {
        return NV_ENC_ERR_INVALID_VERSION;
    }
    if pc.presetCfg.version != nvencapi_struct_version_high(9) {
        return NV_ENC_ERR_INVALID_VERSION;
    }

    NV_ENC_SUCCESS
}

#[test]
fn test_e2e_preset_config_hevc_succeeds_with_correct_guids() {
    let mut preset_config = NV_ENC_PRESET_CONFIG::new_versioned();
    let status = unsafe {
        mock_get_preset_config_ex(
            0xBEEF as *mut c_void,
            NV_ENC_CODEC_HEVC_GUID,
            NV_ENC_PRESET_P1_GUID,
            NV_ENC_TUNING_INFO_ULTRA_LOW_LATENCY,
            &raw mut preset_config,
        )
    };
    assert_eq!(status, NV_ENC_SUCCESS);
}

#[test]
fn test_e2e_preset_config_h264_succeeds_with_correct_guids() {
    let mut preset_config = NV_ENC_PRESET_CONFIG::new_versioned();
    let status = unsafe {
        mock_get_preset_config_ex(
            0xBEEF as *mut c_void,
            NV_ENC_CODEC_H264_GUID,
            NV_ENC_PRESET_P1_GUID,
            NV_ENC_TUNING_INFO_ULTRA_LOW_LATENCY,
            &raw mut preset_config,
        )
    };
    assert_eq!(status, NV_ENC_SUCCESS);
}

#[test]
fn test_e2e_preset_config_rejects_invalid_codec_guid() {
    let bogus_guid = GUID {
        Data1: 0xDEADBEEF,
        Data2: 0,
        Data3: 0,
        Data4: [0; 8],
    };
    let mut preset_config = NV_ENC_PRESET_CONFIG::new_versioned();
    let status = unsafe {
        mock_get_preset_config_ex(
            0xBEEF as *mut c_void,
            bogus_guid,
            NV_ENC_PRESET_P1_GUID,
            NV_ENC_TUNING_INFO_ULTRA_LOW_LATENCY,
            &raw mut preset_config,
        )
    };
    assert_eq!(status, NV_ENC_ERR_INVALID_PARAM);
}

#[test]
fn test_e2e_preset_config_rejects_invalid_preset_guid() {
    let bogus_guid = GUID {
        Data1: 0xCAFEBABE,
        Data2: 0,
        Data3: 0,
        Data4: [0; 8],
    };
    let mut preset_config = NV_ENC_PRESET_CONFIG::new_versioned();
    let status = unsafe {
        mock_get_preset_config_ex(
            0xBEEF as *mut c_void,
            NV_ENC_CODEC_HEVC_GUID,
            bogus_guid,
            NV_ENC_TUNING_INFO_ULTRA_LOW_LATENCY,
            &raw mut preset_config,
        )
    };
    assert_eq!(status, NV_ENC_ERR_INVALID_PARAM);
}

#[test]
fn test_e2e_preset_config_rejects_undefined_tuning_info() {
    let mut preset_config = NV_ENC_PRESET_CONFIG::new_versioned();
    let status = unsafe {
        mock_get_preset_config_ex(
            0xBEEF as *mut c_void,
            NV_ENC_CODEC_HEVC_GUID,
            NV_ENC_PRESET_P1_GUID,
            NV_ENC_TUNING_INFO_UNDEFINED,
            &raw mut preset_config,
        )
    };
    assert_eq!(status, NV_ENC_ERR_INVALID_PARAM);
}

// ── NV_ENC_BUFFER_FORMAT values must match SDK 12.2 nvEncodeAPI.h ──

#[test]
fn buffer_format_values_match_sdk_12_2() {
    // Values from nvEncodeAPI.h lines 379-407 (SDK 12.2)
    // Note: These are now constants, not enum variants, so we test the constant values directly
    assert_eq!(
        _NV_ENC_BUFFER_FORMAT_NV_ENC_BUFFER_FORMAT_UNDEFINED,
        0x00000000
    );
    assert_eq!(_NV_ENC_BUFFER_FORMAT_NV_ENC_BUFFER_FORMAT_NV12, 0x00000001);
    assert_eq!(_NV_ENC_BUFFER_FORMAT_NV_ENC_BUFFER_FORMAT_YV12, 0x00000010);
    assert_eq!(_NV_ENC_BUFFER_FORMAT_NV_ENC_BUFFER_FORMAT_IYUV, 0x00000100);
    assert_eq!(
        _NV_ENC_BUFFER_FORMAT_NV_ENC_BUFFER_FORMAT_YUV444,
        0x00001000
    );
    assert_eq!(
        _NV_ENC_BUFFER_FORMAT_NV_ENC_BUFFER_FORMAT_YUV420_10BIT, 0x00010000,
        "YUV420_10BIT must be 0x00010000 per SDK 12.2, not 0x01000000"
    );
    assert_eq!(
        _NV_ENC_BUFFER_FORMAT_NV_ENC_BUFFER_FORMAT_YUV444_10BIT, 0x00100000,
        "YUV444_10BIT (0x00100000) must exist per SDK 12.2"
    );
    assert_eq!(
        _NV_ENC_BUFFER_FORMAT_NV_ENC_BUFFER_FORMAT_ARGB, 0x01000000,
        "ARGB must be 0x01000000 per SDK 12.2, not 0x02000000"
    );
    assert_eq!(
        _NV_ENC_BUFFER_FORMAT_NV_ENC_BUFFER_FORMAT_ARGB10,
        0x02000000
    );
    assert_eq!(_NV_ENC_BUFFER_FORMAT_NV_ENC_BUFFER_FORMAT_AYUV, 0x04000000);
    assert_eq!(_NV_ENC_BUFFER_FORMAT_NV_ENC_BUFFER_FORMAT_ABGR, 0x10000000);
    assert_eq!(
        _NV_ENC_BUFFER_FORMAT_NV_ENC_BUFFER_FORMAT_ABGR10,
        0x20000000
    );
    assert_eq!(
        _NV_ENC_BUFFER_FORMAT_NV_ENC_BUFFER_FORMAT_U8, 0x40000000,
        "U8 (0x40000000) must exist per SDK 12.2"
    );
}

// ── NV_ENC_BUFFER_USAGE constants must match SDK 12.2 ──

#[test]
fn buffer_usage_constants_match_sdk_12_2() {
    // SDK 12.2 nvEncodeAPI.h lines 777-784
    assert_eq!(NV_ENC_INPUT_IMAGE, 0x0, "NV_ENC_INPUT_IMAGE must be 0x0");
    assert_eq!(NV_ENC_OUTPUT_MOTION_VECTOR, 0x1);
    assert_eq!(NV_ENC_OUTPUT_BITSTREAM, 0x2);
    assert_eq!(NV_ENC_OUTPUT_RECON, 0x4);
}

#[test]
fn register_resource_default_buffer_usage_is_input_image() {
    let res = NV_ENC_REGISTER_RESOURCE::default();
    assert_eq!(
        res.bufferUsage, NV_ENC_INPUT_IMAGE,
        "default bufferUsage must be NV_ENC_INPUT_IMAGE (0x0), not 1"
    );
}

// ── E2E mock test for nvEncRegisterResource ──

/// Mock that validates `NV_ENC_REGISTER_RESOURCE` fields like a real SDK 12.2 driver.
unsafe extern "C" fn mock_register_resource(
    _encoder: *mut c_void,
    params: *mut NV_ENC_REGISTER_RESOURCE,
) -> NVENCSTATUS {
    let p = unsafe { &mut *params };

    // Validate struct version
    if p.version != nvencapi_struct_version(5) {
        return NV_ENC_ERR_INVALID_VERSION;
    }

    // Validate buffer format is a known SDK 12.2 value
    let valid_formats: &[u32] = &[
        0x00000001, // NV12
        0x00000010, // YV12
        0x00000100, // IYUV
        0x00001000, // YUV444
        0x00010000, // YUV420_10BIT
        0x00100000, // YUV444_10BIT
        0x01000000, // ARGB
        0x02000000, // ARGB10
        0x04000000, // AYUV
        0x10000000, // ABGR
        0x20000000, // ABGR10
        0x40000000, // U8
    ];
    if !valid_formats.contains(&(p.bufferFormat as u32)) {
        return NV_ENC_ERR_UNIMPLEMENTED;
    }

    // Validate bufferUsage is a known value
    if p.bufferUsage > 0x4 {
        return NV_ENC_ERR_INVALID_PARAM;
    }

    // Validate resource type
    if p.resourceType != NV_ENC_INPUT_RESOURCE_TYPE_DIRECTX
        && p.resourceType != _NV_ENC_INPUT_RESOURCE_TYPE_NV_ENC_INPUT_RESOURCE_TYPE_CUDADEVICEPTR
        && p.resourceType != _NV_ENC_INPUT_RESOURCE_TYPE_NV_ENC_INPUT_RESOURCE_TYPE_CUDAARRAY
        && p.resourceType != _NV_ENC_INPUT_RESOURCE_TYPE_NV_ENC_INPUT_RESOURCE_TYPE_OPENGL_TEX
    {
        return NV_ENC_ERR_INVALID_PARAM;
    }

    // Simulate successful registration
    p.registeredResource = 0xABCD_0001 as *mut c_void;
    NV_ENC_SUCCESS
}

#[test]
#[allow(clippy::field_reassign_with_default)]
fn test_e2e_register_resource_with_argb_format_succeeds() {
    let mut params = NV_ENC_REGISTER_RESOURCE::new_versioned();
    params.resourceType = NV_ENC_INPUT_RESOURCE_TYPE_DIRECTX;
    params.width = 1920;
    params.height = 1080;
    params.bufferFormat = NV_ENC_BUFFER_FORMAT_ARGB;
    params.bufferUsage = NV_ENC_INPUT_IMAGE;

    let status = unsafe { mock_register_resource(0xBEEF as *mut c_void, &raw mut params) };
    assert_eq!(
        status,
        NV_ENC_SUCCESS,
        "register with ARGB format should succeed, got {} (status={})",
        nvenc_status_to_string(status),
        status
    );
}

#[test]
#[allow(clippy::field_reassign_with_default)]
fn test_e2e_register_resource_rejects_invalid_buffer_format() {
    let mut params = NV_ENC_REGISTER_RESOURCE::new_versioned();
    params.resourceType = NV_ENC_INPUT_RESOURCE_TYPE_DIRECTX;
    params.width = 1920;
    params.height = 1080;
    // Write an invalid buffer format value (0x08000000) directly into the field's memory
    // to simulate what a buggy caller would send to the driver.
    unsafe {
        let fmt_ptr: *mut u32 = std::ptr::addr_of_mut!(params.bufferFormat).cast();
        fmt_ptr.write(0x08000000);
    }
    params.bufferUsage = NV_ENC_INPUT_IMAGE;

    let status = unsafe { mock_register_resource(0xBEEF as *mut c_void, &raw mut params) };
    assert_eq!(
        status, NV_ENC_ERR_UNIMPLEMENTED,
        "invalid buffer format should be rejected with UNIMPLEMENTED"
    );
}

#[test]
#[allow(clippy::field_reassign_with_default)]
fn test_e2e_register_resource_default_params_succeed() {
    // Verify that the Default impl produces params that a real driver would accept
    // (after setting required fields: width, height, bufferFormat, resource ptr)
    let mut params = NV_ENC_REGISTER_RESOURCE::new_versioned();
    params.width = 1920;
    params.height = 1080;
    params.resourceToRegister = 0xDEAD as *mut c_void;
    params.bufferFormat = NV_ENC_BUFFER_FORMAT_ARGB;

    let status = unsafe { mock_register_resource(0xBEEF as *mut c_void, &raw mut params) };
    assert_eq!(
        status,
        NV_ENC_SUCCESS,
        "default params with ARGB should succeed, got {} (status={})",
        nvenc_status_to_string(status),
        status
    );
}

// ── Preset config zero-initialization (matches FFmpeg / NVIDIA samples) ──
//
// The SDK requires NV_ENC_PRESET_CONFIG to be zero-initialized before calling
// nvEncGetEncodePresetConfigEx. Only the version fields should be set.
// Non-zero values in output fields (gopLength, frameIntervalP, rcParams, etc.)
// cause the real driver to return NV_ENC_ERR_INVALID_PARAM.

#[test]
fn preset_config_default_inner_cfg_only_has_version_set() {
    let pc = NV_ENC_PRESET_CONFIG::new_versioned();

    // Outer version must be set
    assert_eq!(pc.version, nvencapi_struct_version_high(5));

    // Inner presetCfg.version must be set
    assert_eq!(pc.presetCfg.version, nvencapi_struct_version_high(9));

    // All other fields in presetCfg must be zero (they are [out] parameters
    // filled by the driver). Non-zero values cause NV_ENC_ERR_INVALID_PARAM.
    assert_eq!(
        pc.presetCfg.gopLength, 0,
        "presetCfg.gopLength must be 0 before query (it's an [out] field)"
    );
    assert_eq!(
        pc.presetCfg.frameIntervalP, 0,
        "presetCfg.frameIntervalP must be 0 before query (it's an [out] field)"
    );
    assert_eq!(
        pc.presetCfg.rcParams.rateControlMode, 0,
        "presetCfg.rcParams.rateControlMode must be 0 before query"
    );
    // rcParams.version should also be 0 — it's part of the output struct
    assert_eq!(
        pc.presetCfg.rcParams.version, 0,
        "presetCfg.rcParams.version must be 0 before query"
    );
}

/// Strict mock that rejects non-zero output fields, matching real driver behavior.
unsafe extern "C" fn mock_preset_config_strict(
    _encoder: *mut c_void,
    encode_guid: GUID,
    preset_guid: GUID,
    tuning_info: NV_ENC_TUNING_INFO,
    preset_config: *mut NV_ENC_PRESET_CONFIG,
) -> NVENCSTATUS {
    let pc = unsafe { &*preset_config };

    // Check versions
    if pc.version != nvencapi_struct_version_high(5) {
        return NV_ENC_ERR_INVALID_VERSION;
    }
    if pc.presetCfg.version != nvencapi_struct_version_high(9) {
        return NV_ENC_ERR_INVALID_VERSION;
    }

    // Reject non-zero output fields (real driver behavior)
    if pc.presetCfg.gopLength != 0
        || pc.presetCfg.frameIntervalP != 0
        || pc.presetCfg.rcParams.rateControlMode != 0
        || pc.presetCfg.rcParams.version != 0
    {
        return NV_ENC_ERR_INVALID_PARAM;
    }

    // Validate input params
    if encode_guid != NV_ENC_CODEC_HEVC_GUID && encode_guid != NV_ENC_CODEC_H264_GUID {
        return NV_ENC_ERR_INVALID_PARAM;
    }
    if preset_guid != NV_ENC_PRESET_P1_GUID {
        return NV_ENC_ERR_INVALID_PARAM;
    }
    if tuning_info == NV_ENC_TUNING_INFO_UNDEFINED {
        return NV_ENC_ERR_INVALID_PARAM;
    }

    // Fill in the preset config like a real driver would
    let pc = unsafe { &mut *preset_config };
    pc.presetCfg.gopLength = 120;
    pc.presetCfg.frameIntervalP = 1;
    pc.presetCfg.rcParams.version = nvencapi_struct_version(1);
    pc.presetCfg.rcParams.rateControlMode = NV_ENC_PARAMS_RC_VBR;
    pc.presetCfg.rcParams.averageBitRate = 10_000_000;
    pc.presetCfg.rcParams.maxBitRate = 20_000_000;

    // Set codec-specific defaults like a real driver would
    if encode_guid == NV_ENC_CODEC_HEVC_GUID {
        let hevc = unsafe { &mut pc.presetCfg.encodeCodecConfig.hevcConfig };
        hevc.idrPeriod = 120;
        hevc.maxNumRefFramesInDPB = 4;
    } else {
        let h264 = unsafe { &mut pc.presetCfg.encodeCodecConfig.h264Config };
        h264.idrPeriod = 120;
        h264.maxNumRefFrames = 4;
    }

    NV_ENC_SUCCESS
}

#[test]
fn test_e2e_strict_mock_accepts_zero_init_preset_config() {
    let mut preset_config = NV_ENC_PRESET_CONFIG::new_versioned();
    let status = unsafe {
        mock_preset_config_strict(
            0xBEEF as *mut c_void,
            NV_ENC_CODEC_HEVC_GUID,
            NV_ENC_PRESET_P1_GUID,
            NV_ENC_TUNING_INFO_ULTRA_LOW_LATENCY,
            &raw mut preset_config,
        )
    };
    assert_eq!(
        status,
        NV_ENC_SUCCESS,
        "zero-init preset config should succeed, got {} (status={})",
        nvenc_status_to_string(status),
        status
    );

    // Verify the mock filled in the preset config
    assert_eq!(preset_config.presetCfg.gopLength, 120);
    assert_eq!(preset_config.presetCfg.frameIntervalP, 1);
}
