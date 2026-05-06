# Smoke Test v2.2.11 — Three-Node P2P Completion

This smoke test verifies the minimum reproducible path for closing v2.2.11 as the P2P completion release. It uses real `libp2p-real` network mode and the external `pulsedag-miner`; it does not introduce smart contracts, pool logic, or private-testnet readiness claims.

## Prerequisites

- Run from the repository root.
- Ensure `cargo`, `curl`, and `python3` are available.
- Ensure local ports `18080`, `18081`, `18082`, `18181`, `18182`, and `18183` are free, or override them with the `PULSEDAG_NODE_*` variables documented in `docs/P2P_REHEARSAL_V2_2_11.md`.

## Automated path

The preferred closeout command is:

```bash
scripts/v2_2_11_smoke_p2p.sh
```

The script performs the required v2.2.11 rehearsal sequence: build release binaries, start three nodes, verify peer connectivity, run the external miner against node A, wait for height increase, verify B/C convergence, restart B, verify B catch-up, and print final status.

## Manual equivalent

### 1. Build release binaries

```bash
cargo build --workspace --release
```

Confirm both binaries exist:

```bash
test -x target/release/pulsedagd
test -x target/release/pulsedag-miner
```

### 2. Start Node A

```bash
scripts/v2_2_11_start_node_a.sh --clean
```

Collect baseline status:

```bash
curl -fsS http://127.0.0.1:18080/health
curl -fsS http://127.0.0.1:18080/status
curl -fsS http://127.0.0.1:18080/p2p/status
curl -fsS http://127.0.0.1:18080/sync/status
```

Expected results:

- `/status` reports `version: "v2.2.11"`.
- `/status` and `/p2p/status` report real `libp2p-real` mode once P2P is enabled.
- Node A is listening for P2P peers.

### 3. Start Node B connected to A

```bash
scripts/v2_2_11_start_node_b.sh --clean
```

Verify B health and peer state:

```bash
curl -fsS http://127.0.0.1:18081/health
curl -fsS http://127.0.0.1:18081/status
curl -fsS http://127.0.0.1:18081/p2p/status
curl -fsS http://127.0.0.1:18081/sync/status
```

Expected result: node B observes at least one real P2P peer.

### 4. Start Node C connected to A/B

```bash
scripts/v2_2_11_start_node_c.sh --clean
```

Verify C health and peer state:

```bash
curl -fsS http://127.0.0.1:18082/health
curl -fsS http://127.0.0.1:18082/status
curl -fsS http://127.0.0.1:18082/p2p/status
curl -fsS http://127.0.0.1:18082/sync/status
```

Expected result: node C observes at least one real P2P peer and can converge through A/B connectivity.

### 5. Verify peer_count

Collect peer counts from all nodes:

```bash
curl -fsS http://127.0.0.1:18080/p2p/status
curl -fsS http://127.0.0.1:18081/p2p/status
curl -fsS http://127.0.0.1:18082/p2p/status
```

Expected result: each node reports real network mode and peer observations consistent with the A/B/C topology.

### 6. Run external miner against A

```bash
scripts/v2_2_11_start_miner_a.sh
```

The miner must remain external. Do not add embedded miner pool logic to satisfy this test.

### 7. Wait for height increase

Poll node A until height increases:

```bash
curl -fsS http://127.0.0.1:18080/status
```

Expected result: node A `best_height` becomes greater than the starting height after it mines or accepts a block.

### 8. Verify B/C convergence

Poll B and C until both reach node A height:

```bash
curl -fsS http://127.0.0.1:18081/status
curl -fsS http://127.0.0.1:18082/status
curl -fsS http://127.0.0.1:18081/sync/status
curl -fsS http://127.0.0.1:18082/sync/status
```

Expected result: node B and node C receive or sync the block from node A and report useful sync state while converging.

### 9. Restart B

Stop B through the tracked rehearsal process and start it again:

```bash
source scripts/v2_2_11_common.sh
stop_node b
scripts/v2_2_11_start_node_b.sh
```

### 10. Verify B catch-up

Poll B after restart:

```bash
curl -fsS http://127.0.0.1:18081/health
curl -fsS http://127.0.0.1:18081/status
curl -fsS http://127.0.0.1:18081/p2p/status
curl -fsS http://127.0.0.1:18081/sync/status
```

Expected result: node B reconnects, reports real network mode, and catches up to node A height.

### 11. Collect final diagnostics

Collect the following from every node before cleanup:

```bash
for port in 18080 18081 18082; do
  curl -fsS "http://127.0.0.1:${port}/health"
  curl -fsS "http://127.0.0.1:${port}/status"
  curl -fsS "http://127.0.0.1:${port}/p2p/status"
  curl -fsS "http://127.0.0.1:${port}/sync/status"
done
```

Archive node logs and miner logs from `.pulsedag-v2_2_11-rehearsal/logs/` with the release closeout evidence.

## Required pass criteria

- Release binaries build successfully.
- Node A/B/C connect in real P2P mode.
- `peer_count` or peer observations show live network connectivity.
- Node A mines or accepts at least one block through the external miner path.
- Node B and Node C converge to node A height.
- Node B catches up after restart.
- `/health`, `/status`, `/p2p/status`, and `/sync/status` are collected for all nodes.
- Duplicate suppression, invalid peer block rejection, and `chain_id` mismatch dropping are verified through unit/integration checks, counters, logs, or targeted rehearsal evidence.
