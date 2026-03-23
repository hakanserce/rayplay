use super::*;

#[test]
fn test_raw_frame_new_stores_fields() {
    let data = vec![1u8, 2, 3, 4];
    let frame = RawFrame::new(data.clone(), 1, 1, 4, 1000);
    assert_eq!(frame.data, data);
    assert_eq!(frame.width, 1);
    assert_eq!(frame.height, 1);
    assert_eq!(frame.stride, 4);
    assert_eq!(frame.timestamp_us, 1000);
}

#[test]
fn test_raw_frame_expected_size_1080p() {
    let frame = RawFrame::new(vec![], 1920, 1080, 1920 * 4, 0);
    assert_eq!(frame.expected_size(), 1920 * 1080 * 4);
}

#[test]
fn test_raw_frame_expected_size_4k() {
    let frame = RawFrame::new(vec![], 3840, 2160, 3840 * 4, 0);
    assert_eq!(frame.expected_size(), 3840 * 2160 * 4);
}

#[test]
fn test_raw_frame_expected_size_zero_for_empty_dimensions() {
    let frame = RawFrame::new(vec![], 0, 0, 0, 0);
    assert_eq!(frame.expected_size(), 0);
}

#[test]
fn test_raw_frame_clone() {
    let frame = RawFrame::new(vec![0xABu8; 8], 2, 1, 8, 999);
    let cloned = frame.clone();
    assert_eq!(cloned.data, frame.data);
    assert_eq!(cloned.timestamp_us, frame.timestamp_us);
}
