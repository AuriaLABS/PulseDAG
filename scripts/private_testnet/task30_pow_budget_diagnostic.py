#!/usr/bin/env python3
"""Read-only, non-consensus Task30 CPU PoW work-budget estimator.

This tool does NOT mine blocks, accept a hash, override consensus compact bits,
change genesis identity, or authorize a rehearsal/mainnet launch. Uniform and
independent hashes at a constant observed threshold are a MODEL, not empirical
proof: retarget, stale templates, startup, reorg and restart add costs. Model
numbers are diagnostic lower bounds for feasibility, not pass criteria.
"""
from __future__ import annotations

import argparse
import json
import math
import sys

MAX_TARGET = (1 << 256) - 1


def calculate(
    target_hex: str,
    hash_rate: float,
    tries_per_batch: int,
    batches_per_block: int,
    blocks: int,
    timeout_minutes: float,
) -> dict[str, object]:
    if len(target_hex) != 64 or any(ch not in "0123456789abcdefABCDEF" for ch in target_hex):
        raise ValueError("target must be exactly 64 hexadecimal digits")
    target = int(target_hex, 16)
    if not math.isfinite(hash_rate) or hash_rate <= 0:
        raise ValueError("hash_rate must be finite and positive")
    if (
        isinstance(tries_per_batch, bool)
        or isinstance(batches_per_block, bool)
        or isinstance(blocks, bool)
        or tries_per_batch < 1
        or batches_per_block < 1
        or blocks < 1
    ):
        raise ValueError("batches, batch tries and blocks must be positive integers")
    if not math.isfinite(timeout_minutes) or timeout_minutes <= 0:
        raise ValueError("timeout_minutes must be finite and positive")
    probability = (target + 1) / (1 << 256)
    bounded_tries_per_block = tries_per_batch * batches_per_block
    if probability >= 1:
        fail_per_block = 0.0
    else:
        fail_per_block = math.exp(bounded_tries_per_block * math.log1p(-probability))
    any_fail = -math.expm1(blocks * math.log1p(-fail_per_block)) if fail_per_block < 1 else 1.0
    expected_hashes_per_block = 1.0 / probability
    expected_mining_seconds = blocks * expected_hashes_per_block / hash_rate
    # The runtime includes node initialization, network convergence, pruning,
    # checkpoint verification and state comparison; this is only MINING time.
    timeout_seconds = timeout_minutes * 60.0
    return {
        "kind": "TASK30_POW_BUDGET_DIAGNOSTIC_ONLY",
        "non_consensus": True,
        "target_hex": target_hex.lower(),
        "hash_rate_per_second": hash_rate,
        "batch_tries": tries_per_batch,
        "batches_per_block": batches_per_block,
        "blocks": blocks,
        "budgeted_hashes_per_block": bounded_tries_per_block,
        "probability_per_hash_uniform_model": probability,
        "expected_hashes_per_block_uniform_model": expected_hashes_per_block,
        "probability_one_block_exhausts_budget_uniform_model": fail_per_block,
        "probability_any_block_exhausts_budget_uniform_model": any_fail,
        "expected_mining_seconds_lower_bound_model": expected_mining_seconds,
        "workflow_timeout_seconds": timeout_seconds,
        "expected_mining_time_exceeds_workflow_timeout": expected_mining_seconds > timeout_seconds,
        "requires_real_pow_for_every_block": True,
        "model_limitations": (
            "Independent uniform hashes at constant observed target; excludes "
            "retarget, startup, networking, pruned restart, I/O and stale templates. "
            "Do not infer actual success/failure from this model."
        ),
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target-hex", required=True, help="observed 64-hex-digit target, not compact bits")
    parser.add_argument("--hash-rate", required=True, type=float, help="observed full-search CPU hashes/s")
    parser.add_argument("--tries-per-batch", required=True, type=int)
    parser.add_argument("--batches-per-block", required=True, type=int)
    parser.add_argument("--blocks", required=True, type=int)
    parser.add_argument("--timeout-minutes", required=True, type=float)
    parser.add_argument(
        "--fail-if-expected-mining-exceeds-timeout",
        action="store_true",
        help="optional planning policy; no change to mining or consensus verification",
    )
    args = parser.parse_args(argv)
    try:
        result = calculate(
            args.target_hex, args.hash_rate, args.tries_per_batch,
            args.batches_per_block, args.blocks, args.timeout_minutes,
        )
    except ValueError as exc:
        parser.error(str(exc))
    print(json.dumps(result, sort_keys=True, indent=2, allow_nan=False))
    if (
        args.fail_if_expected_mining_exceeds_timeout
        and result["expected_mining_time_exceeds_workflow_timeout"]
    ):
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main())
