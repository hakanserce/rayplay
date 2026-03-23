//! Filesystem persistence for server TLS certificates (UC-016).
//!
//! Stores one DER-encoded certificate per host so the client can reconnect
//! without re-pairing. This file is excluded from coverage because it
//! performs OS-level I/O.

use std::path::{Path, PathBuf};

use crate::platform_dirs;
use crate::wire::TransportError;

/// Sanitises a host string for use as a filename component.
///
/// Replaces colons (IPv6) with dashes and removes brackets.
fn sanitise_host(host: &str) -> String {
    host.replace(':', "-").replace(['[', ']'], "")
}

/// Returns the certificate filename for a given host and port.
fn cert_filename(host: &str, port: u16) -> String {
    format!("{}_{port}.der", sanitise_host(host))
}

/// Returns the path for a specific server's certificate under `base_dir`.
fn cert_path_in(base_dir: &Path, host: &str, port: u16) -> PathBuf {
    base_dir.join("certs").join(cert_filename(host, port))
}

/// Writes a certificate to the given path, creating parent directories.
fn write_cert(path: &Path, cert_der: &[u8]) -> Result<(), TransportError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            TransportError::StorageError(format!(
                "failed to create directory {}: {e}",
                parent.display()
            ))
        })?;
    }
    std::fs::write(path, cert_der).map_err(|e| {
        TransportError::StorageError(format!("failed to write {}: {e}", path.display()))
    })?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).map_err(|e| {
            TransportError::StorageError(format!(
                "failed to set permissions on {}: {e}",
                path.display()
            ))
        })?;
    }

    Ok(())
}

/// Reads a certificate from the given path, returning `None` if it doesn't exist.
fn read_cert(path: &Path) -> Result<Option<Vec<u8>>, TransportError> {
    if !path.exists() {
        return Ok(None);
    }
    let bytes = std::fs::read(path).map_err(|e| {
        TransportError::StorageError(format!("failed to read {}: {e}", path.display()))
    })?;
    Ok(Some(bytes))
}

/// Saves a server's DER-encoded TLS certificate to disk.
///
/// Creates parent directories if they do not exist.
///
/// # Errors
///
/// Returns [`TransportError::StorageError`] on I/O errors.
pub fn save_server_cert(host: &str, port: u16, cert_der: &[u8]) -> Result<(), TransportError> {
    let base = platform_dirs::config_dir()?;
    let path = cert_path_in(&base, host, port);
    write_cert(&path, cert_der)
}

/// Loads a previously saved server certificate.
///
/// Returns `None` if no certificate has been saved for this host.
///
/// # Errors
///
/// Returns [`TransportError::StorageError`] on I/O errors.
pub fn load_server_cert(host: &str, port: u16) -> Result<Option<Vec<u8>>, TransportError> {
    let base = platform_dirs::config_dir()?;
    let path = cert_path_in(&base, host, port);
    read_cert(&path)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── sanitise_host ────────────────────────────────────────────────────

    #[test]
    fn test_sanitise_host_ipv4_unchanged() {
        assert_eq!(sanitise_host("192.168.1.10"), "192.168.1.10");
    }

    #[test]
    fn test_sanitise_host_ipv6_replaces_colons() {
        assert_eq!(sanitise_host("::1"), "--1");
    }

    #[test]
    fn test_sanitise_host_ipv6_full_address() {
        assert_eq!(sanitise_host("2001:db8::1"), "2001-db8--1");
    }

    #[test]
    fn test_sanitise_host_ipv6_brackets_removed() {
        assert_eq!(sanitise_host("[::1]"), "--1");
    }

    #[test]
    fn test_sanitise_host_hostname_unchanged() {
        assert_eq!(sanitise_host("my-server.local"), "my-server.local");
    }

    #[test]
    fn test_sanitise_host_empty_string() {
        assert_eq!(sanitise_host(""), "");
    }

    // ── cert_filename ────────────────────────────────────────────────────

    #[test]
    fn test_cert_filename_ipv4() {
        assert_eq!(cert_filename("192.168.1.10", 5000), "192.168.1.10_5000.der");
    }

    #[test]
    fn test_cert_filename_ipv6() {
        assert_eq!(cert_filename("::1", 5000), "--1_5000.der");
    }

    #[test]
    fn test_cert_filename_different_ports() {
        assert_ne!(cert_filename("host", 5000), cert_filename("host", 6000));
    }

    // ── cert_path_in ─────────────────────────────────────────────────────

    #[test]
    fn test_cert_path_in_lives_under_certs_dir() {
        let path = cert_path_in(Path::new("/base"), "host", 5000);
        assert_eq!(path, PathBuf::from("/base/certs/host_5000.der"));
    }

    #[test]
    fn test_cert_path_in_ipv6_sanitised() {
        let path = cert_path_in(Path::new("/base"), "::1", 5000);
        assert_eq!(path, PathBuf::from("/base/certs/--1_5000.der"));
    }

    // ── write_cert / read_cert round-trip (no env vars) ──────────────────

    #[test]
    fn test_write_and_read_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = cert_path_in(dir.path(), "10.0.0.1", 5000);
        write_cert(&path, b"test-certificate-data").unwrap();
        let loaded = read_cert(&path).unwrap();
        assert_eq!(loaded.as_deref(), Some(b"test-certificate-data".as_slice()));
    }

    #[test]
    fn test_read_nonexistent_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let path = cert_path_in(dir.path(), "99.99.99.99", 9999);
        let loaded = read_cert(&path).unwrap();
        assert!(loaded.is_none());
    }

    #[test]
    fn test_write_overwrites_existing() {
        let dir = tempfile::tempdir().unwrap();
        let path = cert_path_in(dir.path(), "10.0.0.1", 5000);
        write_cert(&path, b"old-cert").unwrap();
        write_cert(&path, b"new-cert").unwrap();
        let loaded = read_cert(&path).unwrap().unwrap();
        assert_eq!(loaded, b"new-cert");
    }

    #[test]
    fn test_different_hosts_stored_separately() {
        let dir = tempfile::tempdir().unwrap();
        let path_a = cert_path_in(dir.path(), "host-a", 5000);
        let path_b = cert_path_in(dir.path(), "host-b", 5000);
        write_cert(&path_a, b"cert-a").unwrap();
        write_cert(&path_b, b"cert-b").unwrap();
        assert_eq!(read_cert(&path_a).unwrap().unwrap(), b"cert-a");
        assert_eq!(read_cert(&path_b).unwrap().unwrap(), b"cert-b");
    }

    #[test]
    fn test_different_ports_stored_separately() {
        let dir = tempfile::tempdir().unwrap();
        let path_5000 = cert_path_in(dir.path(), "host", 5000);
        let path_6000 = cert_path_in(dir.path(), "host", 6000);
        write_cert(&path_5000, b"cert-5000").unwrap();
        write_cert(&path_6000, b"cert-6000").unwrap();
        assert_eq!(read_cert(&path_5000).unwrap().unwrap(), b"cert-5000");
        assert_eq!(read_cert(&path_6000).unwrap().unwrap(), b"cert-6000");
    }

    #[test]
    fn test_write_empty_cert_is_allowed() {
        let dir = tempfile::tempdir().unwrap();
        let path = cert_path_in(dir.path(), "host", 5000);
        write_cert(&path, b"").unwrap();
        let loaded = read_cert(&path).unwrap().unwrap();
        assert!(loaded.is_empty());
    }

    #[test]
    fn test_ipv6_host_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = cert_path_in(dir.path(), "2001:db8::1", 5000);
        write_cert(&path, b"ipv6-cert").unwrap();
        let loaded = read_cert(&path).unwrap().unwrap();
        assert_eq!(loaded, b"ipv6-cert");
    }

    #[cfg(unix)]
    #[test]
    fn test_write_sets_mode_0600() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let path = cert_path_in(dir.path(), "host", 5000);
        write_cert(&path, b"secret").unwrap();
        let perms = std::fs::metadata(&path).unwrap().permissions();
        assert_eq!(perms.mode() & 0o777, 0o600);
    }

    #[test]
    fn test_write_creates_parent_directories() {
        let dir = tempfile::tempdir().unwrap();
        let certs_dir = dir.path().join("certs");
        assert!(!certs_dir.exists());
        let path = cert_path_in(dir.path(), "host", 5000);
        write_cert(&path, b"data").unwrap();
        assert!(certs_dir.exists());
    }

    #[test]
    fn test_large_cert_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = cert_path_in(dir.path(), "host", 5000);
        let large_cert = vec![0xAB; 8192];
        write_cert(&path, &large_cert).unwrap();
        let loaded = read_cert(&path).unwrap().unwrap();
        assert_eq!(loaded, large_cert);
    }
}
