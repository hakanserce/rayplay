//! CLI arguments and resolved configuration for the `rayview` client (UC-007).

use std::{net::SocketAddr, path::PathBuf};

use anyhow::{Context, Result};
use clap::Parser;

/// Command-line arguments for the `rayview` binary.
#[derive(Parser, Debug, Clone)]
#[command(
    name = "rayview",
    about = "RayPlay client viewer",
    long_about = "Connect to a RayPlay host and render the streamed display."
)]
pub struct ClientArgs {
    /// IP address of the host to connect to (e.g., 192.168.1.10).
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
}

/// Returns the default certificate path: `$HOME/.config/rayview/server.der`.
fn default_cert_path() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".config/rayview/server.der")
}

impl ClientConfig {
    /// Builds a [`ClientConfig`] from the parsed CLI arguments.
    ///
    /// # Errors
    ///
    /// Returns an error if `args.host` is not a valid IP address.
    pub fn from_args(args: &ClientArgs) -> Result<Self> {
        let host_ip: std::net::IpAddr = args.host.parse().with_context(|| {
            format!(
                "invalid host address '{}': expected an IP address (e.g. 192.168.1.10)",
                args.host
            )
        })?;
        let cert_path = args.cert.clone().unwrap_or_else(default_cert_path);
        Ok(Self {
            server_addr: SocketAddr::new(host_ip, args.port),
            cert_path,
            width: args.width,
            height: args.height,
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
        let err = ClientConfig::from_args(&dummy_args("not-an-ip")).unwrap_err();
        assert!(err.to_string().contains("invalid host address"));
        assert!(err.to_string().contains("not-an-ip"));
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
    fn test_load_cert_bytes_reads_file_contents() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cert.der");
        std::fs::write(&path, b"fakecert").unwrap();
        let config = ClientConfig {
            server_addr: "127.0.0.1:5000".parse().unwrap(),
            cert_path: path,
            width: 1280,
            height: 720,
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
        };
        let err = config.load_cert_bytes().unwrap_err();
        assert!(
            err.to_string()
                .contains("failed to read server certificate")
        );
        assert!(err.to_string().contains("/no/such/file.der"));
    }
}
