//! Application data directory resolution (cross-platform).

use std::path::PathBuf;

pub struct AppPaths {
    pub runtime: PathBuf,
    pub logs: PathBuf,
}

impl AppPaths {
    /// Resolve the per-user app data directory: <data>/DSHDesktop.
    pub fn resolve() -> Self {
        let root = app_data_root().join("DSHDesktop");
        AppPaths {
            runtime: root.join("runtime"),
            logs: root.join("logs"),
        }
    }

    /// Ensure the required directories exist.
    pub fn ensure(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.runtime)?;
        std::fs::create_dir_all(&self.logs)?;
        Ok(())
    }
}

/// Per-user application data directory (where DSHDesktop lives).
#[cfg(windows)]
fn app_data_root() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .or_else(|| std::env::var_os("APPDATA"))
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir().join("AppData").join("Local"))
}

#[cfg(target_os = "macos")]
fn app_data_root() -> PathBuf {
    home_dir().join("Library").join("Application Support")
}

#[cfg(all(unix, not(target_os = "macos")))]
fn app_data_root() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir().join(".local").join("share"))
}

fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}
