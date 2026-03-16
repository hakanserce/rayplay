//! Tests for `run_receive_loop` (UC-007).
//!
//! Kept in a dedicated file because each test requires a loopback QUIC
//! connection, making the suite too large to embed inline in `receive.rs`.

use rayplay_network::QuicVideoTransport;
use rayplay_video::{DecodedFrame, PixelFormat, packet::EncodedPacket};

use super::{
    receive::run_receive_loop,
    test_helper::{NullDecoder, loopback_listener},
};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_run_receive_loop_exits_on_shutdown() {
    let (listener, cert_bytes, addr) = loopback_listener();
    let _server = tokio::spawn(async move { listener.accept().await });
    let transport = QuicVideoTransport::connect(addr, cert_bytes).await.unwrap();

    let (frame_tx, _rx) = crossbeam_channel::bounded::<DecodedFrame>(4);
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    shutdown_tx.send(()).unwrap();

    assert!(
        run_receive_loop(
            transport,
            Box::new(NullDecoder {
                emit: false,
                fail: false
            }),
            frame_tx,
            shutdown_rx
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
    let (_stx, shutdown_rx) = tokio::sync::oneshot::channel();

    assert!(
        run_receive_loop(
            transport,
            Box::new(NullDecoder {
                emit: false,
                fail: false
            }),
            frame_tx,
            shutdown_rx
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
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

    let task = tokio::spawn(run_receive_loop(
        transport,
        Box::new(NullDecoder {
            emit: true,
            fail: false,
        }),
        frame_tx,
        shutdown_rx,
    ));

    // timestamp_us is not in the wire protocol; verify arrival via NullDecoder's
    // fixed 1×1 dimensions.
    let frame = frame_rx
        .recv_timeout(std::time::Duration::from_secs(2))
        .unwrap();
    assert_eq!(frame.width, 1);
    assert_eq!(frame.height, 1);
    shutdown_tx.send(()).unwrap();
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
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

    let task = tokio::spawn(run_receive_loop(
        transport,
        Box::new(NullDecoder {
            emit: false,
            fail: false,
        }),
        frame_tx,
        shutdown_rx,
    ));
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    shutdown_tx.send(()).unwrap();
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

    let (frame_tx, _rx) = crossbeam_channel::bounded::<DecodedFrame>(4);
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

    let task = tokio::spawn(run_receive_loop(
        transport,
        Box::new(NullDecoder {
            emit: false,
            fail: true,
        }),
        frame_tx,
        shutdown_rx,
    ));
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    shutdown_tx.send(()).unwrap();
    // Decode error must be skipped — loop returns Ok, not Err.
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

    let (_stx, shutdown_rx) = tokio::sync::oneshot::channel();
    assert!(
        run_receive_loop(
            transport,
            Box::new(NullDecoder {
                emit: true,
                fail: false
            }),
            frame_tx,
            shutdown_rx
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
    // Pre-fill the channel so the decoded frame hits the Full branch.
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

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(run_receive_loop(
        transport,
        Box::new(NullDecoder {
            emit: true,
            fail: false,
        }),
        frame_tx,
        shutdown_rx,
    ));
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    shutdown_tx.send(()).unwrap();
    task.await.unwrap().unwrap();

    // Channel capacity is 1 and it was pre-filled, so the decoded frame was
    // silently dropped via try_send Full — exactly one frame remains.
    assert_eq!(frame_rx.len(), 1);
}
