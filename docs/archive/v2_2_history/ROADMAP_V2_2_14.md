# PulseDAG v2.2.14 roadmap: storage/replay hardening

v2.2.14 is a storage, replay, snapshot, restore, pruning, and migration-policy hardening release for the path to v3.0.0. It is intentionally not a feature-random release: every change and release note should strengthen the evidence needed for v2.3.0 private testnet readiness and the later v3.0.0 stable network target.

## Release intent

v2.2.14 should make the existing node easier to trust, rehearse, recover, and evaluate. The release is successful when operators can point to reproducible evidence for deterministic storage/replay behavior, snapshot export/import, restore drills, pruning safety, schema compatibility policy, real-libp2p testnet configuration, multi-node operation, and readiness burn-in criteria.

This release does not claim that PulseDAG is ready for v2.3.0 or v3.0.0 by itself. It defines the gates that later releases must satisfy before those milestones can be claimed.

## In scope

- Document and rehearse the hardening evidence required before the v2.3.0 private testnet readiness decision.
- Validate `cargo fmt --check` and `cargo test --workspace` as required release checks.
- Rehearse a three-node topology with restart/rejoin, propagation, and convergence notes.
- Validate deterministic persisted-block ordering by height, timestamp, and hash.
- Validate storage schema compatibility behavior for missing, valid, future, and corrupt metadata.
- Validate snapshot export/import, including restart and restore observations.
- Validate restore drills, pruning safety, and replay/order-independence behavior after restart and from persisted state.
- Validate testnet uses real `libp2p-real` networking rather than the dev loopback/skeleton runtime.
- Capture readiness evidence for a 14-day private-testnet burn-in gate.
- Capture the policy that smart contracts wait until after a stable testnet has completed a 30-day stable period.
- Keep the miner as an external standalone process and keep pool behavior outside the miner.

## Explicitly out of scope

- Public testnet launch or public-network readiness claims.
- Smart contract execution, VM/runtime selection, contract deployment, or contract RPCs.
- Pool server implementation, pool payout logic, pool accounting, or pool policy embedded in the miner.
- Consensus compatibility claims beyond the documented PulseDAG rules and evidence.
- Opportunistic feature expansion that does not directly support v2.3.0/v3.0.0 gates.
- Declaring v2.3.0 readiness without burn-in evidence and release-review sign-off.
- Declaring v3.0.0 readiness without the stable-network gates in `ROADMAP_V3_0_0.md`.

## v2.3.0 private testnet readiness gates

v2.2.14 establishes the evidence format for the v2.3.0 decision. Before v2.3.0 can be treated as private-testnet ready, the project must show:

1. Required checks pass: `cargo fmt --check` and `cargo test --workspace`.
2. A three-node rehearsal runs with documented startup, peer discovery/connectivity, block propagation, restart/rejoin, and convergence results.
3. Mining template and submit validation is exercised against an external miner flow, including stale/invalid submission behavior.
4. Snapshot export/import is exercised and documented with restart/restore evidence.
5. Replay/order-independence behavior is exercised after restart and under varied block/transaction arrival order.
6. A 14-day burn-in completes with no unresolved Sev-1 consensus, storage, replay, sync, or mining-template incidents.
7. Open warnings are either resolved or explicitly carried as private-testnet limitations with owner and follow-up.

## v3.0.0 foundation gates introduced here

v2.2.14 also records the early gates that prevent premature v3 claims:

- v3.0.0 requires stable network evidence, not only local tests.
- v3.0.0 requires successful private testnet operation and subsequent stable-testnet burn-in.
- Smart contracts are post-stable-testnet work and must not precede a 30-day stable testnet period.
- The miner remains external to the node.
- Pool logic remains outside the miner.
- Release evidence must show deterministic replay, snapshot recovery, mining submit behavior, and multi-node convergence.

## Miner and pool boundary

PulseDAG keeps mining as a node RPC contract consumed by an external miner. The node owns block validation, template creation, and submit acceptance/rejection. The miner owns work execution and local device orchestration.

The miner must not become a pool server. Pool coordination, payout logic, account management, share accounting, and pool-specific policy are separate systems and are out of scope for the miner and v2.2.14.

## Smart contract boundary

Smart contracts are intentionally deferred. No smart contract runtime, VM, precompile set, deployment transaction, gas model, or contract API should be added before the stable testnet has burned in for at least 30 days. Contract planning may remain conceptual, but implementation belongs after stable-network evidence exists.

## Release outputs

v2.2.14 should leave behind:

- A completed closing checklist in `CLOSING_CHECKLIST_V2_2_14.md`.
- Updated version positioning in `VERSION_MATRIX.md`.
- Storage migration policy in `STORAGE_MIGRATION_POLICY_V2_2_14.md`.
- Automated release evidence script at `scripts/v2-2-14-release-evidence.sh`.
- v3 gate documentation in `ROADMAP_V3_0_0.md`.
- Evidence links or notes for the required checks, rehearsals, snapshot/replay drills, mining validation, and burn-in status.
