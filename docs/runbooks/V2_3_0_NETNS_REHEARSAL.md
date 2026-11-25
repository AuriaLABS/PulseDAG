# PulseDAG v2.3.0 Isolated-Namespace Rehearsal

> RETIRED PROTOTYPE PROCEDURE. The old live workflow is removed. Do not use this document to deploy a private network. Current first-network policy: [testnet-first launch](V3_0_0_TESTNET_FIRST_LAUNCH.md).

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

The selected fault target is `node-4`. Its reviewed hook takes only its
namespace-local `eth0` link down and destroys only established TCP sockets
whose source or destination port is the PulseDAG P2P port `32333`. Destroying
the local socket state is required because Linux may otherwise retain a TCP
session after link-down for longer than the bounded partition window. Loopback
RPC, the node process, and host-side recovery through `ip netns exec` remain
available throughout the isolation.

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

## Historical Actions execution

The [retired v2.3 namespace workflow](https://github.com/AuriaLABS/PulseDAG/blob/9cbe3ba51bd98d992b517c25e648717d01287d69/.github/workflows/v2_3_0_netns_live_rehearsal.yml) is preserved in Git history only. It required an obsolete release identity and is no longer a dispatch entrypoint.

Current repository hygiene executes the isolated syntax, fixture and safety-contract regressions without creating privileged namespaces or a live network. The existing Task30 runtime acceptance gates remain unchanged pending their native-v3 replacement. No historical rehearsal constitutes native monetary acceptance.

## Evidence

The uploaded artifact contains:

- the exact generated inventory;
- immutable controller evidence and `SHA256SUMS`;
- provisioning logs;
- external miner logs;
- final namespace address, route, and listener snapshots;
- per-node process logs;
- the fault-hook log, including P2P connection counts before and after
  isolation;
- a run summary preserving
  `public_testnet_ready=false` and
  `thirty_day_public_testnet_clock_started=false`.

A successful workflow still requires independent review of the uploaded
artifact and confirmation that the GitHub environment had the intended
protection. Task 13 remains blocked until that review is recorded.
