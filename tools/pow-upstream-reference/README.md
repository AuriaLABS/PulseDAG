# Separately locked upstream KHeavyHash oracle — development only

The pinned Rusty Kaspa **2.0.1** source, revision
`cfafeb4c093fa37a303f1b9f19c58f986b870ce3`, is preserved here solely
for *independent parity tests*, never used by the shipped node, wallet, miner,
P2P library or consensus decision. This folder declares its **own** Cargo
`[workspace]` and must have its **own resolver-generated Cargo.lock**, separate
from the public-node Cargo.lock.

## Why this isolation is necessary

The old test-only `pulsedag-core` dev-deps pinned
`kaspa-hashes/pow -> js-sys =0.3.77`. The official libp2p 0.57.1 parent
requires `libp2p-swarm -> wasm-bindgen-futures -> js-sys =0.3.85`.
Both exact versions cannot be selected in one root Cargo.lock. An isolated
reference-test workspace retains *both* security properties: original full
PoW parity coverage, and the ability to move the public P2P dependencies
away from vulnerable `hickory-proto 0.25.2`.

Run this **outside** the root workspace test suite:

```bash
cargo test --manifest-path tools/pow-upstream-reference/Cargo.toml --test native_pow_backend_migration
cargo test --release --manifest-path tools/pow-upstream-reference/Cargo.toml --test native_pow_backend_migration -- --ignored --nocapture
cargo clippy --manifest-path tools/pow-upstream-reference/Cargo.toml --test native_pow_backend_migration -- -D warnings
```

The first test includes 96 seeds × 5 nonce boundary values (480 comparisons)
between original Kaspa and PulseDAG native KHeavyHash, plus independent
chain-domain/target checks. The release-only ignored test benchmarks both
on one runner to detect a >50% CPU-prepared mining performance regression.

**Security constraints:** This tool is not a consensus authority, not a
binary/release attestation, not a network service, and **never a GO**.
Before actually removing `pulsedag-core`'s original dev dependencies and
moving its parity CI here, a reviewer must validate an exact generated
separate Cargo.lock plus the main Cargo.lock, the genuine 480 parity
comparisons, release throughput and every native-v3/protocol regression.
No `cargo audit` ignore or live monetary/contract activation is permitted.

Public testnet NO-GO · mainnet NO-GO · monetary mining OFF · contracts INACTIVE.
