//! Session handshake for stream parameter negotiation (ADR-010).
//!
//! The client proposes [`StreamParams`] via [`client_handshake`], the host
//! receives them, optionally adjusts (e.g. caps resolution), and responds
//! via [`host_handshake`]. Both sides use the agreed parameters for media
//! streaming.

use rayplay_core::session::{ControlMessage, SessionError, StreamParams};

use crate::control::ControlChannel;

/// Runs the client side of the handshake.
///
/// Sends a [`ControlMessage::HandshakeRequest`] with the desired parameters and
/// waits for a [`ControlMessage::HandshakeResponse`] from the host.
///
/// # Errors
///
/// - [`SessionError::Transport`] if the control channel fails.
/// - [`SessionError::HandshakeFailed`] if the host sends an unexpected message
///   or closes the stream.
pub async fn client_handshake(
    control: &mut ControlChannel,
    desired: StreamParams,
) -> Result<StreamParams, SessionError> {
    control
        .sender
        .send(&ControlMessage::HandshakeRequest(desired))
        .await
        .map_err(|e| SessionError::Transport(e.to_string()))?;

    match control.receiver.recv().await {
        Ok(Some(ControlMessage::HandshakeResponse(params))) => Ok(params),
        Ok(Some(other)) => Err(SessionError::HandshakeFailed(format!(
            "expected HandshakeResponse, got {other:?}"
        ))),
        Ok(None) => Err(SessionError::HandshakeFailed(
            "stream closed during handshake".to_string(),
        )),
        Err(e) => Err(SessionError::Transport(e.to_string())),
    }
}

/// Runs the host side of the handshake.
///
/// Waits for a [`ControlMessage::HandshakeRequest`], passes the proposed
/// parameters through `adjust_fn` (which may cap resolution, change codec,
/// etc.), and sends the adjusted result back as a [`ControlMessage::HandshakeResponse`].
///
/// # Errors
///
/// - [`SessionError::Transport`] if the control channel fails.
/// - [`SessionError::HandshakeFailed`] if the client sends an unexpected
///   message or closes the stream.
pub async fn host_handshake<F>(
    control: &mut ControlChannel,
    adjust_fn: F,
) -> Result<StreamParams, SessionError>
where
    F: FnOnce(StreamParams) -> StreamParams,
{
    let proposed = match control.receiver.recv().await {
        Ok(Some(ControlMessage::HandshakeRequest(params))) => params,
        Ok(Some(other)) => {
            return Err(SessionError::HandshakeFailed(format!(
                "expected HandshakeRequest, got {other:?}"
            )));
        }
        Ok(None) => {
            return Err(SessionError::HandshakeFailed(
                "stream closed during handshake".to_string(),
            ));
        }
        Err(e) => return Err(SessionError::Transport(e.to_string())),
    };

    let agreed = adjust_fn(proposed);

    control
        .sender
        .send(&ControlMessage::HandshakeResponse(agreed.clone()))
        .await
        .map_err(|e| SessionError::Transport(e.to_string()))?;

    Ok(agreed)
}

#[cfg(test)]
mod tests;
