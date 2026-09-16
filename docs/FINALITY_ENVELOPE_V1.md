# Finality envelope v1

Status: **PLANNING SPEC**

Date: 2026-09-16 UTC
Parent issues: #1178, #794
Source thesis: `ROADMAP_V3_0_0.md` Task P8
Depends on: `PULSECLOCK_V1.md`, cadence work in `ROADMAP_V2_5_0.md` Task 41

Planning only. Does not enable default high cadence or change GHOSTDAG `k` on the Task31 candidate.

## Purpose

Publish a measured envelope instead of copying a cadence from another DAG. Public cadence and `k` change only by explicit activation after evidence.

## Measurements (from Task 41)

Test points: ~1s, 500ms, 250ms. Record at least:

- propagation delay;
- orphan / merge pressure;
- DAG width;
- state-apply latency;
- database amplification;
- template freshness and submit latency;
- stale rate;
- sync and finality behavior;
- miner fairness.

The public default MAY be more conservative than the fastest passing point.

## Published fields

| Field | Meaning |
|---|---|
| `cadence_target` | chosen public interval |
| `k` | GHOSTDAG parameter, versioned |
| `finality_depth_pulses` | N such that selected-parent reversals beyond N are treated as operationally final under stated assumptions |
| `assumption_honest_hashrate` | fraction assumed honest |
| `measured_at_sha` | exact candidate that produced the numbers |

Wallets and explorers MAY show `finality_lag` from PulseClock using `finality_depth_pulses`. Consensus still follows GHOSTDAG; this envelope is a published operational bound, not a second finality gadget.

## k rule

Silent `k` changes are forbidden. A new `k` needs:

- a versioned activation contract;
- replay of the million-block (or current) deterministic corpus;
- mixed-version window rules from v2.5 Task 43 if a live network exists.

## Fail closed

If measurements are missing, cadence stays experimental/disabled. Do not infer a public interval from a lab peak.

## Authorization

Planning only. #781 remains the only launch GO. This file does not start the 30-day public-testnet clock.
