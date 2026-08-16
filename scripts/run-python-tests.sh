#!/usr/bin/env bash
# CI helper for ocs_python_repl.
# Runs the ignored Python-runtime integration tests when a Python interpreter
# is available. Intended to be invoked from the repository's CI pipeline.
set -euo pipefail

cd "$(dirname "$0")/.."

cargo test -p ocs_python_repl --test python_runtime -- --ignored
