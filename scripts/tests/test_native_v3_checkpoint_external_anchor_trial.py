"""Offline negative regressions for a TEST-ONLY externally pinned checkpoint digest.

The external pin is an independent fixture input, not a verified public signer.
"""
from __future__ import annotations

import copy
import hashlib
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from native_v3_checkpoint_external_anchor_trial import (  # noqa: E402
    ANCHOR_SCHEMA, CLAIM_SCHEMA, AnchorError, compare_external_pin,
    read_bounded_json, sha256_bounded_snapshot,
)


class IndependentExternalAnchorTrialTests(unittest.TestCase):
    def setUp(self) -> None:
        self.workspace = tempfile.TemporaryDirectory()
        self.addCleanup(self.workspace.cleanup)
        self.dir = Path(self.workspace.name)
        self.snapshot = self.dir / "synthetic-snapshot.bin"
        self.snapshot.write_bytes(b"synthetic preverified native-v3 research state" * 3)
        digest = hashlib.sha256(self.snapshot.read_bytes()).hexdigest()
        self.pin = {
            "schema": ANCHOR_SCHEMA,
            "status": "TEST_ONLY_EXTERNAL_PIN",
            "chain_id": "pulsedag-public-testnet-v3-draft",
            "genesis_hash": "ab" * 32,
            "checkpoint_hash": "cd" * 32,
            "checkpoint_score": 120,
            "minimum_accepted_score": 100,
            "snapshot_sha256": digest,
        }
        self.claim = {
            "schema": CLAIM_SCHEMA,
            "status": "UNAPPROVED_DRAFT",
            "chain_id": self.pin["chain_id"],
            "genesis_hash": self.pin["genesis_hash"],
            "checkpoint_hash": self.pin["checkpoint_hash"],
            "checkpoint_score": self.pin["checkpoint_score"],
            "snapshot_sha256": digest,
            "contracts_enabled": False,
            "monetary_runtime_enabled": False,
        }

    def verified(self) -> dict:
        return compare_external_pin(self.pin, self.claim, self.snapshot)

    def reject_pin(self, field: str, value: object) -> None:
        pin = copy.deepcopy(self.pin)
        pin[field] = value
        with self.subTest(field=field, value=value), self.assertRaises(AnchorError):
            compare_external_pin(pin, self.claim, self.snapshot)

    def reject_claim(self, field: str, value: object) -> None:
        claim = copy.deepcopy(self.claim)
        claim[field] = value
        with self.subTest(field=field, value=value), self.assertRaises(AnchorError):
            compare_external_pin(self.pin, claim, self.snapshot)

    def test_synthetic_pin_is_only_a_local_comparison_not_authority(self) -> None:
        result = self.verified()
        self.assertEqual(result["external_pin_match"], "PASS_ONLY")
        self.assertEqual(result["checkpoint_score"], 120)
        self.assertEqual(result["snapshot_sha256"], self.pin["snapshot_sha256"])
        for key in (
            "independent_pin_provenance_authenticated", "native_dag_consensus_verified",
            "checkpoint_launch_authorized", "monetary_runtime_enabled",
            "contracts_enabled",
        ):
            self.assertIs(result[key], False)

    def test_reject_older_but_well_formed_checkpoint_and_rollback(self) -> None:
        self.reject_claim("checkpoint_score", 99)
        self.reject_claim("checkpoint_score", 119)
        self.reject_claim("checkpoint_score", 121)
        self.reject_pin("minimum_accepted_score", 121)
        self.reject_pin("checkpoint_score", 99)

    def test_reject_cross_network_genesis_and_different_anchor(self) -> None:
        self.reject_claim("chain_id", "pulsedag-mainnet-v3")
        self.reject_pin("chain_id", "pulsedag-mainnet-v3")
        self.reject_claim("genesis_hash", "12" * 32)
        self.reject_claim("checkpoint_hash", "34" * 32)
        self.reject_pin("genesis_hash", "00" * 32)
        self.reject_pin("checkpoint_hash", "00" * 32)

    def test_untrusted_claim_digest_and_real_snapshot_mutation_fail(self) -> None:
        self.reject_claim("snapshot_sha256", "ef" * 32)
        self.reject_pin("snapshot_sha256", "ef" * 32)
        self.snapshot.write_bytes(b"tampered snapshot but unchanged claim and pin")
        with self.assertRaises(AnchorError):
            self.verified()
        self.snapshot.write_bytes(b"")
        with self.assertRaises(AnchorError):
            self.verified()

    def test_no_activation_even_with_synthetic_matching_pin(self) -> None:
        for field in ("contracts_enabled", "monetary_runtime_enabled"):
            for value in (True, 1, 0, "false", None):
                self.reject_claim(field, value)
        for status in ("GO_PUBLIC_TESTNET", "READY", "APPROVED", "FROZEN"):
            self.reject_claim("status", status)
            self.reject_pin("status", status)

    def test_reject_bools_overflow_and_malformed_digests(self) -> None:
        for bad in (True, False, 0, -1, 1 << 64, "120"):
            self.reject_claim("checkpoint_score", bad)
            self.reject_pin("minimum_accepted_score", bad)
        for bad in ("00" * 32, "AB" * 32, "ab" * 31, "placeholder"):
            self.reject_claim("snapshot_sha256", bad)
        self.reject_pin("schema", "wrong-schema")
        self.reject_claim("schema", "wrong-schema")

    def test_reject_missing_unknown_fields_and_duplicate_json(self) -> None:
        pin = copy.deepcopy(self.pin)
        del pin["checkpoint_hash"]
        with self.assertRaises(AnchorError):
            compare_external_pin(pin, self.claim, self.snapshot)
        claim = copy.deepcopy(self.claim)
        claim["wallet_secret"] = "do-not-accept"
        with self.assertRaises(AnchorError):
            compare_external_pin(self.pin, claim, self.snapshot)
        path = self.dir / "pin.json"
        for raw in (
            b'{"schema":"x","schema":"y"}',
            b" " * (64 * 1024 + 1),
            b"{\"schema\":NaN}",
            b"\xff",
        ):
            path.write_bytes(raw)
            with self.subTest(raw=raw[:40]), self.assertRaises(AnchorError):
                read_bounded_json(path)

    def test_snapshot_is_bounded_and_untrusted(self) -> None:
        self.snapshot.write_bytes(b"1" * (8 * 1024 * 1024 + 1))
        with self.assertRaises(AnchorError):
            sha256_bounded_snapshot(self.snapshot)

    def test_cli_success_and_fail_paths_are_explicit_no_go(self) -> None:
        pin_file = self.dir / "external-pin.json"
        claim_file = self.dir / "untrusted-claim.json"
        pin_file.write_text(json.dumps(self.pin), encoding="utf-8")
        claim_file.write_text(json.dumps(self.claim), encoding="utf-8")
        script = Path(__file__).resolve().parents[1] / "native_v3_checkpoint_external_anchor_trial.py"
        command = [sys.executable, str(script), str(pin_file), str(claim_file), str(self.snapshot)]
        passed = subprocess.run(command, text=True, capture_output=True, check=False)
        self.assertEqual(passed.returncode, 0, passed.stderr)
        result = json.loads(passed.stdout)
        self.assertFalse(result["checkpoint_launch_authorized"])
        self.assertFalse(result["independent_pin_provenance_authenticated"])
        self.claim["checkpoint_score"] = 90
        claim_file.write_text(json.dumps(self.claim), encoding="utf-8")
        failed = subprocess.run(command, text=True, capture_output=True, check=False)
        self.assertNotEqual(failed.returncode, 0)
        self.assertIn("NO-GO", failed.stderr)


if __name__ == "__main__":
    unittest.main()
