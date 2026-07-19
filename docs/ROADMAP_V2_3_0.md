# ROADMAP v2.3.0 — Private Testnet Operations

Date: 2026-07-19 UTC

## Starting point

The v2.2.20 hardening closeout recorded `GO_TO_START_V2_3_0_REVIEW` after workspace, staged 5N/1M→5N/2M→5N/4M, mempool/relay, selected-segment lag recovery, and prune/restart/rejoin all passed on the same functional candidate.

This roadmap activates the remaining operational and repository-quality work required for a repeatable multi-host private testnet. It does not authorize a public testnet, a release tag, or a version bump.

## Guardrails

- `VERSION` and Cargo remain `2.2.20` until a separate final approval.
- `public_testnet_ready=false` remains mandatory.
- The 30-day public-testnet clock remains not started.
- Real networking uses `libp2p-real`.
- Mining remains an external application; no embedded pool logic.
- Smart contracts remain out of scope.
- Code comments, developer documentation, commit messages, and pull-request descriptions are written in English.
- Generated output, credentials, local runtime state, and unclassified historical material must not be committed to active repository paths.

## Completed foundations

- Fully green workspace and bounded CI execution.
- Bounded deterministic mempool and real transaction relay.
- Multi-page selected-segment lag recovery.
- Non-zero pruning, snapshot+delta restart, and offline rejoin.
- Same-candidate staged and runtime closeout evidence.

## Active v2.3.0 PR sequence

### Task 07 — Private-testnet bootstrap contract

Status: **MERGED** in PR `#756`.

Define role-based seed/node configuration templates and a fail-closed preflight for chain identity, persistent node identity, real P2P, bootnodes, RPC exposure, pruning, and readiness guardrails.

### Task 08 — Repository professionalization and developer standards

Status: **MERGED** in PR `#757`.

Establish a clean repository structure, contribution guide, English-only code-comment policy, editor defaults, pull-request template, generated-file and secret checks, broken-link validation, cleanup candidate inventory, and a dedicated repository-hygiene CI gate. Historical evidence may only be moved or removed after explicit classification.

### Task 09 — Node lifecycle and bootstrap scripts

Status: **ACTIVE**.

Add idempotent start/stop/status/upgrade/rollback helpers, persistent directory ownership checks, PID/log handling, bootnode resolution, immutable release manifests, health-gated upgrades, automatic failed-upgrade rollback, and safe restart semantics.

### Task 10 — Metrics and dashboard baseline

Status: **PLANNED**.

Publish a versioned metrics inventory, Prometheus scrape example, dashboard definitions, alert thresholds, and evidence that all five private-testnet nodes expose the required health, P2P, sync, mining, mempool, snapshot, and prune signals.

### Task 11 — Operator and incident runbooks

Status: **PLANNED**.

Document bootstrap, miner attachment, backup/restore, partition recovery, high orphan/missing-parent response, disk pressure, RPC abuse, identity rotation, rollback, evidence collection, and incident severity/ownership.

### Task 12 — Multi-host private-testnet rehearsal

Status: **PLANNED**.

Run the topology on separate hosts or isolated network namespaces, prove stable discovery and convergence, exercise restart/partition/rejoin, record checksummed evidence, and produce an explicit private-testnet GO/NO-GO decision.

### Task 13 — Version and release decision

Status: **BLOCKED** pending Tasks 07–12 and separate maintainer approval.

Only after Tasks 07–12 pass and a maintainer explicitly approves the release proposal: update `VERSION`/Cargo to `2.3.0`, generate release notes and artifacts, rerun all candidate gates, and record a release decision. This task does not authorize public-testnet launch.

## Completion criteria

v2.3.0 is complete only when Tasks 07–13 are merged, their required checks and operational evidence pass, repository hygiene is green, no unresolved severity-1 consensus/sync/storage/security blocker exists, and the final private-testnet release decision is recorded.
