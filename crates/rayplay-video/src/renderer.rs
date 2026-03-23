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
mod tests;
