# PulseDAG v2.3.0 Private-Testnet Runbook Index

## Purpose

This index is the active operator entrypoint for v2.3.0 private-testnet operations. Historical v2.2 procedures remain available where they still describe a supported recovery mechanism, but v2.3.0 lifecycle, observability, incident, security, evidence, and rehearsal procedures take precedence.

This index does not authorize a public testnet, a release tag, a version bump, or the start of the 30-day public-testnet clock.

## Start here

1. **Routine bootstrap, lifecycle, miner attachment, upgrade, rollback, or evidence?** Use `docs/runbooks/V2_3_0_PRIVATE_TESTNET_OPERATIONS.md`.
2. **Running the final five-node multi-host rehearsal?** Use `docs/runbooks/V2_3_0_PRIVATE_TESTNET_REHEARSAL.md`.
3. **Active incident or severity decision?** Use `docs/runbooks/V2_3_0_INCIDENT_RESPONSE.md`.
4. **RPC abuse, disk pressure, credential exposure, or identity rotation?** Use `docs/runbooks/V2_3_0_SECURITY_AND_CAPACITY.md`.
5. **Node unhealthy or degraded?** Use `docs/runbooks/MAINTENANCE_SELF_CHECK.md`.
6. **Peer loss or partition symptoms?** Use `docs/runbooks/P2P_RECOVERY.md`.
7. **Lag, missing parents, convergence, or recovery choice?** Use `docs/runbooks/RECOVERY_ORCHESTRATION.md`.
8. **Snapshot/replay rebuild required?** Use `docs/runbooks/REBUILD_FROM_SNAPSHOT_AND_DELTA.md`.
9. **Snapshot restore drill and RTO evidence?** Use `docs/runbooks/SNAPSHOT_RESTORE.md`.

## v2.3.0 operator baseline

- `docs/runbooks/V2_3_0_PRIVATE_TESTNET_OPERATIONS.md` — bootstrap, external miner, routine checks, lifecycle, upgrade/rollback, state protection, evidence, and decommissioning.
- `docs/runbooks/V2_3_0_PRIVATE_TESTNET_REHEARSAL.md` — exact-candidate five-node rehearsal, restart, bounded partition, rejoin, GO/NO-GO, and immutable evidence.
- `docs/runbooks/V2_3_0_INCIDENT_RESPONSE.md` — SEV-1 through SEV-4, roles, evidence custody, containment, recovery, communications, and closure.
- `docs/runbooks/V2_3_0_SECURITY_AND_CAPACITY.md` — RPC abuse, disk pressure, identity/token rotation, and monitoring-network access.
- `scripts/private_testnet/node_lifecycle.py` — supported node install/start/stop/status/restart/upgrade/rollback controller.
- `scripts/private_testnet/multi_host_rehearsal.py` — supported five-node private-testnet rehearsal and evidence verifier.
- `scripts/private_testnet/runtime_metrics_exporter.py` — supported private monitoring exporter.
- `scripts/private_testnet/collect_incident_evidence.py` — redacted, checksummed incident evidence collector.
- `ops/observability/v2.3.0/README.md` — Prometheus, Grafana, alert, and metric baseline.
- `docs/dashboard/README.md` — active dashboard entrypoint.
- `docs/dashboard/ALERTS.md` — active alert catalog and first-response mapping.

## Recovery runbooks

- `docs/runbooks/MAINTENANCE_SELF_CHECK.md` — read-only health, drift, and maintenance checks.
- `docs/runbooks/P2P_RECOVERY.md` — peer loss, partition, topology recovery, and rejoin.
- `docs/runbooks/RECOVERY_ORCHESTRATION.md` — choose recovery, rebuild, or restore.
- `docs/runbooks/REBUILD_FROM_SNAPSHOT_AND_DELTA.md` — snapshot plus delta rebuild and post-checks.
- `docs/runbooks/SNAPSHOT_RESTORE.md` — restore procedure, fallback expectations, and RTO evidence.
- `docs/runbooks/SNAPSHOT_PRUNE_RESTORE_DRILL.md` — guarded private-testnet snapshot/prune/restore drill.
- `docs/runbooks/CHAOS_RESTART_RECOVERY_SUITE.md` — crash, restart, churn, and recovery validation.
- `docs/runbooks/FAST_BOOT_AND_FALLBACK.md` — fast-boot and fallback signal interpretation.

## Upgrade compatibility material

The Task 09 lifecycle controller is the active v2.3.0 mechanism. These v2.2 runbooks remain indexed because release regressions and historical operator flows still reference them:

- `docs/runbooks/STAGING_UPGRADE.md`
- `docs/runbooks/STAGING_ROLLBACK.md`

## Evidence and release support

- `docs/RELEASE_EVIDENCE.md` — release evidence bundle requirements.
- `docs/codex_tasks/v2_3_0_12_multi_host_rehearsal.md` — Task 12 deliverables, acceptance criteria, and guardrails.
- `docs/checklists/V2_2_6_BURNIN_CLOSEOUT.md` — historical v2.2.6 burn-in closeout evidence.
- `docs/BURN_IN_14D.md` — historical 14-day burn-in requirements.
- `docs/runbooks/FINAL_POW_PUBLIC_TESTNET_DRY_RUN.md` — historical public-testnet prerequisite material; not an active launch authorization.

## Escalation rule

An unresolved SEV-1 integrity/security incident, replay gap, convergence failure, credential exposure, destructive recovery without evidence, or failed Task 12 restore/rejoin is a private-testnet no-go. Runbook completion and a private-testnet `GO` never change `public_testnet_ready=false` or start/backdate the public-testnet clock.
