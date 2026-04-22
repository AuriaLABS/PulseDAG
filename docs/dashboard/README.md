# Operator Dashboard Package (v2.1)

This folder contains the published operator dashboard package for the v2.1 readiness cycle.

## Contents
- `assets/pulsedag-operator-overview.json` — importable Grafana dashboard JSON.
- `config/datasource-prometheus.yml` — example datasource definition for dashboard compatibility.

## Recommended panels
The packaged dashboard includes panels for:
- chain best height
- block production cadence
- mempool depth
- peer count
- snapshot availability

## Usage
1. Import `assets/pulsedag-operator-overview.json` into Grafana.
2. Ensure Prometheus datasource name matches `PulseDAG-Prometheus` or update the dashboard datasource.
3. Point datasource to your node metrics endpoint and validate panels are populated.

## Related runbooks
- `docs/runbooks/INDEX.md`
- `docs/runbooks/P2P_RECOVERY.md`
- `docs/runbooks/SNAPSHOT_RESTORE.md`
