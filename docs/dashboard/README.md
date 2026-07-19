# PulseDAG operator dashboards

The active private-testnet observability package is versioned under:

- `ops/observability/v2.3.0/README.md`
- `ops/observability/v2.3.0/metrics-inventory.json`
- `ops/observability/v2.3.0/prometheus-scrape.example.yml`
- `ops/observability/v2.3.0/alert-rules.yml`
- `ops/observability/v2.3.0/grafana-dashboard.json`

The canonical exporter is:

- `scripts/private_testnet/runtime_metrics_exporter.py`

Validation commands:

```bash
python3 scripts/validate_v2_3_0_observability.py
bash scripts/tests/test_v2_3_0_observability.sh
```

The compatibility command below delegates to the same active validator:

```bash
python3 scripts/validate_observability_package.py
```

## Supported operator surfaces

The v2.3.0 package exports a bounded inventory from four stable read-only RPC endpoints:

- node, P2P, snapshot, and pruning state from `GET /status`;
- convergence, lag, consistency, and recovery state from `GET /sync/status`;
- main and orphan mempool state from `GET /tx/mempool`;
- PoW cadence and snapshot health from `GET /pow/health`.

Dashboard and alert metric references are checked against the versioned inventory, and inventory fields are checked against their Rust response structures.

## Security model

Run the exporter beside the node and keep node RPC bound to loopback. Expose only the exporter port to the private monitoring network. The package deliberately excludes private keys, tokens, wallet data, arbitrary peer identifiers, transaction identifiers, and block hashes from metric labels.

## Scope

This is a private-testnet operations baseline. It does not claim v2.3.0 release readiness, public-testnet readiness, or public-testnet live status, and it does not start the 30-day public-testnet clock.
