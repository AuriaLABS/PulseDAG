# Recovery, Rebuild, and Restore Orchestration (v2.2)

## Purpose
Give operators one triage path that answers: **recover networking**, **rebuild local state**, or **run restore drill / restore operation**.

## Triage matrix

| Symptom | Primary checks | Action |
|---|---|---|
| Zero peers / unstable peers | `/p2p/status`, `/p2p/topology`, `/runtime/events` | Run `P2P_RECOVERY.md` first |
| Sync lag without peer health issue | `/sync/status`, `/sync/verify`, `/runtime/events` | Use rebuild workflow (`REBUILD_FROM_SNAPSHOT_AND_DELTA.md`) |
| Storage/snapshot concern or prune planning | `/snapshot`, `/sync/replay-plan`, `/sync/rebuild-preview` | Use restore drill + rebuild checks (`SNAPSHOT_RESTORE.md`, `REBUILD_FROM_SNAPSHOT_AND_DELTA.md`) |
| Repeated fallback warnings at startup/runtime | `/runtime`, `/runtime/events`, `/sync/verify` | Interpret via `FAST_BOOT_AND_FALLBACK.md`, then rebuild if required |

## Standard incident sequence
1. Capture state bundle:
   - `GET /health`
   - `GET /readiness`
   - `GET /status`
   - `GET /sync/status`
   - `GET /sync/verify`
   - `GET /runtime`
   - `GET /runtime/events?limit=200`
2. Determine if issue is **networking** or **state coherence/storage**.
3. Follow the dedicated runbook path (P2P recovery or rebuild/restore).
4. Re-run maintenance gate from `MAINTENANCE_SELF_CHECK.md` before incident closure.

## Escalation criteria
Escalate to rebuild/restore path if either is true:
- `/sync/verify` remains inconsistent after networking recovery steps.
- Runtime events repeatedly indicate snapshot decode/delta replay fallback behavior.

## Recovery completion criteria
- `/health`, `/readiness`, and `/sync/verify` all healthy/consistent.
- `/status.best_height` progresses as expected.
- P2P peer behavior stable for observed interval.
- Incident notes include endpoint snapshots and selected recovery path.

## Runtime no-go escalation mapping
When runtime surfaces `no_go_escalation=true`, treat listed `no_go_reasons` as blockers until remediated and evidenced via operator query-pack snapshots.
