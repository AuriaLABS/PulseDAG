# Mining Rehearsal v2.2.9 (External `pulsedag-miner`)

This runbook exercises external mining against rehearsal node A by default, with optional targets for node B/C.

## 1) Start rehearsal node A

```bash
scripts/v2_2_9_start_node_a.sh
```

## 2) Verify RPC is up

```bash
curl -fsS http://127.0.0.1:18080/health
curl -fsS http://127.0.0.1:18080/status
```

## 3) Request a mining template (sanity check)

```bash
curl -fsS http://127.0.0.1:18080/mining/template
```

## 4) Run external miner (defaults to node A)

```bash
scripts/v2_2_9_start_miner_node_a.sh
```

Environment overrides:

- `PULSEDAG_MINER_ADDRESS` (default: `rehearsal-node-a-miner`)
- `PULSEDAG_MINER_THREADS` (default: `nproc`)
- `PULSEDAG_NODE_RPC` (default from script target)
- `PULSEDAG_MINER_MAX_TRIES` (default: `0`)
- `PULSEDAG_MINER_SLEEP_MS` (default: `250`)

Example with overrides:

```bash
PULSEDAG_MINER_ADDRESS=rehearsal-custom-miner \
PULSEDAG_MINER_THREADS=4 \
PULSEDAG_MINER_MAX_TRIES=1000 \
PULSEDAG_MINER_SLEEP_MS=100 \
PULSEDAG_NODE_RPC=http://127.0.0.1:18080 \
scripts/v2_2_9_start_miner_node_a.sh
```

## 5) Observe mining progress

```bash
curl -fsS http://127.0.0.1:18080/status
curl -fsS http://127.0.0.1:18080/tips
curl -fsS http://127.0.0.1:18080/blocks/recent
```

Also tail node logs if needed:

```bash
scripts/v2_2_9_tail_logs.sh
```

## 6) Stop the miner

If running in the foreground, press `Ctrl+C`.

If backgrounded:

```bash
pkill -f pulsedag-miner
```

## 7) Point miner to node B/C (optional)

Use dedicated wrappers:

```bash
scripts/v2_2_9_start_miner_node_b.sh
scripts/v2_2_9_start_miner_node_c.sh
```

Or override directly:

```bash
PULSEDAG_NODE_RPC=http://127.0.0.1:18081 scripts/v2_2_9_start_miner_node_a.sh
PULSEDAG_NODE_RPC=http://127.0.0.1:18082 scripts/v2_2_9_start_miner_node_a.sh
```

## Notes

- Scripts fail fast with a clear error if `target/release/pulsedag-miner` is missing.
- Scripts fail fast if the target node RPC `/health` endpoint is unreachable.
- No pool logic is used in this rehearsal flow.
