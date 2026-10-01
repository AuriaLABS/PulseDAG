# PulseDAG operator dashboards

The active v3.0.0 release-candidate/private-rehearsal observability package is versioned under:

- `ops/observability/v3.0.0/README.md`
- `ops/observability/v3.0.0/metrics-inventory.json`
- `ops/observability/v3.0.0/prometheus-scrape.example.yml`
- `ops/observability/v3.0.0/alert-rules.yml`
- `ops/observability/v3.0.0/grafana-dashboard.json`

The canonical exporter remains:

- `scripts/private_testnet/runtime_metrics_exporter.py`

Validation commands:

```bash
python3 scripts/validate_v3_0_0_observability.py
bash scripts/tests/test_v3_0_0_observability.sh
```

The compatibility command delegates to the active v3.0.0 validator:

```bash
python3 scripts/validate_observability_package.py
```

## Historical compatibility

`ops/observability/v2.3.0/` and its validator/tests remain retained for historical private-testnet evidence. The exporter continues to accept a v2.3.0 inventory only when explicitly selected for historical compatibility; the default inventory is v3.0.0. New v3.0.0 candidate/rehearsal evidence must use the v3.0.0 package explicitly.

The former **Operator Dashboard Package (v2.2)** is historical compatibility material only. Keeping that label documented preserves the release/RPC compatibility contract; it is not the active dashboard source of truth.

## Supported v3.0.0 operator surfaces

The v3.0.0 package intentionally polls only read-only routes present in the `public_safe` RPC profile:

- `GET /metrics` for commit/state-root, snapshot-verification, mining-submit actor, P2P recovery, selected-chain convergence and RPC-liveness counters;
- `GET /status` for node height, peers, snapshot and degraded/stale status;
- `GET /mempool` for main/orphan transaction pressure.

It does not poll `/admin/*` or `/runtime`, and no operator bearer token, wallet key, seed or credential is required by the exporter.

## Security model

Run the exporter beside the node and keep node RPC bound to loopback/private management as defined by the deployment role. Expose only the exporter port to the monitoring network. The package deliberately excludes credentials, private keys, wallet data and high-cardinality transaction/block identifiers from metric labels.

## Scope

This is a candidate/rehearsal operations baseline. It does not claim release authorization, public-testnet GO, public-testnet live status, Day 0, high-cadence authorization or smart-contract activation, and it does not start the 30-day public-testnet clock.

`GET /dashboard` summary may include observational `pulse` (`pulse_height`, `pulse_time`, `uncertainty_secs`, `finality_lag`, `selected_tip`) from PulseClock. The field is omitted when PulseClock cannot be derived. This is not host time and does not activate covenants.
