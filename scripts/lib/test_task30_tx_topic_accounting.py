#!/usr/bin/env python3
"""Fail-closed Task30 signed relay counters under periodic GetTips (offline test).

Task30 is an internal diagnostic, NOT a previous deployed PulseDAG network.
The first public testnet/mainnet will use only native-v3 identities.
"""

from __future__ import annotations

import json
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

from patch_task30_v24_runtime_compat import patch_relay

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / "scripts/v2_3_0_mempool_tx_relay_evidence.sh"


class Task30TxTopicEvidenceTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.patched = patch_relay(SOURCE.read_text(encoding="utf-8"))
        signature = "task30_tx_topic_publishes() {"
        start = cls.patched.index(signature)
        end = cls.patched.index("\n}\n", start) + 3
        cls.function = cls.patched[start:end]
        if not shutil.which("jq"):
            raise AssertionError("jq required: missing per-topic evidence must fail closed")

    def read_count(self, per_topic: object, global_count: int = 0) -> subprocess.CompletedProcess[str]:
        payload = {"data": {"publish_attempts": global_count}}
        if per_topic is not None:
            payload["data"]["per_topic_publishes"] = per_topic
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp) / "p2p-status.json"
            path.write_text(json.dumps(payload), encoding="utf-8")
            return subprocess.run(
                ["bash", "-euo", "pipefail", "-c",
                 self.function + '\ntask30_tx_topic_publishes "$1"', "_", str(path)],
                check=False,
                capture_output=True,
                text=True,
            )

    def test_periodic_sync_traffic_does_not_count_as_tx_rebroadcast(self) -> None:
        before = self.read_count(
            {"first-native-v3-txs": 1, "first-native-v3-sync": 17}, global_count=21
        )
        after = self.read_count(
            {"first-native-v3-txs": 1, "first-native-v3-sync": 18}, global_count=22
        )
        self.assertEqual(before.returncode, 0, before.stderr)
        self.assertEqual(after.returncode, 0, after.stderr)
        self.assertEqual(before.stdout.strip(), "1")
        self.assertEqual(after.stdout.strip(), "1")

    def test_any_extra_transaction_publication_remains_a_hard_failure(self) -> None:
        before = self.read_count({"net-txs": 1, "net-sync": 4})
        after = self.read_count({"net-txs": 2, "net-sync": 4})
        self.assertEqual(before.returncode, 0, before.stderr)
        self.assertEqual(after.returncode, 0, after.stderr)
        self.assertNotEqual(before.stdout.strip(), after.stdout.strip())
        self.assertIn('[[ "$after_publish" == "$before_publish" ]] || conflict_publish_unchanged=false', self.patched)
        self.assertIn('MEMPOOL_V3_REPLACEMENT_NOT_AUTHORIZED', self.patched)
        self.assertIn('"replacement semantics are not authorized"', self.patched)
        self.assertIn('[[ "$tx_count" == 1 && "$original_count" == 1 ]]', self.patched)

    def test_duplicate_conflict_and_stale_use_scoped_counter_not_global_counter(self) -> None:
        # Includes post-confirmation stale resubmission; its replay must not
        # be silently allowed through by periodic sync-topic publications.
        self.assertEqual(
            self.patched.count('before_publish="$(task30_tx_topic_publishes '), 3
        )
        self.assertEqual(
            self.patched.count('after_publish="$(task30_tx_topic_publishes '), 3
        )
        self.assertNotIn(
            'before_publish="$(jq -r \'.data.publish_attempts // 0\'',
            self.patched,
        )

    def test_missing_ambiguous_or_malformed_transaction_topic_fails_closed(self) -> None:
        bad_counters = [
            None,
            {},
            {"net-sync": 1},
            {"a-txs": 1, "b-txs": 1},
            {"net-txs": -1},
            {"net-txs": 1.2},
            {"net-txs": "1"},
            [],
        ]
        for bad in bad_counters:
            with self.subTest(counter=bad):
                observed = self.read_count(bad)
                self.assertNotEqual(observed.returncode, 0)


if __name__ == "__main__":
    unittest.main()
