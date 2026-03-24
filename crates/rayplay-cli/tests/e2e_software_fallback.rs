//! End-to-end integration tests using the full software fallback pipeline.
//!
//! Encoder: `OpenH264` (H.264), Decoder: `OpenH264` (H.264), Transport: QUIC loopback.
//! Runs on all platforms with the `fallback` feature (enabled by default).

#[path = "e2e_helpers.rs"]
mod e2e_helpers;

use std::time::Duration;

use e2e_helpers::{
    SyntheticCapturer, collect_frames, run_client_recv_decode, run_host_encode_send,
    setup_transport,
};
use rayplay_video::{
    Codec, DecodedFrame, EncoderConfig, OpenH264Decoder, OpenH264Encoder, capture::ScreenCapturer,
    decoder::VideoDecoder, encoder::VideoEncoder,
};
use tokio_util::sync::CancellationToken;

fn create_encoder(width: u32, height: u32, fps: u32) -> Box<dyn VideoEncoder> {
    let config = EncoderConfig::with_codec(width, height, fps, Codec::H264);
    Box::new(OpenH264Encoder::new(config).expect("OpenH264Encoder init"))
}

fn create_decoder() -> Box<dyn VideoDecoder> {
    Box::new(OpenH264Decoder::new(Codec::H264).expect("OpenH264Decoder init"))
}

fn create_capturer(width: u32, height: u32, frame_count: u32) -> Box<dyn ScreenCapturer> {
    Box::new(SyntheticCapturer::new(width, height, frame_count))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_single_frame_roundtrip() {
    let (server, client) = setup_transport().await.expect("transport setup");

    let token = CancellationToken::new();
    let (frame_tx, frame_rx) = crossbeam_channel::bounded::<DecodedFrame>(4);

    let host_token = token.clone();
    let host = tokio::spawn(async move {
        run_host_encode_send(
            server,
            create_capturer(64, 64, 1),
            create_encoder(64, 64, 30),
            host_token,
        )
        .await
    });

    let client_token = token.clone();
    let client_handle = tokio::spawn(async move {
        run_client_recv_decode(client, create_decoder(), frame_tx, client_token).await
    });

    let frames = collect_frames(&frame_rx, 1, Duration::from_secs(10));
    token.cancel();

    let _ = host.await;
    let _ = client_handle.await;

    assert_eq!(frames.len(), 1, "expected 1 decoded frame");
    assert_eq!(frames[0].width, 64);
    assert_eq!(frames[0].height, 64);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_multi_frame_stream() {
    let (server, client) = setup_transport().await.expect("transport setup");

    let token = CancellationToken::new();
    let (frame_tx, frame_rx) = crossbeam_channel::bounded::<DecodedFrame>(16);

    let host_token = token.clone();
    let host = tokio::spawn(async move {
        run_host_encode_send(
            server,
            create_capturer(64, 64, 10),
            create_encoder(64, 64, 30),
            host_token,
        )
        .await
    });

    let client_token = token.clone();
    let client_handle = tokio::spawn(async move {
        run_client_recv_decode(client, create_decoder(), frame_tx, client_token).await
    });

    let frames = collect_frames(&frame_rx, 10, Duration::from_secs(15));
    token.cancel();

    let _ = host.await;
    let _ = client_handle.await;

    assert!(
        frames.len() >= 5,
        "expected at least 5 decoded frames, got {}",
        frames.len()
    );
    for frame in &frames {
        assert_eq!(frame.width, 64);
        assert_eq!(frame.height, 64);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_decoded_data_non_empty() {
    let (server, client) = setup_transport().await.expect("transport setup");

    let token = CancellationToken::new();
    let (frame_tx, frame_rx) = crossbeam_channel::bounded::<DecodedFrame>(4);

    let host_token = token.clone();
    let host = tokio::spawn(async move {
        run_host_encode_send(
            server,
            create_capturer(64, 64, 1),
            create_encoder(64, 64, 30),
            host_token,
        )
        .await
    });

    let client_token = token.clone();
    let client_handle = tokio::spawn(async move {
        run_client_recv_decode(client, create_decoder(), frame_tx, client_token).await
    });

    let frames = collect_frames(&frame_rx, 1, Duration::from_secs(10));
    token.cancel();

    let _ = host.await;
    let _ = client_handle.await;

    assert!(!frames.is_empty(), "expected at least 1 frame");
    assert!(
        !frames[0].data.is_empty(),
        "decoded frame data should not be empty"
    );
    assert!(
        frames[0].data.iter().any(|&b| b != 0),
        "decoded frame data should contain non-zero bytes"
    );
}
