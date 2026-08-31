#!/usr/bin/env python3
"""Fail-closed static validator for the integrated v3.0.0 launch authority."""
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
        "v2.5.0 technical scope is incorporated into v3.0.0",
        "v2.6.0 technical scope is incorporated into v3.0.0",
        "P2P v3 and eclipse resistance",
        "NVIDIA CUDA backend",
        "AMD/ATI production backend",
        "1,000,000 valid DAG blocks",
        "168 contiguous hours",
        "UTXO Covenants v1",
        "Contract Transaction v3",
        "PulseScript",
        "Deterministic Contract VM",
        "Based Applications",
        "PulseProgs / Verifiable Programs",
        "1,000,000 programmable transactions/operations",
        "30 accepted days of programmability-enabled exact-candidate burn-in evidence",
        "1 September 2026",
        "30-day public-testnet clock before mainnet",
    )
    if "v2.5.0 and v2.6.0 therefore remain useful requirement documents" not in roadmap:
        fail("roadmap no longer preserves v2.5/v2.6 as v3 input workstreams")

    require(
        "docs/ROADMAP_V2_5_0.md",
        "Network Scale, Production GPU Mining and Adversarial Resilience",
        "P2P v3 and eclipse resistance",
        "Production GPU mining: NVIDIA + AMD/ATI",
        "Million-block deterministic DAG replay",
    )

    require(
        "docs/ROADMAP_V2_6_0.md",
        "Programmability, Smart Contracts and Verifiable Applications",
        "UTXO Covenants v1",
        "Contract Transaction v3",
        "PulseScript",
        "Deterministic Contract VM",
        "Programmability million-transaction replay",
    )

    require(
        "README.md",
        "definitive public-launch target is **v3.0.0 in Q4 2026**",
        "v2.4.x -> v2.5.0 scale/resilience -> v2.6.0 programmability -> v3.0.0 integrated release",
        "v2.5 scale/P2P/GPU/high-cadence/replay/resilience gates",
        "v2.6 programmability/smart-contract/VM/assets/economics/replay gates",
        "mainnet and a parallel public testnet",
        "GO_V3_DUAL_LAUNCH",
        "PENDING_EXACT_CANDIDATE_EVIDENCE",
    )

    require(
        "docs/README.md",
        "v2.4.x -> v2.5.0 scale/resilience workstream -> v2.6.0 programmability workstream -> v3.0.0 integrated release",
        "ROADMAP_V2_5_0.md",
        "ROADMAP_V2_6_0.md",
        "ROADMAP_V3_0_0.md",
        "30 accepted days of programmability-enabled exact-candidate pre-launch evidence",
    )

    require(
        "docs/VERSION_MATRIX.md",
        "v2.5.0 workstream incorporated into v3.0.0",
        "v2.6.0 workstream incorporated into v3.0.0",
        "v2.4.x -> v2.5.0 scale/resilience workstream -> v2.6.0 programmability workstream -> v3.0.0 definitive release",
        "Mandatory technical milestone incorporated into v3.0.0",
        "GO_V3_DUAL_LAUNCH",
        "PENDING_EXACT_CANDIDATE_EVIDENCE",
    )

    require(
        "docs/ROADMAP_V3_0_LONG_LIVED_CORE.md",
        "v2.4.x -> v2.5.0 scale/resilience -> v2.6.0 programmability -> v3.0.0 integrated release",
        "v2.5.0 workstream — mandatory input to v3",
        "v2.6.0 workstream — mandatory input to v3",
        ">=1,000,000-block deterministic DAG replay",
        ">=1,000,000 programmable-operation deterministic replay",
        "GO_V3_DUAL_LAUNCH",
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

    print("PASS: integrated v2.5 + v2.6 -> v3.0.0 Q4 dual-network launch authority is internally consistent")


if __name__ == "__main__":
    main()
