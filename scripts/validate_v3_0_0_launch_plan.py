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
        "Monetary/economic policy freeze",
        "Freeze two independent public network identities",
        "1 September 2026",
        "30-day public-testnet clock before mainnet",
    )
    if "v2.5.0 and v2.6.0 therefore remain useful requirement documents" not in roadmap:
        fail("roadmap no longer preserves v2.5/v2.6 as v3 input workstreams")

    require(
        "docs/ROADMAP_V2_5_0.md",
        "APPROVED MANDATORY V3.0.0 WORKSTREAM",
        "v2.4.x -> v2.5.0 scale/resilience -> v2.6.0 programmability -> v3.0.0 integrated release",
        "P2P v3 and eclipse resistance",
        "Production GPU mining: NVIDIA + AMD/ATI",
        "Million-block deterministic DAG replay",
        "168 contiguous hours",
        "Integrated v3 pre-launch network acceptance",
        "V2_5_WORKSTREAM_PASS",
        "It is **not** `GO_V3_DUAL_LAUNCH`",
    )

    require(
        "docs/ROADMAP_V2_6_0.md",
        "APPROVED MANDATORY V3.0.0 WORKSTREAM",
        "v2.4.x -> v2.5.0 scale/resilience -> v2.6.0 programmability -> v3.0.0 integrated release",
        "UTXO Covenants v1",
        "Contract Transaction v3",
        "PulseScript",
        "Deterministic Contract VM",
        "Programmability million-transaction replay",
        "30-day programmability exact-candidate burn-in",
        "V2_6_WORKSTREAM_PASS",
        "It is **not** `GO_V3_DUAL_LAUNCH`",
    )

    require(
        "docs/MONETARY_POLICY_V3_0_0.md",
        "PRE-FREEZE / LAUNCH-BLOCKING UNTIL APPROVED",
        "GENESIS_SUPPLY = 1_000_000_000",
        "INITIAL_BLOCK_SUBSIDY = 50",
        "SUBSIDY_HALVING_INTERVAL = 210_000",
        "canonical consensus index",
        "Coinbase maturity",
        "development `genesis-treasury` allocation is not authorized",
    )

    require(
        "docs/GENESIS_V3_0_0.md",
        "NO PRODUCTION GENESIS EXISTS YET",
        "exact timestamp",
        "two clean independent executions",
        "production generator MUST reject placeholder destinations such as `genesis-treasury`",
        "mainnet genesis hash != testnet genesis hash",
    )

    require(
        "docs/NETWORK_PARAMETERS_V3_0_0.md",
        "PRE-FREEZE / LAUNCH-BLOCKING",
        "Mainnet identity — final values TBD",
        "Parallel-testnet identity — final values TBD",
        "wallet signing/broadcast fails closed on network mismatch",
        "`GO_V3_DUAL_LAUNCH` is invalid while any launch-required field remains `TBD`.",
    )

    manifest = require(
        "docs/V3_0_0_LAUNCH_MANIFEST.md",
        "Launch state:",
        "Monetary policy freeze",
        "Mainnet genesis and network identity",
        "Parallel-testnet genesis and network identity",
        "Mandatory separation assertions",
        "Launch-control authority: `#781`",
        "Launch boundary — populate only after GO",
    )
    if "Launch state: **PRE_FREEZE**" not in manifest and "Launch state: **FROZEN**" not in manifest:
        fail("launch manifest state must be PRE_FREEZE or FROZEN")

    require(
        "docs/runbooks/V3_0_0_GENESIS_CEREMONY.md",
        "PRE-FREEZE / DO NOT START PUBLIC NETWORKS",
        "Any mismatch is a hard stop.",
        "No difference, even one atomic unit, is acceptable.",
        "Launch state` to `FROZEN`",
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
        "v2.4.x -> v2.5.0 scale/resilience -> v2.6.0 programmability -> v3.0.0 integrated release",
        ">=1,000,000-block deterministic DAG replay",
        ">=1,000,000 programmable-operation deterministic replay",
        "30 accepted days of programmability-enabled exact-candidate pre-launch evidence",
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
        "v2.5 scale/resilience/P2P/GPU-mining workstream",
        "v2.6 programmability/smart-contract/verifiable-application workstream",
        "GO_V3_DUAL_LAUNCH",
        "CPU/NVIDIA/AMD PoW implementations",
        "contract/VM/proof execution must be deterministic",
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
        "v2.5 scale/resilience/GPU requirements",
        "v2.6 programmability/smart-contract requirements",
        "Mainnet and parallel-testnet identities remain explicitly separated",
        "CPU/NVIDIA/AMD PoW equivalence",
        "Contract/application/proof execution remains deterministic and resource bounded",
        "Production genesis uses an exact frozen timestamp/input manifest",
        "GO_V3_DUAL_LAUNCH` is not claimed while the freeze validator reports `launch_ready=false",
    )

    if not (ROOT / "scripts/validate_v3_0_0_network_freeze.py").is_file():
        fail("missing monetary/genesis/network freeze validator")

    print("PASS: integrated v2.5 + v2.6 -> v3.0.0 Q4 dual-network launch authority is internally consistent")


if __name__ == "__main__":
    main()
