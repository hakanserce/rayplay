//! Filesystem persistence for the client's ed25519 signing key (UC-016).
//!
//! Platform-specific storage of `client_key.bin`. This file is excluded from
//! coverage because it performs OS-level I/O.

use std::path::PathBuf;

use ed25519_dalek::SigningKey;

use crate::wire::TransportError;

/// Returns the platform-specific path for the client signing key file.
///
/// - **macOS:** `~/Library/Application Support/RayPlay/client_key.bin`
/// - **Windows:** `%APPDATA%\RayPlay\client_key.bin`
/// - **Linux:** `$XDG_CONFIG_HOME/rayplay/client_key.bin`
///   (or `~/.config/rayplay/client_key.bin`)
///
/// # Errors
///
/// Returns [`TransportError::TlsError`] if the home directory cannot be
/// determined.
pub fn client_key_path() -> Result<PathBuf, TransportError> {
    let base = if cfg!(target_os = "macos") {
        dirs_path_macos()?
    } else if cfg!(target_os = "windows") {
        dirs_path_windows()?
    } else {
        dirs_path_linux()?
    };
    Ok(base.join("client_key.bin"))
}

/// Loads a previously saved ed25519 signing key from the default path.
///
/// Returns `None` if the key file does not exist.
///
/// # Errors
///
/// Returns [`TransportError::TlsError`] on I/O or key-format errors.
pub fn load_client_key() -> Result<Option<SigningKey>, TransportError> {
    let path = client_key_path()?;
    if !path.exists() {
        return Ok(None);
    }
    let bytes =
        std::fs::read(&path).map_err(|e| TransportError::TlsError(e.to_string()))?;
    let arr: [u8; 32] = bytes
        .try_into()
        .map_err(|v: Vec<u8>| {
            TransportError::TlsError(format!("expected 32 bytes, got {}", v.len()))
        })?;
    Ok(Some(SigningKey::from_bytes(&arr)))
}

/// Saves an ed25519 signing key to the default path.
///
/// Creates parent directories if they do not exist.
///
/// # Errors
///
/// Returns [`TransportError::TlsError`] on I/O errors.
pub fn save_client_key(key: &SigningKey) -> Result<(), TransportError> {
    let path = client_key_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| TransportError::TlsError(e.to_string()))?;
    }
    std::fs::write(&path, key.to_bytes()).map_err(|e| TransportError::TlsError(e.to_string()))
}

fn dirs_path_macos() -> Result<PathBuf, TransportError> {
    let home = std::env::var("HOME").map_err(|_| {
        TransportError::TlsError("cannot determine home directory".to_string())
    })?;
    Ok(PathBuf::from(home)
        .join("Library")
        .join("Application Support")
        .join("RayPlay"))
}

fn dirs_path_windows() -> Result<PathBuf, TransportError> {
    let appdata = std::env::var("APPDATA").map_err(|_| {
        TransportError::TlsError("cannot determine APPDATA directory".to_string())
    })?;
    Ok(PathBuf::from(appdata).join("RayPlay"))
}

fn dirs_path_linux() -> Result<PathBuf, TransportError> {
    let config = std::env::var("XDG_CONFIG_HOME").or_else(|_| {
        std::env::var("HOME").map(|home| format!("{home}/.config"))
    }).map_err(|_| {
        TransportError::TlsError("cannot determine config directory".to_string())
    })?;
    Ok(PathBuf::from(config).join("rayplay"))
}
