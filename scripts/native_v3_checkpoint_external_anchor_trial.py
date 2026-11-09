#!/usr/bin/env python3
"""OFFLINE test-only checkpoint anchor/freshness boundary; NEVER grants network GO.

An externally supplied trusted pin is an INPUT ASSUMPTION, not authenticated here.
A forged or stale pin can defeat this check. No P2P, daemon, miner or disk restore.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from pathlib import Path

ANCHOR_SCHEMA = "pulsedag.v3.external-checkpoint-pin-trial.v1"
CLAIM_SCHEMA = "pulsedag.v3.untrusted-checkpoint-claim-trial.v1"
NETWORK = re.compile(r"^pulsedag-public-testnet-v3(?:-[a-z0-9]+)*$")
SHA256 = re.compile(r"^[0-9a-f]{64}$")
MAX_MANIFEST_BYTES = 64 * 1024
MAX_SNAPSHOT_BYTES = 8 * 1024 * 1024
ANCHOR_KEYS = frozenset({
    "schema", "status", "chain_id", "genesis_hash", "checkpoint_hash",
    "checkpoint_score", "minimum_accepted_score", "snapshot_sha256",
})
CLAIM_KEYS = frozenset({
    "schema", "status", "chain_id", "genesis_hash", "checkpoint_hash",
    "checkpoint_score", "snapshot_sha256", "contracts_enabled",
    "monetary_runtime_enabled",
})


class AnchorError(ValueError):
    """Fail-closed local comparison, not a production checkpoint validation."""


def reject(message: str) -> None:
    raise AnchorError(message)


def no_duplicate_fields(pairs: list[tuple[str, object]]) -> dict:
    result = {}
    for name, value in pairs:
        if name in result:
            reject(f"duplicate field: {name}")
        result[name] = value
    return result


def reject_json_constant(value: str) -> None:
    reject(f"non-JSON constant: {value}")


def read_bounded_json(path: Path) -> object:
    with path.open("rb") as handle:
        raw = handle.read(MAX_MANIFEST_BYTES + 1)
    if not raw or len(raw) > MAX_MANIFEST_BYTES:
        reject("manifest absent or over 64 KiB")
    try:
        return json.loads(
            raw.decode("utf-8"), object_pairs_hook=no_duplicate_fields,
            parse_constant=reject_json_constant,
        )
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        reject(f"invalid JSON manifest: {error}")


def exact_object(value: object, keys: frozenset[str], name: str) -> dict:
    if type(value) is not dict:
        reject(f"{name}: object required")
    actual = set(value)
    if actual != keys:
        reject(f"{name}: missing={sorted(keys-actual)} extra={sorted(actual-keys)}")
    return value


def digest_field(value: object, name: str) -> str:
    if type(value) is not str or SHA256.fullmatch(value) is None or value == "0" * 64:
        reject(f"{name}: nonzero lowercase SHA256 hex required")
    return value


def score_field(value: object, name: str) -> int:
    if type(value) is not int or not 0 < value < 1 << 64:
        reject(f"{name}: positive bounded unsigned score required")
    return value


def sha256_bounded_snapshot(path: Path) -> str:
    digest = hashlib.sha256()
    count = 0
    with path.open("rb") as handle:
        while True:
            block = handle.read(64 * 1024)
            if not block:
                break
            count += len(block)
            if count > MAX_SNAPSHOT_BYTES:
                reject("snapshot exceeds offline 8 MiB diagnostic limit")
            digest.update(block)
    if count == 0:
        reject("empty checkpoint snapshot")
    return digest.hexdigest()


def compare_external_pin(
    pin: object, claim: object, snapshot_path: Path,
) -> dict:
    """Assume the pin was independently provisioned; do not trust the claim."""
    trusted = exact_object(pin, ANCHOR_KEYS, "external pin")
    candidate = exact_object(claim, CLAIM_KEYS, "untrusted claim")
    if trusted["schema"] != ANCHOR_SCHEMA or trusted["status"] != "TEST_ONLY_EXTERNAL_PIN":
        reject("external pin schema/status is not an offline trial")
    if candidate["schema"] != CLAIM_SCHEMA or candidate["status"] != "UNAPPROVED_DRAFT":
        reject("checkpoint claim schema/status is not an unapproved draft")

    chain = trusted["chain_id"]
    if type(chain) is not str or NETWORK.fullmatch(chain) is None:
        reject("external pin is not scoped to draft public-testnet-v3")
    for name in ("genesis_hash", "checkpoint_hash", "snapshot_sha256"):
        digest_field(trusted[name], f"pin.{name}")
        digest_field(candidate[name], f"claim.{name}")
    pinned_score = score_field(trusted["checkpoint_score"], "pin.checkpoint_score")
    minimum = score_field(trusted["minimum_accepted_score"], "pin.minimum_accepted_score")
    claimed_score = score_field(candidate["checkpoint_score"], "claim.checkpoint_score")
    if minimum > pinned_score or claimed_score < minimum:
        reject("checkpoint violates independently pinned minimum score / anti-rollback bound")
    if candidate["chain_id"] != chain:
        reject("cross-network checkpoint claim")
    if candidate["genesis_hash"] != trusted["genesis_hash"]:
        reject("genesis pin mismatch")
    if candidate["checkpoint_hash"] != trusted["checkpoint_hash"]:
        reject("checkpoint hash pin mismatch")
    if claimed_score != pinned_score:
        reject("checkpoint score differs from externally pinned candidate")
    if candidate["snapshot_sha256"] != trusted["snapshot_sha256"]:
        reject("checkpoint digest differs from externally pinned snapshot")
    if candidate["contracts_enabled"] is not False or candidate["monetary_runtime_enabled"] is not False:
        reject("offline checkpoint claim cannot enable contracts or monetary runtime")
    actual = sha256_bounded_snapshot(snapshot_path)
    if actual != trusted["snapshot_sha256"]:
        reject("checkpoint bytes do not match externally pinned snapshot SHA256")
    return {
        "external_pin_match": "PASS_ONLY",
        "snapshot_sha256": actual,
        "checkpoint_score": pinned_score,
        "independent_pin_provenance_authenticated": False,
        "native_dag_consensus_verified": False,
        "checkpoint_launch_authorized": False,
        "monetary_runtime_enabled": False,
        "contracts_enabled": False,
        "note": "Local anti-rollback equality ONLY; no independent signer/finality or network GO",
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("external_pin_json", type=Path)
    parser.add_argument("untrusted_claim_json", type=Path)
    parser.add_argument("snapshot_bytes", type=Path)
    args = parser.parse_args()
    try:
        verdict = compare_external_pin(
            read_bounded_json(args.external_pin_json),
            read_bounded_json(args.untrusted_claim_json),
            args.snapshot_bytes,
        )
    except (AnchorError, OSError) as error:
        print(f"FAIL (NO-GO): {error}", file=sys.stderr)
        return 1
    print(json.dumps(verdict, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
