# PulseDAG v2.2.7 Closing Checklist

Use this checklist before tagging or treating v2.2.7 as closed.

## Version and package alignment

- [ ] `Cargo.toml` workspace package version is `2.2.7`.
- [ ] Workspace members use `version.workspace = true` or are explicitly aligned to `2.2.7`.
- [ ] No repository documentation still describes the current release as `2.2.6`.
- [ ] No v2.2.7 document claims that v2.3.0 private-testnet scope is already complete.

## Documentation alignment

- [ ] `docs/ROADMAP_V2_2_7.md` describes v2.2.7 as the clean foundation closure.
- [ ] `docs/RELEASE_NOTES_V2_2_7.md` exists and lists known limitations.
- [ ] `docs/SMOKE_TEST_V2_2_7.md` exists and is marked manual/partial.
- [ ] `docs/ROADMAP_V2_3_0.md` remains the target for complete private-testnet readiness.
- [ ] `docs/VERSION_MATRIX.md` reflects the v2.2.7 -> v2.2.8 -> v2.3.0 sequence.

## Code validation

Run locally or in CI:

```bash
cargo fmt --check
cargo test --workspace
cargo build --workspace
```

Optional if the repository is clippy-clean in the current environment:

```bash
cargo clippy --workspace --all-targets -- -D warnings
```

## Manual smoke path

- [ ] Start one local node.
- [ ] Request a mining template.
- [ ] Mine or simulate a valid nonce with the external miner flow or local test utility.
- [ ] Submit the candidate through the mining submit flow.
- [ ] Confirm valid PoW is accepted.
- [ ] Confirm invalid PoW is rejected.
- [ ] Confirm duplicate submission is not accepted as new work.
- [ ] Optionally start a second node and verify basic peer connectivity.

## Scope guardrails

- [ ] No smart contracts were added.
- [ ] No smart-contract runtime was enabled.
- [ ] No pool coordination logic was added inside `pulsedag-miner`.
- [ ] Miner remains an external standalone application.
- [ ] v2.2.7 does not claim production readiness.
- [ ] v2.2.7 does not claim full private-testnet completion.

## Handoff to v2.3.0

The following remain out of v2.2.7 and belong to v2.3.0 or later hardening:

- Full multi-node private testnet.
- Complete P2P propagation and sync/recovery hardening.
- Active multi-node PoW operation over a real private topology.
- Operator dashboards, runbooks, and burn-in evidence.
- Snapshot/prune/restore drills where still incomplete.
