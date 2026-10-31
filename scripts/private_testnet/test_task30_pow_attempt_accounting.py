"""Fail-closed synthetic contract tests for cross-process Task30 PoW accounting."""
from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path

from task30_pow_attempt_accounting import EvidenceError, inspect_attempt, main, summarize

TARGET = "00000172ef" + "0" * 54


def exhausted(*, attempts: int = 100, budget: int = 100, backend: str = "cpu") -> str:
    attested = backend == "cpu" and attempts == budget
    evidence = {
        "schema_version": 1,
        "event": "nonce_search_exhausted",
        "reason": "no_valid_pow_within_budget",
        "backend": backend,
        "template_id": "template-119",
        "template_hash": "aa" * 32,
        "block_height": 119,
        "compact_target_bits": 0x1E0172EF,
        "canonical_target_hex": TARGET,
        "protocol_identity_fingerprint": "private-chain-id",
        "attempted_hashes": attempts,
        "configured_hash_budget": budget,
        "checked_nonce_not_a_solution": budget - 1,
        "nonce_domain": "cpu_protocol_strided_full_budget" if attested else "backend_specific_or_partial_unattested",
        "nonce_start_inclusive": 0 if attested else None,
        "nonce_end_exclusive": budget if attested else None,
        "backend_claimed_accepted": False,
        "canonical_accepted": False,
    }
    return "nonce_search_evidence_json: " + json.dumps(evidence, sort_keys=True) + "\n"


def canonical_solution(*, attempts: int = 12, submitted: bool = True) -> str:
    log = (
        "mining: algorithm=kheavyhash pow_engine=canonical_core "
        f"template_id=template-119 height=119 target_hex={TARGET} "
        f"nonce=42 pow_hash={'00' * 32} attempts={attempts} "
        "hashes_per_sec=123456.0 accepted=true elapsed_ms=1\n"
    )
    if submitted:
        log += "submit_result: accepted=true rejected=false reason_code=accepted block_hash=test height=119\n"
    return log


class Task30CrossProcessAccountingTests(unittest.TestCase):
    def setUp(self) -> None:
        self.directory = tempfile.TemporaryDirectory()
        self.root = Path(self.directory.name)

    def tearDown(self) -> None:
        self.directory.cleanup()

    def log(self, name: str, contents: str) -> Path:
        path = self.root / name
        path.write_text(contents, encoding="utf-8")
        return path

    def test_process_retry_lower_bound_and_real_accepted_response(self) -> None:
        self.log("offline-1-attempt-1.log", exhausted())
        self.log("offline-1-attempt-2.log", canonical_solution())
        rows, data = summarize(self.root)
        self.assertEqual(len(rows), 2)
        self.assertEqual(rows[0]["outcome"], "nonce_search_exhausted")
        self.assertEqual(rows[0]["nonce_domain"], "cpu_protocol_strided_full_budget")
        self.assertEqual(rows[1]["outcome"], "node_submit_accepted")
        self.assertEqual(data["hashes_observed_lower_bound"], 112)
        self.assertEqual(data["offline_node_accepted_block_count"], 1)
        self.assertEqual(data["unattested_attempts"], 0)
        self.assertTrue(data["all_attempts_hashes_accounted"])
        self.assertTrue(data["non_consensus"])
        self.assertTrue(data["cannot_authorize_mining_or_mainnet"])

    def test_unknown_attempt_is_never_made_into_zero_work_claim(self) -> None:
        self.log("offline-1-attempt-1.log", "429 Too Many Requests\n")
        self.log("offline-1-attempt-2.log", canonical_solution(attempts=7))
        _, data = summarize(self.root)
        self.assertEqual(data["hashes_observed_lower_bound"], 7)
        self.assertEqual(data["unattested_attempts"], 1)
        self.assertFalse(data["all_attempts_hashes_accounted"])
        self.assertEqual(data["offline_node_accepted_block_count"], 1)

    def test_successful_pow_without_node_acceptance_is_not_block(self) -> None:
        p = self.log("offline-2-attempt-1.log", canonical_solution(submitted=False))
        row = inspect_attempt(p, "offline-2", 1)
        self.assertEqual(row["outcome"], "canonical_solution_not_accepted")
        self.assertEqual(row["hashes_observed_lower_bound"], 12)
        self.assertFalse(row["node_accepted"])

    def test_unknown_accelerator_domain_does_not_claim_full_range(self) -> None:
        self.log("offline-2-attempt-1.log", exhausted(backend="cuda"))
        rows, data = summarize(self.root)
        self.assertEqual(rows[0]["nonce_domain"], "backend_specific_or_partial_unattested")
        self.assertEqual(data["hashes_observed_lower_bound"], 100)
        self.assertEqual(data["offline_node_accepted_block_count"], 0)

    def test_contradiction_and_duplicate_events_rejected(self) -> None:
        self.log("offline-1-attempt-1.log", exhausted() + canonical_solution())
        with self.assertRaises(EvidenceError):
            summarize(self.root)
        self.log("offline-1-attempt-1.log", exhausted() + exhausted())
        with self.assertRaises(EvidenceError):
            summarize(self.root)

    def test_falsely_claimed_cpu_coverage_is_rejected(self) -> None:
        text = exhausted(attempts=20, budget=100)
        payload = json.loads(text.split(": ", 1)[1])
        payload["nonce_domain"] = "cpu_protocol_strided_full_budget"
        payload["nonce_start_inclusive"] = 0
        payload["nonce_end_exclusive"] = 100
        self.log("offline-1-attempt-1.log", "nonce_search_evidence_json: " + json.dumps(payload))
        with self.assertRaises(EvidenceError):
            summarize(self.root)

    def test_bad_hashes_and_accepted_exhaustion_rejected(self) -> None:
        for field, value in [
            ("attempted_hashes", -1),
            ("attempted_hashes", 101),
            ("attempted_hashes", True),
            ("canonical_accepted", True),
            ("canonical_target_hex", "garbled"),
        ]:
            with self.subTest(field=field, value=value):
                payload = json.loads(exhausted().split(": ", 1)[1])
                payload[field] = value
                self.log("offline-1-attempt-1.log", "nonce_search_evidence_json: " + json.dumps(payload))
                with self.assertRaises(EvidenceError):
                    summarize(self.root)

    def test_noncontiguous_log_sequence_rejected(self) -> None:
        self.log("offline-1-attempt-2.log", canonical_solution())
        with self.assertRaises(EvidenceError):
            summarize(self.root)

    def test_malformed_payload_and_submitted_without_mining_rejected(self) -> None:
        self.log("offline-1-attempt-1.log", "nonce_search_evidence_json: {INVALID")
        with self.assertRaises(EvidenceError):
            summarize(self.root)
        self.log("offline-1-attempt-1.log", "submit_result: accepted=true\n")
        with self.assertRaises(EvidenceError):
            summarize(self.root)

    def test_cli_writes_jsonl_and_requires_observed_64_when_requested(self) -> None:
        self.log("offline-1-attempt-1.log", canonical_solution())
        args = [
            "--miner-dir", str(self.root),
            "--jsonl", str(self.root / "accounting.jsonl"),
            "--summary", str(self.root / "accounting-summary.json"),
        ]
        self.assertEqual(main(args), 0)
        data = json.loads((self.root / "accounting-summary.json").read_text())
        self.assertEqual(data["offline_node_accepted_block_count"], 1)
        self.assertEqual(len((self.root / "accounting.jsonl").read_text().splitlines()), 1)
        with self.assertRaises(SystemExit):
            main(args + ["--require-offline-accepted", "64"])

    def test_empty_evidence_never_claims_a_complete_trial(self) -> None:
        rows, data = summarize(self.root)
        self.assertEqual(rows, [])
        self.assertFalse(data["all_attempts_hashes_accounted"])
        self.assertEqual(data["offline_node_accepted_block_count"], 0)


if __name__ == "__main__":
    unittest.main()
