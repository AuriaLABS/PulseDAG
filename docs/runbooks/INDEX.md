# PulseDAG v2.2 Runbook Index

This index consolidates the v2.2 operator package for recovery, rebuild, restore, maintenance, and staging safety workflows.

## Start here (decision flow)
1. **Node unhealthy or degraded?** Start with `docs/runbooks/MAINTENANCE_SELF_CHECK.md`.
2. **Peer loss / partition symptoms?** Use `docs/runbooks/P2P_RECOVERY.md`.
3. **State rebuild or restore required?** Use `docs/runbooks/RECOVERY_ORCHESTRATION.md`.
4. **Need deeper rebuild details?** Use `docs/runbooks/REBUILD_FROM_SNAPSHOT_AND_DELTA.md`.
5. **Need restore drill and RTO evidence?** Use `docs/runbooks/SNAPSHOT_RESTORE.md`.
6. **Need crash/restart/churn validation evidence?** Use `docs/runbooks/CHAOS_RESTART_RECOVERY_SUITE.md`.
7. **Need startup-mode interpretation and fallback counters?** Use `docs/runbooks/FAST_BOOT_AND_FALLBACK.md`.

## Core runbooks
- `docs/runbooks/MAINTENANCE_SELF_CHECK.md` — routine operator self-check, drift checks, and pre-maintenance safety gates.
- `docs/runbooks/P2P_RECOVERY.md` — peer-loss / topology recovery and rejoin checklist.
- `docs/runbooks/RECOVERY_ORCHESTRATION.md` — recovery triage matrix (recovery vs rebuild vs restore).
- `docs/runbooks/REBUILD_FROM_SNAPSHOT_AND_DELTA.md` — snapshot + delta rebuild workflow and post-checks.
- `docs/runbooks/SNAPSHOT_RESTORE.md` — restore drill procedure, fallback expectations, and RTO evidence.
- `docs/runbooks/CHAOS_RESTART_RECOVERY_SUITE.md` — repeatable crash/restart/churn/recovery validation suite and evidence workflow.
- `docs/runbooks/FAST_BOOT_AND_FALLBACK.md` — fast-boot behavior, fallback signals, and when to escalate.

## Staging safety
- `docs/runbooks/STAGING_UPGRADE.md` — staged upgrade validation path.
- `docs/runbooks/STAGING_ROLLBACK.md` — rollback decision and execution path.

## Public testnet readiness gate
- `docs/runbooks/FINAL_POW_PUBLIC_TESTNET_DRY_RUN.md` — final multi-node, multi-miner dry-run and go/no-go gate before public testnet open.

## Evidence and operations support docs
- `docs/checklists/V2_2_3_BURNIN_CLOSEOUT.md` — final v2.2.3 burn-in closeout checklist and evidence index.
- `docs/RELEASE_EVIDENCE.md` — release evidence bundle requirements.
- `docs/BURN_IN_14D.md` — 14-day burn-in requirements.
- `docs/dashboard/README.md` — operator dashboard package.
- `docs/dashboard/ALERTS.md` — official alert catalog and first-response mapping.
