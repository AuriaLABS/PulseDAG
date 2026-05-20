# Roadmap v2.2.10 — Final PoW Completion

## Milestone sequence

- v2.2.9: private-testnet rehearsal baseline.
- **v2.2.10: final PoW completion.**
- **v2.2.11: P2P completion starts.**
- v2.3.0: official private-testnet readiness milestone.

## Objective

Close PoW for PulseDAG with one canonical kHeavyHash-based consensus path and a complete mining runbook aligned with actual node/miner behavior.

## In scope (v2.2.10)

1. Finalize Kaspa-based kHeavyHash engine integration path used by PulseDAG.
2. Enforce 256-bit target comparison semantics.
3. Finalize canonical PulseDAG header adapter for PoW preimage construction.
4. Align node/miner flow and docs (`/pow`, `/mining/template`, `/mining/submit`).
5. Publish final PoW spec and rehearsal runbook.

## Out of scope

- Full Kaspa consensus compatibility claims.
- Production public-network readiness claims.
- Smart contracts.
- Pool logic in miner.
- P2P completion work (starts in v2.2.11).

## Exit criteria

- One active PoW truth: kHeavyHash.
- No active-doc ambiguity around BLAKE3/Keccak as consensus PoW.
- Mining runbook matches real miner CLI and node endpoints.
- Troubleshooting guidance covers invalid/stale/duplicate/mismatch classes.

## Handoff

v2.2.10 closes PoW scope. v2.2.11 begins P2P completion.
