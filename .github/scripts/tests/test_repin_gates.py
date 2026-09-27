"""Regression corpus for the re-pin gates.

The fixtures under fixtures/host/ are the real upstream manifests at every
tag whose shape has broken (or could have broken) a gate.  A shape that
breaks a gate gets its tag vendored here in the same PR as the fix, which is
what keeps "the nightly re-pin died again" from ever being the first
symptom.
"""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import repin_gates as gates  # noqa: E402

HERE = Path(__file__).resolve().parent
FIXTURES = HERE / "fixtures"
HOST = FIXTURES / "host"
OURS = FIXTURES / "ours" / "v2026.36"
VENDORED = ["v2026.36", "v2026.37", "v2026.38", "v2026.39"]

# What host-plan must derive from each vendored shape.  The declared rev is
# ocs_plugin_api's verbatim spelling (abbreviated or not); the locked rev is
# the lockfile's 40-hex expansion.
EXPECTED_PLANS = {
    "v2026.36": {
        "codec_package": "acadrust",
        "codec_url": "https://github.com/HakanSeven12/cadcodec.git",
        "codec_rev": "c91a1c1de89a0fbc006dc360ddb0251ac8c207d1",
        "patch_key": "https://github.com/HakanSeven12/cadcodec.git",
        "patch_url": "https://git@github.com/HakanSeven12/cadcodec.git",
        "patch_rev": "5b56571a190e7a17c8f12d36390d2938b0fb72f7",
        "locked_rev": "5b56571a190e7a17c8f12d36390d2938b0fb72f7",
        "locked_version": "0.5.4",
    },
    "v2026.37": {
        "codec_package": "acadrust",
        "codec_url": "https://github.com/HakanSeven12/cadcodec.git",
        "codec_rev": "6f3711809e39e9ffdf16a63086079b6475d97175",
        "patch_key": "",
        "locked_rev": "6f3711809e39e9ffdf16a63086079b6475d97175",
        "locked_version": "0.5.5",
    },
    "v2026.38": {
        "codec_package": "acadrust",
        "codec_url": "https://github.com/HakanSeven12/cadcodec.git",
        "codec_rev": "5b682ed",
        "patch_key": "",
        "locked_rev": "5b682ed66ea2c89be8142c8dd83d83774fc3de08",
        "locked_version": "0.5.5",
    },
    "v2026.39": {
        "codec_package": "opencadcodec",
        "codec_url": "https://github.com/HakanSeven12/opencadcodec.git",
        "codec_rev": "42b44d2",
        "patch_key": "",
        "locked_rev": "42b44d2343e9925cff8a0c9e94c1d1c5ae8541c7",
        "locked_version": "0.5.5",
    },
}


def host_fixture(tag: str, name: str) -> str:
    return (HOST / tag / name).read_text(encoding="utf-8")


def plan_for(tag: str) -> dict:
    return gates.host_plan(
        gates.load_toml(host_fixture(tag, "api-Cargo.toml")),
        gates.load_toml(host_fixture(tag, "Cargo.toml")),
        host_fixture(tag, "Cargo.lock"),
    )


def our_cargo() -> dict:
    return gates.load_toml((OURS / "Cargo.toml").read_text(encoding="utf-8"))


def our_lock() -> str:
    return (OURS / "Cargo.lock").read_text(encoding="utf-8")


# --------------------------------------------------------------------------
# host-plan: every shape upstream has ever shipped
# --------------------------------------------------------------------------


@pytest.mark.parametrize("tag", VENDORED)
def test_host_plan_matches_every_vendored_shape(tag):
    plan = plan_for(tag)
    for key, value in EXPECTED_PLANS[tag].items():
        assert plan[key] == value, f"{tag}: {key} is {plan[key]!r}, expected {value!r}"


def test_host_plan_rejects_a_lockfile_that_lost_the_codec():
    """The v2026.39 failure mode: the crate was renamed and the parser was
    still looking for the old name.  A missing lockfile stanza is a parser
    gap (UNPARSEABLE), never a silent wrong pin."""
    api = gates.load_toml(host_fixture("v2026.39", "api-Cargo.toml"))
    host = gates.load_toml(host_fixture("v2026.39", "Cargo.toml"))
    lock = host_fixture("v2026.39", "Cargo.lock").replace(
        'name = "opencadcodec"', 'name = "opencadcodec_renamed"'
    )
    with pytest.raises(gates.Unparseable):
        gates.host_plan(api, host, lock)


def test_host_plan_rejects_a_patch_that_does_not_explain_the_lock():
    api = gates.load_toml(host_fixture("v2026.36", "api-Cargo.toml"))
    host = gates.load_toml(host_fixture("v2026.36", "Cargo.toml"))
    lock = host_fixture("v2026.36", "Cargo.lock").replace(
        "#5b56571a190e7a17c8f12d36390d2938b0fb72f7", "#1111111111111111111111111111111111111111"
    )
    with pytest.raises(gates.Unparseable):
        gates.host_plan(api, host, lock)


# --------------------------------------------------------------------------
# gate-series / gate-api
# --------------------------------------------------------------------------


@pytest.mark.parametrize("tag", VENDORED)
def test_gate_series_is_mechanical_for_every_vendored_shape(tag):
    plan = plan_for(tag)
    result = gates.gate_series(
        plan["codec_package"],
        host_fixture(tag, "Cargo.lock"),
        our_cargo(),
        our_lock(),
        tag,
    )
    assert result["ours"] == "0.5.4"
    assert result["host"] == EXPECTED_PLANS[tag]["locked_version"]


def test_gate_series_escalates_on_a_series_move():
    plan = plan_for("v2026.39")
    lock = host_fixture("v2026.39", "Cargo.lock").replace('version = "0.5.5"\nsource = "git+https://github.com/HakanSeven12/opencadcodec.git', 'version = "0.6.0"\nsource = "git+https://github.com/HakanSeven12/opencadcodec.git', 1)
    with pytest.raises(gates.Escalate):
        gates.gate_series(plan["codec_package"], lock, our_cargo(), our_lock(), "v2026.39")


@pytest.mark.parametrize("tag", VENDORED)
def test_gate_api_accepts_the_vendored_windows(tag):
    result = gates.gate_api(
        host_fixture(tag, "manifest.rs"),
        gates.load_toml((OURS / "plugin.toml").read_text(encoding="utf-8")),
        tag,
    )
    assert result["ours"] == 4
    assert result["minimum"] <= 4 <= result["current"]


def test_gate_api_escalates_when_the_host_drops_the_plugin():
    manifest_rs = host_fixture("v2026.39", "manifest.rs").replace(
        "pub const API_VERSION_MIN_SUPPORTED: u32 = 2;", "pub const API_VERSION_MIN_SUPPORTED: u32 = 5;"
    )
    with pytest.raises(gates.Escalate):
        gates.gate_api(manifest_rs, gates.load_toml((OURS / "plugin.toml").read_text(encoding="utf-8")), "v2026.39")


# --------------------------------------------------------------------------
# rewrite
# --------------------------------------------------------------------------


def rewritten(tag, **overrides):
    plan = plan_for(tag)
    kwargs = dict(
        tag=tag,
        codec_package=plan["codec_package"],
        codec_url=plan["codec_url"],
        codec_rev=plan["codec_rev"],
        patch_key=plan["patch_key"],
        patch_url=plan["patch_url"],
        patch_rev=plan["patch_rev"],
    )
    kwargs.update(overrides)
    return gates.rewrite(
        (OURS / "Cargo.toml").read_text(encoding="utf-8"),
        (OURS / "plugin.toml").read_text(encoding="utf-8"),
        **kwargs,
    )


def test_rewrite_to_v2026_39_renames_the_crate_and_drops_the_patch():
    cargo, plugin, newver = rewritten("v2026.39")
    assert newver == "0.1.26"
    assert (
        'acadrust = { package = "opencadcodec", git = "https://github.com/HakanSeven12/opencadcodec.git", '
        'rev = "42b44d2", features = ["serde"] }'
    ) in cargo
    assert 'tag = "v2026.39"' in cargo
    assert cargo.count('tag = "v2026.39", features = ["host"]') == 2
    assert "[patch." not in cargo, "the host has no codec patch at v2026.39; the plugin must not invent one"
    assert gates.PATCH_BEGIN in cargo and gates.PATCH_END in cargo
    assert 'version = "0.1.26"' in cargo and 'version = "0.1.26"' in plugin
    assert "api_version = 4" in plugin, "the protocol major is pinned by src/, the rewrite must not touch it"


def test_rewrite_to_v2026_36_mirrors_the_host_patch():
    cargo, _, _ = rewritten("v2026.36")
    assert (
        'acadrust = { git = "https://github.com/HakanSeven12/cadcodec.git", '
        'rev = "c91a1c1de89a0fbc006dc360ddb0251ac8c207d1", features = ["serde"] }'
    ) in cargo
    assert '[patch."https://github.com/HakanSeven12/cadcodec.git"]' in cargo
    assert (
        'acadrust = { git = "https://git@github.com/HakanSeven12/cadcodec.git", '
        'rev = "5b56571a190e7a17c8f12d36390d2938b0fb72f7" }'
    ) in cargo


def test_rewrite_preserves_crlf():
    cargo_text = (OURS / "Cargo.toml").read_text(encoding="utf-8").replace("\n", "\r\n")
    plugin_text = (OURS / "plugin.toml").read_text(encoding="utf-8")
    cargo, _, _ = gates.rewrite(
        cargo_text, plugin_text, tag="v2026.39", codec_package="opencadcodec",
        codec_url="https://github.com/HakanSeven12/opencadcodec.git", codec_rev="42b44d2",
    )
    assert "\r\n" in cargo
    assert "\r\r" not in cargo


def test_rewrite_refuses_a_manifest_without_the_marker_region():
    with pytest.raises(gates.Unparseable):
        gates.rewrite(
            "[package]\nname = \"x\"\n",
            "",
            tag="v2026.39", codec_package="opencadcodec",
            codec_url="u", codec_rev="r",
        )


# --------------------------------------------------------------------------
# merge-lock / verify-lockstep
# --------------------------------------------------------------------------


def merged_lock(tag, new_version="0.1.26"):
    return gates.merge_lock(
        host_fixture(tag, "Cargo.lock"),
        our_lock(),
        new_version=new_version,
        host_manifest=gates.load_toml(host_fixture(tag, "Cargo.toml")),
    )


@pytest.mark.parametrize("tag", VENDORED)
def test_merge_lock_takes_the_upstream_tree(tag):
    merged = merged_lock(tag)
    packages = gates.lock_packages(merged)
    upstream = gates.lock_packages(host_fixture(tag, "Cargo.lock"))
    for name in (EXPECTED_PLANS[tag]["codec_package"], "ocs_plugin_api", "serde"):
        assert name in packages, f"{name} missing after merging {tag}"
    if tag == "v2026.39":
        assert "acadrust" not in packages, "the old codec name must not survive the merge"
    assert packages["ocs_python_repl"][0]["version"] == "0.1.26"
    for name in gates.PLUGIN_ONLY:
        assert len(packages.get(name, [])) <= 1, f"duplicate [[package]] block for {name} after the merge"
    for name, stanzas in packages.items():
        assert len(stanzas) <= max(1, len(upstream.get(name, []))), (
            f"the merge duplicated {name}: {len(stanzas)} stanzas, upstream has {len(upstream.get(name, []))}"
        )


@pytest.mark.parametrize("tag", VENDORED)
def test_verify_lockstep_passes_right_after_the_merge(tag):
    merged = merged_lock(tag)
    assert gates.verify_lockstep(host_fixture(tag, "Cargo.lock"), merged) == []


def test_verify_lockstep_reports_a_mismatch():
    merged = merged_lock("v2026.39")
    merged = merged.replace('version = "1.0.228"', 'version = "1.0.229"', 1)
    mismatches = gates.verify_lockstep(host_fixture("v2026.39", "Cargo.lock"), merged)
    assert mismatches and all(m.startswith("serde ") for m in mismatches)


# --------------------------------------------------------------------------
# gate-source / gate-abi
# --------------------------------------------------------------------------


def settled_lock(tag):
    """The merged lockfile in its post-build state: cargo rewrites the
    plugin block's dependency list to the codec's real name."""
    merged = merged_lock(tag)
    return merged.replace(' "acadrust",', f' "{EXPECTED_PLANS[tag]["codec_package"]}",')


@pytest.mark.parametrize("tag", VENDORED)
def test_gate_source_accepts_a_faithful_rewrite(tag):
    plan = plan_for(tag)
    cargo, _, _ = rewritten(tag)
    gates.gate_source(
        gates.load_toml(cargo),
        gates.load_toml(host_fixture(tag, "api-Cargo.toml")),
        gates.load_toml(host_fixture(tag, "Cargo.toml")),
    )


def test_gate_source_escalates_on_a_rev_respelling():
    """`rev = "5b682ed"` and its 40-hex expansion are two different cargo
    sources; the gate must catch a lockfile-expanded rev."""
    plan = plan_for("v2026.38")
    cargo, _, _ = rewritten("v2026.38")
    cargo = cargo.replace('rev = "5b682ed"', 'rev = "5b682ed66ea2c89be8142c8dd83d83774fc3de08"')
    with pytest.raises(gates.Escalate):
        gates.gate_source(
            gates.load_toml(cargo),
            gates.load_toml(host_fixture("v2026.38", "api-Cargo.toml")),
            gates.load_toml(host_fixture("v2026.38", "Cargo.toml")),
        )


def test_gate_source_escalates_on_a_stale_patch_mirror():
    cargo, _, _ = rewritten("v2026.39")
    cargo = cargo.replace(
        "# The host workspace does not patch the codec source at this tag. The plugin\n"
        "# must not patch it either: cargo rejects a patch that points at the same\n"
        "# source it replaces, and any other redirect would fork the codec build.\n",
        '[patch."https://github.com/HakanSeven12/opencadcodec.git"]\n'
        'opencadcodec = { git = "https://github.com/HakanSeven12/opencadcodec.git", rev = "ffffffff" }\n',
    )
    with pytest.raises(gates.Escalate):
        gates.gate_source(
            gates.load_toml(cargo),
            gates.load_toml(host_fixture("v2026.39", "api-Cargo.toml")),
            gates.load_toml(host_fixture("v2026.39", "Cargo.toml")),
        )


@pytest.mark.parametrize("tag", VENDORED)
def test_gate_abi_accepts_one_codec_at_the_host_rev(tag):
    plan = plan_for(tag)
    cargo, _, _ = rewritten(tag)
    result = gates.gate_abi(
        gates.load_toml(cargo), settled_lock(tag), plan["locked_rev"], host_fixture(tag, "Cargo.lock")
    )
    assert result["rev"] == plan["locked_rev"]


def test_gate_abi_escalates_on_a_stray_old_codec():
    """A leftover crate from the codec's previous name/repo must not survive
    into the re-pinned graph, even though nothing references it anymore."""
    plan = plan_for("v2026.39")
    cargo, _, _ = rewritten("v2026.39")
    lock = settled_lock("v2026.39") + (
        "\n[[package]]\n"
        'name = "acadrust"\n'
        'version = "0.5.4"\n'
        'source = "git+https://git@github.com/HakanSeven12/cadcodec.git?rev=5b56571a190e7a17c8f12d36390d2938b0fb72f7#5b56571a190e7a17c8f12d36390d2938b0fb72f7"\n'
    )
    with pytest.raises(gates.Escalate):
        gates.gate_abi(gates.load_toml(cargo), lock, plan["locked_rev"], host_fixture("v2026.39", "Cargo.lock"))


def test_gate_abi_escalates_on_a_wrong_rev():
    plan = plan_for("v2026.39")
    cargo, _, _ = rewritten("v2026.39")
    with pytest.raises(gates.Escalate):
        gates.gate_abi(
            gates.load_toml(cargo), settled_lock("v2026.39"), "0" * 40, host_fixture("v2026.39", "Cargo.lock")
        )


# --------------------------------------------------------------------------
# current-pin / inject-source
# --------------------------------------------------------------------------


def test_current_pin_reads_tag_and_codec():
    pin = gates.current_pin((OURS / "Cargo.toml").read_text(encoding="utf-8"))
    assert pin["tag"] == "v2026.36"
    assert pin["package"] == "acadrust"
    assert pin["rev"] == "5b56571a190e7a17c8f12d36390d2938b0fb72f7"
    assert pin["version"] == "0.1.25"


def test_current_pin_rejects_drift_between_metadata_and_dependency_pin():
    """A hand-edited dependency pin that leaves [package.metadata.upstream]
    behind must not pass current-pin: the two spellings of the tag are what
    keep the detect job's comparison honest."""
    cargo_text = (OURS / "Cargo.toml").read_text(encoding="utf-8").replace(
        'tag = "v2026.36", features = ["host"] }',
        'tag = "v2026.35", features = ["host"] }',
    )
    with pytest.raises(gates.Unparseable):
        gates.current_pin(cargo_text)


def test_inject_source_uses_the_discovered_package():
    """After a rename re-pin, the fingerprint must be derived from the
    manifest's `package =` attribute, not from a hardcoded crate name."""
    cargo, _, _ = rewritten("v2026.39")
    plugin, source = gates.inject_source(
        gates.load_toml(cargo),
        settled_lock("v2026.39"),
        (OURS / "plugin.toml").read_text(encoding="utf-8"),
    )
    assert source == (
        "git+https://github.com/HakanSeven12/opencadcodec.git?rev=42b44d2#42b44d2343e9925cff8a0c9e94c1d1c5ae8541c7"
    )
    assert f'acadrust_source = "{source}"' in plugin


# --------------------------------------------------------------------------
# CLI contract: the exit-code taxonomy the workflows grep for
# --------------------------------------------------------------------------


def run_cli(*argv):
    return subprocess.run(
        [sys.executable, str(HERE.parent / "repin_gates.py"), *argv],
        capture_output=True,
        text=True,
    )


def test_cli_exit_codes(tmp_path):
    result = run_cli(
        "host-plan",
        "--plugin-api-manifest", str(HOST / "v2026.39" / "api-Cargo.toml"),
        "--host-manifest", str(HOST / "v2026.39" / "Cargo.toml"),
        "--host-lock", str(HOST / "v2026.39" / "Cargo.lock"),
    )
    assert result.returncode == 0
    assert "codec_package=opencadcodec" in result.stdout

    result = run_cli(
        "gate-api",
        "--manifest-rs", str(HOST / "v2026.39" / "manifest.rs"),
        "--tag", "v2026.39",
        "--plugin-toml", str(OURS / "plugin.toml"),
    )
    assert result.returncode == 0

    corrupt = tmp_path / "corrupt.lock"
    corrupt.write_text("version = 4\n", encoding="utf-8")
    result = run_cli(
        "host-plan",
        "--plugin-api-manifest", str(HOST / "v2026.39" / "api-Cargo.toml"),
        "--host-manifest", str(HOST / "v2026.39" / "Cargo.toml"),
        "--host-lock", str(corrupt),
    )
    assert result.returncode == 2
    assert "UNPARSEABLE" in result.stderr
