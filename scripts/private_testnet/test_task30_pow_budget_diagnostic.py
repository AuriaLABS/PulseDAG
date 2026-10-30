"""Negative and golden-vector tests for Task30's read-only PoW budget estimator."""
from __future__ import annotations

import math
import unittest

from task30_pow_budget_diagnostic import MAX_TARGET, calculate

OBSERVED_FAILING_TARGET = "00000172ef000000000000000000000000000000000000000000000000000000"


class Task30PowBudgetDiagnosticTests(unittest.TestCase):
    def test_observed_64_block_ci_budget_is_infeasible_in_uniform_model(self) -> None:
        d = calculate(OBSERVED_FAILING_TARGET, 33_000, 1_000_000, 60, 64, 240)
        self.assertEqual(d["kind"], "TASK30_POW_BUDGET_DIAGNOSTIC_ONLY")
        self.assertEqual(d["budgeted_hashes_per_block"], 60_000_000)
        self.assertTrue(d["non_consensus"])
        self.assertTrue(d["requires_real_pow_for_every_block"])
        self.assertAlmostEqual(d["expected_hashes_per_block_uniform_model"], 11_578_804, delta=5)
        self.assertAlmostEqual(d["probability_one_block_exhausts_budget_uniform_model"], 0.005617, delta=0.00001)
        self.assertAlmostEqual(d["probability_any_block_exhausts_budget_uniform_model"], 0.30269, delta=0.001)
        self.assertGreater(d["expected_mining_seconds_lower_bound_model"], 14_400)
        self.assertTrue(d["expected_mining_time_exceeds_workflow_timeout"])

    def test_maximum_target_one_hash_always_satisfies_model(self) -> None:
        d = calculate(f"{MAX_TARGET:064x}", 1.0, 1, 1, 1, 1.0)
        self.assertEqual(d["probability_per_hash_uniform_model"], 1.0)
        self.assertEqual(d["probability_one_block_exhausts_budget_uniform_model"], 0.0)
        self.assertEqual(d["probability_any_block_exhausts_budget_uniform_model"], 0.0)
        self.assertFalse(d["expected_mining_time_exceeds_workflow_timeout"])

    def test_easy_target_keeps_sufficient_work_and_no_fake_success(self) -> None:
        d = calculate("7f" + "ff" * 31, 1000, 1000, 10, 64, 60)
        self.assertGreater(d["probability_per_hash_uniform_model"], 0.49)
        self.assertEqual(d["blocks"], 64)
        self.assertFalse(d["expected_mining_time_exceeds_workflow_timeout"])
        self.assertTrue(d["non_consensus"])

    def test_invalid_inputs_fail_closed(self) -> None:
        for target in ["", "0x" + "00" * 32, "zz" * 32, "1234", "00" * 33]:
            with self.subTest(target=target), self.assertRaises(ValueError):
                calculate(target, 1000, 1, 1, 1, 60)
        for rate in [0.0, -1.0, math.nan, math.inf]:
            with self.subTest(rate=rate), self.assertRaises(ValueError):
                calculate(OBSERVED_FAILING_TARGET, rate, 1, 1, 1, 60)
        for count in [0, -1]:
            with self.subTest(count=count), self.assertRaises(ValueError):
                calculate(OBSERVED_FAILING_TARGET, 100, count, 1, 1, 60)
                calculate(OBSERVED_FAILING_TARGET, 100, 1, count, 1, 60)
                calculate(OBSERVED_FAILING_TARGET, 100, 1, 1, count, 60)
        with self.assertRaises(ValueError):
            calculate(OBSERVED_FAILING_TARGET, 100, 1, 1, 1, float("nan"))

    def test_extra_attempts_reduce_exhaustion_without_changing_target(self) -> None:
        a = calculate(OBSERVED_FAILING_TARGET, 33_000, 1_000_000, 20, 64, 240)
        b = calculate(OBSERVED_FAILING_TARGET, 33_000, 1_000_000, 60, 64, 240)
        self.assertEqual(a["target_hex"], b["target_hex"])
        self.assertGreater(
            a["probability_one_block_exhausts_budget_uniform_model"],
            b["probability_one_block_exhausts_budget_uniform_model"],
        )


if __name__ == "__main__":
    unittest.main()
