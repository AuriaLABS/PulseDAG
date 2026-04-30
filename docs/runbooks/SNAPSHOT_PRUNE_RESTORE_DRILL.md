# Snapshot / Prune / Restore Drill Preparation (private testnet)

## Purpose
This runbook prepares operators for private-testnet snapshot/prune/restore drills using conservative wrappers around existing RPC behavior. It does not modify snapshot serialization and does not relax pruning safety.

## Scope
- Preparation and guarded execution for snapshot/prune/restore drills.
- Operator-facing wrappers:
  - `scripts/ops/prune_safety_check.sh`
  - `scripts/ops/restore_drill.sh`

## Safety principles
1. Do not alter snapshot encoding/decoding format.
2. Keep prune behavior safety-first (minimum rollback floor remains node-enforced).
3. Default to dry-run wrappers unless explicitly toggled.
4. Always collect pre/post evidence (`/status`, `/snapshot`, `/sync/verify`, `/readiness`).

## Prerequisites
- A healthy private-testnet node exposing RPC (default `http://127.0.0.1:8080`).
- `curl` and `jq` installed on the operator host.
- Existing snapshot available (`GET /snapshot` => `snapshot_exists: true`).

## Phase 1 — prune safety precheck (dry-run)
```bash
RPC_URL=http://127.0.0.1:8080 KEEP_RECENT_BLOCKS=64 \
  scripts/ops/prune_safety_check.sh
```

What this checks:
- Snapshot exists.
- Snapshot anchor height is coherent with recommended prune floor.
- Replay-plan preview is queryable before maintenance.

Optional execution (explicit opt-in):
```bash
RPC_URL=http://127.0.0.1:8080 KEEP_RECENT_BLOCKS=64 APPLY_PRUNE=1 \
  scripts/ops/prune_safety_check.sh
```

## Phase 2 — restore drill preparation flow
```bash
RPC_URL=http://127.0.0.1:8080 KEEP_RECENT_BLOCKS=64 \
  scripts/ops/restore_drill.sh
```

Default behavior:
- Captures baseline status/snapshot/replay previews.
- Creates a fresh snapshot.
- Skips prune and rebuild unless explicitly enabled.

Optional explicit execution:
```bash
RPC_URL=http://127.0.0.1:8080 KEEP_RECENT_BLOCKS=64 DO_PRUNE=1 DO_REBUILD=1 \
  scripts/ops/restore_drill.sh
```

## Expected verification surfaces
- `/sync/verify` returns coherent chain checks.
- `/readiness` remains healthy.
- `/status` best height/tip remains stable after workflow.

## Failure handling
- If snapshot is missing, wrappers fail fast.
- If any RPC call fails, scripts stop immediately (`set -euo pipefail`).
- If prune/rebuild is not explicitly enabled, scripts remain non-destructive.

## Related runbooks
- `docs/runbooks/SNAPSHOT_RESTORE.md`
- `docs/runbooks/REBUILD_FROM_SNAPSHOT_AND_DELTA.md`
- `docs/runbooks/MAINTENANCE_SELF_CHECK.md`
