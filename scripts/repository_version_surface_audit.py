#!/usr/bin/env python3
"""Fail when primary active repository surfaces disagree with VERSION/Cargo."""

from __future__ import annotations

import argparse
import fnmatch
import re
import subprocess
import sys
from pathlib import Path

HISTORICAL_DOC = re.compile(r"(?:^|_)V?2_2_(?:\d+)(?:_|\.|$)", re.IGNORECASE)
HISTORICAL_WORKFLOW = re.compile(r"^v2_2_\d+_", re.IGNORECASE)

PRIMARY_ACTIVE_FILES = [
    Path("README.md"),
    Path("docs/README.md"),
    Path("docs/VERSION_MATRIX.md"),
]

LEGACY_SCRIPT_FAMILIES = [
    "scripts/v2_2_*",
    "scripts/docker_v2_2_*",
    "scripts/windows/v2_2_*",
    "scripts/tests/test_v2_2_*",
    "scripts/v2-2-*",
    "scripts/*_v2_2_*",
]

LEGACY_CONFIG_FAMILIES = [
    "configs/private-testnet/v2_2_*/*",
]

ACTIVE_DOC_EXCLUDED_ROOTS = {
    Path("docs/archive"),
    Path("docs/codex_tasks"),
}


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


def is_excluded_active_doc(path: Path) -> bool:
    return any(root == path or root in path.parents for root in ACTIVE_DOC_EXCLUDED_ROOTS)


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


def require_markers(
    path: Path, markers: list[str], failures: list[str]
) -> None:
    if not path.is_file():
        failures.append(f"missing active document: {path}")
        return
    text = path.read_text(encoding="utf-8", errors="ignore")
    for marker in markers:
        if marker not in text:
            failures.append(f"{path} is missing current marker: {marker}")


def main() -> int:
    args = parse_args()
    failures: list[str] = []
    warnings: list[str] = []

    version = Path("VERSION").read_text(encoding="utf-8").strip()
    cargo = cargo_version()
    expected_version = f"v{cargo}" if cargo else ""
    if not cargo or version != expected_version:
        failures.append(
            f"VERSION/Cargo mismatch: found {version}/{cargo or 'missing'}"
        )

    require_markers(
        Path("README.md"),
        [
            f"# PulseDAG {version}",
            f"Repository version: `{version}`",
            f"Cargo workspace version: `{cargo}`",
            "PENDING_EXACT_CANDIDATE_EVIDENCE",
            "`public_testnet_ready=false`",
            "`thirty_day_public_testnet_clock_started=false`",
            "`contracts_enabled=false`",
        ],
        failures,
    )
    require_markers(
        Path("docs/README.md"),
        [
            version,
            "PENDING_EXACT_CANDIDATE_EVIDENCE",
            "`public_testnet_ready=false`",
            "`thirty_day_public_testnet_clock_started=false`",
            "`contracts_enabled=false`",
        ],
        failures,
    )
    require_markers(
        Path("docs/VERSION_MATRIX.md"),
        [
            f"| VERSION file | `{version}` |",
            f"| Cargo workspace version | `{cargo}` |",
            "`PENDING_EXACT_CANDIDATE_EVIDENCE`",
            "`public_testnet_ready=false`",
            "`thirty_day_public_testnet_clock_started=false`",
            "`contracts_enabled=false`",
        ],
        failures,
    )

    if version == "v2.4.0":
        require_markers(
            Path("docs/ROADMAP_V2_4_0.md"), ["v2.4.0"], failures
        )
        require_markers(
            Path("docs/PROTOCOL_ACTIVATION_V2_4_0.md"),
            ["v2.4.0", "ghostdag_v1"],
            failures,
        )

    # Primary surfaces must not still identify a different release as current.
    for path in PRIMARY_ACTIVE_FILES:
        if not path.is_file():
            continue
        text = path.read_text(encoding="utf-8", errors="ignore")
        claims = re.findall(
            r"(?:^#\s+PulseDAG\s+v|Repository version:\s*`v|VERSION file \| `v)(\d+\.\d+\.\d+)",
            text,
            flags=re.IGNORECASE | re.MULTILINE,
        )
        for claim in claims:
            if f"v{claim}" != version:
                failures.append(
                    f"stale active-version claim in {path}: v{claim} (current {version})"
                )

    docs_root = Path("docs")
    if docs_root.is_dir():
        for path in sorted(docs_root.rglob("*")):
            if not path.is_file() or is_excluded_active_doc(path):
                continue
            if HISTORICAL_DOC.search(path.name):
                failures.append(f"historical v2.2 document remains in active docs tree: {path}")

    workflow_root = Path(".github/workflows")
    if workflow_root.is_dir():
        for path in sorted(workflow_root.iterdir()):
            if path.is_file() and HISTORICAL_WORKFLOW.search(path.name):
                failures.append(f"historical v2.2 workflow remains active: {path}")

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
        print(f"[PASS] active repository surfaces identify {version} consistently")
        print("[PASS] historical documents and workflows are outside active roots")

    if failures and not args.report:
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
