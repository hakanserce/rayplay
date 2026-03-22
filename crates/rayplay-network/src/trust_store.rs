//! Filesystem persistence for the trust database (UC-016).
//!
//! Platform-specific storage of `trusted_clients.json`. This file is
//! excluded from coverage because it performs OS-level I/O.

use std::path::PathBuf;

use rayplay_core::pairing::TrustDatabase;

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
/// Returns [`TransportError::TlsError`] if the home directory cannot be
/// determined.
pub fn trust_db_path() -> Result<PathBuf, TransportError> {
    let base = if cfg!(target_os = "macos") {
        dirs_path_macos()?
    } else if cfg!(target_os = "windows") {
        dirs_path_windows()?
    } else {
        dirs_path_linux()?
    };
    Ok(base.join("trusted_clients.json"))
}

/// Loads the trust database from the default path.
///
/// Returns an empty database if the file does not exist.
///
/// # Errors
///
/// Returns [`TransportError::TlsError`] on I/O or parse errors.
pub fn load_trust_db() -> Result<TrustDatabase, TransportError> {
    let path = trust_db_path()?;
    if !path.exists() {
        return Ok(TrustDatabase::new());
    }
    let json =
        std::fs::read_to_string(&path).map_err(|e| TransportError::TlsError(e.to_string()))?;
    TrustDatabase::from_json(&json).map_err(|e| TransportError::TlsError(e.to_string()))
}

/// Saves the trust database to the default path.
///
/// Creates parent directories if they do not exist.
///
/// # Errors
///
/// Returns [`TransportError::TlsError`] on I/O or serialization errors.
pub fn save_trust_db(db: &TrustDatabase) -> Result<(), TransportError> {
    let path = trust_db_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| TransportError::TlsError(e.to_string()))?;
    }
    let json = db
        .to_json()
        .map_err(|e| TransportError::TlsError(e.to_string()))?;
    std::fs::write(&path, json).map_err(|e| TransportError::TlsError(e.to_string()))
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
