//! GUI components and UI state management for `RayView` client (UC-101).
//!
//! Provides the UI application state machine, screen implementations, and
//! communication types for the `RayView` graphical interface.
//!
//! # Architecture
//!
//! ```text
//! UiApp ──► AppScreen state machine ──► per-screen functions
//!   │
//!   ├── receives UiEvent (network status, pairing results)
//!   └── sends UiAction (connect, disconnect, submit PIN)
//! ```

pub mod app;
pub mod events;
pub mod host;
pub mod screens;

pub use app::{AppScreen, UiApp};
pub use events::{UiAction, UiEvent};
pub use host::HostEntry;
