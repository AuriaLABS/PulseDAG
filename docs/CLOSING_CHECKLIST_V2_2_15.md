# PulseDAG v2.2.15 closing checklist

v2.2.15 closes only when PulseDAG has sustained P2P multi-node rehearsal evidence. This checklist is a release gate for v2.2.15 and is not a v2.3.0 readiness claim.

## Version and scope gate

- [ ] `VERSION` is `v2.2.15`.
- [ ] Cargo workspace version is `2.2.15`.
- [ ] Cargo workspace license metadata remains `ISC`.
- [ ] `README.md` and `docs/VERSION_MATRIX.md` describe v2.2.15 as the current milestone.
- [ ] v2.2.14 is described as storage/replay hardening closure, not the current milestone.
- [ ] v2.2.16 remains miner/node contract hardening.
- [ ] v2.3.0 remains a readiness decision only, not an automatic launch.
- [ ] No smart contracts are added.
- [ ] No contract runtime is enabled.
- [ ] No pool logic is added.
- [ ] The miner remains a standalone external application.
- [ ] No consensus-rule change is included unless it fixes a documented safety bug with tests.

## Required command gate

Run these commands from the repository root and attach output or CI links:

```bash
cargo fmt --all -- --check
cargo test --workspace
cargo build --workspace
bash scripts/v2-2-15-p2p-churn-rejoin-evidence.sh
bash scripts/v2-2-15-p2p-lag-recovery-evidence.sh
```

- [ ] `cargo fmt --all -- --check` passes.
- [ ] `cargo test --workspace` passes.
- [ ] `cargo build --workspace` passes.
- [ ] `bash scripts/v2-2-15-p2p-churn-rejoin-evidence.sh` passes and writes evidence under `evidence/v2.2.15/`.
- [ ] `bash scripts/v2-2-15-p2p-lag-recovery-evidence.sh` passes and writes evidence under `evidence/v2.2.15/`.

## Release evidence script gate

Run the current release evidence script from the repository root and attach the transcript. If `scripts/v2-2-14-release-evidence.sh` is still the latest script, label the output as the inherited v2.2.14 evidence baseline used for v2.2.15 opening.

```bash
./scripts/v2-2-14-release-evidence.sh
```

- [ ] Release evidence script output is captured.
- [ ] Any release evidence script failure is triaged as blocking or explicitly waived with owner, reason, and follow-up.

## Sustained P2P rehearsal gate

Use `docs/P2P_REHEARSAL_PLAN_V2_2_15.md` as the operator plan.

- [ ] 3-node local rehearsal passes.
- [ ] 5-node local rehearsal passes, if practical.
- [ ] If the 5-node local rehearsal is not practical, the reason and follow-up are recorded.
- [ ] All rehearsal nodes use real `libp2p-real` networking.
- [ ] All nodes in the same rehearsal use the same intended chain id, except for the explicit chain-id isolation test.
- [ ] Mining, if used to create new blocks, is performed through external `pulsedag-miner` or equivalent external RPC client behavior, not embedded node mining.

## Required P2P evidence

Attach logs, endpoint snapshots, command transcripts, and operator notes for each item:

- [ ] Node restart/rejoin evidence from `scripts/v2-2-15-p2p-churn-rejoin-evidence.sh`.
- [ ] Lagging node recovery evidence from `scripts/v2-2-15-p2p-lag-recovery-evidence.sh`.
- [ ] Peer churn evidence.
- [ ] Chain-id isolation evidence.
- [ ] Sync convergence evidence.
- [ ] Peer diagnostics evidence from `/p2p/status` and `/p2p/peers` when available, including local peer id, peer count, connected peer ids, real-network semantics, and recovery counters.
- [ ] Propagation or topic diagnostics from `/p2p/propagation`, `/p2p/topics`, or available replacement endpoints when practical.
- [ ] Sync diagnostics from `/sync/status` and `/sync/missing` when available.
- [ ] Current height, selected tip, chain id, and persisted block count snapshots prove the rejoining or lagging node recovered without manual database deletion.
- [ ] Final `/health` and `/status` snapshots for every node.

## Defect gate

- [ ] No unresolved Sev-1 consensus defect remains open.
- [ ] No unresolved Sev-1 sync defect remains open.
- [ ] Any unresolved Sev-2 P2P, sync, storage, or operator defect is documented with impact, owner, and follow-up milestone.
- [ ] Any rehearsal failure is either fixed and rerun or recorded as a blocking release issue.

## Closeout decision

- [ ] Release notes are updated in `docs/RELEASE_NOTES_V2_2_15.md`.
- [ ] Roadmap scope is updated in `docs/ROADMAP_V2_2_15.md`.
- [ ] Rehearsal plan is updated in `docs/P2P_REHEARSAL_PLAN_V2_2_15.md`.
- [ ] Evidence links are collected in the release issue, PR, or release artifact index.
- [ ] The closeout summary explicitly states that v2.2.15 provides sustained P2P rehearsal evidence and does not claim v2.3.0 readiness by itself.
