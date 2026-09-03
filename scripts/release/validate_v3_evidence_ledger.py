#!/usr/bin/env python3
"""Validate PulseDAG v3 exact-candidate launch evidence ledgers.

This validator intentionally uses only the Python standard library so the
release-evidence gate does not depend on an unfrozen third-party Python graph.
"""

from __future__ import annotations

import argparse
import copy
import json
import re
import sys
from datetime import datetime
from pathlib import Path
from typing import Any

FORMAT = "pulsedag-v3-evidence-ledger"
FORMAT_VERSION = 1
PENDING_DECISION = "PENDING_V3_DUAL_LAUNCH"
FINAL_DECISIONS = {
    "GO_V3_DUAL_LAUNCH",
    "DELAY_V3_DUAL_LAUNCH",
    "NO_GO_V3_DUAL_LAUNCH",
}
HEX_RE = re.compile(r"^[0-9a-f]+$")
SHA256_RE = re.compile(r"^sha256:[0-9a-f]{64}$")
COMMIT_RE = re.compile(r"^[0-9a-f]{40}$|^[0-9a-f]{64}$")
RFC3339_UTC_RE = re.compile(
    r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{1,9})?Z$"
)


class LedgerError(ValueError):
    pass


def fail(message: str) -> None:
    raise LedgerError(message)


def require_dict(value: Any, path: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        fail(f"{path} must be an object")
    return value


def require_list(value: Any, path: str) -> list[Any]:
    if not isinstance(value, list):
        fail(f"{path} must be an array")
    return value


def require_str(value: Any, path: str) -> str:
    if not isinstance(value, str) or not value or value.strip() != value:
        fail(f"{path} must be a non-empty, trimmed string")
    return value


def require_bool(value: Any, path: str) -> bool:
    if not isinstance(value, bool):
        fail(f"{path} must be boolean")
    return value


def require_int(value: Any, path: str) -> int:
    if type(value) is not int:
        fail(f"{path} must be an integer")
    return value


def require_keys(obj: dict[str, Any], path: str, keys: set[str]) -> None:
    missing = sorted(keys - obj.keys())
    if missing:
        fail(f"{path} missing required keys: {', '.join(missing)}")


def require_only_keys(obj: dict[str, Any], path: str, allowed: set[str]) -> None:
    unknown = sorted(obj.keys() - allowed)
    if unknown:
        fail(f"{path} contains unknown keys: {', '.join(unknown)}")


def require_commit(value: Any, path: str) -> str:
    text = require_str(value, path)
    if not COMMIT_RE.fullmatch(text):
        fail(f"{path} must be a canonical lowercase 40- or 64-hex object id")
    return text


def require_sha256(value: Any, path: str) -> str:
    text = require_str(value, path)
    if not SHA256_RE.fullmatch(text):
        fail(f"{path} must use canonical sha256:<64 lowercase hex> form")
    return text


def parse_timestamp(value: Any, path: str) -> datetime:
    text = require_str(value, path)
    if not RFC3339_UTC_RE.fullmatch(text):
        fail(
            f"{path} must be strict UTC RFC3339 "
            "YYYY-MM-DDTHH:MM:SS[.fraction]Z"
        )
    try:
        return datetime.fromisoformat(text[:-1] + "+00:00")
    except ValueError as exc:
        raise LedgerError(f"{path} is not a valid calendar timestamp") from exc


def validate_protocol_identities(value: Any) -> None:
    obj = require_dict(value, "candidate.protocol_identities")
    required = {
        "p2p",
        "transaction",
        "mining",
        "contract",
        "vm",
        "proof",
        "storage",
        "monetary_policy_digest",
    }
    require_keys(obj, "candidate.protocol_identities", required)
    require_only_keys(obj, "candidate.protocol_identities", required)
    for key in required - {"monetary_policy_digest"}:
        require_str(obj[key], f"candidate.protocol_identities.{key}")
    require_sha256(
        obj["monetary_policy_digest"],
        "candidate.protocol_identities.monetary_policy_digest",
    )


def validate_network(name: str, value: Any) -> dict[str, Any]:
    path = f"networks.{name}"
    obj = require_dict(value, path)
    allowed = {
        "network_profile",
        "chain_id",
        "genesis_hash",
        "config_digest",
        "bootnode_identity_digest",
        "signing_domain",
        "application_domain",
    }
    require_keys(obj, path, allowed)
    require_only_keys(obj, path, allowed)
    require_str(obj["network_profile"], f"{path}.network_profile")
    require_str(obj["chain_id"], f"{path}.chain_id")
    genesis = require_str(obj["genesis_hash"], f"{path}.genesis_hash")
    if not HEX_RE.fullmatch(genesis) or len(genesis) < 32:
        fail(f"{path}.genesis_hash must be canonical lowercase hex")
    require_sha256(obj["config_digest"], f"{path}.config_digest")
    require_sha256(obj["bootnode_identity_digest"], f"{path}.bootnode_identity_digest")
    require_str(obj["signing_domain"], f"{path}.signing_domain")
    require_str(obj["application_domain"], f"{path}.application_domain")
    return obj


def validate_configs(value: Any) -> set[str]:
    configs = require_list(value, "configs")
    if not configs:
        fail("configs must contain the frozen network/evidence configuration registry")
    seen_names: set[str] = set()
    seen_digests: set[str] = set()
    required = {"name", "role", "sha256"}
    for index, raw in enumerate(configs):
        path = f"configs[{index}]"
        obj = require_dict(raw, path)
        require_keys(obj, path, required)
        require_only_keys(obj, path, required)
        name = require_str(obj["name"], f"{path}.name")
        require_str(obj["role"], f"{path}.role")
        digest = require_sha256(obj["sha256"], f"{path}.sha256")
        if name in seen_names:
            fail(f"duplicate config name: {name}")
        if digest in seen_digests:
            fail(f"duplicate config sha256: {digest}")
        seen_names.add(name)
        seen_digests.add(digest)
    return seen_digests


def validate_artifacts(value: Any) -> set[str]:
    artifacts = require_list(value, "artifacts")
    if not artifacts:
        fail("artifacts must contain at least one exact-candidate artifact")
    seen_names: set[str] = set()
    seen_digests: set[str] = set()
    required = {
        "name",
        "platform",
        "sha256",
        "sbom_sha256",
        "provenance_sha256",
    }
    for index, raw in enumerate(artifacts):
        path = f"artifacts[{index}]"
        obj = require_dict(raw, path)
        require_keys(obj, path, required)
        require_only_keys(obj, path, required)
        name = require_str(obj["name"], f"{path}.name")
        require_str(obj["platform"], f"{path}.platform")
        digest = require_sha256(obj["sha256"], f"{path}.sha256")
        require_sha256(obj["sbom_sha256"], f"{path}.sbom_sha256")
        require_sha256(obj["provenance_sha256"], f"{path}.provenance_sha256")
        if name in seen_names:
            fail(f"duplicate artifact name: {name}")
        if digest in seen_digests:
            fail(f"duplicate artifact sha256: {digest}")
        seen_names.add(name)
        seen_digests.add(digest)
    return seen_digests


def validate_evidence(
    value: Any,
    candidate_source: str,
    candidate_tree: str,
    artifact_digests: set[str],
    config_digests: set[str],
) -> list[str]:
    records = require_list(value, "evidence")
    if not records:
        fail("evidence must contain at least one gate record")
    seen_gates: set[str] = set()
    statuses: list[str] = []
    required = {
        "gate_id",
        "status",
        "source_sha",
        "tree_sha",
        "artifact_sha256s",
        "config_sha256s",
        "started_at",
        "ended_at",
        "evidence_digest",
        "invalidated",
    }
    for index, raw in enumerate(records):
        path = f"evidence[{index}]"
        obj = require_dict(raw, path)
        require_keys(obj, path, required)
        require_only_keys(obj, path, required)
        gate = require_str(obj["gate_id"], f"{path}.gate_id")
        if gate in seen_gates:
            fail(f"duplicate evidence gate_id: {gate}")
        seen_gates.add(gate)
        status = require_str(obj["status"], f"{path}.status")
        if status not in {"PASS", "FAIL", "PENDING"}:
            fail(f"{path}.status must be PASS, FAIL or PENDING")
        statuses.append(status)
        source = require_commit(obj["source_sha"], f"{path}.source_sha")
        tree = require_commit(obj["tree_sha"], f"{path}.tree_sha")
        if source != candidate_source or tree != candidate_tree:
            fail(f"{path} belongs to a different candidate identity")
        artifact_refs = require_list(obj["artifact_sha256s"], f"{path}.artifact_sha256s")
        for ref_index, ref in enumerate(artifact_refs):
            digest = require_sha256(ref, f"{path}.artifact_sha256s[{ref_index}]")
            if digest not in artifact_digests:
                fail(f"{path} references unknown artifact digest {digest}")
        config_refs = require_list(obj["config_sha256s"], f"{path}.config_sha256s")
        for ref_index, ref in enumerate(config_refs):
            digest = require_sha256(ref, f"{path}.config_sha256s[{ref_index}]")
            if digest not in config_digests:
                fail(f"{path} references undeclared config digest {digest}")
        start = parse_timestamp(obj["started_at"], f"{path}.started_at")
        end = parse_timestamp(obj["ended_at"], f"{path}.ended_at")
        if end < start:
            fail(f"{path}.ended_at precedes started_at")
        require_sha256(obj["evidence_digest"], f"{path}.evidence_digest")
        invalidated = require_bool(obj["invalidated"], f"{path}.invalidated")
        if invalidated and status == "PASS":
            fail(f"{path} cannot be PASS after invalidation")
    return statuses


def validate_ledger(raw: Any) -> None:
    ledger = require_dict(raw, "ledger")
    top_keys = {
        "format",
        "format_version",
        "candidate_frozen",
        "candidate",
        "networks",
        "configs",
        "artifacts",
        "evidence",
        "decision",
    }
    require_keys(ledger, "ledger", top_keys)
    require_only_keys(ledger, "ledger", top_keys)
    if ledger["format"] != FORMAT:
        fail(f"format must be {FORMAT}")
    if require_int(ledger["format_version"], "format_version") != FORMAT_VERSION:
        fail(f"format_version must be {FORMAT_VERSION}")
    frozen = require_bool(ledger["candidate_frozen"], "candidate_frozen")

    candidate = require_dict(ledger["candidate"], "candidate")
    candidate_keys = {
        "release_version",
        "source_sha",
        "tree_sha",
        "version_file",
        "cargo_workspace_version",
        "protocol_identities",
    }
    require_keys(candidate, "candidate", candidate_keys)
    require_only_keys(candidate, "candidate", candidate_keys)
    if require_str(candidate["release_version"], "candidate.release_version") != "v3.0.0":
        fail("candidate.release_version must be v3.0.0")
    if require_str(candidate["version_file"], "candidate.version_file") != "v3.0.0":
        fail("candidate.version_file must be v3.0.0")
    if require_str(candidate["cargo_workspace_version"], "candidate.cargo_workspace_version") != "3.0.0":
        fail("candidate.cargo_workspace_version must be 3.0.0")
    source_sha = require_commit(candidate["source_sha"], "candidate.source_sha")
    tree_sha = require_commit(candidate["tree_sha"], "candidate.tree_sha")
    validate_protocol_identities(candidate["protocol_identities"])

    networks = require_dict(ledger["networks"], "networks")
    require_keys(networks, "networks", {"mainnet", "parallel_testnet"})
    require_only_keys(networks, "networks", {"mainnet", "parallel_testnet"})
    mainnet = validate_network("mainnet", networks["mainnet"])
    testnet = validate_network("parallel_testnet", networks["parallel_testnet"])
    for field in (
        "network_profile",
        "chain_id",
        "genesis_hash",
        "config_digest",
        "bootnode_identity_digest",
        "signing_domain",
        "application_domain",
    ):
        if mainnet[field] == testnet[field]:
            fail(f"mainnet and parallel_testnet must not share {field}")

    config_digests = validate_configs(ledger["configs"])
    for network_name, network in (("mainnet", mainnet), ("parallel_testnet", testnet)):
        if network["config_digest"] not in config_digests:
            fail(
                f"networks.{network_name}.config_digest is not declared "
                "in the frozen config registry"
            )

    artifact_digests = validate_artifacts(ledger["artifacts"])
    statuses = validate_evidence(
        ledger["evidence"],
        source_sha,
        tree_sha,
        artifact_digests,
        config_digests,
    )

    decision = require_str(ledger["decision"], "decision")
    if decision not in FINAL_DECISIONS | {PENDING_DECISION}:
        fail("decision is not a recognized #781 launch-control value")
    if not frozen and decision != PENDING_DECISION:
        fail("an unfrozen candidate may only carry the pending decision")
    if decision == "GO_V3_DUAL_LAUNCH":
        if not frozen:
            fail("GO requires candidate_frozen=true")
        if any(status != "PASS" for status in statuses):
            fail("GO requires every ledger evidence record to be PASS")


def sample_ledger() -> dict[str, Any]:
    source = "1" * 40
    tree = "2" * 40
    artifact = "sha256:" + "3" * 64
    digest4 = "sha256:" + "4" * 64
    digest5 = "sha256:" + "5" * 64
    digest6 = "sha256:" + "6" * 64
    digest7 = "sha256:" + "7" * 64
    digest8 = "sha256:" + "8" * 64
    digest9 = "sha256:" + "9" * 64
    digesta = "sha256:" + "a" * 64
    digestb = "sha256:" + "b" * 64
    return {
        "format": FORMAT,
        "format_version": FORMAT_VERSION,
        "candidate_frozen": True,
        "candidate": {
            "release_version": "v3.0.0",
            "source_sha": source,
            "tree_sha": tree,
            "version_file": "v3.0.0",
            "cargo_workspace_version": "3.0.0",
            "protocol_identities": {
                "p2p": "p2p-v3",
                "transaction": "tx-v3",
                "mining": "mining-v3",
                "contract": "contract-v3",
                "vm": "vm-v1",
                "proof": "proof-v1",
                "storage": "storage-v3",
                "monetary_policy_digest": digest4,
            },
        },
        "networks": {
            "mainnet": {
                "network_profile": "mainnet",
                "chain_id": "pulsedag-mainnet-v3",
                "genesis_hash": "1" * 64,
                "config_digest": digest5,
                "bootnode_identity_digest": digest6,
                "signing_domain": "pulsedag-mainnet-v3",
                "application_domain": "pulsedag-mainnet-app-v3",
            },
            "parallel_testnet": {
                "network_profile": "public-testnet",
                "chain_id": "pulsedag-public-testnet-v3",
                "genesis_hash": "2" * 64,
                "config_digest": digest7,
                "bootnode_identity_digest": digest8,
                "signing_domain": "pulsedag-public-testnet-v3",
                "application_domain": "pulsedag-public-testnet-app-v3",
            },
        },
        "configs": [
            {"name": "mainnet", "role": "network", "sha256": digest5},
            {
                "name": "parallel-testnet",
                "role": "network",
                "sha256": digest7,
            },
        ],
        "artifacts": [
            {
                "name": "pulsedagd-linux-x86_64",
                "platform": "linux-x86_64",
                "sha256": artifact,
                "sbom_sha256": digest9,
                "provenance_sha256": digesta,
            }
        ],
        "evidence": [
            {
                "gate_id": "synthetic-self-test",
                "status": "PASS",
                "source_sha": source,
                "tree_sha": tree,
                "artifact_sha256s": [artifact],
                "config_sha256s": [digest5, digest7],
                "started_at": "2026-09-01T00:00:00Z",
                "ended_at": "2026-09-01T01:00:00Z",
                "evidence_digest": digestb,
                "invalidated": False,
            }
        ],
        "decision": "GO_V3_DUAL_LAUNCH",
    }


def expect_invalid(value: dict[str, Any], label: str) -> None:
    try:
        validate_ledger(value)
    except LedgerError:
        return
    fail(f"self-test expected invalid ledger: {label}")


def run_self_test() -> None:
    valid = sample_ledger()
    validate_ledger(valid)

    mismatch = copy.deepcopy(valid)
    mismatch["evidence"][0]["source_sha"] = "f" * 40
    expect_invalid(mismatch, "cross-candidate evidence")

    network_collision = copy.deepcopy(valid)
    network_collision["networks"]["parallel_testnet"]["chain_id"] = valid["networks"]["mainnet"]["chain_id"]
    expect_invalid(network_collision, "network identity collision")

    undeclared_config = copy.deepcopy(valid)
    undeclared_config["evidence"][0]["config_sha256s"] = ["sha256:" + "c" * 64]
    expect_invalid(undeclared_config, "undeclared evidence config")

    malformed_timestamp = copy.deepcopy(valid)
    malformed_timestamp["evidence"][0]["started_at"] = "2026-09-01 00:00:00Z"
    expect_invalid(malformed_timestamp, "non-RFC3339 timestamp")

    boolean_version = copy.deepcopy(valid)
    boolean_version["format_version"] = True
    expect_invalid(boolean_version, "boolean format version")

    go_with_pending = copy.deepcopy(valid)
    go_with_pending["evidence"][0]["status"] = "PENDING"
    expect_invalid(go_with_pending, "GO with pending evidence")

    invalidated_pass = copy.deepcopy(valid)
    invalidated_pass["evidence"][0]["invalidated"] = True
    expect_invalid(invalidated_pass, "invalidated PASS evidence")

    unfrozen_pending = copy.deepcopy(valid)
    unfrozen_pending["candidate_frozen"] = False
    unfrozen_pending["decision"] = PENDING_DECISION
    validate_ledger(unfrozen_pending)

    unfrozen_go = copy.deepcopy(valid)
    unfrozen_go["candidate_frozen"] = False
    expect_invalid(unfrozen_go, "GO on unfrozen candidate")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("ledger", nargs="?", type=Path)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()

    try:
        if args.self_test:
            run_self_test()
        if args.ledger is not None:
            with args.ledger.open("r", encoding="utf-8") as handle:
                validate_ledger(json.load(handle))
        if not args.self_test and args.ledger is None:
            parser.error("provide a ledger path and/or --self-test")
    except (LedgerError, json.JSONDecodeError, OSError) as exc:
        print(f"v3 evidence ledger validation failed: {exc}", file=sys.stderr)
        return 1

    print("v3 evidence ledger validation passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
