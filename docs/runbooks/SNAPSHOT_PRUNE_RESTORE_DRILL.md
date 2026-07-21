# v2.3.0 Snapshot / Prune / Restore Drill (private testnet)

## Purpose

Prepare and execute a conservative, candidate-scoped private-testnet snapshot/prune/restore drill using wrappers around the supported RPC behavior. The procedure does not modify snapshot serialization and does not relax pruning safety.

## Scope and candidate binding

- Run against the exact v2.3.0 private-testnet release candidate under evaluation.
- Record candidate SHA, `VERSION=v2.3.0`, Cargo workspace version `2.3.0`, node release metadata, operator, and UTC timestamps.
- Use the canonical loopback RPC address from the selected private-testnet configuration; repository v2.3.0 examples use `http://127.0.0.1:8280`.
- Operator-facing wrappers:
  - `scripts/ops/prune_safety_check.sh`
  - `scripts/ops/restore_drill.sh`
- This drill does not authorize a tag, publication, public-testnet launch, or the start/backdating of the 30-day clock.

## Safety principles

1. Do not alter snapshot encoding/decoding format.
2. Keep prune behavior safety-first; the minimum rollback floor remains node-enforced.
3. Default to dry-run wrappers unless destructive execution is explicitly enabled.
4. Always collect pre/post evidence from `/health`, `/status`, `/snapshot`, `/sync/verify`, `/readiness`, and `/maintenance/report`.
5. Preserve the exact pre-action state backup and evidence bundle before prune or rebuild.
6. Treat missing, incomplete, or checksum-invalid evidence as a failed closeout gate.

## Prerequisites

- A healthy v2.3.0 private-testnet node exposing loopback RPC.
- `curl` and `jq` installed on the operator host.
- Existing snapshot available (`GET /snapshot` => `snapshot_exists: true`).
- Snapshot anchor and replay-plan metadata are present and coherent.
- The candidate SHA and release metadata have been captured before mutation.

## Phase 1 — baseline evidence

```bash
RPC_URL=http://127.0.0.1:8280
curl --fail --silent "$RPC_URL/health" | jq
curl --fail --silent "$RPC_URL/readiness" | jq
curl --fail --silent "$RPC_URL/status" | jq
curl --fail --silent "$RPC_URL/snapshot" | jq
curl --fail --silent "$RPC_URL/sync/verify" | jq
curl --fail --silent "$RPC_URL/sync/replay-plan" | jq
curl --fail --silent "$RPC_URL/sync/rebuild-preview" | jq
```

Archive these captures before continuing.

## Phase 2 — prune safety precheck

Dry run:

```bash
RPC_URL=http://127.0.0.1:8280 KEEP_RECENT_BLOCKS=64 \
  scripts/ops/prune_safety_check.sh
```

What this checks:

- Snapshot exists.
- Snapshot anchor height is coherent with the recommended prune floor.
- Replay-plan preview is queryable before maintenance.
- The requested retention window does not bypass the node-enforced rollback minimum.

Optional execution, with explicit opt-in only after reviewing the dry-run evidence:

```bash
RPC_URL=http://127.0.0.1:8280 KEEP_RECENT_BLOCKS=64 APPLY_PRUNE=1 \
  scripts/ops/prune_safety_check.sh
```

## Phase 3 — restore drill preparation flow

Default non-destructive behavior:

```bash
RPC_URL=http://127.0.0.1:8280 KEEP_RECENT_BLOCKS=64 \
  scripts/ops/restore_drill.sh
```

The default flow:

- captures baseline status, snapshot, verification, and replay previews;
- creates a fresh snapshot;
- skips prune and rebuild unless explicitly enabled.

Optional explicit execution:

```bash
RPC_URL=http://127.0.0.1:8280 KEEP_RECENT_BLOCKS=64 DO_PRUNE=1 DO_REBUILD=1 \
  scripts/ops/restore_drill.sh
```

## Phase 4 — post-action verification

Require all of the following:

- `/health` succeeds.
- `/readiness` is healthy and contains no candidate-relevant blocker.
- `/sync/verify` reports coherent chain checks.
- `/status` best height and selected tip are stable or progressing as expected.
- `/maintenance/report` reports a coherent state audit.
- Replay gap, missing-parent backlog, and unresolved storage inconsistency are zero or within an explicitly accepted non-blocking limit.
- Expected peers reconnect and external mining can be reattached when required.

## Evidence bundle

The retained bundle must include:

- exact candidate SHA and release metadata;
- operator and UTC start/end timestamps;
- all baseline and post-action RPC captures;
- wrapper stdout/stderr and exit codes;
- prune/rebuild request parameters;
- restore duration and recovery/fallback events;
- resulting best height and selected tip;
- incident or waiver references, if any;
- SHA-256 manifest covering every evidence file.

## Failure handling

- If a snapshot is missing, wrappers fail fast.
- If any RPC call fails, scripts stop immediately (`set -euo pipefail`).
- If prune/rebuild is not explicitly enabled, scripts remain non-destructive.
- If restore coherence, readiness, evidence integrity, or the RTO criterion fails, the private-testnet release remains `REQUEST_CHANGES` or `NO_GO` until corrected or explicitly waived under the release policy.
- Never relabel an earlier-version or earlier-candidate drill as final v2.3.0 evidence.

## Related runbooks

- `docs/runbooks/SNAPSHOT_RESTORE.md`
- `docs/runbooks/REBUILD_FROM_SNAPSHOT_AND_DELTA.md`
- `docs/runbooks/MAINTENANCE_SELF_CHECK.md`
- `docs/runbooks/V2_3_0_PRIVATE_TESTNET_OPERATIONS.md`
- `docs/checklists/V2_3_0_PRIVATE_TESTNET_RELEASE_CLOSEOUT.md`
