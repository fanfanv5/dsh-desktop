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
        let node = find_node_unix()?;
        let npm = find_npm_unix(&node)?;
        Some(NodeEnv { node, npm_cli: npm })
    }
}

/// Find node on Unix: PATH first, then the common install locations that a
/// GUI launch (Finder / .app bundle) can't see because its PATH is only
/// /usr/bin:/bin:/usr/sbin:/sbin. Covers Homebrew (arm + intel), the macOS
/// pkg installer, Volta, and nvm (newest installed version).
#[cfg(not(windows))]
fn find_node_unix() -> Option<PathBuf> {
    if let Some(node) = find_on_path("node") {
        return Some(node);
    }
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let mut candidates: Vec<PathBuf> = vec![
        PathBuf::from("/opt/homebrew/bin"), // Homebrew on Apple Silicon
        PathBuf::from("/usr/local/bin"),    // Homebrew on Intel / nodejs pkg installer
    ];
    if let Some(home) = &home {
        candidates.push(home.join(".volta").join("bin"));
        // nvm keeps one dir per version; pick the lexicographically largest
        // (v20.10.0 > v9.11.2 for real-world version strings of the same era).
        let nvm = home.join(".nvm").join("versions").join("node");
        let mut versions: Vec<_> = std::fs::read_dir(&nvm)
            .map(|rd| rd.flatten().map(|e| e.file_name()).collect())
            .unwrap_or_default();
        versions.sort();
        for v in versions.iter().rev() {
            candidates.push(nvm.join(v).join("bin"));
        }
    }
    for dir in candidates {
        let candidate = dir.join("node");
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Find npm next to a resolved node, falling back to a PATH lookup. npm
/// usually sits in the same dir as node (Homebrew, Volta, nvm all symlink
/// both), so this survives GUI launches with a stripped PATH.
#[cfg(not(windows))]
fn find_npm_unix(node: &std::path::Path) -> Option<PathBuf> {
    if let Some(dir) = node.parent() {
        let beside = dir.join("npm");
        if beside.is_file() {
            return Some(beside);
        }
    }
    find_on_path("npm")
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
