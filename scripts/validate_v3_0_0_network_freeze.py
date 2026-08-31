#!/usr/bin/env python3
"""Validate v3.0.0 monetary/genesis/network freeze state without inventing launch values."""
from __future__ import annotations

import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
LAUNCH_BOUNDARY = "## Launch boundary — populate only after GO"


def fail(message: str) -> None:
    raise SystemExit(f"v3.0.0 network-freeze validation failed: {message}")


def read(path: str) -> str:
    p = ROOT / path
    if not p.is_file():
        fail(f"required file missing: {path}")
    return p.read_text(encoding="utf-8")


def require(body: str, path: str, *needles: str) -> None:
    for needle in needles:
        if needle not in body:
            fail(f"{path} missing required freeze contract: {needle!r}")


def has_tbd(body: str) -> bool:
    return bool(re.search(r"`TBD`|:\s*TBD\b", body))


def main() -> None:
    policy_path = "docs/MONETARY_POLICY_V3_0_0.md"
    policy = read(policy_path)
    require(
        policy,
        policy_path,
        "# PulseDAG v3.0.0 monetary policy",
        "MAINNET POLICY APPROVED / IMPLEMENTATION + TESTNET FREEZE PENDING",
        "maximum supply: **1,000,000,000.00000000 coins**",
        "atomic precision: **8 decimal places**",
        "genesis-issued spendable mainnet supply: **0 coins**",
        "premine / treasury / foundation allocation: **0 coins**",
        "year-1 mining budget: **500,000,000.00000000 coins**",
        "50% every one economic year",
        "31,536,000 economic seconds (365 days)",
        "15.854895991882293252",
        "coinbase maturity: **3,600 economic seconds",
        "consensus fee burn: **0%**",
        "tail emission after the terminal monetary schedule: **none**",
        "MAX_SUPPLY_ATOMS = 100_000_000_000_000_000",
        "changing the public DAG cadence must not accelerate or slow the monetary schedule",
        "Current implementation baseline",
        "GENESIS_SUPPLY = 1_000_000_000",
        "INITIAL_BLOCK_SUBSIDY = 50",
        "SUBSIDY_HALVING_INTERVAL = 210_000",
        "not compatible with the approved annual-economic-halving direction",
        "development `genesis-treasury` allocation is not authorized",
        "canonical consensus index",
        "Coinbase maturity",
        "issued_supply = approved_genesis_issuance + canonical_mining_issuance - consensus_burns",
        "No code default, test constant, private-testnet allocation or historical genesis output may silently become mainnet monetary policy.",
    )

    genesis_path = "docs/GENESIS_V3_0_0.md"
    genesis = read(genesis_path)
    require(
        genesis,
        genesis_path,
        "# PulseDAG v3.0.0 genesis contract",
        "Required genesis inputs",
        "Required deterministic outputs",
        "exact timestamp",
        "two clean independent executions",
        "production generator MUST reject placeholder destinations such as `genesis-treasury`",
        "different chain IDs",
        "different genesis hashes",
        "CI/freeze validator must fail if final mainnet and testnet chain IDs or genesis hashes are equal",
        "No embedded secrets",
    )

    params_path = "docs/NETWORK_PARAMETERS_V3_0_0.md"
    params = read(params_path)
    require(
        params,
        params_path,
        "# PulseDAG v3.0.0 network-parameter freeze",
        "Mainnet identity",
        "Parallel-testnet identity",
        "mainnet chain ID != testnet chain ID",
        "wallet signing/broadcast fails closed on network mismatch",
        "contract/application/proof domain separation prevents cross-network replay",
        "`GO_V3_DUAL_LAUNCH` is invalid while any launch-required field remains `TBD`.",
    )

    ceremony_path = "docs/runbooks/V3_0_0_GENESIS_CEREMONY.md"
    ceremony = read(ceremony_path)
    require(
        ceremony,
        ceremony_path,
        "# PulseDAG v3.0.0 genesis ceremony runbook",
        "No manual edit is allowed between independent generation runs.",
        "Any mismatch is a hard stop.",
        "No difference, even one atomic unit, is acceptable.",
        "Launch state` to `FROZEN`",
    )

    manifest_path = "docs/V3_0_0_LAUNCH_MANIFEST.md"
    manifest = read(manifest_path)
    match = re.search(r"^Launch state:\s*\*\*([A-Z_]+)\*\*\s*$", manifest, re.MULTILINE)
    if not match:
        fail("launch manifest is missing a valid Launch state")
    state = match.group(1)
    if state not in {"PRE_FREEZE", "FROZEN"}:
        fail(f"unsupported launch manifest state: {state}")

    require(
        manifest,
        manifest_path,
        "Release: `v3.0.0`",
        "Monetary policy freeze",
        "Policy version: `v3.0.0-mainnet-policy-v1`",
        "Maximum mainnet supply: `1,000,000,000.00000000 coins`",
        "Mainnet genesis-issued supply: `0 coins`",
        "Year-1 mining budget: `500,000,000.00000000 coins`",
        "Annual subsidy reduction: `50% every 31,536,000 economic seconds (365 days)`",
        "Coinbase maturity: `3,600 economic seconds`",
        "Consensus fee burn: `0% in v3.0.0`",
        "Mainnet genesis and network identity",
        "Parallel-testnet genesis and network identity",
        "Mandatory separation assertions",
        "Mainnet chain ID differs from testnet",
        "Mainnet genesis hash differs from testnet",
        "Launch-control authority: `#781`",
        "Launch boundary — populate only after GO",
        "Once `Launch state: FROZEN` is recorded",
    )

    if LAUNCH_BOUNDARY not in manifest:
        fail("launch manifest is missing the post-GO boundary marker")
    pre_go, post_go = manifest.split(LAUNCH_BOUNDARY, 1)

    if state == "PRE_FREEZE":
        if not has_tbd(pre_go):
            fail("PRE_FREEZE manifest unexpectedly contains no pre-GO TBD fields")
        print("PASS: v3.0.0 approved mainnet monetary policy and freeze contracts are present; launch_ready=false; state=PRE_FREEZE")
        return

    # FROZEN means the complete pre-GO identity/evidence set is immutable and
    # ready for #781 to decide GO. The policy and network matrices themselves
    # must also have transitioned away from placeholder values.
    if has_tbd(pre_go):
        fail("FROZEN launch manifest still contains pre-GO TBD fields")
    if has_tbd(policy):
        fail("FROZEN launch manifest but monetary-policy document still contains TBD fields")
    if has_tbd(params):
        fail("FROZEN launch manifest but network-parameter document still contains TBD fields")

    required_pass_assertions = (
        "Mainnet chain ID differs from testnet: `PASS`",
        "Mainnet signing/network domain differs from testnet: `PASS`",
        "Mainnet genesis hash differs from testnet: `PASS`",
        "Mainnet/testnet peer bootstrap cannot cross-connect by default: `PASS`",
        "Wallet cross-network signing/broadcast fails closed: `PASS`",
        "Miner cross-network job/submission fails closed: `PASS`",
        "Contract/proof/application replay is domain separated: `PASS`",
    )
    for assertion in required_pass_assertions:
        if assertion not in pre_go:
            fail(f"FROZEN manifest missing separation assertion: {assertion}")

    if "#781 decision:" in pre_go or "first accepted block" in pre_go:
        fail("post-GO launch-result fields leaked into the pre-GO freeze section")

    require(
        post_go,
        manifest_path,
        "#781 decision: `TBD`",
        "Decision UTC: `TBD`",
        "Mainnet first accepted block",
        "Parallel-testnet first accepted block",
    )

    print("PASS: v3.0.0 monetary/genesis/network freeze is complete; launch_ready=true; state=FROZEN; awaiting #781 decision")


if __name__ == "__main__":
    main()
