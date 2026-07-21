#!/usr/bin/env python3
"""Fail when active repository surfaces advertise a stale release."""

from __future__ import annotations

import argparse
import fnmatch
import re
import subprocess
import sys
from pathlib import Path

HISTORICAL_DOC = re.compile(r"(?:^|_)V?2_2_(?:\d+)(?:_|\.|$)", re.IGNORECASE)
HISTORICAL_WORKFLOW = re.compile(r"^v2_2_\d+_", re.IGNORECASE)
STALE_ACTIVE_CLAIM = re.compile(
    r"(?:^#\s+PulseDAG\s+v2\.2\.|current\s+(?:milestone|baseline|version)[^\n]*v2\.2\.|"
    r"repository\s+version[^\n]*v2\.2\.|cargo\s+workspace\s+version[^\n]*2\.2\.)",
    re.IGNORECASE | re.MULTILINE,
)

ACTIVE_CLAIM_FILES = [
    Path("README.md"),
    Path("docs/README.md"),
    Path("docs/VERSION_MATRIX.md"),
    Path("docs/RUNBOOK.md"),
    Path("docs/RELEASE_EVIDENCE.md"),
    Path("apps/pulsedag-miner/README.md"),
]

LEGACY_SCRIPT_FAMILIES = [
    "scripts/v2_2_*",
    "scripts/docker_v2_2_*",
    "scripts/windows/v2_2_*",
    "scripts/tests/test_v2_2_*",
]

LEGACY_CONFIG_FAMILIES = [
    "configs/private-testnet/v2_2_*/*",
]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--strict", action="store_true")
    mode.add_argument("--report", action="store_true")
    return parser.parse_args()


def cargo_version() -> str:
    text = Path("Cargo.toml").read_text(encoding="utf-8")
    match = re.search(r'^version\s*=\s*"(\d+\.\d+\.\d+)"\s*$', text, re.MULTILINE)
    return match.group(1) if match else ""


def tracked_files() -> list[str]:
    result = subprocess.run(
        ["git", "ls-files", "-z"], check=True, capture_output=True
    )
    return [entry.decode("utf-8") for entry in result.stdout.split(b"\0") if entry]


def require_family_manifest(
    *,
    paths: list[str],
    patterns: list[str],
    manifest_path: Path,
    label: str,
    failures: list[str],
    warnings: list[str],
) -> None:
    matched = sorted(
        path for path in paths if any(fnmatch.fnmatch(path, pattern) for pattern in patterns)
    )
    if not matched:
        return
    if not manifest_path.is_file():
        failures.append(f"{label} exist without {manifest_path}")
        return
    text = manifest_path.read_text(encoding="utf-8", errors="ignore")
    missing_patterns = [pattern for pattern in patterns if f"`{pattern}`" not in text]
    if missing_patterns:
        failures.extend(
            f"{manifest_path} does not classify family: {pattern}"
            for pattern in missing_patterns
        )
    warnings.append(f"{len(matched)} {label} paths remain explicitly classified")


def main() -> int:
    args = parse_args()
    failures: list[str] = []
    warnings: list[str] = []

    version = Path("VERSION").read_text(encoding="utf-8").strip()
    cargo = cargo_version()
    if version != "v2.3.0" or cargo != "2.3.0":
        failures.append(f"expected v2.3.0/2.3.0, found {version}/{cargo or 'missing'}")

    required_markers = {
        Path("README.md"): ["# PulseDAG v2.3.0", "PENDING_FINAL_CANDIDATE_EVIDENCE"],
        Path("docs/VERSION_MATRIX.md"): [
            "| VERSION file | `v2.3.0` |",
            "| Cargo workspace version | `2.3.0` |",
        ],
        Path("docs/RUNBOOK.md"): ["# PulseDAG v2.3.0 operator runbook"],
        Path("docs/RELEASE_EVIDENCE.md"): ["# PulseDAG v2.3.0 release evidence policy"],
        Path("apps/pulsedag-miner/README.md"): ["# pulsedag-miner v2.3.0"],
    }
    for path, markers in required_markers.items():
        if not path.is_file():
            failures.append(f"missing active document: {path}")
            continue
        text = path.read_text(encoding="utf-8", errors="ignore")
        for marker in markers:
            if marker not in text:
                failures.append(f"{path} is missing current marker: {marker}")

    for path in ACTIVE_CLAIM_FILES:
        if not path.is_file():
            continue
        text = path.read_text(encoding="utf-8", errors="ignore")
        match = STALE_ACTIVE_CLAIM.search(text)
        if match:
            failures.append(f"stale active-version claim in {path}: {match.group(0).strip()}")

    docs_root = Path("docs")
    if docs_root.is_dir():
        for path in sorted(docs_root.iterdir()):
            if path.is_file() and HISTORICAL_DOC.search(path.name):
                failures.append(f"historical document remains in active docs root: {path}")

    workflow_root = Path(".github/workflows")
    if workflow_root.is_dir():
        for path in sorted(workflow_root.iterdir()):
            if path.is_file() and HISTORICAL_WORKFLOW.search(path.name):
                failures.append(f"historical workflow remains active: {path}")

    paths = tracked_files()
    require_family_manifest(
        paths=paths,
        patterns=LEGACY_SCRIPT_FAMILIES,
        manifest_path=Path("scripts/LEGACY_COMPATIBILITY_V2_3_0.md"),
        label="legacy script/test",
        failures=failures,
        warnings=warnings,
    )
    require_family_manifest(
        paths=paths,
        patterns=LEGACY_CONFIG_FAMILIES,
        manifest_path=Path("configs/private-testnet/LEGACY_COMPATIBILITY_V2_3_0.md"),
        label="legacy private-testnet configuration",
        failures=failures,
        warnings=warnings,
    )

    for failure in failures:
        print(f"[FAIL] {failure}", file=sys.stderr)
    for warning in warnings:
        print(f"[WARN] {warning}", file=sys.stderr)
    if not failures:
        print("[PASS] active repository surfaces identify v2.3.0 consistently")
        print("[PASS] historical documents and workflows are outside active roots")

    if failures and not args.report:
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
