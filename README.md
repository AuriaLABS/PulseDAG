# PulseDAG v2.2.16 current status

This repository is aligned to the **v2.2.16 miner/node contract hardening milestone** and the long-term path to **v3.0 as the first long-lived functional PulseDAG core**.

## Current status

- Active PoW identity: **kHeavyHash**.
- PoW engine framing: **Kaspa-based integration path** adapted for PulseDAG canonical headers.
- Acceptance semantics: **256-bit hash vs 256-bit target comparison**.
- Current milestone: **v2.2.16 miner/node contract hardening**, building on v2.2.15 sustained P2P rehearsal evidence.
- Miner architecture: external `pulsedag-miner` only; no embedded node miner and no pool logic.
- v2.2.16 scope: external miner/node contract hardening, mining template freshness and expiry behavior, stable submit rejection taxonomy, miner telemetry and worker metrics, multi-miner rehearsal, CPU miner hardening, optional miner performance evidence, and optional experimental GPU backend planning only after the canonical PoW adapter exists.
- GPU mining: allowed only inside the standalone external miner, optional, experimental, gated behind the canonical PoW adapter, CPU-verified for every GPU-found nonce before submit, and not required for default builds or v2.2.16 closeout when no GPU is available.
- P2P architecture: real `libp2p-real` mode with chain-id isolated block, tx, and sync topics.
- Smart contracts: out of scope in v2.2.x.
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
10. In v2.2.16, verify miner template fetch, template freshness/expiry behavior, submit validation, stable rejection codes, miner restart/reconnect, CPU miner evidence, telemetry and worker metrics, multi-miner rehearsal, optional miner performance JSON/CSV evidence, and optional GPU smoke evidence only when the canonical PoW adapter and host GPU support are available.
11. Collect `/health`, `/status`, `/p2p/status`, `/p2p/peers`, `/p2p/propagation`, `/sync/status`, and mining diagnostics when available.

## Key documents

- v2.2.16 roadmap: `docs/ROADMAP_V2_2_16.md`.
- v2.2.16 release notes: `docs/RELEASE_NOTES_V2_2_16.md`.
- v2.2.16 closing checklist: `docs/CLOSING_CHECKLIST_V2_2_16.md`.
- v2.2.16 miner/node contract: `docs/MINER_NODE_CONTRACT_V2_2_16.md`.
- v2.2.16 miner benchmark harness: `docs/MINER_BENCHMARK_V2_2_16.md`.
- v2.2.16 GPU backlog: `docs/MINER_GPU_BACKLOG_V2_2_16.md`.
- Repository cleanup policy: `docs/REPO_CLEANUP_V2_2_16.md`.
- Version positioning: `docs/VERSION_MATRIX.md`.
- Final PoW spec: `docs/POW_SPEC_FINAL.md`.
- Long-lived v3.0 core roadmap: `docs/ROADMAP_V3_0_LONG_LIVED_CORE.md`.

## Historical milestone documents

Historical v2.2.x notes, roadmaps, and checklists are kept under `docs/` for auditability and release evidence traceability.
