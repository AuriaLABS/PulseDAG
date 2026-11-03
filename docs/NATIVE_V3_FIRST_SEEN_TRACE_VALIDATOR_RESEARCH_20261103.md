# Native-v3 first-seen propagation trace validator (research only)

**Status: DORMANT / read-only / not a K selection or launch gate.** Extracted as a self-contained slice from draft [#1341](https://github.com/AuriaLABS/PulseDAG/pull/1341) for [#1309](https://github.com/AuriaLABS/PulseDAG/issues/1309). This file's addition does **not** activate a node, miner, protocol setting, public network, monetary runtime or contracts.

`scripts/native_v3_propagation_evidence.py` validates a *provided* first-seen JSONL trace and accompanying manifest against **independently supplied** source/network/genesis pins. It does not collect traces or prove that a trace is genuine: manifests, timestamps, producer IDs, PoW, calibration and physical host independence still require separate authenticated capture and validation. The demonstration in `--self-test` is synthetic data **in memory**, not empirical 25-node evidence.

## Bounded analysis contract

- Requires at least 25 declared nodes on distinct hosts, 16 actual miner IDs, 120 blocks, at least 60 seconds of observations, and a declared 500ms cadence.
- Independently pinned candidate SHA, network ID and genesis hash must match; refuses explicitly synthetic manifests, bad/duplicate JSON fields, NaN/Infinity, unbounded input, invalid clocks, conflicting events and unknown producers.
- Limits manifest size to 128KiB, events to 64MiB / 200,000 rows / 4KiB per record, and potential peer-block comparisons to 1,000,000. These are **parser budgets**, not approved GHOSTDAG consensus limits.
- Reports observed p50/p95/p99/p99.9 first-seen delays and flags missing peer receptions as *right-censored*, potentially leaving the full-population p99.9 without a finite upper bound; it never treats missing data as fast delivery.
- Produces a research result with explicit `no_automatic_K_or_parent_limit_selection=true`, `not_authenticated_native_headers_or_anticone=true`, and `not_convergence_sla_or_mainnet_go=true`.

## Safe reproduction

```sh
python3 scripts/native_v3_propagation_evidence.py --self-test
python3 -O scripts/native_v3_propagation_evidence.py --self-test
```

For independently collected **real** traces, pass `--manifest`, `--events-jsonl`, `--output`, `--expected-sha`, `--expected-network`, and `--expected-genesis` as operator-controlled arguments. Never derive those expected IDs from the untrusted manifest itself. These command-line inputs must not substitute for external event signatures or verified block headers.

## Next independent, uncompleted gates

1. Native-v3 instrumentation must produce independently authenticated first-seen records from actual 500ms multi-node network trials, with calibrated clocks and actual chain-bound PoW headers.
2. Derive genuine blue/red anticone and selected-parent data using a separately approved, fully authenticated GD-01..07 policy. Measured first-seen latency alone cannot establish K or safe pruning.
3. Freeze exact K, max parents, bounded visits and work/retarget rules using independent vectors and adversarial resource evidence. Then implement GD-08 monetary replay, trusted checkpoint provenance, atomic restore/reorg and full-history versus pruned equivalence.
4. Require exact-head CI and separate #781 GO for any network launch.

**Mainnet/public testnet NO-GO. Native-v3 monetary mining OFF. Smart contracts INACTIVE/fail-closed on v3.0.**
