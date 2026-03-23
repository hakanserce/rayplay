//! Filesystem persistence for the client's ed25519 signing key (UC-016).
//!
//! Platform-specific storage of `client_key.bin`. This file is excluded from
//! coverage because it performs OS-level I/O.

use std::path::PathBuf;

use ed25519_dalek::SigningKey;

use crate::platform_dirs;
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
/// Returns [`TransportError::StorageError`] if the home directory cannot be
/// determined.
pub fn client_key_path() -> Result<PathBuf, TransportError> {
    let base = platform_dirs::config_dir()?;
    Ok(base.join("client_key.bin"))
}

/// Loads a previously saved ed25519 signing key from the default path.
///
/// Returns `None` if the key file does not exist.
///
/// # Errors
///
/// Returns [`TransportError::StorageError`] on I/O or key-format errors.
pub fn load_client_key() -> Result<Option<SigningKey>, TransportError> {
    let path = client_key_path()?;
    if !path.exists() {
        return Ok(None);
    }
    let bytes = std::fs::read(&path).map_err(|e| TransportError::StorageError(e.to_string()))?;
    let arr: [u8; 32] = bytes.try_into().map_err(|v: Vec<u8>| {
        TransportError::StorageError(format!("expected 32 bytes, got {}", v.len()))
    })?;
    Ok(Some(SigningKey::from_bytes(&arr)))
}

/// Saves an ed25519 signing key to the default path.
///
/// Creates parent directories if they do not exist.
///
/// # Errors
///
/// Returns [`TransportError::StorageError`] on I/O errors.
pub fn save_client_key(key: &SigningKey) -> Result<(), TransportError> {
    let path = client_key_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| TransportError::StorageError(e.to_string()))?;
    }
    std::fs::write(&path, key.to_bytes())
        .map_err(|e| TransportError::StorageError(e.to_string()))?;

    // Set file permissions to 0600 on Unix systems
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| TransportError::StorageError(e.to_string()))?;
    }

    Ok(())
}
