# PulseDAG v2.4.0 release evidence policy

This document defines the evidence required to publish PulseDAG v2.4.0 and the additional evidence required for a later public-testnet launch. Software release and network launch are separate decisions.

## Version contract

- Repository `VERSION`: `v2.4.0`.
- Cargo workspace version: `2.4.0`.
- Release decision: `APPROVE_TAG_AND_PUBLICATION` once the final exact-SHA software-release gates pass.
- Authoritative release SHA: the immutable commit referenced by tag `v2.4.0`.
- `public_testnet_ready=false`.
- `thirty_day_public_testnet_clock_started=false`.
- `contracts_enabled=false`.

The pre-version-bump implementation baseline `8a1a5f74e03eae695e76bf8a84ddc9d48f94db34` remains prior provenance only; it is not the final published release identity.

## Exact-SHA rule

All evidence used to publish v2.4.0 must bind to one unchanged source SHA. Evidence from different candidates must not be combined to manufacture a pass. The Git tag and release artifacts must resolve to that same final SHA.

## Required software-release evidence

Before tag/publication, the final v2.4.0 candidate must have, at minimum:

- repository hygiene and active-version-surface audit;
- locked Cargo metadata/check and complete workspace tests;
- Clippy with warnings denied;
- P2P real-swarm initialization and sync/recovery regressions required by the release matrix;
- RPC and release validation;
- v2.4 private chain-identity validation where invoked by the release matrix;
- public-safe RPC/profile contract validation;
- dependency/RustSec audit for the committed lockfile;
- wallet security and transaction-plan validation when wallet code is part of the release;
- release node and standalone-miner builds;
- startup and external-miner smoke evidence;
- packaged-artifact verification, checksums and provenance before publication.

## Release artifacts

The release workflow must create separate archives for `pulsedagd` and `pulsedag-miner` for the supported native targets. Each published archive requires:

- exact source commit identity;
- target triple and binary identity;
- SHA-256 checksum;
- build manifest/provenance data;
- successful native unpack/smoke verification;
- consolidated checksum/provenance verification before publication.

If an official end-user `pulsedag-wallet` binary is distributed as part of v2.4.0, its release artifact, checksum, provenance, restore/sign/broadcast smoke matrix and custody limitations must be recorded explicitly rather than inferred from node/miner evidence.

## Additional public-testnet evidence

A published software release is not sufficient for public-testnet GO. Public launch additionally requires the operational programme to record and accept:

- private burn-in evidence on the chosen launch SHA;
- restart, snapshot/export, compact prune, restore and real-P2P rejoin drills;
- the mandatory 5-node/4-miner launch rehearsal;
- final chain/network/genesis/configuration identity and digests;
- at least two bootnodes in independent failure domains;
- public-safe RPC limits, CORS, TLS/DNS/reverse-proxy and firewall ownership;
- persistent P2P identities, storage, backup/snapshot, NTP and recovery policy;
- dashboards, alerts, incident export and operator/on-call ownership;
- no unresolved Sev-1 consensus, storage, replay, sync, mining, security or operator-safety defect;
- required dependency-security disposition for public GO.

## Decision boundary

Publishing `v2.4.0` does not set launch state. `GO_PUBLIC_TESTNET`, Day 0 and the 30-day public-testnet clock are separate explicit launch-control decisions and begin only from the actual recorded public launch.

No release or evidence document may imply smart-contract activation or production/mainnet custody readiness.

## Historical evidence

v2.3.0 and earlier release evidence remains valid historical provenance for the exact versions and SHAs to which it was originally bound. Historical filenames and immutable evidence references are not active v2.4.0 version claims.
