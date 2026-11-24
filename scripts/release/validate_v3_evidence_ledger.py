#!/usr/bin/env python3
"""Validate PulseDAG v3 exact-candidate launch evidence ledgers.

This validator intentionally uses only the Python standard library so the
release-evidence gate does not depend on an unfrozen third-party Python graph.
"""

from __future__ import annotations

import argparse
import copy
import hashlib
import json
import re
import sys
from datetime import datetime, timedelta
from pathlib import Path
from typing import Any

FORMAT = "pulsedag-v3-evidence-ledger"
FORMAT_VERSION = 2
STAGES = {"testnet", "mainnet"}
PENDING_DECISION = "PENDING"
GO_DECISIONS = {"testnet": "GO_PUBLIC_TESTNET", "mainnet": "GO_V3_MAINNET"}
REQUIRED_GATES = {
    "testnet": {
        "native-consensus-monetary", "local-native-regressions",
        "testnet-only-activation", "testnet-identity",
        "public-security", "operator-release", "contracts-inactive",
    },
    "mainnet": {
        "production-release-security", "native-replay-checkpoint",
        "production-adversarial-25n16m", "production-burn-in-168h",
        "contracts-ready-inactive", "mainnet-identity", "public-testnet-acceptance",
    },
}
DECISION_LINK_RE = re.compile(
    r"https://github\.com/AuriaLABS/PulseDAG/issues/781#issuecomment-[1-9][0-9]*"
)
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
        "stage",
        "prior_testnet",
        "decision_record",
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

    stage = require_str(ledger["stage"], "stage")
    if stage not in STAGES:
        fail("stage must be testnet or mainnet")
    networks = require_dict(ledger["networks"], "networks")
    expected_networks = {"testnet"} if stage == "testnet" else {"testnet", "mainnet"}
    require_keys(networks, "networks", expected_networks)
    require_only_keys(networks, "networks", expected_networks)
    identities = {name: validate_network(name, value) for name, value in networks.items()}
    if stage == "mainnet":
        for field in identities["mainnet"]:
            if identities["mainnet"][field] == identities["testnet"][field]:
                fail(f"mainnet and testnet must not share {field}")

    config_digests = validate_configs(ledger["configs"])
    active_config = identities[stage]["config_digest"]
    if active_config not in config_digests:
        fail("active network config digest is not declared")
    artifact_digests = validate_artifacts(ledger["artifacts"])
    statuses = validate_evidence(
        ledger["evidence"], source_sha, tree_sha, artifact_digests, config_digests,
    )

    prior = ledger["prior_testnet"]
    if stage == "testnet":
        if prior is not None:
            fail("testnet entry must not require or claim prior live testnet acceptance")
    else:
        validate_prior_testnet(prior, identities["testnet"])

    decision = require_str(ledger["decision"], "decision")
    if decision not in {PENDING_DECISION, "DELAY", "NO_GO", GO_DECISIONS[stage]}:
        fail("decision is not recognized for this stage")
    if not frozen and decision != PENDING_DECISION:
        fail("an unfrozen candidate may only carry the pending decision")
    record = ledger["decision_record"]
    if decision == GO_DECISIONS[stage]:
        if not frozen or any(status != "PASS" for status in statuses):
            fail("GO record requires a frozen candidate and all evidence PASS")
        gates = {entry["gate_id"]: entry for entry in ledger["evidence"]}
        if not REQUIRED_GATES[stage] <= gates.keys():
            fail("missing mandatory stage gate records")
        approved_at = validate_decision_record(record)
        for entry in gates.values():
            if active_config not in entry["config_sha256s"]:
                fail("GO evidence must reference the active network config")
            if parse_timestamp(entry["ended_at"], "evidence.ended_at") > approved_at:
                fail("GO decision precedes evidence completion")
        if stage == "mainnet":
            if record["issue_comment"] == prior["go_record"]["issue_comment"]:
                fail("mainnet GO must reference a separate authorization comment")
            if parse_timestamp(prior["period_end"], "prior_testnet.period_end") > approved_at:
                fail("mainnet decision precedes accepted testnet period")
            burn = gates["production-burn-in-168h"]
            if parse_timestamp(burn["ended_at"], "burn.end") - parse_timestamp(
                burn["started_at"], "burn.start"
            ) < timedelta(hours=168):
                fail("production burn-in evidence covers less than 168 hours")
    elif record is not None:
        fail("non-GO ledger must not carry a GO authorization record")


def validate_decision_record(raw: Any) -> datetime:
    record = require_dict(raw, "decision_record")
    keys = {"issue_comment", "decided_at"}
    require_keys(record, "decision_record", keys)
    require_only_keys(record, "decision_record", keys)
    link = require_str(record["issue_comment"], "decision_record.issue_comment")
    if not DECISION_LINK_RE.fullmatch(link):
        fail("decision record must reference a #781 comment")
    return parse_timestamp(record["decided_at"], "decision_record.decided_at")


def validate_prior_testnet(raw: Any, identity: dict[str, Any]) -> None:
    prior = require_dict(raw, "prior_testnet")
    keys = {
        "source_sha", "tree_sha", "config_digest", "ledger_digest",
        "go_record", "first_block_at", "period_start", "period_end",
        "acceptance_evidence_digest",
    }
    require_keys(prior, "prior_testnet", keys)
    require_only_keys(prior, "prior_testnet", keys)
    require_commit(prior["source_sha"], "prior_testnet.source_sha")
    require_commit(prior["tree_sha"], "prior_testnet.tree_sha")
    if require_sha256(prior["config_digest"], "prior_testnet.config_digest") != identity["config_digest"]:
        fail("prior testnet config does not match declared testnet identity")
    require_sha256(prior["ledger_digest"], "prior_testnet.ledger_digest")
    require_sha256(prior["acceptance_evidence_digest"], "prior_testnet.acceptance_evidence_digest")
    authorized = validate_decision_record(prior["go_record"])
    first = parse_timestamp(prior["first_block_at"], "prior_testnet.first_block_at")
    start = parse_timestamp(prior["period_start"], "prior_testnet.period_start")
    end = parse_timestamp(prior["period_end"], "prior_testnet.period_end")
    if not authorized <= first <= start <= end:
        fail("prior testnet dates must follow authorization and first accepted block")
    if end - start < timedelta(days=30):
        fail("prior testnet accepted period is shorter than 30 consecutive days")


def unique_object(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        if key in result:
            fail(f"duplicate JSON object key: {key}")
        result[key] = value
    return result


def parse_ledger_bytes(raw: bytes) -> dict[str, Any]:
    try:
        return require_dict(json.loads(raw.decode("utf-8"), object_pairs_hook=unique_object), "ledger")
    except UnicodeDecodeError as exc:
        raise LedgerError("ledger must be UTF-8 JSON") from exc


def verify_prior_ledger(mainnet: dict[str, Any], raw: bytes) -> None:
    prior = mainnet["prior_testnet"]
    if "sha256:" + hashlib.sha256(raw).hexdigest() != prior["ledger_digest"]:
        fail("prior testnet ledger bytes do not match the recorded digest")
    testnet = parse_ledger_bytes(raw)
    validate_ledger(testnet)
    if testnet["stage"] != "testnet" or testnet["decision"] != "GO_PUBLIC_TESTNET":
        fail("prior ledger must contain a valid testnet GO record")
    for key in ("source_sha", "tree_sha"):
        if testnet["candidate"][key] != prior[key]:
            fail(f"prior testnet ledger has a different {key}")
    if testnet["networks"]["testnet"] != mainnet["networks"]["testnet"]:
        fail("prior testnet ledger has a different network identity")
    if testnet["decision_record"] != prior["go_record"]:
        fail("prior testnet ledger has a different authorization record")



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
    ledger = {
        "format": FORMAT,
        "stage": "testnet",
        "prior_testnet": None,
        "decision_record": {"issue_comment": "https://github.com/AuriaLABS/PulseDAG/issues/781#issuecomment-1", "decided_at": "2026-09-01T01:00:00Z"},
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
            "testnet": {
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
        "decision": "GO_PUBLIC_TESTNET",
    }

    del ledger["networks"]["mainnet"]
    ledger["configs"] = [ledger["configs"][1]]
    template = ledger["evidence"][0]
    template["config_sha256s"] = [digest7]
    ledger["evidence"] = [dict(template, gate_id=gate) for gate in sorted(REQUIRED_GATES["testnet"])]
    return ledger


def expect_invalid(value: dict[str, Any], label: str) -> None:
    try:
        validate_ledger(value)
    except LedgerError:
        return
    fail(f"self-test expected invalid ledger: {label}")


def run_self_test() -> None:
    valid = sample_ledger()
    validate_ledger(valid)
    cases = [
        ("old format", lambda x: x.update(format_version=1)),
        ("old dual launch", lambda x: x.update(decision="GO_V3_DUAL_LAUNCH")),
        ("wrong stage GO", lambda x: x.update(decision="GO_V3_MAINNET")),
        ("missing gate", lambda x: x["evidence"].pop()),
        ("cross-candidate evidence", lambda x: x["evidence"][0].update(source_sha="f"*40)),
        ("undeclared config", lambda x: x["evidence"][0].update(config_sha256s=["sha256:"+"f"*64])),
        ("missing config binding", lambda x: x["evidence"][0].update(config_sha256s=[])),
        ("malformed timestamp", lambda x: x["evidence"][0].update(started_at="2026-09-01 00:00:00Z")),
        ("boolean version", lambda x: x.update(format_version=True)),
        ("pending gate", lambda x: x["evidence"][0].update(status="PENDING")),
        ("invalidated pass", lambda x: x["evidence"][0].update(invalidated=True)),
        ("unfrozen GO", lambda x: x.update(candidate_frozen=False)),
        ("missing GO record", lambda x: x.update(decision_record=None)),
        ("premature decision", lambda x: x["decision_record"].update(decided_at="2026-09-01T00:00:00Z")),
        ("unexpected mainnet", lambda x: x["networks"].update(mainnet=copy.deepcopy(x["networks"]["testnet"]))),
    ]
    for label, mutate in cases:
        bad = copy.deepcopy(valid)
        mutate(bad)
        expect_invalid(bad, label)
    pending = copy.deepcopy(valid)
    pending.update(candidate_frozen=False, decision=PENDING_DECISION, decision_record=None)
    validate_ledger(pending)

    main = copy.deepcopy(valid)
    main.update(stage="mainnet", decision="GO_V3_MAINNET")
    main["networks"]["mainnet"] = {
        key: ("sha256:"+"c"*64 if key == "config_digest" else
              "sha256:"+"d"*64 if key == "bootnode_identity_digest" else
              "3"*64 if key == "genesis_hash" else "mainnet-"+key)
        for key in main["networks"]["testnet"]
    }
    config = main["networks"]["mainnet"]["config_digest"]
    main["configs"].append({"name": "mainnet", "role": "network", "sha256": config})
    main["candidate"]["source_sha"] = "e"*40
    main["candidate"]["tree_sha"] = "f"*40
    main["evidence"] = [
        dict(valid["evidence"][0], gate_id=gate, source_sha="e"*40, tree_sha="f"*40,
             config_sha256s=[config], started_at="2026-10-02T00:00:00Z",
             ended_at="2026-10-09T00:00:00Z")
        for gate in sorted(REQUIRED_GATES["mainnet"])
    ]
    main["decision_record"] = {"issue_comment": "https://github.com/AuriaLABS/PulseDAG/issues/781#issuecomment-2", "decided_at": "2026-10-09T00:00:00Z"}
    prior_bytes = json.dumps(valid, sort_keys=True).encode("utf-8")
    main["prior_testnet"] = {
        "source_sha": valid["candidate"]["source_sha"],
        "tree_sha": valid["candidate"]["tree_sha"],
        "config_digest": valid["networks"]["testnet"]["config_digest"],
        "ledger_digest": "sha256:" + hashlib.sha256(prior_bytes).hexdigest(),
        "acceptance_evidence_digest": "sha256:"+"b"*64,
        "go_record": valid["decision_record"],
        "first_block_at": "2026-09-01T02:00:00Z",
        "period_start": "2026-09-01T02:00:00Z",
        "period_end": "2026-10-01T02:00:00Z",
    }
    validate_ledger(main)
    verify_prior_ledger(main, prior_bytes)
    for label, mutate in [
        ("reused authorization", lambda x: x["decision_record"].update(issue_comment=x["prior_testnet"]["go_record"]["issue_comment"])),
        ("missing prior", lambda x: x.update(prior_testnet=None)),
        ("short public period", lambda x: x["prior_testnet"].update(period_end="2026-09-30T02:00:00Z")),
        ("backdated period", lambda x: x["prior_testnet"].update(period_start="2026-09-01T00:00:00Z")),
        ("identity collision", lambda x: x["networks"]["mainnet"].update(chain_id=x["networks"]["testnet"]["chain_id"])),
        ("wrong prior identity", lambda x: x["prior_testnet"].update(config_digest="sha256:"+"f"*64)),
        ("short production burn-in", lambda x: next(e for e in x["evidence"] if e["gate_id"]=="production-burn-in-168h").update(ended_at="2026-10-08T00:00:00Z")),
    ]:
        bad = copy.deepcopy(main)
        mutate(bad)
        expect_invalid(bad, label)

    for label, raw in [
        ("modified prior bytes", prior_bytes + b" "),
        ("duplicate JSON key", b'{"stage":"testnet","stage":"mainnet"}'),
    ]:
        try:
            if label == "duplicate JSON key":
                parse_ledger_bytes(raw)
            else:
                verify_prior_ledger(main, raw)
        except LedgerError:
            continue
        fail(f"self-test expected rejection: {label}")
    altered = copy.deepcopy(valid)
    altered["candidate"]["source_sha"] = "9"*40
    for entry in altered["evidence"]:
        entry["source_sha"] = "9"*40
    altered_bytes = json.dumps(altered).encode("utf-8")
    wrong = copy.deepcopy(main)
    wrong["prior_testnet"]["ledger_digest"] = "sha256:" + hashlib.sha256(altered_bytes).hexdigest()
    try:
        verify_prior_ledger(wrong, altered_bytes)
    except LedgerError:
        pass
    else:
        fail("self-test expected rejection of different prior candidate")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("ledger", nargs="?", type=Path)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--prior-testnet-ledger", type=Path)
    args = parser.parse_args()

    try:
        if args.self_test:
            run_self_test()
        if args.ledger is not None:
            ledger = parse_ledger_bytes(args.ledger.read_bytes())
            validate_ledger(ledger)
            if ledger["stage"] == "mainnet" and ledger["decision"] == "GO_V3_MAINNET":
                if args.prior_testnet_ledger is None:
                    fail("mainnet GO validation requires --prior-testnet-ledger")
                verify_prior_ledger(ledger, args.prior_testnet_ledger.read_bytes())
            elif args.prior_testnet_ledger is not None:
                fail("prior ledger input is only used for mainnet GO validation")
        if not args.self_test and args.ledger is None:
            parser.error("provide a ledger path and/or --self-test")
    except (LedgerError, json.JSONDecodeError, OSError) as exc:
        print(f"v3 evidence ledger validation failed: {exc}", file=sys.stderr)
        return 1

    print("v3 evidence ledger validation passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
