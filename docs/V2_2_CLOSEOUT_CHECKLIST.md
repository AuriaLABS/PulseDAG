# PulseDAG v2.2 closeout checklist (release hygiene)

This checklist is the final v2.2 closeout package for **operational burn-in completion** and **release cut readiness**.

Scope guardrails for this closeout:
- No consensus changes.
- No miner behavior changes.
- No pool logic additions.
- No new product scope; release-hygiene evidence and operational verification only.

## 1) Burn-in closeout checklist
- [ ] Confirm 14-day burn-in execution completed per `docs/BURN_IN_14D.md`.
- [ ] Confirm each day includes runtime alerts, snapshot cadence, pruning cadence, p2p recovery, restart/recovery notes, and mining telemetry summaries.
- [ ] Confirm no unresolved Sev-1 safety incident remains at closeout.
- [ ] Confirm release manager sign-off is attached to the final evidence package.

## 2) Release evidence index (must resolve to real repo surfaces)
- Burn-in execution guide: `docs/BURN_IN_14D.md`
- Evidence package structure and acceptance: `docs/RELEASE_EVIDENCE.md`
- Runbook decision tree and procedures: `docs/runbooks/INDEX.md`
- Runtime and network event stream surfaces: `docs/RUNTIME_EVENT_STREAM.md`
- Dashboard package and telemetry fields: `docs/dashboard/README.md`
- Alert catalog and response mapping: `docs/dashboard/ALERTS.md`
- Startup-mode semantics and fallback interpretation: `docs/runbooks/FAST_BOOT_AND_FALLBACK.md`
- Recovery orchestration: `docs/runbooks/RECOVERY_ORCHESTRATION.md`
- Staging upgrade rehearsal: `docs/runbooks/STAGING_UPGRADE.md`
- Staging rollback rehearsal: `docs/runbooks/STAGING_ROLLBACK.md`
- Release binaries workflow reference: `.github/workflows/release-binaries.yml`
- Burn-in evidence workflow reference (historical name retained): `.github/workflows/v2_1-burnin-evidence.yml`

## 3) Recovery evidence checklist
- [ ] Bundle file restart-recovery-notes/restart-log.md records restart trigger, startup mode observed, elapsed recovery time, and operator follow-up.
- [ ] Bundle file p2p-recovery/recovery-events.csv includes churn/rejoin timing and outcome for each rehearsal incident.
- [ ] Snapshot/restore and/or snapshot+delta rebuild evidence links are attached when recovery required those paths.
- [ ] Runbook references are explicit in notes (`RECOVERY_ORCHESTRATION`, `SNAPSHOT_RESTORE`, `REBUILD_FROM_SNAPSHOT_AND_DELTA`).

## 4) Startup path visibility verification
For every restart/recovery drill, verify and capture:
- [ ] Startup path classification visible to operators (fast-boot vs replay/fallback) using `docs/runbooks/FAST_BOOT_AND_FALLBACK.md`.
- [ ] Runtime/event-stream evidence shows startup lifecycle transitions (`docs/RUNTIME_EVENT_STREAM.md`).
- [ ] Dashboard/runtime telemetry captures startup counters or equivalent fallback indicators (`docs/dashboard/README.md`).
- [ ] Any ambiguity is logged with follow-up owner/date in the evidence bundle.

## 5) Mining telemetry verification (external miner flow)
- [ ] Use `GET /runtime/status` fields documented in `docs/dashboard/README.md` to capture accepted/rejected mining submit trends.
- [ ] Confirm rejection taxonomy is recorded (stale/invalid/template mismatch categories where present).
- [ ] Confirm no unresolved regression pattern remains before release cut.
- [ ] Confirm miner evidence is tied to external-miner operation assumptions from `README.md` and `docs/MINER_FINAL.md`.

## 6) Upgrade/rollback rehearsal checklist
- [ ] Execute staged upgrade rehearsal per `docs/runbooks/STAGING_UPGRADE.md`.
- [ ] Execute staged rollback rehearsal per `docs/runbooks/STAGING_ROLLBACK.md`.
- [ ] Attach validation outputs (baseline, post-upgrade, post-rollback) to evidence bundle.
- [ ] Confirm post-rollback coherence checks pass and operators can recover without ad-hoc undocumented steps.
- [ ] Optional helper validation script output attached when used: `scripts/staging/validate_upgrade_rollback.sh`.

## 7) Release binaries workflow references
- [ ] Link workflow run(s): `.github/workflows/release-binaries.yml`.
- [ ] Record artifact identity (tag/build ref), checksums/hashes if produced, and retention/download location.
- [ ] Verify node/miner commit SHAs in evidence match the burn-in candidate bits.
- [ ] Confirm release-manager closeout includes explicit statement: no new scope introduced during closeout.

## 8) Final release-manager closeout statement template
Use this exact closure language in the evidence bundle `README.md`:

- v2.2 closeout package complete across burn-in, runtime/event evidence, recovery drills, startup-path visibility, mining telemetry, upgrade/rollback rehearsal, and release-binary provenance.
- No consensus changes, miner changes, pool logic additions, or other new product scope were introduced as part of closeout.
- v2.2 is approved for operational burn-in completion gate and final release cut.
