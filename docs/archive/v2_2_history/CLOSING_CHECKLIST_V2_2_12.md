# Closing Checklist v2.2.12 — Full Private-Testnet Rehearsal and Hardening

Use this checklist to close v2.2.12 only after the rehearsal documentation, operator notes, diagnostics, and evidence bundle are complete. v2.2.12 is a hardening milestone, not the official private-testnet readiness release.

## Version alignment

- [ ] `VERSION` is `v2.2.12`.
- [ ] `Cargo.toml` workspace version is `2.2.12`.
- [ ] `Cargo.lock` is updated for local PulseDAG packages if needed.
- [ ] `/status` reports `version: "v2.2.12"`.
- [ ] `/release` reports `version: "v2.2.12"`.

## Documentation and runbook checks

- [ ] `docs/P2P_REHEARSAL_V2_2_12.md` is complete and matches the selected topology.
- [ ] `docs/SMOKE_TEST_V2_2_12.md` captures the baseline A/B/C flow and longer-running extension.
- [ ] `docs/SYNC_RECOVERY_V2_2_12.md` covers restart/rejoin, sync convergence, missing parents, duplicate storms, and firewall issues.
- [ ] `docs/RELEASE_NOTES_V2_2_12.md` preserves the v2.2.12 scope and guardrails.
- [ ] All markdown filenames and cross-references use the `V2_2_12` suffix consistently.
- [ ] Operator ambiguities discovered during rehearsal are converted into runbook updates or follow-up issues.
- [ ] Referenced v2.2.12 docs and scripts exist before publishing closeout notes.

## Required local checks

- [ ] `cargo fmt --check` passes.
- [ ] `cargo test --workspace` passes.
- [ ] `cargo build --workspace` passes.
- [ ] The v2.2.12 smoke script (`scripts/v2_2_12_smoke_p2p.sh`) passes.
- [ ] `bash -n scripts/v2_2_12_collect_evidence.sh` passes before closeout.

## Baseline three-node evidence

- [ ] Node A, node B, and node C start successfully.
- [ ] A/B/C use the same chain id.
- [ ] A/B/C connect in real `libp2p-real` network mode.
- [ ] Node A mines or accepts a block through the external miner/node interface.
- [ ] Node B receives or syncs the block.
- [ ] Node C receives or syncs the block.
- [ ] Restart node B and verify it catches up to node A height.
- [ ] A/B/C diagnostics are collected at closeout: `/health`, `/status`, `/release`, `/p2p/status`, `/p2p/peers`, `/p2p/propagation`, `/sync/status`, and `/sync/missing` where available.

## Full rehearsal and hardening evidence

- [ ] Rehearsal includes multi-node validation beyond a one-shot local smoke command, or documents why the selected topology is constrained.
- [ ] Rehearsal includes multi-operator execution, review, or role-played handoff notes.
- [ ] Sustained rehearsal evidence is captured from a run long enough to observe repeated mining, propagation, and sync convergence cycles.
- [ ] Restart/rejoin rehearsal passes for B and at least one additional node when topology allows.
- [ ] Sync convergence is measured after mining, restart, temporary lag, and peer churn.
- [ ] Duplicate suppression is reviewed and does not cause repeated acceptance or relay storms.
  - Expected evidence: `cargo test -p pulsedag-p2p v2_2_12_duplicate_blockdata_is_delivered_once_and_counted` shows duplicate `BlockData` produces one inbound block event, `inbound_messages=1`, and `inbound_duplicates_suppressed=1`.
  - Expected evidence: `cargo test -p pulsedag-p2p repeated_block_relay_storm_is_deduped_without_counter_inflation` shows repeated outbound block relay attempts publish once and increment `block_outbound_duplicates_suppressed` for duplicates.
  - Expected evidence: `/p2p/status.duplicate_suppression_counters` and `block_propagation_counters` from the live rehearsal do not increase accepted/published counts once per duplicate.
- [ ] Invalid peer block rejection and chain-id mismatch dropping are verified through tests, counters, logs, or targeted rehearsal evidence.
  - Expected evidence: `cargo test -p pulsedag-p2p v2_2_12_block_chain_id_mismatches_are_dropped_and_counted` shows wrong-chain block, announce, and block-data messages produce no flow events and increment `inbound_chain_mismatch_dropped`.
  - Expected evidence: `cargo test -p pulsedag-core rejects_block_with_invalid_pow` and `cargo test -p pulsedag-core invalid_transaction_in_peer_block_returns_invalid_transaction_outcome` show invalid peer blocks are rejected without weakening PoW or consensus validation.
  - Expected evidence: `/sync/status.chain_id_mismatch_drops` and `/p2p/status.inbound_chain_mismatch_dropped` are captured when a mismatch rehearsal is run, with `last_drop_reason` identifying the message family.
  - Expected evidence: `cargo test -p pulsedag-rpc diagnostics_surfaces_last_rejected_peer_block_reason` shows `/diagnostics.last_rejected_peer_block_reason` carries the last peer block rejection reason when runtime has one.
- [ ] Missing parent and orphan diagnostics are reviewed and explained.
- [ ] Peer scoring, cooldown, reconnect, and backoff diagnostics remain useful to operators.
- [ ] Run `scripts/v2_2_12_collect_evidence.sh` against the live rehearsal before stopping nodes.
- [ ] Final evidence includes the collector output for `/health`, `/status`, `/p2p/status`, `/p2p/peers`, `/p2p/propagation`, `/sync/status`, `/sync/missing`, and `/p2p/topics` from participating nodes where available.
- [ ] Logs from all nodes and the external miner are archived in the collector tarball.
- [ ] The collector tarball path and checksum are recorded in closeout notes.
- [ ] Failures, recoveries, unresolved risks, and runbook changes are summarized.

## Release guardrails

- [ ] No smart contracts are introduced or claimed for v2.2.12.
- [ ] No pool logic is introduced or claimed for v2.2.12.
- [ ] `pulsedag-miner` remains external and node-facing through documented RPC/mining interfaces.
- [ ] Documentation does not claim official private-testnet readiness.
- [ ] v2.3.0 remains the readiness decision milestone.
- [ ] Rehearsal evidence is labeled as v2.2.12 hardening evidence, not production readiness evidence.

## Closeout note

When this checklist is complete, v2.2.12 may be closed as the full private-testnet rehearsal and hardening milestone. The readiness decision, including whether the accumulated evidence is sufficient for official private-testnet readiness, remains deferred to v2.3.0.
