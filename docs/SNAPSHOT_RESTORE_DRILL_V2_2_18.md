# Snapshot/restore/rebuild drill (v2.2.18)

This drill validates that a non-seed node can be stopped, restored/rebuilt from snapshot-backed state, restarted, and rejoined to the cluster without unrecoverable state.

## Script

- `scripts/v2_2_18_snapshot_restore_drill.sh`

## Scenario covered

1. Run nodes long enough to produce state (`WARMUP_SECONDS`, default 45s).
2. Capture snapshot metadata from the target node (`/snapshot`).
3. Stop one non-seed node (`TARGET_NODE_NAME`, default `node-B`).
4. Backup the node data directory into an artifact-scoped temp path.
5. Execute snapshot export path (`POST /snapshot/create`) when available.
6. Restore/rebuild from copied state into a separate restored data directory.
7. Restart the node using original bind ports.
8. Verify rejoin (`peer_count > 0`) and convergence (`sync_lag <= 2`).
9. Measure recovery duration.
10. Capture lineage/check context (`lineage-checks.json`) and before/after endpoint evidence.

## Safety controls

- **No destructive delete of operator data**: source `run/<node>-data` is only copied; original directory is not removed.
- **No storage format changes**: script does not modify schema or format.
- **No consensus changes**: script only orchestrates stop/restore/restart.
- **No auto-pass without evidence**: pass/fail is emitted from runtime checks and persisted outputs.

## Output artifacts

Created under `artifacts/v2_2_18_snapshot_restore_drill/<RUN_ID>/`:

- `restore-timing.csv`
- `snapshot-metadata.json`
- `restore-summary.md`
- `before/*.json` endpoint captures
- `after/*.json` endpoint captures
- `lineage-checks.json`
- `snapshot-create-response.json`

## Prerequisites

- Local cluster already running (for example via `scripts/v2_2_18_start_vps_rehearsal.sh`).
- `run/v2_2_18_vps_nodes.pid` exists.
- `target/debug/pulsedagd` exists and is executable.
- Tools: `bash`, `curl`, `jq`, `awk`.

## Usage

```bash
bash scripts/v2_2_18_snapshot_restore_drill.sh
```

With overrides:

```bash
RUN_ID=manual-restore-drill-01 \
TARGET_NODE_NAME=node-B \
WARMUP_SECONDS=60 \
REJOIN_TIMEOUT_SECONDS=240 \
bash scripts/v2_2_18_snapshot_restore_drill.sh
```

## Pass/fail criteria

Pass requires all of the following runtime evidence:

- Node process restarts successfully.
- Node rejoins peers (`peer_count > 0`).
- Node converges to network state (`sync_lag <= 2` in timeout window).
- No storage corruption signs in captured checks/logs.
- No unrecoverable state observed.

If rejoin/convergence is not observed before timeout, the run is recorded as failed in `restore-timing.csv` and `restore-summary.md`.
