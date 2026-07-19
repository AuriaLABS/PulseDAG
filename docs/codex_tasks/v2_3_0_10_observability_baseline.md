# v2.3.0 Task 10 — Metrics, dashboards, and alert baseline

## Goal

Publish a self-contained and versioned observability package for five-node private-testnet operations without relying on missing v2.2 files or fields attributed to later release lines.

## Deliverables

- A v2.3.0 metric inventory grounded in Rust RPC response structures.
- A standard-library exporter that exposes Prometheus text format.
- A five-node Prometheus scrape example.
- Versioned alert rules with warning/critical severity and valid runbook links.
- An importable Grafana dashboard.
- A package validator that checks Rust fields and every dashboard/alert metric reference.
- Compatibility routing from the former observability validator.
- A deterministic RPC fixture regression and dedicated Actions evidence gate.
- English comments, diagnostics, and operator documentation.

## Acceptance criteria

1. Every inventory endpoint and field exists in the referenced Rust response structure.
2. Metric names are unique and include node, P2P, sync, mempool, snapshot/prune, and PoW surfaces.
3. The exporter unwraps the stable `ApiResponse.data` shape and emits valid Prometheus samples.
4. Endpoint failures set `pulsedag_exporter_scrape_success` to zero and return non-zero in one-shot mode.
5. The scrape example contains exactly five unique private-testnet exporter targets.
6. Grafana expressions reference only metrics present in the versioned inventory or exporter built-ins.
7. Alert expressions reference only defined metrics and every alert runbook exists.
8. The active dashboard documentation references only real v2.3.0 package paths.
9. Repository hygiene, Lint, RPC/release, and pre-burn-in checks pass.

## Guardrails

- Node RPC remains loopback-only; only the exporter is exposed to the private monitoring network.
- No private keys, tokens, wallet data, block hashes, transaction identifiers, or arbitrary peer identifiers become metric labels.
- Alert thresholds are operating baselines, not consensus constants.
- No version bump, release tag, public-testnet launch, readiness claim, or 30-day clock start.

## Follow-up

Task 11 publishes operator and incident runbooks tied to these alert surfaces and the Task 09 lifecycle controller.
