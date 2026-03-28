//! Wake-up notifier for the `winit` render loop.
//!
//! [`FrameNotifier`] wraps a [`winit::event_loop::EventLoopProxy`] and signals
//! the render loop each time a decoded frame is sent to the channel.  This
//! allows the event loop to use [`ControlFlow::Wait`] instead of
//! [`ControlFlow::Poll`], eliminating the busy-loop that caused high CPU usage.

use winit::event_loop::EventLoopProxy;

/// Lightweight, cloneable handle that wakes the `winit` event loop.
///
/// The producer thread should call [`notify`](Self::notify) after each
/// successful frame send so the render loop picks up the frame immediately.
#[derive(Debug, Clone)]
pub struct FrameNotifier {
    proxy: Option<EventLoopProxy<()>>,
}

impl FrameNotifier {
    /// Creates a new notifier backed by the given event-loop proxy.
    #[must_use]
    pub fn new(proxy: EventLoopProxy<()>) -> Self {
        Self { proxy: Some(proxy) }
    }

    /// Creates a no-op notifier that silently discards wake signals.
    ///
    /// Useful in tests where no event loop is available.
    #[must_use]
    pub fn no_op() -> Self {
        Self { proxy: None }
    }

    /// Wakes the `winit` event loop so it drains the frame channel.
    ///
    /// If the event loop has already exited (or the notifier is no-op), the
    /// call is silently ignored.
    pub fn notify(&self) {
        if let Some(proxy) = &self.proxy {
            let _ = proxy.send_event(());
        }
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // FrameNotifier wraps an EventLoopProxy<()>, which requires a live event
    // loop to construct.  The type itself is trivial (newtype + send_event),
    // so we verify compile-time properties only.

    #[test]
    fn test_frame_notifier_is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<FrameNotifier>();
    }
}
