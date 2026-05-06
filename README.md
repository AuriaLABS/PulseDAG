# PulseDAG v2.2.11 P2P completion docs

This repository's documentation is aligned to the **v2.2.11 P2P completion** milestone for multi-node private-testnet preparation. Cargo workspace package metadata remains at `2.2.10` until a dedicated release/version-bump PR changes it.

## Current status

- Active PoW identity: **kHeavyHash**.
- PoW engine framing: **Kaspa-based integration path** adapted for PulseDAG canonical headers.
- Acceptance semantics: **256-bit hash vs 256-bit target comparison**.
- Miner architecture: external `pulsedag-miner` (no embedded pool logic).
- P2P architecture: real `libp2p-real` mode with chain-id isolated block, tx, and sync topics.
- v2.2.11 scope: P2P completion docs, three-node rehearsal runbook, and sync troubleshooting for private-testnet preparation.
- Smart contracts: out of scope in v2.2.x.
- Public mainnet readiness: not claimed.
- v2.3.0 readiness: not claimed; v2.3.0 remains the future private-testnet readiness decision milestone.

## P2P operator quick start

Build release binaries:

```bash
cargo build --workspace --release
```

Run the local three-node rehearsal:

```bash
scripts/v2_2_11_smoke_p2p.sh
```

Or start nodes manually:

```bash
scripts/v2_2_11_start_node_a.sh --clean
scripts/v2_2_11_start_node_b.sh --clean
scripts/v2_2_11_start_node_c.sh --clean
scripts/v2_2_11_start_miner_a.sh
```

Core checks:

```bash
curl -fsS http://127.0.0.1:18080/p2p/status
curl -fsS http://127.0.0.1:18080/sync/status
curl -fsS http://127.0.0.1:18081/sync/status
curl -fsS http://127.0.0.1:18082/sync/status
```

The rehearsal is healthy when all nodes share the same `chain_id`, run `libp2p-real`, connect to peers, and converge on `best_height` after the external miner advances node A.

## Mining flow (operator summary)

1. Start node.
2. Check `GET /pow`.
3. Request `POST /mining/template`.
4. Run external `pulsedag-miner`.
5. Submit via `POST /mining/submit` (miner does this automatically).
6. Verify chain movement with `/status`, `/p2p/status`, and `/sync/status`.

## Documentation

- P2P specification: `docs/P2P_SPEC_V2_2_11.md`.
- Three-node P2P rehearsal runbook: `docs/P2P_REHEARSAL_V2_2_11.md`.
- Sync recovery and troubleshooting: `docs/SYNC_RECOVERY_V2_2_11.md`.
- Final PoW spec: `docs/POW_SPEC_FINAL.md`.
- Mining rehearsal: `docs/MINING_REHEARSAL_V2_2_10.md`.
- Version positioning: `docs/VERSION_MATRIX.md`.
