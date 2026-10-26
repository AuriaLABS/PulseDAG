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

## External native-v3 first-seen observations: a strict schema, NOT measurements (2026-10-26)

A new **read-only offline validator**, [`scripts/native_v3_propagation_evidence.py`](../scripts/native_v3_propagation_evidence.py), defines a bounded input contract for *future* externally collected native-v3 propagation traces. Its existence supplies **no real 25-node data**. In particular, the CI runs **synthetic input in memory ONLY**, both normal Python and `-O`; the CI artifacts must **not** be presented as observed networking or K-security evidence. No node, miner, P2P, consensus or smart-contract implementation is modified.

The external operator is responsible for independently measuring synchronized wall clocks and recording, for each block hash, its originating miner's local **`mined`** event and each peer's first **`received`** event. The `mined` time is an **observed production event, not an untrusted block-header timestamp**. The JSON manifest must have `schema=NATIVE_V3_FIRST_SEEN_V1`, an exact 40-hex `candidate_sha`, a run ID, `instrumentation=external_native_v3_first_seen`, `synthetic=false`, `slot_ns=500000000`, and **at least 25 declared node IDs on distinct declared host IDs**, of which at least 16 are miners. Each node must supply `id`, `host_id`, `miner` (Boolean), independently measured `clock_offset_ns` and `clock_uncertainty_ns` (0–5,000,000 ns). **Neither the manifest's declarations nor the hash strings prove actual network identity, independent physical machines, accurate clock calibration, genuine v3 PoW, mining, consensus or anti-Sybil provenance. External signed/artifact-attested capture and cryptographic verification remain separate requirements.**

The trace is **JSON Lines**, one object per node's first event for a given 64-char lowercase-hex block hash:
```json
{"block_hash":"0000000000000000000000000000000000000000000000000000000000000001","node_id":"n01","event":"received","observed_wall_ns":1800000000500000000}
```
The manifest and JSONL are provided separately; no synthetic CI trace is included. The validator requires **at least 120 distinct blocks spanning at least 60 seconds and originating from 16 active declared miners**. It rejects duplicate observations, unknown miners/nodes, unknown block origins, malformed IDs and timestamps, unbounded samples, insufficient nodes or time span, ambiguous duplicate host IDs, invalid/missing clock calibration, and any receive that predates the calibrated mine time by more than the combined declared uncertainties. It reports every missing peer/block observation separately instead of silently deleting it or inventing a latency. For observed and calibrated receiver pairs only, it computes nearest-rank **p50, p95, p99 and p99.9 latency** plus an upper-bound estimate using the **declared** timestamp uncertainties; it prints counts, missingness, provenance limitations and no-automatic-K flag.

Reproduce on independently collected evidence:
```bash
python3 scripts/native_v3_propagation_evidence.py --self-test
python3 scripts/native_v3_propagation_evidence.py \
  --manifest /trusted-capture/native-v3-run-manifest.json \
  --events-jsonl /trusted-capture/native-v3-first-seen.jsonl \
  --expected-sha '<independently_frozen_40_hex_release_commit>' \
  --expected-network '<independently_frozen_native_v3_network_id>' \
  --expected-genesis '<independently_frozen_64_hex_genesis_hash>' \
  --output /trusted-capture/native-v3-latency-summary.json
```

**Limits:** The validator alone cannot attest raw events, establish that producers' observed wall clocks are correct, prove an adversarial/finality safety property or derive actual DAG **blue anticones, parent-availability, red rate, 512-bit work or K security**. The current schema is a latency-and-coverage *input hygiene* gate, not a replacement for native-v3 25-node/16-miner experiments or an approved release metric. Satisfying its minimum sample count does **not** establish representative p99.9 tails, global 500ms convergence, DoS resistance or the numeric K. Full K freeze still requires independently verified authenticated native-v3 block graphs, calibrated multi-region collection, reproducible adversarial experiments, CPU/memory/anticone analysis and protocol-owner decisions. #1309 remains UNAPPROVED, #781 NO-GO, monetary mining OFF, smart contracts INACTIVE.

## Missing receivers mean censored propagation tails, not fast peers (2026-10-26)

The external first-seen validator now explicitly distinguishes **observed-only** p50/p95/p99/p99.9 from potential percentiles over **all expected block/receiver pairs**, including missing first-seen events. A block which is not seen at a peer has **unknown/right-censored** propagation delay, **not zero or a successful observation**. The validator does not infer a successful 500ms network window from the observed subset.

For each p percentile over N expected peer/block pairs, let M be missing and `r=ceil(N*p)` the nearest rank (p as a fraction). Sorting the N−M observed nonnegative delays, the minimum possible full-population p percentile treats M missing as zero: observed rank `r−M` if positive, otherwise 0. The **maximum possible** full-population p percentile treats all M missing as arbitrarily late: observed rank `r` if `r <= N−M`, otherwise **unbounded/unknown** (`null` in machine-readable output). These are deterministic bounds under missingness, not statistical confidence limits and not clock/propagation security proofs. The existing observed-only percentile and separate declared clock-uncertainty estimate remain labeled separately.

CI synthetic adversarial tests explicitly remove one reception and then four (among 2,904 expected block/peer pairs): a single missing observation is still counted, while four unobserved high-tail events make any finite worst-case full-population p99.9 **unprovable**. Removing a receiver's entire 121 observations likewise requires a `null` p99.9 upper bound. This prevents a dangerously optimistic headline like “p99.9 under 500ms” when the slowest receivers have no data. The test is strictly synthetic: real propagation, PoW-authenticated anticone, node trust and K selection remain **unmeasured, unapproved and unauthorised**.

## Reject self-claimed code, chain, and genesis identity (2026-10-26)

The same external-latency evidence parser now **requires three independently supplied operator pins**: `--expected-sha` (full lowercase 40-hex candidate commit), `--expected-network` (frozen network ID) and `--expected-genesis` (frozen 64-hex native-v3 genesis commitment). Those three values must be obtained from an **independent release/network configuration**, never read from the untrusted trace, its metadata or the tool's own output. The manifest must also explicitly contain `candidate_sha`, `network_id` and `genesis_hash`, and all three must match the independent pins exactly or validation **fails closed**, including a well-formed *different* SHA/hash. Normal and Python `-O` CI synthetic tests exercise incorrect SHA, wrong network, fake genesis and missing/malformed caller-supplied pins.

**This is merely identity matching.** An attacker can still fabricate events or lie about physical topology and NTP/PTP calibration while copying the true SHA/network/genesis. The parser does **not** reconstruct blocks or compute authentic GHOSTDAG anticone, and cannot sign or attest the observation collector. Genuine measured evidence needs a separately authenticated source, independently checked native block/envelope and per-peer identities, a preserved raw trace hash, and tamper-evident collection/custody. In the absence of those, neither a successful parser exit nor attractive p99.9 values can approve any numeric K, the parent limit, monetary mining, or mainnet.

## Deep-fork stress probe for cached GHOSTDAG anticone counts (2026-10-26)

The dormant Rust candidate now tracks **exact blue-anticone counts incrementally** across authenticated selected-parent ancestry. Instead of repeatedly recomputing every existing blue-versus-blue relationship for *every* new merge vertex, it clones the selected parent's count map; when a new blue has N incomparable previous blues, it rejects unless N ≤ trial K and **each affected prior count is less than K**; on acceptance, increments those counts exactly once and inserts the new blue with count N. Non-incomparable blues' counts do not change. The new candidate itself has count zero, as it descends from all earlier blue-past vertices. This reuses the same proposed inclusive K-cluster semantics; it changes neither policy IDs, active consensus, nor money. Old independent symbolic/Python and real-PoW Rust color/blue-work vectors remain mandatory. Current local graph cap stays **128 full authenticated headers**; cached counts are diagnostic only and must not be trusted as peer-supplied metadata.

To evaluate something more adversarial than a ten-header eight-parent fork, the opt-in CI local CPU/RSS profile includes **two further generated full-PoW 98-header cases**: eight independent branches of 12 authenticated non-genesis native-v3 headers each, one genesis and a final eight-parent join. For **trial K=0**, expected join: score 13, blue work 26, **0 accepted / 84 rejected** side vertices. For **trial K=32**, expected join: score 37, blue work 74, **24 accepted / 60 rejected** side vertices. These equal-work topological chains expose repeated incomparable-blue checking across multiple merges, while the committed score and native compact-target work are checked independently by assertions; this is a **candidate regression**, not an approved K. The workflow requires **nine** tagged local timing rows instead of seven and archives machine-readable per-SHA measurements. The earlier seven figures in the table above refer **only to their earlier exact-head historical run** and are not assumed to be results of this optimization.

A valid performance comparison must record **both candidate SHAs**, runner CPU/OS/load, graph fixture, three repeats, and changing process VmHWM. Absolute timings or a single CI speedup cannot prove an efficient production node, real-network 500ms convergence, a sound anticone/finality model, or authority to activate mining. The new candidate's exact-head full CI must finish green before treating its new resource data as verified. If incremental caching regresses CPU/memory, does not preserve Rust/Python golden output, or has any inconsistent blue/anticone count, do not integrate the candidate; refine or revert in DRAFT.
