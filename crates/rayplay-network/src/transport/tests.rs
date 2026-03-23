use super::*;

// ── QuicListener::local_addr ──────────────────────────────────────────────
// `quinn::Endpoint::server` requires a tokio runtime even though `listen`
// is not async, so these tests use `#[tokio::test]`.

#[tokio::test]
async fn test_listen_returns_nonzero_port() {
    let bind: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let (listener, _cert) = QuicVideoTransport::listen(bind).unwrap();
    let addr = listener.local_addr().unwrap();
    assert_ne!(addr.port(), 0);
}

#[tokio::test]
async fn test_listen_twice_binds_different_ports() {
    let bind: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let (l1, _) = QuicVideoTransport::listen(bind).unwrap();
    let (l2, _) = QuicVideoTransport::listen(bind).unwrap();
    assert_ne!(
        l1.local_addr().unwrap().port(),
        l2.local_addr().unwrap().port()
    );
}

// ── Loopback integration tests ────────────────────────────────────────────

#[tokio::test]
async fn test_roundtrip_single_fragment() {
    let bind: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let (listener, cert_der) = QuicVideoTransport::listen(bind).unwrap();
    let server_addr = listener.local_addr().unwrap();

    let server_task = tokio::spawn(async move { listener.accept().await.expect("accept") });
    let mut client = QuicVideoTransport::connect(server_addr, cert_der)
        .await
        .expect("connect");
    let mut server = server_task.await.expect("server task");

    let original = EncodedPacket::new(vec![1u8, 2, 3, 4, 5], true, 42, 16_667);
    let sent = client.send_video(&original).await.expect("send_video");
    assert_eq!(sent, 1);

    let received = server.recv_video().await.expect("recv_video");
    assert_eq!(received.data, original.data);
    assert_eq!(received.is_keyframe, original.is_keyframe);
}

#[tokio::test]
async fn test_send_empty_packet_returns_zero_fragments() {
    let bind: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let (listener, cert_der) = QuicVideoTransport::listen(bind).unwrap();
    let server_addr = listener.local_addr().unwrap();
    let _server_task = tokio::spawn(async move { listener.accept().await });

    let mut client = QuicVideoTransport::connect(server_addr, cert_der)
        .await
        .expect("connect");
    let count = client
        .send_video(&EncodedPacket::new(vec![], false, 0, 0))
        .await
        .expect("send_video");
    assert_eq!(count, 0);
}

#[tokio::test]
async fn test_roundtrip_multi_fragment() {
    let bind: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let (listener, cert_der) = QuicVideoTransport::listen(bind).unwrap();
    let server_addr = listener.local_addr().unwrap();

    let server_task = tokio::spawn(async move { listener.accept().await.expect("accept") });
    let mut client = QuicVideoTransport::connect(server_addr, cert_der)
        .await
        .expect("connect");
    let mut server = server_task.await.expect("server task");

    // Use a tiny fragmenter to force 3 fragments for 12 bytes of data.
    client.fragmenter = VideoFragmenter::new(4);
    let data: Vec<u8> = (0u8..12).collect();
    let sent = client
        .send_video(&EncodedPacket::new(data.clone(), false, 0, 0))
        .await
        .expect("send");
    assert_eq!(sent, 3);

    let received = server.recv_video().await.expect("recv");
    assert_eq!(received.data, data);
}

#[tokio::test]
async fn test_keyframe_flag_preserved_through_transport() {
    let bind: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let (listener, cert_der) = QuicVideoTransport::listen(bind).unwrap();
    let server_addr = listener.local_addr().unwrap();

    let server_task = tokio::spawn(async move { listener.accept().await.expect("accept") });
    let mut client = QuicVideoTransport::connect(server_addr, cert_der)
        .await
        .expect("connect");
    let mut server = server_task.await.expect("server task");

    let pkt = EncodedPacket::new(vec![0xDE, 0xAD, 0xBE, 0xEF], true, 0, 0);
    client.send_video(&pkt).await.expect("send");
    let received = server.recv_video().await.expect("recv");
    assert!(received.is_keyframe);
    assert_eq!(received.data, pkt.data);
}

#[tokio::test]
async fn test_sliding_window_eviction_does_not_stall_recv() {
    let bind: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let (listener, cert_der) = QuicVideoTransport::listen(bind).unwrap();
    let server_addr = listener.local_addr().unwrap();

    let server_task = tokio::spawn(async move { listener.accept().await.expect("accept") });
    let mut client = QuicVideoTransport::connect(server_addr, cert_der)
        .await
        .expect("connect");
    let mut server = server_task.await.expect("server task");

    for i in 0u8..10 {
        client
            .send_video(&EncodedPacket::new(vec![i], false, 0, 0))
            .await
            .expect("send");
    }
    for _ in 0u8..10 {
        server.recv_video().await.expect("recv");
    }
}

// ── Error paths ───────────────────────────────────────────────────────────

#[tokio::test]
async fn test_accept_endpoint_closed_when_no_incoming() {
    let bind: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let (listener, _cert) = QuicVideoTransport::listen(bind).unwrap();
    // Close the endpoint so accept() sees no incoming connections.
    listener.endpoint.close(0u32.into(), b"done");
    let result = listener.accept().await;
    assert!(matches!(result, Err(TransportError::EndpointClosed)));
}

#[tokio::test]
async fn test_connect_fails_with_garbage_cert() {
    let bind: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let (listener, _real_cert) = QuicVideoTransport::listen(bind).unwrap();
    let server_addr = listener.local_addr().unwrap();
    let _server = tokio::spawn(async move { listener.accept().await });
    let result = QuicVideoTransport::connect(server_addr, vec![0u8; 16]).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_send_video_returns_error_when_connection_closed() {
    let bind: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let (listener, cert_der) = QuicVideoTransport::listen(bind).unwrap();
    let server_addr = listener.local_addr().unwrap();

    let server_task = tokio::spawn(async move { listener.accept().await.expect("accept") });
    let mut client = QuicVideoTransport::connect(server_addr, cert_der)
        .await
        .expect("connect");
    let server = server_task.await.expect("server task");

    // Drop the server side to close the connection, then try to send.
    server.connection.close(0u32.into(), b"done");
    // Give QUIC time to propagate the close.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let result = client
        .send_video(&EncodedPacket::new(vec![1, 2, 3], false, 0, 0))
        .await;
    assert!(result.is_err());
}

#[test]
fn test_transport_error_io_display() {
    let io_err = std::io::Error::new(std::io::ErrorKind::ConnectionRefused, "refused");
    let err = TransportError::from(io_err);
    assert!(err.to_string().contains("refused"));
}

#[test]
fn test_transport_error_connection_display() {
    let conn_err = quinn::ConnectionError::LocallyClosed;
    let err = TransportError::from(conn_err);
    assert!(err.to_string().contains("connection"));
}

#[test]
fn test_transport_error_send_datagram_too_large_display() {
    let err = TransportError::from(quinn::SendDatagramError::TooLarge);
    assert!(err.to_string().contains("send datagram"));
}

// ── TLS config tests (moved from transport_tls.rs for coverage) ─────────

#[test]
fn test_make_server_config_succeeds() {
    use crate::transport_tls::make_server_config;
    assert!(make_server_config().is_ok());
}

#[test]
fn test_make_server_config_cert_starts_with_sequence_tag() {
    use crate::transport_tls::make_server_config;
    let (cert_der, _) = make_server_config().unwrap();
    assert!(!cert_der.is_empty());
    assert_eq!(cert_der[0], 0x30);
}

#[test]
fn test_make_server_config_produces_unique_certs() {
    use crate::transport_tls::make_server_config;
    let (c1, _) = make_server_config().unwrap();
    let (c2, _) = make_server_config().unwrap();
    assert_ne!(c1.as_ref(), c2.as_ref());
}

#[test]
fn test_make_client_config_succeeds_with_valid_cert() {
    use crate::transport_tls::{make_client_config, make_server_config};
    let (cert_der, _) = make_server_config().unwrap();
    assert!(make_client_config(cert_der).is_ok());
}

#[test]
fn test_make_client_config_fails_with_garbage_cert() {
    use crate::transport_tls::make_client_config;
    use rustls::pki_types::CertificateDer;
    let bad = CertificateDer::from(vec![0u8; 16]);
    assert!(make_client_config(bad).is_err());
}

#[tokio::test]
async fn test_from_connection_uses_default_fragment_payload() {
    let bind: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let (listener, cert_der) = QuicVideoTransport::listen(bind).unwrap();
    let server_addr = listener.local_addr().unwrap();
    let _server = tokio::spawn(async move { listener.accept().await });

    let client = QuicVideoTransport::connect(server_addr, cert_der)
        .await
        .expect("connect");
    assert_eq!(
        client.fragmenter.max_payload(),
        crate::wire::MAX_FRAGMENT_PAYLOAD,
    );
}

// ── connect_insecure ─────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_connect_insecure_establishes_connection() {
    let bind: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let (listener, _cert) = QuicVideoTransport::listen(bind).unwrap();
    let server_addr = listener.local_addr().unwrap();
    let _server = tokio::spawn(async move { listener.accept().await });
    let client = QuicVideoTransport::connect_insecure(server_addr)
        .await
        .expect("insecure connect should succeed");
    assert_eq!(
        client.fragmenter.max_payload(),
        crate::wire::MAX_FRAGMENT_PAYLOAD,
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_connect_insecure_fails_when_no_server() {
    let result = QuicVideoTransport::connect_insecure("127.0.0.1:1".parse().unwrap()).await;
    assert!(result.is_err());
}
