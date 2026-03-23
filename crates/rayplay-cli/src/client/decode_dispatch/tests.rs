use super::*;
use crate::client::test_helper::NullDecoder;

fn make_packet() -> EncodedPacket {
    EncodedPacket::new(vec![0u8; 4], false, 0, 0)
}

#[test]
fn test_decode_and_dispatch_sent() {
    let mut decoder = NullDecoder {
        emit: true,
        fail: false,
    };
    let (tx, rx) = crossbeam_channel::bounded(1);
    let result = decode_and_dispatch(&mut decoder, &make_packet(), &tx);
    assert_eq!(result, DispatchResult::Sent);
    assert_eq!(rx.len(), 1);
}

#[test]
fn test_decode_and_dispatch_channel_full() {
    let mut decoder = NullDecoder {
        emit: true,
        fail: false,
    };
    let (tx, _rx) = crossbeam_channel::bounded(0); // zero-capacity → always full
    let result = decode_and_dispatch(&mut decoder, &make_packet(), &tx);
    assert_eq!(result, DispatchResult::Dropped);
}

#[test]
fn test_decode_and_dispatch_channel_disconnected() {
    let mut decoder = NullDecoder {
        emit: true,
        fail: false,
    };
    let (tx, rx) = crossbeam_channel::bounded(1);
    drop(rx); // disconnect
    let result = decode_and_dispatch(&mut decoder, &make_packet(), &tx);
    assert_eq!(result, DispatchResult::ChannelClosed);
}

#[test]
fn test_decode_and_dispatch_no_frame() {
    let mut decoder = NullDecoder {
        emit: false,
        fail: false,
    };
    let (tx, _rx) = crossbeam_channel::bounded(1);
    let result = decode_and_dispatch(&mut decoder, &make_packet(), &tx);
    assert_eq!(result, DispatchResult::NoFrame);
}

#[test]
fn test_decode_and_dispatch_decode_error() {
    let mut decoder = NullDecoder {
        emit: false,
        fail: true,
    };
    let (tx, _rx) = crossbeam_channel::bounded(1);
    let result = decode_and_dispatch(&mut decoder, &make_packet(), &tx);
    assert_eq!(result, DispatchResult::DecodeError);
}
