# Multi-host Operator Rehearsal v2.2.12

This runbook launches the v2.2.12 A/B/C rehearsal across three Ubuntu hosts plus an external miner process. It uses the existing v2.2.12 shell launchers and keeps the scope to operator rehearsal, evidence collection, cleanup, and failure diagnosis.

v2.2.12 is a hardening rehearsal. It does **not** claim v2.3.0 readiness.

## Scope and non-goals

In scope:

- Three real `pulsedagd` nodes using `libp2p-real`.
- One external `pulsedag-miner` process pointed at node A RPC through a private or explicitly protected path.
- Ubuntu/UFW examples for exposing P2P while keeping RPC private.
- Evidence capture with the existing v2.2.12 collector.

Out of scope:

- Docker requirements or container orchestration.
- Mining-pool coordination or pool logic.
- Smart contracts.
- Any claim that v2.2.12 is v2.3.0-ready.

## Server roles

| Host | Role | Process | Script | Default RPC bind | Default P2P listen |
| --- | --- | --- | --- | --- | --- |
| Server A | Bootnode, source of the bootnode multiaddr, miner target | node A | `scripts/v2_2_12_start_node_a.sh` | `127.0.0.1:18080` | `/ip4/0.0.0.0/tcp/18181` |
| Server B | Peer and restart/rejoin candidate | node B | `scripts/v2_2_12_start_node_b.sh` | `127.0.0.1:18081` | `/ip4/0.0.0.0/tcp/18182` |
| Server C | Peer and independent convergence observer | node C | `scripts/v2_2_12_start_node_c.sh` | `127.0.0.1:18082` | `/ip4/0.0.0.0/tcp/18183` |
| Miner host | External miner operator terminal | miner A | `scripts/v2_2_12_start_miner_a.sh` | Uses node A RPC URL | None |

Node B and node C must dial node A's **P2P** address. The bootnode value must point to node A's P2P port (`18181` in the scripted defaults), never node A's RPC port (`18080`).

## Required binaries and tools

Run on each server before starting its role:

```bash
cd /opt/PulseDAG
cargo build --workspace --release

test -x target/release/pulsedagd
test -x target/release/pulsedag-miner
```

The node scripts require `target/release/pulsedagd` unless `PULSEDAGD_BIN` is overridden. The miner script requires `target/release/pulsedag-miner` unless `PULSEDAG_MINER_BIN` is overridden.

Operational tools used by this runbook:

```bash
command -v curl
command -v python3
command -v tar
command -v ssh
```

## Network security rules

### Strong defaults

- Keep every node RPC listener bound to localhost (`127.0.0.1:<port>`) unless it is explicitly protected by SSH tunneling, VPN, or firewall allowlists.
- Do **not** publish RPC ports (`18080`, `18081`, `18082`) to the public internet.
- P2P ports must be reachable between Server A, Server B, and Server C.
- The bootnode multiaddr passed to B/C must be node A's reachable P2P multiaddr, for example `/ip4/<SERVER_A_PRIVATE_IP>/tcp/18181/p2p/<NODE_A_PEER_ID>`.
- Never use `http://<server-a>:18080`, `<server-a>:18080`, or `/tcp/18080` as a bootnode. `18080` is RPC in the v2.2.12 scripts, not P2P.

### Ubuntu/UFW example

Replace the example IPs with your private host addresses. If hosts communicate over a VPN, use VPN IPs. If they communicate over a cloud private network, use private NIC IPs.

Server A exposes only its P2P port to B and C:

```bash
sudo ufw default deny incoming
sudo ufw default allow outgoing
sudo ufw allow OpenSSH
sudo ufw allow from <SERVER_B_PRIVATE_IP> to any port 18181 proto tcp
sudo ufw allow from <SERVER_C_PRIVATE_IP> to any port 18181 proto tcp
sudo ufw deny 18080/tcp
sudo ufw enable
sudo ufw status verbose
```

Server B exposes only its P2P port to A and C:

```bash
sudo ufw default deny incoming
sudo ufw default allow outgoing
sudo ufw allow OpenSSH
sudo ufw allow from <SERVER_A_PRIVATE_IP> to any port 18182 proto tcp
sudo ufw allow from <SERVER_C_PRIVATE_IP> to any port 18182 proto tcp
sudo ufw deny 18081/tcp
sudo ufw enable
sudo ufw status verbose
```

Server C exposes only its P2P port to A and B:

```bash
sudo ufw default deny incoming
sudo ufw default allow outgoing
sudo ufw allow OpenSSH
sudo ufw allow from <SERVER_A_PRIVATE_IP> to any port 18183 proto tcp
sudo ufw allow from <SERVER_B_PRIVATE_IP> to any port 18183 proto tcp
sudo ufw deny 18082/tcp
sudo ufw enable
sudo ufw status verbose
```

If the miner runs on a separate host, prefer an SSH tunnel to node A RPC instead of opening RPC:

```bash
ssh -N -L 18080:127.0.0.1:18080 <USER>@<SERVER_A_PRIVATE_IP>
```

Only if SSH/VPN tunneling is unavailable, bind node A RPC to a private interface and allowlist the miner host explicitly. Treat this as a protected exception, not the default:

```bash
sudo ufw allow from <MINER_PRIVATE_IP> to any port 18080 proto tcp
```

## Shared environment values

Use the same chain and P2P mode on all hosts:

```bash
export PULSEDAG_REHEARSAL_CHAIN_ID=pulsedag-rehearsal-v2-2-12
export PULSEDAG_REHEARSAL_NETWORK=private
export PULSEDAG_REHEARSAL_P2P_MODE=libp2p-real
export PULSEDAG_REHEARSAL_STATE_DIR=/var/tmp/pulsedag-v2_2_12-rehearsal
export PULSEDAG_REHEARSAL_LOG_DIR=/var/tmp/pulsedag-v2_2_12-rehearsal/logs
export PULSEDAG_REHEARSAL_DATA_ROOT=/var/tmp/pulsedag-v2_2_12-rehearsal/data
```

Optional binary overrides, if the binaries are not under the repository `target/release` directory:

```bash
export PULSEDAGD_BIN=/opt/PulseDAG/target/release/pulsedagd
export PULSEDAG_MINER_BIN=/opt/PulseDAG/target/release/pulsedag-miner
```

## Per-host environment

### Server A: node A bootnode and miner target

Keep RPC local. Listen for P2P on all interfaces or on the private/VPN interface selected by your environment.

```bash
export PULSEDAG_NODE_A_RPC=127.0.0.1:18080
export PULSEDAG_NODE_A_P2P=/ip4/0.0.0.0/tcp/18181
```

Start node A first, then read its peer id locally:

```bash
cd /opt/PulseDAG
scripts/v2_2_12_start_node_a.sh --clean
curl -fsS http://127.0.0.1:18080/health
curl -fsS http://127.0.0.1:18080/p2p/status
```

Extract node A's peer id:

```bash
NODE_A_PEER_ID=$(curl -fsS http://127.0.0.1:18080/p2p/status | python3 -c 'import json,sys; print(json.load(sys.stdin)["data"].get("peer_id", ""))')
echo "$NODE_A_PEER_ID"
```

Build the bootnode value for B/C from node A's P2P address:

```bash
export PULSEDAG_NODE_A_BOOTNODE=/ip4/<SERVER_A_PRIVATE_IP>/tcp/18181/p2p/$NODE_A_PEER_ID
echo "$PULSEDAG_NODE_A_BOOTNODE"
```

### Server B: node B restart/rejoin candidate

Use the same `PULSEDAG_NODE_A_BOOTNODE` value produced on Server A. It must point to node A P2P port `18181`.

```bash
export PULSEDAG_NODE_B_RPC=127.0.0.1:18081
export PULSEDAG_NODE_B_P2P=/ip4/0.0.0.0/tcp/18182
export PULSEDAG_NODE_A_BOOTNODE=/ip4/<SERVER_A_PRIVATE_IP>/tcp/18181/p2p/<NODE_A_PEER_ID>
```

### Server C: independent convergence observer

Use the same node A P2P bootnode value.

```bash
export PULSEDAG_NODE_C_RPC=127.0.0.1:18082
export PULSEDAG_NODE_C_P2P=/ip4/0.0.0.0/tcp/18183
export PULSEDAG_NODE_A_BOOTNODE=/ip4/<SERVER_A_PRIVATE_IP>/tcp/18181/p2p/<NODE_A_PEER_ID>
```

### Miner host: external miner

Preferred private path: create an SSH tunnel from the miner host to Server A's localhost RPC, then point the miner at the local tunnel endpoint.

```bash
export PULSEDAG_MINER_NODE_URL=http://127.0.0.1:18080
export PULSEDAG_MINER_ADDRESS=pulsedag-rehearsal-miner-a
export PULSEDAG_MINER_THREADS=2
export PULSEDAG_MINER_MAX_TRIES=500000
export PULSEDAG_MINER_SLEEP_MS=500
export PULSEDAG_MINER_REFRESH_BEFORE_EXPIRY_MS=1000
```

If node A RPC is explicitly protected on a private network instead of tunneled, set `PULSEDAG_MINER_NODE_URL` to that protected URL, for example `http://<SERVER_A_PRIVATE_IP>:18080`.

## Start order

Start exactly in this order so B/C have a real bootnode and the miner submits work only after the network is observable.

### 1. Start node A on Server A

```bash
cd /opt/PulseDAG
scripts/v2_2_12_start_node_a.sh --clean
curl -fsS http://127.0.0.1:18080/health
curl -fsS http://127.0.0.1:18080/p2p/status
```

Record `PULSEDAG_NODE_A_BOOTNODE` as `/ip4/<SERVER_A_PRIVATE_IP>/tcp/18181/p2p/<NODE_A_PEER_ID>`.

### 2. Start node B on Server B

```bash
cd /opt/PulseDAG
scripts/v2_2_12_start_node_b.sh --clean
curl -fsS http://127.0.0.1:18081/health
curl -fsS http://127.0.0.1:18081/p2p/status
```

### 3. Start node C on Server C

```bash
cd /opt/PulseDAG
scripts/v2_2_12_start_node_c.sh --clean
curl -fsS http://127.0.0.1:18082/health
curl -fsS http://127.0.0.1:18082/p2p/status
```

### 4. Start the external miner

On the miner host, after the SSH tunnel or protected RPC path is ready:

```bash
cd /opt/PulseDAG
scripts/v2_2_12_start_miner_a.sh
```

## Operator checks during the run

Run local checks on each node host or through SSH tunnels:

```bash
curl -fsS http://127.0.0.1:<RPC_PORT>/health
curl -fsS http://127.0.0.1:<RPC_PORT>/status
curl -fsS http://127.0.0.1:<RPC_PORT>/p2p/status
curl -fsS http://127.0.0.1:<RPC_PORT>/sync/status
curl -fsS http://127.0.0.1:<RPC_PORT>/p2p/peers
curl -fsS http://127.0.0.1:<RPC_PORT>/p2p/propagation
```

Use `18080` for node A, `18081` for node B, and `18082` for node C. Peer observations should become non-empty after B/C dial A. Heights should converge after the miner advances node A.

## Evidence collection

The evidence collector queries all three RPC endpoints from one machine. Keep RPC private by collecting from a bastion/operator workstation with SSH tunnels:

```bash
ssh -N -L 18080:127.0.0.1:18080 <USER>@<SERVER_A_PRIVATE_IP>
ssh -N -L 18081:127.0.0.1:18081 <USER>@<SERVER_B_PRIVATE_IP>
ssh -N -L 18082:127.0.0.1:18082 <USER>@<SERVER_C_PRIVATE_IP>
```

Then run the collector from the repository checkout on that workstation:

```bash
cd /opt/PulseDAG
export PULSEDAG_NODE_A_RPC=127.0.0.1:18080
export PULSEDAG_NODE_B_RPC=127.0.0.1:18081
export PULSEDAG_NODE_C_RPC=127.0.0.1:18082
export PULSEDAG_NODE_A_BOOTNODE=/ip4/<SERVER_A_PRIVATE_IP>/tcp/18181/p2p/<NODE_A_PEER_ID>
scripts/v2_2_12_collect_evidence.sh
```

Also preserve per-host logs and metadata because the collector can only copy logs from its local `PULSEDAG_REHEARSAL_LOG_DIR`:

```bash
tar -C /var/tmp -czf pulsedag-v2_2_12-server-a-logs.tgz pulsedag-v2_2_12-rehearsal/logs
sha256sum pulsedag-v2_2_12-server-a-logs.tgz
```

Repeat the log archive command on Server B, Server C, and the miner host. Add the archives, command transcript, firewall status, bootnode value, git commit, and binary versions to the closeout evidence directory.

## Cleanup

Stop the miner first, then C, B, and A. The v2.2.12 shared script has `stop_all_v2_2_12`, but on separate hosts each operator should stop only the process running on that host.

Miner host:

```bash
source /opt/PulseDAG/scripts/v2_2_12_common.sh
stop_miner_a
```

Server C:

```bash
source /opt/PulseDAG/scripts/v2_2_12_common.sh
stop_node c
```

Server B:

```bash
source /opt/PulseDAG/scripts/v2_2_12_common.sh
stop_node b
```

Server A:

```bash
source /opt/PulseDAG/scripts/v2_2_12_common.sh
stop_node a
```

Remove rehearsal state only after evidence is archived and reviewed:

```bash
rm -rf /var/tmp/pulsedag-v2_2_12-rehearsal
```

## Failure diagnosis

### B/C do not connect to A

Check the bootnode first:

```bash
echo "$PULSEDAG_NODE_A_BOOTNODE"
```

It must contain node A's P2P port, for example `/tcp/18181`, and should include `/p2p/<NODE_A_PEER_ID>` for cross-host dialing. If it contains `/tcp/18080`, it is pointing at RPC and must be replaced.

Check P2P reachability from B/C to A:

```bash
nc -vz <SERVER_A_PRIVATE_IP> 18181
```

Check node A firewall and listener:

```bash
sudo ufw status verbose
ss -ltnp | rg '18181|18080'
```

Review local P2P status and logs:

```bash
curl -fsS http://127.0.0.1:<RPC_PORT>/p2p/status
tail -200 /var/tmp/pulsedag-v2_2_12-rehearsal/logs/node-<a|b|c>.log
```

### RPC is unreachable

Confirm the RPC listener is intentionally private:

```bash
ss -ltnp | rg '18080|18081|18082'
```

If RPC is bound to `127.0.0.1`, use SSH tunneling for remote checks. Do not open RPC publicly to make diagnostics easier.

### Miner does not advance height

Verify the miner can reach node A through the tunnel or protected RPC URL:

```bash
curl -fsS "$PULSEDAG_MINER_NODE_URL/health"
tail -200 /var/tmp/pulsedag-v2_2_12-rehearsal/logs/miner-a.log
```

Confirm the miner command matches the v2.2.12 script flags: `--node`, `--miner-address`, `--threads`, `--max-tries`, `--loop`, `--sleep-ms`, and `--refresh-before-expiry-ms`.

### Heights diverge or sync stalls

Collect the status and sync endpoints on all nodes:

```bash
curl -fsS http://127.0.0.1:<RPC_PORT>/status
curl -fsS http://127.0.0.1:<RPC_PORT>/sync/status
curl -fsS http://127.0.0.1:<RPC_PORT>/sync/missing
curl -fsS http://127.0.0.1:<RPC_PORT>/p2p/propagation
```

Compare chain id, height, peer counts, orphan count, pending block requests, pending missing parents, and peer lifecycle/backoff state. Persistent orphan growth, missing parents, or chain-id mismatch is a rehearsal failure until explained in the evidence notes.

### Evidence collector fails

The collector treats `/health`, `/status`, `/p2p/status`, and `/sync/status` as required for A/B/C. If it fails:

1. Confirm SSH tunnels are still running.
2. Confirm `PULSEDAG_NODE_A_RPC`, `PULSEDAG_NODE_B_RPC`, and `PULSEDAG_NODE_C_RPC` point to the local tunnel ports.
3. Run the failing `curl` command manually.
4. Keep the failed archive and logs for diagnosis; do not relabel incomplete evidence as passing closeout evidence.
