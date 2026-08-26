# PulseDAG v2.4.0 release-candidate observability

This directory is the canonical observability package for the v2.4.0 release-candidate and private launch-rehearsal line. It is an implementation/evidence package, not public-testnet authorization.

The package deliberately polls only RPC routes that exist in the `public_safe` profile:

- `GET /metrics`
- `GET /status`
- `GET /mempool`

It does **not** require `/admin/*`, `/runtime`, wallet routes, credentials, bearer tokens, private keys, seeds or operator secrets.

## Package contents

- `metrics-inventory.json`: versioned mapping from exact Rust response fields to Prometheus metrics.
- `prometheus-scrape.example.yml`: five-node private-rehearsal scrape baseline.
- `alert-rules.yml`: warning/critical thresholds for consensus commit/state, storage, mining submit finality, P2P recovery, sync convergence, RPC liveness and mempool pressure.
- `grafana-dashboard.json`: importable v2.4.0 dashboard baseline.

The exporter remains `scripts/private_testnet/runtime_metrics_exporter.py`. v2.3.0 remains supported for historical evidence; v2.4 operators select this inventory explicitly.

## Start one exporter per node

Keep node RPC on loopback. Expose the exporter only to the monitoring network:

```bash
python3 scripts/private_testnet/runtime_metrics_exporter.py \
  --node-url http://127.0.0.1:8080 \
  --inventory ops/observability/v2.4.0/metrics-inventory.json \
  --listen 0.0.0.0:9108 \
  --instance node-1
```

Verify one scrape without starting the HTTP server:

```bash
python3 scripts/private_testnet/runtime_metrics_exporter.py \
  --node-url http://127.0.0.1:8080 \
  --inventory ops/observability/v2.4.0/metrics-inventory.json \
  --instance node-1 \
  --once
```

## Required v2.4 alert classes

The package treats the following as hard-stop or immediate escalation surfaces during candidate freeze/rehearsal:

- accepted-commit publish mismatch or accepted-hash loss;
- invalid canonical state root or unavailable parent-state context;
- stable snapshot verification failure or replay gap;
- mining-submit actor timeout/queue saturation and prolonged pending submits;
- selected-tip mismatch or non-converged final-quiescence tips;
- sustained peer isolation or dropped recovery work;
- current RPC liveness degradation;
- missing snapshot or excessive mempool orphan pressure.

The block-production alert is meaningful only while the launch rehearsal explicitly requires continuous external mining.

## Five-node rehearsal wiring

`prometheus-scrape.example.yml` contains five placeholder exporter targets. Replace the example DNS names only with the private failure-domain addresses recorded for the rehearsal. The file does not define public bootnodes, public DNS or Day 0.

## Validation

Run:

```bash
python3 scripts/validate_v2_4_0_observability.py
bash scripts/tests/test_v2_4_0_observability.sh
```

The validator checks:

- every inventory field against its actual Rust response struct;
- the inventory is restricted to `public_safe` routes;
- dashboard and alert references resolve to declared metrics;
- mandatory consensus/state/mining/P2P/sync/RPC alert classes exist;
- every alert runbook path exists;
- the Prometheus example contains exactly five rehearsal targets;
- the exporter explicitly supports both v2.3.0 and v2.4.0 inventories.

## Guardrails

This package does not set `public_testnet_ready=true`, does not start the 30-day clock, does not enable high cadence or smart contracts, and does not clear #803. Final public infrastructure, owners, DNS/TLS, bootnode peer IDs and launch timestamps remain external freeze/GO evidence.
