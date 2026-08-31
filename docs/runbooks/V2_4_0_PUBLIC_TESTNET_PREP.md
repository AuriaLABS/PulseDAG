# v2.4.0 public-testnet preparation runbook — historical/superseded

Status: **PRE-GO / NOT LAUNCHED — SUPERSEDED FOR FINAL PROJECT LAUNCH**

This runbook documents the old v2.4.0 standalone-public-testnet plan. It remains in the repository for historical evidence, regression checks and compatibility with the v2.4 hardening validator.

The active launch authority is now:

- `docs/ROADMAP_V3_0_0.md`;
- `docs/runbooks/V3_0_0_DUAL_NETWORK_LAUNCH.md`;
- issue #781 for the final `GO_V3_DUAL_LAUNCH` decision.

PulseDAG now targets **v3.0.0 in Q4 2026**, launching **mainnet and a parallel public testnet in one coordinated release window**. The previous standalone testnet-first sequence and its 30-day pre-mainnet clock are no longer project launch prerequisites.

## Historical v2.4 authority

Under the superseded v2.4 plan, issue #781 was the only public launch-control record and this document could not set `GO_PUBLIC_TESTNET`, Day 0, the 30-day clock, high cadence or contracts.

Historical hard preconditions included:

1. #789 private burn-in/restart/prune/restore/rejoin PASS;
2. 5-node/4-miner private rehearsal PASS across independent failure domains;
3. exact source SHA and release artifact digests;
4. frozen chain ID, network profile, genesis hash and config digests;
5. persistent bootnode peer IDs and final multiaddrs;
6. public RPC DNS/TLS ownership and edge policy;
7. storage/backup/NTP/firewall checks;
8. #803 security disposition;
9. explicit `GO_PUBLIC_TESTNET`.

The legacy templates under `configs/public-testnet/` intentionally remain fail-closed for those historical checks.

## Historical role model

### Seed / bootnode

- persistent P2P identity outside the release directory;
- public P2P only;
- RPC loopback/private management;
- no public admin RPC;
- bootnode multiaddr recorded from the live peer ID.

### Ordinary node

- persistent identity and RocksDB volume;
- frozen bootnode set;
- admin disabled on the public-facing process;
- snapshot-gated pruning and backup policy.

### Observer / public RPC

- `PULSEDAG_API_PROFILE=public_safe`;
- admin disabled;
- reverse proxy/TLS ownership where public HTTP is used;
- bounded request body, rate limiting and CORS allowlist.

### External miner

- packaged exact-release miner;
- frozen intended node/RPC endpoint;
- CPU canonical reference path;
- no pool/payout semantics.

## Historical rendering rule

The old v2.4 flow would have kept `PULSEDAG_PUBLIC_TESTNET_READY=false` and P2P/public exposure disabled until final review. It would have set `PULSEDAG_THIRTY_DAY_PUBLIC_TESTNET_CLOCK_STARTED=true` only after the actual standalone public-testnet launch timestamp and first accepted block were recorded.

That transition must **not** be executed now as a project-launch step. The v3.0.0 launch no longer uses a 30-day public-testnet clock before mainnet.

## What remains reusable for v3

The operational controls below remain valid inputs to the v3 launch program:

- persistent P2P identity;
- exact artifact/config/genesis digests;
- public-safe RPC isolation;
- storage/snapshot/backup/recovery drills;
- NTP and disk-pressure monitoring;
- consensus/state divergence alerts;
- mining submit-finality reconciliation;
- incident evidence and rollback discipline;
- no credentials, wallet seeds or private keys in committed configs/evidence.

For the definitive launch, apply those controls separately to the frozen **mainnet** and **parallel-testnet** identities defined by the v3 runbook.

## Hard-stop conditions retained

Do not launch, or stop and record an incident, on consensus/state divergence, unexplained data loss, unresolved security blocker without approved disposition, stale control plane, persistent peer-mesh failure, submit-finality incoherence, artifact/config digest mismatch, accidental cross-network compatibility, or loss of rollback/recovery capability.

Smart-contract activation is controlled by the v3 release scope and its own completed gates; it is no longer mechanically unlocked by a 30-day standalone public-testnet clock.
