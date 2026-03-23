use super::*;

// ── CaptureConfig ──────────────────────────────────────────────────────────

#[test]
fn test_capture_config_default_fps() {
    assert_eq!(CaptureConfig::default().target_fps, 60);
}

#[test]
fn test_capture_config_default_timeout() {
    assert_eq!(CaptureConfig::default().acquire_timeout_ms, 100);
}

#[test]
fn test_capture_config_clone() {
    let cfg = CaptureConfig {
        target_fps: 30,
        acquire_timeout_ms: 50,
    };
    let cloned = cfg.clone();
    assert_eq!(cloned.target_fps, 30);
    assert_eq!(cloned.acquire_timeout_ms, 50);
}

#[test]
fn test_capture_config_serde_roundtrip() {
    let cfg = CaptureConfig {
        target_fps: 120,
        acquire_timeout_ms: 200,
    };
    let json = serde_json::to_string(&cfg).expect("serialize");
    let back: CaptureConfig = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back.target_fps, 120);
    assert_eq!(back.acquire_timeout_ms, 200);
}

// ── CapturedFrame ──────────────────────────────────────────────────────────

#[test]
fn test_captured_frame_buffer_size() {
    let frame = CapturedFrame {
        width: 1920,
        height: 1080,
        stride: 7680, // 1920 * 4
        data: vec![0u8; 7680 * 1080],
        timestamp: Instant::now(),
    };
    assert_eq!(frame.buffer_size(), 7680 * 1080);
}

#[test]
fn test_captured_frame_buffer_size_with_padding() {
    // Stride may include alignment padding (e.g. 7936 instead of 7680).
    let stride = 7936_u32;
    let height = 1080_u32;
    let frame = CapturedFrame {
        width: 1920,
        height,
        stride,
        data: vec![0u8; (stride * height) as usize],
        timestamp: Instant::now(),
    };
    assert_eq!(frame.buffer_size(), (stride * height) as usize);
}

// ── CaptureError ──────────────────────────────────────────────────────────

#[test]
fn test_capture_error_unsupported_platform_display() {
    let msg = CaptureError::UnsupportedPlatform.to_string();
    assert!(msg.contains("not supported"));
}

#[test]
fn test_capture_error_initialization_failed_display() {
    let msg = CaptureError::InitializationFailed("no adapter".into()).to_string();
    assert!(msg.contains("initialize"));
    assert!(msg.contains("no adapter"));
}

#[test]
fn test_capture_error_acquire_failed_display() {
    let msg = CaptureError::AcquireFailed("DXGI_ERROR_ACCESS_LOST".into()).to_string();
    assert!(msg.contains("acquire"));
    assert!(msg.contains("DXGI_ERROR_ACCESS_LOST"));
}

#[test]
fn test_capture_error_timeout_display() {
    let msg = CaptureError::Timeout(Duration::from_millis(100)).to_string();
    assert!(msg.contains("timed out"));
}

// ── build_frame ─────────────────────────────────────────────────────────

#[test]
fn test_build_frame_dimensions() {
    let data = vec![0u8; 8 * 4 * 6];
    let frame = build_frame(data, 8, 6);
    assert_eq!(frame.width, 8);
    assert_eq!(frame.height, 6);
}

#[test]
fn test_build_frame_stride_from_data_length() {
    // 10px wide, but data has 48 bytes per row (padded to 12 pixels).
    let data = vec![0u8; 48 * 5];
    let frame = build_frame(data, 10, 5);
    assert_eq!(frame.stride, 48);
}

#[test]
fn test_build_frame_stride_no_padding() {
    let data = vec![0u8; 10 * 4 * 5];
    let frame = build_frame(data, 10, 5);
    assert_eq!(frame.stride, 40);
}

#[test]
fn test_build_frame_preserves_data() {
    let data: Vec<u8> = (0..16).collect();
    let expected = data.clone();
    let frame = build_frame(data, 2, 2);
    assert_eq!(frame.data, expected);
}

#[test]
fn test_build_frame_buffer_size() {
    let data = vec![0xFFu8; 1920 * 4 * 1080];
    let frame = build_frame(data, 1920, 1080);
    assert_eq!(frame.buffer_size(), 1920 * 4 * 1080);
}

#[test]
fn test_build_frame_zero_height_uses_width() {
    let frame = build_frame(vec![], 0, 0);
    assert_eq!(frame.stride, 0);
    assert!(frame.data.is_empty());
}

#[test]
fn test_build_frame_timestamp_is_recent() {
    let before = Instant::now();
    let frame = build_frame(vec![0u8; 4], 1, 1);
    let after = Instant::now();
    assert!(frame.timestamp >= before);
    assert!(frame.timestamp <= after);
}
