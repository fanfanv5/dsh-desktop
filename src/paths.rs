//! Application data directory resolution.

use std::path::PathBuf;

pub struct AppPaths {
    pub runtime: PathBuf,
    pub logs: PathBuf,
}

impl AppPaths {
    /// Resolve the app data directory: %LOCALAPPDATA%\DSHDesktop.
    pub fn resolve() -> Self {
        let base = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        let root = base.join("DSHDesktop");
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
