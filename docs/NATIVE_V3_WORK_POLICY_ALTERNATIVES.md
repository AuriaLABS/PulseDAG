# Native-v3 work-from-target alternatives — UNAPPROVED

**Status: decision evidence only, NOT consensus rules.** Scope: [#1309](https://github.com/AuriaLABS/PulseDAG/issues/1309) V3-GD-03/06. No selected parent, K, blue/red, scoring, issuance, monetary-v3 activation or contract execution is authorized by this note. Both native-v3 startup barriers stay fail-closed and #781 mainnet remains NO-GO.

## Candidate integer work contributions

A validated native-v3 non-genesis header commits to a canonical full-width target T via compact bits. The KHeavyHash digest determines validity, **not** cumulative GHOSTDAG work. Compare three unfrozen choices for a single valid header:

- **Unit:** one work unit per valid header. It ignores target difficulty.
- **Weighted A:** floor(2^256 / (T + 1)). A 2^256 numerator **requires 257 bits**, even though its final contribution fits u256.
- **Weighted B:** floor((2^256 - 1) / (T + 1)). A u256 numerator. Unlike A, it can yield a different value: this is **not** an interchangeable implementation shortcut.

All division is exact integer division. T must pass independent, canonical native-v3 bounds before evaluation. No floating-point, u64 digest proxy, saturating math or inherited v1 unit-work is acceptable without an explicit approved protocol choice.

| Canonical bits | Unit option | Weighted A | Weighted B |
| --- | ---: | ---: | ---: |
| 0x207fffff | 1 | 2 | 2 |
| 0x1f7fffff | 1 | 512 | 512 |
| 0x1e7fffff | 1 | 131072 | 131072 |
| 0x1d00ffff | 1 | 4295032833 | 4295032833 |
| 0x01010000 | 1 | 2^255 | 2^255 - 1 |

Under A, **two minimum-target valid block contributions sum to 2^256**, overflowing any unsigned 256-bit accumulator. This is a candidate mathematical counterexample, NOT a claim that two such blocks are GHOSTDAG blues. A production accumulator must use chosen and verified wider arithmetic or reject overflow deterministically; it must never wrap/saturate silently.

## Reproduce the evidence

Run:

    python3 scripts/native_v3_compact_target_oracle.py
    python3 scripts/native_v3_work_policy_candidates.py --self-test
    python3 scripts/native_v3_work_policy_candidates.py

The Python stdlib script uses arbitrary-precision integer arithmetic, checks the frozen target bytes against the existing compact-target reference, demonstrates both numerator interpretations and the checked-u256 overflow case, rejects noncanonical targets and emits a JSON machine-readable **comparison**. Large JSON integers are encoded as decimal strings for precision. This is not an independent GHOSTDAG golden-vector implementation.

## What the protocol owner must still approve

V3-GD-03 and V3-GD-06 remain explicitly **UNFROZEN**: select one work formula and its numerator, target validation identity, genesis contribution, unit/weight policy, accumulation bit width and overflow error, blue-work recurrence (selected-parent plus chosen merge-set blues), exact blue-score equality, K/anticone bounds, tie fields, deterministic order, and independent cross-runtime vectors. The owner must approve a versioned rule and spec hash on #1309 **before** any authoritative Rust selector or native monetary rehearsal uses it. This is deliberately not that approval.

The dormant reference cannot establish any #781 GO gate or #1333 contract implementation readiness. Contracts remain INACTIVE in all v3.0 networks.
