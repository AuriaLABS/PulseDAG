# PulseDAG v2.3.0 Isolated-Namespace Rehearsal

## Purpose

Run the Task 12 five-node private-testnet rehearsal on one ephemeral Linux
runner while preserving genuine network isolation. The topology uses five
Linux network namespaces connected through a dedicated bridge. It does not
count five processes sharing one host namespace.

This path is an implementation of the topology already permitted by
`V2_3_0_PRIVATE_TESTNET_REHEARSAL.md`. A successful run remains a private
testnet operations decision only. It does not authorize a version bump, public
testnet, release tag, smart contracts, or the 30-day public-testnet clock.

## Topology

The runner creates:

- `pdg-s1` for `seed-1`;
- `pdg-n1` through `pdg-n4` for the four ordinary nodes;
- bridge `pdgbr0`;
- one veth pair and one unique `10.230.0.0/24` address per node;
- loopback-only RPC on port `8280` inside every namespace;
- real libp2p traffic on TCP port `32333`;
- unique identity, RocksDB, lifecycle, and log paths under
  `/var/lib/pulsedag-task12-netns`;
- one external standalone miner inside the seed namespace.

The selected fault target is `node-4`. Its reviewed hook changes only its
namespace-local `eth0` link. Loopback RPC and host-side recovery through
`ip netns exec` remain available while the P2P path is isolated.

## Safety properties

`scripts/private_testnet/netns_rehearsal.sh`:

- requires a clean checkout at one exact 40-character candidate SHA;
- deletes only the bridge and namespace names owned by this rehearsal;
- never flushes a firewall table or changes the runner default route;
- installs the same release binary into all five lifecycle roots;
- starts a separate miner process rather than embedded node mining;
- uses a 35-minute hard deadline around the live controller;
- attempts process and namespace cleanup on success, failure, interrupt, or
  timeout;
- stores controller evidence separately from operator logs so the controller
  checksum bundle is not modified after verification.

## Actions execution

`.github/workflows/v2_3_0_netns_live_rehearsal.yml` has two modes:

1. Pull requests run syntax, contract, comment-language, version guardrail, and
   repository-hygiene checks without creating privileged namespaces.
2. A push to `main` that changes the namespace rehearsal files, or a manual
   dispatch with `run_live=true`, runs the live topology on `ubuntu-24.04`.

The live job uses the GitHub environment
`pulsedag-private-testnet-rehearsal`. Configure required reviewers for that
environment before treating a run as protected release evidence.

## Evidence

The uploaded artifact contains:

- the exact generated inventory;
- immutable controller evidence and `SHA256SUMS`;
- provisioning logs;
- external miner logs;
- final namespace address, route, and listener snapshots;
- per-node process logs;
- the fault-hook log;
- a run summary preserving
  `public_testnet_ready=false` and
  `thirty_day_public_testnet_clock_started=false`.

A successful workflow still requires independent review of the uploaded
artifact and confirmation that the GitHub environment had the intended
protection. Task 13 remains blocked until that review is recorded.
