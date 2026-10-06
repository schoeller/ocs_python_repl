#!/usr/bin/env python3
"""Record compiler provenance from the published host release run.

The host's Plugin Manager gates API v4+ plugins on their declared
`rustc_version` (byte-compared with the host's build-time compiler — Rust has
no stable ABI) and their declared `acadrust_source`. This script finds the
upstream release run that published the Windows asset of the given host tag,
scrapes the one `rustc --version` line its native jobs actually ran with, and
records it in `host-build.json` plus the channel in `rust-toolchain.toml`, so
every workflow that builds this plugin uses the compiler the shipped host
was built with.

Runs on GitHub Actions with the default GITHUB_TOKEN (public upstream repo).
Exits non-zero when provenance is ambiguous: a wrong compiler record is worse
than a blocked re-pin.
"""

import argparse
import json
import re
import subprocess
from datetime import datetime, timedelta
from pathlib import Path

REPO = "HakanSeven12/OpenCADStudio"
RELEASE_WORKFLOWS = ("release", "weekly release")
# How long before the release exists a release run may have started. A
# scheduled weekly release starts on main, then its prepare job commits and
# tags the release, so the run predates the release by minutes; a wide margin
# costs nothing.
RUN_LEAD = timedelta(minutes=30)


def compiler_from_log(log):
    # Only standalone rustc output, not rustup's 'updated ... (from ...)' lines
    # and not the containerized snap build, which pins its own toolchain.
    versions = set(re.findall(r"\t(?:\d{4}-\S+ )?rustc (\d+\.\d+\.\d+ \([0-9a-f]+ \d{4}-\d{2}-\d{2}\))\s*$", log, re.M))
    if len(versions) != 1:
        raise ValueError(f"Expected one compiler across release platforms, found {sorted(versions)}")
    return "rustc " + versions.pop()


def gh(*args):
    return subprocess.check_output(["gh", *args], text=True, encoding="utf-8")


def parse_time(text):
    return datetime.fromisoformat(text.replace("Z", "+00:00"))


def windows_asset(release):
    assets = [a for a in release["assets"] if a["name"].endswith("-windows-x86_64-portable.exe")]
    if len(assets) != 1 or not re.fullmatch(r"sha256:[0-9a-f]{64}", assets[0].get("digest") or ""):
        raise ValueError("Expected one checksummed Windows portable release asset")
    return assets[0]


def select_release_run(runs, jobs_by_run, asset_uploaded_at):
    """Pick the run that published the Windows asset we are about to record.

    Runs are filed under the commit they STARTED on, which for a scheduled
    weekly release is the parent of the release commit, and a retry may start
    on any later main commit — so the tagged commit identifies nothing. The
    workflow re-uploads every asset on retries, so the bytes on the release
    come from whichever native build finished last. The only fact that ties a
    run to the published asset is that the asset's upload time falls inside
    that run's lifetime; require exactly one such run, and require its
    native jobs to have passed.
    """
    uploaded = parse_time(asset_uploaded_at)
    candidates = []
    for run in runs:
        if run["name"].lower() not in RELEASE_WORKFLOWS:
            continue
        if not parse_time(run["created_at"]) <= uploaded <= parse_time(run["updated_at"]):
            continue
        jobs = {j["name"]: j["conclusion"] for j in jobs_by_run[run["id"]]}
        for wanted in ("build-windows", "verify"):
            if not any(name.strip().endswith(wanted) and conclusion == "success"
                       for name, conclusion in jobs.items()):
                raise ValueError(
                    f"Release run {run['id']} uploaded the Windows asset but no successful "
                    f"'{wanted}' job is in {sorted(jobs)}; verify artifact provenance manually"
                )
        candidates.append(run)
    if len(candidates) != 1:
        raise ValueError(
            "Expected one release run to have published the Windows asset, found "
            f"{[r['id'] for r in candidates]}; verify artifact provenance manually"
        )
    return candidates[0]


def write_host_record(root, info):
    """Write both provenance files with LF endings on every platform; the
    release build byte-compares what it derives from them."""
    (root / "host-build.json").write_text(
        json.dumps(info, indent=2) + "\n", encoding="utf-8", newline="\n")
    channel = info["rustc_version"].split()[1]
    (root / "rust-toolchain.toml").write_text(
        '[toolchain]\nchannel = "' + channel + '"\nprofile = "minimal"\n',
        encoding="utf-8", newline="\n")


def record(tag):
    commit = json.loads(gh("api", f"repos/{REPO}/commits/{tag}"))["sha"]
    release = json.loads(gh("api", f"repos/{REPO}/releases/tags/{tag}"))
    asset = windows_asset(release)
    since = (parse_time(release["created_at"]) - RUN_LEAD).strftime("%Y-%m-%dT%H:%M:%SZ")
    # Ask each release workflow for its own runs: the repository-wide listing
    # is dominated by CI runs and a single page of it can miss a release run.
    workflows = json.loads(gh("api", f"repos/{REPO}/actions/workflows?per_page=100"))["workflows"]
    runs = []
    for workflow in workflows:
        if workflow["name"].lower() in RELEASE_WORKFLOWS:
            runs += json.loads(gh("api", "-X", "GET", f"repos/{REPO}/actions/workflows/{workflow['id']}/runs",
                                  "-f", "branch=main", "-f", f"created=>={since}", "-f", "per_page=100"))["workflow_runs"]
    jobs_by_run = {run["id"]: json.loads(gh("api", f"repos/{REPO}/actions/runs/{run['id']}/jobs?per_page=100"))["jobs"]
                  for run in runs}
    run = select_release_run(runs, jobs_by_run, asset["updated_at"])["id"]
    compiler = compiler_from_log(gh("run", "view", str(run), "--repo", REPO, "--log"))
    root = Path(__file__).resolve().parents[2]
    info = dict(tag=tag, commit=commit, rustc_version=compiler, release_run=run,
                windows_asset=asset["name"], windows_sha256=asset["digest"].split(":")[1])
    write_host_record(root, info)


if __name__ == "__main__":
    p = argparse.ArgumentParser()
    p.add_argument("--tag", required=True)
    record(p.parse_args().tag)
