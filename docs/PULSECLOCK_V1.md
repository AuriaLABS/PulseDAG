# PulseClock v1

Status: **PLANNING SPEC**

Date: 2026-09-15 UTC
Parent issues: #1157, #794
Source thesis: `ROADMAP_V3_0_0.md` Task P1

This document defines PulseClock v1. It does **not** authorize consensus activation, Task31 identity change, high cadence, covenant execution, or `contracts_enabled=true`.

On v2.4.0 and on a future v3.0.0 genesis until an explicit activation contract exists, PulseClock is an observational / application-planning primitive. Consensus validation must not reject a valid GHOSTDAG block solely because a PulseClock consumer disagrees with wall-clock time.

## Purpose

Give PulseDAG a deterministic, DAG-derived time that later vaults, pay streams, HTLCs, channel timeouts, and based-app challenge windows can cite.

Wall-clock and raw header timestamps are not sufficient:

- high-cadence DAGs admit parallel tips;
- miners can skew timestamps within policy bounds;
- partitions change local views before selected-parent convergence.

PulseClock answers: *what pulse is this selected DAG on, and with what uncertainty?*

## Domain

Canonical domain string:

```text
PulseDAG:pulse:v1
```

Any signed or committed pulse statement MUST bind:

- `chain_id`
- pulse version `1`
- the selected-tip block hash used as the observation root

Cross-network replay of pulse statements is forbidden.

## Values

For a node whose current selected tip is `T`:

| Field | Type | Definition |
|---|---|---|
| `pulse_version` | u32 | `1` |
| `chain_id` | chain identity | consensus/network context |
| `selected_tip` | block hash | `T` |
| `pulse_height` | u64 | `blue_score(T)` |
| `pulse_time` | i64 unix seconds | robust time of the recent blue window ending at `T` |
| `window_k` | u32 | number of blue blocks in the sample window |
| `uncertainty_secs` | u32 | half-range (or MAD-derived bound) of the sample after clipping |
| `finality_lag` | u64 | `pulse_height` distance to the last pulse considered final under the published envelope |

`pulse_height` is the only strictly monotonic field under a fixed selected chain. `pulse_time` is allowed to stall; it MUST NOT move backwards on the same selected chain except across an explicit selected-parent reorg that also changes `selected_tip`.

## Window and robust time

Let `W(T)` be the selected-chain blue blocks from `T` walking selected parents until `window_k` samples exist, or genesis is reached.

Default planning values (not activated):

- `window_k = 11`
- clip the lowest and highest timestamp
- `pulse_time = median` of the remaining samples
- `uncertainty_secs = max(1, ceil((max_clipped - min_clipped) / 2))`

Missing or out-of-policy timestamps are dropped from the sample, not replaced by wall clock. If fewer than 3 valid samples remain, `pulse_time` is the median of whatever remains and `uncertainty_secs` is reported as the policy maximum (planning default: 600).

Timestamp policy remains the existing header policy. PulseClock does not invent a second timestamp admission rule.

## Observational API

Planning surface. Not an authorization to change `API_V1.md` on the Task31 candidate.

`GET /api/v1/pulse`

```json
{
  "pulse_version": 1,
  "chain_id": "<hex or textual chain id>",
  "selected_tip": "<block hash>",
  "pulse_height": 0,
  "pulse_time": 0,
  "window_k": 11,
  "uncertainty_secs": 0,
  "finality_lag": 0
}
```

Fail closed if selected tip or blue-score metadata is unavailable. Do not fall back to host time.

## Consumers (future, gated)

When an activation contract later permits it:

- `vault_v1` delay and emergency paths cite `pulse_height` and optionally a `pulse_time` + `uncertainty_secs` floor;
- `pay_stream_v1` release buckets are `pulse_height` intervals;
- `htlc_v1` and `channel_v1` timeouts cite `pulse_height`;
- based-app v0 challenge windows are `N` pulses;
- explorer and wallet display pulse height/time next to confirmations.

Until that contract exists, nodes MAY compute PulseClock for telemetry and tests only.

## Adversarial cases

The same selected DAG MUST yield identical `(pulse_height, pulse_time, uncertainty_secs)` regardless of:

- block arrival order;
- peer order;
- miner timestamp clustering inside policy bounds, after clipping;
- restart from snapshot + delta;
- orphan adoption that does not change selected tip.

A selected-parent reorg MAY change `pulse_time`. After the new selected tip is fixed, all honest nodes MUST converge to the same pulse tuple.

A majority of recent blue timestamps skewed to the policy edge MUST be visible as larger `uncertainty_secs`. Consumers MUST treat `pulse_time + uncertainty_secs` as the earliest safe future bound and `pulse_time - uncertainty_secs` as the latest safe past bound.

## Non-goals

- Replacing GHOSTDAG selected-parent or blue/red classification.
- Using PulseClock as a PoW input.
- Requiring NTP for consensus.
- Making PulseClock a v2.4.0 Task31 protocol field.
- Activating covenant templates.

## Evidence required before any later activation

- Golden vectors: fixed DAG fixtures → exact pulse tuples.
- Reorg fixtures: selected-tip change → new tuple, then stable.
- Skew fixtures: clipped median remains deterministic.
- Replay: `pulsedag verify` reconstructs historical pulse tuples from stored selected-chain metadata without host time.

## Authorization

Planning only. Shipping a read-only observational endpoint on a later candidate still requires a separate API contract and does not activate covenants.
