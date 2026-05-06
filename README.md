# PulseDAG v2.2.11 current status

This repository is aligned to the **v2.2.11 P2P completion milestone**.

## Current status

- Active PoW identity: **kHeavyHash**.
- PoW engine framing: **Kaspa-based integration path** adapted for PulseDAG canonical headers.
- Acceptance semantics: **256-bit hash vs 256-bit target comparison**.
- P2P milestone: **block announce/request/data flow, tx relay, tip exchange, missing parent recovery, orphan handling, peer scoring/backoff, duplicate suppression, and diagnostics are documented for v2.2.11 closeout**.
- Miner architecture: external `pulsedag-miner` (no embedded pool logic).
- Smart contracts: out of scope in v2.2.x.
- v2.2.11 closes P2P completion; **v2.2.12 is the full private-testnet rehearsal and hardening handoff**.
- v2.3.0 remains the private-testnet readiness decision milestone.

## P2P completion flow (operator summary)

1. Build the workspace release binaries.
2. Start node A in real `libp2p-real` mode.
3. Start nodes B and C connected to A.
4. Verify peer connectivity with `/p2p/status` and node state with `/status`.
5. Run the external `pulsedag-miner` against node A.
6. Wait for node A height to increase.
7. Verify B/C receive or sync the block.
8. Restart B and verify it catches up.
9. Collect `/health`, `/status`, `/p2p/status`, and `/sync/status` from A/B/C.

Three-node rehearsal: `docs/P2P_REHEARSAL_V2_2_11.md`.
Smoke test: `docs/SMOKE_TEST_V2_2_11.md`.
Release notes: `docs/RELEASE_NOTES_V2_2_11.md`.
Closing checklist: `docs/CLOSING_CHECKLIST_V2_2_11.md`.
Version positioning: `docs/VERSION_MATRIX.md`.
Final PoW spec: `docs/POW_SPEC_FINAL.md`.
