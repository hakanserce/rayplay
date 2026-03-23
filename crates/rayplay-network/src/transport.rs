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
    transport_tls::{make_client_config, make_client_config_insecure, make_server_config},
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
    pub(crate) endpoint: Endpoint,
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

    /// Creates a client-side transport connecting to `server_addr` without
    /// verifying the server's TLS certificate.
    ///
    /// Used during the SPAKE2 pairing flow where the PIN-based key agreement
    /// provides authentication independently of TLS certificate validation.
    ///
    /// # Security
    ///
    /// This function **skips TLS certificate verification**. It must only be used
    /// during the initial SPAKE2 PIN-based pairing flow, where the shared PIN
    /// provides authentication independently of TLS. Using this for normal
    /// connections would allow man-in-the-middle attacks.
    ///
    /// # Errors
    ///
    /// Returns [`TransportError`] if TLS setup or the QUIC handshake fails.
    ///
    /// # Panics
    ///
    /// Panics if `"0.0.0.0:0"` cannot be parsed as a `SocketAddr` (unreachable
    /// in practice).
    pub async fn connect_insecure(server_addr: SocketAddr) -> Result<Self, TransportError> {
        let client_config = make_client_config_insecure()?;
        let bind_addr: SocketAddr = "0.0.0.0:0".parse().expect("valid wildcard address");
        let mut endpoint = Endpoint::client(bind_addr)?;
        endpoint.set_default_client_config(client_config);
        let connection = endpoint.connect(server_addr, "localhost")?.await?;
        Ok(Self::from_connection(connection))
    }

    /// Returns the DER-encoded certificate of the connected peer, if available.
    ///
    /// For a client connection this is the server's TLS certificate.
    /// Returns `None` if the peer did not present a certificate.
    #[must_use]
    pub fn peer_certificate(&self) -> Option<Vec<u8>> {
        let identity = self.connection.peer_identity()?;
        let certs = identity
            .downcast::<Vec<CertificateDer<'static>>>()
            .ok()?;
        certs.first().map(|c| c.as_ref().to_vec())
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
mod tests;
