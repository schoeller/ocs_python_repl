//! Per-tab REPL session: Python wrapper child and request proxy.

use std::collections::HashMap;
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::Arc;

use ocs_plugin_api::host::HostApi;
use ocs_plugin_api::ipc::proxy::{run_request_proxy_with_shutdown, PROXY_TOKEN_LEN};

use crate::platform::{python_executable, shutdown_group, spawn_wrapper, ProcessGroup};
use crate::session_dir::SessionDir;

pub struct ReplSession {
    group: ProcessGroup,
    session_dir: Option<SessionDir>,
    proxy_shutdown: Option<std::sync::mpsc::Sender<()>>,
    proxy_thread: Option<std::thread::JoinHandle<()>>,
}

impl ReplSession {
    /// Spawn a new REPL session for `tab_id`.
    pub fn spawn(host: &mut dyn HostApi, tab_id: u64) -> anyhow::Result<Self> {
        let python = python_executable()?;

        // Open the V4 document view before starting Python.
        let doc_info = host
            .document_view_v4(tab_id)
            .ok_or_else(|| anyhow::anyhow!("host did not provide a V4 document view"))?;
        let snapshot_path = doc_info.path.clone();

        // Create the session directory with generated Python files.
        let session_dir = SessionDir::create(tab_id)?;
        eprintln!(
            "[python-repl] session directory: {}",
            session_dir.root().display()
        );

        // Bind the request proxy listener *before* spawning the wrapper so the
        // Python `_ocs.Document` can connect immediately. Generate an auth token
        // shared only with the child via environment variable so arbitrary local
        // processes cannot forward privileged host requests.
        let request_listener = TcpListener::bind(("127.0.0.1", 0))?;
        let request_port = request_listener.local_addr()?.port();
        let mut request_token = [0u8; PROXY_TOKEN_LEN];
        getrandom::getrandom(&mut request_token)
            .map_err(|e| std::io::Error::other(e.to_string()))?;

        let proxy_thread = if let Some(request_sender) = host.plugin_request_sender() {
            let request_sender = Arc::from(request_sender);
            let (shutdown_tx, shutdown_rx) = std::sync::mpsc::channel::<()>();
            let handle = std::thread::spawn(move || {
                let _ = run_request_proxy_with_shutdown(
                    request_listener,
                    request_sender,
                    request_token,
                    shutdown_rx,
                );
            });
            Some((shutdown_tx, handle))
        } else {
            None
        };

        // PYTHONPATH = session dir (for ocs/startup) + plugin dir (for the cdylib).
        let plugin_dir = plugin_dir();
        let mut pythonpath = vec![session_dir.root().to_path_buf(), plugin_dir];
        if let Ok(existing) = std::env::var("PYTHONPATH") {
            for part in std::env::split_paths(&existing) {
                pythonpath.push(part);
            }
        }
        let pythonpath = std::env::join_paths(pythonpath)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        let mut env = HashMap::new();
        env.insert("OCS_V4_SNAPSHOT".to_string(), snapshot_path);
        env.insert("OCS_REQUEST_PORT".to_string(), request_port.to_string());
        env.insert(
            "OCS_REQUEST_TOKEN".to_string(),
            request_token
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>(),
        );
        if let Ok(cdylib_path) = std::env::var("OCS_PLUGIN_CDYLIB_PATH") {
            env.insert("OCS_PLUGIN_CDYLIB_PATH".to_string(), cdylib_path);
        }
        env.insert(
            "PYTHONPATH".to_string(),
            pythonpath.to_string_lossy().to_string(),
        );

        let group = spawn_wrapper(&python, session_dir.root(), &env)?;
        eprintln!(
            "[python-repl] spawned wrapper pid={} alive={}",
            group.pid(),
            group.is_alive()
        );

        let (proxy_shutdown, proxy_thread) = proxy_thread
            .map(|(tx, handle)| (Some(tx), Some(handle)))
            .unwrap_or((None, None));

        let mut session = Self {
            group,
            session_dir: Some(session_dir),
            proxy_shutdown,
            proxy_thread,
        };

        // Wait briefly for an immediate wrapper crash (e.g. missing Python),
        // up to ~500 ms, polling every 20 ms. The real REPL readiness is
        // determined by the user seeing the IPython prompt in the terminal.
        let start = std::time::Instant::now();
        while start.elapsed() < std::time::Duration::from_millis(500) {
            if session.is_alive() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        eprintln!(
            "[python-repl] wrapper alive after {:?}={}",
            start.elapsed(),
            session.is_alive()
        );

        Ok(session)
    }

    /// Return true if the wrapper child is still alive.
    pub fn is_alive(&mut self) -> bool {
        self.group.is_alive()
    }

    /// Shut down the session: kill the wrapper process group, stop the request
    /// proxy, and delete the session directory. Safe to call multiple times.
    pub fn shutdown(&mut self) {
        shutdown_group(&self.group, std::time::Duration::from_secs(3));

        if let Some(tx) = self.proxy_shutdown.take() {
            let _ = tx.send(());
        }
        if let Some(handle) = self.proxy_thread.take() {
            let _ = handle.join();
        }

        if let Some(dir) = self.session_dir.take() {
            if let Err(e) = dir.delete() {
                eprintln!("[python-repl] failed to delete session dir: {e}");
            }
        }
    }
}

impl Drop for ReplSession {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Directory that contains the loaded cdylib. Used on `PYTHONPATH` so Python can
/// import the Rust extension by its crate name (`ocs_python_repl`).
fn plugin_dir() -> PathBuf {
    if let Ok(path) = std::env::var("OCS_PLUGIN_CDYLIB_PATH") {
        let path = PathBuf::from(path);
        if let Some(parent) = path.parent() {
            return parent.to_path_buf();
        }
    }
    crate::platform::current_exe_dir()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Command, Stdio};

    #[test]
    fn repl_session_drop_kills_child() {
        // Spawn a long-running process so we can verify that dropping the
        // session kills it through the process group abstraction.
        let python = python_executable().expect("python is required for this test");
        let session_dir = SessionDir::create(999).expect("session dir");
        let mut cmd = Command::new(&python);
        cmd.arg("-c")
            .arg("import time; time.sleep(30)")
            .current_dir(session_dir.root())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let group = crate::platform::spawn_test_group(&mut cmd).expect("spawn");
        let mut session = ReplSession {
            group,
            session_dir: Some(session_dir),
            proxy_shutdown: None,
            proxy_thread: None,
        };
        assert!(session.is_alive());
        session.shutdown();
        assert!(!session.is_alive());
    }
}
