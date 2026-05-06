# Release Notes v2.2.12 — Full Private-Testnet Rehearsal and Hardening

PulseDAG v2.2.12 is the documentation and operations milestone after v2.2.11 P2P completion. It packages the plan for a full private-testnet rehearsal and hardening pass across multi-node, multi-operator, longer-running, restart/rejoin, sync convergence, diagnostics review, runbook hardening, and evidence capture work.

This release does **not** declare official private-testnet readiness. v2.3.0 remains the readiness decision milestone.

## Rehearsal and hardening scope

v2.2.12 focuses on turning the completed v2.2.11 P2P path into an operator-ready rehearsal model:

- Full private-testnet rehearsal planning without readiness claims.
- Multi-node and multi-operator validation beyond the local three-node smoke path.
- Longer-running rehearsal windows that observe repeated mining, propagation, and convergence cycles.
- Restart/rejoin behavior for lagging or intentionally restarted nodes.
- Sync convergence checks after mining, restart, temporary lag, and peer churn.
- Diagnostics review across status, P2P, propagation, peer, sync, and missing-parent endpoints.
- Runbook hardening based on operator ambiguity, recovery notes, and repeatability gaps.
- Evidence capture for the later v2.3.0 readiness decision.

## Inherited baseline from v2.2.11

The v2.2.12 plan keeps the v2.2.11 three-node sequence as the baseline gate:

1. Build release binaries.
2. Start A/B/C with real `libp2p-real` mode and a shared chain id.
3. Connect B and C to A through the real `--bootnode` flag.
4. Run the external `pulsedag-miner` against A RPC.
5. Wait for A height to increase.
6. Verify B/C receive or sync the mined block.
7. Restart B and verify it catches up again.
8. Collect final `/health`, `/status`, `/p2p/status`, and `/sync/status` responses from A/B/C.

The updated operator docs are `docs/P2P_REHEARSAL_V2_2_12.md`, `docs/SMOKE_TEST_V2_2_12.md`, and `docs/SYNC_RECOVERY_V2_2_12.md`.

## Diagnostics expectations

Operators should collect and review:

- `GET /health` — basic liveness.
- `GET /status` — version, chain id, height, tips, orphan count, peer count, P2P mode, and sync state.
- `GET /release` — release version and endpoint inventory where available.
- `GET /p2p/status` — real-network mode, peer counts, message counters, drop counters, duplicate suppression, and peer lifecycle state.
- `GET /p2p/peers` — peer inventory.
- `GET /p2p/propagation` — propagation counters and relay diagnostics.
- `GET /sync/status` — sync phase, selected peer, catch-up state, lag, counters, and errors.
- `GET /sync/missing` — pending block requests and missing parent hashes.

## Evidence expectations

A v2.2.12 rehearsal evidence bundle should include:

- Commit, version, chain id, topology, hosts, ports, and operator roles.
- Node and miner startup commands.
- Node logs and external miner logs.
- Baseline, periodic, pre-restart, post-restart, and final endpoint responses.
- Notes on sync lag, duplicate suppression, invalid block rejection, chain-id mismatch handling, missing parents, or peer backoff.
- Runbook changes or follow-up items discovered during rehearsal.
- A completed `docs/CLOSING_CHECKLIST_V2_2_12.md`.

## Known limitations and guardrails

- v2.2.12 is rehearsal and hardening, not official private-testnet readiness.
- v2.3.0 remains the private-testnet readiness decision milestone.
- Smart contracts remain out of scope for v2.2.x.
- Pool logic remains out of scope and must not be embedded in `pulsedag-miner`.
- The miner remains an external standalone binary communicating with node RPC.
- Rehearsal evidence should not be presented as production, public-testnet, or readiness evidence.

## v2.3.0 handoff

v2.2.12 should hand v2.3.0 a clear evidence package: what passed, what failed, what required operator intervention, what diagnostics were useful, what runbook changes were made, and what risks remain. The readiness decision belongs to v2.3.0.
