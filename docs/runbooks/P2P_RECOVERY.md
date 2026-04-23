# P2P Recovery Runbook (v2.2)

## Purpose
Recover and verify node networking health after peer loss, prolonged zero-peer state, or topology drift.

## Guardrails
- No consensus changes.
- No miner changes.
- Run in staging before production when possible.

## Detection triggers
- `/p2p/status` reports no connected peers unexpectedly.
- `/sync/status` indicates persistent lag while chain traffic exists.
- Runtime events show repeated peer disconnect churn.

## Recovery procedure
1. Capture current state:
   - `GET /health`
   - `GET /readiness`
   - `GET /status`
   - `GET /p2p/status`
   - `GET /p2p/topology`
   - `GET /sync/status`
   - `GET /sync/verify`
   - `GET /runtime/events?limit=200`
2. Interpret peer status using mode semantics from `docs/OPERATIONS_P2P.md`.
3. Confirm seed/bootnode config is correct and reachable.
4. Restart node process cleanly.
5. Re-check `/p2p/status` and `/p2p/topology` for peer convergence.
6. If still isolated, validate firewall/NAT rules and known peer availability.
7. If peer state stabilizes but sync coherence remains bad, continue with:
   - `docs/runbooks/RECOVERY_ORCHESTRATION.md`
   - `docs/runbooks/REBUILD_FROM_SNAPSHOT_AND_DELTA.md`

## Success criteria
- Connected peers return to expected baseline for environment.
- `/sync/verify` remains consistent.
- Runtime events show stable peer behavior over multiple intervals.
