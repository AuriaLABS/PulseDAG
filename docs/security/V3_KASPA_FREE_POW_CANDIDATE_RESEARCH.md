# Native-v3 KHeavyHash: Kaspa-independent candidate (research only)

Issue: [#1387](https://github.com/AuriaLABS/PulseDAG/issues/1387). Status: **EXPERIMENTAL / NOT A PRODUCTION BACKEND / NO GO**.

The fixed Rusty Kaspa `kaspa-pow` parent line reaches `kaspa-consensus-core`, `kaspa-consensus-client` and `workflow-wasm`, leaving the legacy `async-std` and archived `pin-utils` graph compiler-reachable. Removing only a feature on `kaspa-hashes` would not solve the problem.

To prepare **a reviewed, separately reproducible migration**, the isolated `crates/pulsedag-core/src/pow_kaspa_free_candidate.rs` reproduces a bounded KHeavyHash calculation with standard Rust and the existing `sha3` crate, **without referencing Kaspa types**. It starts from the existing fully independent integration-test oracle and preserves its pre-PoW Keccak, nonce cSHAKE, Xoshiro matrix, rank rejection, XOR and final cSHAKE stages. The **integration-test-only** `experimental_kheavyhash_digest` returns `None` after 32 unsuccessful full-rank matrix attempts; **this is a diagnostic budget, not an agreed consensus rule or a substitute for upstream's unbounded generation**.

The independent integration test `native_v3_kaspa_free_pow_oracle_trial.rs` uses a file-local `#[path = "../src/pow_kaspa_free_candidate.rs"]` module import. The candidate is **not exported from `pulsedag-core/src/lib.rs`**, so daemon, miner and other production crates cannot link to it as an ordinary library API. The test now compares the reusable candidate against **both** the previously frozen seven cross-language vectors and canonical Rusty Kaspa backend for the same PoW seeds, including u64 nonce extremes and an altered chain ID. The original independent test oracle still computes its own result, and KHeavyHash consensus remains **only** in `pow.rs`.

## Additional deterministic primitive-differential coverage (NOT production certification)

The standalone Rust integration test now additionally creates **16 reproducible test-only pre-PoW byte seeds** (Keccak256 of versioned test labels) and evaluates **three unique nonce values per seed**, including zero, a nontrivial 64-bit nonce and `u64::MAX`: **48 extra KHeavyHash evaluations**. The independently implemented test oracle, the isolated research candidate and the **unchanged canonical Kaspa engine** must all compute identical 256-bit hashes. A compact-target check alternates two valid difficulty encodings, and repeated nonce results are rejected. The existing seven cross-language frozen native-v3 header fixtures, legacy v1/v2 header tests and integer matrix-rank witnesses remain unchanged. Strict CI requires all **six** tests to pass, not only a generic zero exit code.

These deterministic seeds are **not actual network block headers or a statistically meaningful security fuzz campaign**. Passing does not establish production backend parity under every input, matrix retry distribution, throughput, CPU/GPU/Windows equivalence, or removal of any RustSec exposure. In particular, no change to `kaspa-pow` wiring, monetary runtime or consensus authority is authorized by this test.

## Scope and limits

- Fail-closed CI inspects Rust files under `crates/` and `apps/` to reject imports or invocations outside this one integration test; a future candidate promotion requires explicit separate review, not just adding a runtime call. This is a textual guard, **not** a compiler-enforced proof of non-reachability.
- No `Cargo.toml`, `Cargo.lock`, production hash engine, compact-target acceptance, mined-block validation, miner, P2P, startup, feature flags or contracts are altered.
- **Security is NOT yet remediated**: `kaspa-hashes`, `kaspa-pow`, `workflow-*`, `async-std`, `pin-utils` remain in the same active dependency graph. The exact compiled reachability/security inventory must keep reporting them.
- Before any switch, the candidate needs production-grade matrix-retry/DoS and latency policy, a bit-identical differential test across many target levels/chain IDs/platforms, CPU/GPU parity, no overflow/UB, external source review, a reviewed dependency graph + regenerated lockfile, `cargo tree` and `cargo audit` clean compiler-artifact evidence for all launch roots on Linux and Windows, plus no contracts interference.
- A successful isolated candidate test is not evidence of native-v3 GHOSTDAG/GD-08 canonical monetary policy, checkpoint authenticity, mainnet or public testnet GO.

No monetary-v3 activation is granted. **Public testnet NO-GO; mainnet NO-GO; mining OFF; smart contracts INACTIVE.**
