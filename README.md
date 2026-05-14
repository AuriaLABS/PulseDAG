# PulseDAG v2.2.16 current status

This repository is aligned to the **v2.2.16 miner/node contract hardening milestone** and the long-term path to **v3.0 as the first long-lived functional PulseDAG core**.

## Current status

- Active PoW identity: **kHeavyHash**.
- PoW engine framing: **Kaspa-based integration path** adapted for PulseDAG canonical headers.
- Acceptance semantics: **256-bit hash vs 256-bit target comparison**.
- Current milestone: **v2.2.16 miner/node contract hardening**, building on v2.2.15 sustained P2P rehearsal evidence.
- Miner architecture: external `pulsedag-miner` only; no embedded node miner and no pool logic.
- v2.2.16 scope: canonical miner/node template contract, submit validation, stale template handling, target/difficulty checks, error taxonomy, miner diagnostics, external miner integration evidence, CPU miner hardening, and optional experimental GPU miner work only if feature-gated and non-blocking.
- GPU mining: allowed only inside the standalone external miner, optional, experimental, CPU-verified before submit, and not required for default builds or v2.2.16 closeout when no GPU is available.
- P2P architecture: real `libp2p-real` mode with chain-id isolated block, tx, and sync topics.
- v2.2.12 scope: full private-testnet rehearsal, sustained multi-node/operator validation, runbook hardening, and release evidence capture.
- v2.2.13 scope: consensus/DAG safety audit, invariant and validation evidence, orphan adoption checks, deterministic tip selection, replay/order-independence review, and compatibility-claim guardrails.
- v2.2.14 scope: storage, deterministic replay ordering, snapshot/restore, pruning safety, migration-policy hardening, testnet profile correctness, and release evidence scripting.
- v2.2.15 scope: sustained P2P operation across multiple nodes, including churn, restart/rejoin, lag recovery, convergence, peer diagnostics, and chain-id isolation evidence.
- v2.2.15 evidence bundle: passed on Ubuntu/WSL before opening v2.2.16.
- Smart contracts: out of scope in v2.2.x.
- v2.2.12 rehearsed and hardened the completed P2P path; **it did not claim official private-testnet readiness**.
- v2.2.14 is the storage/replay hardening closure before the sustained P2P rehearsal release.
- v2.2.15 is the sustained P2P rehearsal evidence release before miner/node contract hardening; **it does not claim v2.3.0 readiness**.
- v2.2.16 is the miner/node contract hardening release before API/operator/security hardening.
- v2.3.0 remains the private-testnet readiness decision milestone; it is not an automatic public launch.
- Long-term v3.0 goal: a durable core that can run for years with stable node, PoW, external miner, P2P, sync, storage, snapshots, pruning policy, operator RPC, release evidence, and upgrade policy.

## Full private-testnet rehearsal flow (operator summary)

1. Build the workspace release binaries.
2. Start node A in real `libp2p-real` mode.
3. Start nodes B and C connected to A.
4. Verify peer connectivity with `/p2p/status` and node state with `/status`.
5. Run the external `pulsedag-miner` against node A.
6. Wait for node A height to increase.
7. Verify B/C receive or sync the block.
8. Restart B and verify it catches up.
9. Add churn, lagging-node recovery, chain-id isolation, and convergence checks for v2.2.15 evidence.
10. In v2.2.16, verify miner template fetch, submit validation, stale template rejection, miner restart/reconnect, CPU miner evidence, and optional GPU smoke evidence when available.
11. Collect `/health`, `/status`, `/p2p/status`, `/p2p/peers`, `/p2p/propagation`, `/sync/status`, and mining diagnostics when available.

v2.2.12 roadmap: [docs/ROADMAP_V2_2_12.md](docs/ROADMAP_V2_2_12.md).
Long-lived v3.0 core roadmap: [docs/ROADMAP_V3_0_LONG_LIVED_CORE.md](docs/ROADMAP_V3_0_LONG_LIVED_CORE.md).
v2.2.12 P2P rehearsal: `docs/P2P_REHEARSAL_V2_2_12.md`.
v2.2.12 smoke test: `docs/SMOKE_TEST_V2_2_12.md`.
v2.2.12 sync recovery rehearsal: `docs/SYNC_RECOVERY_V2_2_12.md`.
v2.2.12 release notes: `docs/RELEASE_NOTES_V2_2_12.md`.
v2.2.12 closing checklist: `docs/CLOSING_CHECKLIST_V2_2_12.md`.
v2.2.13 release notes: `docs/RELEASE_NOTES_V2_2_13.md`.
v2.2.13 DAG safety invariants: `docs/DAG_SAFETY_INVARIANTS_V2_2_13.md`.
v2.2.13 closing checklist: `docs/CLOSING_CHECKLIST_V2_2_13.md`.
v2.2.14 roadmap: `docs/ROADMAP_V2_2_14.md`.
v2.2.14 release notes: `docs/RELEASE_NOTES_V2_2_14.md`.
v2.2.14 closing checklist: `docs/CLOSING_CHECKLIST_V2_2_14.md`.
v2.2.14 storage migration policy: `docs/STORAGE_MIGRATION_POLICY_V2_2_14.md`.
v2.2.15 roadmap: `docs/ROADMAP_V2_2_15.md`.
v2.2.15 release notes: `docs/RELEASE_NOTES_V2_2_15.md`.
v2.2.15 closing checklist: `docs/CLOSING_CHECKLIST_V2_2_15.md`.
v2.2.15 P2P rehearsal plan: `docs/P2P_REHEARSAL_PLAN_V2_2_15.md`.
v2.2.16 roadmap: `docs/ROADMAP_V2_2_16.md`.
v2.2.16 release notes: `docs/RELEASE_NOTES_V2_2_16.md`.
v2.2.16 closing checklist: `docs/CLOSING_CHECKLIST_V2_2_16.md`.
v2.2.16 miner/node contract: `docs/MINER_NODE_CONTRACT_V2_2_16.md`.
v2.2.16 GPU backlog: `docs/MINER_GPU_BACKLOG_V2_2_16.md`.
Version positioning: [docs/VERSION_MATRIX.md](docs/VERSION_MATRIX.md).
Final PoW spec: `docs/POW_SPEC_FINAL.md`.