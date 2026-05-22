# OPERATOR RUNBOOK v2.2.19

## Scope
Operator procedure for controlled multi-host private rehearsal and public testnet preparation for v2.2.19.

## Host setup (Ubuntu 24.04)
1. Create non-root operator account:
   ```bash
   sudo useradd -m -s /bin/bash pulsedag
   sudo usermod -aG sudo pulsedag
   ```
2. Install base packages:
   ```bash
   sudo apt update
   sudo apt install -y curl jq ufw tmux unzip
   ```
3. Create runtime directories:
   ```bash
   sudo -u pulsedag mkdir -p /home/pulsedag/{bin,data,logs,conf,snapshots,releases}
   ```

## Windows/WSL local setup
1. Install WSL2 + Ubuntu 24.04.
2. In WSL shell, follow the Ubuntu section above.
3. Keep RPC bound to loopback (`127.0.0.1`) when using Windows port forwarding.

## Non-root execution policy
- Run node and miner processes as `pulsedag` only.
- Do not run node services as `root`.

## Node start
```bash
/home/pulsedag/bin/pulsedagd \
  --config /home/pulsedag/conf/node.toml \
  --rpc-bind 127.0.0.1:28545 \
  --p2p-bind /ip4/0.0.0.0/tcp/32303 \
  --bootnode /ip4/bootnode.example.net/tcp/32303/p2p/12D3KooWExample \
  --data-dir /home/pulsedag/data/node-1
```

## External miner start
```bash
/home/pulsedag/bin/pulsedag-miner \
  --node http://127.0.0.1:28545 \
  --miner-address rehearsal-miner-1 \
  --backend cpu --threads 2 --loop
```

## systemd unit examples
`/etc/systemd/system/pulsedagd.service`
```ini
[Unit]
Description=PulseDAG Node
After=network-online.target
Wants=network-online.target

[Service]
User=pulsedag
Group=pulsedag
ExecStart=/home/pulsedag/bin/pulsedagd --config /home/pulsedag/conf/node.toml
Restart=on-failure
RestartSec=5
LimitNOFILE=1048576
WorkingDirectory=/home/pulsedag

[Install]
WantedBy=multi-user.target
```

`/etc/systemd/system/pulsedag-miner.service`
```ini
[Unit]
Description=PulseDAG External Miner
After=pulsedagd.service
Requires=pulsedagd.service

[Service]
User=pulsedag
Group=pulsedag
ExecStart=/home/pulsedag/bin/pulsedag-miner --node http://127.0.0.1:28545 --miner-address rehearsal-miner-1 --backend cpu --threads 2 --loop
Restart=on-failure
RestartSec=5
WorkingDirectory=/home/pulsedag

[Install]
WantedBy=multi-user.target
```

## Log locations
- journald: `journalctl -u pulsedagd -u pulsedag-miner`
- file logs (if redirected): `/home/pulsedag/logs/*.log`

## Evidence collection
Use `scripts/v2_2_19_collect_remote_rehearsal_evidence.sh` from control workstation to gather status, p2p, and logs.

## Safe stop
```bash
sudo systemctl stop pulsedag-miner
sudo systemctl stop pulsedagd
```
Wait for process exit and flush.

## Snapshot restore
1. Stop services.
2. Backup old data dir.
3. Extract trusted snapshot into `/home/pulsedag/data/node-*`.
4. Start node; verify height and selected tip progression.

## Upgrade / rollback
- Upgrade:
  1. Place new binaries in `/home/pulsedag/releases/v2.2.19/`.
  2. Update symlink `/home/pulsedag/bin/pulsedagd` and miner.
  3. `systemctl daemon-reload && systemctl restart pulsedagd pulsedag-miner`.
- Rollback:
  1. Repoint symlink to previous release.
  2. Restart both services.
  3. Confirm `/status` and miner submit activity recover.

## Guardrails
- Never expose admin RPC publicly.
- Keep RPC loopback-only unless strictly tunneled.
- Do not commit secrets, tokens, or private keys.
