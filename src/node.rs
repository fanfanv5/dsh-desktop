//! Node.js runtime detection (from PATH), cross-platform.

use std::path::PathBuf;

pub struct NodeEnv {
    /// Path to the node executable.
    pub node: PathBuf,
    /// npm entry point. On Windows this is npm-cli.js (invoked as
    /// `node <npm-cli.js>`); on Unix it is the `npm` command (invoked directly).
    pub npm_cli: PathBuf,
}

/// Find node on PATH, then derive the npm entry point.
pub fn detect_node() -> Option<NodeEnv> {
    #[cfg(windows)]
    {
        let node = find_on_path("node.exe")?;
        // Resolve symlinks (e.g. nvm-windows' C:\nvm4w\nodejs -> the real version
        // dir), so the sibling npm-cli.js is located correctly.
        let node = std::fs::canonicalize(&node).unwrap_or(node);
        let node = strip_verbatim_prefix(node);
        let dir = node.parent()?;
        let npm_cli = dir.join("node_modules").join("npm").join("bin").join("npm-cli.js");
        // If npm-cli.js is not beside node (unusual layout), fall back to a PATH lookup.
        let npm_cli = if npm_cli.is_file() {
            npm_cli
        } else {
            find_on_path("npm-cli.js")?
        };
        Some(NodeEnv { node, npm_cli })
    }

    #[cfg(not(windows))]
    {
        let node = find_on_path("node")?;
        let npm = find_on_path("npm")?;
        Some(NodeEnv { node, npm_cli: npm })
    }
}

/// `std::fs::canonicalize` returns a verbatim path on Windows
/// (leading backslash-backslash-question-backslash prefix), which node.js
/// rejects when it is the entry script (EISDIR lstat 'C:'). Strip it back
/// to a normal path before handing it to node.
#[cfg(windows)]
fn strip_verbatim_prefix(p: PathBuf) -> PathBuf {
    let s = p.to_string_lossy().into_owned();
    const PREFIX: &str = "\\\\?\\";
    if let Some(rest) = s.strip_prefix(PREFIX) {
        PathBuf::from(rest)
    } else {
        p
    }
}

fn find_on_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}
