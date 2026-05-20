# Closing Checklist v2.2.11 — P2P Completion

Use this checklist to close v2.2.11 only after all earlier v2.2.11 PRs have merged. Checkboxes should be completed with command output, logs, or captured endpoint responses before tagging or publishing release evidence.

## Version alignment

- [ ] `VERSION` is `v2.2.11`.
- [ ] `Cargo.toml` workspace version is `2.2.11`.
- [ ] `Cargo.lock` is updated for local PulseDAG packages if needed.
- [ ] `GET /status` reports `version: "v2.2.11"`.
- [ ] `GET /release` reports `version: "v2.2.11"`.

## Required local checks

- [ ] `cargo fmt --check` passes.
- [ ] `cargo test --workspace` passes.
- [ ] `cargo build --workspace` passes.
- [ ] `scripts/v2_2_11_smoke_p2p.sh` or equivalent three-node smoke script passes.

## Three-node P2P smoke evidence

- [ ] Node A, Node B, and Node C start successfully.
- [ ] Node A/B/C connect in real network mode.
- [ ] Node A mines or accepts a block through the external miner/node interface.
- [ ] Node B receives or syncs the block.
- [ ] Node C receives or syncs the block.
- [ ] Restart Node B and verify it catches up to Node A height.
- [ ] Duplicate blocks are suppressed and do not cause repeated acceptance or relay storms.
- [ ] Invalid peer block is rejected.
- [ ] `chain_id` mismatch is dropped.
- [ ] `GET /p2p/status` reports real network mode, expected peer count, useful peer lifecycle/backoff state, duplicate/drop counters, and message counters.
- [ ] `GET /sync/status` reports useful sync phase, selected peer/catch-up state, counters, and any last error.
- [ ] Collect `GET /health`, `GET /status`, `GET /p2p/status`, and `GET /sync/status` from nodes A/B/C.

## Release guardrails

- [ ] No smart contracts are introduced or claimed for v2.2.11.
- [ ] No pool logic is introduced or claimed for v2.2.11.
- [ ] Miner remains external and node-facing through documented RPC/mining interfaces.
- [ ] Documentation does not claim official private-testnet readiness.
- [ ] v2.2.12 is identified as full private-testnet rehearsal and hardening.
- [ ] v2.3.0 remains the readiness decision milestone.

## Closeout note

When this checklist is complete, v2.2.11 may be closed as the P2P completion release. Remaining private-testnet rehearsal, hardening, sustained multi-operator validation, and readiness evidence move to v2.2.12 and v2.3.0 as documented in the version matrix.
