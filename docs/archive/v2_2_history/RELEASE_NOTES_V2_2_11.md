# Release Notes v2.2.11 — P2P Completion

PulseDAG v2.2.11 closes the P2P completion milestone. It aligns the repository version to `v2.2.11`, documents the real-network rehearsal path, and packages the diagnostics and smoke-test expectations needed before the v2.2.12 full private-testnet rehearsal.

This release does **not** declare official private-testnet readiness. The readiness decision remains the v2.3.0 milestone.

## P2P completion scope

v2.2.11 focuses on completing and documenting the node-to-node path required for a reproducible local or external-server rehearsal:

- Real `libp2p-real` network mode for three-node rehearsals.
- Block propagation through announce, request, and data exchange.
- Transaction relay plumbing and duplicate handling.
- Tip exchange and catch-up after peer restart.
- Missing parent recovery and orphan management diagnostics.
- Peer scoring, reconnect, cooldown, and backoff visibility.
- P2P diagnostics through `/p2p/status`, `/p2p/peers`, `/p2p/propagation`, `/p2p/topics`, and `/sync/status`.
- Three-node rehearsal scripts under `scripts/v2_2_11_*`.

## Block announce/request/data flow

The v2.2.11 P2P path is documented around the following operator-observable flow:

1. A node accepts or mines a block through the existing node/miner interface.
2. The origin node announces the new block to connected peers.
3. Peers that do not already have the block request it by hash.
4. The origin or another peer sends block data.
5. The receiver validates chain id, parent availability, and block acceptance rules before applying the block.
6. Valid blocks advance local height and become visible through `/status`, `/tips`, `/blocks`, `/p2p/status`, and `/sync/status`.

The release closure requires a three-node rehearsal where node A mines or accepts a block, and nodes B/C receive or sync that block.

## Transaction relay

v2.2.11 keeps transaction relay in the P2P completion scope for node-to-node propagation and diagnostics. The release does not add smart-contract transaction semantics and does not add mining-pool coordination. Operators should verify tx relay counters and duplicate suppression through P2P diagnostics during rehearsals when transactions are submitted.

## Tip exchange

Tip exchange is part of the convergence path for connected peers. Nodes should expose useful current height, tip, peer, and sync state so operators can confirm whether B/C are aligned with A after block production and after restart.

## Missing parent recovery

Peers that receive blocks before parents are available must avoid accepting incomplete chains as final. v2.2.11 documents the missing-parent path as a recovery requirement: missing parents should be requested or surfaced through sync/orphan diagnostics, and convergence should be confirmed through `/sync/status` and node height comparisons.

## Orphan handling

Orphan handling remains a required P2P completion behavior. Orphan counts should remain visible in `/status`, and operators should treat persistent orphan growth as a rehearsal failure requiring log and `/sync/status` review. The closing checklist includes duplicate suppression and invalid block rejection checks so orphan handling is not confused with unsafe acceptance.

## Peer scoring and backoff

The P2P diagnostics expose peer lifecycle and backoff-oriented state so operators can identify healthy, degraded, cooldown, and recovering peers. The release closure expects invalid peer blocks to be rejected, chain-id mismatches to be dropped, and peer/backoff diagnostics to remain useful rather than silent.

## Duplicate suppression

Duplicate inbound and outbound P2P data must not cause repeated block acceptance or uncontrolled relay loops. v2.2.11 treats duplicate block suppression as a smoke/checklist item and recommends reviewing `/p2p/status` and `/p2p/propagation` counters when diagnosing redundant traffic.

## P2P diagnostics

The primary operator endpoints for this release are:

- `GET /status` — node version, chain id, height, tips, orphan count, peer count, P2P mode, and sync state.
- `GET /release` — release version and core endpoint inventory.
- `GET /health` — basic liveness.
- `GET /p2p/status` — real-network mode, peer counts, observations, message/drop counters, duplicate suppression, and peer lifecycle state.
- `GET /p2p/peers` — peer inventory.
- `GET /p2p/propagation` — propagation diagnostics.
- `GET /sync/status` — useful sync phase, selected peer, counters, restart/catch-up state, and errors.

`/status` and `/release` must report `v2.2.11` after this closure.

## Three-node rehearsal scripts

The v2.2.11 rehearsal path is reproducible with:

```bash
cargo build --workspace --release
scripts/v2_2_11_smoke_p2p.sh
```

The smoke script starts node A, node B, and node C in real P2P mode, connects B/C to A, runs the external miner against A, waits for a height increase, verifies B/C convergence, restarts B, and verifies B catches up again. Detailed operator steps are captured in `docs/SMOKE_TEST_V2_2_11.md` and `docs/P2P_REHEARSAL_V2_2_11.md`.

## Known limitations

- v2.2.11 is a P2P completion release, not the official private-testnet readiness release.
- v2.2.12 remains responsible for full private-testnet rehearsal and hardening.
- v2.3.0 remains the private-testnet readiness milestone.
- Smart contracts remain out of scope for v2.2.x.
- Pool logic remains out of scope and must not be embedded in the miner.
- The miner remains an external standalone binary communicating with node RPC.
- Rehearsal evidence should not be presented as production readiness.

## v2.2.12 handoff

v2.2.12 should consume the v2.2.11 P2P completion outputs and run the full private-testnet rehearsal and hardening pass. The next milestone should focus on longer multi-operator rehearsals, restart/rejoin scenarios, sustained sync validation, operational runbook hardening, diagnostics review, and evidence capture without moving readiness claims ahead of v2.3.0.
