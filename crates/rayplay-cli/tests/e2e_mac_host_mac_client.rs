//! End-to-end integration test: macOS host → macOS client simulation.
//!
//! Simulates a Mac host capturing at retina resolution via `ScreenCaptureKit`,
//! encoding with software H.264 (standing in for the real SCK + encoder),
//! and sending to a Mac client decoding with `VideoToolbox` (`VtDecoder`).
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
async fn test_retina_resolution_roundtrip() {
    let (server, client) = setup_transport().await.expect("transport setup");

    let token = CancellationToken::new();
    let (frame_tx, frame_rx) = crossbeam_channel::bounded::<DecodedFrame>(8);

    // 1710×1112 is a typical Mac retina display resolution (both even for OpenH264)
    let host_token = token.clone();
    let host = tokio::spawn(async move {
        run_host_encode_send(
            server,
            create_capturer(1710, 1112, 3),
            create_encoder(1710, 1112, 60),
            host_token,
        )
        .await
    });

    let client_token = token.clone();
    let client_handle = tokio::spawn(async move {
        run_client_recv_decode_render(
            client,
            create_vt_decoder(),
            frame_tx,
            client_token,
            1710,
            1112,
        )
        .await
    });

    let frames = collect_frames(&frame_rx, 1, Duration::from_secs(30));
    token.cancel();

    let _ = host.await;
    let _ = client_handle.await;

    assert!(
        !frames.is_empty(),
        "expected at least 1 frame at retina resolution"
    );
    assert_eq!(frames[0].width, 1710);
    assert_eq!(frames[0].height, 1112);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_multi_frame_at_retina_resolution() {
    let (server, client) = setup_transport().await.expect("transport setup");

    let token = CancellationToken::new();
    let (frame_tx, frame_rx) = crossbeam_channel::bounded::<DecodedFrame>(16);

    let host_token = token.clone();
    let host = tokio::spawn(async move {
        run_host_encode_send(
            server,
            create_capturer(1710, 1112, 5),
            create_encoder(1710, 1112, 60),
            host_token,
        )
        .await
    });

    let client_token = token.clone();
    let client_handle = tokio::spawn(async move {
        run_client_recv_decode_render(
            client,
            create_vt_decoder(),
            frame_tx,
            client_token,
            1710,
            1112,
        )
        .await
    });

    let frames = collect_frames(&frame_rx, 5, Duration::from_secs(30));
    token.cancel();

    let _ = host.await;
    let _ = client_handle.await;

    assert!(
        frames.len() >= 3,
        "expected at least 3 frames at retina resolution, got {}",
        frames.len()
    );
    for frame in &frames {
        assert_eq!(frame.width, 1710);
        assert_eq!(frame.height, 1112);
    }
}
