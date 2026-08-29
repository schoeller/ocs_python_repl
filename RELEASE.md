# Release Process

This document describes how `ocs_python_repl` is released and how its dependencies are kept in lock-step with upstream Open CAD Studio releases.

## Overview

`schoeller/ocs_python_repl` is a standalone plugin crate that tracks upstream `HakanSeven12/OpenCADStudio` releases. The plugin must be built against the **exact** same `ocs_plugin_api` commit, `acadrust` revision, and registry dependency versions as the shipped host, because the V4 shared-memory snapshot and the plugin-host wire protocol are laid out by those crates. A mismatch can produce ABI crashes (for example, `slice::from_raw_parts` failures).

The repository uses two coordinated mechanisms:

1. `.upstream-tag` — the single source of truth for the upstream release pin.
2. GitHub Actions workflows in `.github/workflows/`:
   - `.github/workflows/nightly-check.yml` — polls upstream for new releases and triggers re-pins.
   - `.github/workflows/repin.yml` — fetches upstream manifests, re-pins, validates, and tags.
   - `.github/workflows/release.yml` — builds the plugin cdylib for Linux, Windows, and macOS and attaches the assets to a GitHub Release.

---

## 1. Upstream Pin (`.upstream-tag`)

The file `.upstream-tag` at the repository root declares which upstream Open CAD Studio release the plugin is pinned to.

```text
v0.9.8
```

The `repin` workflow reads this file and overwrites it when a new upstream release is adopted.

---

## 2. Automated Re-Pin (`.github/workflows/repin.yml`)

The `repin` workflow is the only supported way to re-pin dependencies. It can be triggered by `nightly-check.yml` or manually via `workflow_dispatch`.

### What it fetches (from the upstream tag tree)

- `https://github.com/HakanSeven12/OpenCADStudio/blob/<tag>/Cargo.lock`
- `https://github.com/HakanSeven12/OpenCADStudio/blob/<tag>/Cargo.toml`
- `https://github.com/HakanSeven12/OpenCADStudio/blob/<tag>/crates/ocs_plugin_api/Cargo.toml`

The files are fetched via the GitHub API (`repos/.../contents/...`) with the exact upstream release tag.

### What it updates

- `.upstream-tag` — written to the new upstream release tag.
- `Cargo.toml`:
  - `[package.metadata.upstream]` tag
  - `ocs_plugin_api` git dependency pinned to the upstream tag
  - `acadrust` git dependency pinned to the upstream resolved revision
  - `[patch."https://github.com/HakanSeven12/cadcodec.git"]` so `ocs_plugin_api` resolves the same `acadrust` revision as the host
- `Cargo.lock` — merged with the upstream `Cargo.lock` so all shared dependencies match the host tree at the patch level.
- `plugin.toml` — `acadrust_source` is overwritten with the resolved `acadrust` source from `Cargo.lock`.

### Gates

1. **Dependency verification** — every package used by the plugin that also exists in the upstream `Cargo.lock` must have the same version.
2. **Build** — `cargo build --release` (which also resolves the merged lockfile against the local manifest).
3. **Tests** — `cargo test --locked`.
4. **Host smoke test** — clones the upstream host at the target tag, builds it, and loads the plugin to verify `new` and `entities` commands return `ok`.
5. **Dry-run stop** — if `inputs.dry_run` is true, the workflow stops after the gates and reports success.

### Commit, merge, tag and release

After all gates pass:

1. Bumps the plugin patch version in `Cargo.toml` and `plugin.toml`.
2. Creates a branch `repin/<tag>`, commits the four files, and pushes it.
3. Opens and auto-merges a pull request.
4. Tags the resulting `main` commit with the new plugin version (for example, `v0.1.21`).
5. Calls `.github/workflows/release.yml` via `workflow_call` to build and publish the release assets.

If any gate fails, an escalation issue titled `repin blocked: host <tag> needs a human` is opened or updated with a link to the failing run.

---

## 3. Nightly Upstream Check (`.github/workflows/nightly-check.yml`)

The `nightly-check` workflow runs on a schedule and via `workflow_dispatch`:

1. Queries `repos/HakanSeven12/OpenCADStudio/releases/latest` for the latest upstream release tag.
2. Compares it with `.upstream-tag`.
3. Skips if they already match or if a `repin/<tag>` branch already exists.
4. Otherwise triggers `.github/workflows/repin.yml` with `upstream_tag=<latest>`.

---

## 4. Release Build (`.github/workflows/release.yml`)

The `release` workflow builds the plugin cdylib for each desktop platform and attaches the binaries plus `plugin.toml` to a GitHub Release.

### Triggers

| Trigger | Purpose |
|---|---|
| `push: tags: ["v*"]` | A human pushed a version tag. |
| `workflow_call` with `tag` | Called by `repin.yml` after an automated re-pin. |

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
6. **Linux libpython check** — verifies the Linux `.so` links `libpython`.
7. **Stage assets** — copies the built cdylib to `dist/<canonical-name>` and copies `plugin.toml` to `dist/plugin.toml`.
8. **Inject acadrust fingerprint** — reads `Cargo.lock` and writes `acadrust_source` into `dist/plugin.toml`.
9. **Create GitHub Release** — uses `softprops/action-gh-release@v2` to attach the platform binary and `plugin.toml` to the release.

---

## 5. Manual Release Checklist

If you need to cut a release by hand instead of through the automated re-pin workflow:

1. Ensure `Cargo.toml`, `plugin.toml`, and `Cargo.lock` all agree on the version.
2. Ensure `.upstream-tag` matches the upstream release the plugin is built against.
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
- **Source migrations are gated**: ABI mismatches or dependency mismatches stop the bot and create an escalation issue, because they require plugin code changes, not just dependency bumps.
- **ABI matching is paramount**: The upstream `Cargo.lock` is merged into the local lockfile so every shared dependency matches the host at the patch level.
- **Token-trigger limitation**: The `repin` workflow calls `release` via `workflow_call` because `GITHUB_TOKEN` pushes do not fire `push` events.
