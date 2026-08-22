#!/usr/bin/env python3
"""Bump the ocs_python_repl plugin version and keep Cargo.lock in sync.

Usage:
    python scripts/bump-version.py 0.1.7
    python scripts/bump-version.py patch   # increments 0.1.6 -> 0.1.7
    python scripts/bump-version.py         # same as 'patch'

The script updates:
  - Cargo.toml    [package] version
  - plugin.toml   [plugin] version
  - Cargo.lock    package version for ocs_python_repl (via cargo update)

After running, inspect the diff, commit, and tag:
    git add Cargo.toml plugin.toml Cargo.lock
    git commit -m "chore(release): bump ocs_python_repl to v0.1.7"
    git tag -a v0.1.7 -m "ocs_python_repl v0.1.7"
    git push origin main v0.1.7
"""

import os
import re
import subprocess
import sys
from pathlib import Path


def current_version(cargo_toml: str) -> str:
    m = re.search(r'^version = "(\d+)\.(\d+)\.(\d+)"', cargo_toml, re.M)
    if not m:
        raise SystemExit("could not find version in Cargo.toml")
    return m.group(0).split('"')[1]


def bump_version(current: str, arg: str) -> str:
    major, minor, patch = map(int, current.split("."))
    if arg == "patch" or arg == "":
        patch += 1
    elif arg == "minor":
        minor += 1
        patch = 0
    elif arg == "major":
        major += 1
        minor = 0
        patch = 0
    elif re.fullmatch(r"\d+\.\d+\.\d+", arg):
        return arg
    else:
        raise SystemExit(f"unknown bump argument: {arg!r}")
    return f"{major}.{minor}.{patch}"


def replace_version(text: str, newver: str) -> str:
    return re.sub(r'^version = "[\d.]+"', f'version = "{newver}"', text, count=1, flags=re.M)


def main() -> int:
    repo_root = Path(__file__).resolve().parent.parent
    os.chdir(repo_root)

    cargo_path = repo_root / "Cargo.toml"
    plugin_path = repo_root / "plugin.toml"

    cargo = cargo_path.read_text(encoding="utf-8")
    current = current_version(cargo)

    arg = sys.argv[1] if len(sys.argv) > 1 else "patch"
    newver = bump_version(current, arg)

    print(f"bumping {current} -> {newver}")

    cargo = replace_version(cargo, newver)
    cargo_path.write_text(cargo, encoding="utf-8")

    plugin = plugin_path.read_text(encoding="utf-8")
    plugin = replace_version(plugin, newver)
    plugin_path.write_text(plugin, encoding="utf-8")

    print("running cargo update -p ocs_python_repl to sync Cargo.lock ...")
    subprocess.run(["cargo", "update", "-p", "ocs_python_repl"], check=True)

    print(f"done. version is now {newver}.")
    print("next: git add Cargo.toml plugin.toml Cargo.lock && git commit && git tag -a v" + newver)
    return 0


if __name__ == "__main__":
    sys.exit(main())
