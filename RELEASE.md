# Release Process

This document describes how `ocs_python_repl` is released and how its dependencies are kept in lock-step with upstream Open CAD Studio releases.

## Overview

`schoeller/ocs_python_repl` is a standalone plugin crate that tracks upstream `HakanSeven12/OpenCADStudio` releases. The plugin must be built against the **exact** same `ocs_plugin_api` commit, `acadrust` revision, and registry dependency versions as the shipped host, because the V4 shared-memory snapshot and the plugin-host wire protocol are laid out by those crates. A mismatch can produce ABI crashes (for example, `slice::from_raw_parts` failures).

The repository uses three coordinated mechanisms:

1. `.upstream-tag` — the single source of truth for the upstream release pin.
2. `scripts/repin.py` — a local/manual tool to rewrite dependency pins.
3. Two GitHub Actions workflows:
   - `.github/workflows/repin.yml` — watches upstream, re-pins, bumps, tests, and tags.
   - `.github/workflows/release.yml` — builds the plugin cdylib for Linux, Windows, and macOS and attaches the assets to a GitHub Release.

---

## 1. Upstream Pin (`.upstream-tag`)

The file `.upstream-tag` at the repository root declares which upstream Open CAD Studio release the plugin is pinned to.

```text
v0.9.7
```

Both `scripts/repin.py` and the `repin` workflow read this file. When the upstream pin changes, `Cargo.toml` and `Cargo.lock` are updated to match.

---

## 2. Manual Re-Pin (`scripts/repin.py`)

`scripts/repin.py` performs a local, non-destructive re-pin. It updates files but does not commit, tag, or push.

### Usage

```bash
# Re-pin to the tag stored in .upstream-tag
python scripts/repin.py

# Re-pin to a specific upstream tag
python scripts/repin.py v0.9.8
```

### What it fetches

- `https://raw.githubusercontent.com/HakanSeven12/OpenCADStudio/<tag>/Cargo.lock`
- `https://raw.githubusercontent.com/HakanSeven12/OpenCADStudio/<tag>/crates/ocs_plugin_api/Cargo.toml`

### What it updates

- `[package.metadata.upstream]` in `Cargo.toml`
- `ocs_plugin_api` → git dependency pinned to the upstream tag
- `acadrust` → git dependency pinned to the exact upstream `rev`
- Registry dependencies pinned to exact upstream versions:
  - `serde`, `serde_json`, `bincode`, `memmap2`, `anyhow`, `getrandom`, `libc`, `windows-sys`
- `.upstream-tag`

### Why `acadrust` uses the upstream `rev` form

Cargo unifies git sources by their exact reference string. The script therefore copies the short-hash `rev` form declared in upstream's `crates/ocs_plugin_api/Cargo.toml`, not the full 40-character hash from `Cargo.lock`.

---

## 3. Automated Re-Pin (`.github/workflows/repin.yml`)

The `repin` workflow automates the logic in `scripts/repin.py` and adds validation gates.

### Triggers

| Trigger | Purpose |
|---|---|
| `schedule: 17 6 * * *` | Nightly poll of the latest upstream release. The current pin is always overwritten with the latest release tag, even if it was previously a git revision. |
| `workflow_dispatch` | Manual re-pin, optionally to a specific tag or as a dry run. |

### Jobs

#### `detect`

- Reads the current `.upstream-tag`.
- Determines the desired upstream tag:
  - Cron: queries `repos/HakanSeven12/OpenCADStudio/releases/latest` via the GitHub CLI. The latest release tag always becomes the new pin, even if `.upstream-tag` currently holds a git revision.
  - Manual: uses the tag supplied to `workflow_dispatch`.
- Resolves both the current pin and the desired tag to commit SHAs.
- Skips only if both resolve to the same commit (unless `dry_run` is true).
- Skips if a `repin/<tag>` branch already exists (indicating a pending PR).
- Outputs `repin=true` and the target tag when work is needed.

#### `repin`

Runs only when `detect` outputs `repin=true`.

1. **Derive pins**  
   Fetches upstream `Cargo.lock` and `crates/ocs_plugin_api/Cargo.toml`, then rewrites `Cargo.toml` with matching pins (same logic as `scripts/repin.py`).

2. **API version gate**  
   Reads `crates/ocs_plugin_api/src/manifest.rs` from upstream and extracts:
   - `API_VERSION`
   - `API_VERSION_MIN_SUPPORTED`

   It then checks that the plugin's `api_version` in `plugin.toml` is inside `[MIN_SUPPORTED, API_VERSION]`. If the host no longer accepts the plugin's API version, the workflow fails with a source-migration error.

3. **Bump plugin patch version**  
   Increments the patch component of `version` in both `Cargo.toml` and `plugin.toml` (for example, `0.1.6 → 0.1.7`). Also updates `plugin.toml` `api_version` to the host's `API_VERSION` when needed.

4. **Diff gate**  
   Verifies that only these files changed:
   - `Cargo.toml`
   - `plugin.toml`
   - `Cargo.lock`
   - `.upstream-tag`

5. **Build and test**  
   ```bash
   cargo generate-lockfile
   cargo build --release --locked
   cargo test --locked
   ```

6. **Host smoke test**  
   - Clones the upstream host at the target tag.
   - Builds the host.
   - Loads the freshly built plugin into a temporary plugins directory.
   - Runs the host in headless serve mode and verifies `new` and `entities` commands return `ok`.

7. **Dry-run stop**  
   If `inputs.dry_run` is true, the workflow stops here after reporting success.

8. **Open PR**  
   Creates a branch `repin/<tag>`, commits the four files, pushes it, and opens a pull request with a summary of the old and new pins.

9. **Merge and tag**  
   Auto-merges the PR, deletes the branch, tags the resulting `main` commit with the new plugin version (for example, `v0.1.7`), and pushes the tag.

#### `release`

Calls `.github/workflows/release.yml` via `workflow_call`, passing the newly created tag. This is required because tags pushed by `GITHUB_TOKEN` do not fire normal `push` triggers.

#### `escalate`

If any gate fails, opens or updates a GitHub issue titled `repin blocked: host <tag> needs a human` with a link to the failing run and a checklist of possible causes.

---

## 4. Release Build (`.github/workflows/release.yml`)

The `release` workflow builds the plugin cdylib for each desktop platform and attaches the binaries plus `plugin.toml` to a GitHub Release.

### Triggers

| Trigger | Purpose |
|---|---|
| `push: tags: ["v*"]` | A human pushed a version tag |
| `workflow_call` with `tag` | Called by `repin.yml` after an automated re-pin |

### Build matrix

| OS | Extension | Release asset |
|---|---|---|
| `ubuntu-latest` | `.so` | `opencad.python_repl-linux-x86_64.so` |
| `windows-latest` | `.dll` | `opencad.python_repl-windows-x86_64.dll` |
| `macos-latest` | `.dylib` | `opencad.python_repl-macos-aarch64.dylib` |

### Steps per job

1. **Checkout** the tag.
2. **Install** the stable Rust toolchain.
3. **Version consistency check** — verifies the tag (without the leading `v`) matches:
   - `version` in `Cargo.toml`
   - `version` in `plugin.toml`
4. **Set up Python** 3.12 for PyO3.
5. **Build** the release cdylib:
   ```bash
   cargo build --release --locked
   ```
6. **Stage assets** — copies the built cdylib to `dist/<canonical-name>` and copies `plugin.toml` to `dist/plugin.toml`.
7. **Create GitHub Release** — uses `softprops/action-gh-release@v3` to attach the platform binary and `plugin.toml` to the release.

---

## 5. Manual Release Checklist

If you need to cut a release by hand instead of through the automated re-pin workflow:

1. Ensure `Cargo.toml`, `plugin.toml`, and `Cargo.lock` all agree on the version.
2. Ensure the dependency pins match the upstream tag in `.upstream-tag` (run `scripts/repin.py` if unsure).
3. Commit the changes.
4. Create an annotated tag:
   ```bash
   git tag -a v0.1.x -m "ocs_python_repl v0.1.x"
   ```
5. Push the tag:
   ```bash
   git push origin v0.1.x
   ```
6. Monitor the release run at `https://github.com/schoeller/ocs_python_repl/actions`.

---

## 6. Design Rationale

- **Tag-driven releases**: Only `v*` tags trigger publication, making releases explicit and auditable.
- **Mechanical re-pins are automatic**: If upstream publishes a new release and all gates pass, the bot can merge, tag, and release without human intervention.
- **Source migrations are gated**: API version mismatches stop the bot and create an escalation issue, because they require plugin code changes, not just dependency bumps.
- **ABI matching is paramount**: Every pinned dependency is chosen to ensure the plugin's binary interface matches the host's.
- **Token-trigger limitation**: The `repin` workflow calls `release` via `workflow_call` because `GITHUB_TOKEN` pushes do not fire `push` events.
