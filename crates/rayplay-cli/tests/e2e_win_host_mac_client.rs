//! End-to-end integration test: Windows host → macOS client simulation.
//!
//! Simulates a Windows host sending H.264 encoded frames (via OpenH264, standing
//! in for NVENC) to a macOS client decoding with VideoToolbox (VtDecoder).
//!
//! Requires macOS hardware and the `hw-codec-tests` feature flag.
#![cfg(all(target_os = "macos", feature = "hw-codec-tests"))]

#[path = "e2e_helpers.rs"]
mod e2e_helpers;

use std::time::Duration;

use e2e_helpers::{
    SyntheticCapturer, collect_frames, run_client_recv_decode_render, run_host_encode_send,
    setup_transport,
};
use rayplay_video::{
    Codec, DecodedFrame, EncoderConfig, OpenH264Encoder, VtDecoder, capture::ScreenCapturer,
    decoder::VideoDecoder, encoder::VideoEncoder,
};
use tokio_util::sync::CancellationToken;

fn create_encoder(width: u32, height: u32, fps: u32) -> Box<dyn VideoEncoder> {
    let config = EncoderConfig::with_codec(width, height, fps, Codec::H264);
    Box::new(OpenH264Encoder::new(config).expect("OpenH264Encoder init"))
}

fn create_vt_decoder() -> Box<dyn VideoDecoder> {
    Box::new(VtDecoder::new(Codec::H264).expect("VtDecoder init"))
}

fn create_capturer(width: u32, height: u32, frame_count: u32) -> Box<dyn ScreenCapturer> {
    Box::new(SyntheticCapturer::new(width, height, frame_count))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_h264_vtdecoder_roundtrip() {
    let (server, client) = setup_transport().await.expect("transport setup");

    let token = CancellationToken::new();
    let (frame_tx, frame_rx) = crossbeam_channel::bounded::<DecodedFrame>(8);

    let host_token = token.clone();
    let host = tokio::spawn(async move {
        run_host_encode_send(
            server,
            create_capturer(320, 240, 5),
            create_encoder(320, 240, 30),
            host_token,
        )
        .await
    });

    let client_token = token.clone();
    let client_handle = tokio::spawn(async move {
        run_client_recv_decode_render(client, create_vt_decoder(), frame_tx, client_token, 320, 240)
            .await
    });

    let frames = collect_frames(&frame_rx, 3, Duration::from_secs(15));
    token.cancel();

    let _ = host.await;
    let _ = client_handle.await;

    assert!(
        !frames.is_empty(),
        "expected at least 1 frame from VtDecoder"
    );
    assert_eq!(frames[0].width, 320);
    assert_eq!(frames[0].height, 240);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_hardware_frame_has_iosurface() {
    let (server, client) = setup_transport().await.expect("transport setup");

    let token = CancellationToken::new();
    let (frame_tx, frame_rx) = crossbeam_channel::bounded::<DecodedFrame>(8);

    let host_token = token.clone();
    let host = tokio::spawn(async move {
        run_host_encode_send(
            server,
            create_capturer(320, 240, 3),
            create_encoder(320, 240, 30),
            host_token,
        )
        .await
    });

    let client_token = token.clone();
    let client_handle = tokio::spawn(async move {
        run_client_recv_decode_render(client, create_vt_decoder(), frame_tx, client_token, 320, 240)
            .await
    });

    let frames = collect_frames(&frame_rx, 1, Duration::from_secs(15));
    token.cancel();

    let _ = host.await;
    let _ = client_handle.await;

    assert!(!frames.is_empty(), "expected at least 1 hardware frame");
    assert!(
        frames[0].is_hardware_frame,
        "VtDecoder should produce hardware-backed frames"
    );
    assert!(
        frames[0].iosurface.is_some(),
        "hardware frame should have an IOSurface handle"
    );
}
