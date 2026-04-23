# Maintenance and Self-Check Runbook (v2.2)

## Purpose
Provide a practical, repeatable self-check before and after maintenance windows so operators can detect drift early and avoid unnecessary rebuild/restore actions.

## Guardrails
- No consensus behavior changes.
- No miner behavior changes.
- No pool/accounting logic changes.

## When to run
- Daily/shift checks.
- Before restart, config change, or package upgrade.
- After incident recovery.

## Baseline checks (read-only)
Run and archive outputs:

```bash
curl -s http://127.0.0.1:8080/health | jq
curl -s http://127.0.0.1:8080/readiness | jq
curl -s http://127.0.0.1:8080/status | jq
curl -s http://127.0.0.1:8080/sync/status | jq
curl -s http://127.0.0.1:8080/sync/verify | jq
curl -s http://127.0.0.1:8080/checks | jq
curl -s http://127.0.0.1:8080/maintenance/report | jq
curl -s 'http://127.0.0.1:8080/runtime/events?limit=100' | jq
curl -s 'http://127.0.0.1:8080/runtime/events/summary?limit=500' | jq
```

## P2P and topology sanity
```bash
curl -s http://127.0.0.1:8080/p2p/status | jq
curl -s http://127.0.0.1:8080/p2p/topology | jq
```

Interpret using `docs/runbooks/FAST_BOOT_AND_FALLBACK.md` and `docs/OPERATIONS_P2P.md`.

## Snapshot and replay readiness
```bash
curl -s http://127.0.0.1:8080/snapshot | jq
curl -s http://127.0.0.1:8080/sync/replay-plan | jq
curl -s http://127.0.0.1:8080/sync/rebuild-preview | jq
```

If snapshot readiness is poor or replay window is too small for policy, plan maintenance before next risk window.

## Pre-maintenance gate
Proceed only if all are true:
- `/health` and `/readiness` are healthy/ready.
- `/sync/verify` reports consistent state.
- `/status.chain_id` is non-empty and stable.
- Runtime events do not show active critical fault patterns.

If any fail, switch to `docs/runbooks/RECOVERY_ORCHESTRATION.md`.

## Post-maintenance gate
After restart/change:
1. Re-run baseline checks above.
2. Confirm no regression in best height progression.
3. Confirm peer convergence if in real P2P mode.
4. Capture artifacts for the maintenance ticket.

## Fast smoke helper scripts
- `scripts/smoke.ps1`
- `scripts/dev-smoke.ps1`
- `scripts/recovery-smoke.ps1`
- `scripts/staging/validate_upgrade_rollback.sh`
