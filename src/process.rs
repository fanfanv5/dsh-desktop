//! Child process management: a Windows Job Object for whole-tree cleanup,
//! plus spawning dsh web and parsing the URL it prints.

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
    SetInformationJobObject, TerminateJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};
use windows::Win32::System::Threading::{OpenProcess, PROCESS_SET_QUOTA, PROCESS_TERMINATE};

use crate::events::{Proxy, UiEvent};

/// Prevent a console window flashing when spawning node.
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

/// A Windows Job Object configured to kill every member process when closed.
pub struct Job {
    handle: HANDLE,
}

// A job object handle is an opaque kernel handle; using it from multiple
// threads (assign, terminate) is safe, and Arc guarantees the handle outlives
// every user before CloseHandle runs in Drop.
unsafe impl Send for Job {}
unsafe impl Sync for Job {}

impl Job {
    pub fn create() -> Option<Job> {
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

    /// Assign a spawned process (by pid) to this job. Returns false on failure,
    /// which usually means the process already belongs to another job.
    pub fn assign(&self, pid: u32) -> bool {
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

    /// Terminate every process in the job (the whole dsh process tree).
    pub fn kill_all(&self) {
        unsafe {
            let _ = TerminateJobObject(self.handle, 1);
        }
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.handle);
        }
    }
}

/// Spawn dsh web and start reader threads for its stdout/stderr.
///
/// On success the dsh URL is sent as a UiEvent::Ready from the stdout reader
/// thread once dsh prints it. The pid and job-assignment result are recorded in
/// state for cleanup.
pub fn spawn_dsh(
    node: &Path,
    bin_js: &Path,
    log_dir: &Path,
    proxy: Proxy,
    job: &Job,
    state: &ProcessState,
) -> std::io::Result<u32> {
    use std::os::windows::process::CommandExt;

    let mut child = Command::new(node)
        .arg(bin_js)
        .arg("web")
        .arg("--port")
        .arg("0")
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

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

    // stdout reader: parse the printed URL, keep draining to a log.
    let out_log = log_dir.join("dsh-web.out.log");
    let out_proxy = proxy;
    std::thread::spawn(move || {
        let mut log = std::fs::File::create(&out_log).ok();
        let mut ready = false;
        for line in BufReader::new(stdout).lines().flatten() {
            if let Some(f) = log.as_mut() {
                let _ = writeln!(f, "{}", line);
            }
            if !ready {
                if let Some(url) = parse_url(&line) {
                    ready = true;
                    let _ = out_proxy.send_event(UiEvent::Ready { url });
                }
            }
        }
        if !ready {
            let _ = out_proxy.send_event(UiEvent::Status {
                title: "启动失败".into(),
                detail: format!("dsh web 进程意外退出。日志：{}", out_log.display()),
                actions: vec!["retry".into()],
            });
        }
    });

    // stderr reader: drain to a log for diagnostics.
    let err_log = log_dir.join("dsh-web.err.log");
    std::thread::spawn(move || {
        if let Ok(mut f) = std::fs::File::create(&err_log) {
            for line in BufReader::new(stderr).lines().flatten() {
                let _ = writeln!(f, "{}", line);
            }
        }
    });

    Ok(pid)
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
