//! Platform helpers: Python executable discovery, process group management, and
//! wrapper spawning.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use which::which;

static PYTHON_EXE: OnceLock<Option<PathBuf>> = OnceLock::new();

/// Find a suitable Python executable.
///
/// The result is cached for the lifetime of the process because locating a
/// real interpreter on Windows can require spawning several candidate
/// executables to filter out the Microsoft Store alias stub.
///
/// The `OCS_PYTHON_EXE` environment variable is always checked first and is not
/// cached, so users can still change it at runtime.
pub fn python_executable() -> std::io::Result<PathBuf> {
    if let Ok(p) = std::env::var("OCS_PYTHON_EXE") {
        let path = PathBuf::from(p);
        if is_valid_python(&path) {
            return Ok(path);
        }
    }

    match PYTHON_EXE.get_or_init(discover_python) {
        Some(path) => Ok(path.clone()),
        None => Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "No usable Python interpreter found. Set OCS_PYTHON_EXE or install Python from python.org.",
        )),
    }
}

fn discover_python() -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();

    #[cfg(windows)]
    {
        // The py launcher is the most reliable way to find a real Python on
        // Windows. Try well-known install locations first, then PATH.
        if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
            candidates.push(PathBuf::from(local_app_data).join(r"Programs\Python\Launcher\py.exe"));
        }
        candidates.push(PathBuf::from(r"C:\Windows\py.exe"));
        if let Ok(path) = which("py") {
            candidates.push(path);
        }
    }

    if let Ok(path) = which("python3") {
        candidates.push(path);
    }
    if let Ok(path) = which("python") {
        candidates.push(path);
    }

    // Common Windows install locations as a last resort.
    #[cfg(windows)]
    {
        if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
            let base = PathBuf::from(local_app_data);
            candidates.push(base.join(r"Programs\Python\Python312\python.exe"));
            candidates.push(base.join(r"Programs\Python\Python311\python.exe"));
            candidates.push(base.join(r"Programs\Python\Python310\python.exe"));
            candidates.push(base.join(r"Programs\Python\Python39\python.exe"));
        }
        candidates.push(PathBuf::from(r"C:\Python312\python.exe"));
        candidates.push(PathBuf::from(r"C:\Python311\python.exe"));
        candidates.push(PathBuf::from(r"C:\Python310\python.exe"));
        candidates.push(PathBuf::from(r"C:\Python39\python.exe"));
    }

    candidates.into_iter().find(|candidate| is_valid_python(candidate))
}

/// Verify that `path` is a real Python interpreter and not the Microsoft Store
/// alias stub. The stub prints a Store install message instead of running code.
fn is_valid_python(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    let output = match Command::new(path)
        .arg("-c")
        .arg("import sys; print(sys.executable)")
        .output()
    {
        Ok(output) => output,
        Err(_) => return false,
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    // The Microsoft Store stub exits 0 but writes the Store message to stderr
    // and stdout is empty. A real Python prints its executable path to stdout.
    if stdout.trim().is_empty() {
        return false;
    }
    if stderr.to_lowercase().contains("microsoft store") {
        return false;
    }
    true
}

/// Directory containing the current executable.
pub fn current_exe_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
}

/// Spawn the wrapper process `python -u repl_wrapper.py` in `session_dir` with
/// the given environment variables. The wrapper is the plugin's direct child and
/// lives in its own process group / job object so the whole subtree can be
/// killed reliably.
///
/// On Windows the wrapper is created with `CREATE_NEW_CONSOLE` so it owns a
/// visible console. IPython inherits that console, so there is exactly one
/// console window per REPL session and `Ctrl+C` is handled by IPython.
///
/// On Unix the wrapper is a normal headless child process that launches a
/// terminal emulator; the user still sees exactly one terminal window per
/// session.
pub fn spawn_wrapper(
    python: &Path,
    session_dir: &Path,
    env: &HashMap<String, String>,
) -> std::io::Result<ProcessGroup> {
    #[cfg(windows)]
    {
        spawn_wrapper_windows(python, session_dir, env)
    }
    #[cfg(not(windows))]
    {
        spawn_wrapper_unix(python, session_dir, env)
    }
}

#[cfg(windows)]
fn spawn_wrapper_windows(
    python: &Path,
    session_dir: &Path,
    env: &HashMap<String, String>,
) -> std::io::Result<ProcessGroup> {
    use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, HANDLE};
    use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_APPEND_DATA, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE,
        OPEN_ALWAYS,
    };
    use windows_sys::Win32::System::JobObjects::AssignProcessToJobObject;
    use windows_sys::Win32::System::Threading::{
        CreateProcessW, PROCESS_INFORMATION, STARTUPINFOW,
    };

    const CREATE_NEW_CONSOLE: u32 = 0x00000010;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x00000200;
    const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x01000000;
    const CREATE_UNICODE_ENVIRONMENT: u32 = 0x00000400;
    const STARTF_USESTDHANDLES: u32 = 0x00000100;

    let app_name = to_wide_path(python);
    let args = to_wide("-u repl_wrapper.py");
    let env_block = make_env_block(env);
    let cwd_wide = to_wide_path(session_dir);

    // Open a persistent stderr log outside the session directory so crash
    // diagnostics survive `ReplSession::Drop`'s cleanup.
    let stderr_log_path = std::env::temp_dir().join("ocs_repl_wrapper_stderr.log");
    let stderr_log_wide = to_wide_path(&stderr_log_path);
    let mut sa: SECURITY_ATTRIBUTES = unsafe { std::mem::zeroed() };
    sa.nLength = std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32;
    sa.bInheritHandle = 1; // the child must inherit this handle
    let stderr_handle: HANDLE = unsafe {
        CreateFileW(
            stderr_log_wide.as_ptr(),
            FILE_APPEND_DATA,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            &sa,
            OPEN_ALWAYS,
            FILE_ATTRIBUTE_NORMAL,
            std::ptr::null_mut(),
        )
    };
    let stderr_handle = if stderr_handle.is_null()
        || stderr_handle == windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE
    {
        // If we cannot open the log, proceed without redirecting stderr; a
        // missing diagnostic log is still better than failing to start.
        std::ptr::null_mut()
    } else {
        stderr_handle
    };

    let mut startup: STARTUPINFOW = unsafe { std::mem::zeroed() };
    startup.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
    if !stderr_handle.is_null() {
        startup.dwFlags = STARTF_USESTDHANDLES;
        startup.hStdInput = std::ptr::null_mut();
        startup.hStdOutput = std::ptr::null_mut();
        startup.hStdError = stderr_handle;
    }

    let mut proc_info: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };

    // Helper: attempt CreateProcessW with the given creation flags. A fresh
    // command-line buffer is built each time because CreateProcessW may modify it.
    let mut attempt = |flags: u32, inherit_handles: i32| unsafe {
        let mut cmdline = args.clone();
        CreateProcessW(
            app_name.as_ptr(),
            cmdline.as_mut_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            inherit_handles,
            flags,
            env_block.as_ptr() as *const _,
            cwd_wide.as_ptr(),
            &startup,
            &mut proc_info,
        )
    };

    let inherit = if stderr_handle.is_null() { 0 } else { 1 };
    let mut ok = attempt(
        CREATE_NEW_CONSOLE
            | CREATE_NEW_PROCESS_GROUP
            | CREATE_BREAKAWAY_FROM_JOB
            | CREATE_UNICODE_ENVIRONMENT,
        inherit,
    );
    if ok == 0 {
        // Parent job may disallow breakaway; retry without that flag.
        ok = attempt(
            CREATE_NEW_CONSOLE | CREATE_NEW_PROCESS_GROUP | CREATE_UNICODE_ENVIRONMENT,
            inherit,
        );
    }

    // The child now owns the handle reference; close our copy.
    if !stderr_handle.is_null() {
        unsafe {
            let _ = CloseHandle(stderr_handle);
        }
    }

    if ok == 0 {
        return Err(std::io::Error::from_raw_os_error(
            unsafe { GetLastError() } as i32
        ));
    }

    eprintln!(
        "[python-repl] spawn_wrapper python={} wrapper_pid={}",
        python.display(),
        proc_info.dwProcessId
    );

    let _ = unsafe { CloseHandle(proc_info.hThread) };

    let job = create_job_object().ok();
    if let Some(job) = job {
        let assigned = unsafe { AssignProcessToJobObject(job, proc_info.hProcess) };
        if assigned == 0 {
            // Parent job may disallow breakaway; fall back to process-tree
            // enumeration for cleanup instead of the job object.
            let _ = unsafe { CloseHandle(job) };
            return Ok(ProcessGroup {
                job: None,
                handle: proc_info.hProcess,
                pid: proc_info.dwProcessId,
            });
        }
    }

    Ok(ProcessGroup {
        job,
        handle: proc_info.hProcess,
        pid: proc_info.dwProcessId,
    })
}

#[cfg(windows)]
fn make_env_block(env: &HashMap<String, String>) -> Vec<u16> {
    // Merge the provided env with the current process environment. Sort by key
    // and build a Unicode environment block: key=value\0...\0\0.
    let mut merged: HashMap<String, String> = std::env::vars().collect();
    for (k, v) in env {
        merged.insert(k.clone(), v.clone());
    }
    let mut items: Vec<(String, String)> = merged.into_iter().collect();
    items.sort_by(|a, b| a.0.cmp(&b.0));

    let mut block: Vec<u16> = Vec::new();
    for (k, v) in items {
        for ch in k.encode_utf16() {
            block.push(ch);
        }
        block.push('=' as u16);
        for ch in v.encode_utf16() {
            block.push(ch);
        }
        block.push(0);
    }
    block.push(0);
    block
}

#[cfg(windows)]
fn to_wide(s: &str) -> Vec<u16> {
    let mut wide: Vec<u16> = s.encode_utf16().collect();
    wide.push(0);
    wide
}

#[cfg(windows)]
fn to_wide_path(path: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    let mut wide: Vec<u16> = path.as_os_str().encode_wide().collect();
    wide.push(0);
    wide
}

#[cfg(not(windows))]
fn spawn_wrapper_unix(
    python: &Path,
    session_dir: &Path,
    env: &HashMap<String, String>,
) -> std::io::Result<ProcessGroup> {
    use std::os::unix::process::CommandExt;
    use std::process::Stdio;

    let stderr_path = session_dir.join("wrapper_stderr.log");
    let stderr_file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&stderr_path)?;

    let mut cmd = Command::new(python);
    cmd.arg("-u")
        .arg("repl_wrapper.py")
        .current_dir(session_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::from(stderr_file));

    for (k, v) in env {
        cmd.env(k, v);
    }

    let child = unsafe {
        cmd.pre_exec(|| {
            libc::setpgid(0, 0);
            Ok(())
        })
        .spawn()?
    };

    Ok(ProcessGroup {
        pid: child.id(),
        pgid: child.id(),
    })
}

/// Kill the process group created by [`spawn_wrapper`].
#[cfg(unix)]
pub fn kill_group(group: &ProcessGroup) {
    let pgid = group.pgid as libc::pid_t;
    unsafe {
        let _ = libc::kill(-pgid, libc::SIGTERM);
    }
}

#[cfg(windows)]
pub fn kill_group(group: &ProcessGroup) {
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32First, Process32Next, PROCESSENTRY32, TH32CS_SNAPPROCESS,
    };
    use windows_sys::Win32::System::JobObjects::TerminateJobObject;
    use windows_sys::Win32::System::Threading::{OpenProcess, TerminateProcess, PROCESS_TERMINATE};

    unsafe {
        if let Some(job) = group.job {
            let _ = TerminateJobObject(job, 1);
        }

        // The job object only kills processes assigned to it; children started
        // by the wrapper (e.g. IPython) may survive. Enumerate the process tree
        // and terminate descendants, then the root.
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        let mut descendants = Vec::new();
        if snapshot != INVALID_HANDLE_VALUE && !snapshot.is_null() {
            let mut entry = PROCESSENTRY32 {
                dwSize: std::mem::size_of::<PROCESSENTRY32>() as u32,
                ..Default::default()
            };
            if Process32First(snapshot, &mut entry) != 0 {
                let mut parent_to_children: std::collections::HashMap<u32, Vec<u32>> =
                    std::collections::HashMap::new();
                loop {
                    parent_to_children
                        .entry(entry.th32ParentProcessID)
                        .or_default()
                        .push(entry.th32ProcessID);
                    if Process32Next(snapshot, &mut entry) == 0 {
                        break;
                    }
                }
                let mut stack = vec![group.pid];
                while let Some(pid) = stack.pop() {
                    if let Some(children) = parent_to_children.get(&pid) {
                        for &child in children {
                            if child != group.pid && !descendants.contains(&child) {
                                descendants.push(child);
                                stack.push(child);
                            }
                        }
                    }
                }
            }
            let _ = CloseHandle(snapshot);
        }

        for pid in descendants.iter().rev().chain(std::iter::once(&group.pid)) {
            let h = OpenProcess(PROCESS_TERMINATE, 0, *pid);
            if !h.is_null() && h != INVALID_HANDLE_VALUE {
                let _ = TerminateProcess(h, 1);
                let _ = CloseHandle(h);
            }
        }
    }
}

#[cfg(not(any(unix, windows)))]
pub fn kill_group(_group: &ProcessGroup) {}

/// Cross-platform graceful shutdown.
#[cfg(unix)]
pub fn shutdown_group(group: &ProcessGroup, timeout: std::time::Duration) {
    let pgid = group.pgid as libc::pid_t;
    unsafe {
        let _ = libc::kill(-pgid, libc::SIGTERM);
    }
    let start = std::time::Instant::now();
    while start.elapsed() < timeout {
        std::thread::sleep(std::time::Duration::from_millis(50));
        if !group.is_alive() {
            return;
        }
    }
    unsafe {
        let _ = libc::kill(-pgid, libc::SIGKILL);
    }
}

#[cfg(windows)]
pub fn shutdown_group(group: &ProcessGroup, timeout: std::time::Duration) {
    use windows_sys::Win32::System::Console::{
        AttachConsole, FreeConsole, GenerateConsoleCtrlEvent, GetConsoleWindow, CTRL_BREAK_EVENT,
    };

    // Best-effort graceful shutdown: the wrapper and IPython share one console.
    // Attach to that console and send Ctrl+Break. Both processes receive the
    // event; the wrapper's handler terminates IPython and exits.
    unsafe {
        if GetConsoleWindow().is_null() && AttachConsole(group.pid) != 0 {
            let _ = GenerateConsoleCtrlEvent(CTRL_BREAK_EVENT, 0);
            let _ = FreeConsole();
        }
    }

    let start = std::time::Instant::now();
    while start.elapsed() < timeout {
        std::thread::sleep(std::time::Duration::from_millis(50));
        if !group.is_alive() {
            return;
        }
    }
    kill_group(group);
}

#[cfg(not(any(unix, windows)))]
pub fn shutdown_group(_group: &ProcessGroup, _timeout: std::time::Duration) {}

/// Platform-specific token returned by [`spawn_wrapper`].
#[cfg(unix)]
#[derive(Debug, Clone, Copy)]
pub struct ProcessGroup {
    /// The wrapper process id (also the process group leader).
    pid: u32,
    pgid: u32,
}

#[cfg(unix)]
impl ProcessGroup {
    /// Process id of the immediate child wrapper.
    pub fn pid(&self) -> u32 {
        self.pid
    }

    /// Check whether the wrapper child is still alive, reaping it if it has
    /// already exited. This prevents zombie processes from being reported as
    /// alive when the user closes the terminal.
    pub fn is_alive(&self) -> bool {
        unsafe {
            let mut status: libc::c_int = 0;
            let r = libc::waitpid(self.pid as libc::pid_t, &mut status, libc::WNOHANG);
            if r == self.pid as libc::pid_t {
                // The wrapper has exited and been reaped.
                return false;
            }
            if r < 0 {
                // No child to wait for (ECHILD) or another error; treat as not alive.
                return false;
            }
            // r == 0 means the wrapper is still running.
            true
        }
    }
}

#[cfg(windows)]
#[derive(Debug)]
pub struct ProcessGroup {
    job: Option<windows_sys::Win32::Foundation::HANDLE>,
    handle: windows_sys::Win32::Foundation::HANDLE,
    pid: u32,
}

#[cfg(windows)]
unsafe impl Send for ProcessGroup {}

#[cfg(windows)]
impl Drop for ProcessGroup {
    fn drop(&mut self) {
        use windows_sys::Win32::Foundation::CloseHandle;
        unsafe {
            if let Some(job) = self.job {
                let _ = CloseHandle(job);
            }
            let _ = CloseHandle(self.handle);
        }
    }
}

#[cfg(windows)]
impl ProcessGroup {
    /// Process id of the immediate child.
    pub fn pid(&self) -> u32 {
        self.pid
    }

    /// Check whether the immediate child process is still alive.
    pub fn is_alive(&self) -> bool {
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::Threading::{
            GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
        };
        const STILL_ACTIVE: u32 = 259;

        // Prefer the stored handle; if it is invalid, fall back to opening by PID.
        let code = unsafe {
            let mut code: u32 = 0;
            if !self.handle.is_null() {
                let ok = GetExitCodeProcess(self.handle, &mut code);
                if ok != 0 {
                    return code == STILL_ACTIVE;
                }
            }
            let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, self.pid);
            if h.is_null() || h == windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE {
                return false;
            }
            let mut code: u32 = 0;
            let ok = GetExitCodeProcess(h, &mut code);
            let _ = CloseHandle(h);
            if ok == 0 {
                return true;
            }
            code
        };
        code == STILL_ACTIVE
    }
}

#[cfg(not(any(unix, windows)))]
#[derive(Debug, Clone, Copy)]
pub struct ProcessGroup(());

#[cfg(not(any(unix, windows)))]
impl ProcessGroup {
    pub fn pid(&self) -> u32 {
        0
    }

    pub fn is_alive(&self) -> bool {
        false
    }
}

#[cfg(windows)]
fn create_job_object() -> std::io::Result<windows_sys::Win32::Foundation::HANDLE> {
    use std::mem::size_of;
    use windows_sys::Win32::Foundation::{GetLastError, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;
    use windows_sys::Win32::System::JobObjects::{
        CreateJobObjectW, JobObjectExtendedLimitInformation, SetInformationJobObject,
        JOBOBJECT_BASIC_LIMIT_INFORMATION, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };

    unsafe {
        let job = CreateJobObjectW(
            std::ptr::null_mut::<SECURITY_ATTRIBUTES>(),
            std::ptr::null(),
        );
        if job == INVALID_HANDLE_VALUE || job.is_null() {
            return Err(std::io::Error::from_raw_os_error(GetLastError() as i32));
        }
        let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION {
            BasicLimitInformation: JOBOBJECT_BASIC_LIMIT_INFORMATION {
                LimitFlags: JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
                ..Default::default()
            },
            ..Default::default()
        };
        let ok = SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &mut info as *mut _ as *const _,
            size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        );
        if ok == 0 {
            return Err(std::io::Error::from_raw_os_error(GetLastError() as i32));
        }
        Ok(job)
    }
}

/// Test-only helper that spawns `cmd` in a new process group / job object.
#[cfg(any(unix, windows))]
#[cfg(test)]
pub fn spawn_test_group(cmd: &mut Command) -> std::io::Result<ProcessGroup> {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let child = unsafe {
            cmd.pre_exec(|| {
                libc::setpgid(0, 0);
                Ok(())
            })
            .spawn()?
        };
        Ok(ProcessGroup { pgid: child.id() })
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x00000200;
        const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x01000000;
        cmd.creation_flags(CREATE_NEW_PROCESS_GROUP | CREATE_BREAKAWAY_FROM_JOB);
        let child = match cmd.spawn() {
            Ok(child) => child,
            Err(_) => {
                cmd.creation_flags(CREATE_NEW_PROCESS_GROUP);
                cmd.spawn()?
            }
        };
        let pid = child.id();
        // Don't keep the raw handle: it is closed when `child` drops. Process
        // group methods fall back to opening by PID.
        Ok(ProcessGroup {
            job: None,
            handle: std::ptr::null_mut(),
            pid,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn python_executable_resolves_python() {
        // This test is informational on machines without Python.
        let _ = python_executable();
    }
}
