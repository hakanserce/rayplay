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
