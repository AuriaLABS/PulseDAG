#!/usr/bin/env python3
"""Fail-closed static contract checks for v3 deferred reward settlement."""
from __future__ import annotations

from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def fail(message: str) -> None:
    raise SystemExit(f"v3.0.0 reward-settlement validation failed: {message}")


def read(path: str) -> str:
    file = ROOT / path
    if not file.is_file():
        fail(f"required file missing: {path}")
    return file.read_text(encoding="utf-8")


def require(body: str, path: str, *needles: str) -> None:
    for needle in needles:
        if needle not in body:
            fail(f"{path} missing required settlement contract: {needle!r}")


def main() -> None:
    doc_path = "docs/REWARD_SETTLEMENT_V3_0_0.md"
    doc = read(doc_path)
    require(
        doc,
        doc_path,
        "# PulseDAG v3.0.0 deferred reward settlement",
        "ARCHITECTURE APPROVED / PRODUCTION FINALITY + STATE-INTEGRATION FREEZE PENDING",
        "beneficiary output amount to be exactly `0`",
        "settlement_amount(M) = subsidy_atoms_for_score(M) + canonical_block_fees(M)",
        "Synthetic settlement outpoint",
        "deterministic digest of the complete ordered-DAG prefix through that score",
        "Finality alone is insufficient. Maturity alone is insufficient.",
        "no spendable reward UTXO exists",
        "production finality algorithm/version",
        "overall launch state remains `PRE_FREEZE` and `launch_ready=false`",
    )

    impl_path = "crates/pulsedag-core/src/reward_settlement_v3.rs"
    implementation = read(impl_path)
    require(
        implementation,
        impl_path,
        "REWARD_CLAIM_TRANSACTION_VERSION_V3: u32 = 3",
        "REWARD_SETTLEMENT_OUTPOINT_DOMAIN_V3",
        "REWARD_FINALITY_PREFIX_DOMAIN_V3",
        "reward claim output amount must be zero",
        "pub fn compute_reward_claim_txid_v3",
        "pub fn build_reward_claim_transaction_v3",
        "pub fn settlement_outpoint_v3",
        "pub fn bind_reward_finality_boundary_v3",
        "pub fn validate_reward_finality_boundary_v3",
        "pub fn derive_reward_settlement_snapshot_v3",
        "pub fn materializable_reward_utxos_v3",
        "subsidy_atoms_for_score",
        "economic_maturity_reached",
        "old_finality_binding_cannot_survive_a_reordered_prefix",
        "settlement_requires_both_finality_and_economic_maturity",
        "fees_are_carried_into_the_delayed_settlement_amount",
    )

    lib_path = "crates/pulsedag-core/src/lib.rs"
    lib = read(lib_path)
    require(lib, lib_path, "pub mod reward_settlement_v3;")

    score_path = "docs/MONETARY_SCORE_V3_0_0.md"
    score = read(score_path)
    require(
        score,
        score_path,
        "monetary position is **state-derived**",
        "fixed caller-visible coinbase amount are therefore not sufficient as the v3 monetary authority",
        "the final issued reward UTXO must be bound to the canonical settled position and consensus-derived amount",
    )

    manifest_path = "docs/V3_0_0_LAUNCH_MANIFEST.md"
    manifest = read(manifest_path)
    require(
        manifest,
        manifest_path,
        "state-derived/provisional before finality; exact settlement implementation digest `TBD`",
        "Reward-settlement/finality freeze: `TBD`",
        "Launch-control authority: `#781`",
    )

    workflow_path = ".github/workflows/v3_0_0_launch_plan.yml"
    workflow = read(workflow_path)
    require(
        workflow,
        workflow_path,
        "docs/REWARD_SETTLEMENT_V3_0_0.md",
        "crates/pulsedag-core/src/reward_settlement_v3.rs",
        "scripts/validate_v3_0_0_reward_settlement.py",
        "deferred_reward_settlement_required=true",
    )

    print(
        "PASS: v3 deferred reward claims are amountless, chain-bound and prefix-finality-bound; "
        "settlement remains non-live until production finality/state integration is frozen"
    )


if __name__ == "__main__":
    main()
