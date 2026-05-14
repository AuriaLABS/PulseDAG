# PulseDAG v2.2.14 release checklist

This checklist closes v2.2.14 as the v3 foundation hardening release. Do not use this checklist to claim v2.3.0 or v3.0.0 readiness by itself; it records the evidence required before those later decisions.

## Required local checks

- [ ] `cargo fmt --check` passes.
- [ ] `cargo test --workspace` passes.
- [ ] Any skipped or environment-limited checks are documented with reason, owner, and follow-up.

## Three-node rehearsal

- [ ] Three nodes start from clean or documented state.
- [ ] Nodes connect using the intended private-testnet topology settings.
- [ ] Blocks propagate across all nodes.
- [ ] At least one node restart/rejoin is performed.
- [ ] Rejoined node converges to the same selected tip/best height as peers.
- [ ] Rehearsal logs, commands, configuration, and observed results are attached to release evidence.

## Mining template and submit validation

- [ ] Mining template retrieval succeeds through the node RPC/API surface intended for the external miner.
- [ ] A valid external-miner submit path is exercised or explicitly documented as pending with blocker.
- [ ] Invalid, stale, malformed, or duplicate submit behavior is exercised where supported.
- [ ] Rejection reasons are observable and suitable for operator diagnostics.
- [ ] Evidence confirms the miner remains external to the node.
- [ ] Evidence confirms no pool logic is added to the miner.

## Snapshot export/import

- [ ] Snapshot export is performed from a running or safely stopped node using the documented path.
- [ ] Snapshot import/restore is performed into a fresh or documented target state.
- [ ] Restored node starts successfully.
- [ ] Restored node reports expected height/tip/state metadata.
- [ ] Any partial, corrupted, or incompatible snapshot behavior is documented if tested.

## Replay and order-independence

- [ ] Restart replay from persisted state succeeds.
- [ ] Replayed state matches expected selected tip/best height/state metadata.
- [ ] Order-independence evidence is captured for varied block/transaction arrival order or a documented deterministic replay test.
- [ ] Any replay warnings are explained and triaged before release sign-off.

## Burn-in gates

- [ ] 14-day burn-in plan is linked for v2.3.0 private-testnet readiness.
- [ ] 14-day burn-in results are complete or explicitly marked as not yet complete for v2.2.14.
- [ ] No unresolved Sev-1 consensus, storage, replay, sync, or mining-template incident remains open before claiming private-testnet readiness.
- [ ] 30-day stable testnet gate is recorded before any smart contract implementation work may begin.
- [ ] Smart contracts are explicitly documented as post-stable-testnet work.

## Scope and non-goal confirmation

- [ ] v2.2.14 is described as v3 foundation hardening, not a feature-random release.
- [ ] v2.3.0 is described as a private-testnet readiness decision, not an automatic public launch.
- [ ] v3.0.0 gates are documented in `ROADMAP_V3_0_0.md`.
- [ ] Public testnet launch is out of scope.
- [ ] Smart contracts are out of scope until after stable testnet burn-in.
- [ ] The miner remains external.
- [ ] Pool logic is not part of the miner.

## Release sign-off

- [ ] Version matrix updated for v2.2.14 positioning.
- [ ] Release evidence links are collected.
- [ ] Known limitations are documented.
- [ ] Follow-up issues exist for any deferred v2.3.0/v3.0.0 gate.
- [ ] Maintainer release decision is recorded.
