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

## Native Rust local CPU-only evidence — measured on one CI runner (2026-10-25)

The earlier Poisson/Gamma-Poisson table above is **purely hypothetical**. Unlike that table, the following wall-clock durations are **actual local profiling outputs** printed by the *ignored* Rust `native_v3_ghostdag_perf_probe` test in successful [exact-head candidate run 40194809369](https://github.com/AuriaLABS/PulseDAG/actions/runs/40194809369), at commit [`c73fbf5e`](https://github.com/AuriaLABS/PulseDAG/commit/c73fbf5e4eabdc0448d543e67cc178cec44a09d6) on 2026-10-25. The workflow ran three repetitions per fixture. **These times are neither network propagation delays nor a representative production node benchmark.** Every K below remains *unapproved*.

| Local fixture | Authenticated headers, incl. genesis | Trial K | Wall-clock microseconds (3 repetitions) | Median | Median / 500ms target interval |
| --- | ---: | ---: | --- | ---: | ---: |
| Linear 16 @ assumed 500ms timestamps | 17 | 32 | 22,080 / 22,253 / 22,298 | 22.253 ms | 4.45% |
| Linear 64 @ assumed 500ms timestamps | 65 | 32 | 90,162 / 90,637 / 91,256 | 90.637 ms | 18.13% |
| Linear 120 @ assumed 500ms timestamps | 121 | 32 | 173,279 / 173,664 / 174,264 | **173.664 ms** | **34.73%** |
| Eight-parent, complete ten-header DAG | 10 | 0 | 12,572 / 12,683 / 12,872 | 12.683 ms | 2.54% |
| Eight-parent, same topology | 10 | 6 | 12,783 / 12,815 / 12,968 | 12.815 ms | 2.56% |
| Eight-parent, same topology | 10 | 7 | 12,861 / 12,948 / 12,973 | 12.948 ms | 2.59% |
| Eight-parent, same topology | 10 | 32 | 12,833 / 12,856 / 12,988 | 12.856 ms | 2.57% |

The runner's printed `process_vmhwm_kib` was 4,456 for the first fixture, 5,008 for linear 64, and 6,360 for linear 120 and later eight-parent fixtures. **VmHWM is process-lifetime high-water memory at the moment of observation**, not memory consumed *by each case*, nor node service RSS, nor a bound on malicious DAGs. Timing includes **the entire local test fixture evaluation** as implemented; it must not be interpreted as pure incremental block-validation time, guaranteed maximum latency, or real peer convergence. The 121-header synthetic linear case already uses a large part of the 500ms interval on that runner, while the existing hard **128-header/2048-edge diagnostic budget** rejects still-longer complete history. This is a concrete scalability concern, not approval for raising the cap or pruning without proof.

To prevent transcription-only reporting, `scripts/native_v3_ghostdag_perf_evidence.py` now parses all **seven** mandatory probe log rows with exact candidate SHA; verifies case identities, header counts, trial-K inputs, three positive samples, recomputed median, process VmHWM presence and no duplicates/omissions; produces deterministic JSON artifact containing explicit `not_network_propagation_measurement`, `not_K_security_or_parent_cap_evidence` and `not_go_no_mining_activation` flags. CI runs normal/optimized Python self-tests, validates JSON syntax and uploads that per-run artifact alongside the raw local log. The artifact is intended for later *comparisons across comparable runner configurations*, not thresholds, pooled medians, or a production performance pass. Test-only measured timings fluctuate with machine contention, warming, CI hardware and software revisions.

**Actual blocker remains:** no real multi-node 500ms propagation/anticone corpus, adversarial network delay measurements, independently approved numerical K=32 or eight-parent cap, production hot-path CPU/DoS budget, authenticated pruning/finality proof or authoritative native-v3 UTXO replay. The presence of this measured local data does not resolve #1309 or authorize the mainnet.
