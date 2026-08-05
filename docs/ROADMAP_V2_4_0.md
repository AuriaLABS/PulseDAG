# ROADMAP v2.4.0 — Public-testnet candidate completion

Date: 2026-08-03 UTC

## Purpose

v2.4.0 completes the node, miner, storage, observability, security and operator work required to make PulseDAG eligible for a public-testnet GO decision.

It is not itself a launch authorization. Until issue #781 records an explicit `GO_PUBLIC_TESTNET` decision and the public network actually starts:

- `public_testnet_ready=false`;
- `thirty_day_public_testnet_clock_started=false`;
- `contracts_enabled=false`;
- Day 0 is not defined.

The 30-day public-testnet clock remains not started. Smart contracts remain disabled.

The 24-hour private burn-in in #789 must run only after all intended repository changes are merged and one exact replacement candidate SHA is frozen. The original candidate `d6bdb86bec42ec36514c0a41748cdabc413a5b79` was invalidated before operational start and must not be reused.

## Guardrails

- `VERSION` and Cargo package versions remain at the currently approved value until the final v2.4.0 release decision.
- No v2.4.0 tag, release artifact or public endpoint is authorized by this roadmap.
- Evidence from different source SHAs must never be combined.
- Multi-node safety remains fail-closed by default.
- Single-node mining requires the explicit validated operator profile.
- Consensus parameters are version/network constants and are not operator environment settings.
- The private v2.4.0 burn-in identity is `private-testnet-v2.4.0` / `pulsedag-private-v2.4.0`; v2.3.0 identities and databases must not be reused.
- The operator contract is [`V2_4_0_PRIVATE_BURN_IN_OPERATOR_PROFILE.md`](V2_4_0_PRIVATE_BURN_IN_OPERATOR_PROFILE.md).
- Mining remains an external application.
- High-cadence/GHOSTDAG activation and smart contracts remain out of scope.
- Credentials, keys, wallet seeds, tokens, runtime databases and unrestricted endpoints must not be committed or attached to evidence.

## Completed implementation

### Task 14 — Explicit single-node operator profile

Status: **COMPLETE**. Issue #784 is closed.

The profile is explicit and fail-closed. It requires intentional single-node configuration, P2P isolation, no bootnodes, loopback RPC and persistent storage. Empty bootnodes or seed role alone do not activate isolated mining.

### Task 15 — Topology-aware mining-template availability

Status: **COMPLETE**. The corresponding portion of #783 is closed.

- explicit single-node profile + zero peers: mining templates may be issued;
- ordinary node + unexpected zero peers: mining remains unavailable;
- seed role alone does not bypass isolation safety;
- degraded sync, missing-parent recovery and orphan-recovery gates remain authoritative.

### Task 16 — RPC route and metrics-inventory contract

Status: **COMPLETE**. Issue #783 is closed; route alignment was finalized in PR #791.

The official mempool inventory uses the registered `/mempool` route. CI enforces route/inventory consistency and exporter health behavior.

### Task 17 — Liveness and mining-submit finality

Status: **IMPLEMENTED; FINAL CANDIDATE VALIDATION PENDING**.

Liveness endpoints use cached bounded snapshots rather than waiting indefinitely for chain/runtime/storage locks. Backlog tests verify bounded response and no accumulating in-flight handlers.

PR #797 defines the submit-finality contract:

- `submit_timeout_before_acceptance` is definitive non-acceptance;
- `submit_finality_unknown` means the serialized actor may still complete;
- the miner reconciles the submitted block by hash;
- unresolved unknown finality is never counted as a definitive rejection;
- the same block hash is not blindly resubmitted;
- parent-state-context-unavailable evidence has a dedicated metric.

See [`V2_4_0_MINING_SUBMIT_FINALITY.md`](V2_4_0_MINING_SUBMIT_FINALITY.md).

Issue #793 remains open until the final post-security release SHA passes the complete matrix and is recorded as the replacement #789 candidate.

### Task 18 — Canonical target retarget

Status: **COMPLETE IN CODE; PRIVATE EVIDENCE PENDING**. Issue #786 is closed.

The implementation uses deterministic canonical compact target bits, fixed network parameters, selected-chain interval history, a 60-second target interval, a 20-block window, an 8 percent deadband, damping and bounded adjustment. Template, validation and PoW diagnostics consume the same consensus snapshot.

The private burn-in must still demonstrate real hardening/relaxation, restart parity, pruning parity and byte-identical multi-node next-target calculations.

### Task 19 — Storage, pruning and parent-context safety

Status: **IMPLEMENTED; PRIVATE RECOVERY EVIDENCE PENDING**.

Snapshot export/verification, compact pruning, startup reconciliation, retained-set reporting and restore tooling are present. Issue #788 is closed: unavailable historical side-parent state now fails explicitly as `ParentStateContextUnavailable` rather than being mislabeled as a true invalid state root.

Exact historical side-parent reconstruction after compact pruning remains a documented limitation until a checkpoint/undo/historical-overlay design is implemented.

## Active completion work

### Task 20 — Dependency and public-safe security baseline

Status: **ACTIVE**. Tracks #796 and #798.

Required before public-testnet GO:

- `SECURITY.md` and a confidential reporting path;
- testnet-funds and non-production-custody disclaimer;
- pinned dependency vulnerability audit;
- zero unresolved reachable vulnerabilities in the frozen release graph;
- documented, version-specific reachability analysis for any temporary exception;
- least-privilege workflow review;
- admin/operator control plane excluded from public exposure;
- fixed public request limits, per-IP limits and CORS policy.

The first audit correctly identified lockfile vulnerabilities. They must be remediated or dispositioned with evidence; the audit gate must not be weakened to obtain a green result.

### Task 21 — Documentation, release identity and artifacts

Status: **ACTIVE**. Tracks #792 and #794.

Before candidate freeze:

- reconcile roadmaps and runbooks;
- define seed, node, observer and miner configuration examples;
- define upgrade and rollback procedures;
- define supported public API and known limitations;
- leave final SHA, genesis, chain ID, bootnodes, artifact digests and operator timestamps as `TBD`.

After all code/security changes and private evidence pass:

- update `VERSION` and workspace/package versions consistently to 2.4.0;
- freeze genesis, network ID and consensus constants;
- build Linux and Windows node/miner artifacts;
- record SHA-256 digests for source, binaries, archives, images, genesis and configuration;
- validate install/start/mining/restart/rollback from packaged artifacts;
- publish release notes and known limitations.

### Task 22 — Final private candidate and 24-hour burn-in

Status: **BLOCKED**. Controlled by #789.

Freeze one exact SHA only after Tasks 20 and 21 stop advancing the branch. Then run:

- clean single-node baseline with external CPU miner;
- 24 contiguous hours on the unchanged candidate;
- real target movement and coherent node/miner submit telemetry;
- controlled restart;
- snapshot export and verification;
- compact prune and post-prune restart;
- restore drill;
- second clean node sync through real P2P;
- offline/rejoin and consensus-value comparison.

Any code/config consensus change, unexplained restart or invalidating database reset restarts the clock.

### Task 23 — Five-node/four-miner launch rehearsal

Status: **BLOCKED ON #789**. Tracks #794.

Use five real node processes and four external miners across at least two independent failure domains. Complete seed restart, ordinary-node restart, miner restart, disconnect/rejoin, node isolation/rejoin, snapshot/restore and clean-node catch-up. Prove convergence of selected tip, height, compact bits, target and canonical state digest.

### Task 24 — Public-testnet decision and launch

Status: **BLOCKED**. Controlled exclusively by #781.

After all implementation, security, private burn-in, rehearsal, infrastructure and operator evidence is complete, record exactly one decision:

- `GO_PUBLIC_TESTNET`;
- `DELAY_PUBLIC_TESTNET`;
- `NO_GO_PUBLIC_TESTNET`.

Only after GO may operators expose the official public network, record the first accepted public block and start Day 0. Smart contracts remain blocked until at least 30 accepted public-testnet days complete and a separate approval is recorded.

## Final v2.4.0 completion criteria

v2.4.0 is eligible for public-testnet GO only when:

- all blocking code and security defects are closed;
- dependency audit and workflow security gates pass on the exact release SHA;
- #789 records a private PASS with complete evidence;
- the five-node/four-miner rehearsal passes;
- public infrastructure, monitoring, incident response and backup ownership are recorded;
- release identity and artifacts are reproducible and checksummed;
- no unresolved Sev-1 consensus, storage, replay, sync, mining, security or operator-safety issue remains;
- #781 records the final decision.

## Out of scope

- smart-contract activation;
- high-cadence/full-GHOSTDAG production activation;
- mainnet or production custody readiness;
- pool protocols or embedded pool services;
- enabling an unverified GPU mining kernel;
- weakening ordinary-node isolation, finality or storage safeguards for convenience.
