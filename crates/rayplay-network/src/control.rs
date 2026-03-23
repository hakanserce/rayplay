//! Control channel for session management over a reliable QUIC stream (ADR-010).
//!
//! Provides [`ControlSender`] and [`ControlReceiver`] for exchanging
//! [`ControlMessage`]s using length-prefixed JSON over a QUIC bidirectional
//! stream.

use quinn::{RecvStream, SendStream};
use rayplay_core::session::{ControlMessage, SessionError};

use crate::wire::TransportError;

/// Maximum control message size in bytes (64 KiB sanity limit).
pub const MAX_CONTROL_MESSAGE_SIZE: u32 = 65_536;

/// Writes [`ControlMessage`]s to a QUIC send stream.
pub struct ControlSender {
    pub(crate) stream: SendStream,
}

/// Reads [`ControlMessage`]s from a QUIC receive stream.
pub struct ControlReceiver {
    stream: RecvStream,
}

/// Combined control channel handle (one sender + one receiver).
pub struct ControlChannel {
    /// The sending half of the control channel.
    pub sender: ControlSender,
    /// The receiving half of the control channel.
    pub receiver: ControlReceiver,
}

impl ControlSender {
    /// Wraps an existing [`SendStream`].
    pub(crate) fn new(stream: SendStream) -> Self {
        Self { stream }
    }

    /// Sends a [`ControlMessage`] as length-prefixed JSON.
    ///
    /// Wire format: `[u32 LE length][JSON bytes]`.
    ///
    /// # Errors
    ///
    /// - [`TransportError::MessageTooLarge`] if the serialized message exceeds
    ///   [`MAX_CONTROL_MESSAGE_SIZE`].
    /// - [`TransportError::StreamWrite`] if the QUIC stream write fails.
    pub async fn send(&mut self, msg: &ControlMessage) -> Result<(), TransportError> {
        let json =
            serde_json::to_vec(msg).map_err(|e| TransportError::MessageParse(e.to_string()))?;

        let len = u32::try_from(json.len())
            .ok()
            .filter(|&l| l <= MAX_CONTROL_MESSAGE_SIZE)
            .ok_or(TransportError::MessageTooLarge(json.len()))?;

        self.stream
            .write_all(&len.to_le_bytes())
            .await
            .map_err(|e| TransportError::StreamWrite(e.to_string()))?;

        self.stream
            .write_all(&json)
            .await
            .map_err(|e| TransportError::StreamWrite(e.to_string()))?;

        Ok(())
    }
}

impl ControlReceiver {
    /// Wraps an existing [`RecvStream`].
    pub(crate) fn new(stream: RecvStream) -> Self {
        Self { stream }
    }

    /// Reads the next [`ControlMessage`].
    ///
    /// Returns `Ok(None)` if the stream was cleanly closed by the peer.
    ///
    /// # Errors
    ///
    /// - [`TransportError::MessageTooLarge`] if the declared length exceeds
    ///   [`MAX_CONTROL_MESSAGE_SIZE`].
    /// - [`TransportError::StreamRead`] if the QUIC stream read fails.
    /// - [`TransportError::MessageParse`] if JSON deserialization fails.
    pub async fn recv(&mut self) -> Result<Option<ControlMessage>, TransportError> {
        let mut len_buf = [0u8; 4];
        match self.stream.read_exact(&mut len_buf).await {
            Ok(()) => {}
            Err(quinn::ReadExactError::FinishedEarly(_)) => return Ok(None),
            Err(e) => return Err(TransportError::StreamRead(e.to_string())),
        }

        let len = u32::from_le_bytes(len_buf);
        if len > MAX_CONTROL_MESSAGE_SIZE {
            return Err(TransportError::MessageTooLarge(len as usize));
        }

        let mut payload = vec![0u8; len as usize];
        self.stream
            .read_exact(&mut payload)
            .await
            .map_err(|e| TransportError::StreamRead(e.to_string()))?;

        let msg: ControlMessage = serde_json::from_slice(&payload)
            .map_err(|e| TransportError::MessageParse(e.to_string()))?;

        Ok(Some(msg))
    }
}

impl ControlChannel {
    /// Receives a control message with context for error messages.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError::PairingFailed`] if the stream was closed,
    /// or [`SessionError::Transport`] on transport errors.
    pub async fn recv_msg(&mut self, operation: &str) -> Result<ControlMessage, SessionError> {
        match self.receiver.recv().await {
            Ok(Some(msg)) => Ok(msg),
            Ok(None) => Err(SessionError::PairingFailed(format!(
                "stream closed during {operation}"
            ))),
            Err(e) => Err(SessionError::Transport(e.to_string())),
        }
    }

    /// Sends a control message.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError::Transport`] on transport errors.
    pub async fn send_msg(&mut self, msg: &ControlMessage) -> Result<(), SessionError> {
        self.sender
            .send(msg)
            .await
            .map_err(|e| SessionError::Transport(e.to_string()))
    }
}

#[cfg(test)]
mod tests;
