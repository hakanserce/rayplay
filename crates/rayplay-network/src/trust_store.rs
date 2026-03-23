//! Filesystem persistence for the trust database (UC-016).
//!
//! Platform-specific storage of `trusted_clients.json`. This file is
//! excluded from coverage because it performs OS-level I/O.

use std::path::PathBuf;

use rayplay_core::pairing::TrustDatabase;

use crate::platform_dirs;
use crate::wire::TransportError;

/// Returns the platform-specific path for the trust database file.
///
/// - **macOS:** `~/Library/Application Support/RayPlay/trusted_clients.json`
/// - **Windows:** `%APPDATA%\RayPlay\trusted_clients.json`
/// - **Linux:** `$XDG_CONFIG_HOME/rayplay/trusted_clients.json`
///   (or `~/.config/rayplay/trusted_clients.json`)
///
/// # Errors
///
/// Returns [`TransportError::StorageError`] if the home directory cannot be
/// determined.
pub fn trust_db_path() -> Result<PathBuf, TransportError> {
    let base = platform_dirs::config_dir()?;
    Ok(base.join("trusted_clients.json"))
}

/// Loads the trust database from the default path.
///
/// Returns an empty database if the file does not exist.
///
/// # Errors
///
/// Returns [`TransportError::StorageError`] on I/O or parse errors.
pub fn load_trust_db() -> Result<TrustDatabase, TransportError> {
    let path = trust_db_path()?;
    if !path.exists() {
        return Ok(TrustDatabase::new());
    }
    let json = std::fs::read_to_string(&path).map_err(|e| {
        TransportError::StorageError(format!("failed to read {}: {e}", path.display()))
    })?;
    TrustDatabase::from_json(&json).map_err(|e| {
        TransportError::StorageError(format!("failed to parse {}: {e}", path.display()))
    })
}

/// Saves the trust database to the default path.
///
/// Creates parent directories if they do not exist.
///
/// # Errors
///
/// Returns [`TransportError::StorageError`] on I/O or serialization errors.
pub fn save_trust_db(db: &TrustDatabase) -> Result<(), TransportError> {
    let path = trust_db_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            TransportError::StorageError(format!(
                "failed to create directory {}: {e}",
                parent.display()
            ))
        })?;
    }
    let json = db.to_json().map_err(|e| {
        TransportError::StorageError(format!("failed to serialize trust database: {e}"))
    })?;
    std::fs::write(&path, json).map_err(|e| {
        TransportError::StorageError(format!("failed to write {}: {e}", path.display()))
    })?;

    // Set file permissions to 0600 on Unix systems
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).map_err(|e| {
            TransportError::StorageError(format!(
                "failed to set permissions on {}: {e}",
                path.display()
            ))
        })?;
    }

    Ok(())
}
