//! CLI arguments and resolved configuration for the `rayview` client (UC-007).

use std::{net::SocketAddr, path::PathBuf};

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

    /// Force software pipeline — skip hardware acceleration even on supported platforms.
    #[arg(long)]
    pub software: bool,
}

/// Resolved client configuration derived from [`ClientArgs`].
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// Socket address of the `RayPlay` host.
    pub server_addr: SocketAddr,
    /// Path to the host's DER-encoded TLS certificate.
    pub cert_path: PathBuf,
    /// Window width in logical pixels.
    pub width: u32,
    /// Window height in logical pixels.
    pub height: u32,
    /// Pipeline mode for video decoding.
    pub pipeline_mode: PipelineMode,
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
        Ok(Self {
            server_addr,
            cert_path,
            width: args.width,
            height: args.height,
            pipeline_mode,
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
mod tests {
    use clap::Parser;

    use super::*;

    fn dummy_args(host: &str) -> ClientArgs {
        ClientArgs {
            host: host.to_string(),
            port: 5000,
            width: 1280,
            height: 720,
            cert: None,
            software: false,
        }
    }

    #[test]
    fn test_client_args_default_port() {
        assert_eq!(ClientArgs::parse_from(["rayview", "127.0.0.1"]).port, 5000);
    }

    #[test]
    fn test_client_args_default_cert_is_none() {
        assert!(
            ClientArgs::parse_from(["rayview", "127.0.0.1"])
                .cert
                .is_none()
        );
    }

    #[test]
    fn test_client_args_default_dimensions() {
        let args = ClientArgs::parse_from(["rayview", "127.0.0.1"]);
        assert_eq!(args.width, 1280);
        assert_eq!(args.height, 720);
    }

    #[test]
    fn test_client_args_custom_port_and_dimensions() {
        let args = ClientArgs::parse_from([
            "rayview", "10.0.0.1", "--port", "6000", "--width", "1920", "--height", "1080",
        ]);
        assert_eq!(args.port, 6000);
        assert_eq!(args.width, 1920);
        assert_eq!(args.height, 1080);
    }

    #[test]
    fn test_from_args_uses_default_cert_when_none_provided() {
        let config = ClientConfig::from_args(&dummy_args("127.0.0.1")).unwrap();
        assert!(config.cert_path.ends_with(".config/rayview/server.der"));
    }

    #[test]
    fn test_from_args_valid_ipv4_builds_socket_addr() {
        let config = ClientConfig::from_args(&dummy_args("192.168.1.10")).unwrap();
        assert_eq!(config.server_addr.ip().to_string(), "192.168.1.10");
        assert_eq!(config.server_addr.port(), 5000);
    }

    #[test]
    fn test_from_args_invalid_host_returns_descriptive_error() {
        // "not-an-ip" is not a valid hostname either, DNS will fail
        let err = ClientConfig::from_args(&dummy_args("not-an-ip")).unwrap_err();
        assert!(err.to_string().contains("failed to resolve host"));
        assert!(err.to_string().contains("not-an-ip"));
    }

    #[test]
    fn test_from_args_localhost_resolves_to_socket_addr() {
        let config = ClientConfig::from_args(&dummy_args("localhost")).unwrap();
        assert_eq!(config.server_addr.port(), 5000);
    }

    #[test]
    fn test_from_args_unresolvable_hostname_returns_error() {
        let err =
            ClientConfig::from_args(&dummy_args("this-host-does-not-exist-xyz-12345.invalid"))
                .unwrap_err();
        assert!(err.to_string().contains("failed to resolve host"));
    }

    #[test]
    fn test_from_args_ipv6_builds_socket_addr() {
        let config = ClientConfig::from_args(&dummy_args("::1")).unwrap();
        assert_eq!(config.server_addr.ip().to_string(), "::1");
        assert_eq!(config.server_addr.port(), 5000);
    }

    #[test]
    fn test_from_args_dimensions_and_cert_forwarded() {
        let mut args = dummy_args("127.0.0.1");
        args.width = 1920;
        args.height = 1080;
        args.cert = Some("/path/to/cert.der".into());
        let config = ClientConfig::from_args(&args).unwrap();
        assert_eq!(config.width, 1920);
        assert_eq!(config.height, 1080);
        assert_eq!(config.cert_path, std::path::Path::new("/path/to/cert.der"));
    }

    #[test]
    fn test_default_cert_path_without_home_falls_back_to_dot() {
        use std::sync::Mutex;
        static LOCK: Mutex<()> = Mutex::new(());
        let _guard = LOCK.lock().unwrap();

        let orig = std::env::var_os("HOME");
        // SAFETY: single-threaded via mutex
        unsafe { std::env::remove_var("HOME") };
        let path = default_cert_path();
        match orig {
            Some(v) => unsafe { std::env::set_var("HOME", v) },
            None => {}
        }
        assert_eq!(path, std::path::Path::new("./.config/rayview/server.der"));
    }

    #[test]
    fn test_load_cert_bytes_reads_file_contents() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cert.der");
        std::fs::write(&path, b"fakecert").unwrap();
        let config = ClientConfig {
            server_addr: "127.0.0.1:5000".parse().unwrap(),
            cert_path: path,
            width: 1280,
            height: 720,
            pipeline_mode: PipelineMode::Auto,
        };
        assert_eq!(config.load_cert_bytes().unwrap(), b"fakecert");
    }

    #[test]
    fn test_load_cert_bytes_missing_file_returns_descriptive_error() {
        let config = ClientConfig {
            server_addr: "127.0.0.1:5000".parse().unwrap(),
            cert_path: "/no/such/file.der".into(),
            width: 1280,
            height: 720,
            pipeline_mode: PipelineMode::Auto,
        };
        let err = config.load_cert_bytes().unwrap_err();
        assert!(
            err.to_string()
                .contains("failed to read server certificate")
        );
        assert!(err.to_string().contains("/no/such/file.der"));
    }

    // ── --software flag ──────────────────────────────────────────────────────

    #[test]
    fn test_client_args_default_software_is_false() {
        assert!(!ClientArgs::parse_from(["rayview", "127.0.0.1"]).software);
    }

    #[test]
    fn test_client_args_software_flag() {
        assert!(ClientArgs::parse_from(["rayview", "127.0.0.1", "--software"]).software);
    }

    #[test]
    fn test_from_args_pipeline_mode_auto_by_default() {
        let config = ClientConfig::from_args(&dummy_args("127.0.0.1")).unwrap();
        assert_eq!(config.pipeline_mode, PipelineMode::Auto);
    }

    #[test]
    fn test_from_args_pipeline_mode_software_when_flag_set() {
        let mut args = dummy_args("127.0.0.1");
        args.software = true;
        let config = ClientConfig::from_args(&args).unwrap();
        assert_eq!(config.pipeline_mode, PipelineMode::Software);
    }
}
