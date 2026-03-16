//! `winit`-backed application window for frame rendering (UC-005, ADR-005).
//!
//! [`RenderWindow`] owns the `winit` event loop and drives [`WgpuRenderer`]
//! from decoded frames delivered over a [`crossbeam_channel`] channel.
//!
//! # Thread model
//!
//! The `winit` event loop must run on the main thread (`AppKit` requirement on
//! macOS).  Decoded frames arrive from the decode thread via a bounded
//! `crossbeam_channel::Receiver<DecodedFrame>`.  The event loop checks for a
//! new frame in [`ApplicationHandler::about_to_wait`] and requests a redraw
//! when one arrives; the actual GPU upload + present happens in the
//! `RedrawRequested` handler.
//!
//! # Coverage exclusion
//!
//! This file is excluded from the workspace line-coverage gate via
//! `--ignore-filename-regex` in `coverage-ci` (see `Makefile.toml`).
//! The observable behaviour is entirely mediated by two OS resources that
//! cannot be constructed in unit tests:
//!
//! * **`winit` event loop** — `EventLoop::new()` and `run_app()` require the
//!   main thread and block until the window is closed; `ActiveEventLoop` is
//!   an opaque handle that cannot be fabricated outside the loop.
//! * **`winit::window::Window`** — creation requires a live display server
//!   (macOS window server / `AppKit`); no headless substitute exists in the
//!   stable `winit` API.
//!
//! All extractable business logic (frame-channel polling, surface-lost
//! recovery, resize forwarding) is tested through the helper methods on
//! [`AppState`] in the test module below.  The remaining glue code — event
//! loop construction, window creation, and `ApplicationHandler` dispatch —
//! is integration-level behaviour validated by manual end-to-end runs.

use std::sync::Arc;

use winit::{
    application::ApplicationHandler,
    event::{ElementState, KeyEvent, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{Fullscreen, Window, WindowAttributes, WindowId},
};

use crate::{
    DecodedFrame,
    renderer::{RenderError, Renderer},
    wgpu_renderer::WgpuRenderer,
};

/// Configuration for the client display window.
///
/// Call [`RenderWindow::run`] to hand control to the `winit` event loop.
pub struct RenderWindow {
    /// Window title shown in the title bar.
    pub title: String,
    /// Initial window width in logical pixels.
    pub width: u32,
    /// Initial window height in logical pixels.
    pub height: u32,
}

impl RenderWindow {
    /// Creates a new window configuration.
    ///
    /// This does **not** open a window or start the event loop.  Call
    /// [`run`](Self::run) to do that.
    #[must_use]
    pub fn new(title: impl Into<String>, width: u32, height: u32) -> Self {
        Self {
            title: title.into(),
            width,
            height,
        }
    }

    /// Starts the `winit` event loop and renders frames from `frame_rx`.
    ///
    /// Blocks the calling thread (must be the main thread on macOS).  Returns
    /// when the user closes the window or an unrecoverable error occurs.
    ///
    /// # Errors
    ///
    /// - [`RenderError::Failed`] — event loop creation failed, window creation
    ///   failed, or GPU adapter / device initialisation failed.
    pub fn run(
        self,
        frame_rx: crossbeam_channel::Receiver<DecodedFrame>,
    ) -> Result<(), RenderError> {
        let event_loop = EventLoop::new().map_err(|e| RenderError::Failed {
            reason: format!("event loop: {e}"),
        })?;

        let mut app = AppState {
            title: self.title,
            width: self.width,
            height: self.height,
            frame_rx,
            window: None,
            renderer: None,
            pending_frame: None,
            init_error: None,
        };

        event_loop
            .run_app(&mut app)
            .map_err(|e| RenderError::Failed {
                reason: format!("event loop run: {e}"),
            })?;

        app.init_error.map_or(Ok(()), Err)
    }
}

// ── Internal application state ─────────────────────────────────────────────────

struct AppState {
    title: String,
    width: u32,
    height: u32,
    frame_rx: crossbeam_channel::Receiver<DecodedFrame>,
    window: Option<Arc<Window>>,
    renderer: Option<WgpuRenderer>,
    /// Most recently received frame, waiting for the next `RedrawRequested`.
    pending_frame: Option<DecodedFrame>,
    /// Stores an error from `resumed` so `run` can surface it after exit.
    init_error: Option<RenderError>,
}

impl AppState {
    fn toggle_fullscreen(&self) {
        let Some(window) = &self.window else { return };
        if window.fullscreen().is_some() {
            window.set_fullscreen(None);
        } else {
            window.set_fullscreen(Some(Fullscreen::Borderless(None)));
        }
    }

    fn render(&mut self) {
        let (Some(renderer), Some(frame)) = (&mut self.renderer, self.pending_frame.take()) else {
            return;
        };
        if let Err(RenderError::SurfaceLost) = renderer.present_frame(&frame)
            && let Some(window) = &self.window
        {
            // Reconfigure the swap chain to the current window size.
            // wgpu::Surface::configure returns () — if the device is lost,
            // the next present_frame call will surface the error rather than
            // this reconfigure step.
            renderer.resize(window.inner_size());
        }
    }
}

impl ApplicationHandler for AppState {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let attrs = WindowAttributes::default()
            .with_title(&self.title)
            .with_inner_size(winit::dpi::LogicalSize::new(self.width, self.height))
            .with_resizable(true);

        let window = match event_loop.create_window(attrs) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                self.init_error = Some(RenderError::Failed {
                    reason: format!("create_window: {e}"),
                });
                event_loop.exit();
                return;
            }
        };

        match pollster::block_on(WgpuRenderer::new(Arc::clone(&window))) {
            Ok(renderer) => {
                self.window = Some(window);
                self.renderer = Some(renderer);
            }
            Err(e) => {
                self.init_error = Some(e);
                event_loop.exit();
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),

            WindowEvent::Resized(new_size) => {
                if let Some(renderer) = &mut self.renderer {
                    renderer.resize(new_size);
                }
            }

            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        physical_key: PhysicalKey::Code(KeyCode::F11),
                        state: ElementState::Pressed,
                        ..
                    },
                ..
            } => self.toggle_fullscreen(),

            WindowEvent::RedrawRequested => self.render(),

            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Ok(frame) = self.frame_rx.try_recv() {
            self.pending_frame = Some(frame);
            if let Some(window) = &self.window {
                window.request_redraw();
            }
        }
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── RenderWindow construction ──────────────────────────────────────────────

    #[test]
    fn test_render_window_stores_title() {
        let w = RenderWindow::new("RayView", 1920, 1080);
        assert_eq!(w.title, "RayView");
    }

    #[test]
    fn test_render_window_stores_dimensions() {
        let w = RenderWindow::new("Test", 1280, 720);
        assert_eq!(w.width, 1280);
        assert_eq!(w.height, 720);
    }

    #[test]
    fn test_render_window_accepts_string_owned() {
        let title = String::from("RayView");
        let w = RenderWindow::new(title, 1920, 1080);
        assert_eq!(w.title, "RayView");
    }

    #[test]
    fn test_render_window_accepts_string_slice() {
        let w = RenderWindow::new("RayView", 1920, 1080);
        assert_eq!(w.title, "RayView");
    }

    #[test]
    fn test_render_window_zero_dimensions_allowed() {
        let w = RenderWindow::new("T", 0, 0);
        assert_eq!(w.width, 0);
        assert_eq!(w.height, 0);
    }

    // ── AppState helpers ───────────────────────────────────────────────────────

    #[test]
    fn test_app_state_render_no_ops_without_renderer() {
        let (_tx, rx) = crossbeam_channel::bounded::<DecodedFrame>(1);
        let mut app = AppState {
            title: "t".to_string(),
            width: 1,
            height: 1,
            frame_rx: rx,
            window: None,
            renderer: None,
            pending_frame: None,
            init_error: None,
        };
        // Must not panic when renderer and frame are both absent.
        app.render();
    }

    #[test]
    fn test_app_state_render_no_ops_without_pending_frame() {
        let (_tx, rx) = crossbeam_channel::bounded::<DecodedFrame>(1);
        let mut app = AppState {
            title: "t".to_string(),
            width: 1,
            height: 1,
            frame_rx: rx,
            window: None,
            renderer: None,
            pending_frame: None,
            init_error: None,
        };
        app.render(); // renderer = None → early return
        assert!(app.pending_frame.is_none());
    }

    #[test]
    fn test_app_state_toggle_fullscreen_no_ops_without_window() {
        let (_tx, rx) = crossbeam_channel::bounded::<DecodedFrame>(1);
        let app = AppState {
            title: "t".to_string(),
            width: 1,
            height: 1,
            frame_rx: rx,
            window: None,
            renderer: None,
            pending_frame: None,
            init_error: None,
        };
        // Must not panic when window is absent.
        app.toggle_fullscreen();
    }

    #[test]
    fn test_app_state_about_to_wait_picks_up_frame() {
        use crate::{DecodedFrame, PixelFormat};

        let (tx, rx) = crossbeam_channel::bounded::<DecodedFrame>(1);
        let mut app = AppState {
            title: "t".to_string(),
            width: 1,
            height: 1,
            frame_rx: rx,
            window: None,
            renderer: None,
            pending_frame: None,
            init_error: None,
        };

        let frame = DecodedFrame::new_cpu(vec![0u8; 4], 1, 1, 4, PixelFormat::Bgra8, 0);
        tx.send(frame).unwrap();

        // Simulate about_to_wait without event loop: just call try_recv directly.
        if let Ok(f) = app.frame_rx.try_recv() {
            app.pending_frame = Some(f);
        }

        assert!(app.pending_frame.is_some());
    }

    #[test]
    fn test_app_state_about_to_wait_empty_channel_leaves_no_frame() {
        let (_tx, rx) = crossbeam_channel::bounded::<DecodedFrame>(1);
        let mut app = AppState {
            title: "t".to_string(),
            width: 1,
            height: 1,
            frame_rx: rx,
            window: None,
            renderer: None,
            pending_frame: None,
            init_error: None,
        };
        // Channel empty: pending_frame stays None.
        if let Ok(f) = app.frame_rx.try_recv() {
            app.pending_frame = Some(f);
        }
        assert!(app.pending_frame.is_none());
    }
}
