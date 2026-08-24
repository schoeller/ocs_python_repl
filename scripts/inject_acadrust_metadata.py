#!/usr/bin/env python3
"""Inject the resolved `acadrust` fingerprint into a staged `plugin.toml`.

This script reads `Cargo.lock`, extracts the full git source that the
`ocs_python_repl` package resolves to, and rewrites the given `plugin.toml` to
set `acadrust_source` under `[opencad]`.

Usage:
    python scripts/inject_acadrust_metadata.py dist/plugin.toml
"""

import re
import sys
from pathlib import Path


def read_cargo_lock(path: Path) -> str:
    return path.read_text(encoding="utf-8")


def parse_packages(text: str) -> dict:
    """Return a dict keyed by (name, version, source?) -> package block text."""
    packages = {}
    blocks = re.split(r"\n\[\[package\]\]\n", text)
    for block in blocks:
        name = re.search(r'^name = "([^"]+)"', block, re.M)
        version = re.search(r'^version = "([^"]+)"', block, re.M)
        source = re.search(r'^source = "([^"]+)"', block, re.M)
        if name and version:
            key = (name.group(1), version.group(1), source.group(1) if source else None)
            packages[key] = block
    return packages


def direct_dependency_sources(packages: dict, package_name: str) -> dict[str, str]:
    """Return name -> source for a package's direct dependencies from Cargo.lock.

    Cargo.lock lists duplicate crate versions as 'name = "foo"' plus
    'version = "x.y.z"' (and source) inside the package's `dependencies` array.
    """
    pattern = re.compile(
        rf'^name = "{re.escape(package_name)}"\s*\n'
        r'^version = "[^"]+"\s*\n'
        r'(?:^source = "[^"]+"\s*\n)?'
        r'^dependencies = \[(.*?)\]',
        re.M | re.S,
    )
    for block in packages.values():
        m = pattern.search(block)
        if m:
            deps_text = m.group(1)
            deps: dict[str, str] = {}
            for line in deps_text.splitlines():
                line = line.strip().rstrip(",")
                if not line:
                    continue
                line = line.strip('"')
                m2 = re.match(
                    r'^(?P<name>[a-zA-Z0-9_-]+)'
                    r'(?:\s+(?P<version>[^\s(]+))?'
                    r'(?:\s+\((?P<source>[^)]+)\))?$',
                    line,
                )
                if m2:
                    deps[m2.group("name")] = m2.group("source") or ""
            return deps
    raise SystemExit(f"could not find {package_name} package in Cargo.lock")


def find_acadrust_source(packages: dict) -> str:
    """Return the source string for the acadrust package used by ocs_python_repl."""
    deps = direct_dependency_sources(packages, "ocs_python_repl")
    acadrust_source = deps.get("acadrust", "")

    candidates = [
        key for key in packages if key[0] == "acadrust" and key[2] == acadrust_source
    ]
    if not candidates and acadrust_source:
        m = re.search(r"#([0-9a-f]{40})", acadrust_source)
        if m:
            full_hash = m.group(1)
            candidates = [
                key
                for key in packages
                if key[0] == "acadrust" and key[2] and full_hash in key[2]
            ]
    if not candidates:
        candidates = [key for key in packages if key[0] == "acadrust" and key[2]]
    if not candidates:
        raise SystemExit("no acadrust package found in Cargo.lock")

    return candidates[0][2]


def set_toml_value(text: str, section: str, key: str, value: str) -> str:
    """Set a key inside a TOML section, adding the section/key if needed."""
    section_pattern = re.compile(rf'^\[{re.escape(section)}\]\s*$', re.M)
    if not section_pattern.search(text):
        text = text.rstrip() + f"\n\n[{section}]\n"

    def section_end(start: int) -> int:
        next_section = re.search(r'^\[', text[start:], re.M)
        if next_section:
            return start + next_section.start()
        return len(text)

    m = section_pattern.search(text)
    assert m
    sec_start = m.end()
    sec_end = section_end(sec_start)
    section_text = text[sec_start:sec_end]

    key_pattern = re.compile(rf'^{re.escape(key)}\s*=\s*"[^"]*"\s*$', re.M)
    new_line = f'{key} = "{value}"\n'
    if key_pattern.search(section_text):
        section_text = key_pattern.sub(new_line.rstrip(), section_text)
    else:
        section_text = section_text.rstrip() + "\n" + new_line

    return text[:sec_start] + section_text + text[sec_end:]


def main() -> int:
    if len(sys.argv) != 2:
        print(f"usage: {sys.argv[0]} <plugin.toml>", file=sys.stderr)
        return 1

    plugin_toml_path = Path(sys.argv[1])
    repo_root = Path(__file__).resolve().parent.parent
    lock_path = repo_root / "Cargo.lock"

    lock_text = read_cargo_lock(lock_path)
    packages = parse_packages(lock_text)
    source = find_acadrust_source(packages)

    toml_text = plugin_toml_path.read_text(encoding="utf-8")
    toml_text = set_toml_value(toml_text, "opencad", "acadrust_source", source)
    plugin_toml_path.write_text(toml_text, encoding="utf-8")

    print(f"injected acadrust {source}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
