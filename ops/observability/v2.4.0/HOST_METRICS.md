# v2.4.0 host metrics contract

This file defines the host-level monitoring surface required by the v2.4.0 release-candidate observability bundle. It does not authorize public launch.

## Node exporter

Run Prometheus `node_exporter` on every seed, ordinary node and observer host. Bind it only to loopback or the private management/monitoring network; do not expose the exporter directly to the public Internet.

The example scrape job in `prometheus-scrape.example.yml` expects port `9100` and carries the same `network`, `node` and `role` labels as the PulseDAG application exporter.

At minimum, keep the standard filesystem collector enabled so these metrics are present:

- `node_filesystem_avail_bytes`
- `node_filesystem_size_bytes`

The `PulseDAGDiskPressure` rule evaluates the lowest free-space ratio across non-ephemeral filesystems on each host. Operators must additionally verify which filesystem backs `PULSEDAG_ROCKSDB_PATH` and the snapshot/evidence directories before launch and record that mapping in the launch evidence.

## Process restart signal

The application exporter already exposes `pulsedag_node_uptime_seconds` from public-safe `GET /status`. It is a monotonic uptime gauge between process starts. `PulseDAGUnexpectedRestart` treats a decrease in this series as a restart signal. Planned restart/recovery drills must be annotated in the evidence timeline; an unplanned restart is a #789/#794 stop condition.

## Lock-starvation signal

The public-safe RPC surface exposes liveness degradation and oldest in-flight handler age. `PulseDAGSharedStateLockStarvation` intentionally alerts conservatively when RPC liveness remains degraded while an RPC handler is older than five seconds. The node's fail-fast RPC diagnostics identify busy shared chain/runtime state and direct the operator to inspect long-running writers. Do not reinterpret this alert as proof of a specific writer without logs/runtime evidence.

## Mining finality signal

`pulsedag_mining_submit_actor_timeout_total` counts serialized mining-submit actor response timeouts. The RPC guard converts the post-enqueue timeout result into `submit_finality_unknown`, so `PulseDAGSubmitFinalityUnknown` alerts on any new occurrence and requires block-hash reconciliation before classification.

## Security boundary

Host exporters and Prometheus targets must not contain bearer tokens, wallet seeds, private keys or unrestricted admin endpoints. Monitoring configuration is sanitized evidence and does not set `public_testnet_ready`, start the 30-day clock, authorize high cadence, enable contracts or provide production custody.
