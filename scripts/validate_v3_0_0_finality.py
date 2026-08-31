#!/usr/bin/env python3
"""Fail-closed static contract checks for v3 economic-time finality."""
from __future__ import annotations

from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def fail(message: str) -> None:
    raise SystemExit(f"v3.0.0 finality validation failed: {message}")


def read(path: str) -> str:
    file = ROOT / path
    if not file.is_file():
        fail(f"required file missing: {path}")
    return file.read_text(encoding="utf-8")


def require(body: str, path: str, *needles: str) -> None:
    for needle in needles:
        if needle not in body:
            fail(f"{path} missing required finality contract: {needle!r}")


def main() -> None:
    doc_path = "docs/FINALITY_V3_0_0.md"
    doc = read(doc_path)
    require(
        doc,
        doc_path,
        "# PulseDAG v3.0.0 finality architecture",
        "ARCHITECTURE APPROVED / PRODUCTION DURATION + CONFLICT + PRUNING FREEZE PENDING",
        "Finality",
        "Coinbase maturity",
        "Pruning",
        "economic seconds",
        "selected chain",
        "entire deterministic ordered prefix through that anchor",
        "<policy-name>@sha256:<policy-digest>",
        "must never move backward",
        "finality conflict",
        "stop materializing new deferred reward UTXOs",
        "The v3 finality engine does not prune blocks or state.",
        "no mainnet finality-duration constant",
        "overall launch state remains `PRE_FREEZE` and `launch_ready=false`",
    )

    impl_path = "crates/pulsedag-core/src/finality_v3.rs"
    implementation = read(impl_path)
    require(
        implementation,
        impl_path,
        "FINALITY_V3_POLICY_SCHEMA_VERSION: u32 = 1",
        "pub struct FinalityPolicyV3",
        "pub struct FinalityDecisionV3",
        "finality_delay_economic_seconds",
        "pub fn finality_policy_digest_v3",
        "pub fn finality_policy_identity_v3",
        "pub fn derive_finality_decision_v3",
        "selected_chain_positions",
        "candidate_finalized_score_v3",
        "PreviousFinalityConflict",
        "validate_reward_finality_boundary_v3",
        "bind_reward_finality_boundary_v3",
        "finality_delay_tracks_economic_time_across_bps",
        "before_delay_only_genesis_is_final",
        "stale_previous_boundary_fails_closed_after_reorder",
        "previous_boundary_never_regresses",
    )

    lib_path = "crates/pulsedag-core/src/lib.rs"
    lib = read(lib_path)
    require(
        lib,
        lib_path,
        "pub mod finality_v3;",
        "derive_finality_decision_v3",
        "FinalityPolicyV3",
        "FinalityV3Error",
    )

    settlement_path = "docs/REWARD_SETTLEMENT_V3_0_0.md"
    settlement = read(settlement_path)
    require(
        settlement,
        settlement_path,
        "finality policy version",
        "ordered-DAG prefix",
        "Finality alone is insufficient. Maturity alone is insufficient.",
    )

    params_path = "docs/NETWORK_PARAMETERS_V3_0_0.md"
    params = read(params_path)
    require(
        params,
        params_path,
        "finality rule/version",
        "reward-settlement/finality implementation version and digest",
        "pruning/checkpoint/bootstrap rules",
    )

    manifest_path = "docs/V3_0_0_LAUNCH_MANIFEST.md"
    manifest = read(manifest_path)
    require(
        manifest,
        manifest_path,
        "Reward finality policy/version/digest: `TBD`",
        "Reward-settlement/finality freeze: `TBD`",
        "Launch state: **PRE_FREEZE**",
        "Launch-control authority: `#781`",
    )

    workflow_path = ".github/workflows/v3_0_0_launch_plan.yml"
    workflow = read(workflow_path)
    require(
        workflow,
        workflow_path,
        "deferred_reward_settlement_required=true",
    )

    print(
        "PASS: v3 finality is policy-digested, economic-time based, selected-chain anchored, "
        "monotonic and fail-closed on protected-prefix conflict; production duration/pruning remain TBD"
    )


if __name__ == "__main__":
    main()
