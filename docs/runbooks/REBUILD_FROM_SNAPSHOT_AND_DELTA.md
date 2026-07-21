# Rebuild from Snapshot and Delta (v2.3.0 private testnet)

## Goal

Recover v2.3.0 private-testnet chain state by replaying retained blocks on top of the latest validated snapshot, with explicit validation, timing, reconciliation, and fallback evidence bound to one exact release candidate SHA.

## Guardrails

- Run against the exact candidate under evaluation and record its SHA, `VERSION=v2.3.0`, Cargo workspace version `2.3.0`, release metadata, operator, and UTC timestamps.
- Use the configured loopback RPC address; repository v2.3.0 private-testnet examples use `http://127.0.0.1:8280`.
- Capture immutable incident and baseline evidence before mutation.
- Do not use successful recovery evidence to claim public-testnet readiness or to start/backdate the 30-day clock.
- Any failed rebuild, reconciliation, readiness, or evidence-integrity check blocks private-testnet release closeout.

## Preconditions

- `/health` and `/readiness` are reachable and healthy enough to perform the planned operation.
- Operator has current snapshot, replay-plan, rebuild-preview, and maintenance-report visibility.
- A valid snapshot anchor and retained delta path exist, or the expected full-replay fallback path is documented.
- Incident artifacts and a backup of the pre-action state were captured before mutation.
- The previous working release remains available for lifecycle rollback.

## Operator steps

Set the canonical example RPC address:

```bash
RPC_URL=http://127.0.0.1:8280
```

1. Inspect replay prerequisites:

   ```bash
   curl --fail --silent "$RPC_URL/health" | jq
   curl --fail --silent "$RPC_URL/readiness" | jq
   curl --fail --silent "$RPC_URL/status" | jq
   curl --fail --silent "$RPC_URL/snapshot" | jq
   curl --fail --silent "$RPC_URL/sync/verify" | jq
   curl --fail --silent "$RPC_URL/sync/replay-plan" | jq
   curl --fail --silent "$RPC_URL/sync/rebuild-preview" | jq
   curl --fail --silent "$RPC_URL/maintenance/report" | jq '.data.state_audit'
   ```

2. If no snapshot is present, create one first:

   ```bash
   curl --fail --silent -X POST "$RPC_URL/snapshot/create" | jq
   ```

3. Optional prune, only after snapshot validation and a successful dry-run safety check:

   ```bash
   curl --fail --silent -X POST "$RPC_URL/prune" \
     -H 'content-type: application/json' \
     -d '{"keep_recent_blocks":64}' | jq
   ```

   Prune remains bounded by deterministic retention rules: a minimum 16-block rollback window plus a validated snapshot anchor. If prerequisites are missing, prune must be rejected or deferred.

4. Execute rebuild using the snapshot + delta path:

   ```bash
   curl --fail --silent -X POST "$RPC_URL/sync/rebuild" \
     -H 'content-type: application/json' \
     -d '{"force":true,"allow_partial_replay":false,"persist_after_rebuild":true,"reconcile_mempool":true}' | jq
   ```

5. Validate post-rebuild coherence and reconciliation:

   ```bash
   curl --fail --silent "$RPC_URL/sync/verify" | jq
   curl --fail --silent "$RPC_URL/maintenance/report" | jq '.data.state_audit'
   curl --fail --silent "$RPC_URL/maintenance/report?deep=true" | jq '.data.state_audit'
   curl --fail --silent "$RPC_URL/status" | jq
   curl --fail --silent "$RPC_URL/readiness" | jq
   curl --fail --silent "$RPC_URL/p2p/status" | jq
   curl --fail --silent "$RPC_URL/tx/mempool" | jq
   curl --fail --silent "$RPC_URL/runtime/events?limit=50" | jq
   ```

6. Confirm expected peers reconnect, replay gap is zero, selected tip and height are coherent, and external mining can be reattached when required.

7. Preserve all output, timing, event, and checksum evidence under the exact candidate record.

## Expected outcomes

- `/sync/rebuild` succeeds with `rebuilt=true` and `consistency_ok=true`.
- Best height and selected tip match expected chain progression.
- Runtime events include restore/rebuild completion evidence and any fallback reason.
- `/maintenance/report` includes read-only `state_audit` output with explicit pass/fail issues.
- `maintenance/report?deep=true` is used only for manual, opt-in deep verification runs.
- Mempool reconciliation completes without unexplained loss or invalid entries.
- `/readiness` contains no unresolved storage, replay, sync, or operator-safety blocker.

## Corrupt or invalid snapshot behavior

- If snapshot decode or snapshot+delta replay fails and persisted blocks exist, storage falls back to full replay from persisted blocks and emits warning runtime events.
- If snapshot decode fails and no persisted blocks are available, restore fails explicitly without partial block mutation.
- The evidence bundle must identify whether the primary path or fallback path was used and include the measured duration and final state audit.

## Drill repeatability

Use:

```bash
scripts/restore-drill-evidence.sh
scripts/snapshot-productization-evidence.sh
scripts/pruning-snapshot-integration-evidence.sh
```

These commands run targeted storage tests that assert:

1. Snapshot + delta restore reproduces expected best state.
2. Corrupt snapshot fallback is safe, or fails explicitly when fallback is impossible.
3. Rollback safety planning remains explicit; requested, effective, and minimum keep windows are validated.
4. Prune + replay remains coherent post-restore.
5. Restore timing and evidence are emitted in a reproducible report path, including repeat runs.
6. Snapshot export/import bundle verification surfaces are explicit and operator-practical: format/version, chain-ID match, anchor presence, and replay viability.

The final v2.3.0 closeout must retain fresh output from the exact candidate. Historical v2.2 results may be used only as a comparison baseline.

## Required closeout evidence

- Exact candidate SHA and release identity.
- Pre/post RPC captures and state-audit results.
- Rebuild request and response.
- Primary/fallback path used.
- Restore/rebuild duration.
- Final height, selected tip, replay gap, peer state, readiness, and mempool reconciliation summary.
- Operator, UTC timestamps, incident/waiver references, and SHA-256 manifest.

## Related runbooks

- `docs/runbooks/SNAPSHOT_RESTORE.md`
- `docs/runbooks/SNAPSHOT_PRUNE_RESTORE_DRILL.md`
- `docs/runbooks/RECOVERY_ORCHESTRATION.md`
- `docs/runbooks/V2_3_0_PRIVATE_TESTNET_OPERATIONS.md`
- `docs/checklists/V2_3_0_PRIVATE_TESTNET_RELEASE_CLOSEOUT.md`
