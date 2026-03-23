//! `RayView` client — entry point for the UC-007 client module.
//!
//! Submodules implement CLI configuration, QUIC connection setup, and the
//! receive-decode loop.  [`client_main`](super) wires them together.

#[cfg(target_os = "macos")]
mod config;
#[cfg(target_os = "macos")]
mod connect;
#[cfg(target_os = "macos")]
mod decode_dispatch;
#[cfg(target_os = "macos")]
mod receive;
#[cfg(all(test, target_os = "macos"))]
pub(crate) mod test_helper;
#[cfg(all(test, target_os = "macos"))]
mod tests;

#[cfg(target_os = "macos")]
pub use config::{ClientArgs, ClientConfig};
#[cfg(target_os = "macos")]
pub use connect::connect;
