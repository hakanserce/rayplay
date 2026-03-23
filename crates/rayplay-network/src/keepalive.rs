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
mod tests;
