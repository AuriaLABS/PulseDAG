# Native-v3 KHeavyHash: Kaspa-independent candidate (research only)

Issue: [#1387](https://github.com/AuriaLABS/PulseDAG/issues/1387). Status: **EXPERIMENTAL / NOT A PRODUCTION BACKEND / NO GO**.

The fixed Rusty Kaspa `kaspa-pow` parent line reaches `kaspa-consensus-core`, `kaspa-consensus-client` and `workflow-wasm`, leaving the legacy `async-std` and archived `pin-utils` graph compiler-reachable. Removing only a feature on `kaspa-hashes` would not solve the problem.

To prepare **a reviewed, separately reproducible migration**, the isolated `crates/pulsedag-core/src/pow_kaspa_free_candidate.rs` reproduces a bounded KHeavyHash calculation with standard Rust and the existing `sha3` crate, **without referencing Kaspa types**. It starts from the existing fully independent integration-test oracle and preserves its pre-PoW Keccak, nonce cSHAKE, Xoshiro matrix, rank rejection, XOR and final cSHAKE stages. The exported `experimental_kheavyhash_digest` returns `None` after 32 unsuccessful full-rank matrix attempts; **this is a diagnostic budget, not an agreed consensus rule or a substitute for upstream's unbounded generation**.

The unchanged independent integration test `native_v3_kaspa_free_pow_oracle_trial.rs` now also compares the reusable candidate against **both** the previously frozen seven cross-language vectors and canonical Rusty Kaspa backend for the same PoW seeds, including u64 nonce extremes and an altered chain ID. The original independent test oracle still computes its own result, and KHeavyHash consensus remains **only** in `pow.rs`.

## Scope and limits

- No `Cargo.toml`, `Cargo.lock`, production hash engine, compact-target acceptance, mined-block validation, miner, P2P, startup, feature flags or contracts are altered.
- **Security is NOT yet remediated**: `kaspa-hashes`, `kaspa-pow`, `workflow-*`, `async-std`, `pin-utils` remain in the same active dependency graph. The exact compiled reachability/security inventory must keep reporting them.
- Before any switch, the candidate needs production-grade matrix-retry/DoS and latency policy, a bit-identical differential test across many target levels/chain IDs/platforms, CPU/GPU parity, no overflow/UB, external source review, a reviewed dependency graph + regenerated lockfile, `cargo tree` and `cargo audit` clean compiler-artifact evidence for all launch roots on Linux and Windows, plus no contracts interference.
- A successful isolated candidate test is not evidence of native-v3 GHOSTDAG/GD-08 canonical monetary policy, checkpoint authenticity, mainnet or public testnet GO.

No monetary-v3 activation is granted. **Public testnet NO-GO; mainnet NO-GO; mining OFF; smart contracts INACTIVE.**
