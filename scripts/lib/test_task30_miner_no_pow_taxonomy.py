"""Fail-closed adapter contract for private 5-node Task30 miner retries.

Never turn a backend-verified invalid solution into a retryable nonce
exhaustion; the latter requires an explicit typed marker from the miner.
"""
from __future__ import annotations

import unittest
from pathlib import Path

from patch_task30_v24_runtime_compat import patch_prune

ROOT = Path(__file__).resolve().parents[2]


class Task30MinerTaxonomyTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        source = (ROOT / "scripts/lib/v2_3_0_prune_restart_rejoin_harness.sh").read_text(
            encoding="utf-8"
        )
        cls.patched = patch_prune(source)

    def test_explicit_bounded_nonce_exhaustion_is_retryable(self) -> None:
        self.assertIn(
            "if grep -Eq 'nonce_search_exhausted:.*reason=no_valid_pow_within_budget' "
            '"$attempt_log"; then',
            self.patched,
        )
        self.assertIn(
            "no PoW solution within 1000000 tries for $label", self.patched
        )

    def test_backend_verification_failure_is_not_retryable(self) -> None:
        self.assertNotIn(
            "backend_verification_failed:.*hash_above_target", self.patched
        )
        self.assertIn(
            "external miner exited successfully without an accepted submit "
            "or explicit retryable outcome",
            self.patched,
        )

    def test_submit_requires_accepted_true_and_unexpected_error_fails(self) -> None:
        self.assertIn(
            "if grep -q 'submit_result: accepted=true' "
            '"$attempt_log"; then',
            self.patched,
        )
        self.assertIn('tail -80 "$attempt_log" >&2 || true', self.patched)
        self.assertIn("external miner failed to produce an accepted block", self.patched)


if __name__ == "__main__":
    unittest.main()
