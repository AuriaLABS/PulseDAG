# v2.3.0 Task 06: close v2.2.20 and gate the version bump

Date: 2026-07-19 UTC

## Purpose

Separate the `v2.2.20` hardening closeout from the later `2.3.0` version bump so that evidence approval cannot be mistaken for release or public-testnet authorization.

## Phase A — v2.2.20 closeout

Status: COMPLETE

Decision: `GO_TO_START_V2_3_0_REVIEW`

Evaluated candidate: `e65c6c199e07214303b49f7863f5b4988a8ce107`

Final Actions run: `29662737906`

Phase A completed with PASS results for:

- workspace format/check/tests/clippy;
- staged `5N/1M`, `5N/2M` and `5N/4M`;
- mempool and transaction relay;
- selected-segment lag recovery and retained-set convergence;
- prune/restart/rejoin and restore confidence;
- incident and waiver review.

Decision records:

- `docs/CLOSING_CHECKLIST_V2_2_20.md`
- `docs/V2_2_20_FINAL_EVIDENCE_INDEX.md`
- `docs/KNOWN_LIMITATIONS_V2_2_20.md`
- `artifacts/v2_2_20/closeout_decision/final_decision.md`
- `artifacts/v2_2_20/closeout_decision/v2_3_0_start_decision.md`
- `artifacts/v2_2_20/closeout_decision/incident_waiver_ledger.md`

## Phase B — 2.3.0 version bump

Status: BLOCKED PENDING EXPLICIT MAINTAINER APPROVAL

Phase A authorizes review only. Before changing version metadata, the maintainer must explicitly approve the bump after reviewing:

1. formal `v2.3.0` scope;
2. release-control and rollback plan;
3. security and public RPC posture;
4. public-testnet network and operations prerequisites;
5. remaining non-readiness limitations;
6. confirmation that no public launch or burn-in clock is implied by the bump.

Only after approval may a separate change update:

- `VERSION` from `v2.2.20` to `v2.3.0`;
- Cargo workspace/package versions;
- release metadata and version matrix references;
- any release notes required by the repository process.

## Persistent guardrails

- `VERSION=v2.2.20` remains required until Phase B approval.
- `public_testnet_ready=false` remains mandatory.
- `thirty_day_public_testnet_clock_started=false` remains mandatory.
- No public-testnet launch, readiness/live claim, release tag or artifact publication is authorized by Phase A.
- No smart-contract or embedded-pool enablement is part of this task.
