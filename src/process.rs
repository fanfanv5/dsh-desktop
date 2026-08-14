//! Child process management: spawn dsh web, parse the URL it prints, and kill
//! the whole process tree on close (Job Object on Windows, process group on Unix).

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

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
    proxy: Proxy,
    job: &Job,
    state: &ProcessState,
) -> std::io::Result<u32> {
    const MAX_ATTEMPTS: u32 = 6;
    let mut last_err: Option<std::io::Error> = None;
    for attempt in 0..MAX_ATTEMPTS {
        if attempt > 0 {
            // Freshly-created profile junctions can take a while to become
            // visible to a new node process on Windows; back off generously.
            std::thread::sleep(std::time::Duration::from_millis(8000));
        }
        match spawn_once(node, bin_js, log_dir, proxy.clone(), job, state) {
            Ok(pid) => return Ok(pid),
            Err(e) => last_err = Some(e),
        }
    }
    Err(last_err.unwrap_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::Other, "dsh web failed to start after retries")
    }))
}

/// Spawn dsh web once and wait (up to 15s) for it to print its URL.
fn spawn_once(
    node: &Path,
    bin_js: &Path,
    log_dir: &Path,
    proxy: Proxy,
    job: &Job,
    state: &ProcessState,
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
                let _ = writeln!(f, "{}", line);
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
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&err_log)
        {
            let _ = writeln!(f, "--- dsh web spawn ---");
            for line in BufReader::new(stderr).lines().flatten() {
                let _ = writeln!(f, "{}", line);
            }
        }
    });

    // Wait for dsh to announce its URL. If it exits first (or takes too long),
    // kill whatever is left and signal the caller to retry.
    match ready_rx.recv_timeout(std::time::Duration::from_secs(15)) {
        Ok(()) => Ok(pid),
        Err(_) => {
            job.kill_all(pid);
            Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "dsh web exited before printing its URL",
            ))
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
