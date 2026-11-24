# PulseDAG Version Matrix

## Current baseline

| Area | Value |
|---|---|
| VERSION file | `v3.0.0` |
| Cargo workspace version | `3.0.0` |
| Current milestone | Active candidate construction — v3.0.0 first-network implementation and exact-SHA validation |
| Candidate state | Moving candidate; exact final SHA not frozen |
| Final decision | `PENDING_EXACT_CANDIDATE_EVIDENCE` |
| Active CI target | v3.0.0 only |
| Protocol baseline | native-v3 GHOSTDAG and UTXO/PoW authority (integration pending) |
| Chain identity | Fresh v3 chain/genesis identity required; final digest not frozen |
| Release scope | node + standalone miner; user surfaces require their own v3 gate |
| High cadence | disabled by default pending v3 evidence |
| Tag | No `v3.0.0` tag created |
| Publication | GitHub Release publication not authorized |
| Public testnet | `public_testnet_ready=false` |
| 30-day clock | `thirty_day_public_testnet_clock_started=false` |
| Smart contracts | `contracts_enabled=false` |

## Network lifecycle

| Stage | Status |
|---|---|
| Public native-v3 testnet | First network; PRE-GO preparation, launch not authorized |
| Mainnet | Separate genesis and GO after testnet evidence |

No earlier deployed network or state migration obligation exists.

## v3.0.0 evidence state

All release/readiness evidence that can affect launch must be regenerated or explicitly validated on the exact v3.0.0 candidate SHA. Prototype evidence may support applicable regression and provenance only; it does not establish native launch acceptance.

The active v3 candidate is closing:

- exact v3 repository/binary identity;
- multi-node compact-relay and fast-sync convergence;
- deterministic replay/snapshot/prune evidence;
- launch-path GPU-consensus isolation;
- cadence/finality evidence;
- packaging, recovery, dependency/security, and release validation;
- exact-candidate burn-in and launch decision.

## Current authorization boundary

`PENDING_EXACT_CANDIDATE_EVIDENCE` means none of the following is authorized:

- creating the `v3.0.0` tag;
- publishing a GitHub Release;
- launching public testnet or mainnet;
- recording or backdating Day 0;
- setting `public_testnet_ready=true`;
- starting the 30-day public-testnet clock;
- enabling high cadence by default;
- enabling smart contracts;
- treating a version bump, roadmap item, or partial CI pass as launch approval.

## Repository version rule

Primary active repository surfaces must identify `v3.0.0` / `3.0.0` consistently. Active pull-request, release, evidence, and readiness workflows must be v3.0 workflows. Retire obsolete prototype material from the active tree once its useful requirements and regressions are preserved. No prior network compatibility or migration gate.

## What `main` is

`main` is the **moving v3.0.0 candidate line**, not a frozen release or launch authorization.

Presence of inactive modules does not activate them. In particular, `contracts_enabled=false` remains mandatory throughout v3.0.0; activation requires a later separately versioned decision.

| Line | Meaning |
|---|---|
| `main` | Moving v3.0.0 candidate construction line |
| Frozen v3 candidate | Does not exist until an exact SHA is recorded and all mandatory evidence is rerun |
| Earlier development | Git provenance and applicable regression only |

## Live control issues

| Topic | Live issue |
|---|---|
| Coordinated launch control | #781 |
| Integrated v3 program | #794 |
| Compact relay / multi-node evidence | #1034 |
| Remaining crate-graph / persistence migration | #1153 |
| Long-lived v3 core | see `ROADMAP_V3_0_LONG_LIVED_CORE.md` |
