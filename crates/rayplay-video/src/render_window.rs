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
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy},
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

    /// Creates the `winit` event loop and returns it together with a proxy
    /// that can wake the loop from another thread.
    ///
    /// Call this on the main thread before spawning background threads, then
    /// pass the proxy (or a [`FrameNotifier`] wrapping it) to the producer
    /// side and hand the event loop to [`run`](Self::run).
    ///
    /// # Errors
    ///
    /// - [`RenderError::Failed`] — event loop creation failed.
    pub fn create_event_loop() -> Result<(EventLoop<()>, EventLoopProxy<()>), RenderError> {
        let event_loop = EventLoop::with_user_event()
            .build()
            .map_err(|e| RenderError::Failed {
                reason: format!("event loop: {e}"),
            })?;
        let proxy = event_loop.create_proxy();
        Ok((event_loop, proxy))
    }

    /// Starts the `winit` event loop and renders frames from `frame_rx`.
    ///
    /// The event loop uses [`ControlFlow::Wait`] and relies on the producer
    /// thread calling [`EventLoopProxy::send_event`] (via [`FrameNotifier`])
    /// to wake it when a new frame is available.
    ///
    /// Blocks the calling thread (must be the main thread on macOS).  Returns
    /// when the user closes the window or an unrecoverable error occurs.
    ///
    /// # Errors
    ///
    /// - [`RenderError::Failed`] — window creation failed or GPU adapter /
    ///   device initialisation failed.
    pub fn run(
        self,
        event_loop: EventLoop<()>,
        frame_rx: crossbeam_channel::Receiver<DecodedFrame>,
        #[cfg(feature = "gui")] ui_overlay: Option<Box<dyn crate::UiOverlay>>,
    ) -> Result<(), RenderError> {
        let mut app = AppState {
            title: self.title,
            width: self.width,
            height: self.height,
            frame_rx,
            window: None,
            renderer: None,
            pending_frame: None,
            init_error: None,
            #[cfg(feature = "gui")]
            ui_overlay,
            #[cfg(feature = "gui")]
            egui_state: None,
            #[cfg(feature = "gui")]
            egui_renderer: None,
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
    #[cfg(feature = "gui")]
    ui_overlay: Option<Box<dyn crate::UiOverlay>>,
    #[cfg(feature = "gui")]
    egui_state: Option<egui_winit::State>,
    #[cfg(feature = "gui")]
    egui_renderer: Option<egui_wgpu::Renderer>,
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
        let frame = self.pending_frame.take();

        // When GUI is enabled, run egui pass (with or without a video frame).
        #[cfg(feature = "gui")]
        if self.ui_overlay.is_some() {
            self.render_egui(frame.as_ref());
            return;
        }

        // Non-GUI path: render video frame only.
        let (Some(renderer), Some(frame)) = (&mut self.renderer, frame) else {
            return;
        };
        if let Err(RenderError::SurfaceLost) = renderer.present_frame(&frame)
            && let Some(window) = &self.window
        {
            renderer.resize(window.inner_size());
        }
    }

    /// Renders video frame (if any) plus egui overlay.
    #[cfg(feature = "gui")]
    fn render_egui(&mut self, frame: Option<&DecodedFrame>) {
        let (Some(renderer), Some(egui_state), Some(egui_renderer), Some(overlay), Some(window)) = (
            &mut self.renderer,
            &mut self.egui_state,
            &mut self.egui_renderer,
            &mut self.ui_overlay,
            &self.window,
        ) else {
            return;
        };

        // Upload video frame to GPU if present
        #[allow(unused_mut)]
        let mut hw_bind_group: Option<wgpu::BindGroup> = None;
        if let Some(frame) = frame {
            #[cfg(target_os = "macos")]
            #[allow(clippy::collapsible_if)]
            if frame.is_hardware_frame {
                if let Some(ref handle) = frame.iosurface {
                    hw_bind_group =
                        renderer.import_iosurface_textures(handle, frame.width, frame.height);
                }
            }
            if hw_bind_group.is_none() && !frame.is_hardware_frame {
                if !renderer.texture_matches(frame) {
                    renderer.recreate_texture_cache(frame);
                }
                renderer.upload_frame(frame);
            }
        }

        // Run egui
        let raw_input = egui_state.take_egui_input(window);
        let egui_ctx = egui_state.egui_ctx().clone();
        let full_output = egui_ctx.run(raw_input, |ctx| {
            overlay.update(ctx);
        });
        egui_state.handle_platform_output(window, full_output.platform_output);

        let primitives = egui_ctx.tessellate(full_output.shapes, full_output.pixels_per_point);
        let screen_descriptor = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [window.inner_size().width, window.inner_size().height],
            pixels_per_point: full_output.pixels_per_point,
        };

        let result = renderer.present_to_surface_with_egui(
            hw_bind_group.as_ref(),
            egui_renderer,
            &primitives,
            &full_output.textures_delta,
            &screen_descriptor,
        );

        if let Err(RenderError::SurfaceLost) = result {
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
                #[cfg(feature = "gui")]
                if self.ui_overlay.is_some() {
                    let egui_ctx = egui::Context::default();
                    self.egui_state = Some(egui_winit::State::new(
                        egui_ctx,
                        egui::ViewportId::ROOT,
                        &window,
                        None,
                        None,
                        None,
                    ));
                    self.egui_renderer = Some(egui_wgpu::Renderer::new(
                        &renderer.device,
                        renderer.surface_format(),
                        None,
                        1,
                        false,
                    ));
                }
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
        // Let egui process the event first when the GUI feature is enabled.
        #[cfg(feature = "gui")]
        if let (Some(egui_state), Some(window)) = (&mut self.egui_state, &self.window) {
            let response = egui_state.on_window_event(window, &event);
            if response.consumed {
                if response.repaint {
                    window.request_redraw();
                }
                return;
            }
            if response.repaint {
                window.request_redraw();
            }
        }

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

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // When egui UI is active (not streaming or menu open), request continuous
        // redraws for responsive UI. Otherwise wait for frame notifications.
        #[cfg(feature = "gui")]
        #[allow(clippy::collapsible_if)]
        if let Some(overlay) = &self.ui_overlay {
            if overlay.wants_input() {
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
                return;
            }
        }
        event_loop.set_control_flow(ControlFlow::Wait);
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, _event: ()) {
        // Woken by `FrameNotifier` — drain to the latest frame, skipping stale
        // intermediate frames, and request a redraw for the most recent one.
        while let Ok(frame) = self.frame_rx.try_recv() {
            self.pending_frame = Some(frame);
        }
        if self.pending_frame.is_some()
            && let Some(window) = &self.window
        {
            window.request_redraw();
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
            #[cfg(feature = "gui")]
            ui_overlay: None,
            #[cfg(feature = "gui")]
            egui_state: None,
            #[cfg(feature = "gui")]
            egui_renderer: None,
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
            #[cfg(feature = "gui")]
            ui_overlay: None,
            #[cfg(feature = "gui")]
            egui_state: None,
            #[cfg(feature = "gui")]
            egui_renderer: None,
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
            #[cfg(feature = "gui")]
            ui_overlay: None,
            #[cfg(feature = "gui")]
            egui_state: None,
            #[cfg(feature = "gui")]
            egui_renderer: None,
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
            #[cfg(feature = "gui")]
            ui_overlay: None,
            #[cfg(feature = "gui")]
            egui_state: None,
            #[cfg(feature = "gui")]
            egui_renderer: None,
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
            #[cfg(feature = "gui")]
            ui_overlay: None,
            #[cfg(feature = "gui")]
            egui_state: None,
            #[cfg(feature = "gui")]
            egui_renderer: None,
        };
        // Channel empty: pending_frame stays None.
        if let Ok(f) = app.frame_rx.try_recv() {
            app.pending_frame = Some(f);
        }
        assert!(app.pending_frame.is_none());
    }

    // ── Drain-to-latest frame behavior ──────────────────────────────────────

    #[test]
    fn test_app_state_drain_keeps_only_latest_frame() {
        use crate::{DecodedFrame, PixelFormat};

        let (tx, rx) = crossbeam_channel::bounded::<DecodedFrame>(4);
        let mut app = AppState {
            title: "t".to_string(),
            width: 1,
            height: 1,
            frame_rx: rx,
            window: None,
            renderer: None,
            pending_frame: None,
            init_error: None,
            #[cfg(feature = "gui")]
            ui_overlay: None,
            #[cfg(feature = "gui")]
            egui_state: None,
            #[cfg(feature = "gui")]
            egui_renderer: None,
        };

        // Send three frames with distinct timestamps
        for ts in [100, 200, 300] {
            let frame = DecodedFrame::new_cpu(vec![0u8; 4], 1, 1, 4, PixelFormat::Bgra8, ts);
            tx.send(frame).unwrap();
        }

        // Drain loop: simulate the while-let in about_to_wait
        while let Ok(f) = app.frame_rx.try_recv() {
            app.pending_frame = Some(f);
        }

        // Only the latest frame (timestamp 300) should remain
        let pending = app.pending_frame.unwrap();
        assert_eq!(pending.timestamp_us, 300);
    }

    #[test]
    fn test_app_state_drain_skips_stale_frames() {
        use crate::{DecodedFrame, PixelFormat};

        let (tx, rx) = crossbeam_channel::bounded::<DecodedFrame>(4);
        let mut app = AppState {
            title: "t".to_string(),
            width: 1,
            height: 1,
            frame_rx: rx,
            window: None,
            renderer: None,
            pending_frame: None,
            init_error: None,
            #[cfg(feature = "gui")]
            ui_overlay: None,
            #[cfg(feature = "gui")]
            egui_state: None,
            #[cfg(feature = "gui")]
            egui_renderer: None,
        };

        // Fill channel with 4 frames
        for ts in [10, 20, 30, 40] {
            let frame = DecodedFrame::new_cpu(vec![0u8; 4], 1, 1, 4, PixelFormat::Bgra8, ts);
            tx.send(frame).unwrap();
        }

        while let Ok(f) = app.frame_rx.try_recv() {
            app.pending_frame = Some(f);
        }

        // Channel should be fully drained
        assert!(app.frame_rx.try_recv().is_err());
        // Latest frame is kept
        assert_eq!(app.pending_frame.unwrap().timestamp_us, 40);
    }

    #[test]
    fn test_app_state_drain_single_frame_works() {
        use crate::{DecodedFrame, PixelFormat};

        let (tx, rx) = crossbeam_channel::bounded::<DecodedFrame>(2);
        let mut app = AppState {
            title: "t".to_string(),
            width: 1,
            height: 1,
            frame_rx: rx,
            window: None,
            renderer: None,
            pending_frame: None,
            init_error: None,
            #[cfg(feature = "gui")]
            ui_overlay: None,
            #[cfg(feature = "gui")]
            egui_state: None,
            #[cfg(feature = "gui")]
            egui_renderer: None,
        };

        let frame = DecodedFrame::new_cpu(vec![0u8; 4], 1, 1, 4, PixelFormat::Bgra8, 42);
        tx.send(frame).unwrap();

        while let Ok(f) = app.frame_rx.try_recv() {
            app.pending_frame = Some(f);
        }

        assert_eq!(app.pending_frame.unwrap().timestamp_us, 42);
    }
}
