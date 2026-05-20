# PulseDAG v2.2.17 current status

This repository is aligned to the **v2.2.17 API/operator/security hardening closeout** and the long-term path to **v3.0 as the first long-lived functional PulseDAG core**.

## Current status

- Active PoW identity: **kHeavyHash**.
- PoW engine framing: **Kaspa-based integration path** adapted for PulseDAG canonical headers.
- Acceptance semantics: **256-bit hash vs 256-bit target comparison**.
- Current milestone: **v2.2.17 API/operator/security hardening (finalized closeout)**, building on v2.2.16 miner/node contract hardening evidence.
- Miner architecture: external `pulsedag-miner` only; no embedded node miner and no pool logic.
- v2.2.17 scope: RPC/API surface audit, public/operator/admin endpoint classification, admin endpoint lockdown, local-only defaults, optional operator auth, rate limiting, request size limits, CORS policy, safe config validation, diagnostics redaction, readiness/release endpoint hardening, operator runbook updates, and API/security hardening evidence collection.
- GPU mining: allowed only inside the standalone external miner, optional, experimental, gated behind the canonical PoW adapter, CPU-verified for every GPU-found nonce before submit, and not required for default builds or v2.2.16 closeout when no GPU is available.
- P2P architecture: real `libp2p-real` mode with chain-id isolated block, tx, and sync topics.
- Smart contracts: out of scope in v2.2.x.
- v2.2.15 is the sustained P2P rehearsal evidence release before miner/node contract hardening; **it does not claim v2.3.0 readiness**.
- v2.2.16 is the miner/node contract hardening release that feeds into API/operator/security hardening.
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
10. In v2.2.17, verify API/operator/security hardening outcomes: endpoint classification, admin lockdown defaults, optional operator auth behavior, request-size/rate-limit policy, CORS posture, redaction coverage, and readiness/release endpoint disclosure boundaries.
11. Collect `/health`, `/status`, `/p2p/status`, `/p2p/peers`, `/p2p/propagation`, `/sync/status`, and mining diagnostics when available.

## Key documents

- v2.2.16 roadmap: `docs/ROADMAP_V2_2_16.md`.
- v2.2.17 release notes: `docs/RELEASE_NOTES_V2_2_17.md`.
- v2.2.17 closing checklist: `docs/CLOSING_CHECKLIST_V2_2_17.md`.
- API baseline reference: `docs/API_V1.md`.
- Primary operator runbook: `docs/RUNBOOK.md`.
- Runbook index: `docs/runbooks/INDEX.md`.
- Repository cleanup policy: `docs/REPO_CLEANUP_V2_2_16.md`.
- Version positioning: `docs/VERSION_MATRIX.md`.
- Final PoW spec: `docs/POW_SPEC_FINAL.md`.
- Long-lived v3.0 core roadmap: `docs/ROADMAP_V3_0_LONG_LIVED_CORE.md`.

## Historical milestone documents

Historical v2.2.x notes, roadmaps, and checklists are kept under `docs/` for auditability and release evidence traceability.
