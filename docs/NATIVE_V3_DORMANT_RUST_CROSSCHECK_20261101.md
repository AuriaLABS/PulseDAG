# Dormant native-v3 GHOSTDAG Rust cross-check — non-normative integration

**Status: candidate research, NOT an approved GHOSTDAG policy, NOT native monetary consensus.** This bounded increment advances [issue #1309](https://github.com/AuriaLABS/PulseDAG/issues/1309) from the pure Python symbolic baseline merged as #1357 to actual independently verified `BlockHeaderV3` SHA256 identities, canonical KHeavyHash PoW and trial K selection in Rust.

## Exact source split from #1341

Only the following dormant, source-identical elements from [experimental PR #1341](https://github.com/AuriaLABS/PulseDAG/pull/1341) are carried into current `main`: `native_ghostdag_policy_candidate_v3.rs`, the independent Rust topology/PoW probe, the frozen Python↔Rust full-header vectors and corresponding generator/fixture. `lib.rs` exports the uncalled module. A dedicated exact-SHA CI workflow tests these elements on current `main` and checks that **production apps/core do not import the module**.

- **Positive tests:** full chain-bound, native nanosecond header serialization, independent SHA256/KHeavyHash digests, exact compact target comparison; authenticated G/A/B/X0/X1 sibling/join closure; trial K=0/K=1 selected parent, red/blue membership, inclusive blue score and 512-bit checked target-weighted work; Rust structural side-DAG validation.
- **Negative tests:** a faulty KHeavyHash on an unselected side branch is rejected by the independent DAG verifier; invalid compact target / cumulative work overflow are rejected by the dormant candidate math.
- **Conformance boundary:** the Rust candidate rejects missing/invalid ancestry, noncanonical parents and incorrectly committed blue score. The input must fit a **128-header test-only diagnostic budget**, not an approved production parameter; no pruning proof, authenticated production genesis or full economic ordering is claimed.

This work deliberately excludes `state_replay_v3.rs` modifications, in-place UTXO optimization, native monetary reorg tests, storage write paths, v3 startup activation, P2P admission, mining templates and contract compilation/execution. The broader #1341 remains in DRAFT until it is split and retested by risk tier on current `main`.

## Required policy and production gates still OPEN

The formula `floor(2^256/(target+1))`, a u512 accumulator, inclusive own-work and `K=32` / max 8 eligible parents are **proposals**, not frozen protocol values; cross-language K=0/1 vectors are only test fixtures. A protocol-owner rule decision with network-bound genesis IDs and exact public chain versions, measured 25-node/16-miner 500ms propagation/DoS/security, realistic target/retarget changes, pruning proofs and independent exact-byte goldens are required. Deterministic DAG transaction order, signed transfer/fee/reward/UTXO replay, atomic RocksDB recovery and full native P2P/mined validation remain separate projects.

Do not import this candidate module into the production consensus, convert diagnostic policy into authority, use it to enable monetary mining or claim a public testnet/mainnet GO. `monetary_v3` mode stays fail-closed; smart contracts remain fully INACTIVE in v3.0 even while their offline core is completed per #1333.

## Reproduction on exact source SHA

```bash
python3 scripts/native_v3_crosslang_ghostdag_vectors.py --verify docs/fixtures/native_v3_ghostdag_fork_crosslang.json
cargo fmt --all -- --check
cargo test --locked -p pulsedag-core --lib native_ghostdag_policy_candidate_v3::tests::
cargo test --locked -p pulsedag-core --test native_v3_ghostdag_policy_bridge
cargo test --locked -p pulsedag-core --test native_v3_ghostdag_crosslang_vectors
cargo clippy --locked -p pulsedag-core --lib -- -D warnings
```
