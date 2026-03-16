//! `RayView` client — entry point for the UC-007 client module.
//!
//! Submodules implement CLI configuration, QUIC connection setup, and the
//! receive-decode loop.  [`client_main`](super) wires them together.

mod config;
mod connect;
mod receive;
#[cfg(test)]
pub(crate) mod test_helper;
#[cfg(test)]
mod tests;

pub use config::{ClientArgs, ClientConfig};
#[cfg(target_os = "macos")]
pub use connect::connect;
