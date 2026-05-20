# PulseDAG v2.2.10 Closing Checklist

- [ ] `VERSION` is `v2.2.10`.
- [ ] `Cargo.toml` workspace version is `2.2.10`.
- [ ] `/pow` reports `kHeavyHash` in active-devnet framing.
- [ ] `/mining/template` includes `target_hex`.
- [ ] Miner uses `pulsedag-core` PoW implementation.
- [ ] Valid work is accepted.
- [ ] Invalid work is rejected.
- [ ] Mutated work is rejected.
- [ ] Duplicate work is not accepted twice.
- [ ] `cargo fmt --check` passes.
- [ ] `cargo test --workspace` passes.
- [ ] `cargo build --workspace` passes.
- [ ] Optional `cargo clippy --workspace --all-targets -- -D warnings` passes (if repo is clean).
- [ ] No smart contracts.
- [ ] No pool logic.
- [ ] Miner remains external.
