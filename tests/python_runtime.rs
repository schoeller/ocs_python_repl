use std::path::PathBuf;
use std::process::{Command, Stdio};

/// Best-effort path to the built `cdylib` so `ocs/__init__.py` can load it.
/// The test binary is in `target/<profile>/deps/`, so the cdylib is one
/// directory up under `target/<profile>/`.
fn find_cdylib() -> Option<PathBuf> {
    let ext = std::env::consts::DLL_EXTENSION;
    let prefix = std::env::consts::DLL_PREFIX;
    let name = format!("{prefix}ocs_python_repl.{ext}");
    let exe = std::env::current_exe().ok()?;
    let target_profile = exe.parent()?.parent()?;
    let candidate = target_profile.join(&name);
    if candidate.is_file() {
        Some(candidate)
    } else {
        None
    }
}

fn find_python() -> Option<std::path::PathBuf> {
    let python = ocs_python_repl::platform::python_executable().ok()?;
    let probe = Command::new(&python)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    if probe.map(|s| s.success()).unwrap_or(false) {
        Some(python)
    } else {
        None
    }
}

#[test]
#[ignore = "requires a Python interpreter"]
fn generated_python_files_compile() {
    let Some(python) = find_python() else {
        eprintln!("Python not found; skipping");
        return;
    };

    let session_dir = ocs_python_repl::session_dir::SessionDir::create(9999).unwrap();
    let files = [
        session_dir.root().join("repl_wrapper.py"),
        session_dir.root().join("startup.py"),
        session_dir.root().join("ocs/__init__.py"),
        session_dir.root().join("ocs/entities.py"),
    ];
    let mut failures = Vec::new();
    for path in &files {
        let output = Command::new(&python)
            .arg("-m")
            .arg("py_compile")
            .arg(path.as_os_str())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .expect("python run");
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !output.status.success() {
            failures.push(format!("{}: {stderr}", path.display()));
        }
    }
    session_dir.delete().unwrap();
    assert!(failures.is_empty(), "failed to compile: {failures:?}");
}

#[test]
#[ignore = "requires a Python interpreter"]
fn pyimport_magic_loads_script() {
    let Some(python) = find_python() else {
        eprintln!("Python not found; skipping");
        return;
    };

    let session_dir = ocs_python_repl::session_dir::SessionDir::create(9998).unwrap();
    let script = session_dir.root().join("test_script.py");
    std::fs::write(&script, "loaded_value = 42\n").unwrap();

    // startup.py imports ocs; we just verify the magic function exists by
    // invoking it via a small Python snippet that skips the host-dependent
    // ocs.doc initialization.
    let cdylib = find_cdylib();

    let probe = format!(
        r#"
import sys
sys.path.insert(0, r"{}")
import startup
startup._pyimport(r"{}")
assert startup.loaded_value == 42
"#,
        session_dir.root().display(),
        script.display()
    );
    let mut cmd = Command::new(&python);
    cmd.arg("-c")
        .arg(&probe)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(path) = cdylib.as_deref() {
        cmd.env("OCS_PLUGIN_CDYLIB_PATH", path);
    }
    let output = cmd.output().expect("python run");
    let stderr = String::from_utf8_lossy(&output.stderr);
    session_dir.delete().unwrap();
    assert!(output.status.success(), "pyimport failed: {stderr}");
}

#[test]
#[ignore = "requires a Python interpreter"]
fn pyexport_magic_writes_history() {
    let Some(python) = find_python() else {
        eprintln!("Python not found; skipping");
        return;
    };

    let session_dir = ocs_python_repl::session_dir::SessionDir::create(9997).unwrap();
    let out = session_dir.root().join("out.py");

    let cdylib = find_cdylib();

    let probe = format!(
        r#"
import sys
sys.path.insert(0, r"{}")
import startup
startup._pyexport(r"{}")
"#,
        session_dir.root().display(),
        out.display()
    );
    let mut cmd = Command::new(&python);
    cmd.arg("-c")
        .arg(&probe)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(path) = cdylib.as_deref() {
        cmd.env("OCS_PLUGIN_CDYLIB_PATH", path);
    }
    let output = cmd.output().expect("python run");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "pyexport failed: {stderr}");
    let content = std::fs::read_to_string(&out).unwrap();
    session_dir.delete().unwrap();
    assert!(
        content.contains("No input history"),
        "unexpected export: {content}"
    );
}
