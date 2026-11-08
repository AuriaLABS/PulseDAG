"""Negative-only safety regressions for pre-GO public-testnet identity drafts.

All apparent hashes and peer IDs here are SYNTHETIC test values, not real
network identities, frozen artifacts, genesis or permission to launch.
"""
from __future__ import annotations

import copy
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from v3_public_testnet_identity_shape_gate import (  # noqa: E402
    CADENCE_SHA256,
    POLICY_SHA256,
    CandidateError,
    check_candidate,
    parse_manifest,
)


def candidate() -> dict:
    return {
        "schema": "pulsedag.v3.public-testnet-identity-draft.v1",
        "status": "UNAPPROVED_DRAFT",
        "network_profile": "public-testnet-v3",
        "chain_id": "pulsedag-public-testnet-v3-draft",
        "genesis_hash": "ab" * 32,
        "genesis_timestamp_ns": 1_800_000_000_000_000_000,
        "signing_domain": "pulsedag:v3:public-testnet:draft-not-frozen",
        "consensus_candidate": "native-v3-unapproved",
        "source_git_sha": "cd" * 20,
        "node_artifact_sha256": "12" * 32,
        "miner_artifact_sha256": "34" * 32,
        "config_sha256": "56" * 32,
        "monetary_policy_sha256": POLICY_SHA256,
        "cadence_sha256": CADENCE_SHA256,
        "bootnode_peer_ids": ["12D3KooWSyntheticBootnodeA", "12D3KooWSyntheticBootnodeB"],
        "monetary_runtime_enabled": False,
        "contracts_enabled": False,
        "admin_enabled": False,
        "public_testnet_ready": False,
        "thirty_day_clock_started": False,
        "allow_legacy_fallback": False,
        "future_mainnet_identity": None,
    }


class DraftIdentityShapeGateTests(unittest.TestCase):
    def reject(self, name: str, value: object) -> None:
        m = candidate()
        m[name] = value
        with self.subTest(name=name, value=value), self.assertRaises(CandidateError):
            check_candidate(m)

    def test_synthetic_shape_success_never_grants_go(self) -> None:
        result = check_candidate(candidate())
        self.assertEqual(result["schema_shape"], "PASS_ONLY")
        for flag in (
            "public_testnet_launch_authorized", "mainnet_launch_authorized",
            "monetary_runtime_enabled", "contracts_enabled",
            "source_and_artifacts_authenticated", "genesis_and_consensus_verified",
            "mainnet_cross_identity_frozen",
        ):
            self.assertIs(result[flag], False)

    def test_frozen_or_go_status_denied(self) -> None:
        for status in ("APPROVED", "GO_PUBLIC_TESTNET", "FROZEN", "READY"):
            self.reject("status", status)

    def test_reject_legacy_and_claimed_live_consensus_modes(self) -> None:
        for mode in ("legacy", "ghostdag_v1", "monetary_v3", "native-v3-active"):
            self.reject("consensus_candidate", mode)

    def test_reject_mainnet_and_private_namespaces(self) -> None:
        for chain in ("pulsedag-mainnet-v3", "pulsedag-private-v3", "pulsedag-testnet"):
            self.reject("chain_id", chain)
        self.reject("signing_domain", "pulsedag:v3:mainnet:draft")

    def test_reject_malformed_hashes_and_placeholders(self) -> None:
        for value in ("0" * 64, "AB" * 32, "REPLACE_ME", "ab" * 31, "ff" * 32 + "0"):
            self.reject("genesis_hash", value)
        self.reject("source_git_sha", "0" * 40)
        self.reject("node_artifact_sha256", "TBD")
        self.reject("config_sha256", "00" * 32)

    def test_reject_monetary_fingerprint_drift(self) -> None:
        self.reject("monetary_policy_sha256", "aa" * 32)
        self.reject("cadence_sha256", "bb" * 32)

    def test_reject_runtime_activation_contracts_and_public_flags(self) -> None:
        for field in (
            "monetary_runtime_enabled", "contracts_enabled", "admin_enabled",
            "public_testnet_ready", "thirty_day_clock_started", "allow_legacy_fallback",
        ):
            self.reject(field, True)
            self.reject(field, "false")
            self.reject(field, 0)

    def test_reject_numeric_boolean_genesis_and_overflow(self) -> None:
        for value in (True, 0, -1, (1 << 64), "1800000000000000000"):
            self.reject("genesis_timestamp_ns", value)

    def test_require_unique_proposed_bootnode_identities(self) -> None:
        self.reject("bootnode_peer_ids", [])
        self.reject("bootnode_peer_ids", ["12D3KooWSyntheticBootnodeA"] * 2)
        self.reject("bootnode_peer_ids", "12D3KooWSyntheticBootnodeA")
        self.reject("bootnode_peer_ids", ["short", "also-short"])

    def test_reject_non_peer_id_bootnode_ascii_and_accept_lexical_cidv1(self) -> None:
        for invalid in (
            "12D3KooWInvalid0Bootnode",  # base58btc disallows zero
            "12D3KooWInvalidOBootnode",  # capital O
            "12D3KooWInvalidIBootnode",  # capital I
            "12D3KooWInvalidlBootnode",  # lowercase l
            "12D3KooWInvalid!Bootnode",  # punctuation
            "baf" + "a" * 20 + "0",  # invalid CIDv1 base32 character
        ):
            with self.subTest(invalid=invalid):
                self.reject("bootnode_peer_ids", [candidate()["bootnode_peer_ids"][0], invalid])

        m = candidate()
        m["bootnode_peer_ids"] = ["baf" + "a" * 32, "12D3KooWSyntheticBootnodeB"]
        verdict = check_candidate(m)
        self.assertEqual(verdict["schema_shape"], "PASS_ONLY")
        self.assertFalse(verdict["public_testnet_launch_authorized"])
        self.assertFalse(verdict["genesis_and_consensus_verified"])

    def test_reject_same_daemon_and_miner_digest(self) -> None:
        self.reject("miner_artifact_sha256", candidate()["node_artifact_sha256"])

    def test_reject_missing_unexpected_and_secret_named_fields(self) -> None:
        m = candidate()
        del m["contracts_enabled"]
        with self.assertRaises(CandidateError):
            check_candidate(m)
        m = candidate()
        m["wallet_seed"] = "secret"
        with self.assertRaises(CandidateError):
            check_candidate(m)

    def test_mainnet_identity_not_required_before_testnet_freeze(self) -> None:
        m = candidate()
        m["future_mainnet_identity"] = {
            "chain_id": "pulsedag-mainnet-v3",
            "genesis_hash": "cd" * 32,
            "signing_domain": "pulsedag:v3:mainnet:later",
        }
        result = check_candidate(m)
        self.assertTrue(result["future_mainnet_identity_shape_checked"])
        self.assertFalse(result["mainnet_cross_identity_frozen"])
        self.assertFalse(result["mainnet_launch_authorized"])

    def test_reject_future_mainnet_disguised_as_other_network(self) -> None:
        baseline = {
            "chain_id": "pulsedag-mainnet-v3",
            "genesis_hash": "cd" * 32,
            "signing_domain": "pulsedag:v3:mainnet:later",
        }
        for incorrect in (
            {"chain_id": "pulsedag-public-testnet-v3-other"},
            {"chain_id": "pulsedag-mainnet-v2"},
            {"chain_id": "pulsedag-mainnet-v3-DRAFT"},
            {"chain_id": "pulsedag-mainnet-v3-shadow_2"},
            {"signing_domain": "pulsedag:v3:public-testnet:other"},
            {"signing_domain": "pulsedag:v3:mainnet:x"},
            {"signing_domain": "pulsedag:v3:mainnet:BAD"},
            {"signing_domain": "pulsedag:legacy:mainnet:other"},
        ):
            draft = candidate()
            draft["future_mainnet_identity"] = {**baseline, **incorrect}
            with self.subTest(incorrect=incorrect), self.assertRaises(CandidateError):
                check_candidate(draft)

    def test_absent_future_mainnet_is_not_mistaken_for_authority(self) -> None:
        verdict = check_candidate(candidate())
        self.assertFalse(verdict["future_mainnet_identity_shape_checked"])
        self.assertFalse(verdict["mainnet_cross_identity_frozen"])
        self.assertFalse(verdict["mainnet_launch_authorized"])

    def test_any_testnet_mainnet_overlap_rejected(self) -> None:
        m = candidate()
        distinct = {
            "chain_id": "pulsedag-mainnet-v3",
            "genesis_hash": "cd" * 32,
            "signing_domain": "pulsedag:v3:mainnet:later",
        }
        for field in distinct:
            v = copy.deepcopy(m)
            v["future_mainnet_identity"] = {**distinct, field: m[field]}
            with self.subTest(field=field), self.assertRaises(CandidateError):
                check_candidate(v)

    def test_reject_duplicate_json_fields_and_oversized_manifest(self) -> None:
        with self.assertRaises(CandidateError):
            parse_manifest(b'{"schema": "one", "schema": "two"}')
        with self.assertRaises(CandidateError):
            parse_manifest(b" " * (64 * 1024 + 1))
        with self.assertRaises(CandidateError):
            parse_manifest(b"\xff")
        self.assertEqual(check_candidate(parse_manifest(json.dumps(candidate()).encode()))["schema_shape"], "PASS_ONLY")

    def test_cli_success_still_prints_explicit_no_go(self) -> None:
        script = Path(__file__).resolve().parents[1] / "v3_public_testnet_identity_shape_gate.py"
        with tempfile.TemporaryDirectory() as work:
            path = Path(work) / "synthetic-not-launch-authority.json"
            path.write_text(json.dumps(candidate()), encoding="utf-8")
            result = subprocess.run([sys.executable, str(script), str(path)], check=False,
                                    capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            outcome = json.loads(result.stdout)
            self.assertFalse(outcome["public_testnet_launch_authorized"])
            self.assertFalse(outcome["monetary_runtime_enabled"])
            broken = candidate()
            broken["contracts_enabled"] = True
            path.write_text(json.dumps(broken), encoding="utf-8")
            failure = subprocess.run([sys.executable, str(script), str(path)], check=False,
                                     capture_output=True, text=True)
            self.assertNotEqual(failure.returncode, 0)
            self.assertIn("NO-GO", failure.stderr)


if __name__ == "__main__":
    unittest.main()
