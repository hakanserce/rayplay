//! CLI arguments and resolved configuration for the `rayview` client (UC-007).

use std::{net::SocketAddr, path::PathBuf, time::Duration};

use anyhow::{Context, Result};
use clap::Parser;
use rayplay_video::PipelineMode;

/// Command-line arguments for the `rayview` binary.
#[derive(Parser, Debug, Clone)]
#[command(
    name = "rayview",
    about = "RayPlay client viewer",
    long_about = "Connect to a RayPlay host and render the streamed display."
)]
pub struct ClientArgs {
    /// IP address or hostname of the host to connect to (e.g., 192.168.1.10 or my-host.local).
    pub host: String,

    /// UDP port the host is listening on.
    #[arg(short, long, default_value_t = 5000)]
    pub port: u16,

    /// Window width in pixels.
    #[arg(long, default_value_t = 1280)]
    pub width: u32,

    /// Window height in pixels.
    #[arg(long, default_value_t = 720)]
    pub height: u32,

    /// Path to the host's DER-encoded TLS certificate.
    ///
    /// Defaults to `~/.config/rayview/server.der`.  Required until the
    /// SPAKE2 pairing flow (ADR-007) replaces manual cert distribution.
    #[arg(long)]
    pub cert: Option<PathBuf>,

    /// Pair with the host using a 6-digit PIN (first-time connection).
    ///
    /// When set, the client connects without verifying the server certificate
    /// and performs SPAKE2 PIN-based pairing.  The resulting ed25519 key is
    /// saved locally for subsequent trusted reconnections.
    #[arg(long)]
    pub pair: bool,

    /// Force software pipeline — skip hardware acceleration even on supported platforms.
    #[arg(long)]
    pub software: bool,

    /// Maximum time in seconds to keep reconnecting before giving up (0 = infinite).
    #[arg(long, default_value_t = 30)]
    pub reconnect_timeout: u64,
}

/// Resolved client configuration derived from [`ClientArgs`].
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// Socket address of the `RayPlay` host.
    pub server_addr: SocketAddr,
    /// Path to the host's DER-encoded TLS certificate.
    pub cert_path: PathBuf,
    /// Whether to initiate PIN-based pairing (first-time connection).
    pub pair: bool,
    /// Window width in logical pixels.
    pub width: u32,
    /// Window height in logical pixels.
    pub height: u32,
    /// Pipeline mode for video decoding.
    pub pipeline_mode: PipelineMode,
    /// Maximum reconnect duration (0 = infinite).
    pub reconnect_timeout: Duration,
}

/// Returns the default certificate path: `$HOME/.config/rayview/server.der`.
fn default_cert_path() -> PathBuf {
    std::env::var_os("HOME")
        .map_or_else(|| PathBuf::from("."), PathBuf::from)
        .join(".config/rayview/server.der")
}

/// Resolves a host string (IP or hostname) and port into a [`SocketAddr`].
///
/// Tries a direct IP parse first to avoid a DNS lookup, then falls back to
/// DNS resolution via [`std::net::ToSocketAddrs`].
fn resolve_host(host: &str, port: u16) -> Result<SocketAddr> {
    use std::net::ToSocketAddrs;

    // Fast path: try direct IP parse first (avoids DNS lookup)
    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        return Ok(SocketAddr::new(ip, port));
    }

    // Fallback: DNS resolution
    format!("{host}:{port}")
        .to_socket_addrs()
        .with_context(|| format!("failed to resolve host '{host}'"))?
        .next()
        .with_context(|| format!("no addresses found for host '{host}'"))
}

impl ClientConfig {
    /// Builds a [`ClientConfig`] from the parsed CLI arguments.
    ///
    /// # Errors
    ///
    /// Returns an error if `args.host` cannot be parsed as an IP address or
    /// resolved via DNS.
    pub fn from_args(args: &ClientArgs) -> Result<Self> {
        let server_addr = resolve_host(&args.host, args.port)?;
        let cert_path = args.cert.clone().unwrap_or_else(default_cert_path);
        let pipeline_mode = if args.software {
            PipelineMode::Software
        } else {
            PipelineMode::Auto
        };
        let reconnect_timeout = Duration::from_secs(args.reconnect_timeout);
        Ok(Self {
            server_addr,
            cert_path,
            pair: args.pair,
            width: args.width,
            height: args.height,
            pipeline_mode,
            reconnect_timeout,
        })
    }

    /// Reads the server's TLS certificate bytes from disk.
    ///
    /// # Errors
    ///
    /// Returns an error if the certificate file cannot be read.
    pub fn load_cert_bytes(&self) -> Result<Vec<u8>> {
        std::fs::read(&self.cert_path).with_context(|| {
            format!(
                "failed to read server certificate from '{}'",
                self.cert_path.display()
            )
        })
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests;
