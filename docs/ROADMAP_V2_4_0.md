# ROADMAP v2.4.0 — Operator Modes and Runtime Resilience

Date: 2026-07-31 UTC

## Starting point

The v2.3.0 private-testnet line established repeatable multi-host bootstrap, lifecycle tooling, observability, incident runbooks, and protected rehearsal evidence.

A real Windows + Docker Desktop single-node burn-in then exposed three operator-facing gaps:

1. intentional single-node mining is blocked by the zero-peer mining-template guard when real P2P is enabled;
2. the versioned metrics inventory can drift from the registered RPC route surface, degrading exporter health;
3. sustained high-cadence mining can make liveness endpoints serve stale cached snapshots when the chain read lock remains busy.

Issue `#783` records the first two gaps. The third was reproduced during the same burn-in after more than 2,100 accepted blocks with zero rejected submissions.

v2.4.0 is therefore focused on explicit operator modes, route-contract enforcement, and control-plane resilience under sustained block production. It is not a smart-contract release and it does not authorize public-testnet launch.

## Guardrails

- `VERSION` and Cargo remain at the currently approved release until a separate v2.4.0 release decision authorizes a bump.
- No `v2.4.0` tag or release artifact may be published from this roadmap branch without explicit maintainer approval.
- `public_testnet_ready=false` remains mandatory.
- The 30-day public-testnet clock remains not started and must not be backdated.
- Multi-node private-testnet safety remains fail-closed by default.
- Any single-node mode must be explicit, validated, and impossible to enable accidentally through an empty bootnode list alone.
- Mining remains an external application; no embedded pool logic is introduced.
- Smart contracts remain disabled and out of scope.
- Code comments, developer documentation, commits, and pull-request descriptions remain English-only.
- Credentials, private keys, local runtime state, generated burn-in output, and operator-specific configuration must not be committed.

## Active v2.4.0 work sequence

### Task 14 — Explicit single-node operator profile

Status: **PLANNED**.

Add a first-class single-node profile for local development, deterministic burn-in, and operator validation.

Required behavior:

- explicit configuration such as `PULSEDAG_SINGLE_NODE_MODE=true` or an equivalent typed profile;
- P2P disabled or isolated by policy, not by ad hoc operator patching;
- loopback-only RPC by default;
- no bootnode requirement;
- clear startup identity showing that the node is intentionally isolated;
- no public-testnet readiness claim;
- deterministic transition path back to the normal private multi-node profile.

The profile must fail validation if combined with contradictory public or multi-host settings.

### Task 15 — Topology-aware mining-template availability

Status: **PLANNED**. Tracks issue `#783`.

Split the current zero-peer mining guard into explicit topology semantics:

- ordinary private/testnet nodes continue to reject mining templates while unexpectedly isolated;
- an explicitly configured single-node profile may mine with `peer_count=0`;
- seed status alone must not silently bypass the guard;
- template errors must expose a stable reason code and actionable operator message;
- switching from single-node to multi-node operation must restore the normal isolation guard.

Required tests:

- single-node profile + zero peers -> template available;
- ordinary node + zero peers -> template unavailable;
- seed without explicit single-node profile + zero peers -> existing safety behavior preserved;
- degraded sync, missing-parent recovery, or orphan recovery -> template unavailable in every profile.

### Task 16 — RPC route and metrics-inventory contract

Status: **PLANNED**. Tracks issue `#783`.

Prevent route drift between the v2.4.0 RPC router and all versioned observability inventories.

Required changes:

- correct the mempool inventory endpoint to the registered route;
- add a machine-readable route manifest or equivalent test fixture;
- validate every exporter endpoint against the router in CI;
- validate that every required inventory field exists in the endpoint response contract;
- keep exporter `/metrics` available during partial collection failure while making `/health` diagnostics identify the failing endpoint;
- add a release gate that rejects stale or unregistered inventory paths.

Acceptance requires exporter `/health` to return HTTP 200 against a healthy node with the official v2.4.0 inventory.

### Task 17 — Liveness snapshots under sustained mining load

Status: **PLANNED**.

Harden `/health`, `/status`, `/p2p/status`, and exporter collection against chain-lock contention observed during high-cadence single-node mining.

Required investigation and changes:

- measure chain read-lock hold time during template creation, block acceptance, persistence, snapshotting, and pruning;
- keep bounded liveness endpoints responsive without reporting misleading zeroed fields;
- separate immutable or atomically published status data from expensive chain reads where practical;
- define stale and degraded thresholds per endpoint;
- ensure cached snapshots retain coherent generation, height, tip, persistence, and contract-state fields;
- expose lock-contention and snapshot-age metrics;
- add stress coverage with sustained external mining and concurrent Prometheus scrapes.

Minimum acceptance target:

- no liveness timeout or stale-snapshot degradation during a bounded 60-second-cadence burn-in;
- bounded, explicitly characterized behavior under intentionally extreme cadence;
- mining and persistence continue without accepted-state loss, orphan growth, or storage repair.

### Task 18 — Mining cadence and difficulty safety

Status: **PLANNED**.

Make operator intent explicit when block production differs materially from the configured target interval.

Required behavior:

- document the difference between consensus target interval, miner loop delay, and effective block cadence;
- warn when development difficulty and miner delay produce sustained over-cadence block production;
- publish effective block interval and template-to-submit latency metrics;
- provide safe reference defaults for single-node burn-in and multi-node rehearsal;
- preserve the ability to run bounded high-cadence stress tests behind an explicit experimental flag.

This task must not silently change consensus difficulty rules for existing networks.

### Task 19 — Reference operator packaging and recovery

Status: **PLANNED**.

Publish a maintained reference deployment for one-node local burn-in without making platform-specific packaging part of consensus.

Scope:

- Docker Compose reference stack for node, external CPU miner, exporter, Prometheus, and Grafana;
- offline wallet generation with the private key excluded from miner and node containers;
- PowerShell-compatible lifecycle scripts without PowerShell 7-only APIs;
- idempotent start, status, logs, stop, restart, and backup flows;
- loopback-only RPC and local-only P2P defaults;
- documented migration from single-node mode to a real multi-host private topology;
- no committed credentials, wallets, generated data, or host-specific addresses.

Reference packaging must consume official repository artifacts and configuration contracts rather than carrying permanent downstream patches.

### Task 20 — v2.4.0 burn-in and compatibility matrix

Status: **PLANNED**.

Run the exact v2.4.0 candidate through both isolated and networked scenarios.

Required matrix:

- single-node 24-hour burn-in at safe reference cadence;
- bounded high-cadence contention test;
- restart and snapshot recovery with the same wallet and node identity;
- exporter and dashboard continuity across restart;
- transition from explicit single-node profile to ordinary private multi-node profile;
- existing five-node private-testnet bootstrap and fault-recovery regression;
- zero accepted-state loss, zero unresolved storage inconsistency, and no silent route-contract drift.

Evidence must identify the exact commit, workflow run, artifact checksums, configuration profile, block range, accepted/rejected submission counts, snapshot ages, and final consistency state.

### Task 21 — Version and release decision

Status: **BLOCKED ON TASKS 14–20**.

Prepare a separate release-decision proposal only after every required implementation, CI, compatibility, and burn-in gate passes on the same candidate.

The proposal must confirm:

- issue `#783` is resolved or explicitly dispositioned;
- single-node mode is explicit and documented;
- ordinary multi-node isolation safety remains intact;
- exporter route contracts are CI-enforced;
- liveness remains bounded under the accepted operating envelope;
- no severity-1 consensus, storage, sync, security, or key-management blocker remains;
- rollback compatibility and operator migration are documented;
- public-testnet readiness and the 30-day clock remain unchanged unless separately authorized.

Only after explicit approval may a follow-up candidate update `VERSION` and Cargo to `2.4.0`, generate release notes and artifacts, and create the release tag.

## Completion criteria

v2.4.0 is complete only when Tasks 14–21 are merged or formally dispositioned, their required tests and evidence pass on the exact release candidate, the existing v2.3.0 multi-host guarantees remain green, no unresolved severity-1 blocker exists, and the final private-testnet release decision is recorded.

## Out of scope

- public-testnet launch;
- starting or backdating the 30-day public-testnet clock;
- smart-contract activation;
- GPU mining enablement without a canonical verified kernel;
- pool protocols or embedded pool services;
- consensus algorithm replacement;
- weakening ordinary-node isolation safeguards for operator convenience.
