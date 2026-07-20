#!/usr/bin/env python3
"""Validate the exact fail-closed PulseDAG v2.3.0 release candidate."""

from __future__ import annotations

import argparse
import copy
import json
import re
import subprocess
import tomllib
from pathlib import Path
from typing import Any

BASE_TAG = "v2.2.20"
BASE_SHA = "14a1c38249830ee6912d8e70d6d223126cf7f63b"
PROPOSAL_SHA = "4a3d4e3df587f9bd6f438ddd7359a5148f0cff8e"
PROPOSAL_MERGE_SHA = "fec0b304a2544245826e5f799d9932d157818d43"
EXPECTED_VERSION_FILE = "v2.3.0"
EXPECTED_CARGO_VERSION = "2.3.0"
EXPECTED_BASE_WORKSPACE_VERSION = "2.2.20"
EXPECTED_RUNTIME_CRATE_FILES = {"crates/pulsedag-p2p/src/lib.rs"}
WORKSPACE_PACKAGES = {
    "pulsedag-api",
    "pulsedag-core",
    "pulsedag-crypto",
    "pulsedag-miner",
    "pulsedag-p2p",
    "pulsedag-rpc",
    "pulsedag-storage",
    "pulsedag-wallet",
    "pulsedagd",
}

DECISION = Path("docs/release/V2_3_0_RELEASE_DECISION.md")
APPROVAL = Path("docs/release/V2_3_0_RELEASE_APPROVAL_RECORD.md")
NOTES = Path("docs/release/V2_3_0_RELEASE_NOTES.md")
INSTALL = Path("docs/INSTALL_BINARIES_V2_3_0.md")
RELEASE_WORKFLOW = Path(".github/workflows/release-binaries.yml")
CANDIDATE_WORKFLOW = Path(".github/workflows/v2_3_0_release_candidate.yml")


class ValidationError(RuntimeError):
    """Raised when the versioned candidate violates the release contract."""


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValidationError(message)


def run_git(*args: str) -> str:
    process = subprocess.run(
        ["git", *args],
        check=False,
        capture_output=True,
        text=True,
    )
    if process.returncode != 0:
        raise ValidationError(
            f"git {' '.join(args)} failed: {process.stderr.strip() or process.stdout.strip()}"
        )
    return process.stdout.strip()


def required_text(path: Path, needles: tuple[str, ...]) -> str:
    require(path.is_file(), f"missing required file: {path}")
    text = path.read_text(encoding="utf-8")
    for needle in needles:
        require(needle in text, f"{path} is missing required text: {needle}")
    return text


def first_cargo_version(text: str) -> str:
    match = re.search(r'^version\s*=\s*"([^"]+)"', text, flags=re.MULTILINE)
    if match is None:
        raise ValidationError("Cargo.toml does not contain a workspace package version")
    return match.group(1)


def current_decision(text: str) -> str:
    match = re.search(
        r"^## Current decision\s+`([^`]+)`",
        text,
        flags=re.MULTILINE,
    )
    if match is None:
        raise ValidationError("release decision file does not contain a current decision")
    return match.group(1)


def contains_true_assignment(text: str, key: str) -> bool:
    pattern = rf"(?m)^\s*(?:[-*]\s*)?`?{re.escape(key)}=true`?(?:[.,;:]?\s*)$"
    return re.search(pattern, text) is not None


def parse_lock(text: str, label: str) -> list[dict[str, Any]]:
    try:
        parsed = tomllib.loads(text)
    except tomllib.TOMLDecodeError as exc:
        raise ValidationError(f"{label} is not valid TOML: {exc}") from exc
    packages = parsed.get("package")
    require(isinstance(packages, list), f"{label} has no package array")
    return packages


def canonical_package(package: dict[str, Any], ignore_version: bool = False) -> str:
    value = copy.deepcopy(package)
    if ignore_version:
        value.pop("version", None)
    return json.dumps(value, sort_keys=True, separators=(",", ":"))


def package_map(packages: list[dict[str, Any]], label: str) -> dict[str, dict[str, Any]]:
    selected: dict[str, dict[str, Any]] = {}
    for package in packages:
        name = package.get("name")
        if name in WORKSPACE_PACKAGES:
            require(name not in selected, f"duplicate workspace package {name} in {label}")
            selected[name] = package
    require(set(selected) == WORKSPACE_PACKAGES, f"workspace package set mismatch in {label}")
    return selected


def non_workspace_packages(packages: list[dict[str, Any]]) -> list[str]:
    values = [
        canonical_package(package)
        for package in packages
        if package.get("name") not in WORKSPACE_PACKAGES
    ]
    return sorted(values)


def validate_lockfile() -> dict[str, object]:
    current_text = Path("Cargo.lock").read_text(encoding="utf-8")
    base_text = run_git("show", f"{BASE_TAG}:Cargo.lock")
    current_packages = parse_lock(current_text, "current Cargo.lock")
    base_packages = parse_lock(base_text, f"{BASE_TAG} Cargo.lock")

    current_workspace = package_map(current_packages, "current Cargo.lock")
    base_workspace = package_map(base_packages, f"{BASE_TAG} Cargo.lock")

    for name in sorted(WORKSPACE_PACKAGES):
        current = current_workspace[name]
        base = base_workspace[name]
        require(
            current.get("version") == EXPECTED_CARGO_VERSION,
            f"{name} lockfile version is {current.get('version')!r}",
        )
        require(
            base.get("version") == EXPECTED_BASE_WORKSPACE_VERSION,
            f"{name} base lockfile version is {base.get('version')!r}",
        )
        require(
            canonical_package(current, ignore_version=True)
            == canonical_package(base, ignore_version=True),
            f"{name} lockfile metadata changed beyond the workspace version",
        )

    require(
        non_workspace_packages(current_packages) == non_workspace_packages(base_packages),
        "Cargo.lock contains dependency drift beyond workspace version changes",
    )

    return {
        "workspace_packages": sorted(WORKSPACE_PACKAGES),
        "workspace_version": EXPECTED_CARGO_VERSION,
        "dependency_drift": False,
        "package_count": len(current_packages),
    }


def validate(out_dir: Path | None) -> dict[str, object]:
    version = Path("VERSION").read_text(encoding="utf-8").strip()
    cargo_text = Path("Cargo.toml").read_text(encoding="utf-8")
    cargo_version = first_cargo_version(cargo_text)
    require(version == EXPECTED_VERSION_FILE, f"unexpected VERSION: {version}")
    require(cargo_version == EXPECTED_CARGO_VERSION, f"unexpected Cargo version: {cargo_version}")

    resolved_base = run_git("rev-parse", BASE_TAG)
    require(resolved_base == BASE_SHA, f"unexpected {BASE_TAG} SHA: {resolved_base}")
    require(not run_git("tag", "--list", "v2.3.0"), "v2.3.0 tag already exists")

    candidate_sha = run_git("rev-parse", "HEAD")
    commits_ahead = int(run_git("rev-list", "--count", f"{BASE_TAG}..HEAD"))
    changed_files = [
        line
        for line in run_git("diff", "--name-only", f"{BASE_TAG}..HEAD").splitlines()
        if line
    ]
    runtime_crate_files = {
        path for path in changed_files if path.startswith("crates/") and path.endswith(".rs")
    }
    require(
        runtime_crate_files == EXPECTED_RUNTIME_CRATE_FILES,
        f"unexpected runtime crate scope: {sorted(runtime_crate_files)}",
    )

    changed_manifests = [
        path
        for path in changed_files
        if path == "Cargo.toml" or path.endswith("/Cargo.toml")
    ]
    require(changed_manifests == ["Cargo.toml"], f"unexpected Cargo manifests: {changed_manifests}")

    decision_text = required_text(
        DECISION,
        (
            "APPROVE_RELEASE_CANDIDATE",
            PROPOSAL_SHA,
            PROPOSAL_MERGE_SHA,
            "PENDING_FINAL_CANDIDATE_EVIDENCE",
            "version_bump_authorized=true",
            "public_testnet_ready=false",
            "thirty_day_public_testnet_clock_started=false",
        ),
    )
    require(
        current_decision(decision_text) == "APPROVE_RELEASE_CANDIDATE",
        "current release decision is not APPROVE_RELEASE_CANDIDATE",
    )
    approval_text = required_text(
        APPROVAL,
        (
            "APPROVE_RELEASE_CANDIDATE",
            "Maintainer: `kalekoi`",
            PROPOSAL_SHA,
            PROPOSAL_MERGE_SHA,
            "does not authorize a `v2.3.0` tag",
        ),
    )
    notes_text = required_text(
        NOTES,
        (
            "# PulseDAG v2.3.0 release notes",
            "APPROVE_RELEASE_CANDIDATE",
            "PENDING_FINAL_CANDIDATE_EVIDENCE",
            "Connected-peer counts in `libp2p-real` mode require active transport sessions.",
            "Public-testnet readiness is not claimed.",
        ),
    )
    require("release notes — draft" not in notes_text.lower(), "final notes still identify as draft")
    require(
        not Path("docs/release/V2_3_0_RELEASE_NOTES_DRAFT.md").exists(),
        "superseded draft release notes still exist",
    )
    install_text = required_text(
        INSTALL,
        (
            "Install binaries v2.3.0",
            "pulsedagd-v2.3.0-x86_64-unknown-linux-gnu.tar.gz",
            "pulsedag-miner-v2.3.0-x86_64-pc-windows-msvc.zip",
            "/p2p/<seed-peer-id>",
            "public_testnet_ready=false",
        ),
    )
    release_workflow = required_text(
        RELEASE_WORKFLOW,
        (
            "docs/INSTALL_BINARIES_V2_3_0.md",
            "x86_64-unknown-linux-gnu",
            "x86_64-pc-windows-msvc",
            "x86_64-apple-darwin",
        ),
    )
    require(
        release_workflow.count("docs/INSTALL_BINARIES_V2_3_0.md") == 2,
        "release workflow must package the v2.3.0 guide with both binaries",
    )
    required_text(
        CANDIDATE_WORKFLOW,
        (
            "v2.3.0 exact release candidate",
            "cargo metadata --locked --format-version 1",
            "validate_v2_3_0_release_candidate.py",
            "x86_64-unknown-linux-gnu",
            "x86_64-pc-windows-msvc",
            "x86_64-apple-darwin",
        ),
    )

    for label, text in (
        ("decision", decision_text),
        ("approval", approval_text),
        ("notes", notes_text),
        ("install", install_text),
    ):
        require(
            not contains_true_assignment(text, "public_testnet_ready"),
            f"{label} claims public readiness",
        )
        require(
            not contains_true_assignment(text, "thirty_day_public_testnet_clock_started"),
            f"{label} claims the public-testnet clock started",
        )

    lock = validate_lockfile()
    manifest: dict[str, object] = {
        "gate": "v2.3.0-exact-release-candidate",
        "candidate_sha": candidate_sha,
        "base_tag": BASE_TAG,
        "base_sha": BASE_SHA,
        "proposal_sha": PROPOSAL_SHA,
        "proposal_merge_sha": PROPOSAL_MERGE_SHA,
        "commits_ahead": commits_ahead,
        "changed_file_count": len(changed_files),
        "runtime_crate_files": sorted(runtime_crate_files),
        "version": version,
        "cargo_version": cargo_version,
        "lockfile": lock,
        "proposal_decision": "APPROVE_RELEASE_CANDIDATE",
        "final_release_decision": "PENDING_FINAL_CANDIDATE_EVIDENCE",
        "version_bump_authorized": True,
        "tag_authorized": False,
        "publication_authorized": False,
        "public_testnet_ready": False,
        "thirty_day_public_testnet_clock_started": False,
        "result": "PASS",
    }
    if out_dir is not None:
        out_dir.mkdir(parents=True, exist_ok=True)
        (out_dir / "release-candidate-manifest.json").write_text(
            json.dumps(manifest, indent=2, sort_keys=True) + "\n",
            encoding="utf-8",
        )
    return manifest


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out-dir", type=Path)
    args = parser.parse_args()
    try:
        manifest = validate(args.out_dir)
    except ValidationError as exc:
        if args.out_dir is not None:
            args.out_dir.mkdir(parents=True, exist_ok=True)
            (args.out_dir / "validation-error.txt").write_text(
                f"FAIL: {exc}\n",
                encoding="utf-8",
            )
        print(f"FAIL: {exc}")
        return 1
    print(json.dumps(manifest, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
