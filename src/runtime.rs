//! @deepseek-ai/dsh install / upgrade / version management via npm.

use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

use crate::node::NodeEnv;
use crate::process::Job;
use std::sync::Arc;

pub const PKG: &str = "@deepseek-ai/dsh";

/// Prevent console windows flashing when we shell out to node (Windows).
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

pub struct Runtime {
    pub node: PathBuf,
    pub npm_cli: PathBuf,
    pub runtime_dir: PathBuf,
    /// Children (npm, prewarm node) are assigned here so closing the app
    /// never leaves an orphaned install running.
    job: Arc<Job>,
}

impl Runtime {
    pub fn new(node_env: &NodeEnv, runtime_dir: PathBuf, job: Arc<Job>) -> Self {
        Runtime {
            node: node_env.node.clone(),
            npm_cli: node_env.npm_cli.clone(),
            runtime_dir,
            job,
        }
    }

    fn pkg_json_path(&self) -> PathBuf {
        self.runtime_dir
            .join("node_modules")
            .join("@deepseek-ai")
            .join("dsh")
            .join("package.json")
    }

    /// The dsh launcher entry point once installed.
    pub fn bin_js(&self) -> PathBuf {
        self.runtime_dir
            .join("node_modules")
            .join("@deepseek-ai")
            .join("dsh")
            .join("lib")
            .join("bin.js")
    }

    /// Installed version, or None when dsh is not installed yet.
    pub fn installed_version(&self) -> Option<String> {
        let text = std::fs::read_to_string(self.pkg_json_path()).ok()?;
        extract_json_string(&text, "version")
    }

    /// Whether dsh's launcher entry point is present on disk. This is the
    /// reliable "did the install succeed" signal: npm can exit non-zero on
    /// warnings or a failed postinstall while the package is still functional.
    pub fn is_installed(&self) -> bool {
        self.bin_js().is_file()
    }

    /// Query the latest published version from the npm registry.
    pub fn latest_version(&self, timeout: Duration) -> Option<String> {
        let out = self.npm_capture(
            &["view", "--fetch-retries=5", "--fetch-timeout=120000", PKG, "version"],
            timeout,
        )?;
        if out.status.success() {
            let s = String::from_utf8_lossy(&out.stdout);
            Some(s.trim().to_string())
        } else {
            None
        }
    }

    /// Run a full `dsh web --port 0` boot once and wait for it to print its
    /// URL, then kill it. This settles the ~/.dsh profile junctions AND warms
    /// the module import cache, so the first real launch doesn't race them.
    /// The wait budget is caller-controlled: right after a fresh install,
    /// antivirus scanning can make this boot take minutes, so give it several
    /// minutes there. Best-effort: failures are ignored here and recovered by
    /// the spawn retry loop instead.
    pub fn prewarm(&self, timeout: Duration) {
        use std::io::{BufRead, BufReader};
        use std::sync::mpsc;

        let mut cmd = Command::new(&self.node);
        cmd.arg(self.bin_js()).arg("web").arg("--port").arg("0");
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null());
        let Ok(mut child) = cmd.spawn() else { return };
        // Kill-on-close even if the app is shut down mid-prewarm.
        let _ = self.job.assign(child.id());
        let Some(stdout) = child.stdout.take() else { return };
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().flatten() {
                if line.contains("dsh web: http://") {
                    let _ = tx.send(());
                    break;
                }
            }
        });
        let _ = rx.recv_timeout(timeout);
        let _ = child.kill();
        let _ = child.wait();
    }

    /// Install the latest @deepseek-ai/dsh into the managed runtime dir.
    pub fn install_latest(&self, timeout: Duration) -> Option<Output> {
        // Give npm a package.json anchor so it installs into runtime_dir.
        let anchor = self.runtime_dir.join("package.json");
        if !anchor.exists() {
            let _ = std::fs::write(&anchor, r#"{"name":"dsh-desktop-runtime","private":true}"#);
        }
        let prefix = self.runtime_dir.to_str()?;
        let spec = format!("{}@latest", PKG);
        self.npm_capture(
            &[
                "install",
                "--prefix",
                prefix,
                "--no-save",
                "--no-audit",
                "--no-fund",
                "--loglevel=error",
                "--fetch-retries=5",
                "--fetch-retry-maxtimeout=120000",
                "--fetch-timeout=600000",
                &spec,
            ],
            timeout,
        )
    }

    /// Run an npm subcommand through node + npm-cli.js, capturing output with a timeout.
    fn npm_capture(&self, args: &[&str], timeout: Duration) -> Option<Output> {
        // Windows: node <npm-cli.js> <args> (avoids npm.cmd quoting issues).
        // Unix: npm <args> directly.
        let mut cmd = if cfg!(windows) {
            let mut c = Command::new(&self.node);
            c.arg(&self.npm_cli);
            c
        } else {
            Command::new(&self.npm_cli)
        };
        cmd.args(args);
        // Default to the npmmirror.com mirror: the official registry is slow
        // and frequently connection-reset on this network. DSH_NPM_REGISTRY
        // overrides it without editing .npmrc.
        let registry = std::env::var_os("DSH_NPM_REGISTRY")
            .unwrap_or_else(|| std::ffi::OsString::from("https://registry.npmmirror.com"));
        cmd.env("npm_config_registry", registry);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

        let mut child = cmd.spawn().ok()?;
        // Kill-on-close even if the app is shut down mid-install.
        let _ = self.job.assign(child.id());
        let stdout = child.stdout.take()?;
        let stderr = child.stderr.take()?;

        // Drain output on background threads so a full pipe never deadlocks.
        let out_thread = std::thread::spawn(move || read_all(stdout));
        let err_thread = std::thread::spawn(move || read_all(stderr));

        let start = Instant::now();
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break Some(status),
                Ok(None) => {
                    if start.elapsed() > timeout {
                        let _ = child.kill();
                        let _ = child.wait();
                        break None;
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
                Err(_) => break None,
            }
        };

        let stdout = out_thread.join().unwrap_or_default();
        let stderr = err_thread.join().unwrap_or_default();
        status.map(|status| Output { status, stdout, stderr })
    }
}

fn read_all(mut reader: impl Read) -> Vec<u8> {
    let mut buf = Vec::new();
    let _ = reader.read_to_end(&mut buf);
    buf
}

/// Extract a JSON string field's value from a small JSON document (no serde needed).
fn extract_json_string(text: &str, field: &str) -> Option<String> {
    let needle = format!("\"{}\"", field);
    let idx = text.find(&needle)?;
    let after = &text[idx + needle.len()..];
    let after_colon = after.find(':')? + 1;
    let value = after[after_colon..].trim_start();
    let quote = value.strip_prefix('"')?;
    let end = quote.find('"')?;
    Some(quote[..end].to_string())
}
