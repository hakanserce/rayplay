//! `RayView` client — entry point for the UC-007 client module.
//!
//! Submodules implement CLI configuration, QUIC connection setup, and the
//! receive-decode loop.  [`client_main`](super) wires them together.

#[cfg(any(target_os = "macos", test))]
mod config;
#[cfg(any(target_os = "macos", test))]
mod connect;
#[cfg(any(target_os = "macos", test))]
mod decode_dispatch;
#[cfg(any(target_os = "macos", test))]
mod receive;
#[cfg(test)]
pub(crate) mod test_helper;
#[cfg(test)]
mod tests;

#[cfg(any(target_os = "macos", test))]
pub use config::{ClientArgs, ClientConfig};
#[cfg(target_os = "macos")]
pub use connect::connect;
