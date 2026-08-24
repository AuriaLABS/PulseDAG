#!/usr/bin/env python3
"""Fail-closed Task31 validator for the temporary v2.4.0 RustSec warning disposition."""

from __future__ import annotations

import datetime as dt
import json
import os
import re
import subprocess
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
REVIEW_DEADLINE = dt.date(2026, 8, 31)

EXPECTED_WARNINGS = {
    ("RUSTSEC-2025-0052", "async-std", "1.13.2", "unmaintained"),
    ("RUSTSEC-2024-0375", "atty", "0.2.14", "unmaintained"),
    ("RUSTSEC-2021-0145", "atty", "0.2.14", "unsound"),
    ("RUSTSEC-2025-0141", "bincode", "1.3.3", "unmaintained"),
    ("RUSTSEC-2024-0384", "instant", "0.1.13", "unmaintained"),
    ("RUSTSEC-2024-0407", "linkme", "0.2.10", "unsound"),
    ("RUSTSEC-2024-0436", "paste", "1.0.15", "unmaintained"),
    ("RUSTSEC-2024-0370", "proc-macro-error", "1.0.4", "unmaintained"),
    ("RUSTSEC-2025-0010", "ring", "0.16.20", "unmaintained"),
    ("RUSTSEC-2026-0002", "lru", "0.12.5", "unsound"),
    ("RUSTSEC-2026-0253", "lru", "0.12.5", "unsound"),
}

EXPECTED_LOCKED_VERSIONS = {
    "async-std": {"1.13.2"}, "atty": {"0.2.14"}, "bincode": {"1.3.3"},
    "instant": {"0.1.13"}, "linkme": {"0.2.10"}, "paste": {"1.0.15"},
    "proc-macro-error": {"1.0.4"}, "ring": {"0.16.20", "0.17.14"},
    "lru": {"0.12.5"}, "workflow-core": {"0.18.0"}, "workflow-log": {"0.18.0"},
    "workflow-core-macros": {"0.18.0"}, "workflow-wasm-macros": {"0.18.0"},
    "hexplay": {"0.3.0"}, "intertrait": {"0.2.2"}, "kaspa-core": {"0.15.0"},
}
REQUIRED_PATCHED = {"crossbeam-epoch": "0.9.20", "anyhow": "1.0.103", "event-listener": "5.4.2"}
FORBIDDEN_OLD = {"crossbeam-epoch": "0.9.18", "anyhow": "1.0.102", "event-listener": "5.4.1"}

EXPECTED_LINUX_COMPILED = {
    "async-std": {"pulsedagd": True, "pulsedag-miner": True},
    "atty": {"pulsedagd": True, "pulsedag-miner": True},
    "bincode": {"pulsedagd": True, "pulsedag-miner": False},
    "instant": {"pulsedagd": True, "pulsedag-miner": True},
    "linkme": {"pulsedagd": True, "pulsedag-miner": True},
    "paste": {"pulsedagd": True, "pulsedag-miner": False},
    "proc-macro-error": {"pulsedagd": True, "pulsedag-miner": True},
    "ring": {"pulsedagd": False, "pulsedag-miner": False},
    "lru": {"pulsedagd": True, "pulsedag-miner": False},
}


def fail(message: str) -> None:
    raise SystemExit(f"RustSec warning disposition validation failed: {message}")


def package_versions(lock_text: str) -> dict[str, list[str]]:
    versions: dict[str, list[str]] = {}
    for raw in lock_text.split("[[package]]")[1:]:
        name = re.search(r'^name = "([^"]+)"$', raw, re.MULTILINE)
        version = re.search(r'^version = "([^"]+)"$', raw, re.MULTILINE)
        if name and version:
            versions.setdefault(name.group(1), []).append(version.group(1))
    return versions


def direct_dependency_version(manifest: Path, dependency: str) -> str:
    data = tomllib.loads(manifest.read_text(encoding="utf-8"))
    value = data.get("dependencies", {}).get(dependency)
    if isinstance(value, str):
        return value
    if isinstance(value, dict):
        return str(value.get("version", ""))
    return ""


def assert_no_custom_global_allocator() -> None:
    result = subprocess.run(
        ["git", "grep", "-n", "global_allocator", "--", "*.rs"], cwd=ROOT,
        check=False, text=True, encoding="utf-8", errors="replace",
        stdout=subprocess.PIPE, stderr=subprocess.PIPE,
    )
    if result.returncode not in (0, 1):
        fail(f"git grep failed: {result.stderr.strip()}")
    if result.stdout.strip():
        fail("custom global allocator marker found; Windows atty disposition requires re-review:\n" + result.stdout)


def main() -> None:
    today = dt.datetime.now(dt.timezone.utc).date()
    if today > REVIEW_DEADLINE:
        fail(f"disposition expired on {REVIEW_DEADLINE.isoformat()}; remove or re-review every warning before continuing")

    lock_versions = package_versions((ROOT / "Cargo.lock").read_text(encoding="utf-8"))
    for package, expected_versions in EXPECTED_LOCKED_VERSIONS.items():
        observed = set(lock_versions.get(package, []))
        if observed != expected_versions:
            fail(f"expected {package} versions {sorted(expected_versions)}, found {sorted(observed) or 'nothing'}")
    for package, expected_version in REQUIRED_PATCHED.items():
        observed = set(lock_versions.get(package, []))
        if expected_version not in observed:
            fail(f"required patched {package} {expected_version} is missing; found {sorted(observed)}")
    for package, old_version in FORBIDDEN_OLD.items():
        if old_version in set(lock_versions.get(package, [])):
            fail(f"previous vulnerable {package} {old_version} returned to Cargo.lock")

    if direct_dependency_version(ROOT / "apps" / "pulsedagd" / "Cargo.toml", "bincode") not in {"1", "1.3", "1.3.3"}:
        fail("pulsedagd bincode dependency changed; storage disposition needs review")
    if direct_dependency_version(ROOT / "crates" / "pulsedag-storage" / "Cargo.toml", "bincode") not in {"1", "1.3", "1.3.3"}:
        fail("pulsedag-storage bincode dependency changed; migration review required")

    assert_no_custom_global_allocator()

    audit = tomllib.loads((ROOT / ".cargo" / "audit.toml").read_text(encoding="utf-8"))
    ignored = set(audit.get("advisories", {}).get("ignore", []))
    warning_ids = {item[0] for item in EXPECTED_WARNINGS}
    hidden_warnings = ignored & warning_ids
    if hidden_warnings:
        fail(f"warning advisories must remain visible, but are ignored: {sorted(hidden_warnings)}")

    evidence_dir = ROOT / "ci-evidence" / "rustsec-warning-disposition"
    summary_path = evidence_dir / "warning-reachability-summary.json"
    if not summary_path.is_file():
        fail("exact-candidate warning reachability summary is missing")
    summary = json.loads(summary_path.read_text(encoding="utf-8"))
    candidate_sha = os.environ.get("PULSEDAG_CANDIDATE_SHA") or os.environ.get("GITHUB_SHA", "")
    if candidate_sha and summary.get("source_sha") != candidate_sha:
        fail(f"reachability source_sha={summary.get('source_sha')} does not match candidate {candidate_sha}")
    if summary.get("runner_os") != "Linux":
        fail(f"Task31 PR gate requires Linux reachability evidence, found {summary.get('runner_os')!r}")
    if set(summary.get("roots", [])) != {"pulsedagd", "pulsedag-miner"}:
        fail(f"unexpected compiled roots: {summary.get('roots')}")

    warnings = summary.get("warnings", {})
    for advisory, package, version, _kind in EXPECTED_WARNINGS:
        item = warnings.get(package)
        if not isinstance(item, dict):
            fail(f"reachability summary missing package {package}")
        if item.get("version") != version:
            fail(f"reachability version drift for {package}: {item.get('version')}")
        if advisory not in item.get("advisory_ids", []):
            fail(f"reachability summary missing {advisory} for {package}")

    for package, roots in EXPECTED_LINUX_COMPILED.items():
        compiled_by_root = warnings[package].get("compiled_by_root", {})
        for root, expected in roots.items():
            observed = bool(compiled_by_root.get(root, []))
            if observed != expected:
                fail(f"Linux reachability drift for {package} in {root}: expected compiled={expected}, found {observed}")

    document = (ROOT / "docs" / "security" / "V2_4_0_RUSTSEC_WARNING_DISPOSITION.md").read_text(encoding="utf-8")
    for required in (
        "Review deadline: `2026-08-31 UTC`", "public-testnet blockers",
        "Windows exact-candidate revalidation remains pending",
        "No warning ID is added to `.cargo/audit.toml`.",
        "`crossbeam-epoch 0.9.20`", "`anyhow 1.0.103`", "`event-listener 5.4.2`",
    ):
        if required not in document:
            fail(f"disposition record is missing required text: {required!r}")
    for advisory, package, version, _kind in EXPECTED_WARNINGS:
        for required in (advisory, f"`{package} {version}`"):
            if required not in document:
                fail(f"disposition record is missing {required!r}")

    validation = evidence_dir / "validation.txt"
    validation.write_text("\n".join([
        "result=PASS", f"source_sha={summary.get('source_sha')}", f"validated_utc_date={today.isoformat()}",
        f"review_deadline={REVIEW_DEADLINE.isoformat()}",
        "authorization=task31-technical-candidate-private-only",
        "linux_exact_candidate_reachability=true", "windows_exact_candidate_revalidation_pending=true",
        "public_testnet_unsound_blockers=atty@0.2.14,linkme@0.2.10,lru@0.12.5",
        "custom_global_allocator=false", "warning_count=11", "",
    ]), encoding="utf-8")
    print("PASS: Task31 RustSec warning disposition is exact, visible, unexpired and private-only")


if __name__ == "__main__":
    main()
