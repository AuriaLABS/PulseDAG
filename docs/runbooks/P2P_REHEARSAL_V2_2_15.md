# PulseDAG v2.2.15 local P2P rehearsal runbook

This runbook describes the local, sustained, multi-node P2P rehearsal harness for the v2.2.15 operational-readiness milestone. The rehearsal is intended to produce reproducible evidence that several `pulsedagd` processes can run together with real libp2p networking before the v2.3.0 private-testnet readiness decision.

## Scope and safety

The rehearsal harness is operational only:

- It does **not** deploy or add smart contracts.
- It does **not** add pool logic.
- It does **not** change consensus rules.
- It starts local `pulsedagd` processes with `PULSEDAG_P2P_MODE=libp2p-real`.
- It does not use skeleton, memory, simulated, or dev-loopback P2P mode for the main rehearsal.
- It writes data under `evidence/v2.2.15/p2p-rehearsal/` by default and marks runtime directories before the cleanup script can remove them.

## Prerequisites

Run from the repository root. The scripts require:

- `cargo`
- `curl`
- `python3`
- a built `pulsedagd` binary at `target/release/pulsedagd`, `target/debug/pulsedagd`, or a custom path in `PULSEDAGD_BIN`

Recommended preflight commands:

```bash
cargo fmt --all -- --check
cargo test --workspace
cargo build --workspace
```

For a release binary, run:

```bash
cargo build --workspace --release
```

## Run the 3-node rehearsal

From a clean clone with dependencies installed, build the workspace and run:

```bash
cargo build --workspace
bash scripts/v2-2-15-p2p-3node-rehearsal.sh
```

The 3-node harness starts `node-01`, `node-02`, and `node-03` with:

- the same chain id, defaulting to `pulsedag-rehearsal-v2-2-15`
- unique RPC ports, defaulting to `19080`, `19081`, and `19082`
- unique P2P ports, defaulting to `19180`, `19181`, and `19182`
- isolated RocksDB storage directories per node
- isolated log files per node
- real libp2p mode (`libp2p-real`)

The script waits for RPC startup, waits for peer connectivity, captures initial evidence, keeps the rehearsal running for the sustained window, captures final evidence, and prints `PASS` or `FAIL` sections.

## Run the 5-node rehearsal

From the repository root:

```bash
cargo build --workspace
bash scripts/v2-2-15-p2p-5node-rehearsal.sh
```

The 5-node wrapper uses the same harness and sets `PULSEDAG_REHEARSAL_NODE_COUNT=5`. It starts `node-01` through `node-05` with unique RPC/P2P ports and isolated storage/logs.

## Useful overrides

All overrides are optional:

| Variable | Default | Purpose |
| --- | --- | --- |
| `PULSEDAGD_BIN` | `target/release/pulsedagd`, then `target/debug/pulsedagd` | Use a specific node binary. |
| `PULSEDAG_REHEARSAL_CHAIN_ID` | `pulsedag-rehearsal-v2-2-15` | Chain id shared by all rehearsal nodes. |
| `PULSEDAG_REHEARSAL_RPC_BASE_PORT` | `19080` | First RPC port; later nodes increment by 1. |
| `PULSEDAG_REHEARSAL_P2P_BASE_PORT` | `19180` | First P2P port; later nodes increment by 1. |
| `PULSEDAG_REHEARSAL_DURATION_SECS` | `60` | Sustained observation window after startup evidence. |
| `PULSEDAG_REHEARSAL_EVIDENCE_ROOT` | `evidence/v2.2.15/p2p-rehearsal` | Evidence output root. |
| `PULSEDAG_REHEARSAL_KEEP_RUNNING` | `0` | Set to `1` to leave nodes running after the script exits. |

Example longer run:

```bash
PULSEDAG_REHEARSAL_DURATION_SECS=900 \
  bash scripts/v2-2-15-p2p-5node-rehearsal.sh
```

## Inspect logs and evidence

Each run creates a timestamped evidence directory:

```text
evidence/v2.2.15/p2p-rehearsal/<run-id>/
```

Expected files and directories include:

```text
rehearsal-config.json
summary.txt
logs/node-01.log
logs/node-02.log
logs/node-03.log
node-01/initial/health.json
node-01/initial/status.json
node-01/initial/p2p-status.json
node-01/initial/p2p-peers.json
node-01/initial/tips.json
node-01/initial/dag.json
node-01/initial/admin-dag-consistency.json
node-01/final/health.json
node-01/final/status.json
node-01/final/p2p-status.json
node-01/final/p2p-peers.json
node-01/final/tips.json
node-01/final/dag.json
node-01/final/admin-dag-consistency.json
runtime/data/node-01/rocksdb/
```

For a 5-node run, expect the same per-node evidence for `node-04` and `node-05`.

Inspect the final summary:

```bash
cat evidence/v2.2.15/p2p-rehearsal/<run-id>/summary.txt
```

Inspect node logs:

```bash
tail -100 evidence/v2.2.15/p2p-rehearsal/<run-id>/logs/node-01.log
tail -100 evidence/v2.2.15/p2p-rehearsal/<run-id>/logs/node-02.log
tail -100 evidence/v2.2.15/p2p-rehearsal/<run-id>/logs/node-03.log
```

Inspect final P2P status:

```bash
python3 -m json.tool evidence/v2.2.15/p2p-rehearsal/<run-id>/node-01/final/p2p-status.json
```

## Confirm all nodes converge

A passing run requires the final convergence checks to confirm that every node reports:

- `chain_id` equal to the configured rehearsal chain id
- `p2p_mode` equal to `libp2p-real`
- `connected_peers_are_real_network` equal to `true`
- the same `best_height`
- the same `selected_tip`

You can manually compare the final status files:

```bash
python3 - <<'PY'
import glob, json
for path in sorted(glob.glob('evidence/v2.2.15/p2p-rehearsal/*/node-*/final/status.json')):
    with open(path, encoding='utf-8') as f:
        data = json.load(f)['data']
    print(path, data['chain_id'], data['best_height'], data['selected_tip'], data['p2p_mode'], data['peer_count'])
PY
```

## Clean rehearsal state

To stop any recorded rehearsal processes and remove only marked v2.2.15 runtime data directories:

```bash
bash scripts/v2-2-15-p2p-clean-rehearsal-data.sh
```

The cleanup script intentionally skips unmarked directories. It does not remove `summary.txt`, JSON evidence, or copied logs from completed runs; it removes the marked `runtime/` directory that contains node data and PID files.

## Known limitations

- This is a local single-host rehearsal; it does not prove WAN behavior, NAT traversal, firewall handling, or public testnet operations.
- The default rehearsal does not mine blocks. Convergence is therefore based on shared genesis state, selected tip, height, and peer connectivity unless an operator runs additional workload during a `PULSEDAG_REHEARSAL_KEEP_RUNNING=1` session.
- Peer connectivity depends on local port availability. Override the base ports if they are already in use.
- The harness validates operational evidence only; it is not a consensus, performance, or security certification.
