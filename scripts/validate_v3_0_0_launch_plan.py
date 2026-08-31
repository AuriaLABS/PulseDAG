#!/usr/bin/env python3
"""Fail-closed static validator for the v3.0.0 dual-network launch authority."""
from __future__ import annotations

from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def fail(message: str) -> None:
    raise SystemExit(f"v3.0.0 launch-plan validation failed: {message}")


def require(path: str, *needles: str) -> str:
    p = ROOT / path
    if not p.is_file():
        fail(f"required file missing: {path}")
    body = p.read_text(encoding="utf-8")
    for needle in needles:
        if needle not in body:
            fail(f"{path} missing required launch contract: {needle!r}")
    return body


def main() -> None:
    roadmap = require(
        "docs/ROADMAP_V3_0_0.md",
        "Q4 2026",
        "mainnet and a parallel public testnet",
        "GO_V3_DUAL_LAUNCH",
        "DELAY_V3_DUAL_LAUNCH",
        "NO_GO_V3_DUAL_LAUNCH",
        "1 September 2026",
        "30-day public-testnet clock",
        "v2.4.0 and v2.4.1",
        "independent public network identities",
    )
    if "standalone public testnet first" in roadmap.lower():
        fail("roadmap reintroduced standalone-testnet-first sequencing")

    require(
        "README.md",
        "definitive public-launch target is **v3.0.0 in Q4 2026**",
        "mainnet and a parallel public testnet",
        "GO_V3_DUAL_LAUNCH",
        "PENDING_EXACT_CANDIDATE_EVIDENCE",
        "public_testnet_ready=false",
        "thirty_day_public_testnet_clock_started=false",
    )

    require(
        "docs/README.md",
        "definitive public-launch target is now **v3.0.0 in Q4 2026**",
        "GO_V3_DUAL_LAUNCH",
        "ROADMAP_V3_0_0.md",
        "V3_0_0_DUAL_NETWORK_LAUNCH.md",
        "ROADMAP_V3_0_LONG_LIVED_CORE.md",
    )

    require(
        "docs/VERSION_MATRIX.md",
        "Definitive public-launch target | **v3.0.0**",
        "Q4 2026 (October-December 2026)",
        "mainnet + parallel public testnet in one coordinated release window",
        "GO_V3_DUAL_LAUNCH",
        "not a mandatory release rung",
        "PENDING_EXACT_CANDIDATE_EVIDENCE",
    )

    require(
        "docs/ROADMAP_V3_0_LONG_LIVED_CORE.md",
        "SEQUENCING SUPERSEDED BY `ROADMAP_V3_0_0.md`",
        "mainnet and a parallel public testnet",
        "30-day stable-testnet burn-in before v3.0.0",
        "GO_V3_DUAL_LAUNCH",
        "not a prerequisite 30-day public launch phase",
    )

    require(
        "docs/runbooks/V3_0_0_DUAL_NETWORK_LAUNCH.md",
        "PRE-GO / Q4 2026 TARGET / NOT LAUNCHED",
        "mainnet + parallel-testnet",
        "GO_V3_DUAL_LAUNCH",
        "different chain IDs",
        "different genesis blocks/hashes",
        "first accepted mainnet block/height",
        "first accepted parallel-testnet block/height",
        "Hard-stop / rollback conditions",
    )

    require(
        "configs/v3-launch/README.md",
        "NOT A DEPLOYABLE CONFIGURATION",
        "mainnet + parallel public testnet",
        "chain ID",
        "genesis",
        "bootnode peer IDs",
        "Do not copy or promote `configs/public-testnet/`",
    )

    require(
        "SECURITY.md",
        "v3.0.0",
        "Q4 2026",
        "GO_V3_DUAL_LAUNCH",
        "mainnet and the parallel testnet",
        "public_testnet_ready=false",
        "thirty_day_public_testnet_clock_started=false",
    )

    require(
        "configs/public-testnet/README.md",
        "SUPERSEDED FOR PROJECT LAUNCH PLANNING",
        "GO_PUBLIC_TESTNET",
        "GO_V3_DUAL_LAUNCH",
        "configs/v3-launch/README.md",
    )

    require(
        "docs/runbooks/V2_4_0_PUBLIC_TESTNET_PREP.md",
        "SUPERSEDED FOR FINAL PROJECT LAUNCH",
        "GO_PUBLIC_TESTNET",
        "GO_V3_DUAL_LAUNCH",
        "PULSEDAG_THIRTY_DAY_PUBLIC_TESTNET_CLOCK_STARTED=true",
    )

    require(
        ".github/pull_request_template.md",
        "v3.0.0 launch impact",
        "mainnet / parallel testnet / both",
        "No existing v2.4.x tag/binary/evidence is relabeled as v3.0.0",
        "Mainnet and parallel-testnet identities remain explicitly separated",
    )

    print("PASS: v3.0.0 Q4 dual-network launch authority is internally consistent")


if __name__ == "__main__":
    main()
