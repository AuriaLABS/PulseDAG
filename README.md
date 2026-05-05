# PulseDAG v2.2.10 current status

This repository is aligned to the **v2.2.10 final PoW completion milestone**.

## Current status

- Active PoW identity: **kHeavyHash**.
- PoW engine framing: **Kaspa-based integration path** adapted for PulseDAG canonical headers.
- Acceptance semantics: **256-bit hash vs 256-bit target comparison**.
- Miner architecture: external `pulsedag-miner` (no embedded pool logic).
- Smart contracts: out of scope in v2.2.x.
- v2.2.10 closes PoW; **v2.2.11 starts P2P completion**.

## Mining flow (operator summary)

1. Start node.
2. Check `GET /pow`.
3. Request `POST /mining/template`.
4. Run `pulsedag-miner`.
5. Submit via `POST /mining/submit` (miner does this automatically).
6. Verify chain movement with `/status` and `/tips`.

Detailed runbook: `docs/MINING_REHEARSAL_V2_2_10.md`.
Final PoW spec: `docs/POW_SPEC_FINAL.md`.
Version positioning: `docs/VERSION_MATRIX.md`.
