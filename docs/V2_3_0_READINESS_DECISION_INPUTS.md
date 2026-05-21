# v2.3.0 Readiness Decision Inputs (from v2.2.18 RC closeout)

> This document is a **decision-review input**. It is not a standalone readiness declaration.

## Decision context
- Source milestone: `v2.2.18` private-testnet RC closeout.
- Decision target: whether to open formal v2.3.0 readiness review.
- Rule: v2.2.18 may recommend a v2.3.0 decision review, but must not declare v2.3.0 ready unless complete decision evidence exists.

## Evidence summary table

| Input category | Current value / summary | Evidence path(s) | Status |
|---|---|---|---|
| Evidence run IDs | Not yet recorded in consolidated handoff bundle. | `PENDING` | PENDING |
| Topology used | Planned target is 5-node + 4-miner RC topology; final executed topology not yet attached. | `docs/TOPOLOGY_MANIFEST_V2_2_18.md`; run bundle `topology/manifest.yaml` | PENDING |
| Duration achieved | RC sustained duration record not yet attached. | `summary.md` in evidence run bundle (expected) | PENDING |
| Nodes count | Planned 5 nodes for RC full rehearsal. | `docs/V2_2_18_PRIVATE_TESTNET_RC_PLAN.md` | PENDING |
| Miners count | Planned 4 miners for RC full rehearsal. | `docs/V2_2_18_PRIVATE_TESTNET_RC_PLAN.md` | PENDING |
| Perturbation results | Perturbation drill outputs not yet linked in closeout handoff. | `perturbation/summary.md` (expected) | PENDING |
| Snapshot/restore results | Snapshot/restore drill output not yet linked in closeout handoff. | `snapshot/restore_timing.csv` (expected) | PENDING |
| Sync convergence results | Convergence measurements not yet linked in closeout handoff. | `sync/convergence_summary.md` (expected) | PENDING |
| Miner telemetry results | Accept/reject share and node-acceptance visibility evidence not yet linked. | `miners/accept_reject_summary.md` (expected) | PENDING |
| Unresolved incidents | No consolidated incident export attached yet. | incident register export (expected) | PENDING |
| Waivers | No waiver ledger attached for this decision handoff. | waiver log (expected) | PENDING |
| Known limitations | Known limitations exist but decision-scoped mapping is not finalized. | `docs/KNOWN_LIMITATIONS_V2_2_18.md` | PENDING |

## Unresolved incidents
- Current state: **PENDING** (incident list not attached to this handoff).
- Hard rule: any unresolved Sev-1 consensus/sync issue => **NO-GO**.

## Waivers
- Current state: **PENDING** (no waiver list attached).
- Required fields for each waiver before decision: owner, UTC approval date, scope, expiry/exit criteria.

## Known limitations
- Baseline source: `docs/KNOWN_LIMITATIONS_V2_2_18.md`.
- Decision mapping status: **PENDING** (must identify which limitations are acceptable for v2.3.0 review entry).

## GO / NO-GO recommendation
- **Recommendation: NO-GO (provisional)**.
- Rationale:
  1. Required evidence inputs are not yet complete in this handoff document (multiple PENDING rows).
  2. Incident and waiver completeness is still PENDING.
  3. Rule enforcement: readiness cannot be declared with incomplete decision evidence; unresolved Sev-1 consensus/sync would force NO-GO.

## Exit criteria to re-evaluate recommendation
1. Populate evidence run IDs and attach full evidence bundle paths.
2. Fill measured topology, duration, node/miner counts from executed runs.
3. Attach perturbation, snapshot/restore, sync convergence, and miner telemetry outputs.
4. Attach incident register and waiver ledger.
5. Recompute recommendation with explicit GO/NO-GO signoff and UTC timestamp.
