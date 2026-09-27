#!/usr/bin/env python3
"""Parsers and gates for the nightly upstream re-pin of ocs_python_repl.

The plugin is a cdylib loaded by OpenCADStudio, so "compatible" means built
against exactly the same codec crate, the same ocs_plugin_api commit and the
same transitive dependency versions as the shipped host.  Cargo keys a git
source on the literal `?rev=` text, so the plugin must copy ocs_plugin_api's
spelling byte for byte; an abbreviated rev and its 40-hex expansion resolve to
the same commit and are still two different sources.

Every fact the gates derive comes from the upstream Cargo.lock (ground
truth); every spelling they rewrite comes from the upstream manifests
verbatim.  Upstream has respelled its dependency graph repeatedly — the
`[patch]` redirect at v0.9.8, the `git@` redirect at v2026.36, no patch at
v2026.37/v2026.38, and the crate+repository rename acadrust ->
opencadcodec at v2026.39 — so nothing here may hardcode a crate or
repository name: the codec is discovered as "the git dependency of
ocs_plugin_api", whatever it is called this week.

Exit codes (the two-code taxonomy this repository inherited from the
landsurvey plugin):

    0  ok — the fact was derived, the gate passed
    1  ESCALATE — a human decision is required (codec series moved, api
       window dropped the plugin, two codecs in one graph, dependency
       mismatch); the nightly run opens an escalation issue
    2  UNPARSEABLE — the upstream shape is not understood; this is a bug in
       this repository, not a decision.  Vendor the offending manifests
       under .github/scripts/tests/fixtures/host/<tag>/ and teach the parser
       in the same PR.

The corpus of vendored manifests under tests/fixtures/host/ is the
regression suite: a shape that breaks a gate gets its tag added there in the
same PR as the fix.  The canary workflow runs the same read-only gates
against upstream `main` every night so the next respelling is caught with
lead time instead of a blocked release.
"""

from __future__ import annotations

import argparse
import os
import re
import sys
import tomllib
from pathlib import Path

OK, ESCALATE, UNPARSEABLE = 0, 1, 2

UPSTREAM_REPO = "HakanSeven12/OpenCADStudio"


class Escalate(Exception):
    """A human decision is required; the re-pin must stop."""


class Unparseable(Exception):
    """The upstream shape is not understood; the parser needs a fix."""


GIT_SOURCE = re.compile(r"git\+(?P<url>[^?#]+)(?:\?[^#]*)?#(?P<rev>[0-9a-f]{40})")
SLUG_SCHEME = re.compile(r"^[a-z+]+://")
SLUG_GIT_SUFFIX = re.compile(r"\.git$")

PATCH_BEGIN = "# BEGIN MIRRORED PATCH (managed by .github/scripts/repin_gates.py)"
PATCH_END = "# END MIRRORED PATCH"
PATCH_REGION = re.compile(
    r"^[ \t]*" + re.escape(PATCH_BEGIN) + r"[^\n]*\n"
    r".*?^[ \t]*" + re.escape(PATCH_END) + r"[^\n]*\n",
    re.M | re.S,
)
PATCH_SENTINEL = "\x00mirrored-patch\x00"

# Crates the plugin adds on top of the host tree.  The lockfile merge keeps
# the plugin's own build of these and takes upstream's for everything else.
PLUGIN_ONLY = {
    "which",
    "pyo3",
    "pyo3-build-config",
    "pyo3-ffi",
    "pyo3-macros",
    "pyo3-macros-backend",
}


def slug(url: str) -> str:
    """Repository identity of a git URL; `git@` userinfo is preserved because
    cargo counts it as part of a source's identity."""
    return SLUG_GIT_SUFFIX.sub("", SLUG_SCHEME.sub("", url))


def read(path: str | Path) -> str:
    return Path(path).read_text(encoding="utf-8")


def load_toml(text: str) -> dict:
    try:
        return tomllib.loads(text)
    except tomllib.TOMLDecodeError as e:
        raise Unparseable(f"cannot parse a manifest: {e}") from e


def write_text(path: str | Path, text: str, eol: str = "\n") -> None:
    if eol == "\r\n":
        text = text.replace("\r\n", "\n").replace("\n", "\r\n")
    Path(path).write_bytes(text.encode("utf-8"))


def emit(**kv) -> None:
    for key, value in kv.items():
        print(f"{key}={value}")
    out = os.environ.get("GITHUB_OUTPUT")
    if out:
        with open(out, "a", encoding="utf-8") as handle:
            for key, value in kv.items():
                handle.write(f"{key}={value}\n")


# --------------------------------------------------------------------------
# lockfiles
# --------------------------------------------------------------------------


def lock_packages(lock_text: str) -> dict[str, list[dict]]:
    """Every [[package]] stanza, keyed by package name, in file order."""
    blocks = re.findall(r"\[\[package\]\]\n(.*?(?=\n\[\[package\]\]\n|$))", lock_text, re.S)
    packages: dict[str, list[dict]] = {}
    for block in blocks:
        name = re.search(r'^name = "([^"]+)"', block, re.M)
        version = re.search(r'^version = "([^"]+)"', block, re.M)
        source = re.search(r'^source = "([^"]+)"', block, re.M)
        if not (name and version):
            raise Unparseable(f"lockfile stanza without name/version: {block[:80]!r}")
        packages.setdefault(name.group(1), []).append(
            {
                "version": version.group(1),
                "source": source.group(1) if source else None,
                "block": block,
            }
        )
    return packages


def locked_package(lock_text: str, name: str) -> dict:
    stanzas = lock_packages(lock_text).get(name, [])
    if not stanzas:
        raise Unparseable(f"no [[package]] named {name!r} in the lockfile")
    if len(stanzas) > 1:
        raise Escalate(
            f"{len(stanzas)} lockfile stanzas named {name!r} — two builds of the codec in one graph"
        )
    return stanzas[0]


def git_source(pkg: dict, what: str) -> tuple[str, str]:
    source = pkg.get("source")
    if not source or not source.startswith("git+"):
        raise Unparseable(f"{what} is not locked to a git source: {source!r}")
    match = GIT_SOURCE.search(source)
    if not match:
        raise Unparseable(f"cannot read the git source of {what}: {source!r}")
    return match.group("url"), match.group("rev")


# --------------------------------------------------------------------------
# manifests
# --------------------------------------------------------------------------


def dep_table(manifest: dict, table: str) -> dict:
    value = manifest.get(table, {})
    if not isinstance(value, dict):
        raise Unparseable(f"[{table}] is not a table")
    return value


def git_deps(table: dict) -> list[tuple[str, dict]]:
    return [
        (key, dep)
        for key, dep in table.items()
        if isinstance(dep, dict) and "git" in dep
    ]


def codec_dep_of_plugin_api(plugin_api_manifest: dict) -> dict:
    """The codec crate: the single git dependency ocs_plugin_api declares.

    The dependency key has been respelled upstream (acadrust -> codec) and
    the crate itself was renamed (acadrust -> opencadcodec at v2026.39), so
    the only stable identity is "the git dependency of ocs_plugin_api"."""
    found: dict[tuple[str, str, str], None] = {}
    for table in ("dependencies", "build-dependencies"):
        for key, dep in git_deps(dep_table(plugin_api_manifest, table)):
            if "rev" not in dep:
                raise Escalate(
                    f"ocs_plugin_api's {key} dependency is pinned to a branch or tag, "
                    "not a rev — a moving pin ships a different build on every rebuild"
                )
            found.setdefault((dep.get("package", key), dep["git"], dep["rev"]), None)
    if not found:
        raise Unparseable(
            "ocs_plugin_api declares no git dependency — cannot tell which crate is the codec"
        )
    if len(found) > 1:
        raise Unparseable(
            f"ocs_plugin_api declares {len(found)} distinct git dependencies — "
            "cannot tell which one is the codec"
        )
    (package, url, rev), _ = next(iter(found.items()))
    return {"package": package, "url": url, "rev": rev}


def patch_entry(manifest: dict, package: str) -> tuple[str, str, str] | None:
    """The [patch."URL"] table redirecting the codec package, if any."""
    patches = manifest.get("patch", {})
    if not isinstance(patches, dict):
        raise Unparseable("[patch] is not a table")
    found = []
    for key, table in patches.items():
        if not isinstance(table, dict):
            continue
        for dep_key, dep in table.items():
            real = dep.get("package", dep_key) if isinstance(dep, dict) else dep_key
            if real == package:
                found.append((key, dep))
    if not found:
        return None
    if len(found) > 1:
        raise Escalate(
            f"the codec {package!r} is patched by {len(found)} tables — a source change, not a re-pin"
        )
    key, dep = found[0]
    if not isinstance(dep, dict) or "git" not in dep or "rev" not in dep:
        raise Escalate(
            f"the patch for {package!r} is not a git rev pin — a registry or branch "
            "patch is a source change, not a re-pin"
        )
    return key, dep["git"], dep["rev"]


def host_plan(plugin_api_manifest: dict, host_manifest: dict, host_lock: str) -> dict:
    """Reproduce the codec build the host ships and reconcile it against its
    own lockfile: ocs_plugin_api's declaration plus the host root's [patch]
    must explain the lock, or nothing downstream can be trusted."""
    declared = codec_dep_of_plugin_api(plugin_api_manifest)
    locked = locked_package(host_lock, declared["package"])
    locked_url, locked_rev = git_source(locked, f"the host's locked {declared['package']}")
    patch = patch_entry(host_manifest, declared["package"])

    plan = {
        "codec_package": declared["package"],
        "codec_url": declared["url"],
        "codec_rev": declared["rev"],
        "locked_url": locked_url,
        "locked_rev": locked_rev,
        "locked_version": locked["version"],
        "patch_key": patch[0] if patch else "",
        "patch_url": patch[1] if patch else "",
        "patch_rev": patch[2] if patch else "",
    }

    if patch is None:
        if slug(locked_url) != slug(declared["url"]) or not locked_rev.startswith(declared["rev"]):
            raise Unparseable(
                f"{declared['package']} is locked to {locked_url}@{locked_rev} but "
                f"ocs_plugin_api declares {declared['url']}@{declared['rev']} with no "
                "patch explaining the difference — something else is redirecting the source"
            )
        return plan

    key, url, rev = patch
    if slug(key) != slug(declared["url"]):
        raise Escalate(
            f"the host patches {key!r} but ocs_plugin_api declares {declared['url']!r} — "
            "mirroring the patch would still leave two codec sources in the graph"
        )
    if slug(url) != slug(locked_url) or not locked_rev.startswith(rev):
        raise Unparseable(
            f"the patch redirects to {url}@{rev} but the host lockfile has "
            f"{locked_url}@{locked_rev} — the patch does not explain the lock"
        )
    return plan


def our_codec_dep(cargo: dict) -> dict:
    """The plugin's codec dependency.  The key `acadrust` is the plugin's own
    naming — src/ says `use acadrust::` — and `package =` renames the crate
    when upstream renames it, keeping the plugin source stable."""
    deps = dep_table(cargo, "dependencies")
    if "acadrust" not in deps or not isinstance(deps["acadrust"], dict):
        raise Unparseable("the plugin manifest no longer declares the `acadrust` dependency key")
    dep = deps["acadrust"]
    if "git" not in dep or "rev" not in dep:
        raise Escalate("the plugin's codec dependency is not a git rev pin — restore it before re-pinning")
    return {
        "package": dep.get("package", "acadrust"),
        "url": dep["git"],
        "rev": dep["rev"],
        "features": dep.get("features", []),
    }


# --------------------------------------------------------------------------
# gates
# --------------------------------------------------------------------------


def compat_series(version: str) -> tuple[int, int]:
    parts = version.split(".")
    if len(parts) < 2 or not all(part.isdigit() for part in parts[:2]):
        raise Unparseable(f"cannot read the semver series of {version!r}")
    return int(parts[0]), int(parts[1])


def gate_series(codec_package: str, host_lock: str, our_cargo: dict, our_lock: str, tag: str) -> dict:
    """A codec series move (0.4 -> 0.5) is a source migration, not a re-pin."""
    ours = our_codec_dep(our_cargo)
    ours_locked = locked_package(our_lock, ours["package"])
    host_locked = locked_package(host_lock, codec_package)
    if compat_series(ours_locked["version"]) != compat_series(host_locked["version"]):
        raise Escalate(
            f"the codec series moved from {ours_locked['version']} to "
            f"{host_locked['version']} at {tag} — that is a source migration, not a re-pin"
        )
    return {"ours": ours_locked["version"], "host": host_locked["version"]}


API_VERSION_RE = re.compile(r"pub const API_VERSION: u32 = (\d+);")
API_MIN_RE = re.compile(r"pub const API_VERSION_MIN_SUPPORTED: u32 = (\d+);")


def gate_api(manifest_rs: str, plugin_toml: dict, tag: str) -> dict:
    """The host must still accept the plugin's declared api_version.

    This plugin pins its protocol major deliberately (the V4 shared-memory
    snapshot in src/lib.rs), so the gate verifies acceptance and never
    rewrites the number: opting into a newer protocol is a source change."""
    hi = API_VERSION_RE.search(manifest_rs)
    lo = API_MIN_RE.search(manifest_rs)
    if not (hi and lo):
        raise Unparseable(
            f"cannot read API_VERSION / API_VERSION_MIN_SUPPORTED from "
            f"ocs_plugin_api's manifest.rs at {tag}"
        )
    current, minimum = int(hi.group(1)), int(lo.group(1))
    ours = plugin_toml.get("plugin", {}).get("api_version")
    if not isinstance(ours, int):
        raise Unparseable("cannot read api_version from plugin.toml")
    if not minimum <= ours <= current:
        raise Escalate(
            f"the host at {tag} accepts api_version {minimum}..{current} but the plugin "
            f"declares {ours} — the plugin's protocol major needs a source migration"
        )
    return {"current": current, "minimum": minimum, "ours": ours}


def gate_source(our_cargo: dict, plugin_api_manifest: dict, host_manifest: dict) -> dict:
    """Byte-for-byte identity with ocs_plugin_api's codec spelling, and a
    patch mirror that matches the host root's exactly (or is absent when the
    host's is)."""
    want = codec_dep_of_plugin_api(plugin_api_manifest)
    ours = our_codec_dep(our_cargo)
    if (ours["url"], ours["rev"]) != (want["url"], want["rev"]):
        raise Escalate(
            f"the plugin's codec pin is {ours['url']}@{ours['rev']} but ocs_plugin_api "
            f"declares {want['url']}@{want['rev']} byte for byte — cargo would resolve "
            "two codec sources from the same commit"
        )
    if ours["package"] != want["package"]:
        raise Escalate(
            f"the plugin depends on crate {ours['package']!r} but ocs_plugin_api depends "
            f"on {want['package']!r} — two different crates in one graph"
        )
    want_patch = patch_entry(host_manifest, want["package"])
    got_patch = patch_entry(our_cargo, ours["package"])
    if want_patch != got_patch:
        raise Escalate(
            f"the plugin's mirrored patch {got_patch!r} does not match the host's {want_patch!r}"
        )
    return {}


def gate_abi(our_cargo: dict, our_lock: str, host_rev: str, host_lock: str) -> dict:
    """After re-locking, exactly one codec package may be in the graph, at
    the revision the host is built against — and no other git source beyond
    ocs_plugin_api and what the host tree itself locks."""
    ours = our_codec_dep(our_cargo)
    slugs = {slug(ours["url"])}
    patch = patch_entry(our_cargo, ours["package"])
    if patch:
        slugs.add(slug(patch[0]))
        slugs.add(slug(patch[1]))
    locked_url, locked_rev = git_source(
        locked_package(our_lock, ours["package"]), "the plugin's locked codec"
    )
    slugs.add(slug(locked_url))

    upstream_names = set(lock_packages(host_lock))
    found = []
    for name, stanzas in lock_packages(our_lock).items():
        for pkg in stanzas:
            source = pkg.get("source")
            if not source or not source.startswith("git+"):
                continue
            match = GIT_SOURCE.search(source)
            if not match:
                continue
            if slug(match.group("url")) in slugs:
                found.append((name, match.group("rev")))
            elif name not in upstream_names and name != "ocs_plugin_api":
                raise Escalate(
                    f"unexpected git source in the graph: {name} at {match.group('url')} — "
                    "the plugin tree must not carry git crates the host does not lock"
                )

    names = {name for name, _ in found}
    if len(names) > 1:
        raise Escalate(f"two codec crates in the graph: {sorted(names)}")
    if not found:
        raise Escalate("no codec package found in the plugin's lockfile")
    name, rev = found[0]
    if rev != host_rev:
        raise Escalate(
            f"the plugin locked {name}@{rev} but the host is built against {host_rev}"
        )
    return {"name": name, "rev": rev}


# --------------------------------------------------------------------------
# rewrite
# --------------------------------------------------------------------------


def render_patch_block(package: str, patch: tuple[str, str, str] | None) -> str:
    if patch is None:
        return (
            f"{PATCH_BEGIN}\n"
            "# The host workspace does not patch the codec source at this tag. The plugin\n"
            "# must not patch it either: cargo rejects a patch that points at the same\n"
            "# source it replaces, and any other redirect would fork the codec build.\n"
            f"{PATCH_END}\n"
        )
    key, url, rev = patch
    return (
        f"{PATCH_BEGIN}\n"
        "# The host workspace redirects the codec source below. The plugin mirrors the\n"
        "# redirect byte for byte so both resolve the same codec build; a [patch] only\n"
        "# applies from the workspace root, which is why it lives here too.\n"
        f'[patch."{key}"]\n'
        f'{package} = {{ git = "{url}", rev = "{rev}" }}\n'
        f"{PATCH_END}\n"
    )


def bump_patch_version(version: str) -> str:
    parts = version.split(".")
    if len(parts) != 3 or not all(part.isdigit() for part in parts):
        raise Unparseable(f"cannot bump the plugin version {version!r}")
    return f"{parts[0]}.{parts[1]}.{int(parts[2]) + 1}"


def rewrite(
    cargo_text: str,
    plugin_text: str,
    *,
    tag: str,
    codec_package: str,
    codec_url: str,
    codec_rev: str,
    patch_key: str = "",
    patch_url: str = "",
    patch_rev: str = "",
) -> tuple[str, str, str]:
    """Rewrite the plugin manifests for the new pin; returns (cargo, plugin, newver)."""
    eol = "\r\n" if "\r\n" in cargo_text else "\n"
    if eol == "\r\n":
        cargo_text = cargo_text.replace("\r\n", "\n")
        plugin_text = plugin_text.replace("\r\n", "\n")

    # Lift the mirrored-patch region out first: it contains a line that reads
    # exactly like the codec dependency the rewrite edits, and the sentinel
    # keeps "rewrote 1" honest.
    region = PATCH_REGION.search(cargo_text)
    if not region:
        raise Unparseable(f"cannot find the {PATCH_BEGIN} .. {PATCH_END} region in Cargo.toml")
    cargo_text = cargo_text[: region.start()] + PATCH_SENTINEL + cargo_text[region.end() :]

    cargo_text, n = re.subn(
        r'^(\[package\.metadata\.upstream\]\s*\n)(?:tag|rev) = "[^"]+"[ \t]*\n',
        rf'\g<1>tag = "{tag}"\n',
        cargo_text,
        flags=re.M,
    )
    if n != 1:
        raise Unparseable(f"expected 1 upstream metadata entry, rewrote {n}")

    ocs_value = (
        f'{{ git = "https://github.com/{UPSTREAM_REPO}", tag = "{tag}", features = ["host"] }}'
    )
    cargo_text, n = re.subn(
        r"^ocs_plugin_api = \{[^}]+\}",
        f"ocs_plugin_api = {ocs_value}",
        cargo_text,
        count=2,
        flags=re.M,
    )
    if n != 2:
        raise Unparseable(f"expected 2 ocs_plugin_api entries, rewrote {n}")

    # The codec dependency: url and rev verbatim from ocs_plugin_api (never
    # the lockfile's expanded sha), `package =` when the crate was renamed,
    # features preserved so adopting one does not need a workflow change.
    features = re.search(r"^acadrust = \{[^}]*?features = (\[[^\]]*\])", cargo_text, re.M)
    features_part = f", features = {features.group(1)}" if features else ""
    package_part = f'package = "{codec_package}", ' if codec_package != "acadrust" else ""
    codec_value = f'{{ {package_part}git = "{codec_url}", rev = "{codec_rev}"{features_part} }}'
    cargo_text, n = re.subn(
        r"^acadrust = \{[^}]+\}",
        f"acadrust = {codec_value}",
        cargo_text,
        count=1,
        flags=re.M,
    )
    if n != 1:
        raise Unparseable(f"expected 1 codec dependency entry, rewrote {n}")

    old = re.search(r'^version = "([^"]+)"', cargo_text, re.M)
    if not old:
        raise Unparseable("cannot read the plugin version from Cargo.toml")
    newver = bump_patch_version(old.group(1))
    cargo_text = re.sub(r'^version = "[^"]+"', f'version = "{newver}"', cargo_text, count=1, flags=re.M)
    plugin_text = re.sub(r'^version = "[^"]+"', f'version = "{newver}"', plugin_text, count=1, flags=re.M)

    patch = (patch_key, patch_url, patch_rev) if patch_key else None
    cargo_text = cargo_text.replace(PATCH_SENTINEL, render_patch_block(codec_package, patch))

    if eol == "\r\n":
        cargo_text = cargo_text.replace("\n", "\r\n")
        plugin_text = plugin_text.replace("\n", "\r\n")
    return cargo_text, plugin_text, newver


# --------------------------------------------------------------------------
# lockfile merge and lockstep verification
# --------------------------------------------------------------------------


def merge_lock(
    upstream_lock: str,
    our_lock: str,
    *,
    new_version: str,
    host_manifest: dict,
    plugin_name: str = "ocs_python_repl",
) -> str:
    """Take the host's lockfile wholesale, keep the plugin's own package and
    its plugin-only extras, and drop the host's root package: the plugin
    replaces it as the workspace root."""
    host_package = host_manifest.get("package", {}).get("name")
    if not isinstance(host_package, str) or not host_package:
        raise Unparseable("cannot read the host root package name from its Cargo.toml")

    header_match = re.match(r"(#.*?\nversion = \d+\n)", upstream_lock, re.S)
    header = header_match.group(1) if header_match else (
        "# This file is automatically @generated by Cargo.\n"
        "# It is not intended for manual editing.\n"
        "version = 4\n"
    )
    upstream_blocks = [
        block.strip()
        for block in re.findall(r"\[\[package\]\]\n(.*?(?=\n\[\[package\]\]\n|$))", upstream_lock, re.S)
    ]
    local_blocks = re.split(r"\n\[\[package\]\]\n", our_lock)

    keep_blocks: list[str] = []
    our_block: str | None = None
    for block in local_blocks:
        match = re.search(r'^name = "([^"]+)"', block, re.M)
        if not match:
            continue
        name = match.group(1)
        if name in PLUGIN_ONLY:
            keep_blocks.append(block.strip())
        elif name == plugin_name:
            our_block = block.strip()
    if our_block is None:
        raise Unparseable(f"{plugin_name} package not found in the plugin's Cargo.lock")

    our_block, n = re.subn(
        r'^version = "[^"]+"', f'version = "{new_version}"', our_block, count=1, flags=re.M
    )
    if n != 1:
        raise Unparseable("cannot rewrite the plugin's own version inside its Cargo.lock block")

    blocks: list[str] = []
    placed = False
    for block in upstream_blocks:
        match = re.search(r'^name = "([^"]+)"', block, re.M)
        if not match:
            raise Unparseable("upstream lockfile block without a name")
        name = match.group(1)
        if name == host_package:
            blocks.append(our_block)
            placed = True
        elif name in PLUGIN_ONLY or name == plugin_name:
            continue
        else:
            blocks.append(block)
    if not placed:
        blocks.append(our_block)
    blocks.extend(keep_blocks)

    return header + "\n[[package]]\n" + "\n[[package]]\n".join(blocks) + "\n"


def verify_lockstep(upstream_lock: str, our_lock: str) -> list[str]:
    """Every package the plugin shares with the host tree must match it at
    the patch level."""
    upstream: dict[str, set[str]] = {}
    for name, stanzas in lock_packages(upstream_lock).items():
        upstream[name] = {stanza["version"] for stanza in stanzas}

    mismatches = []
    for name, stanzas in sorted(lock_packages(our_lock).items()):
        if name == "ocs_python_repl" or name in PLUGIN_ONLY:
            continue
        if name not in upstream:
            continue
        for stanza in stanzas:
            if stanza["version"] not in upstream[name]:
                mismatches.append(f"{name} {stanza['version']} (upstream has {sorted(upstream[name])})")
    return mismatches


def inject_source(our_cargo: dict, our_lock: str, plugin_text: str, codec_package: str | None = None) -> tuple[str, str]:
    """Record the resolved codec source in plugin.toml's [opencad] table.

    The field keeps its historical name `acadrust_source`: it is the
    fingerprint of whatever crate carries the entity types this week."""
    if codec_package is None:
        codec_package = our_codec_dep(our_cargo)["package"]
    source = locked_package(our_lock, codec_package).get("source")
    if not source or not source.startswith("git+"):
        raise Unparseable(f"{codec_package!r} is not locked to a git source: {source!r}")
    if "[opencad]" not in plugin_text:
        plugin_text = plugin_text.rstrip() + "\n\n[opencad]\n"
    plugin_text, n = re.subn(
        r'^(\[opencad\]\s*\n)(?:acadrust_source\s*=\s*"[^"]*"\s*\n)?',
        rf'\g<1>acadrust_source = "{source}"\n',
        plugin_text,
        flags=re.M,
    )
    if n == 0:
        plugin_text = re.sub(
            r"^(\[opencad\]\s*\n)",
            rf'\g<1>acadrust_source = "{source}"\n',
            plugin_text,
            flags=re.M,
        )
    return plugin_text, source


def current_pin(cargo_text: str) -> dict:
    """The pin we ship today. The tag is read from
    [package.metadata.upstream] and must agree with the tag= spelled in the
    ocs_plugin_api dependency lines — two spellings of one fact that drift
    silently are worse than one."""
    cargo = load_toml(cargo_text)
    metadata = cargo.get("package", {}).get("metadata", {}).get("upstream", {})
    tag = metadata.get("tag") if isinstance(metadata, dict) else None
    if not isinstance(tag, str) or not tag:
        raise Unparseable("cannot read [package.metadata.upstream].tag from Cargo.toml")
    dep_tags = set(re.findall(r'^ocs_plugin_api = \{[^}]*?\btag = "([^"]+)"', cargo_text, re.M))
    if dep_tags != {tag}:
        raise Unparseable(
            f"[package.metadata.upstream].tag is {tag!r} but the ocs_plugin_api "
            f"dependency pins {sorted(dep_tags)!r} — the pins drifted"
        )
    dep = our_codec_dep(cargo)
    version = cargo.get("package", {}).get("version")
    return {
        "tag": tag,
        "package": dep["package"],
        "url": dep["url"],
        "rev": dep["rev"],
        "version": version or "",
    }


# --------------------------------------------------------------------------
# CLI
# --------------------------------------------------------------------------


def guarded(fn, **kwargs):
    try:
        return fn(**kwargs)
    except Escalate as exc:
        print(f"-> ESCALATE: {exc}", file=sys.stderr)
        sys.exit(ESCALATE)
    except Unparseable as exc:
        print(f"-> UNPARSEABLE: {exc}", file=sys.stderr)
        sys.exit(UNPARSEABLE)


def main(argv: list[str] | None = None) -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = parser.add_subparsers(dest="command", required=True)

    p = sub.add_parser("current-pin", help="print the pin we ship today")
    p.add_argument("--cargo", default="Cargo.toml")

    p = sub.add_parser("host-plan", help="reproduce the codec source the host resolves")
    p.add_argument("--plugin-api-manifest", required=True)
    p.add_argument("--host-manifest", required=True)
    p.add_argument("--host-lock", required=True)

    p = sub.add_parser("gate-series", help="codec series must not move")
    p.add_argument("--codec-package", required=True)
    p.add_argument("--host-lock", required=True)
    p.add_argument("--tag", required=True)
    p.add_argument("--cargo", default="Cargo.toml")
    p.add_argument("--our-lock", default="Cargo.lock")

    p = sub.add_parser("gate-api", help="host must still accept our api_version")
    p.add_argument("--manifest-rs", required=True)
    p.add_argument("--tag", required=True)
    p.add_argument("--plugin-toml", default="plugin.toml")

    p = sub.add_parser("gate-source", help="codec spelling must match ocs_plugin_api byte for byte")
    p.add_argument("--plugin-api-manifest", required=True)
    p.add_argument("--host-manifest", required=True)
    p.add_argument("--cargo", default="Cargo.toml")

    p = sub.add_parser("gate-abi", help="exactly one codec, at the host's rev")
    p.add_argument("--host-rev", required=True)
    p.add_argument("--host-lock", required=True)
    p.add_argument("--cargo", default="Cargo.toml")
    p.add_argument("--our-lock", default="Cargo.lock")

    p = sub.add_parser("rewrite", help="rewrite the manifests for the new pin")
    p.add_argument("--tag", required=True)
    p.add_argument("--codec-package", required=True)
    p.add_argument("--codec-url", required=True)
    p.add_argument("--codec-rev", required=True)
    p.add_argument("--patch-key", default="")
    p.add_argument("--patch-url", default="")
    p.add_argument("--patch-rev", default="")
    p.add_argument("--cargo", default="Cargo.toml")
    p.add_argument("--plugin", default="plugin.toml")

    p = sub.add_parser("merge-lock", help="merge the upstream lockfile into ours")
    p.add_argument("--upstream-lock", required=True)
    p.add_argument("--host-manifest", required=True)
    p.add_argument("--new-version", required=True)
    p.add_argument("--our-lock", default="Cargo.lock")

    p = sub.add_parser("verify-lockstep", help="shared dependencies must match upstream")
    p.add_argument("--upstream-lock", required=True)
    p.add_argument("--our-lock", default="Cargo.lock")

    p = sub.add_parser("inject-source", help="record the codec source in plugin.toml")
    p.add_argument("--codec-package", default=None)
    p.add_argument("--cargo", default="Cargo.toml")
    p.add_argument("--our-lock", default="Cargo.lock")
    p.add_argument("--plugin-toml", default="plugin.toml")

    args = vars(parser.parse_args(argv))
    command = args.pop("command")

    if command == "current-pin":
        pin = guarded(
            current_pin,
            cargo_text=read(args.pop("cargo")),
        )
        emit(
            current_tag=pin["tag"],
            current_package=pin["package"],
            current_url=pin["url"],
            current_rev=pin["rev"],
            current_version=pin["version"],
        )
    elif command == "host-plan":
        plan = guarded(
            host_plan,
            plugin_api_manifest=load_toml(read(args.pop("plugin_api_manifest"))),
            host_manifest=load_toml(read(args.pop("host_manifest"))),
            host_lock=read(args.pop("host_lock")),
        )
        emit(**plan)
    elif command == "gate-series":
        result = guarded(
            gate_series,
            codec_package=args.pop("codec_package"),
            host_lock=read(args.pop("host_lock")),
            our_cargo=load_toml(read(args.pop("cargo"))),
            our_lock=read(args.pop("our_lock")),
            tag=args.pop("tag"),
        )
        emit(series_ours=result["ours"], series_host=result["host"])
    elif command == "gate-api":
        result = guarded(
            gate_api,
            manifest_rs=read(args.pop("manifest_rs")),
            plugin_toml=load_toml(read(args.pop("plugin_toml"))),
            tag=args.pop("tag"),
        )
        emit(api_current=result["current"], api_minimum=result["minimum"], api_ours=result["ours"])
    elif command == "gate-source":
        guarded(
            gate_source,
            our_cargo=load_toml(read(args.pop("cargo"))),
            plugin_api_manifest=load_toml(read(args.pop("plugin_api_manifest"))),
            host_manifest=load_toml(read(args.pop("host_manifest"))),
        )
        emit(source="ok")
    elif command == "gate-abi":
        result = guarded(
            gate_abi,
            our_cargo=load_toml(read(args.pop("cargo"))),
            our_lock=read(args.pop("our_lock")),
            host_rev=args.pop("host_rev"),
            host_lock=read(args.pop("host_lock")),
        )
        emit(codec_name=result["name"], codec_rev=result["rev"])
    elif command == "rewrite":
        cargo_path = args.pop("cargo")
        plugin_path = args.pop("plugin")
        tag = args.pop("tag")
        cargo_text, plugin_text, newver = guarded(
            rewrite,
            cargo_text=read(cargo_path),
            plugin_text=read(plugin_path),
            tag=tag,
            codec_package=args.pop("codec_package"),
            codec_url=args.pop("codec_url"),
            codec_rev=args.pop("codec_rev"),
            patch_key=args.pop("patch_key"),
            patch_url=args.pop("patch_url"),
            patch_rev=args.pop("patch_rev"),
        )
        write_text(cargo_path, cargo_text)
        write_text(plugin_path, plugin_text)
        emit(newver=newver)
    elif command == "merge-lock":
        our_lock_path = args.pop("our_lock")
        merged = guarded(
            merge_lock,
            upstream_lock=read(args.pop("upstream_lock")),
            our_lock=read(our_lock_path),
            new_version=args.pop("new_version"),
            host_manifest=load_toml(read(args.pop("host_manifest"))),
        )
        write_text(our_lock_path, merged)
        emit(merged="ok")
    elif command == "verify-lockstep":
        mismatches = guarded(
            verify_lockstep,
            upstream_lock=read(args.pop("upstream_lock")),
            our_lock=read(args.pop("our_lock")),
        )
        if mismatches:
            print("Dependency mismatch with the upstream Cargo.lock:", file=sys.stderr)
            for mismatch in mismatches:
                print(f"  {mismatch}", file=sys.stderr)
            sys.exit(ESCALATE)
        emit(lockstep="ok")
    elif command == "inject-source":
        plugin_path = args.pop("plugin_toml")
        plugin_text, source = guarded(
            inject_source,
            our_cargo=load_toml(read(args.pop("cargo"))),
            our_lock=read(args.pop("our_lock")),
            plugin_text=read(plugin_path),
            codec_package=args.pop("codec_package"),
        )
        write_text(plugin_path, plugin_text)
        emit(codec_source=source)


if __name__ == "__main__":
    main()
