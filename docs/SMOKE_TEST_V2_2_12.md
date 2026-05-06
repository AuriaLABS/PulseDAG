# Smoke Test v2.2.12 — Private-Testnet Rehearsal Baseline

This smoke test is the v2.2.12 documentation baseline for a full private-testnet rehearsal and hardening pass. It starts from the v2.2.11 three-node sequence, then requires operators to extend the run with longer duration, restart/rejoin checks, sync convergence review, diagnostics capture, and runbook notes.

This is a documentation-only rehearsal plan. v2.2.12 does **not** claim official private-testnet readiness; v2.3.0 remains the readiness decision milestone.

## Prerequisites

- Run from the repository root.
- Ensure `cargo`, `curl`, and `python3` are available.
- Ensure rehearsal RPC/P2P ports are free or documented for the selected hosts.
- Choose one shared chain id for all nodes.
- Assign operator roles for launch, monitoring, restart/rejoin, and evidence collection.
- Keep RPC private to operators; expose only the intended P2P ports between nodes.

## Automated baseline

Until v2.2.12-specific scripts are added, use the v2.2.11 smoke script as the baseline compatibility gate:

```bash
scripts/v2_2_11_smoke_p2p.sh
```

The baseline validates the inherited sequence: build release binaries, start A/B/C with real P2P, connect B/C to A, run the external miner against A, wait for height increase, verify B/C convergence, restart B, verify B catch-up, and print final status.

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

### 2. Start node A

Start node A with real P2P enabled and no bootnode. Record the command, environment, chain id, data directory, RPC port, and P2P multiaddr.

Baseline command when using existing scripts:

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

### 3. Start node B connected to A

```bash
scripts/v2_2_11_start_node_b.sh --clean
```

Verify B health, peer state, and sync state:

```bash
curl -fsS http://127.0.0.1:18081/health
curl -fsS http://127.0.0.1:18081/status
curl -fsS http://127.0.0.1:18081/p2p/status
curl -fsS http://127.0.0.1:18081/sync/status
```

### 4. Start node C connected to A/B

```bash
scripts/v2_2_11_start_node_c.sh --clean
```

Verify C health, peer state, and sync state:

```bash
curl -fsS http://127.0.0.1:18082/health
curl -fsS http://127.0.0.1:18082/status
curl -fsS http://127.0.0.1:18082/p2p/status
curl -fsS http://127.0.0.1:18082/sync/status
```

### 5. Verify peer observations

```bash
curl -fsS http://127.0.0.1:18080/p2p/status
curl -fsS http://127.0.0.1:18081/p2p/status
curl -fsS http://127.0.0.1:18082/p2p/status
```

Expected result: every node reports real network mode and peer observations consistent with the topology.

### 6. Run the external miner against A

```bash
scripts/v2_2_11_start_miner_a.sh
```

The miner must remain external. Do not add embedded pool logic to `pulsedag-miner` to satisfy this smoke test.

### 7. Wait for height increase

```bash
curl -fsS http://127.0.0.1:18080/status
```

Expected result: node A `best_height` becomes greater than the starting height after it mines or accepts a block.

### 8. Verify B/C convergence

```bash
curl -fsS http://127.0.0.1:18081/status
curl -fsS http://127.0.0.1:18082/status
curl -fsS http://127.0.0.1:18081/sync/status
curl -fsS http://127.0.0.1:18082/sync/status
```

Expected result: B and C receive or sync the block from A and expose useful sync state while converging.

### 9. Restart B and verify rejoin

```bash
source scripts/v2_2_11_common.sh
stop_node b
scripts/v2_2_11_start_node_b.sh
```

Collect post-restart evidence:

```bash
curl -fsS http://127.0.0.1:18081/health
curl -fsS http://127.0.0.1:18081/status
curl -fsS http://127.0.0.1:18081/p2p/status
curl -fsS http://127.0.0.1:18081/sync/status
```

Expected result: B reconnects, reports real network mode, and catches up to A height.

### 10. Extend the run for v2.2.12 hardening

After the baseline passes, keep the network running for the selected rehearsal window. Capture periodic snapshots and operator notes:

```bash
for port in 18080 18081 18082; do
  curl -fsS "http://127.0.0.1:${port}/status"
  curl -fsS "http://127.0.0.1:${port}/p2p/status"
  curl -fsS "http://127.0.0.1:${port}/p2p/propagation"
  curl -fsS "http://127.0.0.1:${port}/sync/status"
done
```

Optional hardening actions:

- Restart another non-bootnode and verify rejoin.
- Temporarily stop the external miner, restart it, and verify mining resumes through node RPC.
- Add an additional node or host and confirm convergence.
- Have a second operator collect evidence independently and compare results.

### 11. Collect final diagnostics

```bash
for port in 18080 18081 18082; do
  curl -fsS "http://127.0.0.1:${port}/health"
  curl -fsS "http://127.0.0.1:${port}/status"
  curl -fsS "http://127.0.0.1:${port}/p2p/status"
  curl -fsS "http://127.0.0.1:${port}/p2p/peers"
  curl -fsS "http://127.0.0.1:${port}/p2p/propagation"
  curl -fsS "http://127.0.0.1:${port}/sync/status"
  curl -fsS "http://127.0.0.1:${port}/sync/missing"
done
```

Archive node logs, miner logs, command transcripts, endpoint snapshots, and operator notes with the release closeout evidence.

## Required pass criteria

- Release binaries build successfully.
- Node A/B/C connect in real P2P mode.
- Peer observations show live network connectivity.
- Node A mines or accepts at least one block through the external miner path.
- Nodes B and C converge to node A height.
- Node B catches up after restart.
- The longer-running rehearsal captures repeated status, P2P, propagation, and sync snapshots.
- Diagnostics are reviewed for duplicate suppression, invalid peer block rejection, chain-id mismatch dropping, orphan state, missing parents, and peer backoff.
- `/health`, `/status`, `/p2p/status`, and `/sync/status` are collected for all nodes at baseline and closeout.
- Documentation and evidence preserve the v2.2.12 boundary: hardening only, readiness decision deferred to v2.3.0.
