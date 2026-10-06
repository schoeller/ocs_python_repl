# Release Process

This document describes how `ocs_python_repl` is released and how its dependencies are kept in lock-step with upstream Open CAD Studio releases.

## Overview

`schoeller/ocs_python_repl` is a standalone plugin crate that tracks upstream `HakanSeven12/OpenCADStudio` releases. The plugin must be built against the **exact** same `ocs_plugin_api` commit, `acadrust` revision, and registry dependency versions as the shipped host, because the V4 shared-memory snapshot and the plugin-host wire protocol are laid out by those crates. A mismatch can produce ABI crashes (for example, `slice::from_raw_parts` failures).

The repository uses these coordinated mechanisms:

1. `[package.metadata.upstream].tag` in `Cargo.toml` — the single source of truth for the upstream release pin.
2. `.github/scripts/repin_gates.py` — the parsers and gates the workflows call; unit-tested against a vendored corpus of real upstream manifests under `.github/scripts/tests/fixtures/host/`.
3. GitHub Actions workflows in `.github/workflows/`:
   - `.github/workflows/repin.yml` — runs nightly; detects new upstream releases, re-pins through the gates, and merges, tags and releases on green.
   - `.github/workflows/canary.yml` — parses upstream `main` with the same gates a night early and opens a shape-drift issue when the parsers fall behind.
   - `.github/workflows/ci.yml` — runs the gates corpus plus a `--locked` build/test and lockfile-freshness check on every push and pull request.
   - `.github/workflows/release.yml` — builds the plugin cdylib for Linux, Windows, and macOS and attaches the assets to a GitHub Release.

---

## 1. Upstream Pin (`[package.metadata.upstream].tag`)

The upstream release pin lives in `Cargo.toml`:

```toml
[package.metadata.upstream]
tag = "v2026.36"
```

The `repin` workflow rewrites it when a new upstream release is adopted. The `current-pin` gate requires it to agree with the tag spelled in the `ocs_plugin_api` dependency lines, so the two spellings of one fact cannot drift silently.

---

## 2. Automated Re-Pin (`.github/workflows/repin.yml`)

The `repin` workflow is the only supported way to re-pin dependencies. It runs on a nightly schedule (02:10 UTC) and can be dispatched manually via `workflow_dispatch`, optionally with an `upstream_tag` and a `dry_run` flag that runs every gate and ships nothing.

### The codec crate

The crate that carries the entity types is **discovered** as "the git dependency of `ocs_plugin_api`" — never hardcoded. Upstream has renamed it repeatedly (`acadrust` -> `opencadcodec` at v2026.39) and respelled its manifests regularly (a `[patch]` redirect at v0.9.8/v2026.36, none at v2026.37+, abbreviated revs at v2026.38+). The plugin keeps its `use acadrust::` source paths stable through cargo's dependency rename (`acadrust = { package = "opencadcodec", ... }`).

### What it fetches (from the upstream tag tree)

- `Cargo.toml` and `Cargo.lock` (host root)
- `crates/ocs_plugin_api/Cargo.toml`
- `crates/ocs_plugin_api/src/manifest.rs` (for the plugin API version window)

The files are fetched via the GitHub API (`repos/.../contents/...`) with the exact upstream release tag.

### What it updates

- `Cargo.toml`:
  - `[package.metadata.upstream]` tag
  - `ocs_plugin_api` git dependency pinned to the upstream tag
  - the codec git dependency pinned to **ocs_plugin_api's verbatim spelling** (never the lockfile's expanded sha — an abbreviated rev and its expansion are two different cargo sources), with `package =` added when the crate was renamed
  - the `# BEGIN MIRRORED PATCH` region — the host root's `[patch]` redirect mirrored byte for byte, or an explicit "no patch" note when the host has none (cargo rejects a patch that points at the same source it replaces)
  - the plugin patch version, bumped **before** the lockfile merge so the lockfile is never left stale
- `Cargo.lock` — merged with the upstream `Cargo.lock` so all shared dependencies match the host tree at the patch level, including the plugin's own version.
- `plugin.toml` — version bump, and `acadrust_source` overwritten with the resolved codec source from `Cargo.lock` (the field keeps its historical name; it fingerprints whatever crate carries the entity types).

### Gates, in order

1. **current-pin** — `[package.metadata.upstream].tag` and the `ocs_plugin_api` dependency pins must agree.
2. **host-plan** — ocs_plugin_api's declaration plus the host root's `[patch]` must reproduce the host lockfile's locked codec source, or nothing downstream is trusted.
3. **codec series** — a series move (0.4 -> 0.5) is a source migration, not a re-pin.
4. **api version window** — the host must still accept the plugin's declared `api_version`. The plugin pins its protocol major deliberately (the V4 shared-memory snapshot in `src/lib.rs`), so the gate verifies acceptance and never rewrites the number.
5. **rewrite + lockfile merge** — as described above.
6. **build** — `cargo build --release` (unlocked once, so cargo can adjust the merged lockfile), plus `cargo check --target` for Windows and macOS so the committed lockfile carries those targets' transitive deps.
7. **lockstep verification** — every package the plugin shares with the host tree must match the upstream `Cargo.lock` at the same version.
8. **source identity** — the plugin's codec pin must equal ocs_plugin_api's byte for byte, and the mirrored patch must match the host's exactly (or both be absent).
9. **one codec** — exactly one codec package in the graph, at the revision the host is built against, and no other git source beyond `ocs_plugin_api` and what the host tree itself locks.
10. **mechanical diff** — the re-pin may touch only `Cargo.toml`, `Cargo.lock` and `plugin.toml`.
11. **tests** — `cargo test --locked`.
12. **host smoke test** — clones the upstream host at the target tag, builds it, and loads the plugin headlessly to verify `new` and `entities` commands return `ok`.
13. **dry-run stop** — if `inputs.dry_run` is true, the workflow stops here and reports success.

The gates exit with a two-code taxonomy the escalation job relies on: `1` = a human decision is required, `2` = the upstream shape is not understood (a bug in this repository — vendor the offending manifests under `.github/scripts/tests/fixtures/host/` and teach the parser in the same PR).

### Commit, merge, tag and release

After all gates pass:

1. Creates a branch `repin/<tag>`, commits the re-pinned manifests, and pushes it.
2. Opens and auto-merges a pull request; the PR is the audit trail and the revert point.
3. Tags the resulting `main` commit with the new plugin version (for example, `v0.1.26`).
4. Calls `.github/workflows/release.yml` via `workflow_call` to build and publish the release assets.

Shipping is PR-first with a **direct-push fallback**: if pull-request creation or merge fails — most commonly because the repository setting *Allow GitHub Actions to create and approve pull requests* (Settings → Actions → Workflow permissions, off by default) denies the default token, which is exactly how the v2026.39–v2026.40.1 re-pins stalled while every build and test was green — the workflow warns, pushes the re-pin branch straight to `main` (rebasing once if `main` moved during the run), and continues to the tag. A missing audit trail must never block the release itself; the commit on `main` still carries the full pin description.

If any gate fails, an escalation job re-runs the read-only gates to classify the failure (parser gap, human decision, or downstream break), records the exact failing step name from the run, and opens or updates an issue titled `repin blocked: host <tag> needs a human` with the diagnosis, the failing step and the fix recipe.

---

## 3. Nightly Schedule and Canary

The schedule lives directly on the `repin` workflow: its `detect` job queries `repos/HakanSeven12/OpenCADStudio/releases/latest`, compares it with the current pin (`current-pin` output), skips when they match or a `repin/<tag>` branch (a pending PR) already exists, and otherwise runs the re-pin gates. A dry-run dispatch against the current pin re-runs every gate without shipping anything.

The `canary` workflow (01:41 UTC, before the re-pin) runs the same read-only gates against upstream `main`. When a gate cannot parse upstream's manifests (exit 2), it opens a shape-drift issue with lead time before the next release is cut; decision-level changes (series move, api window) are noticed but left to the re-pin's own escalation.

---

## 4. Continuous Integration (`.github/workflows/ci.yml`)

Every push to `main` and every pull request:

1. Runs the gates corpus: `python -m pytest .github/scripts/tests` against the vendored upstream manifests.
2. Builds and tests with `cargo build --release --locked` and `cargo test --locked`.
3. Enforces lockfile freshness: `cargo metadata --locked` fails if regenerating `Cargo.lock` would change anything, so a manifest bump without a lockfile refresh cannot land silently.
4. Enforces cross-platform lockfile completeness: `cargo check --target` for Windows and macOS may fail at compile time (no foreign linkers on the runner) but must not need to touch `Cargo.lock` — otherwise the next `--locked` release build on that platform would break.

---

## 5. Release Build (`.github/workflows/release.yml`)

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
8. **Inject codec fingerprint** — `repin_gates.py inject-source` reads `Cargo.lock` and writes `acadrust_source` into `dist/plugin.toml`, discovering the codec package from `Cargo.toml` instead of hardcoding its name.
9. **Create GitHub Release** — uses `softprops/action-gh-release@v2` to attach the platform binary and `plugin.toml` to the release.

---

## 6. Manual Release Checklist

If you need to cut a release by hand instead of through the automated re-pin workflow:

1. Ensure `Cargo.toml`, `plugin.toml`, and `Cargo.lock` all agree on the version.
2. Ensure `[package.metadata.upstream].tag` matches the upstream release the plugin is built against.
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

## 7. Design Rationale

- **Tag-driven releases**: Only `v*` tags trigger publication, making releases explicit and auditable.
- **Mechanical re-pins are automatic**: If upstream publishes a new release and all gates pass, the bot can merge, tag, and release without human intervention; the merged PR is the audit trail and the revert point.
- **Source migrations are gated**: ABI mismatches, codec series moves or dependency mismatches stop the bot and create a classified escalation issue, because they require plugin code changes, not just dependency bumps.
- **ABI matching is paramount**: The upstream `Cargo.lock` is merged into the local lockfile so every shared dependency matches the host at the patch level, and the codec source is copied verbatim from `ocs_plugin_api`'s manifest (cargo keys a git source on the literal `?rev=` text, so an abbreviated rev and its expansion are two different sources).
- **Nothing hardcodes a crate name**: upstream renamed the codec crate (`acadrust` -> `opencadcodec`) and respelled its manifests repeatedly; the gates discover the codec as "the git dependency of ocs_plugin_api", and a shape they cannot read is a bug in this repository, not a decision.
- **The corpus is the incident memory**: every upstream shape that ever broke a gate is vendored under `.github/scripts/tests/fixtures/host/` and asserted in CI, and the canary parses upstream `main` a night early so the next respelling is caught with lead time.
- **A failed run leaves a diagnosed artifact**: the previous escalation opened an issue listing every possible cause generically; the escalation job now re-runs the read-only gates, classifies the failure (parser gap, human decision, or downstream break) and writes the matching fix recipe — which is how three consecutive weekly releases (v2026.37–v2026.39) went un-re-pinned without the actual cause ever being named.
- **Token-trigger limitation**: The `repin` workflow calls `release` via `workflow_call` because `GITHUB_TOKEN` pushes do not fire `push` events.
