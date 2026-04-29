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
- `GET /operator/query-pack`
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
  - `external_mining_rejected_duplicate_block`
  - `external_mining_rejected_invalid_block`
  - `external_mining_rejected_chain_id_mismatch`
  - `external_mining_rejected_internal_error`
  - `external_mining_rejected_storage_error`
  - template-health rollups grounded in submit outcomes:
    - `external_mining_template_health` (`healthy`, `watch`, `stale_dominant`, `idle`, `counter_mismatch`)
    - `external_mining_template_stale_submit_ratio_bps`
    - `external_mining_hashrate_hps`
    - `external_mining_worker_efficiency_bps`
    - `external_mining_stale_efficiency_bps`
    - `external_mining_template_usefulness_bps`
    - `external_mining_template_rollup`

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
  - catch-up progress modeling:
    - `sync_catchup_stage` (`steady`, `discovering`, `acquiring`, `validating`, `recovering`, `degraded`)
    - `sync_lag_blocks`
    - `sync_lag_band` (`aligned`, `near_tip`, `catching_up`, `lagging`, `severely_lagging`)
    - `sync_catchup_progress_bps` (bounded `0..=10000`)
    - `sync_catchup_summary`
    - `sync_recovery_reason` (explicit degraded/stalled/catch-up explanation)
- p2p health rollup:
  - `p2p_peer_health_total`
  - `p2p_peer_health_counters_coherent`
  - `p2p_surface_health`
  - explicit peer lifecycle and shaping signals:
    - `p2p_peer_lifecycle_watch`
    - `p2p_peer_lifecycle_cooldown`
    - `p2p_degraded_mode`
    - `p2p_connection_shaping_active`
- mempool pressure rollup:
  - `mempool_capacity_remaining_transactions`
  - `mempool_orphan_pressure_bps`
  - `mempool_pressure_tier` (`normal`, `elevated`, `high_pressure`, `saturated`)
  - `mempool_orphan_pressure_tier` (`normal`, `elevated`, `high_pressure`, `saturated`)
  - explicit backpressure signaling:
    - `mempool_backpressure_active`
    - `mempool_backpressure_signal` (`none`, `mempool_high_pressure`, `orphan_high_pressure`, `mempool_saturated`, `orphan_saturated`, `at_capacity`)

  - explicit pressure ceilings (bounded and deterministic):
    - high-pressure ceiling: `8000` bps (`mempool_high_pressure` / `orphan_high_pressure`)
    - saturated ceiling: `9500` bps (`mempool_saturated` / `orphan_saturated`)
    - hard-capacity ceiling: `at_capacity` when `mempool_capacity_remaining_transactions == 0`
  - `mempool_surface_health`
- external mining rollup:
  - `external_mining_surface_health`
 - PoW retarget diagnostics:
  - `retarget_multiplier_bps`
  - `retarget_min_bps`
  - `retarget_max_bps`
  - `retarget_was_clamped`
  - `retarget_rationale`
  - `retarget_signal_quality`
- operator alert classes + incident diagnostics (v2.4):
  - `runtime_alert_classes` (explicit classes such as `node_integrity`, `sync_pipeline`, `mempool_pressure`, `p2p_recovery`, `mining_submissions`, `tip_stagnation`)
  - incident triage helpers:
    - `incident_primary_surface`
    - `incident_summary`
    - `incident_indicators`
- SLO-style health rollups (v2.4):
  - `node_health_slo_bps`
  - `sync_health_slo_bps`
  - `p2p_health_slo_bps`
  - `mempool_health_slo_bps`
  - `mining_health_slo_bps`
  - `runtime_health_slo_bps`
  - all values are bounded `0..=10000` and derived from explicit coherence/degraded states (counter mismatches incur the strongest penalty).

These fields are strictly additive and designed for coherent dashboards: contradictory combinations are normalized into explicit degraded/counter-mismatch states instead of ambiguous operator signals.

### Sync status catch-up extensions (v2.3)
`GET /sync/status` now mirrors bounded operator catch-up visibility:
- `catchup_stage`
- `lag_blocks`
- `lag_band`
- `catchup_progress_bps` (bounded `0..=10000`)
- `catchup_summary`
- `recovery_reason`

### Cross-surface runtime rollup normalization (v2.3)
To reduce contradictory operator interpretations across endpoints:
- `GET /diagnostics` now includes `runtime_surface_rollup` (startup/sync/tx/mining/node normalized health summary).
- `GET /runtime/events/summary` now also includes `runtime_surface_rollup`.
- Rollup values are computed from the same normalization logic used by `GET /runtime/status`, so dashboards can join these surfaces without conflicting status semantics.
- v2.4 extends this with incident-facing consistency:
  - diagnostics now mirrors `incident_primary_surface`, `incident_summary`, and `incident_indicators`.
  - `runtime_surface_rollup` now carries alert classes and SLO-style bps rollups so status, diagnostics, and event-summary surfaces remain aligned during incidents.

### Operator trend windows + incident snapshots (v2.5)
To make troubleshooting/burn-in dashboards more actionable without changing consensus behavior:
- `GET /runtime/status` now includes `incident_snapshot` (bounded incident state built from the same rollup as top-level incident fields).
- `GET /runtime/events/summary` now includes:
  - `incident_snapshot`
  - `trend_windows` with bounded windows:
    - `last_5m` (300s)
    - `last_30m` (1800s)
    - `last_2h` (7200s)
  - each window carries `event_count`, `warn_or_error_count`, `dominant_kind`, and an incident snapshot that is normalized to avoid contradictory summaries.
- `GET /diagnostics` mirrors both `incident_snapshot` and `trend_windows` so dashboards can join runtime/diagnostics/event surfaces with coherent incident semantics.

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


## Performance baseline companion (v2.2.4)

For operator baseline capture across p2p churn recovery, sync convergence, runtime/status responsiveness, and read-side RPC latency, use:

```bash
scripts/p2p-sync-rpc-baseline.sh http://127.0.0.1:8080
```

Methodology and artifact format are documented in `docs/benchmarks/V2_2_4_P2P_SYNC_RPC_BASELINE_METHODOLOGY.md`.

For explicit hot-path coverage (sync, relay, mempool, mining, recovery), use:

```bash
scripts/hot-path-baseline.sh http://127.0.0.1:8080
```

Methodology is documented in `docs/benchmarks/HOT_PATH_MEASUREMENT_METHODOLOGY.md`.
### Operator query pack (v2.5)
`GET /operator/query-pack` provides an explicit, read-only incident/audit bundle for operators:
- `incident_view` and `runtime_rollup` mirrors diagnostics/runtime normalization.
- `sync_recovery_view` for sync incidents and recovery-confidence triage.
- `relay_health_view` for p2p/tx propagation coherency checks.
- `mining_audit_view` for external mining submission/template health and rejection coherence.
- `startup_recovery_view` for startup path/fallback and replay requirements.
- `deterministic_notes` for explicit audit semantics (`operator_read_only_surface`, etc.).

- queue pressure counters now include `p2p_queue_backpressure_drops` to make relay backpressure explicit under load.
