#!/usr/bin/env python3
"""Validate that the retired v2.4 RustSec record stays historical.

This script is intentionally not a current Cargo.lock security gate. Active
current-line dependency validation lives in validate_v3_dependency_security.py.
"""

from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def fail(message: str) -> None:
    raise SystemExit(f"historical v2.4 RustSec record validation failed: {message}")


def require(path: Path, *needles: str) -> str:
    if not path.is_file():
        fail(f"required file missing: {path.relative_to(ROOT)}")
    body = path.read_text(encoding="utf-8")
    for needle in needles:
        if needle not in body:
            fail(f"{path.relative_to(ROOT)} missing required historical marker {needle!r}")
    return body


def main() -> None:
    disposition = require(
        ROOT / "docs/security/V2_4_0_RUSTSEC_WARNING_DISPOSITION.md",
        "historical / superseded; not an active current-lock security gate",
        "Historical review deadline: `2026-08-31 UTC` — expired.",
        "`linkme 0.2.10` / `linkme-impl 0.2.10` are absent",
        "`lru 0.12.5` is absent",
        "`atty 0.2.14` remains visible",
        "docs/security/V3_BINCODE_MIGRATION_PLAN.md",
        "No warning ID is added to `.cargo/audit.toml`.",
    )

    # Preserve old provenance without pretending those packages remain current.
    for historical in (
        "`RUSTSEC-2024-0407`",
        "`linkme 0.2.10`",
        "`RUSTSEC-2026-0002`",
        "`RUSTSEC-2026-0253`",
        "`lru 0.12.5`",
    ):
        if historical not in disposition:
            fail(f"historical provenance lost: {historical}")

    active_doc = require(
        ROOT / "docs/security/V3_DEPENDENCY_SECURITY.md",
        "Issue authority: #1127",
        "lru 0.12.5",
        "linkme 0.2.10",
        "atty 0.2.14",
        "final launch security approval",
    )
    active_validator = require(
        ROOT / "scripts/validate_v3_dependency_security.py",
        '("lru", "0.12.5")',
        '("linkme", "0.2.10")',
        '("atty", "0.2.14")',
        "final_v3_launch_security_ready",
    )
    require(
        ROOT / ".github/workflows/dependency-audit.yml",
        "python scripts/validate_v3_dependency_security.py",
        "rustup toolchain install 1.91.0",
    )

    # Ensure the historical record cannot silently reclaim authority.
    if "Status: active" in disposition:
        fail("historical v2.4 record still claims active status")
    if "This remediation does **not** close #1127" not in active_doc:
        fail("active v3 record lost the explicit no-close boundary")
    if "REQUIRED_VISIBLE_BLOCKERS" not in active_validator:
        fail("active v3 validator lost visible blocker accounting")

    print(
        "PASS: v2.4 RustSec disposition is historical-only; active dependency "
        "security authority remains the v3 exact-candidate gate"
    )


if __name__ == "__main__":
    main()
