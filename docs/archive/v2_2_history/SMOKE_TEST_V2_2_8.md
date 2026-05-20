# PulseDAG v2.2.8 Smoke Test (Pre-Private-Testnet Hardening)

## Automated checks

```bash
cargo fmt --check
cargo test --workspace
cargo build --workspace
```

## Manual smoke

### Single node
1. Start `pulsedagd` using private or local profile.
2. `curl /health`
3. `curl /status`
4. `curl /release`
5. `curl /pow`
6. `curl /runtime` (if enabled)
7. `curl /metrics` (if enabled)

### Mining
1. Request `/mining/template`.
2. Run external `pulsedag-miner`.
3. Submit valid work.
4. Confirm height/tips/block count changes after valid work acceptance.
5. Submit/attempt invalid work and confirm rejection.

### P2P partial
1. Start node A with private profile.
2. Start node B with different ports.
3. Connect B to A with bootnode/peer configuration.
4. Check `/p2p/status` (or equivalent).
5. Record whether propagation is fully automated or still partial.

## Scope honesty
- v2.2.8 smoke is pre-private-testnet hardening smoke.
- v2.3.0 remains the complete private-testnet milestone.
