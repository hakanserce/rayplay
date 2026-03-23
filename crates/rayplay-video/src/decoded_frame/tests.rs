use super::*;

// ── PixelFormat ────────────────────────────────────────────────────────────

#[test]
fn test_pixel_format_bgra8_bytes_per_pixel() {
    assert_eq!(PixelFormat::Bgra8.bytes_per_pixel(), Some(4));
}

#[test]
fn test_pixel_format_nv12_bytes_per_pixel_is_none() {
    assert_eq!(PixelFormat::Nv12.bytes_per_pixel(), None);
}

#[test]
fn test_pixel_format_clone_and_eq() {
    let a = PixelFormat::Bgra8;
    let b = a.clone();
    assert_eq!(a, b);
    assert_ne!(a, PixelFormat::Nv12);
}

#[test]
fn test_pixel_format_debug() {
    assert!(format!("{:?}", PixelFormat::Bgra8).contains("Bgra8"));
    assert!(format!("{:?}", PixelFormat::Nv12).contains("Nv12"));
}

// ── DecodedFrame::new_cpu ──────────────────────────────────────────────────

#[test]
fn test_decoded_frame_new_cpu_stores_all_fields() {
    let data = vec![1u8, 2, 3, 4];
    let frame = DecodedFrame::new_cpu(data.clone(), 1, 1, 4, PixelFormat::Bgra8, 1000);
    assert_eq!(frame.data, data);
    assert_eq!(frame.width, 1);
    assert_eq!(frame.height, 1);
    assert_eq!(frame.stride, 4);
    assert_eq!(frame.format, PixelFormat::Bgra8);
    assert_eq!(frame.timestamp_us, 1000);
    assert!(!frame.is_hardware_frame);
}

#[test]
fn test_decoded_frame_new_cpu_clone() {
    let frame = DecodedFrame::new_cpu(vec![0xABu8; 8], 2, 1, 8, PixelFormat::Nv12, 42);
    let cloned = frame.clone();
    assert_eq!(cloned.data, frame.data);
    assert_eq!(cloned.timestamp_us, frame.timestamp_us);
    assert_eq!(cloned.format, frame.format);
}

// ── DecodedFrame::new_hardware ─────────────────────────────────────────────

#[test]
fn test_decoded_frame_new_hardware_has_empty_data() {
    let frame = DecodedFrame::new_hardware_test_stub(1920, 1080, 1920 * 4, PixelFormat::Nv12, 0);
    assert!(frame.data.is_empty());
    assert!(frame.is_hardware_frame);
    assert_eq!(frame.width, 1920);
    assert_eq!(frame.height, 1080);
    assert_eq!(frame.format, PixelFormat::Nv12);
}

// ── expected_data_size ─────────────────────────────────────────────────────

#[test]
fn test_expected_data_size_hardware_frame_is_zero() {
    let frame = DecodedFrame::new_hardware_test_stub(1920, 1080, 1920 * 4, PixelFormat::Nv12, 0);
    assert_eq!(frame.expected_data_size(), 0);
}

#[test]
fn test_expected_data_size_bgra8_1080p() {
    let frame = DecodedFrame::new_cpu(vec![], 1920, 1080, 1920 * 4, PixelFormat::Bgra8, 0);
    assert_eq!(frame.expected_data_size(), 1920 * 4 * 1080);
}

#[test]
fn test_expected_data_size_bgra8_4k() {
    let frame = DecodedFrame::new_cpu(vec![], 3840, 2160, 3840 * 4, PixelFormat::Bgra8, 0);
    assert_eq!(frame.expected_data_size(), 3840 * 4 * 2160);
}

#[test]
fn test_expected_data_size_nv12_1080p() {
    // NV12: stride * height * 3 / 2 = 1920 * 1080 * 3 / 2
    let frame = DecodedFrame::new_cpu(vec![], 1920, 1080, 1920, PixelFormat::Nv12, 0);
    assert_eq!(frame.expected_data_size(), 1920 * 1080 * 3 / 2);
}

#[test]
fn test_expected_data_size_zero_dimensions() {
    let frame = DecodedFrame::new_cpu(vec![], 0, 0, 0, PixelFormat::Bgra8, 0);
    assert_eq!(frame.expected_data_size(), 0);
}

#[test]
fn test_expected_data_size_bgra8_stride_wider_than_width() {
    // Hardware alignment: VideoToolbox may pad rows to e.g. 2048 for a 1920px frame.
    let frame = DecodedFrame::new_cpu(vec![], 1920, 1080, 2048 * 4, PixelFormat::Bgra8, 0);
    assert_eq!(frame.expected_data_size(), 2048 * 4 * 1080);
}

#[test]
fn test_expected_data_size_nv12_stride_wider_than_width() {
    // NV12 with hardware stride padding: stride=2048 for 1920px frame.
    let frame = DecodedFrame::new_cpu(vec![], 1920, 1080, 2048, PixelFormat::Nv12, 0);
    assert_eq!(frame.expected_data_size(), 2048 * 1080 * 3 / 2);
}

// ── IoSurfaceHandle (macOS only) ────────────────────────────────────────

#[cfg(target_os = "macos")]
#[test]
fn test_iosurface_handle_debug() {
    // Create a dummy handle using CoreFoundation's CFAllocatorGetDefault
    // (a long-lived CF object safe to retain/release for testing).
    unsafe extern "C" {
        fn CFAllocatorGetDefault() -> *mut std::ffi::c_void;
    }
    let ptr = unsafe { CFAllocatorGetDefault() };
    // Retain so our handle can release it.
    unsafe { CFRetain(ptr.cast_const()) };
    let handle = unsafe { IoSurfaceHandle::from_retained(ptr) };
    let dbg = format!("{handle:?}");
    assert!(dbg.contains("IoSurfaceHandle"));
}

#[cfg(target_os = "macos")]
#[test]
fn test_iosurface_handle_clone() {
    unsafe extern "C" {
        fn CFAllocatorGetDefault() -> *mut std::ffi::c_void;
    }
    let ptr = unsafe { CFAllocatorGetDefault() };
    unsafe { CFRetain(ptr.cast_const()) };
    let handle = unsafe { IoSurfaceHandle::from_retained(ptr) };
    let cloned = handle.clone();
    assert_eq!(handle.as_ptr(), cloned.as_ptr());
    // Both drop without double-free (each was independently retained).
}

#[cfg(target_os = "macos")]
#[test]
fn test_decoded_frame_new_hardware_test_stub_has_no_iosurface() {
    let frame = DecodedFrame::new_hardware_test_stub(1920, 1080, 1920, PixelFormat::Nv12, 0);
    assert!(frame.is_hardware_frame);
    assert!(frame.iosurface.is_none());
    assert!(frame.data.is_empty());
}

#[cfg(target_os = "macos")]
#[test]
fn test_decoded_frame_new_cpu_has_no_iosurface() {
    let frame = DecodedFrame::new_cpu(vec![0; 4], 1, 1, 4, PixelFormat::Bgra8, 0);
    assert!(frame.iosurface.is_none());
}

#[cfg(target_os = "macos")]
#[test]
fn test_decoded_frame_new_hardware_stores_iosurface() {
    unsafe extern "C" {
        fn CFAllocatorGetDefault() -> *mut std::ffi::c_void;
    }
    let ptr = unsafe { CFAllocatorGetDefault() };
    // Extra retain so the handle can release it independently.
    unsafe { CFRetain(ptr.cast_const()) };
    let handle = unsafe { IoSurfaceHandle::from_retained(ptr) };
    let frame = DecodedFrame::new_hardware(64, 64, 64, PixelFormat::Nv12, 99, handle);
    assert!(frame.is_hardware_frame);
    assert!(frame.iosurface.is_some());
    assert!(frame.data.is_empty());
    assert_eq!(frame.timestamp_us, 99);
}

#[cfg(target_os = "macos")]
#[test]
fn test_decoded_frame_new_hardware_clone() {
    unsafe extern "C" {
        fn CFAllocatorGetDefault() -> *mut std::ffi::c_void;
    }
    let ptr = unsafe { CFAllocatorGetDefault() };
    unsafe { CFRetain(ptr.cast_const()) };
    let handle = unsafe { IoSurfaceHandle::from_retained(ptr) };
    let frame = DecodedFrame::new_hardware(4, 4, 4, PixelFormat::Nv12, 0, handle);
    let cloned = frame.clone();
    assert!(cloned.iosurface.is_some());
    assert_eq!(cloned.width, 4);
}
