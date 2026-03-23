use super::*;

#[test]
fn test_encoded_packet_new_stores_fields() {
    let data = vec![0u8, 1, 2, 3];
    let pkt = EncodedPacket::new(data.clone(), true, 1000, 16_667);
    assert_eq!(pkt.data, data);
    assert!(pkt.is_keyframe);
    assert_eq!(pkt.timestamp_us, 1000);
    assert_eq!(pkt.duration_us, 16_667);
}

#[test]
fn test_encoded_packet_len() {
    let pkt = EncodedPacket::new(vec![0u8; 128], false, 0, 0);
    assert_eq!(pkt.len(), 128);
}

#[test]
fn test_encoded_packet_is_empty_true_for_empty_data() {
    let pkt = EncodedPacket::new(vec![], false, 0, 0);
    assert!(pkt.is_empty());
}

#[test]
fn test_encoded_packet_is_empty_false_for_non_empty_data() {
    let pkt = EncodedPacket::new(vec![0u8], false, 0, 0);
    assert!(!pkt.is_empty());
}

#[test]
fn test_encoded_packet_clone() {
    let pkt = EncodedPacket::new(vec![1, 2, 3], true, 42, 100);
    let cloned = pkt.clone();
    assert_eq!(cloned.data, pkt.data);
    assert_eq!(cloned.is_keyframe, pkt.is_keyframe);
    assert_eq!(cloned.timestamp_us, pkt.timestamp_us);
    assert_eq!(cloned.duration_us, pkt.duration_us);
}

#[test]
fn test_encoded_packet_non_keyframe() {
    let pkt = EncodedPacket::new(vec![0u8; 64], false, 16_667, 16_667);
    assert!(!pkt.is_keyframe);
}
