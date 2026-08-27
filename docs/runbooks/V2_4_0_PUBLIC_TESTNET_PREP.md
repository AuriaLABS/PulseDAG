# v2.4.0 public-testnet preparation runbook

Status: **PRE-GO / NOT LAUNCHED**

Authority: issue #781 is the only public launch-control record. This document prepares operations; it cannot set `GO_PUBLIC_TESTNET`, Day 0, the 30-day clock, high cadence or contracts.

## Hard preconditions

Before any public endpoint or bootnode is advertised, all of the following must be attached to #781/#794 on one exact release SHA:

1. #789 private burn-in/restart/prune/restore/rejoin PASS;
2. 5-node/4-miner private rehearsal PASS across independent failure domains;
3. exact source SHA and release artifact digests;
4. frozen chain ID, network profile, genesis hash and config digests;
5. at least two persistent bootnode peer IDs and final `/p2p/<peer-id>` multiaddrs;
6. public RPC DNS/TLS owner, request limits, per-IP limits and CORS allowlist;
7. storage/backup/NTP/firewall checks for every role;
8. security review including #803 disposition;
9. explicit `GO_PUBLIC_TESTNET` in #781.

Until then, the repository templates under `configs/public-testnet/` intentionally retain placeholders and `PULSEDAG_P2P_ENABLED=false`.

## Role separation

### Seed / bootnode

- persistent P2P identity stored outside the release directory;
- public P2P only; RPC stays loopback/private management;
- no admin RPC exposure;
- no miner process on the seed host unless separately approved;
- bootnode multiaddr recorded from the live peer ID, never guessed.

### Ordinary node

- persistent identity and RocksDB volume;
- bootstraps only from the frozen bootnode set;
- admin disabled on the public-facing process;
- snapshot-gated pruning and backup policy enabled.

### Observer / public RPC

- `PULSEDAG_API_PROFILE=public_safe`;
- admin disabled;
- node RPC remains loopback behind a TLS reverse proxy where public HTTP access is required;
- explicit request-body limit, per-IP rate limit and CORS allowlist;
- no credentials, wallet seeds or private keys in proxy/static configuration.

### External miner

- use packaged `pulsedag-miner` from the exact release evidence;
- connect only to the frozen node/RPC endpoint;
- CPU path is the canonical reference;
- no pool/payout functionality is implied;
- use valueless testnet addresses only.

## Host baseline

For every node role record in evidence:

- OS/image identifier and host/failure-domain label;
- UTC/NTP synchronization status and drift alert;
- CPU/RAM/disk capacity and free-space alert threshold;
- persistent RocksDB and P2P identity paths;
- backup/snapshot destination and restore test reference;
- inbound/outbound firewall rules;
- service manager restart policy;
- exact binary/archive SHA-256 and config SHA-256.

Never store GitHub tokens, SSH private keys, wallet seeds or operator-auth tokens in committed config or evidence artifacts.

## Network/firewall policy

- Public P2P: expose only the frozen P2P TCP port required by the role.
- Public RPC: expose only through the owned TLS/reverse-proxy endpoint; node process RPC remains loopback/private management by default.
- Admin/operator surface: never expose to the public internet. Use loopback or a private management network.
- SSH/management: allow only the operator management source ranges; record ownership outside the repository.
- Deny all other unsolicited inbound traffic.

Final IPs, DNS names and firewall source ranges are deployment evidence and remain `TBD` until infrastructure is allocated.

## DNS and TLS

Before GO record:

- DNS names and accountable owner;
- A/AAAA records and expected endpoint mapping;
- TLS certificate issuer/expiry and renewal owner;
- reverse-proxy config digest;
- public status endpoint and incident/status-update path.

No `example.*` or placeholder hostname may appear in the final launch evidence.

## Storage, snapshot and recovery

- RocksDB must live on persistent storage outside extracted release directories.
- Snapshot creation/verification is an operator-only action.
- Compact prune requires a verified snapshot according to the configured gate.
- Restore and clean-node catch-up must be demonstrated in the private rehearsal before public GO.
- Preserve enough free disk for compaction/recovery; alert before the configured safety margin is exhausted.

## Observability

At minimum collect and alert on:

- selected tip/height/state digest divergence;
- stale `/status`, `/runtime/status` or `/sync/status`;
- peer loss/reconnect failure;
- mining submit finality unknown/reconciliation growth;
- chain-lock starvation or accepted-commit conflicts;
- RocksDB/disk pressure;
- snapshot/prune failure;
- unexpected process restart.

Evidence export must include UTC timestamps, node role, exact SHA, config digest and perturbation/incident log.

## Rendering the templates

Only after the freeze record is complete:

1. copy the appropriate `.template` file to a host-local configuration;
2. replace **every** `__TASK31_FREEZE_REQUIRED__` token with the frozen values;
3. record the rendered config SHA-256;
4. keep `PULSEDAG_PUBLIC_TESTNET_READY=false` and P2P/public exposure disabled until the final owner review;
5. after #781 records `GO_PUBLIC_TESTNET`, enable only the role-specific exposure authorized by the launch record;
6. do not set `PULSEDAG_THIRTY_DAY_PUBLIC_TESTNET_CLOCK_STARTED=true` until the actual public launch timestamp and first accepted public block are recorded.

## Hard-stop conditions

Do not launch, or stop and record an incident, on any consensus/state divergence, unexplained data loss, security blocker without approved disposition, stale control plane, persistent peer-mesh failure, submit-finality incoherence, artifact/config digest mismatch, or loss of rollback/recovery capability.

Smart contracts remain disabled until at least 30 accepted public-testnet days and a separate activation approval.
