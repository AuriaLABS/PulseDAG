# Ubuntu/VPS Rehearsal (v2.2.18)

This runbook defines a conservative Ubuntu/VPS rehearsal for v2.2.18.

Security defaults:

- RPC is bound to localhost only (`127.0.0.1`)
- no RPC exposure to the internet
- no pool logic
- no consensus changes
- no root required for script execution (except optional firewall setup by operator)

## Scenario

- Default: 3-node private shape (A/B/C)
- Optional: 5-node RC shape (A/B/C + D/E)
- External CPU miners (separate miner process(es), not embedded in nodes)

## Recommended host/network posture

- Prefer one VPS per node for realistic networking, or one larger VPS for rehearsal-only process isolation.
- Keep RPC on localhost; use SSH tunnels for remote inspection.
- Restrict P2P ports to known peer IPs where possible.

### Firewall recommendations (UFW example)

> Optional and operator-owned. These commands usually require root.

```bash
# deny by default
sudo ufw default deny incoming
sudo ufw default allow outgoing

# allow SSH from your admin IP only (replace CIDR)
sudo ufw allow from <admin-ip-or-cidr> to any port 22 proto tcp

# allow P2P from known peers only (replace IP and port)
sudo ufw allow from <peer-ip> to any port 30333 proto tcp

# keep RPC local-only by bind address; do not open RPC port in UFW
sudo ufw enable
sudo ufw status verbose
```

## Optional SSH tunnel for local RPC access

If you need to query node RPC from your laptop while keeping node RPC bound to `127.0.0.1` on VPS:

```bash
ssh -N -L 18080:127.0.0.1:18080 ubuntu@<vps-host>
```

Then locally query:

```bash
curl -sS http://127.0.0.1:18080/health
```

## Scripts

From repository root:

### 1) Start rehearsal

```bash
bash scripts/v2_2_18_start_vps_rehearsal.sh
```

Optional 5-node shape:

```bash
NODE_SHAPE=5 bash scripts/v2_2_18_start_vps_rehearsal.sh
```

### 2) Collect evidence

```bash
bash scripts/v2_2_18_collect_vps_evidence.sh
```

Optional custom node endpoints:

```bash
NODE_URLS="http://127.0.0.1:18080 http://127.0.0.1:28080" \
  bash scripts/v2_2_18_collect_vps_evidence.sh
```

### 3) Stop rehearsal

```bash
bash scripts/v2_2_18_stop_vps_rehearsal.sh
```

## Runtime directories and artifacts

Scripts create and use:

- `logs/`
- `run/`
- `artifacts/v2_2_18_private_rc/<run_id>/`

Artifacts include:

- endpoint JSON captures
- process metadata and PIDs
- git commit
- `VERSION`
- Cargo metadata where practical (`cargo metadata`)
- summary markdown

## Environment knobs

- `NODE_SHAPE=3|5` (default `3`)
- `RUN_ID=<timestamp-or-label>`
- `MINER_COUNT=<n>` (default `1`)
- `P2P_BASE_PORT=<port>` (default `30333`)
- `RPC_BASE_PORT=<port>` (default `18080`)

## Notes

- This rehearsal is intended for operator validation and evidence collection.
- Keep RPC bound to localhost only; do not expose RPC publicly.
