# PulseDAG Version Matrix

## Current baseline

| Area | Value |
|---|---|
| VERSION file | `v3.0.0` |
| Cargo workspace version | `3.0.0` |
| Current milestone | v3.0.0 candidate construction and exact-SHA validation |
| Candidate state | Moving candidate; exact final SHA not frozen |
| Final decision | `PENDING_EXACT_CANDIDATE_EVIDENCE` |
| Active CI target | v3.0.0 only |
| Protocol baseline | transaction v2 + block-header v2 + `ghostdag_v1` with v3 runtime/evidence surfaces |
| Chain identity | Fresh v3 chain/genesis identity required; final digest not frozen |
| Release scope | node + standalone miner; user surfaces require their own v3 gate |
| High cadence | disabled by default pending v3 evidence |
| Tag | No `v3.0.0` tag created |
| Publication | GitHub Release publication not authorized |
| Public testnet | `public_testnet_ready=false` |
| 30-day clock | `thirty_day_public_testnet_clock_started=false` |
| Smart contracts | `contracts_enabled=false` |

## Version progression

| Version | Scope | Status |
|---|---|---|
| `v2.2.x` | Earlier private-testnet hardening and rehearsal | Historical |
| `v2.3.0` | Previous private-testnet candidate baseline | Historical / compatibility only |
| `v2.4.0` | Transaction/header v2 and GHOSTDAG predecessor candidate | Historical predecessor / regression input |
| `v2.5.0` | Scale/GPU/resilience planning source | Folded into v3 backlog; not an active release target |
| `v2.6.0` | Programmability planning source | Folded into v3 backlog; not an active release target |
| `v3.0.0` | Current Pulse Layer candidate | **Active candidate construction** |

## v3.0.0 evidence state

All release/readiness evidence that can affect launch must be regenerated or explicitly validated on the exact v3.0.0 candidate SHA. Historical v2.x evidence can prove compatibility, regression coverage, migration behavior, or provenance, but it is not a v3 launch gate.

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

Primary active repository surfaces must identify `v3.0.0` / `3.0.0` consistently. Active pull-request, release, evidence, and readiness workflows must be v3.0 workflows. Earlier-version files may remain only when clearly historical, compatibility-oriented, migration-oriented, or archival; they must not be required as earlier-version release gates.

## What `main` is

`main` is the **moving v3.0.0 candidate line**, not a frozen release or launch authorization.

Presence of inactive modules does not activate them. In particular, `contracts_enabled=false` remains mandatory until a separate explicit activation decision changes that boundary.

| Line | Meaning |
|---|---|
| `main` | Moving v3.0.0 candidate construction line |
| Frozen v3 candidate | Does not exist until an exact SHA is recorded and all mandatory evidence is rerun |
| Historical v2.x | Compatibility, migration, regression, and provenance only |

## Live control issues

| Topic | Live issue |
|---|---|
| Coordinated launch control | #781 |
| Integrated v3 program | #794 |
| Compact relay / multi-node evidence | #1034 |
| Remaining crate-graph / persistence migration | #1153 |
| Long-lived v3 core | see `ROADMAP_V3_0_LONG_LIVED_CORE.md` |
