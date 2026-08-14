//! Background controller: detect environment, install/upgrade dsh, spawn the web
//! server, and react to user actions from the initialization screen.

use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;
use std::time::Duration;

use crate::events::{Command, Proxy, UiEvent};
use crate::node::detect_node;
use crate::paths::AppPaths;
use crate::process::{spawn_dsh, Job, ProcessState};
use crate::runtime::Runtime;

const INSTALL_TIMEOUT: Duration = Duration::from_secs(1800);
const VIEW_TIMEOUT: Duration = Duration::from_secs(120);

/// Start the controller thread; returns the command sender for the UI thread.
pub fn start(proxy: Proxy, job: Arc<Job>, state: Arc<ProcessState>) -> Sender<Command> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || run(proxy, rx, job, state));
    tx
}

fn status(proxy: &Proxy, title: &str, detail: &str, actions: &[&str]) {
    let _ = proxy.send_event(UiEvent::Status {
        title: title.to_string(),
        detail: detail.to_string(),
        actions: actions.iter().map(|s| s.to_string()).collect(),
    });
}

/// Block until a user command arrives, or None when the UI thread is gone.
fn wait_command(rx: &Receiver<Command>) -> Option<Command> {
    match rx.recv() {
        Ok(cmd) => Some(cmd),
        Err(_) => None,
    }
}

fn tail(s: &str, max: usize) -> String {
    let s = s.trim();
    if s.len() <= max {
        s.to_string()
    } else {
        let start = s.len() - max;
        format!("…{}", &s[start..])
    }
}

/// Persist an npm invocation's output so failures are diagnosable without
/// needing to copy text out of the UI.
fn log_output(logs: &std::path::Path, name: &str, out: &Option<std::process::Output>) {
    let Some(out) = out else { return };
    let mut text = String::new();
    text.push_str(&format!("exit code: {:?}\n", out.status.code()));
    text.push_str("=== stdout ===\n");
    text.push_str(&String::from_utf8_lossy(&out.stdout));
    text.push_str("\n=== stderr ===\n");
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    let _ = std::fs::write(logs.join(name), text);
}

fn run(proxy: Proxy, rx: Receiver<Command>, job: Arc<Job>, state: Arc<ProcessState>) {
    let paths = AppPaths::resolve();
    let _ = paths.ensure();

    'outer: loop {
        // 1. Detect Node.js.
        status(&proxy, "正在检测 Node.js…", "", &[]);
        let node_env = match detect_node() {
            Some(n) => n,
            None => {
                status(
                    &proxy,
                    "未检测到 Node.js",
                    "DSH 需要 Node.js 才能运行。请先安装 Node.js（建议 LTS），并确保 node 已加入 PATH，然后点击重试。",
                    &["retry"],
                );
                match wait_command(&rx) {
                    Some(Command::Retry) => continue 'outer,
                    _ => return,
                }
            }
        };

        let runtime = Runtime::new(&node_env, paths.runtime.clone());
        let mut installed = runtime.installed_version();

        // 2. Install if missing.
        if !runtime.is_installed() {
            status(&proxy, "首次安装 @deepseek-ai/dsh…", "正在下载并安装，可能需要一两分钟，请稍候。", &[]);
            let result = runtime.install_latest(INSTALL_TIMEOUT);
            log_output(&paths.logs, "install.log", &result);
            // npm's exit code is not a reliable success signal: it can exit
            // non-zero on warnings or a failed postinstall while the package is
            // still functional. Decide by whether bin.js actually landed.
            installed = runtime.installed_version();
            if !runtime.is_installed() {
                let detail = match result {
                    Some(out) => {
                        let err = String::from_utf8_lossy(&out.stderr).into_owned();
                        let out_text = String::from_utf8_lossy(&out.stdout).into_owned();
                        let text = if err.trim().is_empty() { out_text } else { err };
                        format!("npm 安装失败：\n{}", tail(&text, 2000))
                    }
                    None => "安装超时或未完成，请检查网络后重试。".to_string(),
                };
                status(&proxy, "安装失败", &detail, &["retry"]);
                match wait_command(&rx) {
                    Some(Command::Retry) => continue 'outer,
                    _ => return,
                }
            }
        }

        let current = installed.unwrap_or_else(|| "未知".to_string());

        // 3. Check for updates.
        status(&proxy, "正在检测更新…", "", &[]);
        if let Some(latest) = runtime.latest_version(VIEW_TIMEOUT) {
            if latest != current {
                status(
                    &proxy,
                    "发现新版本",
                    &format!("当前版本：{}，最新版本：{}。", current, latest),
                    &["upgrade", "skip"],
                );
                match wait_command(&rx) {
                    Some(Command::Upgrade) => {
                        status(&proxy, "正在升级…", &format!("正在升级到 {}，请稍候。", latest), &[]);
                        let up = runtime.install_latest(INSTALL_TIMEOUT);
                        log_output(&paths.logs, "upgrade.log", &up);
                        // Fall through and start whatever is now installed.
                    }
                    Some(Command::SkipUpdate) => {}
                    _ => return,
                }
            }
        }

        // 4. Spawn dsh web.
        status(&proxy, "正在启动 DSH…", "", &[]);
        match spawn_dsh(&node_env.node, &runtime.bin_js(), &paths.logs, proxy.clone(), &job, &state) {
            Ok(_) => {
                // The Ready event is delivered by the stdout reader thread; the
                // dsh process keeps running independently. Stay alive so that a
                // later "重试" click (shown if dsh exits unexpectedly) can
                // re-spawn instead of being silently dropped.
                loop {
                    match wait_command(&rx) {
                        Some(Command::Retry) => continue 'outer,
                        _ => return,
                    }
                }
            }
            Err(e) => {
                status(&proxy, "启动失败", &format!("无法启动 dsh web：{}", e), &["retry"]);
                match wait_command(&rx) {
                    Some(Command::Retry) => continue 'outer,
                    _ => return,
                }
            }
        }
    }
}
