# Operator Dashboard Package (v2.2)

This folder documents the official v2.2 observability package.

## Official package files
- `ops/dashboard/v2.2/official-dashboards.json` — canonical dashboard definitions by runtime surface.
- `ops/dashboard/v2.2/official-alert-rules.json` — canonical alert rules mapped to real runtime fields.
- `scripts/validate_observability_package.py` — validator that checks dashboard/alert field references against emitted API surfaces.

## Covered operator surfaces
- p2p health/recovery
- sync state and lag
- mempool pressure
- relay behavior
- snapshot/prune/rebuild health
- mining flow health
- node release/runtime health

## Data source grounding (real node surfaces)
The package references only fields emitted by the node APIs:
- `GET /runtime/status`
- `GET /status`
- `GET /sync/status`
- `GET /p2p/status`
- `GET /tx/mempool`
- `GET /pow/health`

### External mining telemetry fields (v2.2)
`GET /runtime/status` now includes dedicated external mining flow telemetry:
- `external_mining_templates_emitted`
- `external_mining_templates_invalidated`
- `external_mining_stale_work_detected`
- `external_mining_submit_accepted`
- `external_mining_submit_rejected`
- rejection taxonomy:
  - `external_mining_rejected_invalid_pow`
  - `external_mining_rejected_stale_template`
  - `external_mining_rejected_unknown_template`
  - `external_mining_rejected_submit_block_error`
  - `external_mining_rejected_storage_error`

## How operators use this package
1. Wire your telemetry collector (Prometheus, OTEL collector, or Grafana JSON/Infinity datasource) to poll the API endpoints above.
2. Import/translate dashboard panels in `ops/dashboard/v2.2/official-dashboards.json` into your Grafana dashboard objects.
3. Materialize alerts from `ops/dashboard/v2.2/official-alert-rules.json` in your alerting backend.
4. Run validation before rollout:
   ```bash
   python3 scripts/validate_observability_package.py
   ```

## Related runbooks
- `docs/runbooks/INDEX.md`
- `docs/runbooks/P2P_RECOVERY.md`
- `docs/runbooks/RECOVERY_ORCHESTRATION.md`
- `docs/runbooks/REBUILD_FROM_SNAPSHOT_AND_DELTA.md`
- `docs/runbooks/SNAPSHOT_RESTORE.md`

- `docs/dashboard/ALERTS.md`
