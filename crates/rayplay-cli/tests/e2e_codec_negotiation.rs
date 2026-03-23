//! End-to-end integration tests for codec negotiation handshake + streaming.
//!
//! Tests the full flow: QUIC connect → control channel → handshake → stream.
//! Uses the software fallback pipeline (OpenH264) so it runs on all platforms.

#[path = "e2e_helpers.rs"]
mod e2e_helpers;

use std::time::Duration;

use e2e_helpers::{
    SyntheticCapturer, collect_frames, run_client_recv_decode, run_host_encode_send,
};
use rayplay_core::session::StreamParams;
use rayplay_network::{client_handshake, host_handshake};
use rayplay_video::{
    Codec, DecodedFrame, EncoderConfig, OpenH264Decoder, OpenH264Encoder, capture::ScreenCapturer,
    decoder::VideoDecoder, encoder::VideoEncoder,
};
use tokio_util::sync::CancellationToken;

fn create_h264_encoder(width: u32, height: u32, fps: u32) -> Box<dyn VideoEncoder> {
    let config = EncoderConfig::with_codec(width, height, fps, Codec::H264);
    Box::new(OpenH264Encoder::new(config).expect("OpenH264Encoder init"))
}

fn create_h264_decoder() -> Box<dyn VideoDecoder> {
    Box::new(OpenH264Decoder::new(Codec::H264).expect("OpenH264Decoder init"))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_handshake_agrees_on_h264() {
    let bind_addr = "127.0.0.1:0".parse().unwrap();
    let (listener, cert_der) = rayplay_network::QuicVideoTransport::listen(bind_addr).unwrap();
    let server_addr = listener.local_addr().unwrap();

    // Use a oneshot to coordinate — the server sends its agreed params back,
    // keeping the transport alive until the client has also finished.
    let (server_done_tx, server_done_rx) = tokio::sync::oneshot::channel::<StreamParams>();

    let server_task = tokio::spawn(async move {
        let transport = listener.accept().await.expect("accept");
        let mut control = transport.accept_control().await.expect("accept_control");

        let agreed = host_handshake(&mut control, |_proposed| StreamParams {
            width: 64,
            height: 64,
            fps: 30,
            codec: "h264".to_string(),
        })
        .await
        .expect("host_handshake");

        let result = agreed.clone();
        let _ = server_done_tx.send(result);

        // Keep transport alive until test completes — drop of `transport`
        // closes the QUIC connection which races with the client read.
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        drop(transport);
        agreed
    });

    let client_task = tokio::spawn(async move {
        let transport = rayplay_network::QuicVideoTransport::connect(server_addr, cert_der)
            .await
            .expect("connect");
        let mut control = transport.open_control().await.expect("open_control");

        let desired = StreamParams {
            width: 1920,
            height: 1080,
            fps: 60,
            codec: "hevc".to_string(),
        };

        client_handshake(&mut control, desired)
            .await
            .expect("client_handshake")
    });

    let client_params = client_task.await.unwrap();
    let host_params = server_done_rx.await.unwrap();

    // Cancel the server sleep
    server_task.abort();

    assert_eq!(host_params.codec, "h264");
    assert_eq!(client_params.codec, "h264");
    assert_eq!(host_params.width, 64);
    assert_eq!(client_params.width, 64);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_handshake_then_stream() {
    let bind_addr = "127.0.0.1:0".parse().unwrap();
    let (listener, cert_der) = rayplay_network::QuicVideoTransport::listen(bind_addr).unwrap();
    let server_addr = listener.local_addr().unwrap();

    let token = CancellationToken::new();
    let (frame_tx, frame_rx) = crossbeam_channel::bounded::<DecodedFrame>(16);

    // Host: accept → handshake → stream
    let host_token = token.clone();
    let host_handle = tokio::spawn(async move {
        let transport = listener.accept().await.expect("accept");
        let mut control = transport.accept_control().await.expect("accept_control");

        let _agreed = host_handshake(&mut control, |_proposed| StreamParams {
            width: 64,
            height: 64,
            fps: 30,
            codec: "h264".to_string(),
        })
        .await
        .expect("host_handshake");

        let capturer: Box<dyn ScreenCapturer> = Box::new(SyntheticCapturer::new(64, 64, 5));
        let encoder = create_h264_encoder(64, 64, 30);

        run_host_encode_send(transport, capturer, encoder, host_token).await
    });

    // Client: connect → handshake → receive+decode
    let client_token = token.clone();
    let client_handle = tokio::spawn(async move {
        let transport = rayplay_network::QuicVideoTransport::connect(server_addr, cert_der)
            .await
            .expect("connect");
        let mut control = transport.open_control().await.expect("open_control");

        let agreed = client_handshake(
            &mut control,
            StreamParams {
                width: 1920,
                height: 1080,
                fps: 60,
                codec: "hevc".to_string(),
            },
        )
        .await
        .expect("client_handshake");

        assert_eq!(agreed.codec, "h264", "negotiated codec should be h264");

        let decoder = create_h264_decoder();
        run_client_recv_decode(transport, decoder, frame_tx, client_token).await
    });

    let frames = collect_frames(&frame_rx, 5, Duration::from_secs(15));
    token.cancel();

    let _ = host_handle.await;
    let _ = client_handle.await;

    assert!(
        frames.len() >= 3,
        "expected at least 3 frames after handshake, got {}",
        frames.len()
    );
    for frame in &frames {
        assert_eq!(frame.width, 64);
        assert_eq!(frame.height, 64);
    }
}
