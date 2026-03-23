use super::*;
use crate::transport::QuicVideoTransport;
use rayplay_core::session::StreamParams;
use std::net::SocketAddr;

/// Sets up a loopback QUIC connection and opens control channels on both
/// sides. The client sends a trigger `Keepalive` (then drained) because
/// QUIC only notifies the peer of a new stream when a STREAM frame arrives.
async fn control_pair() -> (ControlChannel, ControlChannel) {
    let bind: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let (listener, cert_der) = QuicVideoTransport::listen(bind).unwrap();
    let server_addr = listener.local_addr().unwrap();

    let server_task = tokio::spawn(async move {
        let transport = listener.accept().await.expect("accept");
        transport.accept_control().await.expect("accept_control")
    });

    let client_transport = QuicVideoTransport::connect(server_addr, cert_der)
        .await
        .expect("connect");

    let mut client_ctrl = client_transport.open_control().await.expect("open_control");
    // Write a keepalive so the server's accept_bi sees the stream.
    client_ctrl
        .sender
        .send(&ControlMessage::Keepalive)
        .await
        .expect("trigger");

    let mut server_ctrl = server_task.await.expect("server task");
    // Drain the trigger message.
    let _ = server_ctrl.receiver.recv().await.expect("drain trigger");

    (client_ctrl, server_ctrl)
}

fn sample_params() -> StreamParams {
    StreamParams {
        width: 1920,
        height: 1080,
        fps: 60,
        codec: "hevc".to_string(),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_roundtrip_handshake_request() {
    let (mut client, mut server) = control_pair().await;

    let msg = ControlMessage::HandshakeRequest(sample_params());
    client.sender.send(&msg).await.unwrap();
    let received = server.receiver.recv().await.unwrap();
    assert_eq!(received, Some(msg));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_roundtrip_handshake_response() {
    let (mut client, mut server) = control_pair().await;

    let msg = ControlMessage::HandshakeResponse(sample_params());
    server.sender.send(&msg).await.unwrap();
    let received = client.receiver.recv().await.unwrap();
    assert_eq!(received, Some(msg));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_roundtrip_keepalive() {
    let (mut client, mut server) = control_pair().await;

    client
        .sender
        .send(&ControlMessage::Keepalive)
        .await
        .unwrap();
    let received = server.receiver.recv().await.unwrap();
    assert_eq!(received, Some(ControlMessage::Keepalive));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_roundtrip_keepalive_ack() {
    let (mut client, mut server) = control_pair().await;

    server
        .sender
        .send(&ControlMessage::KeepaliveAck)
        .await
        .unwrap();
    let received = client.receiver.recv().await.unwrap();
    assert_eq!(received, Some(ControlMessage::KeepaliveAck));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_roundtrip_disconnect() {
    let (mut client, mut server) = control_pair().await;

    client
        .sender
        .send(&ControlMessage::Disconnect)
        .await
        .unwrap();
    let received = server.receiver.recv().await.unwrap();
    assert_eq!(received, Some(ControlMessage::Disconnect));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_recv_returns_none_on_clean_close() {
    let (mut client, mut server) = control_pair().await;

    client.sender.stream.finish().unwrap();
    let result = server.receiver.recv().await.unwrap();
    assert_eq!(result, None);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_multiple_messages_in_sequence() {
    let (mut client, mut server) = control_pair().await;

    let messages = vec![
        ControlMessage::HandshakeRequest(sample_params()),
        ControlMessage::Keepalive,
        ControlMessage::Disconnect,
    ];

    for msg in &messages {
        client.sender.send(msg).await.unwrap();
    }

    for expected in &messages {
        let received = server.receiver.recv().await.unwrap();
        assert_eq!(received.as_ref(), Some(expected));
    }
}

#[tokio::test]
async fn test_max_control_message_size_constant() {
    assert_eq!(MAX_CONTROL_MESSAGE_SIZE, 65_536);
}

/// QUIC only notifies the peer of a new bidi stream when a STREAM frame
/// is sent (i.e. when the opener writes data). This test verifies that
/// open_bi + write triggers the server's accept_bi.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_raw_quic_bidi_stream_works() {
    let bind: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let (listener, cert_der) = QuicVideoTransport::listen(bind).unwrap();
    let server_addr = listener.local_addr().unwrap();

    let server_task = tokio::spawn(async move {
        let transport = listener.accept().await.expect("accept");
        transport.connection.accept_bi().await.expect("accept_bi")
    });

    let client = QuicVideoTransport::connect(server_addr, cert_der)
        .await
        .expect("connect");

    let (client_bi, server_bi) = tokio::join!(
        async {
            let (mut send, recv) = client.connection.open_bi().await.unwrap();
            send.write_all(b"hello").await.unwrap();
            (send, recv)
        },
        async { server_task.await.expect("server task") },
    );
    let (_s, _r) = client_bi;
    let (_s2, _r2) = server_bi;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_recv_oversized_length_returns_message_too_large() {
    let (mut client, mut server) = control_pair().await;

    // Write a u32 LE length that exceeds MAX_CONTROL_MESSAGE_SIZE directly
    // on the underlying stream.
    let huge_len: u32 = MAX_CONTROL_MESSAGE_SIZE + 1;
    client
        .sender
        .stream
        .write_all(&huge_len.to_le_bytes())
        .await
        .unwrap();

    let err = server.receiver.recv().await.unwrap_err();
    assert!(
        matches!(err, crate::wire::TransportError::MessageTooLarge(_)),
        "expected MessageTooLarge, got {err:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_recv_invalid_json_returns_message_parse() {
    let (mut client, mut server) = control_pair().await;

    // Write a valid length prefix followed by garbage JSON.
    let garbage = b"not valid json";
    let len = u32::try_from(garbage.len()).unwrap();
    client
        .sender
        .stream
        .write_all(&len.to_le_bytes())
        .await
        .unwrap();
    client.sender.stream.write_all(garbage).await.unwrap();

    let err = server.receiver.recv().await.unwrap_err();
    assert!(
        matches!(err, crate::wire::TransportError::MessageParse(_)),
        "expected MessageParse, got {err:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_send_after_connection_closed_returns_stream_write() {
    let (mut client, server) = control_pair().await;

    // Close the server's connection to break the client's send stream.
    drop(server);
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let result = client.sender.send(&ControlMessage::Keepalive).await;
    assert!(result.is_err());
}
