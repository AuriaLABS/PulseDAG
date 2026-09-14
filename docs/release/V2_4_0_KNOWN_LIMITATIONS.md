# PulseDAG v2.4.0 — known limitations and release boundary

Status: Task31 candidate preparation. This document is not a release, activation or public-testnet authorization.

## Current technical scope

The v2.4.0 repository contains the node and standalone external miner technical stack. The previously exposed legacy raw-key wallet RPC surface has been removed/tombstoned and **no official end-user custody wallet is part of the current release candidate**. Do not advertise production custody or seed/key management as a v2.4.0 feature.

`pulsedag-miner` is an external standalone miner. It does not provide pool coordination, shares, payouts or accounting. The CPU path is the canonical operational reference; optional GPU support does not change consensus.

## Public-testnet security blockers

Issue #1127 is the live authoritative RustSec/public-GO dependency record (historical #803), with the remaining dependency cleanup tracked in #1139.

The current lock no longer contains `linkme 0.2.10` or `lru 0.12.5`; those were historical v2.4 findings removed through supported parent-stack migrations. They must not be reintroduced and are enforced as forbidden legacy versions by the active v3 dependency-security gate.

The reachable `atty 0.2.14` warning remains visible and remains an unresolved public-GO blocker. Runtime `bincode 1.3.3` is also still visible as an unmaintained first-party dependency; its migration must preserve storage, snapshot, fast-sync and rollback compatibility according to `docs/security/V3_BINCODE_MIGRATION_PLAN.md`.

Hickory 0.25.2 advisories remain visible as lock-only residue and are permitted only while exact clean compiler-artifact evidence proves them unreachable from every launch root. `.cargo/audit.toml` must not hide these advisories.

A stable expected warning set is not security approval. Unsupported transitive leaf patches are not authorized.

## Activation guardrails

Until an explicit decision is recorded in the relevant control issue:

- public-testnet launch is not authorized;
- `public_testnet_ready=false`;
- `thirty_day_public_testnet_clock_started=false`;
- default high cadence is not authorized;
- smart contracts remain disabled;
- no Day 0 timestamp may be recorded.

Smart-contract activation is not part of the v3.0 launch; it requires a separately gated later protocol upgrade.

## Public network package

Files under `configs/public-testnet/` are pre-GO templates only. They intentionally contain `__TASK31_FREEZE_REQUIRED__` placeholders and keep P2P/public readiness disabled. They are not final bootnode, DNS, TLS, chain/network or RPC deployment values.

Final seed/node/observer/miner configs must be rendered only after the exact release SHA, chain/network identity, genesis/config digests, bootnode peer IDs/multiaddrs, public endpoint ownership and artifact digests are frozen in #781/#794.

## Exact-SHA evidence rule

Task30/Task31 evidence is valid only for the source SHA and activation contract it actually tested. Any source, dependency, config, release-surface or workflow change that produces a new release candidate requires the affected exact-SHA gates to rerun. Evidence from different SHAs must not be combined to claim final readiness.

Historical or superseded burn-ins do not count toward final v3 launch evidence. Final burn-in requirements are controlled by #781/#794 on one unchanged exact candidate.

## Operator and infrastructure boundary

Repository templates do not provision or prove real public infrastructure. Before public GO, operators must separately record and verify failure-domain separation, persistent P2P identities, firewall policy, NTP/time sync, storage/backup, DNS/TLS ownership, observability, incident escalation and recovery procedures.

See `SECURITY.md`, `docs/security/V3_DEPENDENCY_SECURITY.md`, `docs/security/V3_BINCODE_MIGRATION_PLAN.md`, #781, #794, #1127, #1139, historical #803 and #873.
