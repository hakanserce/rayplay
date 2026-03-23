//! Platform-specific configuration directory resolution.
//!
//! Excluded from coverage because it queries OS environment variables.

use crate::wire::TransportError;
use std::path::PathBuf;

/// Returns the platform-specific `RayPlay` configuration directory.
pub(crate) fn config_dir() -> Result<PathBuf, TransportError> {
    if cfg!(target_os = "macos") {
        dirs_macos()
    } else if cfg!(target_os = "windows") {
        dirs_windows()
    } else {
        dirs_linux()
    }
}

fn dirs_macos() -> Result<PathBuf, TransportError> {
    let home = std::env::var("HOME")
        .map_err(|_| TransportError::StorageError("cannot determine home directory".to_string()))?;
    Ok(PathBuf::from(home)
        .join("Library")
        .join("Application Support")
        .join("RayPlay"))
}

fn dirs_windows() -> Result<PathBuf, TransportError> {
    let appdata = std::env::var("APPDATA").map_err(|_| {
        TransportError::StorageError("cannot determine APPDATA directory".to_string())
    })?;
    Ok(PathBuf::from(appdata).join("RayPlay"))
}

fn dirs_linux() -> Result<PathBuf, TransportError> {
    let config = std::env::var("XDG_CONFIG_HOME")
        .or_else(|_| std::env::var("HOME").map(|home| format!("{home}/.config")))
        .map_err(|_| {
            TransportError::StorageError("cannot determine config directory".to_string())
        })?;
    Ok(PathBuf::from(config).join("rayplay"))
}
