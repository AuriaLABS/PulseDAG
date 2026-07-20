#!/usr/bin/env python3
"""Validate the fail-closed v2.3.0 release-decision proposal."""

from __future__ import annotations

import argparse
import json
import re
import subprocess
from pathlib import Path

BASE_TAG = "v2.2.20"
BASE_SHA = "14a1c38249830ee6912d8e70d6d223126cf7f63b"
EXPECTED_VERSION_FILE = "v2.2.20"
EXPECTED_CARGO_VERSION = "2.2.20"
EXPECTED_RUNTIME_CRATE_FILES = {"crates/pulsedag-p2p/src/lib.rs"}

PROPOSAL = Path("docs/release/V2_3_0_RELEASE_PROPOSAL.md")
NOTES = Path("docs/release/V2_3_0_RELEASE_NOTES_DRAFT.md")
DECISION = Path("docs/release/V2_3_0_RELEASE_DECISION.md")
INSTALL = Path("docs/INSTALL_BINARIES_V2_3_0.md")
RELEASE_WORKFLOW = Path(".github/workflows/release-binaries.yml")


class ValidationError(RuntimeError):
    """Raised when the proposal violates a required contract."""


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


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValidationError(message)


def first_cargo_version(text: str) -> str:
    match = re.search(r'^version\s*=\s*"([^"]+)"', text, flags=re.MULTILINE)
    if match is None:
        raise ValidationError("Cargo.toml does not contain a package version")
    return match.group(1)


def required_text(path: Path, needles: tuple[str, ...]) -> str:
    require(path.is_file(), f"missing required file: {path}")
    text = path.read_text(encoding="utf-8")
    for needle in needles:
        require(needle in text, f"{path} is missing required text: {needle}")
    return text


def validate(out_dir: Path | None) -> dict[str, object]:
    version = Path("VERSION").read_text(encoding="utf-8").strip()
    cargo_version = first_cargo_version(Path("Cargo.toml").read_text(encoding="utf-8"))
    require(version == EXPECTED_VERSION_FILE, f"VERSION changed before approval: {version}")
    require(
        cargo_version == EXPECTED_CARGO_VERSION,
        f"Cargo version changed before approval: {cargo_version}",
    )

    resolved_base = run_git("rev-parse", BASE_TAG)
    require(resolved_base == BASE_SHA, f"unexpected {BASE_TAG} SHA: {resolved_base}")
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

    version_diff = run_git(
        "diff",
        "--name-only",
        f"{BASE_TAG}..HEAD",
        "--",
        "VERSION",
        "Cargo.toml",
        "Cargo.lock",
    )
    require(not version_diff, f"version or dependency metadata changed before approval: {version_diff}")

    proposal = required_text(
        PROPOSAL,
        (
            "PENDING_MAINTAINER_DECISION",
            BASE_SHA,
            "22fa09b19da2893fa73b91b198b26675bd1e6e32",
            "version_bump_authorized=false",
            "public_testnet_ready=false",
            "x86_64-unknown-linux-gnu",
            "x86_64-pc-windows-msvc",
            "x86_64-apple-darwin",
        ),
    )
    notes = required_text(
        NOTES,
        (
            "release notes — draft",
            "PENDING_MAINTAINER_DECISION",
            "Connected-peer counts in `libp2p-real` mode now require active transport sessions.",
            "Public-testnet readiness is not claimed.",
        ),
    )
    decision = required_text(
        DECISION,
        (
            "PENDING_MAINTAINER_DECISION",
            "APPROVE_RELEASE_CANDIDATE",
            "REQUEST_CHANGES",
            "NO_GO",
            "VERSION=v2.2.20",
            "public_testnet_ready=false",
        ),
    )
    install = required_text(
        INSTALL,
        (
            "Install binaries v2.3.0",
            "pulsedagd-v2.3.0-x86_64-unknown-linux-gnu.tar.gz",
            "pulsedag-miner-v2.3.0-x86_64-pc-windows-msvc.zip",
            "/p2p/<seed-peer-id>",
            "public_testnet_ready=false",
        ),
    )
    workflow = required_text(
        RELEASE_WORKFLOW,
        (
            "docs/INSTALL_BINARIES_V2_3_0.md",
            "x86_64-unknown-linux-gnu",
            "x86_64-pc-windows-msvc",
            "x86_64-apple-darwin",
        ),
    )
    require(
        "docs/INSTALL_BINARIES_V2_2_19.md" not in workflow,
        "release workflow still packages the v2.2.19 install guide",
    )
    require(
        workflow.count("docs/INSTALL_BINARIES_V2_3_0.md") == 2,
        "release workflow must package the v2.3.0 install guide with both binaries",
    )

    for label, text in (
        ("proposal", proposal),
        ("notes", notes),
        ("decision", decision),
        ("install", install),
    ):
        require("public_testnet_ready=true" not in text, f"{label} claims public readiness")
        require(
            "thirty_day_public_testnet_clock_started=true" not in text,
            f"{label} claims the public-testnet clock started",
        )

    manifest: dict[str, object] = {
        "gate": "v2.3.0-release-decision-proposal",
        "candidate_sha": candidate_sha,
        "base_tag": BASE_TAG,
        "base_sha": BASE_SHA,
        "commits_ahead": commits_ahead,
        "changed_file_count": len(changed_files),
        "runtime_crate_files": sorted(runtime_crate_files),
        "version": version,
        "cargo_version": cargo_version,
        "decision": "PENDING_MAINTAINER_DECISION",
        "version_bump_authorized": False,
        "public_testnet_ready": False,
        "thirty_day_public_testnet_clock_started": False,
        "result": "PASS",
    }
    if out_dir is not None:
        out_dir.mkdir(parents=True, exist_ok=True)
        (out_dir / "release-proposal-manifest.json").write_text(
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
        print(f"FAIL: {exc}")
        return 1
    print(json.dumps(manifest, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
