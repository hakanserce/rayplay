//! Tests for `run_receive_loop` (UC-007).
//!
//! Kept in a dedicated file because each test requires a loopback QUIC
//! connection, making the suite too large to embed inline in `receive.rs`.

use rayplay_network::QuicVideoTransport;
use rayplay_video::{DecodedFrame, FrameNotifier, PixelFormat, packet::EncodedPacket};
use tokio_util::sync::CancellationToken;

use super::{
    receive::run_receive_loop,
    test_helper::{NullDecoder, SkipBadDecoder, loopback_listener},
};

fn notifier() -> FrameNotifier {
    FrameNotifier::no_op()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_run_receive_loop_exits_on_shutdown() {
    let (listener, cert_bytes, addr) = loopback_listener();
    let _server = tokio::spawn(async move { listener.accept().await });
    let transport = QuicVideoTransport::connect(addr, cert_bytes).await.unwrap();

    let (frame_tx, _rx) = crossbeam_channel::bounded::<DecodedFrame>(4);
    let token = CancellationToken::new();
    token.cancel();

    assert!(
        run_receive_loop(
            transport,
            Box::new(NullDecoder {
                emit: false,
                fail: false
            }),
            frame_tx,
            notifier(),
            token
        )
        .await
        .is_ok()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_run_receive_loop_transport_error_propagates() {
    let (listener, cert_bytes, addr) = loopback_listener();
    let server_task = tokio::spawn(async move { listener.accept().await.unwrap() });
    let transport = QuicVideoTransport::connect(addr, cert_bytes).await.unwrap();
    let server = server_task.await.unwrap();

    drop(server); // dropping closes the QUIC connection
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let (frame_tx, _rx) = crossbeam_channel::bounded::<DecodedFrame>(4);
    let token = CancellationToken::new();

    assert!(
        run_receive_loop(
            transport,
            Box::new(NullDecoder {
                emit: false,
                fail: false
            }),
            frame_tx,
            notifier(),
            token
        )
        .await
        .is_err()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_run_receive_loop_forwards_decoded_frame() {
    let (listener, cert_bytes, addr) = loopback_listener();
    let server_task = tokio::spawn(async move { listener.accept().await.unwrap() });
    let transport = QuicVideoTransport::connect(addr, cert_bytes).await.unwrap();
    let mut server = server_task.await.unwrap();

    server
        .send_video(&EncodedPacket::new(vec![1u8], true, 42, 16_667))
        .await
        .unwrap();

    let (frame_tx, frame_rx) = crossbeam_channel::bounded::<DecodedFrame>(4);
    let token = CancellationToken::new();

    let task = tokio::spawn(run_receive_loop(
        transport,
        Box::new(NullDecoder {
            emit: true,
            fail: false,
        }),
        frame_tx,
        notifier(),
        token.clone(),
    ));

    let frame = frame_rx
        .recv_timeout(std::time::Duration::from_secs(2))
        .unwrap();
    assert_eq!(frame.width, 1);
    assert_eq!(frame.height, 1);
    token.cancel();
    task.await.unwrap().unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_run_receive_loop_buffering_none_continues() {
    let (listener, cert_bytes, addr) = loopback_listener();
    let server_task = tokio::spawn(async move { listener.accept().await.unwrap() });
    let transport = QuicVideoTransport::connect(addr, cert_bytes).await.unwrap();
    let mut server = server_task.await.unwrap();

    server
        .send_video(&EncodedPacket::new(vec![1u8], false, 0, 16_667))
        .await
        .unwrap();

    let (frame_tx, _rx) = crossbeam_channel::bounded::<DecodedFrame>(4);
    let token = CancellationToken::new();

    let task = tokio::spawn(run_receive_loop(
        transport,
        Box::new(NullDecoder {
            emit: false,
            fail: false,
        }),
        frame_tx,
        notifier(),
        token.clone(),
    ));
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    token.cancel();
    assert!(task.await.unwrap().is_ok());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_run_receive_loop_decode_error_is_skipped() {
    let (listener, cert_bytes, addr) = loopback_listener();
    let server_task = tokio::spawn(async move { listener.accept().await.unwrap() });
    let transport = QuicVideoTransport::connect(addr, cert_bytes).await.unwrap();
    let mut server = server_task.await.unwrap();

    server
        .send_video(&EncodedPacket::new(vec![0xDE, 0xAD], false, 0, 0))
        .await
        .unwrap();
    server
        .send_video(&EncodedPacket::new(vec![1u8], true, 0, 0))
        .await
        .unwrap();

    let (frame_tx, frame_rx) = crossbeam_channel::bounded::<DecodedFrame>(4);
    let token = CancellationToken::new();

    let task = tokio::spawn(run_receive_loop(
        transport,
        Box::new(SkipBadDecoder),
        frame_tx,
        notifier(),
        token.clone(),
    ));
    frame_rx
        .recv_timeout(std::time::Duration::from_secs(2))
        .unwrap();
    token.cancel();
    assert!(task.await.unwrap().is_ok());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_run_receive_loop_exits_when_channel_disconnected() {
    let (listener, cert_bytes, addr) = loopback_listener();
    let server_task = tokio::spawn(async move { listener.accept().await.unwrap() });
    let transport = QuicVideoTransport::connect(addr, cert_bytes).await.unwrap();
    let mut server = server_task.await.unwrap();

    server
        .send_video(&EncodedPacket::new(vec![1u8], true, 0, 0))
        .await
        .unwrap();

    let (frame_tx, frame_rx) = crossbeam_channel::bounded::<DecodedFrame>(1);
    drop(frame_rx);

    let token = CancellationToken::new();
    assert!(
        run_receive_loop(
            transport,
            Box::new(NullDecoder {
                emit: true,
                fail: false
            }),
            frame_tx,
            notifier(),
            token
        )
        .await
        .is_ok()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_run_receive_loop_drops_frame_when_channel_full() {
    let (listener, cert_bytes, addr) = loopback_listener();
    let server_task = tokio::spawn(async move { listener.accept().await.unwrap() });
    let transport = QuicVideoTransport::connect(addr, cert_bytes).await.unwrap();
    let mut server = server_task.await.unwrap();

    let (frame_tx, frame_rx) = crossbeam_channel::bounded::<DecodedFrame>(1);
    frame_tx
        .send(DecodedFrame::new_cpu(
            vec![0; 4],
            1,
            1,
            4,
            PixelFormat::Bgra8,
            0,
        ))
        .unwrap();

    server
        .send_video(&EncodedPacket::new(vec![1u8], true, 99, 0))
        .await
        .unwrap();

    let token = CancellationToken::new();
    let task = tokio::spawn(run_receive_loop(
        transport,
        Box::new(NullDecoder {
            emit: true,
            fail: false,
        }),
        frame_tx,
        notifier(),
        token.clone(),
    ));
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    token.cancel();
    task.await.unwrap().unwrap();

    assert_eq!(frame_rx.len(), 1);
}
