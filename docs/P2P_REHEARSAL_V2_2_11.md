# P2P Rehearsal v2.2.11: three real libp2p nodes

This runbook launches a reproducible three-node PulseDAG P2P rehearsal using the release binaries from this repository. It is intentionally local-process based: no Docker, smart-contract, mining-pool, or pool-coordination logic is required.

## What the launcher validates

The smoke script performs the following sequence:

1. Builds the full workspace with `cargo build --workspace --release`.
2. Starts node A, B, and C with real `libp2p-real` P2P mode.
3. Uses one shared chain id for all nodes.
4. Gives every node its own RocksDB directory and RPC/P2P ports.
5. Connects B and C to node A with the real `pulsedagd` `--bootnode` CLI flag.
6. Checks `/health` and `/p2p/status` on all nodes.
7. Verifies peers are connected.
8. Starts external `pulsedag-miner` against node A RPC with no pool logic.
9. Waits for node A height to become greater than zero.
10. Verifies B and C receive or sync the mined block.
11. Restarts B and verifies it catches up again.
12. Prints a final A/B/C status summary.

## Files

| File                               | Purpose                                                                             |
| ---------------------------------- | ----------------------------------------------------------------------------------- |
| `scripts/v2_2_11_common.sh`        | Shared defaults, process management, endpoint helpers, and real binary invocations. |
| `scripts/v2_2_11_start_node_a.sh`  | Starts node A.                                                                      |
| `scripts/v2_2_11_start_node_b.sh`  | Starts node B and passes node A as bootnode.                                        |
| `scripts/v2_2_11_start_node_c.sh`  | Starts node C and passes node A as bootnode.                                        |
| `scripts/v2_2_11_start_miner_a.sh` | Starts external miner against node A RPC.                                           |
| `scripts/v2_2_11_smoke_p2p.sh`     | End-to-end three-node rehearsal smoke test.                                         |

## Real CLI flags used

The node launcher uses only flags currently accepted by `pulsedagd`:

```bash
pulsedagd \
  --network private \
  --rpc-listen 127.0.0.1:18080 \
  --p2p-listen /ip4/0.0.0.0/tcp/18181 \
  --bootnode /ip4/127.0.0.1/tcp/18181
```

`--bootnode` is used only for nodes B and C. Node A starts without a bootnode.

The data directory and rehearsal networking mode are configured through existing `pulsedagd` environment variables:

```bash
PULSEDAG_ROCKSDB_PATH=...
PULSEDAG_CHAIN_ID=pulsedag-rehearsal-v2-2-11
PULSEDAG_P2P_ENABLED=true
PULSEDAG_P2P_MODE=libp2p-real
PULSEDAG_P2P_MDNS=false
```

The miner launcher uses only flags currently accepted by `pulsedag-miner`:

```bash
pulsedag-miner \
  --node http://127.0.0.1:18080 \
  --miner-address pulsedag-rehearsal-miner-a \
  --threads 2 \
  --max-tries 500000 \
  --loop \
  --sleep-ms 500 \
  --refresh-before-expiry-ms 1000
```

## Local usage

From the repository root:

```bash
scripts/v2_2_11_smoke_p2p.sh
```

By default the smoke script cleans the rehearsal data, starts all processes, validates the flow, and stops the processes on exit.

To keep the nodes and miner running after the smoke test:

```bash
PULSEDAG_REHEARSAL_KEEP_RUNNING=1 scripts/v2_2_11_smoke_p2p.sh
```

To start individual components manually:

```bash
cargo build --workspace --release
scripts/v2_2_11_start_node_a.sh --clean
scripts/v2_2_11_start_node_b.sh --clean
scripts/v2_2_11_start_node_c.sh --clean
scripts/v2_2_11_start_miner_a.sh
```

Health and P2P checks:

```bash
curl -fsS http://127.0.0.1:18080/health
curl -fsS http://127.0.0.1:18080/p2p/status
curl -fsS http://127.0.0.1:18081/health
curl -fsS http://127.0.0.1:18082/health
```

## Configuration knobs

All defaults can be overridden without editing scripts:

| Variable                          | Default                         | Meaning                                                     |
| --------------------------------- | ------------------------------- | ----------------------------------------------------------- |
| `PULSEDAGD_BIN`                   | `target/release/pulsedagd`      | Node binary path.                                           |
| `PULSEDAG_MINER_BIN`              | `target/release/pulsedag-miner` | Miner binary path.                                          |
| `PULSEDAG_REHEARSAL_STATE_DIR`    | `.pulsedag-v2_2_11-rehearsal`   | PID, logs, and data root.                                   |
| `PULSEDAG_REHEARSAL_CHAIN_ID`     | `pulsedag-rehearsal-v2-2-11`    | Shared chain id for A/B/C.                                  |
| `PULSEDAG_NODE_A_RPC`             | `127.0.0.1:18080`               | Node A RPC bind address.                                    |
| `PULSEDAG_NODE_B_RPC`             | `127.0.0.1:18081`               | Node B RPC bind address.                                    |
| `PULSEDAG_NODE_C_RPC`             | `127.0.0.1:18082`               | Node C RPC bind address.                                    |
| `PULSEDAG_NODE_A_P2P`             | `/ip4/0.0.0.0/tcp/18181`        | Node A P2P listen multiaddr.                                |
| `PULSEDAG_NODE_B_P2P`             | `/ip4/0.0.0.0/tcp/18182`        | Node B P2P listen multiaddr.                                |
| `PULSEDAG_NODE_C_P2P`             | `/ip4/0.0.0.0/tcp/18183`        | Node C P2P listen multiaddr.                                |
| `PULSEDAG_NODE_A_BOOTNODE`        | `/ip4/127.0.0.1/tcp/18181`      | Bootnode multiaddr supplied to B/C.                         |
| `PULSEDAG_MINER_ADDRESS`          | `pulsedag-rehearsal-miner-a`    | Miner payout/address string sent to node A.                 |
| `PULSEDAG_MINER_THREADS`          | `2`                             | External miner thread count.                                |
| `PULSEDAG_MINER_MAX_TRIES`        | `500000`                        | External miner max tries per template.                      |
| `PULSEDAG_REHEARSAL_KEEP_RUNNING` | `0`                             | Keep processes running after smoke if set to `1` or `true`. |

## External-server usage

Use the same scripts on separate servers by overriding bind and bootnode addresses.

On server A, expose P2P publicly and keep RPC private unless you intentionally secure and expose it:

```bash
export PULSEDAG_NODE_A_RPC=127.0.0.1:18080
export PULSEDAG_NODE_A_P2P=/ip4/0.0.0.0/tcp/18181
export PULSEDAG_REHEARSAL_CHAIN_ID=pulsedag-rehearsal-v2-2-11
scripts/v2_2_11_start_node_a.sh --clean
```

On server B:

```bash
export PULSEDAG_NODE_B_RPC=127.0.0.1:18081
export PULSEDAG_NODE_B_P2P=/ip4/0.0.0.0/tcp/18182
export PULSEDAG_NODE_A_BOOTNODE=/ip4/<SERVER_A_PUBLIC_IP>/tcp/18181
export PULSEDAG_REHEARSAL_CHAIN_ID=pulsedag-rehearsal-v2-2-11
scripts/v2_2_11_start_node_b.sh --clean
```

On server C:

```bash
export PULSEDAG_NODE_C_RPC=127.0.0.1:18082
export PULSEDAG_NODE_C_P2P=/ip4/0.0.0.0/tcp/18183
export PULSEDAG_NODE_A_BOOTNODE=/ip4/<SERVER_A_PUBLIC_IP>/tcp/18181
export PULSEDAG_REHEARSAL_CHAIN_ID=pulsedag-rehearsal-v2-2-11
scripts/v2_2_11_start_node_c.sh --clean
```

Start the external miner on server A or any host that can reach node A RPC:

```bash
export PULSEDAG_MINER_NODE_URL=http://127.0.0.1:18080
export PULSEDAG_MINER_ADDRESS=<YOUR_REHEARSAL_MINER_ADDRESS>
scripts/v2_2_11_start_miner_a.sh
```

For external servers, open only the P2P TCP port between rehearsal hosts unless you have a separate authenticated/filtered RPC exposure plan.

## Cleanup

The smoke script stops tracked processes automatically unless `PULSEDAG_REHEARSAL_KEEP_RUNNING=1` is set. For manually started processes, source the common script and stop all tracked processes:

```bash
source scripts/v2_2_11_common.sh
stop_all_v2_2_11
```

Remove rehearsal state and data:

```bash
rm -rf .pulsedag-v2_2_11-rehearsal
```

## Troubleshooting

- Logs are stored under `.pulsedag-v2_2_11-rehearsal/logs/` by default.
- If `/p2p/status` reports a mode other than `libp2p-real`, check `PULSEDAG_P2P_MODE` overrides in your environment.
- If B/C do not connect, verify `PULSEDAG_NODE_A_BOOTNODE` is reachable from those hosts and that firewalls allow node A's P2P TCP port.
- If mining does not advance height, increase `PULSEDAG_MINER_MAX_TRIES`, `PULSEDAG_REHEARSAL_MINE_WAIT_SECS`, or `PULSEDAG_MINER_THREADS` for slower hosts.
- If block propagation is visible but heights do not converge, inspect `/sync/status`, `/sync/missing`, `/orphans`, and `/p2p/propagation` on the lagging node.

## Height convergence verification

The rehearsal is successful when all nodes report the same chain id and converge to the same height after mining and restart catch-up:

```bash
for port in 18080 18081 18082; do
  echo "== $port /sync/status =="
  curl -fsS "http://127.0.0.1:${port}/sync/status"
  echo
  echo "== $port /p2p/status =="
  curl -fsS "http://127.0.0.1:${port}/p2p/status"
  echo
done
```

Minimum expected steady-state signals:

- `chain_id` matches on A/B/C.
- `/p2p/status.mode` is `libp2p-real`.
- Each node has at least one `connected_peers` entry or a recent recovery observation in the scripted smoke output.
- `/sync/status.best_height` converges across A/B/C.
- `/sync/status.pending_block_requests=0` after catch-up.
- `/sync/status.pending_missing_parents=0` after catch-up.
- `/sync/status.orphan_count=0` after catch-up.

## Related P2P documents

- Final protocol specification: `docs/P2P_SPEC_V2_2_11.md`.
- Sync recovery and troubleshooting guide: `docs/SYNC_RECOVERY_V2_2_11.md`.
- Version positioning and guardrails: `docs/VERSION_MATRIX.md`.
