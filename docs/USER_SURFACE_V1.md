# User surface v1

Status: **PLANNING SPEC**

Date: 2026-09-16 UTC
Parent issues: #1178, #794, #819
Source thesis: `ROADMAP_V3_0_0.md` Task P7
Repos: `AuriaLABS/PulseDAG`, `AuriaLABS/pulsedag-explorer`, `AuriaLABS/PulseDAG-Desktop`

Planning only. Does not add a custody wallet to the v2.4.0 node/miner candidate.

## Purpose

A 3.0 demo is not differentiated until a non-core user can send value, see the DAG, and read PulseClock. Implementation lives outside consensus.

## Surfaces

### Wallet (non-custodial)

- create / restore / sign / send / reconcile on supported platforms;
- tx v2 + `submission_id` + `chain_id` domains;
- no private keys on public node RPC;
- PulseClock displayed as height + uncertainty;
- vault / HTLC / stream UI only when those templates are admitted; until then those flows stay hidden or fail closed;
- mainnet vs testnet signing separation (#794 Workstream C).

Tracked with #819. Not part of the current node tarball unless separately ported and revalidated.

### Explorer

Must render a DAG, not only a linear list:

- parents, children, selected parent;
- blue / red if classified;
- pulse height/time when PulseClock is observationally available;
- search for block, tx, address;
- read-only; no mining or admin RPC.

Current `pulsedag-explorer` is a v2.3 private-testnet panel. v3 surface needs a DAG view + PulseClock fields.

### Desktop

Operator + light wallet, not only a process supervisor:

- start/stop node and external miner;
- provenance-checked binaries;
- loopback RPC by default;
- optional wallet pane using the same non-custodial rules.

## Demo gate (from ROADMAP_V3_0_0)

1. two non-conflicting txs apply from parallel blocks;
2. vault moves after N pulses and is visible in the explorer;
3. based-app commit settles on pulse;
4. `pulsedag verify` matches the published digest;
5. a non-core user sends value with the official wallet.

Items 1–4 are protocol/docs. Item 5 is this surface.

## Authorization

Planning only. Does not claim an official custody wallet on the Task31 candidate.
