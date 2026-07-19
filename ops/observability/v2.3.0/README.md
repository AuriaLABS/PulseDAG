# PulseDAG private-testnet observability v2.3.0

This directory is the canonical observability package for the v2.3.0 private-testnet release line.

## Package contents

- `metrics-inventory.json`: versioned mapping from stable RPC fields to Prometheus metrics.
- `prometheus-scrape.example.yml`: five-node scrape baseline.
- `alert-rules.yml`: warning and critical alert thresholds with runbook links.
- `grafana-dashboard.json`: importable Grafana dashboard baseline.

The runtime exporter is `scripts/private_testnet/runtime_metrics_exporter.py`. It uses Python's standard library and polls only:

- `GET /status`
- `GET /sync/status`
- `GET /tx/mempool`
- `GET /pow/health`

## Start one exporter per node

Run the exporter on the same host as the node so RPC remains loopback-only:

```bash
python3 scripts/private_testnet/runtime_metrics_exporter.py \
  --node-url http://127.0.0.1:8280 \
  --listen 0.0.0.0:9108 \
  --instance node-1
```

Expose port `9108` only to the monitoring network. Do not expose the node's operator RPC port publicly.

Verify one collection without starting the HTTP server:

```bash
python3 scripts/private_testnet/runtime_metrics_exporter.py \
  --node-url http://127.0.0.1:8280 \
  --instance node-1 \
  --once
```

## Prometheus and Grafana

1. Copy `prometheus-scrape.example.yml` into the Prometheus configuration directory.
2. Replace the five example DNS names with monitoring-network addresses.
3. Place `alert-rules.yml` beside the Prometheus configuration.
4. Import `grafana-dashboard.json` into Grafana.
5. Confirm all five targets report `up == 1` and `pulsedag_exporter_scrape_success == 1`.

## Threshold baseline

| Surface | Warning | Critical |
|---|---|---|
| Exporter | — | unreachable for 2 minutes |
| RPC collection | — | incomplete for 1 minute |
| P2P | degraded for 3 minutes | zero peers for 3 minutes |
| Sync lag | more than 100 blocks for 5 minutes | more than 500 blocks for 5 minutes |
| Missing parents | backlog above 128 for 5 minutes | handled through sync consistency escalation |
| Mempool | orphan pool above 80% for 5 minutes | operator escalation if combined with consistency failure |
| Snapshot | missing for 15 minutes | replay gap above zero for 5 minutes |
| PoW cadence | below 30 or above 90 seconds for 10 minutes | operator escalation if block production stops |

These are private-testnet operating thresholds, not protocol consensus constants.

## Validation

Run:

```bash
python3 scripts/validate_v2_3_0_observability.py
bash scripts/tests/test_v2_3_0_observability.sh
```

The validator checks inventory fields against Rust response structures, validates every dashboard and alert metric reference, verifies runbook paths, and enforces exactly five example scrape targets.

## Guardrails

This package does not authorize a public testnet, a version bump, a release tag, or the start of the 30-day public-testnet clock. It does not expose private keys, operator tokens, administrative RPC endpoints, or wallet data as metric labels.
