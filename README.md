# PulseDAG v2.2.12 current status

This repository is aligned to the **v2.2.12 full private-testnet rehearsal and hardening milestone**.

## Current status

- Active PoW identity: **kHeavyHash**.
- PoW engine framing: **Kaspa-based integration path** adapted for PulseDAG canonical headers.
- Acceptance semantics: **256-bit hash vs 256-bit target comparison**.
- P2P milestone: **v2.2.11 P2P completion outputs are the baseline for v2.2.12 rehearsal and hardening**.
- Miner architecture: external `pulsedag-miner` (no embedded pool logic).
- P2P architecture: real `libp2p-real` mode with chain-id isolated block, tx, and sync topics.
- v2.2.12 scope: full private-testnet rehearsal, sustained multi-node/operator validation, runbook hardening, and release evidence capture.
- Smart contracts: out of scope in v2.2.x.
- v2.2.12 rehearses and hardens the completed P2P path; **it does not claim official private-testnet readiness**.
- v2.3.0 remains the private-testnet readiness decision milestone.

## Full private-testnet rehearsal flow (operator summary)

1. Build the workspace release binaries.
2. Start node A in real `libp2p-real` mode.
3. Start nodes B and C connected to A.
4. Verify peer connectivity with `/p2p/status` and node state with `/status`.
5. Run the external `pulsedag-miner` against node A.
6. Wait for node A height to increase.
7. Verify B/C receive or sync the block.
8. Restart B and verify it catches up.
9. Collect `/health`, `/status`, `/p2p/status`, and `/sync/status` from A/B/C.

v2.2.12 roadmap: `docs/ROADMAP_V2_2_12.md`.
v2.2.11 P2P baseline rehearsal: `docs/P2P_REHEARSAL_V2_2_11.md`.
v2.2.11 smoke test baseline: `docs/SMOKE_TEST_V2_2_11.md`.
v2.2.11 release notes baseline: `docs/RELEASE_NOTES_V2_2_11.md`.
v2.2.11 closing checklist baseline: `docs/CLOSING_CHECKLIST_V2_2_11.md`.
Version positioning: `docs/VERSION_MATRIX.md`.
Final PoW spec: `docs/POW_SPEC_FINAL.md`.
