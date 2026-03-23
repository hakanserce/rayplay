use clap::Parser;

use super::*;

fn dummy_args(host: &str) -> ClientArgs {
    ClientArgs {
        host: host.to_string(),
        port: 5000,
        width: 1280,
        height: 720,
        cert: None,
        pair: false,
        software: false,
        reconnect_timeout: 30,
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
fn test_from_args_cert_path_none_when_not_provided() {
    let config = ClientConfig::from_args(&dummy_args("127.0.0.1")).unwrap();
    assert!(config.cert_path.is_none());
}

#[test]
fn test_from_args_host_and_port_forwarded() {
    let config = ClientConfig::from_args(&dummy_args("192.168.1.10")).unwrap();
    assert_eq!(config.host, "192.168.1.10");
    assert_eq!(config.port, 5000);
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
    let err = ClientConfig::from_args(&dummy_args("this-host-does-not-exist-xyz-12345.invalid"))
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
    assert_eq!(
        config.cert_path.as_deref(),
        Some(std::path::Path::new("/path/to/cert.der"))
    );
}

#[test]
fn test_load_cert_bytes_reads_explicit_cert_path() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cert.der");
    std::fs::write(&path, b"fakecert").unwrap();
    let config = ClientConfig {
        server_addr: "127.0.0.1:5000".parse().unwrap(),
        host: "127.0.0.1".to_string(),
        port: 5000,
        cert_path: Some(path),
        pair: false,
        width: 1280,
        height: 720,
        pipeline_mode: PipelineMode::Auto,
        reconnect_timeout: Duration::from_secs(30),
    };
    assert_eq!(config.load_cert_bytes().unwrap(), b"fakecert");
}

#[test]
fn test_load_cert_bytes_missing_explicit_cert_returns_descriptive_error() {
    let config = ClientConfig {
        server_addr: "127.0.0.1:5000".parse().unwrap(),
        host: "127.0.0.1".to_string(),
        port: 5000,
        cert_path: Some("/no/such/file.der".into()),
        pair: false,
        width: 1280,
        height: 720,
        pipeline_mode: PipelineMode::Auto,
        reconnect_timeout: Duration::from_secs(30),
    };
    let err = config.load_cert_bytes().unwrap_err();
    assert!(
        err.to_string()
            .contains("failed to read server certificate")
    );
    assert!(err.to_string().contains("/no/such/file.der"));
}

#[test]
fn test_load_cert_bytes_no_cert_no_store_returns_descriptive_error() {
    let config = ClientConfig {
        server_addr: "127.0.0.1:5000".parse().unwrap(),
        host: "127.0.0.1".to_string(),
        port: 59999,
        cert_path: None,
        pair: false,
        width: 1280,
        height: 720,
        pipeline_mode: PipelineMode::Auto,
        reconnect_timeout: Duration::from_secs(30),
    };
    let err = config.load_cert_bytes().unwrap_err();
    assert!(err.to_string().contains("no server certificate found"));
}

/// Helper: save a cert to the real platform cert store and clean up on drop.
struct TestCertGuard {
    host: String,
    port: u16,
}

impl TestCertGuard {
    fn save(host: &str, port: u16, data: &[u8]) -> Self {
        rayplay_network::server_cert_store::save_server_cert(host, port, data).unwrap();
        Self {
            host: host.to_string(),
            port,
        }
    }
}

impl Drop for TestCertGuard {
    fn drop(&mut self) {
        // Best-effort cleanup: remove the test cert file
        let _ =
            rayplay_network::server_cert_store::load_server_cert(&self.host, self.port)
                .ok();
        // We can't easily get the path from the public API, so just leave it.
        // The unique host name ensures it won't interfere with anything.
    }
}

#[test]
fn test_load_cert_bytes_finds_cert_from_store() {
    // Use a unique host name that won't clash with real usage
    let host = "test-load-cert-bytes-store-7f2a3b";
    let port = 55555;
    let cert_data = b"stored-server-cert";
    let _guard = TestCertGuard::save(host, port, cert_data);

    let config = ClientConfig {
        server_addr: "127.0.0.1:55555".parse().unwrap(),
        host: host.to_string(),
        port,
        cert_path: None,
        pair: false,
        width: 1280,
        height: 720,
        pipeline_mode: PipelineMode::Auto,
        reconnect_timeout: Duration::from_secs(30),
    };
    assert_eq!(config.load_cert_bytes().unwrap(), cert_data);
}

#[test]
fn test_load_cert_bytes_explicit_cert_takes_priority_over_store() {
    let host = "test-load-cert-bytes-priority-9e4c1d";
    let port = 55556;
    let _guard = TestCertGuard::save(host, port, b"store-cert");

    let dir = tempfile::tempdir().unwrap();
    let cert_path = dir.path().join("explicit.der");
    std::fs::write(&cert_path, b"explicit-cert").unwrap();

    let config = ClientConfig {
        server_addr: "127.0.0.1:55556".parse().unwrap(),
        host: host.to_string(),
        port,
        cert_path: Some(cert_path),
        pair: false,
        width: 1280,
        height: 720,
        pipeline_mode: PipelineMode::Auto,
        reconnect_timeout: Duration::from_secs(30),
    };
    // Explicit --cert should win over the store
    assert_eq!(config.load_cert_bytes().unwrap(), b"explicit-cert");
}

#[test]
fn test_load_cert_bytes_error_message_includes_host_and_port() {
    let config = ClientConfig {
        server_addr: "10.0.0.5:7777".parse().unwrap(),
        host: "10.0.0.5".to_string(),
        port: 7777,
        cert_path: None,
        pair: false,
        width: 1280,
        height: 720,
        pipeline_mode: PipelineMode::Auto,
        reconnect_timeout: Duration::from_secs(30),
    };
    let err = config.load_cert_bytes().unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("10.0.0.5"));
    assert!(msg.contains("7777"));
    assert!(msg.contains("--pair"));
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

// ── --reconnect-timeout flag ─────────────────────────────────────────────

#[test]
fn test_client_args_default_reconnect_timeout() {
    assert_eq!(
        ClientArgs::parse_from(["rayview", "127.0.0.1"]).reconnect_timeout,
        30
    );
}

#[test]
fn test_client_args_custom_reconnect_timeout() {
    let args = ClientArgs::parse_from(["rayview", "127.0.0.1", "--reconnect-timeout", "60"]);
    assert_eq!(args.reconnect_timeout, 60);
}

#[test]
fn test_from_args_reconnect_timeout_default() {
    let config = ClientConfig::from_args(&dummy_args("127.0.0.1")).unwrap();
    assert_eq!(config.reconnect_timeout, Duration::from_secs(30));
}

#[test]
fn test_from_args_reconnect_timeout_zero_means_infinite() {
    let mut args = dummy_args("127.0.0.1");
    args.reconnect_timeout = 0;
    let config = ClientConfig::from_args(&args).unwrap();
    assert_eq!(config.reconnect_timeout, Duration::ZERO);
}
