# CI helper for ocs_python_repl.
# Runs the ignored Python-runtime integration tests when a Python interpreter
# is available. Intended to be invoked from the repository's CI pipeline.
$ErrorActionPreference = "Stop"
$crate = Split-Path -Parent $PSScriptRoot
Push-Location $crate
try {
    & cargo test -p ocs_python_repl --test python_runtime -- --ignored
} finally {
    Pop-Location
}
