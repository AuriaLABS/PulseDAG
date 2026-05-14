# PulseDAG v2.2.15 sustained P2P rehearsal plan

This plan defines the sustained P2P multi-node rehearsal required for v2.2.15 closeout. It assumes v2.2.14 storage/replay hardening is the baseline and focuses on multi-node operation, churn, restart/rejoin, lag recovery, convergence, peer diagnostics, and chain-id isolation.

## Boundaries

- Use real `libp2p-real` networking for rehearsal nodes.
- Keep mining external to the node. Use `pulsedag-miner` or equivalent external RPC client behavior if blocks are needed.
- Do not add smart contracts, enable a contract runtime, or add pool logic.
- Do not claim v2.3.0 readiness from this rehearsal alone.
- Avoid consensus-rule changes unless they fix a documented safety bug with tests.

## Evidence directory

Create one evidence directory per run and include:

- Date, commit, binary versions, chain id, host, ports, and topology.
- Startup commands for every node and external miner/client.
- Node logs and external miner/client logs.
- Baseline, periodic, pre-event, post-event, and final endpoint snapshots.
- Operator notes for failures, recovery actions, timing, and follow-up.
- A completed copy of `docs/CLOSING_CHECKLIST_V2_2_15.md`.

## Required endpoints

Capture these endpoints when available for every node:

- `/health`
- `/status`
- `/p2p/status`
- `/p2p/peers`
- `/p2p/propagation`
- `/p2p/topics`
- `/sync/status`
- `/sync/missing`

If an endpoint is unavailable, record the HTTP status, error, or replacement endpoint used.

## Phase 0: command and release evidence baseline

From the repository root, run and archive:

```bash
cargo fmt --all -- --check
cargo test --workspace
cargo build --workspace
./scripts/v2-2-14-release-evidence.sh
```

If the release evidence script is inherited from v2.2.14, label it as the baseline release-evidence script output and keep v2.2.15-specific P2P evidence in the same evidence bundle.

## Phase 1: three-node local rehearsal

1. Build the workspace binaries.
2. Start node A as the bootnode with a clean data directory.
3. Start nodes B and C with clean data directories and node A as bootnode.
4. Verify every node reports healthy status.
5. Verify B and C observe node A, and node A observes B/C.
6. Produce or import blocks through the external mining/RPC path.
7. Confirm A/B/C converge on height, tips, or equivalent sync status.
8. Restart B and verify it rejoins and catches up without manual data edits.
9. Capture final health, status, P2P, peer, and sync snapshots.

Pass criteria: A/B/C remain connected or recover, blocks converge, restart/rejoin succeeds, and diagnostics explain any temporary lag or missing data.

## Phase 2: five-node local rehearsal, if practical

1. Extend the topology to nodes A/B/C/D/E on unique RPC and P2P ports.
2. Use one shared chain id for all five nodes.
3. Connect nodes through node A and, if supported, additional peer links for redundancy.
4. Run long enough to observe repeated propagation and sync cycles.
5. Restart at least one non-bootnode.
6. Stop one peer long enough to lag, continue block production, then restart it.
7. Confirm all available nodes converge.
8. Capture endpoint snapshots and logs for all five nodes.

Pass criteria: at least five nodes can be started and observed, churn or restart does not cause permanent divergence, and lagging nodes recover. If local resources make this impractical, record CPU, memory, port, or time constraints and keep the three-node evidence as the required baseline.

## Phase 3: peer churn evidence

Exercise at least two churn events:

- Stop and restart a non-bootnode.
- Add a fresh node to an already running topology.
- Temporarily stop multiple peers while retaining at least one connected path.
- Restart the bootnode after peers have discovered each other, if practical.

For each event, capture pre-event and post-event peer lists, P2P status, sync status, logs, and convergence notes.

## Phase 4: lagging-node recovery evidence

1. Let node A and at least one peer advance.
2. Stop node C or isolate it long enough to fall behind.
3. Restart or reconnect node C without editing its data directory.
4. Confirm node C requests, receives, or otherwise recovers missing data.
5. Capture `/sync/status`, `/sync/missing`, peer diagnostics, and final convergence snapshots.

Pass criteria: the lagging node catches up or the diagnostics clearly identify a release-blocking recovery issue.

## Phase 5: chain-id isolation evidence

1. Start the baseline topology on the intended v2.2.15 rehearsal chain id.
2. Start an additional node with a different chain id and unique ports.
3. Attempt to connect the mismatched-chain node to the baseline bootnode.
4. Confirm block, transaction, and sync topics remain isolated and the mismatched node does not contaminate accepted data.
5. Capture P2P status, topic names when available, logs, and final state for both matching and mismatched nodes.

Pass criteria: mismatched-chain peers are rejected, ignored, isolated by topic, or otherwise prevented from causing cross-chain data acceptance.

## Phase 6: closeout review

Close v2.2.15 only after:

- Required commands pass.
- Release evidence script output is attached.
- Three-node rehearsal passes.
- Five-node rehearsal passes or is explicitly deferred as impractical.
- Restart/rejoin, lag recovery, churn, chain-id isolation, and sync convergence evidence is attached.
- No unresolved Sev-1 consensus or sync defect remains open.
