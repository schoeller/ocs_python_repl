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

/// Python files generated at compile time by `build.rs`, embedded into the
/// cdylib so the plugin works when installed on a machine without the build
/// tree.
mod embedded {
    pub const REPL_WRAPPER: &str = include_str!(concat!(env!("OUT_DIR"), "/python/repl_wrapper.py"));
    pub const STARTUP: &str = include_str!(concat!(env!("OUT_DIR"), "/python/startup.py"));
    pub const OCS_INIT: &str = include_str!(concat!(env!("OUT_DIR"), "/python/ocs/__init__.py"));
    pub const OCS_INIT_STUB: &str =
        include_str!(concat!(env!("OUT_DIR"), "/python/ocs/__init__.pyi"));
    pub const OCS_EXTENSION_STUB: &str =
        include_str!(concat!(env!("OUT_DIR"), "/python/ocs/_ocs.pyi"));
    pub const ENTITIES: &str = include_str!(concat!(env!("OUT_DIR"), "/python/ocs/entities.py"));
    pub const ENTITIES_STUB: &str =
        include_str!(concat!(env!("OUT_DIR"), "/python/ocs/entities.pyi"));
    pub const PY_TYPED: &str = include_str!(concat!(env!("OUT_DIR"), "/python/py.typed"));
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

        let ocs_dir = root.join("ocs");
        fs::create_dir_all(&ocs_dir)?;

        write_file(&root.join("repl_wrapper.py"), embedded::REPL_WRAPPER)?;
        write_file(&root.join("startup.py"), embedded::STARTUP)?;
        write_file(&ocs_dir.join("__init__.py"), embedded::OCS_INIT)?;
        write_file(&ocs_dir.join("__init__.pyi"), embedded::OCS_INIT_STUB)?;
        write_file(&ocs_dir.join("_ocs.pyi"), embedded::OCS_EXTENSION_STUB)?;
        write_file(&ocs_dir.join("entities.py"), embedded::ENTITIES)?;
        write_file(&ocs_dir.join("entities.pyi"), embedded::ENTITIES_STUB)?;
        write_file(&root.join("py.typed"), embedded::PY_TYPED)?;

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

fn write_file(path: &Path, content: &str) -> std::io::Result<()> {
    fs::write(path, content.as_bytes())
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
