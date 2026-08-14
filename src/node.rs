//! Node.js runtime detection (from PATH).

use std::path::PathBuf;

pub struct NodeEnv {
    /// Path to node.exe.
    pub node: PathBuf,
    /// Path to npm's cli entry (npm-cli.js), used to avoid .cmd quoting issues.
    pub npm_cli: PathBuf,
}

/// Find node.exe on PATH, then derive the sibling npm-cli.js location.
pub fn detect_node() -> Option<NodeEnv> {
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

/// `std::fs::canonicalize` returns a verbatim path on Windows
/// (leading backslash-backslash-question-backslash prefix), which node.js
/// rejects when it is the entry script (EISDIR lstat 'C:'). Strip it back
/// to a normal path before handing it to node.
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
