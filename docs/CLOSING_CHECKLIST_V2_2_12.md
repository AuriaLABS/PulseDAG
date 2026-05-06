# Closing Checklist v2.2.12 — Full Private-Testnet Rehearsal and Hardening

Use this checklist to close v2.2.12 only after the rehearsal documentation, operator notes, diagnostics, and evidence bundle are complete. v2.2.12 is a hardening milestone, not the official private-testnet readiness release.

## Version alignment

- [ ] `VERSION` is `v2.2.12` when the release is cut.
- [ ] `Cargo.toml` workspace version is `2.2.12` when the release is cut.
- [ ] `Cargo.lock` is updated for local PulseDAG packages if needed.
- [ ] `GET /status` reports `version: "v2.2.12"` for release binaries.
- [ ] `GET /release` reports `version: "v2.2.12"` where available.

## Documentation and runbook checks

- [ ] `docs/P2P_REHEARSAL_V2_2_12.md` is complete and matches the selected topology.
- [ ] `docs/SMOKE_TEST_V2_2_12.md` captures the baseline A/B/C flow and longer-running extension.
- [ ] `docs/SYNC_RECOVERY_V2_2_12.md` covers restart/rejoin, sync convergence, missing parents, duplicate storms, and firewall issues.
- [ ] `docs/RELEASE_NOTES_V2_2_12.md` preserves the v2.2.12 scope and guardrails.
- [ ] All markdown filenames and cross-references use the `V2_2_12` suffix consistently.
- [ ] Operator ambiguities discovered during rehearsal are converted into runbook updates or follow-up issues.

## Required local checks

- [ ] `cargo fmt --check` passes when code changes are included.
- [ ] `cargo test --workspace` passes when code changes are included.
- [ ] `cargo build --workspace --release` passes for rehearsal binaries.
- [ ] The v2.2.11 baseline smoke script or an equivalent v2.2.12 rehearsal path passes.
- [ ] `bash -n scripts/v2_2_12_collect_evidence.sh` passes before closeout.

## Baseline three-node evidence

- [ ] Node A, node B, and node C start successfully.
- [ ] A/B/C use the same chain id.
- [ ] A/B/C connect in real `libp2p-real` network mode.
- [ ] Node A mines or accepts a block through the external miner/node interface.
- [ ] Node B receives or syncs the block.
- [ ] Node C receives or syncs the block.
- [ ] Restart node B and verify it catches up to node A height.
- [ ] Collect `/health`, `/status`, `/p2p/status`, and `/sync/status` from A/B/C at closeout.

## Full rehearsal and hardening evidence

- [ ] Rehearsal includes multi-node validation beyond a one-shot local smoke command, or documents why the selected topology is constrained.
- [ ] Rehearsal includes multi-operator execution, review, or role-played handoff notes.
- [ ] Rehearsal runs long enough to observe repeated mining, propagation, and sync convergence cycles.
- [ ] Restart/rejoin behavior is tested for B and at least one additional node when topology allows.
- [ ] Sync convergence is measured after mining, restart, temporary lag, and peer churn.
- [ ] Duplicate suppression is reviewed and does not cause repeated acceptance or relay storms.
- [ ] Invalid peer block rejection and chain-id mismatch dropping are verified through tests, counters, logs, or targeted rehearsal evidence.
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
