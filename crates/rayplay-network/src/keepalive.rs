//! Keepalive exchange for disconnect detection (ADR-010).
//!
//! Sends [`ControlMessage::Keepalive`] at a fixed interval and expects
//! [`ControlMessage::KeepaliveAck`] within a timeout. If the peer stops
//! responding, [`SessionError::KeepaliveTimeout`] is returned.

use std::time::Duration;

use rayplay_core::session::{ControlMessage, SessionError};
use tokio_util::sync::CancellationToken;

use crate::control::{ControlReceiver, ControlSender};

/// Default keepalive interval.
pub const DEFAULT_KEEPALIVE_INTERVAL: Duration = Duration::from_secs(5);

/// Default keepalive timeout (how long to wait for an ack).
pub const DEFAULT_KEEPALIVE_TIMEOUT: Duration = Duration::from_secs(10);

/// Runs the keepalive sender loop.
///
/// Sends a [`ControlMessage::Keepalive`] every `interval`. Stops when
/// `cancel` is triggered or a send error occurs.
///
/// # Errors
///
/// Returns [`SessionError::Transport`] if a send fails.
pub async fn run_keepalive_sender(
    sender: &mut ControlSender,
    interval: Duration,
    cancel: CancellationToken,
) -> Result<(), SessionError> {
    loop {
        tokio::select! {
            () = cancel.cancelled() => return Ok(()),
            () = tokio::time::sleep(interval) => {}
        }

        sender
            .send(&ControlMessage::Keepalive)
            .await
            .map_err(|e| SessionError::Transport(e.to_string()))?;
    }
}

/// Runs the keepalive responder loop.
///
/// Reads control messages. On [`ControlMessage::Keepalive`], sends
/// [`ControlMessage::KeepaliveAck`]. On [`ControlMessage::Disconnect`],
/// returns [`SessionError::RemoteClosed`]. Other messages are ignored
/// (they belong to a higher-level protocol layer).
///
/// If no message arrives within `timeout`, returns
/// [`SessionError::KeepaliveTimeout`].
///
/// # Errors
///
/// - [`SessionError::KeepaliveTimeout`] if the peer stops sending.
/// - [`SessionError::RemoteClosed`] on `Disconnect`.
/// - [`SessionError::Transport`] on stream errors.
pub async fn run_keepalive_responder(
    sender: &mut ControlSender,
    receiver: &mut ControlReceiver,
    timeout: Duration,
    cancel: CancellationToken,
) -> Result<(), SessionError> {
    loop {
        let msg = tokio::select! {
            () = cancel.cancelled() => return Ok(()),
            result = tokio::time::timeout(timeout, receiver.recv()) => {
                match result {
                    Ok(Ok(Some(msg))) => msg,
                    Ok(Ok(None)) => return Err(SessionError::RemoteClosed),
                    Ok(Err(e)) => return Err(SessionError::Transport(e.to_string())),
                    Err(_) => return Err(SessionError::KeepaliveTimeout),
                }
            }
        };

        match msg {
            ControlMessage::Keepalive => {
                sender
                    .send(&ControlMessage::KeepaliveAck)
                    .await
                    .map_err(|e| SessionError::Transport(e.to_string()))?;
            }
            ControlMessage::Disconnect => return Err(SessionError::RemoteClosed),
            _ => { /* ignore other messages in this loop */ }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::control::ControlChannel;
    use crate::transport::QuicVideoTransport;
    use std::net::SocketAddr;

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

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_keepalive_sender_sends_on_interval() {
        let (mut client, mut server) = control_pair().await;
        let cancel = CancellationToken::new();
        let cancel2 = cancel.clone();

        let sender_task = tokio::spawn(async move {
            run_keepalive_sender(
                &mut client.sender,
                Duration::from_millis(50),
                cancel2,
            )
            .await
        });

        // Receive at least 2 keepalives
        for _ in 0..2 {
            let msg = server.receiver.recv().await.unwrap().unwrap();
            assert_eq!(msg, ControlMessage::Keepalive);
        }

        cancel.cancel();
        let result = sender_task.await.unwrap();
        assert!(result.is_ok());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_keepalive_sender_stops_on_cancel() {
        let (mut client, _server) = control_pair().await;
        let cancel = CancellationToken::new();
        cancel.cancel();

        let result = run_keepalive_sender(
            &mut client.sender,
            Duration::from_secs(60),
            cancel,
        )
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_keepalive_responder_replies_with_ack() {
        let (mut client, mut server) = control_pair().await;
        let cancel = CancellationToken::new();
        let cancel2 = cancel.clone();

        let responder_task = tokio::spawn(async move {
            run_keepalive_responder(
                &mut server.sender,
                &mut server.receiver,
                Duration::from_secs(5),
                cancel2,
            )
            .await
        });

        // Send keepalive from client
        client
            .sender
            .send(&ControlMessage::Keepalive)
            .await
            .unwrap();

        // Read ack
        let ack = client.receiver.recv().await.unwrap().unwrap();
        assert_eq!(ack, ControlMessage::KeepaliveAck);

        cancel.cancel();
        let result = responder_task.await.unwrap();
        assert!(result.is_ok());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_keepalive_responder_timeout() {
        let (mut _client, mut server) = control_pair().await;
        let cancel = CancellationToken::new();

        // No keepalives sent → should timeout
        let result = run_keepalive_responder(
            &mut server.sender,
            &mut server.receiver,
            Duration::from_millis(50),
            cancel,
        )
        .await;
        assert!(matches!(result, Err(SessionError::KeepaliveTimeout)));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_keepalive_responder_disconnect_message() {
        let (mut client, mut server) = control_pair().await;
        let cancel = CancellationToken::new();

        client
            .sender
            .send(&ControlMessage::Disconnect)
            .await
            .unwrap();

        let result = run_keepalive_responder(
            &mut server.sender,
            &mut server.receiver,
            Duration::from_secs(5),
            cancel,
        )
        .await;
        assert!(matches!(result, Err(SessionError::RemoteClosed)));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_keepalive_responder_cancel() {
        let (_client, mut server) = control_pair().await;
        let cancel = CancellationToken::new();
        cancel.cancel();

        let result = run_keepalive_responder(
            &mut server.sender,
            &mut server.receiver,
            Duration::from_secs(60),
            cancel,
        )
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_keepalive_responder_ignores_other_messages() {
        let (mut client, mut server) = control_pair().await;
        let cancel = CancellationToken::new();
        let cancel2 = cancel.clone();

        let responder_task = tokio::spawn(async move {
            run_keepalive_responder(
                &mut server.sender,
                &mut server.receiver,
                Duration::from_secs(5),
                cancel2,
            )
            .await
        });

        // Send a KeepaliveAck (which the responder should ignore) then a Keepalive
        client
            .sender
            .send(&ControlMessage::KeepaliveAck)
            .await
            .unwrap();
        client
            .sender
            .send(&ControlMessage::Keepalive)
            .await
            .unwrap();

        // Should still get an ack for the Keepalive
        let ack = client.receiver.recv().await.unwrap().unwrap();
        assert_eq!(ack, ControlMessage::KeepaliveAck);

        cancel.cancel();
        let result = responder_task.await.unwrap();
        assert!(result.is_ok());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_keepalive_responder_stream_closed_cleanly() {
        let (mut client, mut server) = control_pair().await;
        let cancel = CancellationToken::new();

        // Cleanly finish the client's send stream so responder sees Ok(None).
        client.sender.stream.finish().unwrap();

        let result = run_keepalive_responder(
            &mut server.sender,
            &mut server.receiver,
            Duration::from_secs(5),
            cancel,
        )
        .await;
        assert!(matches!(result, Err(SessionError::RemoteClosed)));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_keepalive_responder_transport_error() {
        let (client, mut server) = control_pair().await;
        let cancel = CancellationToken::new();

        // Drop entire client to trigger transport error.
        drop(client);
        tokio::time::sleep(Duration::from_millis(50)).await;

        let result = run_keepalive_responder(
            &mut server.sender,
            &mut server.receiver,
            Duration::from_secs(5),
            cancel,
        )
        .await;
        assert!(result.is_err());
    }

    #[test]
    fn test_default_constants() {
        assert_eq!(DEFAULT_KEEPALIVE_INTERVAL, Duration::from_secs(5));
        assert_eq!(DEFAULT_KEEPALIVE_TIMEOUT, Duration::from_secs(10));
    }
}
