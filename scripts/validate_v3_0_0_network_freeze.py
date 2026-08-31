#!/usr/bin/env python3
"""Validate v3.0.0 monetary/genesis/network freeze state without inventing launch values."""
from __future__ import annotations

import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


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


def main() -> None:
    policy_path = "docs/MONETARY_POLICY_V3_0_0.md"
    policy = read(policy_path)
    require(
        policy,
        policy_path,
        "PRE-FREEZE / LAUNCH-BLOCKING UNTIL APPROVED",
        "GENESIS_SUPPLY = 1_000_000_000",
        "INITIAL_BLOCK_SUBSIDY = 50",
        "SUBSIDY_HALVING_INTERVAL = 210_000",
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
        "NO PRODUCTION GENESIS EXISTS YET",
        "exact timestamp",
        "two clean independent executions",
        "production generator MUST reject placeholder destinations such as `genesis-treasury`",
        "mainnet chain ID != testnet chain ID",
        "mainnet genesis hash != testnet genesis hash",
        "No embedded secrets",
    )

    params_path = "docs/NETWORK_PARAMETERS_V3_0_0.md"
    params = read(params_path)
    require(
        params,
        params_path,
        "PRE-FREEZE / LAUNCH-BLOCKING",
        "Mainnet identity — final values TBD",
        "Parallel-testnet identity — final values TBD",
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
        "PRE-FREEZE / DO NOT START PUBLIC NETWORKS",
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
        "Mainnet genesis and network identity",
        "Parallel-testnet genesis and network identity",
        "Mandatory separation assertions",
        "Mainnet chain ID differs from testnet",
        "Mainnet genesis hash differs from testnet",
        "#781 final-decision reference",
        "Once `Launch state: FROZEN` is recorded",
    )

    if state == "PRE_FREEZE":
        if "`TBD`" not in manifest:
            fail("PRE_FREEZE manifest unexpectedly contains no TBD launch fields")
        print("PASS: v3.0.0 freeze contracts are present; launch_ready=false; state=PRE_FREEZE")
        return

    # FROZEN is intentionally strict. A freeze cannot coexist with unresolved
    # placeholders or unasserted network-separation checks.
    if re.search(r"`TBD`|:\s*TBD\b", manifest):
        fail("FROZEN launch manifest still contains TBD fields")

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
        if assertion not in manifest:
            fail(f"FROZEN manifest missing separation assertion: {assertion}")

    if "#781 decision: `GO_V3_DUAL_LAUNCH`" not in manifest:
        fail("FROZEN manifest does not bind the exact #781 GO decision")

    print("PASS: v3.0.0 monetary/genesis/network freeze is complete; launch_ready=true; state=FROZEN")


if __name__ == "__main__":
    main()
