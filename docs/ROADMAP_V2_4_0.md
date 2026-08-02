# ROADMAP v2.4.0 — Operator Modes and Runtime Resilience

Date: 2026-08-01 UTC

## Starting point

The v2.3.0 private-testnet line established repeatable multi-host bootstrap, lifecycle tooling, observability, incident runbooks, and protected rehearsal evidence.

A real Windows + Docker Desktop single-node burn-in then exposed five release-relevant gaps:

1. intentional single-node mining is blocked by the zero-peer mining-template guard when real P2P is enabled;
2. the versioned metrics inventory can drift from the registered RPC route surface, degrading exporter health;
3. sustained high-cadence mining can make liveness endpoints serve stale cached snapshots when the chain read lock remains busy;
4. mining submissions can time out at the RPC boundary after the same block has already been accepted;
5. consensus difficulty can remain permanently trapped at `1`, making miner delay—not consensus—the effective block clock.

Issue `#783` records the first two gaps. Issue `#786` records the consensus retarget defect. The lock-contention and late-submit behavior were reproduced during the same 24-hour burn-in.

v2.4.0 is focused on explicit operator modes, route-contract enforcement, control-plane resilience, submission finality, and correct target-based difficulty regulation. It is not a smart-contract release and it does not authorize public-testnet launch.

## Guardrails

- `VERSION` and Cargo remain at the currently approved release until a separate v2.4.0 release decision authorizes a bump.
- No `v2.4.0` tag or release artifact may be published from this roadmap branch without explicit maintainer approval.
- `public_testnet_ready=false` remains mandatory.
- The 30-day public-testnet clock remains not started and must not be backdated.
- Multi-node private-testnet safety remains fail-closed by default.
- Any single-node mode must be explicit, validated, and impossible to enable accidentally through an empty bootnode list alone.
- Consensus retarget parameters are fixed by network/version and cannot be changed by operator environment variables.
- Mining remains an external application; no embedded pool logic is introduced.
- Smart contracts remain disabled and out of scope.
- Code comments, developer documentation, commits, and pull-request descriptions remain English-only.
- Credentials, private keys, local runtime state, generated burn-in output, and operator-specific configuration must not be committed.

## Active v2.4.0 work sequence

### Task 14 — Explicit single-node operator profile

Status: **ACTIVE** in issue `#784` on branch `feature/2.4.0-single-node-profile`.

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

### Task 17 — Liveness and submission finality under sustained mining load

Status: **PLANNED**.

Harden `/health`, `/status`, `/p2p/status`, exporter collection, and `/mining/submit` against chain-lock contention and delayed actor completion.

Required investigation and changes:

- measure chain read/write lock hold time during template creation, block acceptance, persistence, snapshotting, and pruning;
- keep bounded liveness endpoints responsive without reporting misleading zeroed fields;
- separate immutable or atomically published status data from expensive chain reads where practical;
- define stale and degraded thresholds per endpoint;
- ensure cached snapshots retain coherent generation, height, tip, persistence, and contract-state fields;
- expose lock-contention, snapshot-age, submit-queue, and post-accept latency metrics;
- distinguish timeout before acceptance from timeout with unknown finality;
- make miners reconcile an unknown submit by block hash before recording a rejection;
- add stress coverage with sustained external mining and concurrent Prometheus scrapes.

Minimum acceptance target:

- no liveness timeout or stale-snapshot degradation during a bounded 60-second-cadence burn-in;
- no accepted block counted as a definitive miner rejection;
- bounded, explicitly characterized behavior under intentionally extreme cadence;
- mining and persistence continue without accepted-state loss, orphan growth, or storage repair.

### Task 18 — Canonical target retarget and mining cadence safety

Status: **ACTIVE — CONSENSUS RELEASE BLOCKER**. Tracks issue `#786`.

Replace integer-difficulty adjustment with deterministic canonical 256-bit target adjustment.

Required consensus behavior:

- target interval fixed at 60 seconds;
- observation window taken only from the selected chain;
- genesis height/timestamp excluded from interval signal;
- compact target bits used as the canonical header representation;
- a fixed v2.4.0 PoW limit defines the easiest allowed target;
- fast blocks reduce target and increase work;
- slow blocks increase target without exceeding the PoW limit;
- the easiest allowed target can harden immediately and cannot become an absorbing state;
- fixed-width deterministic arithmetic with no floating point;
- block template, `/pow`, and validation consume the same consensus snapshot;
- environment variables cannot change consensus retarget parameters.

Operator behavior:

- document the difference between consensus target interval, miner polling delay, and observed block cadence;
- publish current/suggested bits, target hex, PoW limit, selected-chain interval, work multiplier, target multiplier, and clamp diagnostics;
- provide safe polling/backoff defaults without using miner sleep as a block clock;
- retain bounded high-cadence stress tests behind an explicit non-consensus test profile.

Required tests:

- legacy `difficulty=1` history transitions to a harder compact target under fast blocks;
- easiest target + one-second blocks -> strictly harder next target;
- stable 60-second history -> unchanged bits;
- slow history -> easier target, clamped to PoW limit;
- side branches do not affect the selected-chain calculation;
- genesis timestamp zero does not contaminate early signal;
- snapshot, restart, pruning, and multi-node replay produce identical next bits;
- template, metrics, miner, and validator agree byte-for-byte.

Compatibility:

- this is an explicit consensus-rule change;
- the preferred private-testnet rollout is a clean v2.4.0 chain restart;
- preserving an old chain requires a separately reviewed activation height;
- no mixed-version implicit transition is allowed.

Design and implementation contract: [`DIFFICULTY_RETARGET_V2_4_0.md`](DIFFICULTY_RETARGET_V2_4_0.md).

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

Status: **BLOCKED ON TASKS 14–19**.

Run the exact v2.4.0 candidate through both isolated and networked scenarios.

Required matrix:

- single-node 24-hour burn-in with consensus-regulated cadence;
- bounded high-cadence contention test;
- restart and snapshot recovery with the same wallet and node identity;
- exporter and dashboard continuity across restart;
- transition from explicit single-node profile to ordinary private multi-node profile;
- existing five-node private-testnet bootstrap and fault-recovery regression;
- selected-chain target agreement across every node;
- zero accepted-state loss, zero unresolved storage inconsistency, and no silent route-contract drift.

Evidence must identify the exact commit, workflow run, artifact checksums, configuration profile, block range, accepted/unknown/rejected submission counts, target/bits history, snapshot ages, and final consistency state.

### Task 21 — Version and release decision

Status: **BLOCKED ON TASKS 14–20**.

Prepare a separate release-decision proposal only after every required implementation, CI, compatibility, and burn-in gate passes on the same candidate.

The proposal must confirm:

- issues `#783` and `#786` are resolved or explicitly dispositioned;
- single-node mode is explicit and documented;
- ordinary multi-node isolation safety remains intact;
- exporter route contracts are CI-enforced;
- accepted mining submissions cannot be misreported as definitive rejections;
- liveness remains bounded under the accepted operating envelope;
- selected-chain retarget converges without miner-delay cadence control;
- no severity-1 consensus, storage, sync, security, or key-management blocker remains;
- rollback compatibility and operator migration are documented;
- public-testnet readiness and the 30-day clock remain unchanged unless separately authorized.

Only after explicit approval may a follow-up candidate update `VERSION` and Cargo to `2.4.0`, generate release notes and artifacts, and create the release tag.

## Completion criteria

v2.4.0 is complete only when Tasks 14–21 are merged or formally dispositioned, their required tests and evidence pass on the exact release candidate, the existing v2.3.0 multi-host guarantees remain green, issue `#786` is closed with deterministic agreement evidence, no unresolved severity-1 blocker exists, and the final private-testnet release decision is recorded.

## Out of scope

- public-testnet launch;
- starting or backdating the 30-day public-testnet clock;
- smart-contract activation;
- GPU mining enablement without a canonical verified kernel;
- pool protocols or embedded pool services;
- replacing kHeavyHash;
- weakening ordinary-node isolation safeguards for operator convenience.
