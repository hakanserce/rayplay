//! Core streaming logic and shared traits for `RayPlay`.

pub mod frame;
pub mod packet;
pub mod pairing;
pub mod session;

pub use frame::RawFrame;
pub use packet::EncodedPacket;
pub use pairing::{PairingError, TrustDatabase, TrustedClient};
pub use session::{ControlMessage, PairingOutcome, SessionError, SessionState, StreamParams};

use std::future::Future;
use thiserror::Error;

/// Errors produced by the network transport layer.
#[derive(Debug, Error)]
pub enum NetworkError {
    /// A generic transport-level error with a descriptive message.
    #[error("transport error: {0}")]
    Transport(String),
    /// The connection was closed by the remote peer.
    #[error("connection closed")]
    ConnectionClosed,
    /// The local endpoint was shut down.
    #[error("endpoint closed")]
    EndpointClosed,
}

/// Platform-agnostic network transport abstraction.
///
/// Implementations live in `rayplay-network`. This trait keeps
/// `rayplay-video` and `rayplay-input` independent of `quinn`.
pub trait NetworkTransport: Send {
    /// Sends an encoded video packet to the remote peer.
    fn send_video(
        &mut self,
        packet: &EncodedPacket,
    ) -> impl Future<Output = Result<(), NetworkError>> + Send;

    /// Receives the next reassembled video packet from the remote peer.
    fn recv_video(&mut self) -> impl Future<Output = Result<EncodedPacket, NetworkError>> + Send;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::packet::EncodedPacket;
    use std::collections::VecDeque;

    /// A mock transport used to verify the `NetworkTransport` trait contract.
    struct MockTransport {
        outbox: Vec<EncodedPacket>,
        inbox: VecDeque<EncodedPacket>,
    }

    impl MockTransport {
        fn new() -> Self {
            Self {
                outbox: Vec::new(),
                inbox: VecDeque::new(),
            }
        }

        fn enqueue_incoming(&mut self, packet: EncodedPacket) {
            self.inbox.push_back(packet);
        }
    }

    impl NetworkTransport for MockTransport {
        async fn send_video(&mut self, packet: &EncodedPacket) -> Result<(), NetworkError> {
            self.outbox.push(packet.clone());
            Ok(())
        }

        async fn recv_video(&mut self) -> Result<EncodedPacket, NetworkError> {
            self.inbox.pop_front().ok_or(NetworkError::ConnectionClosed)
        }
    }

    #[tokio::test]
    async fn test_mock_transport_send_video_stores_packet() {
        let mut t = MockTransport::new();
        let pkt = EncodedPacket::new(vec![1, 2, 3], true, 0, 16_667);
        t.send_video(&pkt).await.expect("send should succeed");
        assert_eq!(t.outbox.len(), 1);
        assert_eq!(t.outbox[0].data, pkt.data);
    }

    #[tokio::test]
    async fn test_mock_transport_recv_video_returns_queued_packet() {
        let mut t = MockTransport::new();
        let pkt = EncodedPacket::new(vec![10, 20], false, 100, 16_667);
        t.enqueue_incoming(pkt.clone());
        let received = t.recv_video().await.expect("recv should succeed");
        assert_eq!(received.data, pkt.data);
        assert_eq!(received.is_keyframe, pkt.is_keyframe);
    }

    #[tokio::test]
    async fn test_mock_transport_recv_video_empty_returns_connection_closed() {
        let mut t = MockTransport::new();
        let err = t.recv_video().await.unwrap_err();
        assert!(matches!(err, NetworkError::ConnectionClosed));
    }

    #[tokio::test]
    async fn test_mock_transport_send_multiple_packets() {
        let mut t = MockTransport::new();
        for i in 0..5u8 {
            let pkt = EncodedPacket::new(vec![i], i == 0, u64::from(i), 16_667);
            t.send_video(&pkt).await.expect("send should succeed");
        }
        assert_eq!(t.outbox.len(), 5);
    }

    #[tokio::test]
    async fn test_mock_transport_recv_video_fifo_order() {
        let mut t = MockTransport::new();
        let pkt1 = EncodedPacket::new(vec![1], true, 0, 16_667);
        let pkt2 = EncodedPacket::new(vec![2], false, 1, 16_667);
        t.enqueue_incoming(pkt1.clone());
        t.enqueue_incoming(pkt2.clone());

        let r1 = t.recv_video().await.expect("first recv");
        let r2 = t.recv_video().await.expect("second recv");
        assert_eq!(r1.data, pkt1.data);
        assert_eq!(r2.data, pkt2.data);
    }

    #[test]
    fn test_network_error_transport_display() {
        let e = NetworkError::Transport("timeout".to_string());
        assert_eq!(e.to_string(), "transport error: timeout");
    }

    #[test]
    fn test_network_error_connection_closed_display() {
        let e = NetworkError::ConnectionClosed;
        assert_eq!(e.to_string(), "connection closed");
    }

    #[test]
    fn test_network_error_endpoint_closed_display() {
        let e = NetworkError::EndpointClosed;
        assert_eq!(e.to_string(), "endpoint closed");
    }

    #[test]
    fn test_network_error_debug_format() {
        let e = NetworkError::Transport("err".to_string());
        let s = format!("{e:?}");
        assert!(s.contains("Transport"));
    }
}
