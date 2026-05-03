# Local Multi-node Rehearsal (v2.2.9)

> This setup is for **local rehearsal only**. It is **not** an official private testnet environment.

## Scripts

- `scripts/v2_2_9_start_node_a.sh`
- `scripts/v2_2_9_start_node_b.sh`
- `scripts/v2_2_9_start_node_c.sh`
- `scripts/v2_2_9_status.sh`
- `scripts/v2_2_9_tail_logs.sh`
- `scripts/v2_2_9_stop_all.sh`

State is stored in:

- PID files: `.pulsedag-rehearsal/*.pid`
- Logs: `.pulsedag-rehearsal/logs/*.log`
- Data directories: `data/rehearsal-{a,b,c}`

## Build

The scripts expect `target/release/pulsedagd`.

```bash
cargo build --release
```

## Start nodes

```bash
./scripts/v2_2_9_start_node_a.sh
./scripts/v2_2_9_start_node_b.sh
./scripts/v2_2_9_start_node_c.sh
```

Node assignment:

- Node A: network `rehearsal-a`, RPC `127.0.0.1:18080`, P2P `0.0.0.0:18181`
- Node B: network `rehearsal-b`, RPC `127.0.0.1:18081`, P2P `0.0.0.0:18182`
- Node C: network `rehearsal-c`, RPC `127.0.0.1:18082`, P2P `0.0.0.0:18183`

`node-b` and `node-c` attempt bootnode wiring to prior nodes using `--bootnode`. If the binary does not support this flag, adjust flags for your build and restart.

## Check status

```bash
./scripts/v2_2_9_status.sh
```

The status script probes these endpoints (where available):

- `/health`
- `/status`
- `/tips`
- `/pow`
- `/runtime`
- `/metrics`
- `/p2p/status`

## Tail logs

```bash
./scripts/v2_2_9_tail_logs.sh
```

## Stop / reset

Stop all tracked nodes gracefully:

```bash
./scripts/v2_2_9_stop_all.sh
```

Optional cleanup for a fresh rehearsal:

```bash
rm -rf .pulsedag-rehearsal data/rehearsal-a data/rehearsal-b data/rehearsal-c
```

## Known limitations

- CLI flags can drift across builds; bootnode flags are best-effort and may require adjustment.
- Endpoint availability can differ by runtime profile; `404` in status output is treated as "not exposed".
- Scripts are local-node helpers and do not replace operator runbooks.

## v2.3.0 handoff

For v2.3.0, carry these scripts forward by:

1. Verifying final `pulsedagd` CLI flags for network/RPC/P2P/bootnodes.
2. Updating port allocations if the v2.3.0 plan reserves different ranges.
3. Extending status probes to any new observability endpoints.
4. Keeping this workflow marked as rehearsal unless promoted by release governance.
