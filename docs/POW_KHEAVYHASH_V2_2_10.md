# kHeavyHash Notes for PulseDAG v2.2.10

This document is the operator/developer companion to `docs/POW_SPEC_FINAL.md`.

## Finalized in v2.2.10

- Public PoW identity remains `kHeavyHash`.
- Consensus path is Kaspa-based engine integration adapted to PulseDAG canonical headers.
- Validation is 256-bit target comparison.
- Node and miner are aligned on a single canonical PoW path.

## Not part of this claim

- No full Kaspa consensus parity claim.
- No production-readiness claim.
- No smart contracts.
- No in-miner pool logic.

## Runtime surfaces to verify

- `GET /pow` exposes active algorithm metadata.
- `POST /mining/template` returns canonical work package.
- `POST /mining/submit` enforces template lifecycle and final PoW checks.

## Common mismatch symptoms

- Miner reports local solution, node returns `invalid_pow`.
- Rapid `stale_template` after tip movement.
- `duplicate` on repeated submit of same solved block.
- Rejections caused by header mutation or target-decoding mismatch.

When these happen, verify miner and node are both built from the same v2.2.10 line.
