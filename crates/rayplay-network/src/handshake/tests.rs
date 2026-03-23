use super::*;
use crate::transport::QuicVideoTransport;
use std::net::SocketAddr;

/// Sets up a loopback QUIC connection and opens control channels.
/// The client sends a trigger keepalive so the server's `accept_bi` fires
/// (QUIC only notifies the peer when a STREAM frame is sent).
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
    client_ctrl
        .sender
        .send(&ControlMessage::Keepalive)
        .await
        .expect("trigger");

    let mut server_ctrl = server_task.await.expect("server task");
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
async fn test_handshake_happy_path_identity() {
    let (mut client, mut server) = control_pair().await;

    let (client_result, server_result) = tokio::join!(
        client_handshake(&mut client, sample_params()),
        host_handshake(&mut server, |p| p),
    );

    let agreed_client = client_result.unwrap();
    let agreed_server = server_result.unwrap();
    assert_eq!(agreed_client, agreed_server);
    assert_eq!(agreed_client, sample_params());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_handshake_host_adjusts_params() {
    let (mut client, mut server) = control_pair().await;

    let (client_result, server_result) = tokio::join!(
        client_handshake(&mut client, sample_params()),
        host_handshake(&mut server, |mut p| {
            p.width = 1280;
            p.height = 720;
            p
        }),
    );

    let agreed_client = client_result.unwrap();
    let agreed_server = server_result.unwrap();
    assert_eq!(agreed_client, agreed_server);
    assert_eq!(agreed_client.width, 1280);
    assert_eq!(agreed_client.height, 720);
    assert_eq!(agreed_client.fps, 60);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_client_handshake_unexpected_message() {
    let (mut client, mut server) = control_pair().await;

    let client_task =
        tokio::spawn(async move { client_handshake(&mut client, sample_params()).await });

    // Server sends wrong message type
    let _ = server.receiver.recv().await; // drain HandshakeRequest
    server
        .sender
        .send(&ControlMessage::Keepalive)
        .await
        .unwrap();

    let err = client_task.await.unwrap().unwrap_err();
    assert!(matches!(err, SessionError::HandshakeFailed(_)));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_host_handshake_unexpected_message() {
    let (mut client, mut server) = control_pair().await;

    // Client sends Keepalive instead of HandshakeRequest
    client
        .sender
        .send(&ControlMessage::Keepalive)
        .await
        .unwrap();

    let err = host_handshake(&mut server, |p| p).await.unwrap_err();
    assert!(matches!(err, SessionError::HandshakeFailed(_)));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_client_handshake_stream_closed_cleanly() {
    let (mut client, mut server) = control_pair().await;

    let client_task =
        tokio::spawn(async move { client_handshake(&mut client, sample_params()).await });

    // Drain the HandshakeRequest, then cleanly finish the send stream.
    let _ = server.receiver.recv().await;
    server.sender.stream.finish().unwrap();

    let err = client_task.await.unwrap().unwrap_err();
    assert!(matches!(err, SessionError::HandshakeFailed(_)));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_host_handshake_stream_closed_cleanly() {
    let (mut client, mut server) = control_pair().await;

    // Cleanly finish the client's send stream so host recv sees Ok(None).
    client.sender.stream.finish().unwrap();

    let result = host_handshake(&mut server, |p| p).await;
    assert!(matches!(result, Err(SessionError::HandshakeFailed(_))));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_client_handshake_transport_error_on_send() {
    let (mut client, server) = control_pair().await;

    // Close server connection so client send fails.
    drop(server);
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let err = client_handshake(&mut client, sample_params())
        .await
        .unwrap_err();
    assert!(matches!(err, SessionError::Transport(_)));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_client_handshake_transport_error_on_recv() {
    let (mut client, mut server) = control_pair().await;

    let client_task =
        tokio::spawn(async move { client_handshake(&mut client, sample_params()).await });

    // Drain the request, then reset the stream to cause a read error.
    let _ = server.receiver.recv().await;
    server.sender.stream.reset(0u32.into()).ok();
    drop(server);

    let err = client_task.await.unwrap().unwrap_err();
    assert!(matches!(
        err,
        SessionError::HandshakeFailed(_) | SessionError::Transport(_)
    ));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_host_handshake_transport_error_on_recv() {
    let (client, mut server) = control_pair().await;

    // Drop client to close the stream.
    drop(client);
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let result = host_handshake(&mut server, |p| p).await;
    assert!(result.is_err());
}
