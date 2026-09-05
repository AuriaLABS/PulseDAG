#!/usr/bin/env python3
"""Fail-closed validation of the v3 coordinated-launch manifest."""

from __future__ import annotations

import argparse
import json
import re
import sys
import copy
import tempfile
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_MANIFEST = ROOT / "docs" / "V3_0_0_LAUNCH_MANIFEST.md"
REQUIRED_NETWORK_FIELDS = {"chain_id", "genesis_hash", "signing_domain", "bootnode_identity_digest"}
REQUIRED_ASSERTIONS = {
    "network_identity_separation",
    "genesis_reproducibility",
    "cross_network_mismatch_fails_closed",
    "artifacts_and_evidence_exact_candidate",
}


class ManifestError(ValueError):
    pass


def fail(message: str) -> None:
    raise ManifestError(message)


def load_manifest(path: Path) -> dict[str, Any]:
    text = path.read_text(encoding="utf-8")
    blocks = re.findall(r"```json\s*(.*?)\s*```", text, re.DOTALL)
    if len(blocks) != 1:
        fail("manifest must contain exactly one fenced JSON object")
    try:
        value = json.loads(blocks[0])
    except json.JSONDecodeError as exc:
        fail(f"invalid manifest JSON: {exc}")
    if not isinstance(value, dict):
        fail("manifest JSON must be an object")
    return value


def validate_manifest(manifest: dict[str, Any]) -> bool:
    required = {
        "format", "manifest_version", "launch_state", "decision",
        "exact_candidate", "mainnet", "parallel_testnet", "assertions",
    }
    missing = required - manifest.keys()
    if missing:
        fail(f"missing required keys: {', '.join(sorted(missing))}")
    if manifest["format"] != "pulsedag-v3-launch-manifest" or manifest["manifest_version"] != 1:
        fail("unsupported manifest format or version")
    if manifest["launch_state"] not in {"PRE_FREEZE", "FROZEN"}:
        fail("launch_state must be PRE_FREEZE or FROZEN")
    if manifest["decision"] not in {
        "GO_V3_DUAL_LAUNCH", "DELAY_V3_DUAL_LAUNCH", "NO_GO_V3_DUAL_LAUNCH",
    }:
        fail("decision is not a recognized v3 launch decision")
    candidate = manifest["exact_candidate"]
    if not isinstance(candidate, dict) or candidate.get("release") != "v3.0.0":
        fail("exact_candidate.release must be v3.0.0")
    networks = []
    for name in ("mainnet", "parallel_testnet"):
        network = manifest[name]
        if not isinstance(network, dict) or REQUIRED_NETWORK_FIELDS - network.keys():
            fail(f"{name} is missing required identity fields")
        networks.append(network)
    assertions = manifest["assertions"]
    if not isinstance(assertions, dict) or REQUIRED_ASSERTIONS - assertions.keys():
        fail("assertions are incomplete")
    for key, value in assertions.items():
        if value not in {"PASS", "PENDING", "FAIL"}:
            fail(f"assertions.{key} must be PASS, PENDING, or FAIL")
    launch_values = [candidate, *networks]
    has_tbd = any(value == "TBD" for obj in launch_values for value in obj.values())
    identities_distinct = all(
        networks[0][field] != networks[1][field] for field in REQUIRED_NETWORK_FIELDS
    )

    if manifest["decision"] == "GO_V3_DUAL_LAUNCH" and manifest["launch_state"] != "FROZEN":
        fail("GO_V3_DUAL_LAUNCH requires launch_state=FROZEN")
    if manifest["launch_state"] == "FROZEN" and has_tbd:
        fail("frozen launch identities must not contain TBD values")
    if manifest["decision"] == "GO_V3_DUAL_LAUNCH" and any(
        assertions[key] != "PASS" for key in REQUIRED_ASSERTIONS
    ):
        fail("GO_V3_DUAL_LAUNCH requires all required assertions to be PASS")
    if manifest["launch_state"] == "FROZEN" and not identities_distinct:
        fail("frozen mainnet and parallel_testnet identities must be distinct")

    return (
        manifest["launch_state"] == "FROZEN"
        and manifest["decision"] == "GO_V3_DUAL_LAUNCH"
        and not has_tbd
        and identities_distinct
        and all(assertions[key] == "PASS" for key in REQUIRED_ASSERTIONS)
    )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("manifest", nargs="?", type=Path, default=DEFAULT_MANIFEST)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    try:
        if args.self_test:
            sample = {
                "format": "pulsedag-v3-launch-manifest",
                "manifest_version": 1,
                "launch_state": "FROZEN",
                "decision": "GO_V3_DUAL_LAUNCH",
                "exact_candidate": {"release": "v3.0.0", "source_sha": "a", "tree_sha": "b"},
                "mainnet": {field: f"main-{field}" for field in REQUIRED_NETWORK_FIELDS},
                "parallel_testnet": {field: f"test-{field}" for field in REQUIRED_NETWORK_FIELDS},
                "assertions": {key: "PASS" for key in REQUIRED_ASSERTIONS},
            }
            if not validate_manifest(sample):
                fail("frozen passing sample was not launch-ready")
            invalid = copy.deepcopy(sample)
            invalid["parallel_testnet"]["chain_id"] = invalid["mainnet"]["chain_id"]
            try:
                validate_manifest(invalid)
            except ManifestError:
                pass
            else:
                fail("network identity collision was accepted")
            with tempfile.TemporaryDirectory() as directory:
                sample_path = Path(directory) / "manifest.md"
                sample_path.write_text(f"```json\n{json.dumps(sample)}\n```\n", encoding="utf-8")
                load_manifest(sample_path)
        ready = validate_manifest(load_manifest(args.manifest))
    except (ManifestError, OSError, json.JSONDecodeError) as exc:
        print(f"v3 network freeze validation failed: {exc}", file=sys.stderr)
        return 1
    print(f"launch_ready={'true' if ready else 'false'}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
