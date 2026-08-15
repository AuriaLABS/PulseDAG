# v2.4.0 private-testnet release closeout checklist

Use this checklist to bind the final v2.4.0 release decision to one exact versioned candidate. Completing this checklist does not by itself authorize a public-testnet launch.

## Status legend

- `[x]` verified from repository, workflow, or accepted evidence for the exact final candidate.
- `[ ]` not yet closed for the exact final candidate.
- Earlier candidate evidence may be retained as provenance but cannot replace required exact-SHA reruns after later source/configuration changes.

## Candidate identity

- [ ] Exact final versioned candidate SHA recorded.
- [ ] `VERSION=v2.4.0` verified on the exact candidate.
- [ ] Cargo workspace/local PulseDAG package versions `2.4.0` verified.
- [ ] `Cargo.lock` is synchronized with the workspace version and contains no unrelated dependency drift.
- [ ] Branch/head drift check confirms the validated SHA is still the intended release candidate.
- [ ] Candidate release decision/evidence record references this exact SHA only.

## Required exact-SHA CI and security evidence

- [ ] Repository hygiene and v2.4.0 active-version-surface audit pass.
- [ ] Locked Cargo metadata/check, compile-all-tests and every workspace package test pass.
- [ ] Workspace Clippy passes with warnings denied.
- [ ] P2P real-swarm initialization and current sync/recovery regressions pass.
- [ ] v2.4.0 private chain-identity gate passes.
- [ ] RPC and Release Validation passes.
- [ ] v2.4.0 public-testnet profile/route-isolation contract passes.
- [ ] Wallet transaction-plan validation passes.
- [ ] Wallet security validation passes.
- [ ] Dependency/RustSec audit passes for the committed lockfile.
- [ ] Required reachable unsound/unmaintained dependency disposition for public GO is complete or explicitly blocks launch.
- [ ] Pre-burn-in verification passes on the same exact candidate.

## Packaging and artifact evidence

- [ ] Release node and standalone miner build from the exact candidate.
- [ ] Linux x86_64 archive/checksum/manifest/provenance/native smoke pass.
- [ ] Windows x86_64 archive/checksum/manifest/provenance/native smoke pass.
- [ ] macOS x86_64 archive/checksum/manifest/provenance/native smoke pass if retained in the release workflow matrix.
- [ ] Consolidated `SHA256SUMS.txt`, `release-provenance.json` and `INSTALL-VERIFY.md` verify independently.
- [ ] Clean installation/startup/mining/restart/rollback from packaged artifacts is evidenced.
- [ ] If an official wallet binary is published, its checksum/provenance plus restore/sign/broadcast/recovery evidence is separately recorded.
- [ ] No artifact/evidence bundle contains credentials, wallet seeds, private keys or passwords.

## Private operational burn-in

- [ ] Actual UTC start timestamp recorded for the exact candidate; no backdating.
- [ ] Sanitized fixed configuration and digest recorded.
- [ ] New clean v2.4 database/private-network initialization recorded.
- [ ] Process/container inventory and binary/image digests recorded.
- [ ] 24 contiguous hours completed without an invalidating stop condition.
- [ ] Retarget movement/current-next bits/target evidence is coherent across node/miner surfaces.
- [ ] Mining submissions remain coherent; unknown finality is reconciled and not miscounted as rejection.
- [ ] Runtime health, resource, RPC and alert evidence remains bounded/understood.
- [ ] Planned node restart passes.
- [ ] Snapshot/export verification passes.
- [ ] Compact prune and post-prune restart/mining pass.
- [ ] Restore drill passes.
- [ ] Clean second node synchronizes over real P2P.
- [ ] Offline/rejoin drill converges automatically.
- [ ] Final height, selected tip, compact bits/target and canonical state digest converge.
- [ ] Final incident list and explicit PASS/FAIL decision are recorded.

## Mandatory 5-node / 4-miner private launch rehearsal

- [ ] Five real nodes and four external miners run on one unchanged release SHA.
- [ ] At least two independent hosts/regions/failure domains are used.
- [ ] Seed restart, ordinary-node restart, miner restart and miner disconnect/rejoin pass.
- [ ] Node isolation/rejoin passes.
- [ ] Snapshot/prune/restore and clean-node catch-up pass within the launch topology.
- [ ] Tip/height/bits/target/canonical state digest converge across nodes.
- [ ] Node/miner submit-finality telemetry reconciles.
- [ ] Full topology, UTC timeline, metrics/resources, perturbations, incidents and raw exports are attached.

## Public-safe infrastructure and operations

- [ ] At least two public bootnodes in independent failure domains are frozen with persistent peer IDs/full multiaddrs.
- [ ] Final chain ID, network profile, genesis hash and configuration digests are recorded.
- [ ] Public RPC/status endpoints, body limits, per-IP limits and CORS policy are recorded.
- [ ] TLS/DNS/reverse-proxy ownership and trusted client-IP boundary are recorded.
- [ ] Admin/operator control plane is not publicly exposed.
- [ ] Firewall ingress/egress and management access policy are verified.
- [ ] Storage sizing, backup/snapshot/retention and disaster-recovery procedures are verified.
- [ ] NTP/time synchronization monitoring is verified.
- [ ] Dashboard/metric inventory and launch alerts are frozen.
- [ ] Primary and backup operators are named.
- [ ] UTC on-call window and incident escalation/public status-update path are tested.
- [ ] Security reporting route is published/tested.
- [ ] Hard-stop, rollback and delayed-launch criteria are documented.

## Final release decision

Choose exactly one after all required **release** evidence is complete:

- [ ] `APPROVE_TAG_AND_PUBLICATION`
- [ ] `REQUEST_CHANGES`
- [ ] `NO_GO`

Record:

- maintainer/release owner and UTC decision time;
- exact candidate SHA;
- workflow run IDs and artifact digests;
- rationale and unresolved limitations;
- rollback conditions;
- explicit tag/publication authorization.

## Public-testnet boundary

Release/tag approval is not equivalent to public launch approval. Before Day 0:

- [ ] the authoritative launch-control record reviews the complete evidence;
- [ ] exactly one `GO_PUBLIC_TESTNET`, `DELAY_PUBLIC_TESTNET`, or `NO_GO_PUBLIC_TESTNET` decision is recorded;
- [ ] for GO, the exact release/genesis/network/configuration/bootnode identity is frozen;
- [ ] the public network is actually started and first accepted public block/height is recorded;
- [ ] only then is `public_testnet_ready=true` recorded;
- [ ] only then is `thirty_day_public_testnet_clock_started=true` recorded with the exact Day 0 UTC timestamp.

Smart contracts remain disabled until the required accepted public-testnet period is complete and a separate activation decision is recorded. Production/mainnet custody readiness is not implied by this checklist.

## Pre-bump provenance

Before the version-surface preparation, `release/2.4.0` was validated at SHA `8a1a5f74e03eae695e76bf8a84ddc9d48f94db34`. That evidence demonstrates the implementation baseline before the bump, but it is not the exact final versioned candidate after this PR is merged.

## Sign-off

- Maintainer: ____________________
- Release owner: _________________
- Operations owner: ______________
- Backup operator: _______________
- Decision date/time (UTC): __________________________
- Exact final candidate SHA: ___________________________________________
- Final release decision: ____________________________
- Public launch decision reference: __________________
- Blocking issue IDs, if any: __________________________________________
