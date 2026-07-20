# ROADMAP v2.3.0 — Private Testnet Operations

Date: 2026-07-20 UTC

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

Status: **MERGED** in PR `#758`.

Add idempotent start/stop/status/upgrade/rollback helpers, persistent directory ownership checks, PID/log handling, bootnode resolution, immutable release manifests, health-gated upgrades, automatic failed-upgrade rollback, and safe restart semantics.

### Task 10 — Metrics and dashboard baseline

Status: **MERGED** in PR `#759`.

Publish a versioned metrics inventory, Prometheus scrape example, dashboard definitions, alert thresholds, and evidence that all five private-testnet nodes expose the required health, P2P, sync, mining, mempool, snapshot, and prune signals.

### Task 11 — Operator and incident runbooks

Status: **MERGED** in PR `#761`.

Document bootstrap, miner attachment, backup/restore, partition recovery, high orphan/missing-parent response, disk pressure, RPC abuse, identity rotation, rollback, evidence collection, and incident severity/ownership.

### Task 12 — Multi-host private-testnet rehearsal

Status: **COMPLETE**. Tooling and runtime corrections merged through PRs `#762`, `#764`, `#766`, `#767`, and `#768`; the protected live workflow recorded an independently reviewed `GO`.

Accepted evidence:

- candidate `22fa09b19da2893fa73b91b198b26675bd1e6e32`;
- workflow run `29773225491`;
- artifact `v2_3_0_task12_netns_22fa09b19da2893fa73b91b198b26675bd1e6e32_29773225491_1`;
- artifact SHA-256 `a31246a014e88287e653b732c5edf54af08d26f5d0ffac19f60b49f369db88ce`;
- 56 unique controller checksum entries verified with no duplicates;
- all nine mandatory phases `PASS` with `failure=null`;
- `version_bump_authorized=false`, `public_testnet_ready=false`, and `thirty_day_public_testnet_clock_started=false` preserved.

The rehearsal proved five-node baseline convergence, external mining before and after faults, ordinary-node restart/rejoin, zero direct peers during bounded target isolation, exact restoration, final convergence, healthy endpoint surfaces, and zero replay or storage consistency gaps.

### Task 13 — Version and release decision

Status: **ACTIVE — RELEASE-DECISION PROPOSAL ONLY**.

Task 12 `GO` satisfies the operational prerequisite to prepare a separate v2.3.0 release-decision proposal. Proposal review must confirm the exact candidate scope, all required CI and artifact gates, release-note content, rollback compatibility, unresolved-severity review, and explicit maintainer authorization.

Until that separate authorization is recorded:

- do not update `VERSION` or Cargo from `2.2.20`;
- do not create a `v2.3.0` tag or publish release artifacts;
- do not claim public-testnet readiness;
- do not start or backdate the 30-day public-testnet clock;
- do not introduce or enable smart contracts.

Only after the proposal is explicitly approved may a follow-up candidate PR update `VERSION`/Cargo to `2.3.0`, generate release notes and artifacts, rerun every required gate on that exact candidate, and record the final private-testnet release decision. Task 13 does not authorize public-testnet launch.

## Completion criteria

v2.3.0 is complete only when Tasks 07–13 are merged, their required checks and operational evidence pass, repository hygiene is green, no unresolved severity-1 consensus/sync/storage/security blocker exists, and the final private-testnet release decision is recorded.
