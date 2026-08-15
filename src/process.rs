//! Child process management: spawn dsh web, parse the URL it prints, and kill
//! the whole process tree on close (Job Object on Windows, process group on Unix).

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::RecvTimeoutError;
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[cfg(windows)]
use windows::Win32::Foundation::{CloseHandle, HANDLE};
#[cfg(windows)]
use windows::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
    SetInformationJobObject, TerminateJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};
#[cfg(windows)]
use windows::Win32::System::Threading::{OpenProcess, PROCESS_SET_QUOTA, PROCESS_TERMINATE};

use crate::events::{Proxy, UiEvent};

/// Prevent a console window flashing when spawning node (Windows).
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

/// Shared bookkeeping for the managed child, readable from the UI thread at cleanup time.
pub struct ProcessState {
    pub pid: Mutex<Option<u32>>,
    pub assigned: AtomicBool,
}

impl ProcessState {
    pub fn new() -> Self {
        ProcessState { pid: Mutex::new(None), assigned: AtomicBool::new(false) }
    }
}

/// A handle that can kill the whole dsh process tree.
///
/// Windows: a Job Object configured with KILL_ON_JOB_CLOSE.
/// Unix: a marker; the child is a process-group leader and is killed by group id.
#[cfg(windows)]
pub struct Job {
    handle: HANDLE,
}

#[cfg(not(windows))]
pub struct Job;

// A job object handle is an opaque kernel handle; using it from multiple
// threads (assign, terminate) is safe, and Arc guarantees the handle outlives
// every user before CloseHandle runs in Drop.
#[cfg(windows)]
unsafe impl Send for Job {}
#[cfg(windows)]
unsafe impl Sync for Job {}

impl Job {
    pub fn create() -> Option<Job> {
        #[cfg(windows)]
        {
            unsafe {
                let handle = CreateJobObjectW(None, None).ok()?;
                let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
                info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                let size = std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32;
                if SetInformationJobObject(
                    handle,
                    JobObjectExtendedLimitInformation,
                    &info as *const _ as *const _,
                    size,
                )
                .is_err()
                {
                    let _ = CloseHandle(handle);
                    return None;
                }
                Some(Job { handle })
            }
        }
        #[cfg(not(windows))]
        {
            Some(Job)
        }
    }

    /// Register a spawned process. Windows: assign it to the job object (returns
    /// false if it already belongs to another job). Unix: no-op (the process
    /// group was already set at spawn time).
    pub fn assign(&self, pid: u32) -> bool {
        #[cfg(windows)]
        {
            unsafe {
                let proc = match OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, false, pid) {
                    Ok(p) => p,
                    Err(_) => return false,
                };
                let ok = AssignProcessToJobObject(self.handle, proc).is_ok();
                let _ = CloseHandle(proc);
                ok
            }
        }
        #[cfg(not(windows))]
        {
            let _ = pid;
            true
        }
    }

    /// Kill the whole process tree. Windows: terminate the job. Unix: kill the
    /// process group the child leads.
    pub fn kill_all(&self, pid: u32) {
        #[cfg(windows)]
        {
            let _ = pid; // the job object covers the whole tree; pid is unused here
            unsafe {
                let _ = TerminateJobObject(self.handle, 1);
            }
        }
        #[cfg(not(windows))]
        {
            unsafe {
                let _ = libc::kill(-(pid as i32), libc::SIGKILL);
            }
        }
    }
}

#[cfg(windows)]
impl Drop for Job {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.handle);
        }
    }
}

/// Spawn dsh web, waiting (with retries) for it to print its URL.
///
/// The first launch right after a fresh install can transiently fail while dsh
/// sets up its profile (junctions, caches, etc.); re-spawn automatically so the
/// user never has to close and reopen the app.
pub fn spawn_dsh(
    node: &Path,
    bin_js: &Path,
    log_dir: &Path,
    profile_fallback: &Path,
    proxy: Proxy,
    job: &Job,
    state: &ProcessState,
) -> std::io::Result<u32> {
    const MAX_ATTEMPTS: u32 = 6;
    let mut last_err: Option<std::io::Error> = None;
    for attempt in 0..MAX_ATTEMPTS {
        // Surface retry progress: without this the init screen sits on a
        // static "Starting DSH…" for minutes while attempts churn.
        let _ = proxy.send_event(UiEvent::Status {
            title: "Starting DSH…".to_string(),
            detail: format!("启动尝试 {}/{}，失败详情见 logs/dsh-web.err.log", attempt + 1, MAX_ATTEMPTS),
            actions: vec![],
        });
        if attempt > 0 {
            // The dir holds only links that dsh's boot re-creates
            // (healProfilesModuleFallback) — wiping it between attempts forces
            // a clean rebuild instead of retrying against a corrupted link
            // set. Windows-only: there the links are junctions and the heal
            // always runs. On Unix dsh only re-links after a boot that gets
            // past the loader phase, so wiping here can leave the profile
            // unresolvable for every later attempt (observed: 195 symlinks
            // gone, all retries failing with ERR_UNSUPPORTED_DIR_IMPORT).
            #[cfg(windows)]
            let _ = std::fs::remove_dir_all(profile_fallback);
            // Freshly-created profile junctions can take a while to become
            // visible to a new node process on Windows; back off generously.
            std::thread::sleep(Duration::from_millis(8000));
        }
        // First attempt waits long: right after a fresh install, antivirus
        // scanning can make the first boot take minutes, and that slowdown is
        // not a failure. Retries wait short: a crashed process still fails
        // fast via stdout EOF (Disconnected), so the cap only bounds hung boots.
        let wait = if attempt == 0 { Duration::from_secs(180) } else { Duration::from_secs(30) };
        log_spawn_context(log_dir, profile_fallback, attempt);
        match spawn_once(node, bin_js, log_dir, proxy.clone(), job, state, wait) {
            Ok(pid) => return Ok(pid),
            Err(e) => {
                last_err = Some(e);
                // dsh's loader rolls back the whole boot when a single plugin
                // entry fails to import — there is no built-in skip. Disable
                // the offending entry in the user's patch layer so the next
                // attempt boots without it, and say so on the init screen.
                if let Some(id) = disable_failed_loader_entry(log_dir, profile_fallback) {
                    let _ = proxy.send_event(UiEvent::Status {
                        title: "跳过故障插件，正在重试…".to_string(),
                        detail: format!(
                            "插件 {} 加载失败，已在 cordis.patch.yml 中自动禁用。修复插件后删掉那一行的 disabled 即可恢复。",
                            id
                        ),
                        actions: vec![],
                    });
                }
            }
        }
    }
    Err(last_err.unwrap_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::Other, "dsh web failed to start after retries")
    }))
}

/// Auto-recover from a fatal loader-entry failure. dsh's loader rolls back
/// the WHOLE boot when a single plugin entry fails to import (no built-in
/// skip), so one broken plugin bricks the app. Here we disable the offending
/// entry in the user's cordis.patch.yml (profile level first, then home
/// level) so the next spawn attempt boots without it. Returns the entry id.
/// Capped at 5 entries per process so a systemic failure never guts the
/// whole config silently.
fn disable_failed_loader_entry(log_dir: &Path, profile_fallback: &Path) -> Option<String> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static DISABLED: AtomicUsize = AtomicUsize::new(0);
    if DISABLED.load(Ordering::Relaxed) >= 5 {
        return None;
    }
    let err_text = read_tail(&log_dir.join("dsh-web.err.log"), 128 * 1024)?;
    // The loader's fatal message, e.g.
    // "failed to import loader entry tool-ocr (/path/to/plugin): ..."
    const MARKER: &str = "failed to import loader entry ";
    let pos = err_text.rfind(MARKER)?;
    let id: String = err_text[pos + MARKER.len()..]
        .chars()
        .take_while(|c| !c.is_whitespace() && *c != '(' && *c != ':')
        .collect();
    if id.is_empty() {
        return None;
    }
    let dsh_home = profile_fallback.parent()?.parent()?.to_path_buf();
    let candidates = [
        dsh_home.join("profiles").join("web").join("cordis.patch.yml"),
        dsh_home.join("cordis.patch.yml"),
    ];
    for path in candidates {
        let Ok(text) = std::fs::read_to_string(&path) else { continue };
        if let Some(patched) = insert_disabled(&text, &id) {
            if std::fs::write(&path, patched).is_ok() {
                DISABLED.fetch_add(1, Ordering::Relaxed);
                return Some(id);
            }
        } else if text.contains(&format!("id: {}", id)) || text.contains(&format!("id: '{}'", id)) {
            // The entry exists but is already disabled (previous recovery).
            return None;
        }
    }
    None
}

/// Insert `disabled: true` under the patch entry whose `- id:` line matches
/// `id` (plain or quoted), keeping the entry's indentation. Returns None
/// when no matching entry line exists.
fn insert_disabled(text: &str, id: &str) -> Option<String> {
    let lines: Vec<&str> = text.lines().collect();
    let idx = lines.iter().position(|l| {
        let t = l.trim_start();
        t == format!("- id: {id}")
            || t == format!("- id: '{id}'")
            || t == format!("- id: \"{id}\"")
    })?;
    if lines.get(idx + 1).map(|l| l.contains("disabled:")).unwrap_or(false) {
        return None; // already disabled
    }
    let indent = lines[idx].len() - lines[idx].trim_start().len();
    let mut out = String::with_capacity(text.len() + 32);
    for (n, line) in lines.iter().enumerate() {
        out.push_str(line);
        out.push('\n');
        if n == idx {
            out.push_str(&" ".repeat(indent + 2));
            out.push_str("disabled: true # auto-disabled by DSH Desktop: plugin failed to load\n");
        }
    }
    Some(out)
}

/// Read at most the last `max` bytes of a file (as UTF-8, lossy on cut
/// boundaries is fine for our grep-style scanning).
fn read_tail(path: &Path, max: usize) -> Option<String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = std::fs::File::open(path).ok()?;
    let len = f.metadata().ok()?.len() as usize;
    if len > max {
        f.seek(SeekFrom::Start((len - max) as u64)).ok()?;
    }
    let mut buf = Vec::new();
    f.read_to_end(&mut buf).ok()?;
    Some(String::from_utf8_lossy(&buf).into_owned())
}

/// Append a per-attempt context snapshot (cwd, environment, junction audit) to
/// logs/spawn-context.log. Boot failures have proven sensitive to how the app
/// itself was launched (installer postinstall vs Explorer vs terminal); this
/// captures the exact conditions of every spawn for post-mortem comparison.
fn log_spawn_context(log_dir: &Path, profile_fallback: &Path, attempt: u32) {
    use std::io::Write;
    let path = log_dir.join("spawn-context.log");
    let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) else {
        return;
    };
    let _ = writeln!(f, "=== spawn attempt {} at unix {} ===", attempt, std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0));
    let _ = writeln!(f, "cwd: {:?}", std::env::current_dir());
    let mut vars: Vec<(String, String)> = std::env::vars().map(|(k, v)| (k, v)).collect();
    vars.sort();
    for (k, v) in vars {
        let _ = writeln!(f, "{}={}", k, v);
    }
    // Junction audit: count links and verify a sample resolves to a real dir.
    let scope = profile_fallback.join("@deepseek-ai");
    let names: Vec<_> = std::fs::read_dir(&scope)
        .map(|rd| rd.flatten().map(|e| e.file_name()).collect())
        .unwrap_or_default();
    let sample = scope.join("cordis-plugin-timer");
    let sample_ok = std::fs::symlink_metadata(&sample)
        .and_then(|m| Ok(m.file_type().is_symlink()))
        .map(|l| if l { sample.join("package.json").is_file() } else { false })
        .unwrap_or(false);
    let _ = writeln!(f, "junction-audit: count={} timer-link+target={}", names.len(), sample_ok);
    // Token + ACL view of the exact package dir the loader must read through.
    // Windows-only: it shells out through cmd; other platforms have no ACL
    // story and no cmd to run.
    #[cfg(windows)]
    {
        let target = profile_fallback
            .join("@deepseek-ai")
            .join("cordis-plugin-timer")
            .to_string_lossy()
            .into_owned();
        let _ = writeln!(f, "--- whoami ---");
        let script = format!("whoami /user & whoami /groups & icacls \"{}\"", target);
        let mut cmd = std::process::Command::new("cmd");
        cmd.arg("/c").arg(script);
        // Hide the console: this app is GUI-subsystem, so a bare cmd child would
        // flash a window on every spawn attempt.
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        if let Ok(out) = cmd.output() {
            let _ = f.write_all(&out.stdout);
            let _ = f.write_all(&out.stderr);
        }
    }
}

/// Unix: wrap the `node <bin> web --port 0` launch in a tiny /bin/sh watchdog
/// so the child dies with this app even when we are SIGTERM/SIGKILLed (or
/// crash) and the cleanup path never runs — Unix has no Job Object
/// equivalent. The wrapper polls its parent ($PPID is this app) once a
/// second. stdout/stderr still flow through unchanged.
#[cfg(unix)]
pub fn watchdog_command(node: &Path, bin_js: &Path) -> Command {
    let node = node.to_string_lossy().replace('\'', r"'\''");
    let bin = bin_js.to_string_lossy().replace('\'', r"'\''");
    let mut cmd = Command::new("/bin/sh");
    cmd.arg("-c").arg(format!(
        "'{node}' '{bin}' web --port 0 & C=$!; trap 'kill $C' TERM INT; while kill -0 $PPID 2>/dev/null && kill -0 $C 2>/dev/null; do sleep 1; done; kill $C 2>/dev/null"
    ));
    cmd
}

/// Spawn dsh web once and wait (up to `wait` total) for it to print its URL.
///
/// "Still starting slowly" and "exited" are distinguished: stdout EOF means the
/// process died before printing its URL (fail fast, caller retries), while a
/// live-but-silent process keeps getting time until the deadline — a fresh
/// install can boot for minutes under antivirus scanning.
fn spawn_once(
    node: &Path,
    bin_js: &Path,
    log_dir: &Path,
    proxy: Proxy,
    job: &Job,
    state: &ProcessState,
    wait: Duration,
) -> std::io::Result<u32> {
    #[cfg(unix)]
    let mut cmd = watchdog_command(node, bin_js);
    #[cfg(not(unix))]
    let mut cmd = Command::new(node);
    #[cfg(not(unix))]
    cmd.arg(bin_js);
    cmd.arg("web")
        .arg("--port")
        .arg("0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // New process group so we can kill the whole tree with kill(-pgid).
        cmd.process_group(0);
    }

    let mut child = cmd.spawn()?;

    let pid = child.id();
    let stdout = child.stdout.take().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::Other, "failed to capture dsh stdout")
    })?;
    let stderr = child.stderr.take().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::Other, "failed to capture dsh stderr")
    })?;
    // NOTE: dropping child here does NOT terminate the process.

    *state.pid.lock().unwrap() = Some(pid);
    state.assigned.store(job.assign(pid), Ordering::Relaxed);

    let (ready_tx, ready_rx) = std::sync::mpsc::channel();

    // stdout reader: parse the printed URL, keep draining to a log.
    let out_log = log_dir.join("dsh-web.out.log");
    let out_proxy = proxy;
    std::thread::spawn(move || {
        let start = Instant::now();
        let mut log = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&out_log)
            .ok();
        if let Some(f) = log.as_mut() {
            let _ = writeln!(f, "--- dsh web spawn ---");
        }
        let mut ready = false;
        for line in BufReader::new(stdout).lines().flatten() {
            if let Some(f) = log.as_mut() {
                // Offset from spawn time makes slow-boot vs fast-boot obvious.
                let _ = writeln!(f, "t+{:.1}s {}", start.elapsed().as_secs_f32(), line);
            }
            if !ready {
                if let Some(url) = parse_url(&line) {
                    ready = true;
                    let _ = ready_tx.send(());
                    let _ = out_proxy.send_event(UiEvent::Ready { url });
                }
            }
        }
        // EOF: dropping ready_tx signals "exited before ready" to the waiter.
    });

    // stderr reader: drain to a log for diagnostics (append, so a failed
    // launch's error survives a later successful one).
    let err_log = log_dir.join("dsh-web.err.log");
    std::thread::spawn(move || {
        let start = Instant::now();
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&err_log)
        {
            let _ = writeln!(f, "--- dsh web spawn ---");
            for line in BufReader::new(stderr).lines().flatten() {
                let _ = writeln!(f, "t+{:.1}s {}", start.elapsed().as_secs_f32(), line);
            }
        }
    });

    // Wait for dsh to announce its URL, in slices so a still-alive process can
    // use the whole budget while an exited one (EOF drops ready_tx) fails fast.
    let deadline = Instant::now() + wait;
    loop {
        let slice = deadline.saturating_duration_since(Instant::now()).min(Duration::from_secs(5));
        match ready_rx.recv_timeout(slice) {
            Ok(()) => return Ok(pid),
            Err(RecvTimeoutError::Disconnected) => {
                job.kill_all(pid);
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "dsh web exited before printing its URL",
                ));
            }
            Err(RecvTimeoutError::Timeout) => {
                if Instant::now() >= deadline {
                    job.kill_all(pid);
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        format!("dsh web did not print its URL within {} seconds", wait.as_secs()),
                    ));
                }
                // Process is still alive but silent: probably a slow first
                // boot (antivirus scan); keep waiting until the deadline.
            }
        }
    }
}

/// Extract the loopback URL from dsh's startup line
/// (for example: dsh web: http://127.0.0.1:62958).
pub fn parse_url(line: &str) -> Option<String> {
    let marker = "dsh web: ";
    let idx = line.find(marker)?;
    let rest = &line[idx + marker.len()..];
    let url = rest.split_whitespace().next()?;
    if url.starts_with("http://") {
        Some(url.to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::parse_url;

    #[test]
    fn parses_loopback_url() {
        assert_eq!(
            parse_url("dsh web: http://127.0.0.1:62958"),
            Some("http://127.0.0.1:62958".to_string())
        );
    }

    #[test]
    fn takes_only_the_url_token() {
        assert_eq!(
            parse_url("info dsh web: http://127.0.0.1:1 extra words"),
            Some("http://127.0.0.1:1".to_string())
        );
    }

    #[test]
    fn missing_marker_is_none() {
        assert_eq!(parse_url("server listening on port 1234"), None);
        assert_eq!(parse_url(""), None);
    }

    #[test]
    fn non_http_url_is_none() {
        assert_eq!(parse_url("dsh web: https://127.0.0.1:62958"), None);
        assert_eq!(parse_url("dsh web: "), None);
    }

    use super::insert_disabled;

    const PATCH: &str = "other:\n  - id: mcp-a\n    name: x\n- insert:\n    - id: tool-ocr\n      name: '/p/dsh-tool-ocr'\n    - id: mcp-b\n      name: y\n";

    #[test]
    fn insert_disabled_targets_matching_entry() {
        let out = insert_disabled(PATCH, "tool-ocr").unwrap();
        let lines: Vec<&str> = out.lines().collect();
        let i = lines.iter().position(|l| l.trim_start() == "- id: tool-ocr").unwrap();
        assert_eq!(lines[i + 1].trim(), "disabled: true # auto-disabled by DSH Desktop: plugin failed to load");
        // indent follows the entry (4 spaces + 2)
        assert!(lines[i + 1].starts_with("      disabled"));
        // siblings untouched
        assert!(out.contains("- id: mcp-b"));
    }

    #[test]
    fn insert_disabled_accepts_quoted_ids() {
        let y = "- insert:\n    - id: 'weird id'\n      name: z\n";
        assert!(insert_disabled(y, "weird id").is_some());
    }

    #[test]
    fn insert_disabled_skips_missing_and_existing() {
        assert!(insert_disabled(PATCH, "no-such-entry").is_none());
        let once = insert_disabled(PATCH, "tool-ocr").unwrap();
        // second run must not double-insert
        assert!(insert_disabled(&once, "tool-ocr").is_none());
    }
}
