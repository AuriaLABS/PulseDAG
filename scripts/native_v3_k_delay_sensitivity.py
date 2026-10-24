#!/usr/bin/env python3
"""NON-NORMATIVE 500ms native-v3 GHOSTDAG overlap proxy, not a K recommendation.

Compute P(N > K) where N is the number of blocks produced during a chosen
hypothetical propagation window at 2 blocks/s. N is NOT a measured GHOSTDAG
anticone; this is NOT real peer propagation, adversarial security, DAG
convergence, a protocol policy, a miner or a production readiness gate.

Two explicitly ASSUMED count distributions:
 - homogeneous Poisson, mean = 2 * window seconds;
 - Gamma-Poisson (negative binomial) with illustrative shape r=2 at
   identical mean (over-dispersion / bursts; not a fitted distribution).

All calculations use Python Decimal and sum a non-negative *upper tail*
directly, avoiding cancellation in 1-CDF for very small probabilities.
"""
from __future__ import annotations

import argparse
import json
from decimal import Decimal, localcontext

BLOCK_INTERVAL_SECONDS = Decimal("0.5")
BLOCKS_PER_SECOND = Decimal(1) / BLOCK_INTERVAL_SECONDS
WINDOWS_SECONDS = ("0.1", "0.5", "1", "2", "5")
TRIAL_K = (0, 6, 7, 32)
ASSUMED_BURST_SHAPE = Decimal(2)
SMALL = Decimal("1e-90")
PRECISION = 96


def upper_tail(mean: Decimal, k: int, burst_shape: Decimal | None = None) -> Decimal:
    """Probability for N>K, with NO subtraction from unity."""
    if not mean.is_finite() or mean < 0 or type(k) is not int or k < 0:
        raise ValueError("nonnegative finite mean and integer K required")
    if burst_shape is not None and (
        not burst_shape.is_finite() or burst_shape <= 0
    ):
        raise ValueError("burst shape must be positive and finite")
    with localcontext() as ctx:
        ctx.prec = PRECISION
        if burst_shape is None:
            prob = (-mean).exp()

            def ratio(n: int) -> Decimal:
                return mean / Decimal(n)

        else:
            q = mean / (burst_shape + mean)
            prob = (Decimal(1) - q) ** burst_shape

            def ratio(n: int) -> Decimal:
                return q * (burst_shape + n - 1) / Decimal(n)

        # Begin at the first strictly excessive overlap count (K+1).
        for n in range(1, k + 2):
            prob *= ratio(n)
        tail = prob
        for n in range(k + 2, 4097):
            prob *= ratio(n)
            tail += prob
            if prob < SMALL:
                break
        else:
            raise ArithmeticError("overlap tail failed to converge in 4096 terms")
        if not Decimal(0) <= tail <= Decimal(1):
            raise ArithmeticError("calculated probability outside [0,1]")
        return +tail


def study() -> list[dict[str, str | int]]:
    rows = []
    for window in WINDOWS_SECONDS:
        mean = BLOCKS_PER_SECOND * Decimal(window)
        for k in TRIAL_K:
            rows.append({
                "propagation_window_seconds_assumed": window,
                "block_rate_per_second_assumed": str(BLOCKS_PER_SECOND),
                "mean_overlapping_blocks_assumed": str(mean),
                "trial_K_NOT_APPROVED": k,
                "poisson_P_over_K": format(upper_tail(mean, k), ".12E"),
                "bursty_r2_P_over_K": format(
                    upper_tail(mean, k, ASSUMED_BURST_SHAPE), ".12E"
                ),
            })
    return rows


def self_test() -> None:
    assert BLOCK_INTERVAL_SECONDS == Decimal("0.5")
    assert upper_tail(Decimal(0), 0) == 0
    assert upper_tail(Decimal(0), 32, ASSUMED_BURST_SHAPE) == 0
    for model in (None, ASSUMED_BURST_SHAPE):
        for window in WINDOWS_SECONDS:
            mean = Decimal(window) * BLOCKS_PER_SECOND
            ks = [upper_tail(mean, k, model) for k in TRIAL_K]
            assert all(Decimal(0) <= p <= Decimal(1) for p in ks)
            assert all(x >= y for x, y in zip(ks, ks[1:]))
        for k in TRIAL_K:
            probs = [
                upper_tail(Decimal(window) * BLOCKS_PER_SECOND, k, model)
                for window in WINDOWS_SECONDS
            ]
            assert all(x <= y for x, y in zip(probs, probs[1:]))

    # Independently hand-derived exact cases: NB(r=2,mean=2) uses q=1/2.
    assert abs(upper_tail(Decimal(2), 6, Decimal(2)) - Decimal("0.03515625")) < Decimal("1e-84")
    assert abs(upper_tail(Decimal(2), 7, Decimal(2)) - Decimal("0.01953125")) < Decimal("1e-84")
    # Poisson K=0 identity with analytic reference.
    with localcontext() as ctx:
        ctx.prec = PRECISION
        ref = Decimal(1) - (-Decimal(1)).exp()
        assert abs(upper_tail(Decimal(1), 0) - ref) < Decimal("1e-85")
    assert upper_tail(Decimal(1), 32) > 0  # Never round a tiny tail to zero.
    rows = study()
    assert len(rows) == len(WINDOWS_SECONDS) * len(TRIAL_K)
    assert len({
        (x["propagation_window_seconds_assumed"], x["trial_K_NOT_APPROVED"])
        for x in rows
    }) == len(rows)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="run deterministic assertions")
    parser.add_argument("--json", action="store_true", help="emit machine-readable proxy data")
    args = parser.parse_args()
    if args.check:
        self_test()
    rows = study()
    if args.json:
        print(json.dumps({
            "status": "TEST_ONLY_UNAPPROVED",
            "not_real_network_measurements": True,
            "500ms_block_interval_assumed": True,
            "peer_delay_empirical_evidence": False,
            "not_ghostdag_K_security_probability": True,
            "model_note": "Poisson and illustrative Gamma-Poisson r=2 overlap proxies",
            "rows": rows,
        }, indent=2))
    else:
        print("TEST ONLY — 500ms Poisson / bursty r=2 overlap proxy; NOT an approved GHOSTDAG K")
        print("window_s K  P_Poisson(N>K)       P_GammaPoisson_r2(N>K)")
        for row in rows:
            print(
                f'{row["propagation_window_seconds_assumed"]:>8}'
                f' {row["trial_K_NOT_APPROVED"]:>2} '
                f'{row["poisson_P_over_K"]:>20} '
                f'{row["bursty_r2_P_over_K"]:>24}'
            )
        if args.check:
            print("PASS: deterministic overlap proxy invariants and exact NB(r=2) checks")
        print("No empirical peer delays, attacks, canonical K choice or mainnet GO inferred.")


if __name__ == "__main__":
    main()
