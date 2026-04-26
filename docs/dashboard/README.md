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
- `GET /diagnostics`
- `GET /runtime/events/summary`
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
- stale-work attribution helpers:
  - `external_mining_stale_work_submit_rejections`
  - `external_mining_stale_work_template_invalidations`
- `external_mining_submit_accepted`
- `external_mining_submit_rejected`
- submit/rejection counter coherence helpers:
  - `external_mining_submit_total`
  - `external_mining_submit_outcome_total`
  - `external_mining_submit_outcome_counters_coherent`
  - `external_mining_submit_outcome_counter_delta`
  - `external_mining_rejection_reason_total`
  - `external_mining_rejection_counters_coherent`
  - `external_mining_rejection_counter_delta`
- rejection taxonomy:
  - `external_mining_rejected_invalid_pow`
  - `external_mining_rejected_stale_template`
  - `external_mining_rejected_unknown_template`
  - `external_mining_rejected_submit_block_error`
  - `external_mining_rejected_storage_error`

### Propagation diagnostics fields (v2.2)
`GET /runtime/status` also exposes compact propagation diagnostics for operator triage:
- tx inbound/drop/rebroadcast coherence helpers:
  - `tx_inbound_counters_coherent`
  - `tx_inbound_counter_delta`
  - `tx_drop_reason_counters_coherent`
  - `tx_drop_reason_counter_delta`
  - `tx_rebroadcast_outcomes_coherent`
  - `tx_rebroadcast_outcome_counter_delta`
  - `tx_propagation_health`
- tx relay decision counters:
  - `p2p_tx_outbound_duplicates_suppressed`
  - `p2p_tx_outbound_first_seen_relayed`
  - `p2p_tx_outbound_recovery_relayed`
  - `p2p_tx_outbound_priority_relayed`
  - `p2p_tx_outbound_budget_suppressed`
  - `p2p_tx_relay_total_events`
  - `p2p_tx_relay_duplicate_ratio_bps`
  - `p2p_tx_relay_budget_suppression_ratio_bps`
- block relay decision counters:
  - `p2p_block_outbound_duplicates_suppressed`
  - `p2p_block_outbound_first_seen_relayed`
  - `p2p_block_outbound_recovery_relayed`
  - `p2p_block_relay_total_events`
  - `p2p_block_relay_duplicate_ratio_bps`

### Runtime status surface extensions (v2.3)
`GET /runtime/status` now includes additive operator-facing coherence/health helpers across node, sync, p2p, mempool, and mining surfaces:
- node-level rollup:
  - `node_runtime_surface_health`
- sync rollup and backlogs:
  - `sync_surface_health`
  - `sync_counters_coherent`
  - `sync_blocks_request_backlog`
  - `sync_blocks_validation_backlog`
- p2p health rollup:
  - `p2p_peer_health_total`
  - `p2p_peer_health_counters_coherent`
  - `p2p_surface_health`
- mempool pressure rollup:
  - `mempool_capacity_remaining_transactions`
  - `mempool_orphan_pressure_bps`
  - `mempool_surface_health`
- external mining rollup:
  - `external_mining_surface_health`

These fields are strictly additive and designed for coherent dashboards: contradictory combinations are normalized into explicit degraded/counter-mismatch states instead of ambiguous operator signals.

### Cross-surface runtime rollup normalization (v2.3)
To reduce contradictory operator interpretations across endpoints:
- `GET /diagnostics` now includes `runtime_surface_rollup` (startup/sync/tx/mining/node normalized health summary).
- `GET /runtime/events/summary` now also includes `runtime_surface_rollup`.
- Rollup values are computed from the same normalization logic used by `GET /runtime/status`, so dashboards can join these surfaces without conflicting status semantics.

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
