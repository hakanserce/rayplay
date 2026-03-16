//! [`Renderer`] trait and [`RenderError`] for frame presentation (UC-005).
//!
//! See ADR-005 for the rendering architecture decision (`winit` + `wgpu`).

use thiserror::Error;

use crate::DecodedFrame;

/// Errors produced by the rendering pipeline.
#[derive(Debug, Error)]
pub enum RenderError {
    /// The render surface was lost (e.g. window minimized or resized).
    ///
    /// Call [`crate::WgpuRenderer::resize`] with the new window size, then
    /// retry the next frame.
    #[error("render surface lost")]
    SurfaceLost,

    /// No suitable GPU adapter was found on this device.
    #[error("no suitable GPU adapter found")]
    NoAdapter,

    /// A general rendering failure with a human-readable reason.
    #[error("rendering failed: {reason}")]
    Failed { reason: String },
}

/// Trait for presenting decoded video frames to a display surface.
///
/// Implementations must be [`Send`] so they can be driven from a dedicated
/// render thread. See ADR-005 for the rendering architecture.
pub trait Renderer: Send {
    /// Presents a decoded video frame to the display surface.
    ///
    /// # Errors
    ///
    /// - [`RenderError::SurfaceLost`] — the surface must be reconfigured via
    ///   [`crate::WgpuRenderer::resize`] before the next call.
    /// - [`RenderError::Failed`] — a GPU or driver error occurred.
    fn present_frame(&mut self, frame: &DecodedFrame) -> Result<(), RenderError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DecodedFrame, PixelFormat};

    // ── RenderError display messages ───────────────────────────────────────────

    #[test]
    fn test_render_error_surface_lost_message() {
        let msg = RenderError::SurfaceLost.to_string();
        assert!(msg.contains("surface lost"), "got: {msg}");
    }

    #[test]
    fn test_render_error_no_adapter_message() {
        let msg = RenderError::NoAdapter.to_string();
        assert!(msg.contains("adapter"), "got: {msg}");
    }

    #[test]
    fn test_render_error_failed_includes_reason() {
        let msg = RenderError::Failed {
            reason: "out of memory".to_string(),
        }
        .to_string();
        assert!(msg.contains("out of memory"), "got: {msg}");
    }

    #[test]
    fn test_render_error_failed_debug_format() {
        let err = RenderError::Failed {
            reason: "gpu crash".to_string(),
        };
        let dbg = format!("{err:?}");
        assert!(dbg.contains("Failed"), "got: {dbg}");
        assert!(dbg.contains("gpu crash"), "got: {dbg}");
    }

    #[test]
    fn test_render_error_surface_lost_debug_format() {
        let dbg = format!("{:?}", RenderError::SurfaceLost);
        assert!(dbg.contains("SurfaceLost"), "got: {dbg}");
    }

    #[test]
    fn test_render_error_no_adapter_debug_format() {
        let dbg = format!("{:?}", RenderError::NoAdapter);
        assert!(dbg.contains("NoAdapter"), "got: {dbg}");
    }

    // ── Renderer trait contract (NullRenderer test double) ─────────────────────

    struct NullRenderer {
        next_result: Option<RenderError>,
        call_count: usize,
    }

    impl NullRenderer {
        fn ok() -> Self {
            Self {
                next_result: None,
                call_count: 0,
            }
        }

        fn with_error(err: RenderError) -> Self {
            Self {
                next_result: Some(err),
                call_count: 0,
            }
        }
    }

    impl Renderer for NullRenderer {
        fn present_frame(&mut self, _frame: &DecodedFrame) -> Result<(), RenderError> {
            self.call_count += 1;
            match self.next_result.take() {
                None => Ok(()),
                Some(RenderError::SurfaceLost) => Err(RenderError::SurfaceLost),
                Some(RenderError::NoAdapter) => Err(RenderError::NoAdapter),
                Some(RenderError::Failed { reason }) => Err(RenderError::Failed { reason }),
            }
        }
    }

    fn make_bgra_frame() -> DecodedFrame {
        DecodedFrame::new_cpu(vec![0u8; 4], 1, 1, 4, PixelFormat::Bgra8, 0)
    }

    #[test]
    fn test_null_renderer_ok_path() {
        let mut r = NullRenderer::ok();
        assert!(r.present_frame(&make_bgra_frame()).is_ok());
        assert_eq!(r.call_count, 1);
    }

    #[test]
    fn test_null_renderer_surface_lost_error() {
        let mut r = NullRenderer::with_error(RenderError::SurfaceLost);
        let err = r.present_frame(&make_bgra_frame()).unwrap_err();
        assert!(matches!(err, RenderError::SurfaceLost));
        assert_eq!(r.call_count, 1);
    }

    #[test]
    fn test_null_renderer_failed_error() {
        let mut r = NullRenderer::with_error(RenderError::Failed {
            reason: "oops".to_string(),
        });
        let err = r.present_frame(&make_bgra_frame()).unwrap_err();
        assert!(matches!(err, RenderError::Failed { .. }));
    }

    #[test]
    fn test_null_renderer_no_adapter_error() {
        let mut r = NullRenderer::with_error(RenderError::NoAdapter);
        let err = r.present_frame(&make_bgra_frame()).unwrap_err();
        assert!(matches!(err, RenderError::NoAdapter));
    }

    #[test]
    fn test_null_renderer_counts_calls() {
        let mut r = NullRenderer::ok();
        let frame = make_bgra_frame();
        r.present_frame(&frame).unwrap();
        r.present_frame(&frame).unwrap();
        r.present_frame(&frame).unwrap();
        assert_eq!(r.call_count, 3);
    }

    #[test]
    fn test_null_renderer_recovers_after_error() {
        let mut r = NullRenderer::with_error(RenderError::SurfaceLost);
        let frame = make_bgra_frame();
        assert!(r.present_frame(&frame).is_err());
        // Next call succeeds (error was consumed)
        assert!(r.present_frame(&frame).is_ok());
        assert_eq!(r.call_count, 2);
    }

    #[test]
    fn test_renderer_impl_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<NullRenderer>();
    }
}
