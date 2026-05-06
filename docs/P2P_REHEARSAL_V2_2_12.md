# P2P Rehearsal v2.2.12 — Full Private-Testnet Rehearsal and Hardening

This runbook extends the v2.2.11 three-node P2P completion rehearsal into the v2.2.12 full private-testnet rehearsal and hardening pass. It keeps the v2.2.11 baseline sequence of node A, node B, node C, an external miner against node A, restart of node B, and final `/health`, `/status`, `/p2p/status`, and `/sync/status` evidence collection, then expands the exercise for longer-running, multi-node, and multi-operator validation.

v2.2.12 is **not** official private-testnet readiness. v2.3.0 remains the readiness decision milestone.

## Scope

The v2.2.12 rehearsal validates:

1. Multi-node startup with real `libp2p-real` networking.
2. Multi-operator handoff of launch, monitoring, restart, and evidence duties.
3. A longer-running rehearsal window instead of a short local-only smoke pass.
4. Restart/rejoin behavior for at least one non-bootnode and, where possible, a bootnode restart window.
5. Sync convergence after mining, restart, peer churn, and temporary lag.
6. Diagnostics review across `/health`, `/status`, `/p2p/status`, `/p2p/peers`, `/p2p/propagation`, `/sync/status`, and `/sync/missing`.
7. Runbook hardening: every manual step should be clear enough for another operator to repeat.
8. Evidence capture suitable for the v2.3.0 readiness decision, without claiming readiness in v2.2.12.

## Boundaries

- v2.2.12 does not declare official private-testnet readiness.
- v2.3.0 remains the readiness decision milestone.
- Smart contracts remain out of scope for v2.2.x.
- Pool logic must not be added inside `pulsedag-miner`.
- The miner remains an external standalone process that talks to node RPC.
- Rehearsal evidence must be labeled as v2.2.12 hardening evidence, not production or public readiness evidence.

## Baseline inherited from v2.2.11

Use the v2.2.11 three-node path as the first gate before extending duration or topology:

1. Build release binaries with `cargo build --workspace --release`.
2. Start node A, node B, and node C with one shared chain id.
3. Give every node its own data directory and RPC/P2P ports.
4. Connect B and C to node A using the real `pulsedagd` `--bootnode` flag.
5. Check `/health` and `/p2p/status` on all nodes.
6. Verify peer observations are non-empty and consistent with the topology.
7. Start external `pulsedag-miner` against node A RPC.
8. Wait for node A height to become greater than the starting height.
9. Verify B and C receive or sync the mined block.
10. Restart B and verify it catches up again.
11. Collect final A/B/C `/health`, `/status`, `/p2p/status`, and `/sync/status` responses.

The detailed smoke procedure is maintained in `docs/SMOKE_TEST_V2_2_12.md`.

## Expanded v2.2.12 rehearsal model

After the baseline gate passes, extend the rehearsal as follows:

| Phase | Goal | Minimum evidence |
| --- | --- | --- |
| Baseline gate | Prove the v2.2.11 A/B/C path still works | Command transcript, logs, final endpoint bundle |
| Multi-operator handoff | Split ownership across at least two operators or role-played operator terminals | Operator notes showing who started, monitored, restarted, and collected evidence |
| Longer run | Keep the network running long enough to observe repeated mining, propagation, and sync cycles | Periodic height, peer, sync, and propagation snapshots |
| Restart/rejoin | Restart B and at least one additional node when topology allows | Before/after endpoint snapshots and logs |
| Sync convergence | Confirm lagging nodes converge after mining and restart | A/B/C height comparison and `/sync/status` snapshots |
| Diagnostics review | Review counters, drops, duplicate suppression, and peer lifecycle state | `/p2p/status`, `/p2p/propagation`, `/sync/status`, `/sync/missing` bundle |
| Runbook hardening | Capture every ambiguity or missing operator step | Action list for documentation or script follow-up |
| Closeout | Decide whether the rehearsal evidence is adequate for v2.3.0 readiness review inputs | Completed `docs/CLOSING_CHECKLIST_V2_2_12.md` |

## Suggested topology

Start with the local or single-host A/B/C topology from v2.2.11. When external hosts are available, expand to a private-testnet rehearsal topology:

- Node A: initial bootnode and miner target.
- Node B: restart/rejoin candidate.
- Node C: independent convergence observer.
- Optional node D/E: additional operators, hosts, regions, or firewall profiles.

For external hosts, bootnodes must use reachable P2P multiaddrs. RPC endpoints should remain private to operators through localhost, SSH forwarding, VPN, or firewall allowlists.

## Evidence bundle

Create one evidence directory per rehearsal attempt and include:

- Rehearsal metadata: date, operators, commit, binary version, topology, chain id, hostnames, and ports.
- Startup commands for each node and miner.
- Logs for all nodes and the external miner.
- Baseline, periodic, pre-restart, post-restart, and final endpoint responses.
- Notes on failures, remediations, and runbook changes.
- A completed copy of `docs/CLOSING_CHECKLIST_V2_2_12.md`.

Use the rehearsal evidence collector for closeout capture:

```bash
scripts/v2_2_12_collect_evidence.sh
```

The collector writes an evidence directory under the v2.2.12 rehearsal state path, captures required A/B/C `/health`, `/status`, `/p2p/status`, and `/sync/status` responses, attempts optional `/sync/missing`, `/p2p/propagation`, `/p2p/peers`, and `/p2p/topics` responses, copies node/miner logs from the rehearsal log directory, saves the chain id, RPC/P2P addresses, bootnode value, binary paths, and git commit, then creates a final `tar.gz` archive. Treat a non-zero collector exit as a closeout blocker because at least one required endpoint could not be captured.

The collector inherits the same overrides as the v2.2.12 launch scripts, including `PULSEDAG_REHEARSAL_STATE_DIR`, `PULSEDAG_REHEARSAL_LOG_DIR`, `PULSEDAG_NODE_A_RPC`, `PULSEDAG_NODE_B_RPC`, `PULSEDAG_NODE_C_RPC`, and binary path overrides.

## Pass criteria

The rehearsal is acceptable for v2.2.12 closeout when:

- The v2.2.11 baseline path passes under v2.2.12 documentation.
- Nodes connect in real P2P mode and keep useful peer observations during the longer run.
- Blocks produced through the external miner converge across participating nodes.
- Restarted nodes rejoin and catch up without data loss or manual state edits.
- Diagnostics explain any lag, drops, duplicate suppression, missing parents, or peer backoff.
- Operators capture enough evidence for a v2.3.0 readiness decision review.
- No v2.2.12 document or artifact claims readiness ahead of v2.3.0.
