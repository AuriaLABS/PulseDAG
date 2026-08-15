# v2.4.0 public-testnet completion program

## Status

Implementation and evidence program. This document does not authorize a public launch, version tag, Day 0, the 30-day public-testnet clock, smart contracts, high cadence or GHOSTDAG activation.

Launch control remains issue `#781`. Program tracking remains issue `#794`.

## Objective

Complete the v2.4.0 code, release, security, infrastructure and operational work required to make a public-testnet GO decision reviewable and reproducible.

The program deliberately separates work that can be completed in the repository from evidence that can only be produced on operator infrastructure.

## Non-negotiable sequence

1. Resolve all blocking code and observability defects.
2. Run the complete repository validation matrix on one exact SHA.
3. Freeze a replacement private candidate.
4. Complete the private 24-hour burn-in, restart, snapshot, pruning, restore and real-P2P rejoin gate in `#789`.
5. Complete a five-node/four-miner private launch rehearsal across independent failure domains.
6. Freeze release identity, genesis, chain configuration, public bootnodes and artifact digests.
7. Review evidence in `#781` and select exactly one of `GO_PUBLIC_TESTNET`, `DELAY_PUBLIC_TESTNET` or `NO_GO_PUBLIC_TESTNET`.
8. Only after GO, launch publicly and record Day 0.

Public infrastructure may be prepared earlier, but it must not be advertised as the official network and must not start the public-testnet clock.

## Phase 1 — blocking correctness and observability

Tracking: `#793`.

Required outcomes:

- `/mining/submit` never reports an ambiguous actor timeout as a definitive rejection;
- post-enqueue timeout returns explicit `submit_finality_unknown` semantics;
- pre-acceptance chain-lock timeout remains definitively classified;
- the standalone miner reconciles unknown finality by block hash with bounded retries;
- node and miner telemetry separate accepted, definitively rejected, reconciled and unresolved submissions;
- `ParentStateContextUnavailable` increments its dedicated metric;
- regression coverage proves delayed acceptance, reconciliation and counter coherence.

Exit gate:

- implementation merged into `release/2.4.0`;
- format, Clippy, workspace tests, miner/node contract and packaged smokes pass;
- no unresolved P1/P2 review finding on the changed surfaces.

## Phase 2 — roadmap, runbook and active-version reconciliation

Tracking: `#792`.

Required outcomes:

- the v2.4.0 roadmap reflects actual Task 14–18 state;
- the single-node runbook reflects the implemented topology-aware mining contract;
- active documentation distinguishes private candidate, private release, public-testnet eligibility and public launch;
- historical v2.3.0 documents remain historical rather than silently becoming active v2.4.0 instructions;
- no public readiness claim is introduced.

Exit gate:

- documentation validation and repository-hygiene checks pass;
- every active operational command is executable against the current candidate.

## Phase 3 — replacement pre-burn-in candidate

Required outcomes:

- one exact `release/2.4.0` SHA is selected;
- complete workspace and release validation passes on that exact SHA;
- evidence artifact name, ID and SHA-256 digest are recorded;
- branch comparison proves no drift after validation;
- sanitized single-node configuration and persistent storage path are prepared;
- `#789` is updated with the replacement candidate.

Exit gate:

- no source, consensus or configuration change after the candidate is frozen;
- the operational UTC start is not recorded until processes actually start.

## Phase 4 — private 24-hour burn-in and recovery

Tracking: `#789`.

Required topology:

- one isolated v2.4.0 node and external miner for baseline;
- planned restart;
- snapshot/export verification;
- compact prune and post-prune restart;
- restore drill;
- at least one independent second node joining from a clean database;
- offline/rejoin drill through real P2P.

Required evidence:

- continuous height progress and target movement;
- coherent current/next compact bits and target values;
- no ambiguous submit accounting;
- bounded memory, disk and RPC latency;
- fresh liveness surfaces under mining load;
- identical selected tip, height, bits, target and canonical state digest after sync/rejoin;
- no lost accepted block or storage/memory mismatch.

Exit gate:

- explicit `PASS` in `#789` on one unchanged SHA;
- any failure becomes a tracked defect and invalidates the candidate clock.

## Phase 5 — public-safe release and security baseline

Required repository deliverables:

- `SECURITY.md` with a private reporting route, supported versions and severity expectations;
- pinned dependency-vulnerability scanning using RustSec/cargo-audit or equivalent;
- workflow least-privilege review;
- explicit `public_safe` configuration examples;
- admin/operator routes disabled on public RPC;
- public RPC request-body limit, per-IP rate limit and CORS contract;
- credential and secret scanning for configs/evidence;
- testnet-funds and non-production-custody disclaimer;
- release notes and known limitations.

Required release identity:

- `VERSION` and Cargo package surfaces consistently identify v2.4.0 only after approval;
- chain ID, network profile, genesis hash and consensus constants are frozen;
- Linux and Windows node/miner artifacts are generated;
- checksums, binary versions and build provenance are recorded;
- clean install, startup, mining, restart, upgrade and rollback smokes pass.

Exit gate:

- reproducible release bundle validated without unpublished local patches or credentials.

## Phase 6 — public network package

Required roles:

- at least two seed/bootnodes in independent failure domains;
- ordinary full nodes;
- observer/evidence collector;
- external miners;
- primary and backup launch operators.

Required configuration:

- persistent P2P identity for every node;
- final `/p2p/<peer-id>` bootnode multiaddrs;
- DNS/TLS ownership where used;
- firewall and management-access policy;
- RocksDB sizing, snapshot, backup and retention policy;
- NTP/time-sync monitoring;
- restart, node replacement and disaster-recovery runbooks;
- public status and incident-reporting endpoints.

Exit gate:

- every launch role can be rebuilt from the frozen artifacts and sanitized configuration bundle.

## Phase 7 — five-node/four-miner private launch rehearsal

Minimum requirements:

- 24 contiguous hours on one exact release SHA;
- five real node processes and four external miners;
- at least two hosts, regions or independent failure domains;
- seed restart;
- ordinary-node restart;
- miner restart;
- miner disconnect/rejoin;
- node isolation/rejoin;
- snapshot export/verify, compact prune, restore and clean-node catch-up;
- coherent submit-finality reconciliation;
- convergence of selected tip, height, bits, target and state digest.

Evidence bundle:

- topology and role inventory;
- binary/image/configuration digests;
- UTC timeline;
- beginning, perturbation and final status snapshots;
- node/miner logs and exported metrics;
- resource and latency summary;
- incident list, including explicit `none` when applicable;
- GO/NO-GO table against every acceptance criterion.

Exit gate:

- zero unresolved Sev-1 defect;
- every unexplained rejection, restart, divergence or stale control-plane event resolved or converted into a blocker.

## Phase 8 — launch decision and Day 0

Tracking: `#781`.

Before GO, record:

- exact v2.4.0 source SHA;
- artifact and image digests;
- genesis hash, chain ID and network profile;
- final bootnodes and public endpoints;
- primary/backup operators and first-24-hour on-call window;
- evidence links and incident summary;
- rollback/hard-stop criteria.

After `GO_PUBLIC_TESTNET` only:

1. start seeds, ordinary nodes, observer and miners;
2. verify mesh, convergence, mining and public status;
3. record first accepted public block and exact UTC timestamp;
4. set `public_testnet_ready=true`;
5. set `thirty_day_public_testnet_clock_started=true`;
6. record Day 0.

## Explicit exclusions

This program does not include:

- smart-contract execution or contract RPCs;
- high-cadence activation;
- production key custody;
- pool payout/accounting logic;
- claims of Kaspa wire, state or consensus compatibility;
- mainnet readiness.

## Completion definition

The program is complete only when `#794` has all implementation and rehearsal gates checked and `#781` contains a final decision. Public launch completion and Day 0 remain recorded exclusively in `#781`.
