# Native v3 500 ms GHOSTDAG K sensitivity — hypothetical only

**Status: NOT APPROVED / NOT MEASURED / NO MAINNET AUTHORIZATION** (24 October 2026)

Source: [#1309](https://github.com/AuriaLABS/PulseDAG/issues/1309), [candidate policy](NATIVE_V3_GHOSTDAG_FREEZE_CANDIDATE_20261022.md). Reproducible exploratory calculator: [`scripts/native_v3_k_delay_sensitivity.py`](../scripts/native_v3_k_delay_sensitivity.py).

## Scope and math

The user-targeted native-v3 cadence is **500 ms per block**, or **2 blocks/s**. For a *hypothetical* propagation window (d) seconds, assume overlapping block production count (N) has **mean μ = 2d**. This is only a toy count proxy. It is **not** the actual GHOSTDAG blue-anticone count, false-red probability, fork selection, confirmation safety, or an attack-success estimate.

The script computes **P(N > K)** via a nonnegative upper-tail sum to avoid catastrophic floating-point cancellation for tiny tails, under two unmeasured alternative statistical assumptions:

1. **Homogeneous Poisson**: (N \sim \mathrm{Poisson}(2d)). This presumes independent block generation and a fixed propagation window, not observed peer delays.
2. **Illustrative bursty Gamma–Poisson** with negative-binomial shape **r = 2** and the same mean. It has variance μ + μ²/2 and therefore probes over-dispersion, but r=2 is **not fitted to PulseDAG traffic**.

Every propagation window in the following table is **chosen for sensitivity**, not sampled from any network trace. Every K is an **UNAPPROVED comparison input**, not a protocol parameter.

## Selected illustrative results (probabilities, not percents)

| Hypothetical window | K | Poisson P(N>K) | Bursty r=2 P(N>K) |
|---|---:|---:|---:|
| 0.5 s | 6 | 0.00008324 | 0.00259107 |
| 0.5 s | 7 | 0.00001025 | 0.00096530 |
| 1.0 s | 6 | 0.00453381 | 0.03515625 |
| 1.0 s | 7 | 0.00109672 | 0.01953125 |
| 2.0 s | 6 | 0.11067398 | 0.19509221 |
| 2.0 s | 7 | 0.05113362 | 0.14306762 |
| 2.0 s | 32 | 1.76 × 10⁻¹⁹ | 0.00001854 |
| 5.0 s | 32 | 0.00000000737 | 0.01584646 |

**Interpretation:** Under even these deliberately simplistic assumptions, changing the assumed propagation tail or burstiness can change the overlap count substantially at the same K. No given K (including trial K=32) is justified without *actual propagation/arrival distributions* and an authenticated native-DAG anticone measurement. In the r=2 case, a larger tail probability under one K/window does not imply a universally worse distribution at every threshold: the shape changes the entire distribution.

## Evidence still REQUIRED for a real V3-GD-02 / V3-GD-05 freeze

- Instrument multi-node **real 500ms native-v3** propagation, compact-target PoW arrival, parent-frontier availability, 95th/99th/99.9th-percentile delays, clock drift, late/stale/orphan arrival and independent region/peer asymmetry. Record peer counts, block sizes, nodes, CPU cores, runtime configuration, sample sizes, and dataset hashes.
- Recompute per-block **actual authenticated DAG anticone blues**, merge-set size, ordering, red rate, memory, CPU, candidate traversal and worst-case validation time on the collected full-fork corpora. Compare candidate K and parent caps under 500ms and malicious fan-out/withheld-block scenarios.
- Distinguish short deterministic direct-parent limits (**proposed 8**) from potentially much larger multigeneration anticone counts (trial **K=32**). Do not infer one from the other.
- Record a clearly specified target for false-red tolerance, adversarial delays, maximum graph traversal, host resource envelopes, pruning proofs, and malicious peer DoS before choosing versioned numeric K and cap. Revalidate frozen SHA256/KHeavyHash vectors with an independent implementation.
- Until [#1309](https://github.com/AuriaLABS/PulseDAG/issues/1309) is formally frozen and [#781](https://github.com/AuriaLABS/PulseDAG/issues/781) separately authorizes an **isolated** monetary rehearsal, **do not** enable native-v3 mining or contracts. Smart contracts must remain **INACTIVE** on v3.0.

## Reproducibility (stdlib, no external packages)

```sh
python3 scripts/native_v3_k_delay_sensitivity.py --check
python3 -O scripts/native_v3_k_delay_sensitivity.py --check
python3 scripts/native_v3_k_delay_sensitivity.py --json
```

The dedicated candidate workflow checks deterministic normal/optimized results, Poisson/NB exact cases, K/window monotonicity and arithmetic range. Passing it **only establishes arithmetic reproducibility**. It is neither network security evidence nor a mainnet GO criterion.
