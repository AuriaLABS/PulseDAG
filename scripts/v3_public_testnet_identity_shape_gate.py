#!/usr/bin/env python3
"""Offline shape check for a PROPOSED native-v3 public-testnet identity.

This is not genesis authentication, artifact attestation, consensus validation,
monetary-v3 activation, GO authorization, or a deployable config renderer.
It NEVER contacts nodes, writes configuration, toggles env flags or grants GO.
"""
from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

SCHEMA = "pulsedag.v3.public-testnet-identity-draft.v1"
POLICY_SHA256 = "134009249c301682df78d0c950fc1a70604eeddb9396361e9594e6fac121680b"
CADENCE_SHA256 = "6c571a95c57b7987e6a65d744d2abc9ed83c2a064ddb53b42e50cf1d6d03277f"
SHA256 = re.compile(r"^[0-9a-f]{64}$")
GIT_SHA = re.compile(r"^[0-9a-f]{40}$")
TESTNET_CHAIN = re.compile(r"^pulsedag-public-testnet-v3(?:-[a-z0-9]+)*$")
TESTNET_DOMAIN = re.compile(r"^pulsedag:v3:public-testnet:[a-z0-9_-]{4,64}$")
MAINNET_CHAIN = re.compile(r"^pulsedag-mainnet-v3(?:-[a-z0-9]+)*$")
MAINNET_DOMAIN = re.compile(r"^pulsedag:v3:mainnet:[a-z0-9_-]{4,64}$")
PLACEHOLDERS = ("TBD", "PLACEHOLDER", "FREEZE_REQUIRED", "REPLACE_ME", "EXAMPLE")
MAX_MANIFEST_BYTES = 64 * 1024
REQUIRED = frozenset({
    "schema", "status", "network_profile", "chain_id", "genesis_hash",
    "genesis_timestamp_ns", "signing_domain", "consensus_candidate",
    "source_git_sha", "node_artifact_sha256", "miner_artifact_sha256",
    "config_sha256", "monetary_policy_sha256", "cadence_sha256",
    "bootnode_peer_ids", "monetary_runtime_enabled", "contracts_enabled",
    "admin_enabled", "public_testnet_ready",
    "thirty_day_clock_started", "allow_legacy_fallback",
    "future_mainnet_identity",
})


class CandidateError(ValueError):
    """The candidate manifest cannot be used even as an offline identity draft."""


def reject(reason: str) -> None:
    raise CandidateError(reason)


def _exact_keys(value: object, keys: frozenset[str], label: str) -> dict:
    if type(value) is not dict:
        reject(f"{label}: expected JSON object")
    provided = set(value)
    if provided != keys:
        reject(f"{label}: missing={sorted(keys - provided)} extra={sorted(provided - keys)}")
    return value


def _text(value: object, label: str) -> str:
    if type(value) is not str or not value or value != value.strip():
        reject(f"{label}: expected nonempty trimmed string")
    if any(token.lower() in value.lower() for token in PLACEHOLDERS):
        reject(f"{label}: unresolved placeholder")
    if any(ord(c) < 33 or ord(c) > 126 for c in value):
        reject(f"{label}: invalid or non-ASCII character")
    return value


def _hex(value: object, label: str, git: bool = False) -> str:
    value = _text(value, label)
    pattern = GIT_SHA if git else SHA256
    if not pattern.fullmatch(value) or value == "0" * len(value):
        reject(f"{label}: expected nonzero canonical lowercase hex digest")
    return value


def _false(value: object, label: str) -> None:
    # bool alone; JSON zero, empty strings and null do not satisfy this gate.
    if value is not False:
        reject(f"{label}: must be JSON false")


def _no_duplicates(pairs: list[tuple[str, object]]) -> dict:
    result = {}
    for key, value in pairs:
        if key in result:
            reject(f"duplicate JSON field: {key}")
        result[key] = value
    return result


def parse_manifest(raw: bytes) -> object:
    if not raw or len(raw) > MAX_MANIFEST_BYTES:
        reject("manifest missing or exceeds 64KiB")
    try:
        return json.loads(raw.decode("utf-8"), object_pairs_hook=_no_duplicates)
    except (UnicodeDecodeError, json.JSONDecodeError) as exc:
        reject(f"invalid UTF-8 JSON: {exc}")


def check_candidate(candidate: object) -> dict:
    m = _exact_keys(candidate, REQUIRED, "candidate")
    if _text(m["schema"], "schema") != SCHEMA:
        reject("unsupported schema")
    if _text(m["status"], "status") != "UNAPPROVED_DRAFT":
        reject("draft review must not claim a frozen or GO status")
    if _text(m["network_profile"], "network_profile") != "public-testnet-v3":
        reject("only the proposed v3 public testnet profile is supported")
    if _text(m["consensus_candidate"], "consensus_candidate") != "native-v3-unapproved":
        reject("cannot use legacy/ghostdag_v1 or claim live native-v3 consensus")
    chain_id = _text(m["chain_id"], "chain_id")
    if not TESTNET_CHAIN.fullmatch(chain_id):
        reject("chain_id is not in the v3 public testnet namespace")
    domain = _text(m["signing_domain"], "signing_domain")
    if not TESTNET_DOMAIN.fullmatch(domain) or domain == chain_id:
        reject("signing domain is not a distinct testnet-only namespace")

    genesis = _hex(m["genesis_hash"], "genesis_hash")
    if (type(m["genesis_timestamp_ns"]) is not int
            or not 0 < m["genesis_timestamp_ns"] < (1 << 64)):
        reject("genesis_timestamp_ns must be a positive u64 integer")

    _hex(m["source_git_sha"], "source_git_sha", git=True)
    node = _hex(m["node_artifact_sha256"], "node_artifact_sha256")
    miner = _hex(m["miner_artifact_sha256"], "miner_artifact_sha256")
    _hex(m["config_sha256"], "config_sha256")
    if node == miner:
        reject("node and miner cannot have the same artifact digest")
    if _hex(m["monetary_policy_sha256"], "monetary_policy_sha256") != POLICY_SHA256:
        reject("monetary policy fingerprint does not match the current candidate")
    if _hex(m["cadence_sha256"], "cadence_sha256") != CADENCE_SHA256:
        reject("cadence fingerprint does not match the current candidate")

    bootnodes = m["bootnode_peer_ids"]
    if type(bootnodes) is not list or not 2 <= len(bootnodes) <= 16:
        reject("require 2-16 distinct proposed public-testnet bootnode identities")
    peers = [_text(peer, "bootnode_peer_id") for peer in bootnodes]
    if any(not 16 <= len(peer) <= 128 for peer in peers) or len(set(peers)) != len(peers):
        reject("invalid or duplicate bootnode peer identity")

    for field in (
        "monetary_runtime_enabled", "contracts_enabled", "admin_enabled",
        "public_testnet_ready", "thirty_day_clock_started", "allow_legacy_fallback",
    ):
        _false(m[field], field)

    # Mainnet need not have a frozen identity until AFTER public testnet;
    # if one is supplied, collision is still prohibited immediately.
    other = m["future_mainnet_identity"]
    if other is not None:
        v = _exact_keys(other, frozenset({"chain_id", "genesis_hash", "signing_domain"}), "mainnet")
        other_chain = _text(v["chain_id"], "mainnet.chain_id")
        other_genesis = _hex(v["genesis_hash"], "mainnet.genesis_hash")
        other_domain = _text(v["signing_domain"], "mainnet.signing_domain")
        if not MAINNET_CHAIN.fullmatch(other_chain):
            reject("future mainnet chain_id must belong to the mainnet-v3 namespace")
        if not MAINNET_DOMAIN.fullmatch(other_domain):
            reject("future mainnet signing domain must be mainnet-v3 scoped")
        if other_chain == chain_id or other_genesis == genesis or other_domain == domain:
            reject("public testnet identity overlaps future mainnet identity")

    return {
        "schema_shape": "PASS_ONLY",
        "network": "public-testnet-v3",
        "source_and_artifacts_authenticated": False,
        "genesis_and_consensus_verified": False,
        "future_mainnet_identity_shape_checked": other is not None,
        "mainnet_cross_identity_frozen": False,  # A draft never authenticates or freezes mainnet.
        "public_testnet_launch_authorized": False,
        "mainnet_launch_authorized": False,
        "monetary_runtime_enabled": False,
        "contracts_enabled": False,
        "explanation": "UNAPPROVED offline JSON shape only; exact artifact/genesis/consensus checks, security, operators and explicit #781 GO still required",
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("candidate_json", type=Path, help="offline candidate manifest; never a live env file")
    args = parser.parse_args()
    try:
        raw = args.candidate_json.read_bytes()
        result = check_candidate(parse_manifest(raw))
    except (CandidateError, OSError) as exc:
        print(f"FAIL (NO-GO): {exc}", file=sys.stderr)
        return 1
    print(json.dumps(result, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
