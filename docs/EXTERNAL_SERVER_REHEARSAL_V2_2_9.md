# PulseDAG v2.2.9 External Server Rehearsal Runbook

> **Scope:** This runbook is for **rehearsal only** on external Ubuntu servers.
>
> - **v2.2.9 is a rehearsal release**, not an official private testnet.
> - **v2.3.0 remains the official private-testnet milestone**.
> - **Production readiness is not claimed**.

This guide describes how to deploy one or more PulseDAG nodes on external servers while keeping RPC private by default.

---

## 1) Ubuntu dependencies

Run on each server:

```bash
sudo apt update
sudo apt -y upgrade
sudo apt -y install \
  git \
  curl \
  build-essential \
  pkg-config \
  libssl-dev \
  clang \
  cmake \
  protobuf-compiler

curl https://sh.rustup.rs -sSf | sh -s -- -y
source "$HOME/.cargo/env"
rustc --version
cargo --version
```

---

## 2) Clone and build

```bash
cd /opt
sudo git clone https://github.com/AuriaLABS/PulseDAG.git
sudo chown -R "$USER":"$USER" /opt/PulseDAG
cd /opt/PulseDAG

# Optional: checkout the exact rehearsal branch/tag/commit for v2.2.9
# git checkout <ref>

cargo build --workspace --release
```

Verify version markers:

```bash
# Check VERSION file (if present)
[ -f VERSION ] && cat VERSION || echo "VERSION file not found"

# Check package versions in Cargo.toml files
rg '^version\s*=\s*"' Cargo.toml crates/*/Cargo.toml
```

---

## 3) Node A service (external server)

A known rehearsal server may be `187.33.159.18` (**example only**, not mandatory).

Create dedicated paths:

```bash
sudo mkdir -p /var/lib/pulsedag/rehearsal-a
sudo mkdir -p /etc/pulsedag
```

Create systemd unit `/etc/systemd/system/pulsedagd-a.service`:

```ini
[Unit]
Description=PulseDAG Node A (v2.2.9 rehearsal)
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=<YOUR_USER>
WorkingDirectory=/opt/PulseDAG
ExecStart=/opt/PulseDAG/target/release/pulsedagd \
  --profile rehearsal-a \
  --data-dir /var/lib/pulsedag/rehearsal-a \
  --rpc-bind 127.0.0.1:18080 \
  --p2p-bind 0.0.0.0:28080
Restart=always
RestartSec=3
LimitNOFILE=65535

[Install]
WantedBy=multi-user.target
```

> If your binary uses different flag names, run:
>
> ```bash
> /opt/PulseDAG/target/release/pulsedagd --help
> ```
>
> and map the runbook arguments accordingly.

Enable and follow logs:

```bash
sudo systemctl daemon-reload
sudo systemctl enable pulsedagd-a
sudo systemctl start pulsedagd-a
sudo systemctl status pulsedagd-a --no-pager
sudo journalctl -u pulsedagd-a -f
```

If you need an isolated profile, replace `rehearsal-a` with `private`.

---

## 4) Miner service

Create `/etc/systemd/system/pulsedag-miner-a.service`:

```ini
[Unit]
Description=PulseDAG Miner A (v2.2.9 rehearsal)
After=network-online.target pulsedagd-a.service
Wants=network-online.target
Requires=pulsedagd-a.service

[Service]
Type=simple
User=<YOUR_USER>
WorkingDirectory=/opt/PulseDAG
ExecStart=/opt/PulseDAG/target/release/pulsedag-miner \
  --node-rpc http://127.0.0.1:18080 \
  --miner-address <MINER_ADDRESS> \
  --threads %NPROC% \
  --loop
Restart=always
RestartSec=3

[Install]
WantedBy=multi-user.target
```

Replace `%NPROC%` before enabling:

```bash
NPROC=$(nproc)
sudo sed -i "s/%NPROC%/${NPROC}/g" /etc/systemd/system/pulsedag-miner-a.service
```

Start and inspect miner logs:

```bash
sudo systemctl daemon-reload
sudo systemctl enable pulsedag-miner-a
sudo systemctl start pulsedag-miner-a
sudo systemctl status pulsedag-miner-a --no-pager
sudo journalctl -u pulsedag-miner-a -f
```

Notes:
- RPC endpoint should stay local (`127.0.0.1`) unless you intentionally change architecture.
- Loop mode is enabled.
- No pool logic is included in this runbook.

---

## 5) Firewall and RPC protection

### UFW baseline

```bash
sudo ufw default deny incoming
sudo ufw default allow outgoing
sudo ufw allow OpenSSH

# Allow only the P2P port used by pulsedagd
sudo ufw allow 28080/tcp

sudo ufw enable
sudo ufw status verbose
```

Rules of thumb:
- Do **not** open RPC port (example: `18080/tcp`) publicly.
- P2P should be the only PulseDAG port exposed in a default rehearsal setup.

### SSH tunnel for RPC access

From your local machine:

```bash
ssh -L 18080:127.0.0.1:18080 user@server
```

Then local tooling can query `http://127.0.0.1:18080` safely over SSH.

---

## 6) Node B / Node C on other servers

On additional servers:
- Use profiles like `rehearsal-b` and `rehearsal-c`.
- Use separate data directories and ports.
- Connect to Node A as bootnode/peer.

Example Node B service pattern:

```ini
ExecStart=/opt/PulseDAG/target/release/pulsedagd \
  --profile rehearsal-b \
  --data-dir /var/lib/pulsedag/rehearsal-b \
  --rpc-bind 127.0.0.1:18081 \
  --p2p-bind 0.0.0.0:28081 \
  --add-peer <NODE_A_P2P_ADDR>
```

Example Node C service pattern:

```ini
ExecStart=/opt/PulseDAG/target/release/pulsedagd \
  --profile rehearsal-c \
  --data-dir /var/lib/pulsedag/rehearsal-c \
  --rpc-bind 127.0.0.1:18082 \
  --p2p-bind 0.0.0.0:28082 \
  --add-peer <NODE_A_P2P_ADDR>
```

Health checks from each host (or via SSH tunnel):

```bash
curl -fsS http://127.0.0.1:18081/status
curl -fsS http://127.0.0.1:18081/p2p/status
```

---

## 7) Verification checklist

Run against each node RPC endpoint (local bind or tunneled):

```bash
curl -fsS http://127.0.0.1:18080/health
curl -fsS http://127.0.0.1:18080/status
curl -fsS http://127.0.0.1:18080/release
curl -fsS http://127.0.0.1:18080/pow
curl -fsS http://127.0.0.1:18080/tips
curl -fsS http://127.0.0.1:18080/runtime
curl -fsS http://127.0.0.1:18080/metrics
```

Mining template and miner checks:

```bash
# Endpoint name can vary by build; inspect API docs/help if needed
curl -fsS http://127.0.0.1:18080/mining/template
sudo journalctl -u pulsedag-miner-a -n 200 --no-pager
```

P2P and propagation/catch-up checks:

```bash
sudo journalctl -u pulsedagd-a -n 300 --no-pager

# Optional periodic status sampling
watch -n 5 'curl -fsS http://127.0.0.1:18080/status | head -c 400; echo'
```

Validate that:
- Height/tips progress over time.
- Node B/C catch up to Node A.
- P2P logs show stable peers and message exchange.

---

## 8) Reset procedure

```bash
# Stop services
sudo systemctl stop pulsedag-miner-a
sudo systemctl stop pulsedagd-a

# Backup current data dir
TS=$(date +%Y%m%d-%H%M%S)
sudo mv /var/lib/pulsedag/rehearsal-a "/var/lib/pulsedag/rehearsal-a.backup-${TS}"

# Recreate empty dir with ownership
sudo mkdir -p /var/lib/pulsedag/rehearsal-a
sudo chown -R <YOUR_USER>:<YOUR_USER> /var/lib/pulsedag/rehearsal-a

# Restart
sudo systemctl start pulsedagd-a
sudo systemctl start pulsedag-miner-a
```

If you want a hard reset without backup, replace `mv` with `rm -rf` (use with caution).

---

## 9) Known limitations and release positioning

- This document targets **v2.2.9 rehearsal** behavior.
- It is **not** an official private-testnet declaration.
- **v2.3.0** remains the planned official private-testnet milestone.
- This setup does **not** claim production readiness.

