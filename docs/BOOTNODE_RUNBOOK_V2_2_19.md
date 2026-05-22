# BOOTNODE RUNBOOK v2.2.19

## Purpose
Operate durable bootnodes for multi-host private rehearsal.

## Bootnode host baseline
- Ubuntu 24.04 LTS.
- Dedicated non-root user `pulsedag`.
- Static DNS name preferred (example: `bootnode-1.example.net`).

## Required ports
- TCP 32303 (P2P) inbound from rehearsal peers.
- SSH restricted by source allowlist.
- No public admin RPC.

## Bootnode node profile
- Disable mining on bootnode host.
- Keep RPC on `127.0.0.1` for local health checks.

## Start command example
```bash
/home/pulsedag/bin/pulsedagd \
  --rpc-bind 127.0.0.1:28545 \
  --p2p-bind /ip4/0.0.0.0/tcp/32303 \
  --network-profile private-rehearsal-v2_2_19 \
  --data-dir /home/pulsedag/data/bootnode-1
```

## Publish bootnode multiaddr
After startup, capture peer id from logs and publish:
`/ip4/bootnode-1.example.net/tcp/32303/p2p/<peer-id>`

## Health checks
```bash
curl -fsS http://127.0.0.1:28545/status | jq .
curl -fsS http://127.0.0.1:28545/p2p/status | jq .
```

## Bootnode lifecycle
- Start with systemd.
- Rotate binaries with controlled restart.
- Keep at least one warm standby bootnode defined in `bootnodes.example.toml`.
