# PulseDAG v2.2.15 roadmap: sustained P2P multi-node rehearsal

v2.2.15 opens the sustained P2P multi-node rehearsal release after the v2.2.14 storage, replay, snapshot, restore, pruning, and migration-policy hardening milestone. The release focuses on proving that the existing node can remain connected, recover, and converge across multiple peers under realistic operator actions.

This is a rehearsal and evidence release. It does not add smart contracts, does not enable a contract runtime, does not add pool logic, and does not move mining into the node. The miner remains a standalone external application that talks to node RPC.

## Release intent

v2.2.15 should turn the completed P2P path and the v2.2.14 durability work into repeatable multi-node operating evidence. The release is successful when operators can show local three-node and, where practical, five-node rehearsals with peer churn, restart/rejoin, lag recovery, convergence, diagnostics, and chain-id isolation evidence.

v2.2.15 does not claim v2.3.0 private-testnet readiness by itself. It produces the P2P evidence required for the later v2.3.0 readiness decision.

## In scope

- Run sustained local multi-node rehearsals using real `libp2p-real` networking.
- Complete a three-node local rehearsal as the required baseline gate.
- Complete a five-node local rehearsal when practical for the host and available operator time.
- Capture restart/rejoin evidence for at least one non-bootnode and, if practical, a bootnode recovery window.
- Capture lagging-node recovery evidence after temporary stop, network delay, or delayed startup.
- Capture peer churn evidence by adding, stopping, and reintroducing peers without manual data edits.
- Capture chain-id isolation evidence showing mismatched-chain peers do not contaminate block, transaction, or sync topics.
- Capture sync convergence evidence using height/status comparisons and `/sync/status` snapshots.
- Review peer diagnostics and operator-facing endpoints including `/health`, `/status`, `/p2p/status`, `/p2p/peers`, `/p2p/propagation`, `/sync/status`, and `/sync/missing` when available.
- Preserve v2.2.14 storage/replay guardrails while rehearsing longer P2P operation.
- Keep consensus-rule changes out of scope unless they fix a documented safety bug and include tests.

## Explicitly out of scope

- Smart contract execution, deployment, gas accounting, VM selection, contract RPCs, or enabling a contract runtime.
- Pool server implementation, pool payout logic, share accounting, pool policy, or embedded pool coordination in `pulsedag-miner`.
- Moving the miner into the node process.
- Public testnet launch, public-network readiness claims, or automatic v2.3.0 promotion.
- New consensus features, compatibility claims, or protocol-rule changes that are not required to fix a documented safety bug.
- Feature expansion that does not improve sustained P2P rehearsal evidence.

## Required exit evidence

v2.2.15 closeout requires:

1. `VERSION` is `v2.2.15`, Cargo workspace version is `2.2.15`, and license metadata remains `ISC`.
2. `cargo fmt --all -- --check` passes.
3. `cargo test --workspace` passes.
4. `cargo build --workspace` passes.
5. The release evidence script output is captured or, if the v2.2.14 script remains the current evidence script, its output is labeled as the inherited release-evidence baseline.
6. A three-node local rehearsal passes with startup, peer discovery, mining through the external miner, block propagation, restart/rejoin, and convergence notes.
7. A five-node local rehearsal passes when practical, or the checklist records why it was deferred.
8. Node restart/rejoin evidence is attached.
9. Lagging-node recovery evidence is attached.
10. Peer churn evidence is attached.
11. Chain-id isolation evidence is attached.
12. Sync convergence evidence is attached.
13. No unresolved Sev-1 consensus or sync defect remains open at closeout.

## Release outputs

v2.2.15 should leave behind:

- A completed closing checklist in `docs/CLOSING_CHECKLIST_V2_2_15.md`.
- Release notes in `docs/RELEASE_NOTES_V2_2_15.md`.
- A sustained P2P rehearsal plan in `docs/P2P_REHEARSAL_PLAN_V2_2_15.md`.
- Updated positioning in `docs/VERSION_MATRIX.md` and `README.md`.
- Evidence links or notes for the required cargo checks, release evidence script output, three-node rehearsal, five-node rehearsal decision, restart/rejoin, lag recovery, churn, chain-id isolation, and sync convergence.
