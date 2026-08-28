#!/usr/bin/env python3
"""Re-pin ocs_python_repl dependencies to match an upstream OpenCADStudio ref.

The single source of truth for the upstream ref is `.upstream-tag` at the
repository root. It may contain either a release tag (e.g. `v0.9.7`) or a full
git revision. This script fetches the upstream `Cargo.lock` at that ref and
rewrites `Cargo.toml` with matching pins for:

- ocs_plugin_api (git tag or rev)
- acadrust (git rev)
- serde, serde_json, bincode, memmap2, anyhow, getrandom, libc, windows-sys
  (registry exact versions)

Usage:
    python scripts/repin.py [v0.9.6 | 1fb770fb...]

If no ref is given, the value from `.upstream-tag` is used. The script does not
commit, tag, or push; it only updates the local files.
"""

import os
import re
import sys
import urllib.request
from pathlib import Path

UPSTREAM = "HakanSeven12/OpenCADStudio"
UPSTREAM_URL = f"https://github.com/{UPSTREAM}"
LOCK_URL = f"https://raw.githubusercontent.com/{UPSTREAM}/{{tag}}/Cargo.lock"
API_TOML_URL = f"https://raw.githubusercontent.com/{UPSTREAM}/{{tag}}/crates/ocs_plugin_api/Cargo.toml"


def read_upstream_ref(path: Path) -> str:
    text = path.read_text(encoding="utf-8").strip()
    # Accept the first non-empty, non-comment line.
    for line in text.splitlines():
        line = line.split("#", 1)[0].strip()
        if line:
            return line
    raise SystemExit(f"no upstream ref found in {path}")


def fetch_url(url: str) -> str:
    print(f"fetching {url}")
    with urllib.request.urlopen(url, timeout=60) as resp:
        return resp.read().decode("utf-8")


def fetch_lock(tag: str) -> str:
    return fetch_url(LOCK_URL.format(tag=tag))


def fetch_api_toml(tag: str) -> str:
    return fetch_url(API_TOML_URL.format(tag=tag))


def acadrust_rev_from_api_toml(text: str) -> tuple[str, str]:
    """Return (git_url, rev) for acadrust as declared in ocs_plugin_api/Cargo.toml.

    Cargo unifies git sources by their exact reference string, so we must use
    the same rev form that upstream uses (e.g. a short hash), not the full
    40-char hash from Cargo.lock.
    """
    for line in text.splitlines():
        line = line.split("#", 1)[0].strip()
        if "acadrust" not in line:
            continue
        m = re.search(r'git\s*=\s*"([^"]+)"', line)
        if not m:
            continue
        url = m.group(1)
        m = re.search(r'rev\s*=\s*"([^"]+)"', line)
        if not m:
            raise SystemExit(f"acadrust dependency in ocs_plugin_api/Cargo.toml has no rev: {line}")
        return url, m.group(1)
    raise SystemExit("could not find acadrust dependency in ocs_plugin_api/Cargo.toml")


def lock_packages(text: str):
    """Yield (name, version, source) blocks from a Cargo.lock file."""
    blocks = re.split(r"\n\[\[package\]\]\n", text)
    for block in blocks:
        name = re.search(r'^name = "([^"]+)"', block, re.M)
        version = re.search(r'^version = "([^"]+)"', block, re.M)
        source = re.search(r'^source = "([^"]+)"', block, re.M)
        if name and version:
            yield name.group(1), version.group(1), source.group(1) if source else None


def upstream_direct_deps(text: str, package_name: str = "ocs_plugin_api") -> dict[str, str]:
    """Return name -> version for a package's direct deps from Cargo.lock.

    Cargo.lock lists duplicate crate versions as 'name = "foo"' plus
    'version = "x.y.z"' inside the package's `dependencies` array. We parse the
    ocs_plugin_api block so we match the versions the host contract actually
    uses, rather than versions pulled in by unrelated workspace crates.
    """
    pattern = re.compile(
        rf'^name = "{re.escape(package_name)}"\s*\n'
        r'^version = "[^"]+"\s*\n'
        r'(?:^source = "[^"]+"\s*\n)?'
        r'^dependencies = \[(.*?)\]',
        re.M | re.S,
    )
    m = pattern.search(text)
    if not m:
        raise SystemExit(f"could not find {package_name} package in upstream Cargo.lock")

    deps_text = m.group(1)
    deps: dict[str, str] = {}
    for line in deps_text.splitlines():
        line = line.strip().rstrip(",")
        if not line:
            continue
        # Each entry is either 'foo' or 'foo x.y.z'.
        parts = line.strip('"').split()
        if len(parts) == 1:
            deps[parts[0]] = ""
        else:
            deps[parts[0]] = parts[1]
    return deps


def pkg_version(packages: dict, direct_deps: dict[str, str], name: str) -> str:
    if name in direct_deps and direct_deps[name]:
        return direct_deps[name]
    if name not in packages:
        raise SystemExit(f"{name} not found in upstream Cargo.lock")
    return packages[name][0]


def git_source(packages: dict, name: str) -> tuple[str, str]:
    ver, src = packages[name]
    if src is None:
        raise SystemExit(f"{name} is not a git dependency in upstream Cargo.lock")
    m = re.fullmatch(r"git\+(?P<url>[^?#]+)(\?[^#]*)?#(?P<rev>[0-9a-f]{40})", src)
    if not m:
        raise SystemExit(f"unparseable git source for {name}: {src}")
    return m.group("url"), m.group("rev")


def replace_dep(cargo: str, name: str, new_value: str) -> str:
    pattern = rf'^{name} = \{{[^}}]+\}}'
    cargo, n = re.subn(pattern, f'{name} = {new_value}', cargo, flags=re.M)
    if n == 0:
        raise SystemExit(f"dependency {name} not found in Cargo.toml")
    return cargo


def is_sha(ref: str) -> bool:
    return len(ref) >= 7 and all(c in "0123456789abcdef" for c in ref.lower())


def metadata_key(ref: str) -> str:
    return "rev" if is_sha(ref) else "tag"


def main() -> int:
    repo_root = Path(__file__).resolve().parent.parent
    ref_path = repo_root / ".upstream-tag"

    ref = sys.argv[1] if len(sys.argv) > 1 else read_upstream_ref(ref_path)
    print(f"re-pinning to upstream ref {ref}")

    lock_text = fetch_lock(ref)
    api_toml_text = fetch_api_toml(ref)
    packages = {name: (ver, src) for name, ver, src in lock_packages(lock_text)}
    direct_deps = upstream_direct_deps(lock_text)

    acad_url, acad_rev = acadrust_rev_from_api_toml(api_toml_text)

    cargo_path = repo_root / "Cargo.toml"
    cargo = cargo_path.read_text(encoding="utf-8")

    # Update the metadata ref (tag or rev). The existing key may differ from
    # the new one (e.g. switching from a pinned revision to a release tag).
    key = metadata_key(ref)
    cargo, n = re.subn(
        r'^(\[package\.metadata\.upstream\]\s*\n)(?:tag|rev) = "[^"]+"[ \t]*\n',
        rf'\g<1>{key} = "{ref}"\n',
        cargo,
        flags=re.M,
    )
    if n != 1:
        raise SystemExit(f"expected 1 upstream metadata entry to rewrite, rewrote {n}")

    # ocs_plugin_api: git by tag or rev.
    ocs_api_value = f'{{ git = "{UPSTREAM_URL}", {key} = "{ref}", features = ["host"] }}'
    cargo = replace_dep(cargo, "ocs_plugin_api", ocs_api_value)

    # acadrust: git by rev.
    acad_value = f'{{ git = "{acad_url}", rev = "{acad_rev}", features = ["serde"] }}'
    cargo = replace_dep(cargo, "acadrust", acad_value)

    # Registry dependencies: pin to exact upstream versions.
    registry_pins = {
        "serde": f'{{ version = "={pkg_version(packages, direct_deps, "serde")}", features = ["derive"] }}',
        "serde_json": f'{{ version = "={pkg_version(packages, direct_deps, "serde_json")}" }}',
        "bincode": f'{{ version = "={pkg_version(packages, direct_deps, "bincode")}" }}',
        "memmap2": f'{{ version = "={pkg_version(packages, direct_deps, "memmap2")}" }}',
        "anyhow": f'{{ version = "={pkg_version(packages, direct_deps, "anyhow")}" }}',
        "getrandom": f'{{ version = "={pkg_version(packages, direct_deps, "getrandom")}" }}',
        "libc": f'{{ version = "={pkg_version(packages, direct_deps, "libc")}" }}',
    }

    windows_sys_version = pkg_version(packages, direct_deps, "windows-sys")
    registry_pins["windows-sys"] = (
        f'{{ version = "={windows_sys_version}", '
        f'features = ["Win32_Foundation", "Win32_System_Threading", '
        f'"Win32_System_JobObjects", "Win32_System_Diagnostics", '
        f'"Win32_System_Diagnostics_ToolHelp", "Win32_System_Console"] }}'
    )

    for dep_name, dep_value in registry_pins.items():
        cargo = replace_dep(cargo, dep_name, dep_value)

    cargo_path.write_text(cargo, encoding="utf-8")
    ref_path.write_text(ref + "\n", encoding="utf-8")

    print("updated Cargo.toml and .upstream-tag")
    print(f"  ocs_plugin_api -> {ref}")
    print(f"  acadrust       -> {acad_url}@{acad_rev[:7]}")
    for dep_name in registry_pins:
        print(f"  {dep_name:<15} -> {pkg_version(packages, direct_deps, dep_name)}")

    return 0


if __name__ == "__main__":
    sys.exit(main())
