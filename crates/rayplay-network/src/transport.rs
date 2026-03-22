//! QUIC-based video transport using RFC 9221 unreliable datagrams (ADR-003).
//!
//! # Two-phase server lifecycle
//!
//! The server lifecycle is split so the caller can share the certificate with
//! the client **before** blocking on an incoming connection:
//!
//! ```ignore
//! // Host side
//! let (listener, cert_der) = QuicVideoTransport::listen("0.0.0.0:5000".parse()?)?;
//! // … distribute cert_der to the client (PIN pairing, QR code, etc.) …
//! let mut host = listener.accept().await?;
//!
//! // Client side
//! let mut client = QuicVideoTransport::connect(server_addr, cert_der).await?;
//! ```
//!
//! # Self-signed TLS
//!
//! A self-signed certificate is generated via `rcgen` for development and
//! testing. ADR-007 will replace this with a SPAKE2 pairing flow.

use std::net::SocketAddr;

use quinn::{Connection, Endpoint};
use rustls::pki_types::CertificateDer;

use rayplay_core::packet::EncodedPacket;

use crate::{
    control::{ControlChannel, ControlReceiver, ControlSender},
    fragmenter::VideoFragmenter,
    reassembler::{MAX_IN_FLIGHT_FRAMES, VideoReassembler},
    transport_tls::{make_client_config, make_server_config},
    wire::{TransportError, VideoFragment},
};

/// Datagram receive-buffer size in bytes (64 KiB).
///
/// Configuring a non-`None` value on a `TransportConfig` enables RFC 9221
/// unreliable datagram support for that endpoint.
pub const MAX_DATAGRAM_BUFFER: usize = 64 * 1024;

// ── QuicListener ─────────────────────────────────────────────────────────────

/// A bound QUIC server endpoint that can accept one incoming connection.
///
/// Created by [`QuicVideoTransport::listen`].  Call [`accept`] after
/// distributing the certificate to the client.
///
/// [`accept`]: QuicListener::accept
pub struct QuicListener {
    endpoint: Endpoint,
}

impl QuicListener {
    /// Returns the local address the listener is bound to.
    ///
    /// # Errors
    ///
    /// Returns [`TransportError::Io`] if the OS cannot return the address.
    pub fn local_addr(&self) -> Result<SocketAddr, TransportError> {
        self.endpoint.local_addr().map_err(TransportError::Io)
    }

    /// Waits for one incoming QUIC connection and returns a [`QuicVideoTransport`].
    ///
    /// # Errors
    ///
    /// - [`TransportError::EndpointClosed`] if the endpoint shuts down before
    ///   a connection arrives.
    /// - [`TransportError::Connection`] if the QUIC handshake fails.
    pub async fn accept(&self) -> Result<QuicVideoTransport, TransportError> {
        let incoming = self
            .endpoint
            .accept()
            .await
            .ok_or(TransportError::EndpointClosed)?;
        let connection = incoming.await?;
        Ok(QuicVideoTransport::from_connection(connection))
    }
}

// ── QuicVideoTransport ────────────────────────────────────────────────────────

/// QUIC-based video transport that sends and receives [`EncodedPacket`]s as
/// RFC 9221 unreliable datagrams.
pub struct QuicVideoTransport {
    pub(crate) connection: Connection,
    pub(crate) fragmenter: VideoFragmenter,
    reassembler: VideoReassembler,
}

impl QuicVideoTransport {
    /// Binds a server endpoint and returns a [`QuicListener`] together with
    /// the self-signed certificate DER.
    ///
    /// Share the certificate with the connecting client via an out-of-band
    /// channel **before** calling [`QuicListener::accept`].
    ///
    /// # Errors
    ///
    /// Returns [`TransportError`] if TLS generation or socket binding fails.
    pub fn listen(bind_addr: SocketAddr) -> Result<(QuicListener, Vec<u8>), TransportError> {
        let (cert_der, server_config) = make_server_config()?;
        let endpoint = Endpoint::server(server_config, bind_addr)?;
        Ok((QuicListener { endpoint }, cert_der.as_ref().to_vec()))
    }

    /// Creates a client-side transport connecting to `server_addr`.
    ///
    /// `server_cert` must be the DER certificate returned by the server's
    /// [`listen`] call.
    ///
    /// # Errors
    ///
    /// Returns [`TransportError`] if TLS setup or the QUIC handshake fails.
    ///
    /// # Panics
    ///
    /// Panics if `"0.0.0.0:0"` cannot be parsed as a `SocketAddr` (unreachable
    /// in practice).
    ///
    /// [`listen`]: QuicVideoTransport::listen
    pub async fn connect(
        server_addr: SocketAddr,
        server_cert: Vec<u8>,
    ) -> Result<Self, TransportError> {
        let client_config = make_client_config(CertificateDer::from(server_cert))?;
        let bind_addr: SocketAddr = "0.0.0.0:0".parse().expect("valid wildcard address");
        let mut endpoint = Endpoint::client(bind_addr)?;
        endpoint.set_default_client_config(client_config);
        let connection = endpoint.connect(server_addr, "localhost")?.await?;
        Ok(Self::from_connection(connection))
    }

    /// Wraps an existing [`Connection`] with default fragmenter and reassembler.
    pub(crate) fn from_connection(connection: Connection) -> Self {
        Self {
            connection,
            fragmenter: VideoFragmenter::with_default_payload(),
            reassembler: VideoReassembler::with_default_max(),
        }
    }

    /// Opens a bidirectional QUIC stream for session control (client side).
    ///
    /// # Errors
    ///
    /// Returns [`TransportError::Connection`] if the stream cannot be opened.
    pub async fn open_control(&self) -> Result<ControlChannel, TransportError> {
        let (send, recv) = self.connection.open_bi().await?;
        Ok(ControlChannel {
            sender: ControlSender::new(send),
            receiver: ControlReceiver::new(recv),
        })
    }

    /// Accepts a bidirectional QUIC stream for session control (host side).
    ///
    /// # Errors
    ///
    /// Returns [`TransportError::Connection`] if the stream cannot be accepted.
    pub async fn accept_control(&self) -> Result<ControlChannel, TransportError> {
        let (send, recv) = self.connection.accept_bi().await?;
        Ok(ControlChannel {
            sender: ControlSender::new(send),
            receiver: ControlReceiver::new(recv),
        })
    }

    /// Sends an [`EncodedPacket`] as one or more QUIC unreliable datagrams.
    ///
    /// Returns the number of fragments sent.  Returns `Ok(0)` for empty
    /// packets without sending any datagrams.
    ///
    /// # Errors
    ///
    /// Returns [`TransportError::SendDatagram`] if the QUIC layer rejects a
    /// datagram.
    // `async` is intentional: keeps the signature symmetric with `recv_video`
    // and compatible with the `NetworkTransport` trait's async contract.
    #[allow(clippy::unused_async)]
    pub async fn send_video(&mut self, packet: &EncodedPacket) -> Result<usize, TransportError> {
        let frags = self.fragmenter.fragment(packet);
        let count = frags.len();
        for frag in frags {
            self.connection.send_datagram(frag.encode())?;
        }
        Ok(count)
    }

    /// Waits for the next fully-reassembled [`EncodedPacket`].
    ///
    /// Fragments for incomplete frames dropped by the network are silently
    /// discarded; this method loops until a complete frame is available.
    ///
    /// # Errors
    ///
    /// Returns [`TransportError::Connection`] if the underlying QUIC
    /// connection is lost.
    pub async fn recv_video(&mut self) -> Result<EncodedPacket, TransportError> {
        loop {
            let datagram = self.connection.read_datagram().await?;
            let frag = VideoFragment::decode(&datagram)?;

            // Evict frames outside the sliding window to bound memory.
            #[allow(clippy::cast_possible_truncation)]
            let window = MAX_IN_FLIGHT_FRAMES as u32;
            if frag.frame_id >= window {
                self.reassembler.evict_before(frag.frame_id - window);
            }

            if let Some(packet) = self.reassembler.ingest(frag) {
                return Ok(packet);
            }
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
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
}
