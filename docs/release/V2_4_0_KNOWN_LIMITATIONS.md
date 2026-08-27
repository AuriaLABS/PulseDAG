# PulseDAG v2.4.0 — known limitations and release boundary

Status: Task31 candidate preparation. This document is not a release, activation or public-testnet authorization.

## Current technical scope

The v2.4.0 repository contains the node and standalone external miner technical stack. The previously exposed legacy raw-key wallet RPC surface has been removed/tombstoned and **no official end-user custody wallet is part of the current release candidate**. Do not advertise production custody or seed/key management as a v2.4.0 feature.

`pulsedag-miner` is an external standalone miner. It does not provide pool coordination, shares, payouts or accounting. The CPU path is the canonical operational reference; optional GPU support does not change consensus.

## Public-testnet security blockers

Issue #803 remains the authoritative RustSec/public-GO dependency record. The current fail-closed disposition keeps reachable `atty 0.2.14`, `linkme 0.2.10` and `lru 0.12.5` visible as public-testnet blockers until removed through supported parent-stack upgrades or an explicitly reviewed public-GO disposition.

A stable expected warning set is not security approval. Unsupported transitive leaf patches are not authorized.

## Activation guardrails

Until an explicit decision is recorded in the relevant control issue:

- public-testnet launch is not authorized;
- `public_testnet_ready=false`;
- `thirty_day_public_testnet_clock_started=false`;
- default high cadence is not authorized;
- smart contracts remain disabled;
- no Day 0 timestamp may be recorded.

Smart-contract activation remains separately gated after at least 30 accepted public-testnet days and separate approval.

## Public network package

Files under `configs/public-testnet/` are pre-GO templates only. They intentionally contain `__TASK31_FREEZE_REQUIRED__` placeholders and keep P2P/public readiness disabled. They are not final bootnode, DNS, TLS, chain/network or RPC deployment values.

Final seed/node/observer/miner configs must be rendered only after the exact release SHA, chain/network identity, genesis/config digests, bootnode peer IDs/multiaddrs, public endpoint ownership and artifact digests are frozen in #781/#794.

## Exact-SHA evidence rule

Task30/Task31 evidence is valid only for the source SHA and activation contract it actually tested. Any source, dependency, config, release-surface or workflow change that produces a new release candidate requires the affected exact-SHA gates to rerun. Evidence from different SHAs must not be combined to claim final readiness.

The 24-hour private burn-in clock starts only after one unchanged final candidate is intentionally launched and its valid start evidence is recorded. Historical or superseded burn-ins do not count.

## Operator and infrastructure boundary

Repository templates do not provision or prove real public infrastructure. Before public GO, operators must separately record and verify failure-domain separation, persistent P2P identities, firewall policy, NTP/time sync, storage/backup, DNS/TLS ownership, observability, incident escalation and recovery procedures.

See `SECURITY.md`, `docs/runbooks/V2_4_0_PUBLIC_TESTNET_PREP.md`, #781, #794, #803 and #873.
