#!/usr/bin/env python3
"""Fail-closed validator for the temporary v2.4.0 RustSec warning disposition."""

from __future__ import annotations

import datetime as dt
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
}

EXPECTED_LOCKED_PACKAGES = {
    "async-std": "1.13.2",
    "atty": "0.2.14",
    "bincode": "1.3.3",
    "instant": "0.1.13",
    "linkme": "0.2.10",
    "paste": "1.0.15",
    "proc-macro-error": "1.0.4",
    "workflow-core": "0.18.0",
    "workflow-log": "0.18.0",
    "workflow-core-macros": "0.18.0",
    "workflow-wasm-macros": "0.18.0",
    "hexplay": "0.3.0",
    "intertrait": "0.2.2",
    "kaspa-core": "0.15.0",
}

REQUIRED_DOCUMENT_TEXT = (
    "Review deadline: `2026-08-31 UTC`",
    "valueless,\nprivate, non-public burn-in only",
    "The two reachable unsound warnings (`atty` and `linkme`) remain public-testnet\nblockers",
    "0516420770f87e27eafa6771e58e6c6e6e4aa01f",
    "74e54026043e4f2aebd43e5cb4bbeb80f2e67be9",
    "sha256:6b8ea3d4e1e2dc7843826739355513f6187c266254dbd32c24d58f8ac0ac20de",
    "sha256:0464fe820156b8655c566509ad5cfb2a588a071f24351c4a4642cf59dc9400f8",
    "No warning ID is added to `.cargo/audit.toml`.",
)


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
        ["git", "grep", "-n", "global_allocator", "--", "*.rs"],
        cwd=ROOT,
        check=False,
        text=True,
        encoding="utf-8",
        errors="replace",
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    if result.returncode not in (0, 1):
        fail(f"git grep failed: {result.stderr.strip()}")
    if result.stdout.strip():
        fail(
            "custom global allocator marker found; Windows atty disposition is invalid:\n"
            + result.stdout
        )


def main() -> None:
    today = dt.datetime.now(dt.timezone.utc).date()
    if today > REVIEW_DEADLINE:
        fail(
            f"disposition expired on {REVIEW_DEADLINE.isoformat()}; "
            "remove or re-review every warning before continuing"
        )

    lock_versions = package_versions((ROOT / "Cargo.lock").read_text(encoding="utf-8"))
    for package, expected_version in EXPECTED_LOCKED_PACKAGES.items():
        observed = lock_versions.get(package, [])
        if observed != [expected_version]:
            fail(
                f"expected exactly {package} {expected_version}, "
                f"found {observed or 'nothing'}"
            )

    if direct_dependency_version(
        ROOT / "apps" / "pulsedagd" / "Cargo.toml", "bincode"
    ) not in {"1", "1.3", "1.3.3"}:
        fail("pulsedagd bincode dependency changed; storage disposition needs review")
    if direct_dependency_version(
        ROOT / "crates" / "pulsedag-storage" / "Cargo.toml", "bincode"
    ) not in {"1", "1.3", "1.3.3"}:
        fail("pulsedag-storage bincode dependency changed; migration review required")

    assert_no_custom_global_allocator()

    audit = tomllib.loads((ROOT / ".cargo" / "audit.toml").read_text(encoding="utf-8"))
    ignored = set(audit.get("advisories", {}).get("ignore", []))
    warning_ids = {item[0] for item in EXPECTED_WARNINGS}
    hidden_warnings = ignored & warning_ids
    if hidden_warnings:
        fail(f"warning advisories must remain visible, but are ignored: {sorted(hidden_warnings)}")

    analyzer = (ROOT / "scripts" / "analyze_v2_4_0_rustsec_warnings.py").read_text(
        encoding="utf-8"
    )
    for advisory, package, version, _kind in EXPECTED_WARNINGS:
        if advisory not in analyzer or f'WarningPackage("{package}", "{version}"' not in analyzer:
            fail(f"reachability analyzer is missing {advisory} for {package} {version}")

    document = (
        ROOT / "docs" / "security" / "V2_4_0_RUSTSEC_WARNING_DISPOSITION.md"
    ).read_text(encoding="utf-8")
    for required in REQUIRED_DOCUMENT_TEXT:
        if required not in document:
            fail(f"disposition record is missing required text: {required!r}")
    for advisory, package, version, _kind in EXPECTED_WARNINGS:
        for required in (advisory, f"`{package} {version}`"):
            if required not in document:
                fail(f"disposition record is missing {required!r}")

    evidence_dir = ROOT / "ci-evidence" / "rustsec-warning-disposition"
    evidence_dir.mkdir(parents=True, exist_ok=True)
    summary = evidence_dir / "validation.txt"
    summary.write_text(
        "\n".join(
            [
                "result=PASS",
                f"validated_utc_date={today.isoformat()}",
                f"review_deadline={REVIEW_DEADLINE.isoformat()}",
                "authorization=private-valueless-burn-in-only",
                "public_testnet_unsound_blockers=atty@0.2.14,linkme@0.2.10",
                "custom_global_allocator=false",
                "warning_count=8",
                *(
                    f"warning={advisory},{package},{version},{kind}"
                    for advisory, package, version, kind in sorted(EXPECTED_WARNINGS)
                ),
                "",
            ]
        ),
        encoding="utf-8",
    )
    print(
        "PASS: v2.4.0 warning disposition remains exact, visible, unexpired "
        "and limited to private valueless burn-in"
    )


if __name__ == "__main__":
    main()
