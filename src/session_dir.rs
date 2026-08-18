//! Temporary session directory for a Python REPL tab.
//!
//! Each `PYTHONSHELL` invocation creates a directory containing the
//! build-generated `repl_wrapper.py`, `startup.py`, `ocs` package, stubs, and
//! `py.typed`. The compiled `_ocs` extension is *not* copied; it is loaded from
//! the plugin directory via `PYTHONPATH`.

use std::fs;
use std::path::{Path, PathBuf};

/// A temporary directory that holds the Python files for one REPL session.
pub struct SessionDir {
    root: PathBuf,
}

impl SessionDir {
    /// Create a temp session directory for `tab_id` and copy the build-generated
    /// Python files into it.
    pub fn create(tab_id: u64) -> std::io::Result<Self> {
        let mut root = std::env::temp_dir();
        root.push(format!(
            "ocs_python_repl_{}_{}_{}",
            std::process::id(),
            tab_id,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
        ));
        fs::create_dir_all(&root)?;

        let generated = PathBuf::from(env!("OUT_DIR")).join("python");
        copy_dir_all(&generated, &root)?;

        Ok(Self { root })
    }

    /// Path to the session directory.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Best-effort recursive deletion, with a short retry loop to give child
    /// processes time to release file locks.
    pub fn delete(self) -> std::io::Result<()> {
        let mut last_err = None;
        for attempt in 0..10 {
            if !self.root.exists() {
                return Ok(());
            }
            match fs::remove_dir_all(&self.root) {
                Ok(()) => return Ok(()),
                Err(e) => {
                    last_err = Some(e);
                    if attempt < 9 {
                        std::thread::sleep(std::time::Duration::from_millis(100));
                    }
                }
            }
        }
        Err(last_err.unwrap())
    }
}

fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let file_name = src_path.file_name().expect("entry has a name");
        let dst_path = dst.join(file_name);
        if src_path.is_dir() {
            copy_dir_all(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_dir_creates_expected_files() {
        let dir = SessionDir::create(123).unwrap();
        assert!(dir.root().exists());
        assert!(dir.root().join("repl_wrapper.py").exists());
        assert!(dir.root().join("startup.py").exists());
        assert!(dir.root().join("ocs/__init__.py").exists());
        assert!(dir.root().join("ocs/entities.py").exists());
        assert!(dir.root().join("py.typed").exists());
        dir.delete().unwrap();
    }
}
