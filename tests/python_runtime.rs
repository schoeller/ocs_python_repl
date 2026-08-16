use std::process::{Command, Stdio};

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
    let output = Command::new(&python)
        .arg("-c")
        .arg(&probe)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("python run");
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
    let output = Command::new(&python)
        .arg("-c")
        .arg(&probe)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("python run");
    let stderr = String::from_utf8_lossy(&output.stderr);
    session_dir.delete().unwrap();
    assert!(output.status.success(), "pyexport failed: {stderr}");
    let content = std::fs::read_to_string(&out).unwrap();
    assert!(
        content.contains("No input history"),
        "unexpected export: {content}"
    );
}
