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
        if attempt > 0 {
            // The dir holds only junctions that dsh's boot re-creates
            // (healProfilesModuleFallback); wiping it between attempts forces
            // a clean rebuild instead of retrying against a corrupted link set.
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
            Err(e) => last_err = Some(e),
        }
    }
    Err(last_err.unwrap_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::Other, "dsh web failed to start after retries")
    }))
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
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    if let Ok(out) = cmd.output() {
        let _ = f.write_all(&out.stdout);
        let _ = f.write_all(&out.stderr);
    }
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
    let mut cmd = Command::new(node);
    cmd.arg(bin_js)
        .arg("web")
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
}
